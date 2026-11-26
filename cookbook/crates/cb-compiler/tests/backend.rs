//! Differential tests: the tree-walking interpreter is the oracle; the VM at
//! -O0..-O3 must print the same output and fail with the same message and trace.
//!
//! `PEBBLE_SEEDS=N` lengthens the random runs. Known harness limit: fuel is counted
//! in tree nodes by the interpreter and in instructions by the VM, so a generated
//! program with an endless loop can run out of fuel in one and reach a real trap
//! later in the other (seen from seed 6602 of the imperative generator, where all
//! back ends agree once the fuel limit is high enough).

mod common;
use common::*;
use pebble::interp::RunOptions;

fn all_programs() -> Vec<std::path::PathBuf> {
    let root = manifest_dir();
    let mut v = files_with_ext(&root.join("tests/run"), "peb");
    v.extend(files_with_ext(&root.join("examples"), "peb"));
    v
}

#[test]
fn run_tests_and_examples_agree_on_all_backends() {
    big_stack(|| {
        let files = all_programs();
        assert!(
            files.len() >= 64,
            "expected the 52 run tests plus the examples, found {}",
            files.len()
        );
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            let an = analyze_ok(&src);
            assert_all_agree(&f.display().to_string(), &an.hir, &RunOptions::default());
        }
    });
}

#[test]
fn run_tests_match_recorded_stdout_on_vm() {
    big_stack(|| {
        for f in files_with_ext(&manifest_dir().join("tests/run"), "peb") {
            let src = std::fs::read_to_string(&f).unwrap();
            let an = analyze_ok(&src);
            let want = std::fs::read_to_string(f.with_extension("stdout")).unwrap();
            for level in [0, 3] {
                let o = run_on(
                    &an.hir,
                    pebble::driver::Backend::Vm,
                    level,
                    &RunOptions::default(),
                );
                let mut got = o.stdout.clone();
                if let Some(e) = &o.error {
                    got.push_str(&format!("--- {e}\n"));
                }
                assert_eq!(got, want, "{} at -O{level}", f.display());
            }
        }
    });
}

// ------------------------------------------------------------ traps agree

const TRAP_PROGRAMS: &[(&str, &str, pebble::interp::RuntimeErrorKind)] = &[
    (
        "div_zero",
        "fn d(a: int, b: int) -> int { a / b }\nfn main() { println(\"before\"); println(to_str(d(1, len(\"\")))); }",
        pebble::interp::RuntimeErrorKind::DivZero,
    ),
    (
        "rem_zero",
        "fn main() { let z = len(\"\"); println(to_str(5 % z)); }",
        pebble::interp::RuntimeErrorKind::DivZero,
    ),
    (
        "overflow_mul",
        "fn f(x: int) -> int { x * x }\nfn main() { println(to_str(f(4000000000))); }",
        pebble::interp::RuntimeErrorKind::Overflow,
    ),
    (
        "overflow_add_imm",
        "fn main() { let mut m = 9223372036854775807; m = m + 1; println(to_str(m)); }",
        pebble::interp::RuntimeErrorKind::Overflow,
    ),
    (
        "overflow_neg",
        "fn main() { let m = -9223372036854775807 - 1; println(to_str(-m)); }",
        pebble::interp::RuntimeErrorKind::Overflow,
    ),
    (
        "overflow_min_div",
        "fn main() { let m = -9223372036854775807 - 1; let d = -1; println(to_str(m / d)); }",
        pebble::interp::RuntimeErrorKind::Overflow,
    ),
    (
        "bounds",
        "fn main() { let a = [1, 2, 3]; println(to_str(a[len(a)])); }",
        pebble::interp::RuntimeErrorKind::Bounds,
    ),
    (
        "bounds_negative",
        "fn main() { let a = [1, 2, 3]; let i = 0 - 1; println(to_str(a[i])); }",
        pebble::interp::RuntimeErrorKind::Bounds,
    ),
    (
        "bounds_store",
        "fn main() { let mut a = [1, 2]; a[2] = 5; }",
        pebble::interp::RuntimeErrorKind::Bounds,
    ),
    (
        "bounds_nested_field",
        "struct P { x: int }\nfn main() { let mut a = [P { x: 1 }]; a[3].x = 2; }",
        pebble::interp::RuntimeErrorKind::Bounds,
    ),
    (
        "pop_empty",
        "fn main() { let mut a: [int] = []; pop(a); }",
        pebble::interp::RuntimeErrorKind::EmptyPop,
    ),
    (
        "assert",
        "fn main() { assert(1 > 2); }",
        pebble::interp::RuntimeErrorKind::AssertFail,
    ),
    (
        "assert_msg",
        "fn main() { assert(1 > 2, \"nope\"); }",
        pebble::interp::RuntimeErrorKind::AssertFail,
    ),
    (
        "panic",
        "fn main() { panic(\"boom\"); }",
        pebble::interp::RuntimeErrorKind::Panic,
    ),
    (
        "trace_through_inlined_calls",
        "fn leaf(n: int) -> int { if n > 2 { panic(\"deep\") } else { n } }\nfn mid(n: int) -> int { leaf(n) + 1 }\nfn top(n: int) -> int { mid(n) + 1 }\nfn main() { println(to_str(top(1))); println(to_str(top(5))); }",
        pebble::interp::RuntimeErrorKind::Panic,
    ),
    (
        "bce_must_see_callee_shrinking_the_array",
        "fn shrink(mut a: [int]) { pop(a); pop(a); }\nfn main() { let xs = [1, 2, 3, 4]; let n = len(xs); let mut i = 0; while i < n { println(to_str(xs[i])); shrink(xs); i += 1; } }",
        pebble::interp::RuntimeErrorKind::Bounds,
    ),
    (
        "const_trap",
        "const C: int = 1 / len(\"\");\nfn main() { println(to_str(C)); }",
        pebble::interp::RuntimeErrorKind::DivZero,
    ),
    (
        "depth",
        "fn f(n: int) -> int { f(n + 1) + 1 }\nfn main() { println(to_str(f(0))); }",
        pebble::interp::RuntimeErrorKind::DepthLimit,
    ),
    (
        "fuel",
        "fn main() { let mut i = 0; while i >= 0 { i = (i + 1) % 7; } }",
        pebble::interp::RuntimeErrorKind::FuelExhausted,
    ),
    (
        "mutual_recursion_depth",
        "fn a(n: int) -> int { b(n) + 1 }\nfn b(n: int) -> int { a(n + 1) + 1 }\nfn main() { println(to_str(a(0))); }",
        pebble::interp::RuntimeErrorKind::DepthLimit,
    ),
];

