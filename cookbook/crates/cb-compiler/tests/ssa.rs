//! SSA construction and destruction, SCCP, GVN and LICM.
//!
//! * Property tests on RANDOM CONTROL-FLOW GRAPHS (including irreducible ones that
//!   no PEBBLE source can produce): after `ssa` every use is dominated by its single
//!   definition (checked against a brute-force dominator oracle, not the code under
//!   test), and a tiny MIR evaluator with real phi semantics shows that every SSA
//!   pass, before and after leaving SSA, computes what the original computed,
//!   including whether and how it traps.
//! * Parallel-copy sequentialisation against a simultaneous-assignment oracle.
//! * The lost-copy and swap problems, from source.
//! * Negative tests of the SSA rules of the verifier.

mod common;
use common::*;
use pebble::interp::RunOptions;
use pebble::mir::{self, *};
use pebble::opt::{self, OptOptions};
use pebble::sema::Ty;
use pebble::syntax::ast::BinOp;
use std::collections::BTreeMap;

// ---------------------------------------------------------------- random CFGs

const VARS: u32 = 5;
const COND: u32 = VARS; // the one bool local

fn int(v: i64) -> Operand {
    Operand::Const(Const::Int(v))
}

fn assign(d: u32, rv: Rvalue) -> Stmt {
    Stmt::new(StmtKind::Assign(d, rv), 0)
}

fn random_operand(rng: &mut Rng) -> Operand {
    if rng.below(4) == 0 {
        int(rng.below(5) as i64 - 1)
    } else {
        Operand::Local(rng.below(VARS as usize) as u32)
    }
}

fn random_stmt(rng: &mut Rng) -> Stmt {
    let d = rng.below(VARS as usize) as u32;
    let (a, b) = (random_operand(rng), random_operand(rng));
    let rv = match rng.below(9) {
        0 | 1 => Rvalue::Use(a),
        2 | 3 => Rvalue::Binary(BinOp::Add, a, b),
        4 => Rvalue::Binary(BinOp::Sub, a, b),
        5 => Rvalue::Binary(BinOp::Rem, a, int(97)),
        6 => Rvalue::Binary(BinOp::Div, a, b), // may trap: must not be speculated
        7 => Rvalue::Binary(BinOp::Mul, a, b), // may overflow
        _ => Rvalue::Use(int(rng.below(10) as i64)),
    };
    assign(d, rv)
}

fn random_func(seed: u64) -> MirProgram {
    let mut rng = Rng::new(seed);
    let n = 2 + rng.below(7);
    let mut blocks = Vec::new();
    for b in 0..n {
        let mut stmts: Vec<Stmt> = Vec::new();
        if b == 0 {
            for v in 0..VARS {
                stmts.push(assign(v, Rvalue::Use(int(rng.below(7) as i64))));
            }
            stmts.push(assign(COND, Rvalue::Use(Operand::Const(Const::Bool(true)))));
        }
        for _ in 0..rng.below(5) {
            stmts.push(random_stmt(&mut rng));
        }
        let target = |rng: &mut Rng| rng.below(n) as BlockId;
        let term = if b + 1 == n {
            // Fold every variable into the result so that all of them are observable.
            for v in 1..VARS {
                stmts.push(assign(
                    0,
                    Rvalue::Binary(BinOp::Rem, Operand::Local(0), int(1009)),
                ));
                stmts.push(assign(
                    0,
                    Rvalue::Binary(BinOp::Mul, Operand::Local(0), int(31)),
                ));
                stmts.push(assign(
                    v,
                    Rvalue::Binary(BinOp::Rem, Operand::Local(v), int(1009)),
                ));
                stmts.push(assign(
                    0,
                    Rvalue::Binary(BinOp::Add, Operand::Local(0), Operand::Local(v)),
                ));
            }
            Terminator::Return(Operand::Local(0))
        } else if rng.below(3) == 0 {
            // Mostly forward, so that most programs reach the return block.
            Terminator::Goto((b + 1 + rng.below(n - b - 1)) as BlockId)
        } else {
            let (x, y) = (random_operand(&mut rng), random_operand(&mut rng));
            let op = [BinOp::Lt, BinOp::Eq, BinOp::Ne][rng.below(3)];
            stmts.push(assign(COND, Rvalue::Binary(op, x, y)));
            let fwd = (b + 1 + rng.below(n - b - 1)) as BlockId;
            let any = target(&mut rng);
            let (then_bb, else_bb) = if rng.below(2) == 0 {
                (fwd, any)
            } else {
                (any, fwd)
            };
            Terminator::Branch {
                cond: Operand::Local(COND),
                then_bb,
                else_bb,
            }
        };
        blocks.push(Block::new(stmts, term));
    }
    let mut locals: Vec<LocalDecl> = (0..VARS)
        .map(|_| LocalDecl {
            ty: Ty::Int,
            name: None,
        })
        .collect();
    locals.push(LocalDecl {
        ty: Ty::Bool,
        name: None,
    });
    let f = Func {
        name: "main".into(),
        inst_name: format!("random_{seed}"),
        params: 0,
        locals,
        blocks,
        ret: Ty::Int,
        scopes: vec![Scope {
            name: "main".into(),
            parent: None,
            call_site: Default::default(),
        }],
        is_const: false,
        ssa: false,
    };
    MirProgram {
        funcs: vec![f],
        main: 0,
        types: Default::default(),
        stats: Default::default(),
    }
}

