//! MIR golden tests: lower a small program, run the passes named in its
//! `// passes:` line and compare the printed MIR (before and after) with
//! `tests/mir/NAME.mir`. `BLESS=1` rewrites the goldens. Each case is also run
//! on the VM and must behave like the unoptimised program.

mod common;
use common::*;
use pebble::interp::RunOptions;

fn snapshot(src: &str) -> (String, pebble::sema::Program) {
    let an = analyze_ok(src);
    let passes = passes_directive(src);
    let refs: Vec<&str> = passes.iter().map(|s| s.as_str()).collect();
    let before = pebble::driver::compile_passes(&an.hir, &[]).unwrap_or_else(|e| panic!("{e}"));
    let after = pebble::driver::compile_passes(&an.hir, &refs).unwrap_or_else(|e| panic!("{e}"));
    (
        format!(
            "=== before ===\n{}\n=== after: {} ===\n{}",
            before.mir,
            passes.join(", "),
            after.mir
        ),
        an.hir,
    )
}

#[test]
fn mir_golden_snapshots() {
    big_stack(|| {
        let files = files_with_ext(&manifest_dir().join("tests/mir"), "peb");
        assert!(files.len() >= 10, "need golden cases for every pass");
        let mut failures = Vec::new();
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            let (snap, hir) = snapshot(&src);
            expect_file(&f.with_extension("mir"), &snap, &mut failures);
            // Observable behaviour is unchanged by the passes.
            let passes = passes_directive(&src);
            let refs: Vec<&str> = passes.iter().map(|s| s.as_str()).collect();
            let c = pebble::driver::compile_passes(&hir, &refs).unwrap();
            let mut out = Vec::new();
            let (res, _) = pebble::driver::run_chunk(&c.chunk, &mut out, &RunOptions::default());
            let mut oracle = Vec::new();
            let ores = pebble::interp::run(&hir, &mut oracle);
            assert_eq!(out, oracle, "{}", f.display());
            assert_eq!(
                res.err().map(|e| e.to_string()),
                ores.err().map(|e| e.to_string()),
                "{}",
                f.display()
            );
        }
        assert!(
            failures.is_empty(),
            "{} MIR snapshot mismatches (BLESS=1 to update):\n{}",
            failures.len(),
            failures.join("\n")
        );
    });
}

/// The golden files must show each optimisation actually happening.
#[test]
fn golden_cases_demonstrate_their_pass() {
    big_stack(|| {
        let read = |name: &str| {
            let src = std::fs::read_to_string(manifest_dir().join(format!("tests/mir/{name}.peb")))
                .unwrap();
            let (snap, _) = snapshot(&src);
            let (before, after) = snap.split_once("=== after:").unwrap();
            (before.to_string(), after.to_string())
        };
        let (_, a) = read("dead_branch");
        assert!(!a.contains("* 100"), "dead arm should be gone:\n{a}");
        let (_, a) = read("const_fold_across_blocks");
        assert!(
            a.contains("return 15") || a.contains("= 15"),
            "x*2 + x should fold to 15:\n{a}"
        );
        let (b, a) = read("bce_counted_loop");
        assert!(
            !b.contains("!nocheck") && a.contains("!nocheck"),
            "bounds check should be dropped:\n{a}"
        );
        let (_, a) = read("bce_not_proved");
        assert!(
            !a.contains("!nocheck"),
            "pop in the loop must keep the check:\n{a}"
        );
        let (_, a) = read("bce_length_hoisted");
        assert!(
            a.contains("!nocheck"),
            "hoisted length is still provable:\n{a}"
        );
        let (b, a) = read("tail_recursion");
        assert!(
            b.matches("call fn#1").count() == 2 && a.matches("call fn#1").count() == 1,
            "the self call should become a jump:\n{a}"
        );
        let (b, a) = read("cse_local");
        assert!(
            b.matches(" * ").count() == 2 && a.matches(" * ").count() == 1,
            "second multiplication should be reused:\n{a}"
        );
        let (_, a) = read("keep_trapping_ops");
        assert!(
            a.contains(" / ") || a.contains("/ "),
            "division by zero must survive:\n{a}"
        );
        let (_, a) = read("inline_single_site");
        assert!(
            a.matches("fn work").count() == 0,
            "inlined single-site callee should be removed:\n{a}"
        );
        let (_, a) = read("ssa_phi_placement");
        assert!(
            a.contains("phi") && a.matches("= phi").count() == 3,
            "pruned SSA: phis for i and s only (header and join):\n{a}"
        );
        let (_, a) = read("sccp_loop_constant");
        assert!(
            a.contains("to_str(1)") && !a.contains("= 2"),
            "x is the constant 1 and the dead arm is gone:\n{a}"
        );
        // ... which plain constant propagation cannot see.
        let src = std::fs::read_to_string(manifest_dir().join("tests/mir/sccp_loop_constant.peb"))
            .unwrap();
        let plain = pebble::driver::compile_passes(
            &analyze_ok(&src).hir,
            &["const-prop", "dce", "simplify-cfg"],
        )
        .unwrap();
        assert!(
            !format!("{}", plain.mir).contains("to_str(1)"),
            "{}",
            plain.mir
        );
        let (b, a) = read("gvn_across_blocks");
        assert!(
            b.matches(" * ").count() == 4 && a.matches(" * ").count() == 1,
            "one multiplication should be left:\n{a}"
        );
        let (_, a) = read("licm_hoist");
        let header = a
            .split("fn f(")
            .nth(1)
            .unwrap()
            .split("bb1:")
            .next()
            .unwrap();
        assert!(
            header.contains(" * 3") && header.contains(" > 100") && header.contains(" % 7"),
            "all three belong in the preheader:\n{a}"
        );
        let (_, a) = read("licm_no_speculation");
        let preheader = a
            .split("fn f(")
            .nth(1)
            .unwrap()
            .split("bb1:")
            .next()
            .unwrap();
        assert!(
            !preheader.contains(" / ") && !preheader.contains(" * "),
            "trapping body operations must not be hoisted:\n{a}"
        );
        let (_, a) = read("out_of_ssa_swap");
        assert!(
            a.contains("_8 = _5\n    _5 = _6\n    _6 = _8"),
            "the swap needs a temporary:\n{a}"
        );
        let (_, a) = read("match_switch");
        assert!(a.contains("switch"), "enum match should use a switch:\n{a}");
    });
}
