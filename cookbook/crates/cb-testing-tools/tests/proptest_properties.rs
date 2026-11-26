//! Property-based testing with proptest. Instead of hand-picked examples we state LAWS and let
//! proptest generate hundreds of inputs; on failure it SHRINKS to a minimal counterexample.
use std::time::Duration;

use cb_testing_tools::duration::{format_duration, parse_duration};
use cb_testing_tools::expr::{
    Expr, Op, eval, format_expr, format_expr_buggy, parse_expr, simplify,
};
use cb_testing_tools::lru::LruMap;
use proptest::prelude::*;
use proptest::test_runner::{
    Config, FileFailurePersistence, RngAlgorithm, TestError, TestRng, TestRunner,
};

// ---- strategies ------------------------------------------------------------------------------

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![Just(Op::Add), Just(Op::Sub), Just(Op::Mul), Just(Op::Div)]
}

/// RECURSIVE strategy for trees: leaves, then up to depth 4 / ~32 nodes of Neg and Bin.
/// Literals are non-negative because `-5` parses as `Neg(Num(5))`, not `Num(-5)`.
fn arb_expr() -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![
        3 => (0i64..1000).prop_map(Expr::Num),
        2 => "[a-z][a-z0-9_]{0,3}".prop_map(Expr::Var), // regex strategy: String matching the pattern
    ];
    leaf.prop_recursive(4, 32, 4, |inner| {
        prop_oneof![
            inner.clone().prop_map(|e| Expr::Neg(Box::new(e))),
            (arb_op(), inner.clone(), inner).prop_map(|(op, l, r)| Expr::Bin(
                op,
                Box::new(l),
                Box::new(r)
            )),
        ]
    })
}

// `prop_compose!` builds a strategy from a function whose params are drawn from other strategies.
prop_compose! {
    fn arb_duration()(days in 0u64..400, h in 0u64..24, m in 0u64..60, s in 0u64..60, ms in 0u64..1000) -> Duration {
        Duration::from_millis((((days * 24 + h) * 60 + m) * 60 + s) * 1000 + ms)
    }
}

fn env(name: &str) -> Option<i64> {
    Some(name.bytes().map(i64::from).sum::<i64>() - 300)
}

fn size(e: &Expr) -> usize {
    match e {
        Expr::Num(_) | Expr::Var(_) => 1,
        Expr::Neg(x) => 1 + size(x),
        Expr::Bin(_, l, r) => 1 + size(l) + size(r),
    }
}

// ---- configuration: cases, seeds, persistence ---------------------------------------------------
//
// `ProptestConfig` controls case count and the RNG. By default every run uses a fresh random
// seed (good: more coverage over time) and, on failure, appends the failing seed to
// `proptest-regressions/<test file>.txt` next to the crate; those saved seeds are REPLAYED FIRST
// on every later run, so a bug found once stays a permanent regression test. COMMIT that
// directory. For a reproducible CI suite pin the seed (`rng_seed: RngSeed::Fixed(..)` below, or
// the env var `PROPTEST_RNG_SEED=123`) and the case count (`PROPTEST_CASES=1000`).
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 128,
        rng_seed: proptest::test_runner::RngSeed::Fixed(0xC0FFEE),
        // The default (`SourceParallel`) cannot locate lib.rs/main.rs for integration tests, so
        // name the regression file explicitly.
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/proptest_properties.txt",
        ))),
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    // Round trip: parse is a left inverse of format over ALL trees.
    #[test]
    fn expr_roundtrip(e in arb_expr()) {
        prop_assert_eq!(parse_expr(&format_expr(&e)), Ok(e));
    }

    // Idempotence: formatting a parsed formatted tree gives the same text again (canonical form).
    #[test]
    fn format_is_canonical(e in arb_expr()) {
        let once = format_expr(&e);
        let twice = format_expr(&parse_expr(&once).unwrap());
        prop_assert_eq!(once, twice);
    }

    // Idempotence of an optimiser, plus semantic preservation (oracle = the evaluator).
    #[test]
    fn simplify_idempotent_and_sound(e in arb_expr()) {
        let s = simplify(&e);
        prop_assert_eq!(simplify(&s), s.clone());
        prop_assert_eq!(eval(&s, &env), eval(&e, &env));
    }

    // Algebraic laws on wrapping arithmetic: commutativity and associativity of + and *.
    #[test]
    fn add_mul_laws(a in arb_expr(), b in arb_expr(), c in arb_expr(), op in prop_oneof![Just(Op::Add), Just(Op::Mul)]) {
        let bin = |l: &Expr, r: &Expr| Expr::Bin(op, Box::new(l.clone()), Box::new(r.clone()));
        prop_assert_eq!(eval(&bin(&a, &b), &env), eval(&bin(&b, &a), &env));
        prop_assert_eq!(eval(&bin(&bin(&a, &b), &c), &env), eval(&bin(&a, &bin(&b, &c)), &env));
        // ... and the PARSER knows + is left-associative even though the tree differs:
        prop_assert_ne!(format_expr(&bin(&a, &bin(&b, &c))), format_expr(&bin(&bin(&a, &b), &c)));
    }

    // Parsers must reject garbage with Err, never panic: any::<String>() produces arbitrary unicode.
    #[test]
    fn parsers_never_panic(s in any::<String>()) {
        let _ = parse_expr(&s);
        let _ = parse_duration(&s);
    }

    // Duration: round trip over a composed strategy, and over raw u64 milliseconds via any::<T>().
    #[test]
    fn duration_roundtrip(d in arb_duration(), raw in any::<u32>()) {
        prop_assert_eq!(parse_duration(&format_duration(d)), Ok(d));
        let d2 = Duration::from_millis(u64::from(raw));
        prop_assert_eq!(parse_duration(&format_duration(d2)), Ok(d2));
    }

    // Regex strategy as a GRAMMAR: anything the pattern generates must parse, and formatting
    // the result is idempotent (non-canonical "90s" becomes "1m30s" and then stays there).
    #[test]
    fn duration_canonicalises(s in "([0-9]{1,3}(d|h|m|s|ms)){1,4}") {
        let canon = format_duration(parse_duration(&s).unwrap());
        prop_assert_eq!(format_duration(parse_duration(&canon).unwrap()), canon);
    }
}