#[derive(Debug, PartialEq)]
enum End {
    Value(i64),
    Trap(&'static str),
    OutOfSteps,
}

/// Evaluate an int/bool function. Phis of a block are read all at once on entry.
fn eval(f: &Func, max_blocks: usize) -> End {
    fn read(env: &BTreeMap<u32, i64>, o: &Operand) -> i64 {
        match o {
            Operand::Local(l) => *env.get(l).unwrap_or_else(|| panic!("read of unset _{l}")),
            Operand::Const(Const::Int(v)) => *v,
            Operand::Const(Const::Bool(b)) => *b as i64,
            c => panic!("unexpected constant {c:?}"),
        }
    }
    let mut env = BTreeMap::new();
    let (mut block, mut from) = (0 as BlockId, None::<BlockId>);
    for _ in 0..max_blocks {
        let blk = &f.blocks[block as usize];
        let incoming: Vec<(u32, i64)> = blk
            .phis
            .iter()
            .map(|p| {
                let pred = from.expect("phi in the entry block");
                let arg = &p
                    .args
                    .iter()
                    .find(|a| a.0 == pred)
                    .unwrap_or_else(|| panic!("no phi argument for bb{pred}"))
                    .1;
                (p.dest, read(&env, arg))
            })
            .collect();
        env.extend(incoming);
        for s in &blk.stmts {
            let StmtKind::Assign(d, rv) = &s.kind else {
                panic!("unexpected statement {s:?}")
            };
            let v = match rv {
                Rvalue::Use(a) => read(&env, a),
                Rvalue::Cast(a) => read(&env, a),
                Rvalue::Unary(op, a) => {
                    let x = read(&env, a);
                    match op {
                        pebble::syntax::ast::UnOp::Not => (x == 0) as i64,
                        pebble::syntax::ast::UnOp::Neg => match x.checked_neg() {
                            Some(v) => v,
                            None => return End::Trap("overflow"),
                        },
                    }
                }
                Rvalue::Binary(op, a, b) => {
                    let (x, y) = (read(&env, a), read(&env, b));
                    let r = match op {
                        BinOp::Add => x.checked_add(y),
                        BinOp::Sub => x.checked_sub(y),
                        BinOp::Mul => x.checked_mul(y),
                        BinOp::Div if y == 0 => return End::Trap("div0"),
                        BinOp::Rem if y == 0 => return End::Trap("div0"),
                        BinOp::Div => x.checked_div(y),
                        BinOp::Rem => x.checked_rem(y),
                        BinOp::Lt => Some((x < y) as i64),
                        BinOp::Le => Some((x <= y) as i64),
                        BinOp::Gt => Some((x > y) as i64),
                        BinOp::Ge => Some((x >= y) as i64),
                        BinOp::Eq => Some((x == y) as i64),
                        BinOp::Ne => Some((x != y) as i64),
                        other => panic!("unexpected operator {other:?}"),
                    };
                    match r {
                        Some(v) => v,
                        None => return End::Trap("overflow"),
                    }
                }
                other => panic!("unexpected rvalue {other:?}"),
            };
            env.insert(*d, v);
        }
        from = Some(block);
        block = match &blk.term {
            Terminator::Goto(t) => *t,
            Terminator::Branch {
                cond,
                then_bb,
                else_bb,
            } => {
                if read(&env, cond) != 0 {
                    *then_bb
                } else {
                    *else_bb
                }
            }
            Terminator::Return(o) => return End::Value(read(&env, o)),
            other => panic!("unexpected terminator {other:?}"),
        };
    }
    End::OutOfSteps
}

/// Dominance by definition: `a` dominates `b` when removing `a` makes `b` unreachable.
fn brute_dominates(f: &Func, a: BlockId, b: BlockId) -> bool {
    if a == b {
        return true;
    }
    let mut seen = vec![false; f.blocks.len()];
    let mut stack = vec![0];
    while let Some(x) = stack.pop() {
        if x == a || seen[x as usize] {
            continue;
        }
        seen[x as usize] = true;
        stack.extend(f.blocks[x as usize].term.successors());
    }
    !seen[b as usize]
}

fn check_ssa_by_brute_force(f: &Func) -> Result<(), String> {
    let reach = f.reachable();
    let mut def: BTreeMap<u32, (BlockId, i64)> = BTreeMap::new();
    for &b in &reach {
        let blk = &f.blocks[b as usize];
        let defs = blk.phis.iter().map(|p| (p.dest, -1)).chain(
            blk.stmts
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.def().map(|d| (d, i as i64))),
        );
        for (d, at) in defs {
            if def.insert(d, (b, at)).is_some() {
                return Err(format!("_{d} defined twice"));
            }
        }
    }
    let ok = |l: u32, b: BlockId, at: i64| {
        def.get(&l).is_some_and(|&(db, di)| {
            if db == b {
                di < at
            } else {
                brute_dominates(f, db, b)
            }
        })
    };
    let preds = f.predecessors();
    for &b in &reach {
        let blk = &f.blocks[b as usize];
        for p in &blk.phis {
            let mut from: Vec<BlockId> = p.args.iter().map(|a| a.0).collect();
            from.sort_unstable();
            let mut want = preds[b as usize].clone();
            want.sort_unstable();
            if from != want {
                return Err(format!(
                    "phi _{} in bb{b} has arguments for {from:?}, predecessors are {want:?}",
                    p.dest
                ));
            }
            for (pred, o) in &p.args {
                if let Operand::Local(l) = o
                    && !ok(*l, *pred, i64::MAX)
                {
                    return Err(format!("phi argument _{l} from bb{pred} is not dominated"));
                }
            }
        }
        let mut bad = None;
        for (i, s) in blk.stmts.iter().enumerate() {
            s.for_each_use(&mut |o| {
                if let Operand::Local(l) = o
                    && !ok(*l, b, i as i64)
                {
                    bad = Some(format!(
                        "use of _{l} in bb{b} statement {i} is not dominated"
                    ));
                }
            });
        }
        blk.term.for_each_use(&mut |o| {
            if let Operand::Local(l) = o
                && !ok(*l, b, i64::MAX)
            {
                bad = Some(format!(
                    "use of _{l} in the terminator of bb{b} is not dominated"
                ));
            }
        });
        if let Some(m) = bad {
            return Err(m);
        }
    }
    Ok(())
}