#[test]
fn traps_have_identical_messages_kinds_and_traces() {
    big_stack(|| {
        let ro = RunOptions {
            max_depth: 10_000,
            fuel: 3_000_000,
        };
        for (name, src, kind) in TRAP_PROGRAMS {
            let an = analyze_ok(src);
            let all = run_everywhere(&an.hir, &ro);
            for (label, o) in &all {
                assert_eq!(o.kind, Some(*kind), "{name} on {label}: {o:?}");
            }
            assert_all_agree(name, &an.hir, &ro);
        }
    });
}

#[test]
fn trace_names_inlined_frames() {
    big_stack(|| {
        let (_, src, _) = TRAP_PROGRAMS
            .iter()
            .find(|p| p.0 == "trace_through_inlined_calls")
            .unwrap();
        let an = analyze_ok(src);
        let o = run_on(
            &an.hir,
            pebble::driver::Backend::Vm,
            3,
            &RunOptions::default(),
        );
        let e = o.error.unwrap();
        for f in ["leaf", "mid", "top", "main"] {
            assert!(e.contains(&format!("at {f}")), "missing frame {f} in\n{e}");
        }
        // And the optimiser really did inline: the O3 program has fewer functions.
        let c3 = pebble::driver::compile(
            &an.hir,
            &pebble::opt::OptOptions {
                level: 3,
                dump_after: vec![],
                verify: true,
            },
            &mut |_, _| {},
        )
        .unwrap();
        assert!(
            c3.chunk.funcs.len() < 4,
            "expected inlining, found {} functions",
            c3.chunk.funcs.len()
        );
    });
}

#[test]
fn deep_tail_recursion_is_the_documented_deviation() {
    // O0 hits the 10 000 frame limit; the tail-call-to-loop pass removes the frames.
    big_stack(|| {
        let src = "fn count(n: int, acc: int) -> int { if n == 0 { acc } else { count(n - 1, acc + 1) } }\nfn main() { println(to_str(count(50000, 0))); }";
        let an = analyze_ok(src);
        let ro = RunOptions::default();
        let o0 = run_on(&an.hir, pebble::driver::Backend::Vm, 0, &ro);
        let o2 = run_on(&an.hir, pebble::driver::Backend::Vm, 2, &ro);
        assert_eq!(o0.kind, Some(pebble::interp::RuntimeErrorKind::DepthLimit));
        assert_eq!(o2.stdout, "50000\n");
        assert!(o2.error.is_none());
    });
}