// ---- model-based testing -----------------------------------------------------------------------
// Generate a random SEQUENCE of operations, run it against the real implementation and against a
// trivially-correct reference model (an ordered Vec, MRU first), compare after every step.

#[derive(Debug, Clone)]
enum LruOp {
    Put(u8, u8),
    Get(u8),
    Remove(u8),
}

fn arb_lru_op() -> impl Strategy<Value = LruOp> {
    // small key space (0..6) => lots of collisions, hits and evictions
    prop_oneof![
        4 => (0u8..6, any::<u8>()).prop_map(|(k, v)| LruOp::Put(k, v)),
        4 => (0u8..6).prop_map(LruOp::Get),
        1 => (0u8..6).prop_map(LruOp::Remove),
    ]
}

prop_compose! {
    fn arb_lru_scenario()(cap in 1usize..5, ops in prop::collection::vec(arb_lru_op(), 0..80)) -> (usize, Vec<LruOp>) {
        (cap, ops)
    }
}

proptest! {
    #![proptest_config(config())]
    #[test]
    fn lru_matches_model((cap, ops) in arb_lru_scenario()) {
        let mut real = LruMap::new(cap);
        let mut model: Vec<(u8, u8)> = Vec::new(); // front = most recently used
        for op in ops {
            match op {
                LruOp::Put(k, v) => {
                    let evicted = if let Some(i) = model.iter().position(|e| e.0 == k) {
                        model.remove(i);
                        None
                    } else if model.len() == cap {
                        model.pop()
                    } else {
                        None
                    };
                    model.insert(0, (k, v));
                    prop_assert_eq!(real.put(k, v), evicted);
                }
                LruOp::Get(k) => {
                    let want = model.iter().position(|e| e.0 == k).map(|i| {
                        let e = model.remove(i);
                        model.insert(0, e);
                        e.1
                    });
                    prop_assert_eq!(real.get(&k).copied(), want);
                }
                LruOp::Remove(k) => {
                    let want = model.iter().position(|e| e.0 == k).map(|i| model.remove(i).1);
                    prop_assert_eq!(real.remove(&k), want);
                }
            }
            // invariants after EVERY step, not just at the end
            prop_assert!(real.len() <= cap);
            prop_assert_eq!(real.keys_mru(), model.iter().map(|e| e.0).collect::<Vec<_>>());
        }
    }
}

// ---- shrinking ---------------------------------------------------------------------------------
// `format_expr_buggy` forgets the parentheses on right operands: `a - (b - c)` prints as
// `a - b - c`. Random failing trees can be big; proptest shrinks them toward the smallest tree
// that still fails. We drive `TestRunner` by hand (with a fixed ChaCha seed => deterministic)
// so the failure is *asserted on* instead of failing the suite.

#[test]
fn shrinking_finds_a_minimal_counterexample() {
    let cfg = Config {
        cases: 512,
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner =
        TestRunner::new_with_rng(cfg, TestRng::from_seed(RngAlgorithm::ChaCha, &[7u8; 32]));
    let result = runner.run(&arb_expr(), |e| {
        prop_assert_eq!(parse_expr(&format_expr_buggy(&e)), Ok(e));
        Ok(())
    });
    let Err(TestError::Fail(_reason, minimal)) = result else {
        panic!("the buggy printer should be caught")
    };
    println!(
        "minimal counterexample: {minimal:?} => {}",
        format_expr_buggy(&minimal)
    );
    // The smallest failing shape is Bin(op, leaf, Bin(op, leaf, leaf)) with the SAME precedence level: 5 nodes.
    assert!(
        size(&minimal) <= 5,
        "shrinking should reach <= 5 nodes, got {minimal:?}"
    );
}

// The raw experience (random, big counterexample, writes a regression file): run with
//   cargo test -p cb-testing-tools --test proptest_properties -- --ignored
proptest! {
    #[test]
    #[ignore = "deliberately fails: shows proptest's failure report and writes proptest-regressions/"]
    fn buggy_printer_roundtrip(e in arb_expr()) {
        prop_assert_eq!(parse_expr(&format_expr_buggy(&e)), Ok(e));
    }
}
