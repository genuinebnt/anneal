//! Verifier negative tests and VM fuzzing: corrupted MIR and bytecode must be
//! rejected (never panic), and arbitrary bytecode must never crash the VM and
//! must always terminate under the fuel limit.

mod common;
use common::*;
use pebble::bc::{self, text};
use pebble::interp::RunOptions;
use pebble::mir::{self, *};
use pebble::sema::Ty;

const SRC: &str = "struct P { a: int, b: bool }\nenum E { A, B(int) }\n\
fn add(x: int, y: int) -> int { x + y }\n\
fn main() {\n  let p = P { a: 1, b: true };\n  let xs = [1, 2, 3];\n  let mut i = 0;\n  while i < len(xs) { println(to_str(add(xs[i], p.a))); i += 1; }\n\
  match E::B(4) { E::A => println(\"a\"), E::B(n) => println(to_str(n)) }\n}\n";

fn good_mir() -> MirProgram {
    let an = analyze_ok(SRC);
    let m = mir::lower::lower(&an.hir).unwrap();
    mir::verify::verify(&m).expect("pristine MIR verifies");
    m
}

fn rejects(name: &str, mutate: impl FnOnce(&mut MirProgram)) {
    let mut m = good_mir();
    mutate(&mut m);
    let errs = mir::verify::verify(&m).expect_err(&format!("{name}: corrupted MIR was accepted"));
    assert!(!errs.is_empty(), "{name}");
}

fn main_fn(m: &mut MirProgram) -> &mut Func {
    let id = m.main as usize;
    &mut m.funcs[id]
}

#[test]
fn corrupted_mir_is_rejected() {
    big_stack(|| {
        rejects("jump out of range", |m| {
            main_fn(m).blocks[0].term = Terminator::Goto(999)
        });
        rejects("branch on int", |m| {
            main_fn(m).blocks[0].term = Terminator::Branch {
                cond: Operand::int(1),
                then_bb: 0,
                else_bb: 0,
            }
        });
        rejects("type mismatch", |m| {
            let f = main_fn(m);
            let l = f.new_local(Ty::Int);
            f.blocks[0].stmts.insert(
                0,
                Stmt::new(
                    StmtKind::Assign(l, Rvalue::Use(Operand::Const(Const::Bool(true)))),
                    0,
                ),
            );
        });
        rejects("use of uninitialised local", |m| {
            let f = main_fn(m);
            let a = f.new_local(Ty::Int);
            let b = f.new_local(Ty::Int);
            f.blocks[0].stmts.insert(
                0,
                Stmt::new(StmtKind::Assign(b, Rvalue::Use(Operand::Local(a))), 0),
            );
        });
        rejects("local out of range", |m| {
            let f = main_fn(m);
            f.blocks[0].stmts.insert(
                0,
                Stmt::new(StmtKind::Assign(0, Rvalue::Use(Operand::Local(9999))), 0),
            );
        });
        rejects("wrong return type", |m| {
            let f = &mut m.funcs[1];
            for b in &mut f.blocks {
                if let Terminator::Return(_) = b.term {
                    b.term = Terminator::Return(Operand::Const(Const::Bool(false)));
                }
            }
        });
        rejects("call arity", |m| {
            for b in &mut main_fn(m).blocks {
                if let Terminator::Call { args, .. } = &mut b.term {
                    args.pop();
                }
            }
        });
        rejects("call of missing function", |m| {
            for b in &mut main_fn(m).blocks {
                if let Terminator::Call { callee, .. } = &mut b.term {
                    *callee = Callee::Direct(77);
                }
            }
        });
        rejects("no blocks", |m| main_fn(m).blocks.clear());
        rejects("non-monomorphic local", |m| {
            let f = main_fn(m);
            f.locals[0].ty = Ty::Param("T".into());
        });
        rejects("aggregate arity", |m| {
            for b in &mut main_fn(m).blocks {
                for s in &mut b.stmts {
                    if let StmtKind::Assign(_, Rvalue::Agg(_, ops)) = &mut s.kind {
                        ops.push(Operand::int(1));
                    }
                }
            }
        });
        rejects("main out of range", |m| m.main = 500);
        rejects("scope out of range", |m| {
            let f = main_fn(m);
            f.blocks[0].stmts.insert(
                0,
                Stmt::new(StmtKind::Assign(0, Rvalue::Use(Operand::int(1))), 77),
            );
        });
        rejects("switch value is a bool", |m| {
            main_fn(m).blocks[0].term = Terminator::Switch {
                value: Operand::Const(Const::Bool(true)),
                cases: vec![],
                default: 0,
            };
        });
    });
}