// ------------------------------------------------------- generated programs

/// Snippets appended to `main` so that a share of the generated programs trap.
fn trap_snippet(k: usize) -> &'static str {
    [
        "    let zz = [1, 2, 3];\n    println(to_str(zz[len(zz) + 1]));\n",
        "    let mut ov = 9223372036854775807;\n    ov = ov + 1;\n    println(to_str(ov));\n",
        "    assert(len(\"ab\") == 3, \"generated\");\n",
        "    let mut e = [1];\n    pop(e);\n    pop(e);\n",
        "    let d = len(\"a\") - 1;\n    println(to_str(10 / d));\n",
    ][k % 5]
}

fn generated(seed: u64) -> String {
    let mut g = common::progen::Gen {
        rng: Rng::new(seed + 100),
        fns: Vec::new(),
        counter: 0,
    };
    let mut src = g.program();
    if seed.is_multiple_of(3) {
        src.truncate(src.len() - 2); // drop the closing "}\n" of main
        src.push_str(trap_snippet(seed as usize / 3));
        src.push_str("}\n");
    }
    src
}

#[test]
fn generated_programs_agree_on_all_backends() {
    big_stack(|| {
        let ro = RunOptions {
            max_depth: 10_000,
            fuel: 5_000_000,
        };
        let (mut traps, mut kinds) = (0, std::collections::BTreeSet::new());
        // `PEBBLE_SEEDS=20000 cargo test ...` runs a longer soak.
        let n: u64 = std::env::var("PEBBLE_SEEDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1500);
        for seed in 0..n {
            let src = generated(seed);
            let an = pebble::analyze(&src, &Default::default());
            assert!(!an.has_errors(), "seed {seed} does not check:\n{src}");
            let all = run_everywhere(&an.hir, &ro);
            let (_, oracle) = &all[0];
            for (label, o) in &all[1..] {
                assert!(
                    outcomes_agree(oracle, o),
                    "seed {seed}: {label} differs from the interpreter\n{src}\noracle: {oracle:?}\n{label}: {o:?}"
                );
            }
            if let Some(k) = oracle.kind {
                traps += 1;
                kinds.insert(format!("{k:?}"));
            }
        }
        assert!(traps >= 150, "too few trapping programs: {traps}");
        assert!(kinds.len() >= 4, "trap kinds seen: {kinds:?}");
    });
}

// ----------------------------------------------------- one pass at a time

#[test]
fn every_pass_alone_preserves_behaviour_on_all_programs() {
    big_stack(|| {
        let ro = RunOptions {
            max_depth: 10_000,
            fuel: 20_000_000,
        };
        for f in all_programs() {
            let src = std::fs::read_to_string(&f).unwrap();
            let an = analyze_ok(&src);
            let mut out = Vec::new();
            let oracle = pebble::interp::run_with(&an.hir, &mut out, &ro);
            for pass in pebble::opt::PASS_NAMES {
                // SSA passes only run between `ssa` and `out-of-ssa`.
                let seq: Vec<&str> = match *pass {
                    "ssa" | "out-of-ssa" => vec!["ssa", "out-of-ssa"],
                    p if pebble::opt::SSA_ONLY.contains(&p) => vec!["ssa", p, "out-of-ssa"],
                    p => vec![p],
                };
                let c = pebble::driver::compile_passes(&an.hir, &seq)
                    .unwrap_or_else(|e| panic!("{} / {pass}: {e}", f.display()));
                let mut vout = Vec::new();
                let (res, _) = pebble::driver::run_chunk(&c.chunk, &mut vout, &ro);
                assert_eq!(vout, out, "{} / pass {pass}: output differs", f.display());
                assert_eq!(
                    res.err().map(|e| e.to_string()),
                    oracle.as_ref().err().map(|e| e.to_string()),
                    "{} / pass {pass}: error differs",
                    f.display()
                );
            }
        }
    });
}

// ------------------------------------------------------- monomorphisation

fn mono_stats(src: &str) -> pebble::mir::MonoStats {
    let an = analyze_ok(src);
    pebble::mir::lower::lower(&an.hir).unwrap().stats
}

#[test]
fn monomorphisation_counts_distinct_instances_once() {
    big_stack(|| {
        let src = "fn id<T>(x: T) -> T { x }\nfn pair<A, B>(a: A, b: B) -> (A, B) { (id(a), id(b)) }\n\
                   fn main() { let a = id(1); let b = id(2); let c = id(\"s\"); let d = id([1]); let e = pair(1, true); let f = pair(2, false); let g = pair(\"x\", 1);\n\
                   println(to_str(a + b + len(c) + len(d) + e.0 + f.0 + g.1)); }";
        let st = mono_stats(src);
        let get = |n: &str| {
            st.generic_instances
                .iter()
                .find(|(m, _)| m == n)
                .map(|(_, c)| *c)
        };
        // id: int, str, [int], bool (the pair calls reuse int/bool/str).
        assert_eq!(get("id"), Some(4), "{st:?}");
        // pair: (int, bool) twice counts once, plus (str, int).
        assert_eq!(get("pair"), Some(2), "{st:?}");
        assert_eq!(st.total_instances, 1 + 4 + 2);
    });
}

#[test]
fn unused_generic_functions_are_not_instantiated() {
    big_stack(|| {
        let st = mono_stats("fn never<T>(x: T) -> T { x }\nfn main() { println(\"hi\"); }");
        assert_eq!(st.total_instances, 1);
        assert!(st.generic_instances.is_empty());
    });
}

#[test]
fn bounded_generic_recursion_is_one_instance() {
    big_stack(|| {
        let src = "fn down<T>(x: T, n: int) -> int { if n == 0 { 0 } else { down(x, n - 1) + 1 } }\nfn main() { println(to_str(down(\"a\", 5))); }";
        let st = mono_stats(src);
        assert_eq!(st.generic_instances, vec![("down".to_string(), 1)]);
    });
}

#[test]
fn polymorphic_recursion_is_rejected_not_looped() {
    big_stack(|| {
        let src = "fn grow<T>(x: T, n: int) -> int { if n == 0 { 0 } else { grow([x], n - 1) } }\nfn main() { println(to_str(grow(1, 3))); }";
        let an = analyze_ok(src);
        let err = pebble::mir::lower::lower(&an.hir).unwrap_err();
        assert!(err.message.contains("polymorphic recursion"), "{err}");
    });
}

#[test]
fn generic_programs_run_identically() {
    big_stack(|| {
        for name in ["generic_option", "generic_list", "higher_order"] {
            let src = std::fs::read_to_string(manifest_dir().join(format!("examples/{name}.peb")))
                .unwrap();
            let an = analyze_ok(&src);
            assert_all_agree(name, &an.hir, &RunOptions::default());
        }
    });
}

// --------------------------------------------------------------- benchmarks

#[test]
fn bench_programs_agree_and_print_a_comparison() {
    big_stack(|| {
        let mut rows = Vec::new();
        for f in files_with_ext(&manifest_dir().join("examples/bench"), "peb") {
            let src = std::fs::read_to_string(&f).unwrap();
            let an = analyze_ok(&src);
            let name = f.file_stem().unwrap().to_string_lossy().to_string();
            let row = pebble::driver::bench_program(&name, &an.hir).unwrap();
            assert!(row.agree, "{name}: back ends disagree");
            rows.push(row);
        }
        assert!(rows.len() >= 7);
        // Timing is printed only (run with --nocapture); nothing is asserted about speed.
        println!("{}", pebble::driver::format_bench(&rows));
    });
}

#[test]
fn imperative_programs_agree_on_all_backends() {
    big_stack(|| {
        let ro = RunOptions {
            max_depth: 10_000,
            fuel: 3_000_000,
        };
        let n: u64 = std::env::var("PEBBLE_SEEDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500);
        let (mut traps, mut checked) = (0, 0);
        for seed in 0..n {
            let src = common::impgen::ImpGen::new(seed).program();
            let an = pebble::analyze(&src, &Default::default());
            assert!(
                !an.has_errors(),
                "seed {seed} does not check:\n{src}\n{:?}",
                an.diagnostics
                    .iter()
                    .map(|d| d.message.clone())
                    .collect::<Vec<_>>()
            );
            let all = run_everywhere(&an.hir, &ro);
            let (_, oracle) = &all[0];
            for (label, o) in &all[1..] {
                assert!(
                    outcomes_agree(oracle, o),
                    "seed {seed}: {label} differs from the interpreter\n{src}\noracle: {oracle:?}\n{label}: {o:?}"
                );
            }
            traps += oracle.kind.is_some() as usize;
            checked += 1;
        }
        assert!(checked == n as usize);
        assert!(traps < checked, "every imperative program trapped");
    });
}

#[test]
#[ignore]
fn dump_imperative_seed() {
    let seed: u64 = std::env::var("PEBBLE_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    // Debugging aid: `PEBBLE_SEED=58 cargo test --release dump_imperative -- --ignored --nocapture`.
    println!("{}", common::impgen::ImpGen::new(seed).program());
}

// ------------------------------------------------- source locations in traps
//
// Compared between the interpreter (oracle) and the VM at every -O level, in
// `Outcome`: stdout, kind, the whole message with the function trace, the span
// of the failing expression (`-->`) and the call site of every frame but
// `main`. The only exception is fuel exhaustion, where the interpreter counts
// tree nodes and the VM counts instructions: there only the kind must match
// (the VM still reports *a* span and trace).

#[test]
fn every_trap_has_a_location_that_matches_the_interpreter_at_every_level() {
    big_stack(|| {
        let ro = RunOptions {
            max_depth: 10_000,
            fuel: 3_000_000,
        };
        for (name, src, kind) in TRAP_PROGRAMS {
            let an = analyze_ok(src);
            for (label, o) in run_everywhere(&an.hir, &ro) {
                assert!(o.span.is_some(), "{name} on {label}: no `-->` location");
                if *kind != pebble::interp::RuntimeErrorKind::FuelExhausted {
                    let oracle = run_on(&an.hir, Backend::Interp, 0, &ro);
                    assert_eq!(o.span, oracle.span, "{name} on {label}: location differs");
                    assert_eq!(
                        o.call_sites, oracle.call_sites,
                        "{name} on {label}: call sites differ"
                    );
                }
            }
        }
    });
}

use pebble::driver::Backend;

#[test]
fn rendered_trap_points_at_the_expression_and_notes_inlined_frames() {
    big_stack(|| {
        let src = "fn leaf(n: int) -> int { if n > 2 { panic(\"deep\") } else { n } }\nfn mid(n: int) -> int { leaf(n) + 1 }\nfn main() { println(to_str(mid(5))); }\n";
        let an = analyze_ok(src);
        let file = pebble::source::SourceFile::new("t.peb", src.to_string());
        let render = |level: u8, backend: Backend| {
            let mut out = Vec::new();
            let opts = pebble::opt::OptOptions {
                level,
                dump_after: vec![],
                verify: true,
            };
            pebble::driver::run(&an.hir, backend, &opts, &mut out, &RunOptions::default())
                .unwrap()
                .unwrap_err()
                .render(&file)
        };
        let want = "runtime error: panic: deep\n  at leaf\n  at mid\n  at main\n  --> t.peb:1:37";
        assert_eq!(render(0, Backend::Interp), want);
        assert_eq!(render(0, Backend::Vm), want);
        assert_eq!(render(2, Backend::Vm), want);
        let o3 = render(3, Backend::Vm);
        assert!(o3.starts_with(want), "{o3}");
        assert!(
            o3.contains("note: `leaf` was inlined from t.peb:2:25"),
            "{o3}"
        );
        assert!(
            o3.contains("note: `mid` was inlined from t.peb:3:28"),
            "{o3}"
        );
    });
}

#[test]
fn fuel_exhaustion_reports_kind_message_and_a_span_like_the_interpreter() {
    big_stack(|| {
        let src = "fn main() {\n    let mut i = 0;\n    while i >= 0 { i = (i + 1) % 7; }\n}\n";
        let an = analyze_ok(src);
        let ro = RunOptions {
            max_depth: 10_000,
            fuel: 100_000,
        };
        let all = run_everywhere(&an.hir, &ro);
        for (label, o) in &all {
            assert_eq!(
                o.kind,
                Some(pebble::interp::RuntimeErrorKind::FuelExhausted),
                "{label}"
            );
            assert!(
                o.error
                    .as_deref()
                    .unwrap()
                    .starts_with("runtime error: step limit exceeded (possible infinite loop)"),
                "{label}: {o:?}"
            );
            let sp = o
                .span
                .unwrap_or_else(|| panic!("{label}: fuel error without a span"));
            // Somewhere inside the loop (line 3 of the program).
            let line3 = src.lines().take(2).map(|l| l.len() + 1).sum::<usize>();
            assert!(
                sp.start as usize >= line3,
                "{label}: span {sp} is before the loop"
            );
        }
    });
}