fn passes(p: &mut MirProgram, list: &[&str]) -> Result<(), String> {
    opt::run_passes(
        p,
        list,
        &OptOptions {
            level: 0,
            dump_after: vec![],
            verify: true,
        },
        &mut |_, _| {},
    )
}

const PIPELINES: &[&[&str]] = &[
    &["ssa"],
    &["ssa", "sccp"],
    &["ssa", "gvn"],
    &["ssa", "licm"],
    &["ssa", "gvn", "licm"],
    &["ssa", "sccp", "gvn", "licm"],
    &["ssa", "licm", "gvn", "sccp"],
];

#[test]
fn ssa_properties_and_behaviour_on_random_cfgs() {
    big_stack(|| {
        let n: u64 = std::env::var("PEBBLE_SEEDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4000);
        let (mut used, mut values, mut traps, mut with_phis, mut loops) =
            (0u64, 0u64, 0u64, 0u64, 0u64);
        for seed in 0..n {
            let original = random_func(seed);
            mir::verify::verify(&original)
                .unwrap_or_else(|e| panic!("seed {seed}: generator made bad MIR: {e:?}"));
            let want = eval(&original.funcs[0], 300);
            let dt = pebble::opt::dom::DomTree::compute(&original.funcs[0]);
            // The dominator tree agrees with the definition.
            for &a in &dt.rpo {
                for &b in &dt.rpo {
                    assert_eq!(
                        dt.dominates(a, b),
                        brute_dominates(&original.funcs[0], a, b),
                        "seed {seed}: dominates({a}, {b})"
                    );
                }
            }
            loops +=
                u64::from(!pebble::opt::dom::natural_loops(&original.funcs[0], &dt).is_empty());
            for pipeline in PIPELINES {
                let mut p = original.clone();
                passes(&mut p, pipeline)
                    .unwrap_or_else(|e| panic!("seed {seed} {pipeline:?}: {e}\n{original:#?}"));
                check_ssa_by_brute_force(&p.funcs[0]).unwrap_or_else(|e| {
                    panic!("seed {seed} {pipeline:?}: {e}\n{:#?}", p.funcs[0].blocks)
                });
                with_phis += u64::from(p.funcs[0].blocks.iter().any(|b| !b.phis.is_empty()));
                let in_ssa = if want == End::OutOfSteps {
                    None
                } else {
                    Some(eval(&p.funcs[0], 3000))
                };
                passes(&mut p, &["out-of-ssa"])
                    .unwrap_or_else(|e| panic!("seed {seed} {pipeline:?} + out-of-ssa: {e}"));
                assert!(p.funcs[0].blocks.iter().all(|b| b.phis.is_empty()) && !p.funcs[0].ssa);
                if let Some(in_ssa) = in_ssa {
                    assert_eq!(
                        in_ssa, want,
                        "seed {seed} {pipeline:?}: SSA form behaves differently\n{original:#?}\n{:#?}",
                        p.funcs[0].blocks
                    );
                    assert_eq!(
                        eval(&p.funcs[0], 3000),
                        want,
                        "seed {seed} {pipeline:?}: wrong after out-of-ssa\n{:#?}\n{:#?}",
                        original.funcs[0].blocks,
                        p.funcs[0].blocks
                    );
                }
            }
            match want {
                End::Value(_) => values += 1,
                End::Trap(_) => traps += 1,
                End::OutOfSteps => continue,
            }
            used += 1;
        }
        println!(
            "{used}/{n} terminating ({values} values, {traps} traps), {loops} with natural loops, {with_phis} SSA forms with phis"
        );
        assert!(
            values * 4 > n && traps * 50 > n && loops * 4 > n && with_phis > n,
            "weak coverage"
        );
    });
}

// ---------------------------------------------------------------- parallel copies

#[test]
fn sequentialised_parallel_copies_match_simultaneous_assignment() {
    let run = |dests: &[u32], sources: &[Operand]| {
        let mut f = random_func(1).funcs.remove(0);
        let copies: Vec<(u32, Operand)> =
            dests.iter().copied().zip(sources.iter().cloned()).collect();
        let seq = opt::ssa::sequentialize(&mut f, copies.clone());
        let initial = |l: u32| 100 + i64::from(l);
        let mut env: BTreeMap<u32, i64> = (0..f.locals.len() as u32)
            .map(|l| (l, initial(l)))
            .collect();
        for (d, o) in &seq {
            let v = match o {
                Operand::Local(l) => env[l],
                Operand::Const(Const::Int(v)) => *v,
                c => panic!("{c:?}"),
            };
            env.insert(*d, v);
        }
        for l in 0..VARS {
            let want = match copies.iter().find(|c| c.0 == l) {
                Some((_, Operand::Local(s))) => initial(*s),
                Some((_, Operand::Const(Const::Int(v)))) => *v,
                _ => initial(l),
            };
            assert_eq!(env[&l], want, "_{l} after {seq:?} for parallel {copies:?}");
        }
        (seq, copies)
    };
    // The swap problem needs exactly one temporary; a three-cycle too; a chain none.
    let l = Operand::Local;
    assert_eq!(run(&[0, 1], &[l(1), l(0)]).0.len(), 3);
    assert_eq!(run(&[0, 1, 2], &[l(1), l(2), l(0)]).0.len(), 4);
    assert_eq!(run(&[0, 1, 2], &[l(1), l(2), l(3)]).0.len(), 3);
    assert_eq!(run(&[0, 1, 2, 3], &[l(1), l(0), l(3), l(2)]).0.len(), 6);
    assert!(run(&[0], &[l(0)]).0.is_empty());
    // Random sets with distinct destinations.
    let mut rng = Rng::new(5);
    for _ in 0..20_000 {
        let mut dests: Vec<u32> = (0..VARS).collect();
        for i in (1..dests.len()).rev() {
            dests.swap(i, rng.below(i + 1));
        }
        dests.truncate(1 + rng.below(VARS as usize));
        let sources: Vec<Operand> = dests.iter().map(|_| random_operand(&mut rng)).collect();
        run(&dests, &sources);
    }
}

// ---------------------------------------------------------------- lost copy and swap, from source

fn behaves_like_the_interpreter(src: &str, list: &[&str], expected_stdout: &str) -> String {
    let an = analyze_ok(src);
    let c = pebble::driver::compile_passes(&an.hir, list).unwrap_or_else(|e| panic!("{e}"));
    let mut out = Vec::new();
    let (res, _) = pebble::driver::run_chunk(&c.chunk, &mut out, &RunOptions::default());
    res.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(String::from_utf8_lossy(&out), expected_stdout, "{list:?}");
    assert_all_agree("source case", &an.hir, &RunOptions::default());
    let dumped = pebble::driver::compile_passes(&an.hir, &list[..list.len() - 1]).unwrap();
    format!("{}", dumped.mir)
}

/// After GVN removes the copy `last = i`, the value of `i` from the PREVIOUS
/// iteration is read after the loop while `i` has already advanced: a copy placed
/// naively at the end of the latch would overwrite it (Briggs et al., lost copy).
#[test]
fn lost_copy_problem() {
    big_stack(|| {
        let src = "fn main() {\n    let n = len(\"abcde\");\n    let mut i = 1;\n    let mut last = 0;\n    loop {\n        last = i;\n        i = i + 1;\n        if i > n { break; }\n    }\n    println(to_str(last));\n    println(to_str(i));\n}\n";
        let ssa = behaves_like_the_interpreter(src, &["ssa", "gvn", "out-of-ssa"], "5\n6\n");
        assert!(ssa.contains("phi"), "{ssa}");
    });
}

/// `a, b = b, a` through a temporary becomes two phis that read each other once
/// the temporary is copy-propagated away: the copies must run as if in parallel.
#[test]
fn swap_problem() {
    big_stack(|| {
        let src = "fn main() {\n    let n = len(\"abc\");\n    let mut a = 1;\n    let mut b = 2;\n    let mut c = 3;\n    let mut i = 0;\n    while i < n * 2 - 1 {\n        let t = a;\n        a = b;\n        b = c;\n        c = t;\n        i = i + 1;\n    }\n    println(to_str(a * 100 + b * 10 + c));\n}\n";
        // Five rotations of (1, 2, 3).
        let ssa = behaves_like_the_interpreter(src, &["ssa", "gvn", "out-of-ssa"], "312\n");
        assert!(ssa.contains("phi"), "{ssa}");
    });
}

// ---------------------------------------------------------------- verifier negatives

/// A diamond that needs one phi, in valid SSA form.
fn diamond() -> MirProgram {
    let src = "fn pick(c: bool) -> int { let mut x = 1; if c { x = 2; } x }\nfn main() { println(to_str(pick(len(\"\") == 0))); }";
    let an = analyze_ok(src);
    let mut m = mir::lower::lower(&an.hir).unwrap();
    passes(&mut m, &["ssa"]).unwrap();
    m
}

#[test]
fn verifier_rejects_broken_ssa() {
    big_stack(|| {
        let pristine = diamond();
        mir::verify::verify(&pristine).expect("pristine SSA verifies");
        let fi = pristine
            .funcs
            .iter()
            .position(|f| f.blocks.iter().any(|b| !b.phis.is_empty()))
            .expect("a function with a phi");
        let join = pristine.funcs[fi]
            .blocks
            .iter()
            .position(|b| !b.phis.is_empty())
            .unwrap();
        let rejects = |what: &str, needle: &str, mutate: &dyn Fn(&mut Func)| {
            let mut m = pristine.clone();
            mutate(&mut m.funcs[fi]);
            let errs = mir::verify::verify(&m).expect_err(&format!("{what}: accepted"));
            assert!(
                errs.iter().any(|e| e.contains(needle)),
                "{what}: wanted `{needle}` in {errs:?}"
            );
        };
        rejects("second definition", "defined more than once", &|f| {
            let d = f.blocks[join].phis[0].dest;
            f.blocks[0].stmts.push(Stmt::new(
                StmtKind::Assign(d, Rvalue::Use(Operand::Const(Const::Int(0)))),
                0,
            ));
        });
        rejects(
            "missing phi argument",
            "has no argument for predecessor",
            &|f| {
                f.blocks[join].phis[0].args.pop();
            },
        );
        rejects("duplicate phi argument", "two arguments", &|f| {
            let a = f.blocks[join].phis[0].args[0].clone();
            f.blocks[join].phis[0].args.push(a);
        });
        rejects(
            "argument from a non-predecessor",
            "not a predecessor",
            &|f| {
                f.blocks[join].phis[0]
                    .args
                    .push((join as BlockId, Operand::Const(Const::Int(0))));
            },
        );
        rejects(
            "phi argument not available on its edge",
            "not defined on that path",
            &|f| {
                let d = f.blocks[join].phis[0].dest;
                f.blocks[join].phis[0].args[0].1 = Operand::Local(d);
            },
        );
        rejects(
            "use not dominated by its definition",
            "not dominated",
            &|f| {
                // Define a local in one arm of the diamond and return it after the join.
                let arm = f.blocks[join].phis[0].args[0].0 as usize;
                let other = f.blocks[join].phis[0].args[1].0 as usize;
                let l = f.new_local(Ty::Int);
                let target = if arm == 0 { other } else { arm };
                f.blocks[target].stmts.push(Stmt::new(
                    StmtKind::Assign(l, Rvalue::Use(Operand::Const(Const::Int(7)))),
                    0,
                ));
                let d = f.blocks[join].phis[0].dest;
                for b in &mut f.blocks {
                    b.term.for_each_use_mut(&mut |o| {
                        if *o == Operand::Local(d) {
                            *o = Operand::Local(l);
                        }
                    });
                    for s in &mut b.stmts {
                        s.for_each_use_mut(&mut |o| {
                            if *o == Operand::Local(d) {
                                *o = Operand::Local(l);
                            }
                        });
                    }
                }
            },
        );
        rejects(
            "use before definition in one block",
            "not dominated",
            &|f| {
                let l = f.new_local(Ty::Int);
                let m = f.new_local(Ty::Int);
                f.blocks[0].stmts.insert(
                    0,
                    Stmt::new(StmtKind::Assign(m, Rvalue::Use(Operand::Local(l))), 0),
                );
                f.blocks[0].stmts.push(Stmt::new(
                    StmtKind::Assign(l, Rvalue::Use(Operand::Const(Const::Int(1)))),
                    0,
                ));
            },
        );
        rejects("phi argument of the wrong type", "phi argument", &|f| {
            f.blocks[join].phis[0].args[0].1 = Operand::Const(Const::Bool(true));
        });
        rejects("phi outside SSA form", "not in SSA form", &|f| {
            f.ssa = false
        });
    });
}

#[test]
fn register_passes_and_ssa_passes_refuse_the_wrong_form() {
    big_stack(|| {
        let mut m = diamond();
        let e = passes(&mut m, &["dce"]).unwrap_err();
        assert!(e.contains("in SSA form"), "{e}");
        let mut m = random_func(0);
        let e = passes(&mut m, &["gvn"]).unwrap_err();
        assert!(e.contains("is not in SSA form"), "{e}");
    });
}

// ---------------------------------------------------------------- what LICM saves

/// Executed VM instructions at -O3 with and without `licm`. The counts are printed
/// (run with `--nocapture`); the only assertion is that behaviour is unchanged.
#[test]
fn licm_instruction_counts_are_printed_for_the_benchmarks() {
    big_stack(|| {
        let ro = RunOptions {
            max_depth: 10_000,
            fuel: u64::MAX / 2,
        };
        let with: Vec<&str> = opt::pipeline(3);
        let without: Vec<&str> = with.iter().copied().filter(|p| *p != "licm").collect();
        println!(
            "{:<20} {:>16} {:>16}",
            "program", "without licm", "with licm"
        );
        for f in files_with_ext(&manifest_dir().join("examples/bench"), "peb") {
            let an = analyze_ok(&std::fs::read_to_string(&f).unwrap());
            let run = |list: &[&str]| {
                let c =
                    pebble::driver::compile_passes(&an.hir, list).unwrap_or_else(|e| panic!("{e}"));
                let mut out = Vec::new();
                let (res, stats) = pebble::driver::run_chunk(&c.chunk, &mut out, &ro);
                (out, res.err().map(|e| e.to_string()), stats.instructions)
            };
            let (a, b) = (run(&without), run(&with));
            assert_eq!((&a.0, &a.1), (&b.0, &b.1), "{}", f.display());
            println!(
                "{:<20} {:>16} {:>16}",
                f.file_stem().unwrap().to_string_lossy(),
                a.2,
                b.2
            );
        }
    });
}