fn good_chunk() -> bc::Chunk {
    let an = analyze_ok(SRC);
    let c = pebble::driver::compile(
        &an.hir,
        &pebble::opt::OptOptions {
            level: 2,
            dump_after: vec![],
            verify: true,
        },
        &mut |_, _| {},
    )
    .unwrap();
    c.chunk
}

fn bc_rejects(name: &str, mutate: impl FnOnce(&mut bc::Chunk)) {
    let mut c = good_chunk();
    mutate(&mut c);
    assert!(
        bc::verify::verify(&c).is_err(),
        "{name}: corrupted bytecode was accepted"
    );
}

#[test]
fn corrupted_bytecode_is_rejected() {
    use bc::*;
    big_stack(|| {
        bc_rejects("register out of range", |c| {
            c.funcs[0].code.insert(
                0,
                Instr::Move {
                    d: Reg(60000),
                    s: Reg(0),
                },
            )
        });
        bc_rejects("jump out of range", |c| {
            let f = &mut c.funcs[0];
            f.code.insert(0, Instr::Jmp { t: Tgt(9999) });
            f.pc_scope.insert(0, 0);
        });
        bc_rejects("constant out of range", |c| {
            c.funcs[0].code.insert(
                0,
                Instr::LoadK {
                    d: Reg(0),
                    k: Kst(9999),
                },
            )
        });
        bc_rejects("function out of range", |c| {
            c.funcs[0].code.insert(
                0,
                Instr::LoadFn {
                    d: Reg(0),
                    f: Fid(99),
                },
            )
        });
        bc_rejects("list out of range", |c| {
            c.funcs[0].code.insert(
                0,
                Instr::MkArray {
                    d: Reg(0),
                    list: Lst(99),
                },
            )
        });
        bc_rejects("switch out of range", |c| {
            c.funcs[0].code.insert(
                0,
                Instr::Switch {
                    r: Reg(0),
                    s: Swt(99),
                },
            )
        });
        bc_rejects("falls off the end", |c| {
            let f = &mut c.funcs[0];
            f.code.push(Instr::LoadUnit { d: Reg(0) });
            f.pc_scope.push(0);
        });
        bc_rejects("call arity", |c| {
            let f = &mut c.funcs[0];
            f.lists.push(vec![].into_boxed_slice());
            let l = Lst((f.lists.len() - 1) as u32);
            f.code.insert(
                0,
                Instr::Call {
                    d: Reg(0),
                    f: Fid(c.main),
                    args: l,
                },
            );
        });
        bc_rejects("empty function", |c| {
            c.funcs[0].code.clear();
            c.funcs[0].pc_scope.clear();
        });
        bc_rejects("bad main", |c| c.main = 42);
        bc_rejects("pc_scope length", |c| {
            c.funcs[0].pc_scope.pop();
        });
        bc_rejects("list names a register beyond the frame", |c| {
            c.funcs[0].lists.push(vec![65000].into_boxed_slice())
        });
    });
}

#[test]
fn assembler_rejects_garbage_text() {
    for bad in [
        "",
        "func",
        "func f0 \"x\" regs=1 params=0 const=false\n  0: bogus r0\nend",
        "func f0 \"x\" regs=1 params=0 const=false\n  0: move r0\nend",
        "const k0 = float 1.0",
        "main f",
        "func f0 \"x\" regs=1 params=0 const=false\n  0: loadi r0, 1",
        "\u{0}\u{1}\u{2}",
        "func f5 \"x\" regs=1 params=0 const=false\nend",
    ] {
        let r = text::assemble(bad);
        if bad.is_empty() {
            assert!(r.is_ok());
        } else {
            assert!(
                r.is_err() || bc::verify::verify(&r.unwrap()).is_err(),
                "accepted: {bad:?}"
            );
        }
    }
}

#[test]
fn disassembly_round_trips_through_the_assembler() {
    big_stack(|| {
        for f in files_with_ext(&manifest_dir().join("tests/run"), "peb")
            .into_iter()
            .chain(files_with_ext(&manifest_dir().join("examples"), "peb"))
        {
            let src = std::fs::read_to_string(&f).unwrap();
            let an = analyze_ok(&src);
            for level in [0, 3] {
                let c = pebble::driver::compile(
                    &an.hir,
                    &pebble::opt::OptOptions {
                        level,
                        dump_after: vec![],
                        verify: true,
                    },
                    &mut |_, _| {},
                )
                .unwrap();
                let t1 = text::disassemble(&c.chunk);
                let back =
                    text::assemble(&t1).unwrap_or_else(|e| panic!("{}: {e}\n{t1}", f.display()));
                assert_eq!(
                    text::disassemble(&back),
                    t1,
                    "{}: text does not round-trip",
                    f.display()
                );
                bc::verify::verify(&back).unwrap_or_else(|e| {
                    panic!("{}: reassembled chunk invalid: {e:?}", f.display())
                });
                // And the reassembled program behaves identically.
                let ro = RunOptions {
                    max_depth: 10_000,
                    fuel: 5_000_000,
                };
                let (mut a, mut b) = (Vec::new(), Vec::new());
                let ra = pebble::vm::run(&c.chunk, &mut a, &ro)
                    .0
                    .err()
                    .map(|e| e.to_string());
                let rb = pebble::vm::run(&back, &mut b, &ro)
                    .0
                    .err()
                    .map(|e| e.to_string());
                assert_eq!((a, ra), (b, rb), "{}", f.display());
            }
        }
    });
}

// ------------------------------------------------------------------- fuzzing

fn run_bounded(c: &bc::Chunk) -> bool {
    let ro = RunOptions {
        max_depth: 40,
        fuel: 4_000,
    };
    let mut out = Vec::new();
    let _ = pebble::vm::run(c, &mut out, &ro);
    true
}

#[test]
fn mutated_listings_never_panic_the_verifier_or_vm() {
    big_stack(|| {
        let seeds: Vec<String> = [
            "fib",
            "sieve",
            "guards",
            "higher_order",
            "struct_variants",
            "generic_list",
        ]
        .iter()
        .flat_map(|n| {
            let src =
                std::fs::read_to_string(manifest_dir().join(format!("examples/{n}.peb"))).unwrap();
            let an = analyze_ok(&src);
            [0u8, 3].map(|level| {
                let c = pebble::driver::compile(
                    &an.hir,
                    &pebble::opt::OptOptions {
                        level,
                        dump_after: vec![],
                        verify: false,
                    },
                    &mut |_, _| {},
                )
                .unwrap();
                text::disassemble(&c.chunk)
            })
        })
        .collect();
        let mut rng = Rng::new(2024);
        let (mut assembled, mut verified) = (0, 0);
        for round in 0..1500 {
            let base = &seeds[round % seeds.len()];
            let mut lines: Vec<String> = base.lines().map(str::to_string).collect();
            for _ in 0..1 + rng.below(4) {
                let i = rng.below(lines.len());
                match rng.below(5) {
                    0 => {
                        lines.remove(i);
                    }
                    1 => {
                        let l = lines[i].clone();
                        lines.insert(i, l);
                    }
                    2 => {
                        let j = rng.below(lines.len());
                        lines.swap(i, j);
                    }
                    3 => {
                        // Replace one number by a random one.
                        let l = &lines[i];
                        if let Some(p) = l.find(|c: char| c.is_ascii_digit()) {
                            let end = l[p..]
                                .find(|c: char| !c.is_ascii_digit())
                                .map_or(l.len(), |e| p + e);
                            lines[i] = format!("{}{}{}", &l[..p], rng.below(40), &l[end..]);
                        }
                    }
                    _ => {
                        let l = &lines[i];
                        let m = [
                            "move", "add", "jmp", "call", "ret", "index", "setfield", "switch",
                            "getfield", "div",
                        ][rng.below(10)];
                        if let Some((_, rest)) = l.split_once(": ") {
                            lines[i] = format!(
                                "  {}: {m} {}",
                                i,
                                rest.split_once(' ').map_or("", |x| x.1)
                            );
                        }
                    }
                }
            }
            let Ok(c) = text::assemble(&lines.join("\n")) else {
                continue;
            };
            assembled += 1;
            if bc::verify::verify(&c).is_ok() {
                verified += 1;
                run_bounded(&c);
            }
        }
        assert!(assembled > 200, "only {assembled} mutants assembled");
        assert!(verified > 20, "only {verified} mutants verified");
    });
}

/// (mnemonic, operand kinds): r = register, k = constant, i = small int, t = jump target, f = function, l = list.
const TEMPLATES: &[(&str, &str)] = &[
    ("move", "rr"),
    ("loadk", "rk"),
    ("loadi", "ri"),
    ("loadb", "rb"),
    ("loadunit", "r"),
    ("loadfn", "rf"),
    ("add", "rrr"),
    ("sub", "rrr"),
    ("mul", "rrr"),
    ("div", "rrr"),
    ("rem", "rrr"),
    ("addi", "rri"),
    ("subi", "rri"),
    ("muli", "rri"),
    ("neg", "rr"),
    ("eqi", "rrr"),
    ("nei", "rrr"),
    ("lti", "rrr"),
    ("lei", "rrr"),
    ("gti", "rrr"),
    ("gei", "rrr"),
    ("lts", "rrr"),
    ("eq", "rrr"),
    ("ne", "rrr"),
    ("not", "rr"),
    ("cast", "rr"),
    ("concat", "rrr"),
    ("mkagg", "ril"),
    ("mkarray", "rl"),
    ("getfield", "rri"),
    ("setfield", "rir"),
    ("tag", "rr"),
    ("index", "rrr"),
    ("indexu", "rrr"),
    ("setindex", "rrr"),
    ("setindexu", "rrr"),
    ("len", "rr"),
    ("push", "rr"),
    ("pop", "rr"),
    ("chars", "rr"),
    ("tostr", "rr"),
    ("print", "r"),
    ("println", "r"),
    ("println0", ""),
    ("assert", "r"),
    ("assertmsg", "rr"),
    ("panic", "r"),
    ("jmp", "t"),
    ("jmpif", "rt"),
    ("jmpifnot", "rt"),
    ("jmplt", "rrt"),
    ("jmpge", "rrt"),
    ("jmplti", "rit"),
    ("jmpgei", "rit"),
    ("jmpgti", "rit"),
    ("jmplei", "rit"),
    ("call", "rfl"),
    ("callind", "rrl"),
    ("ret", "r"),
];

fn random_chunk_text(rng: &mut Rng) -> String {
    let nregs = 1 + rng.below(8);
    let nfuncs = 1 + rng.below(3);
    let mut s = String::from(
        "main f0\nconst k0 = int 7\nconst k1 = str \"zz\"\nconst k2 = int 9000000000\n",
    );
    for fi in 0..nfuncs {
        let n = 3 + rng.below(25);
        let nparams = if fi == 0 { 0 } else { rng.below(2) };
        s.push_str(&format!("func f{fi} \"fn{fi}\" regs={nregs} params={nparams} const=false\n  scope 0 \"fn{fi}\" -\n"));
        let reg = |rng: &mut Rng| format!("r{}", rng.below(nregs));
        for pc in 0..n {
            let (mn, kinds) = TEMPLATES[rng.below(TEMPLATES.len())];
            let ops: Vec<String> = kinds
                .chars()
                .map(|k| match k {
                    'r' => reg(rng),
                    'k' => format!("k{}", rng.below(3)),
                    'i' => format!("{}", rng.below(6) as i64 - 1),
                    'b' => ["true", "false"][rng.below(2)].to_string(),
                    't' => format!("@{}", rng.below(n + 1)),
                    'f' => format!("f{}", rng.below(nfuncs)),
                    _ => format!(
                        "[{}]",
                        (0..rng.below(3))
                            .map(|_| reg(rng))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                })
                .collect();
            s.push_str(&format!("  {pc}: {mn} {}\n", ops.join(", ")));
        }
        s.push_str(&format!("  {n}: ret r0\nend\n"));
    }
    s
}

#[test]
fn random_bytecode_never_panics_and_terminates() {
    big_stack(|| {
        let mut rng = Rng::new(99);
        let (mut verified, mut ok_runs) = (0, 0);
        for _ in 0..3000 {
            let t = random_chunk_text(&mut rng);
            let Ok(c) = text::assemble(&t) else { continue };
            if bc::verify::verify(&c).is_err() {
                continue;
            }
            verified += 1;
            let ro = RunOptions {
                max_depth: 40,
                fuel: 4_000,
            };
            let mut out = Vec::new();
            if pebble::vm::run(&c, &mut out, &ro).0.is_ok() {
                ok_runs += 1;
            }
        }
        assert!(verified > 500, "only {verified} random chunks verified");
        assert!(
            ok_runs > 5,
            "no random program ran to completion ({ok_runs})"
        );
    });
}

#[test]
fn garbage_text_never_panics_the_assembler() {
    let mut rng = Rng::new(5);
    let alphabet: Vec<char> = "abcdefr0123456789 ,:@[]{}_\"\\\n;-=funcscopeconstmain"
        .chars()
        .collect();
    for _ in 0..3000 {
        let n = rng.below(120);
        let s: String = (0..n).map(|_| *rng.pick(&alphabet)).collect();
        if let Ok(c) = text::assemble(&s) {
            let _ = bc::verify::verify(&c);
        }
    }
}

#[test]
fn unverifiable_mir_never_reaches_codegen_in_the_driver() {
    // `compile` with verification enabled returns Err instead of panicking when a
    // pass is asked for by a wrong name.
    big_stack(|| {
        let an = analyze_ok(SRC);
        assert!(pebble::driver::compile_passes(&an.hir, &["no-such-pass"]).is_err());
    });
}
