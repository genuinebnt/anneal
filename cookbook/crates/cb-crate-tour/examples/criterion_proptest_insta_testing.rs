//! RECIPE 1117 (crates): testing tools: proptest (strategies, shrinking, stateful), insta inline snapshots, assert_cmd + predicates, hand-written fakes, criterion
//! LEVEL: intermediate
//! Use in: any crate whose behaviour is hard to cover with a handful of examples.   Run: cargo run --release -p cb-crate-tour --example criterion_proptest_insta_testing
//! COMMON USES: round-trip and oracle properties for parsers and data structures, snapshots of formatted output, black-box CLI tests, benchmarks that guard hot paths.
//! METHODS: proptest::proptest!, proptest::prop_compose!, proptest::prop_oneof!, Strategy::{prop_map, prop_recursive}, proptest::collection::{vec, btree_map}, proptest::option::of, any, Just, TestRunner::new_with_rng, TestRng::deterministic_rng, TestRunner::run, TestError::Fail, prop_assert!, prop_assert_eq!, prop_assume!, insta::assert_snapshot!, insta::assert_debug_snapshot!, assert_cmd::Command, Assert::{success, failure, code, stdout, stderr}, predicates::prelude::predicate, predicate::str::{contains, starts_with, is_match}, PredicateBooleanExt::{and, not}, Strategy::{prop_flat_map, prop_filter, new_tree}, ValueTree::{simplify, complicate, current}, proptest::sample::{select, Index}, TestError::Abort, FileFailurePersistence::Direct, insta::Settings::{new, add_filter, set_description, bind}, assert_cmd::Command::{write_stdin, env, env_clear, current_dir, output}, predicate::{eq, lt, ge, in_iter, function}, predicate::float::is_close, predicate::path::{exists, is_file, is_dir, missing, eq_file}, Predicate::{eval, find_case}, and, in benches/crate_tour_bench.rs, criterion::{criterion_group!, criterion_main!, Criterion::benchmark_group, BenchmarkId, Throughput, Bencher::{iter, iter_batched}}
//! DOCS: https://docs.rs/proptest/1/proptest/ https://docs.rs/insta/1/insta/ https://docs.rs/assert_cmd/2/ https://docs.rs/predicates/3/ https://docs.rs/criterion/0.5/criterion/
//! CRATES: proptest = "1", insta = "1" (feature filters), assert_cmd = "2", predicates = "3", criterion = "0.5" (not the newest line)
//!
//! WHERE TESTS LIVE
//!   unit tests         -> `#[cfg(test)] mod tests` next to the code; may test private items
//!   integration tests  -> tests/*.rs; public API only; each file is its own crate (share helpers via tests/support/mod.rs)
//!   doctests           -> examples in /// comments that compile and run
//!   benchmarks         -> benches/*.rs with `harness = false`; see benches/crate_tour_bench.rs, which includes THIS file
//!                         Run: cargo bench -p cb-crate-tour --bench crate_tour_bench -- --quick
//!
//! PROPERTY CHEAT SHEET: round trip (decode(encode(x)) == x) ; oracle (fast == obviously correct) ; invariant (output is
//! sorted AND a permutation) ; idempotence (f(f(x)) == f(x)) ; metamorphic (relation between two runs) ; never panics.
//! REGRESSION FILES: inside `proptest!` tests, a failing seed is saved under proptest-regressions/; COMMIT those files.
//! The sections here disable persistence (except `regression_files`, which writes to a temporary directory) and fix the
//! RNG so that every run is identical.
//! SNAPSHOTS: review changes with `cargo insta review` (cargo-insta tool). A snapshot proves "unchanged", not "correct".
//!
//! Read: https://proptest-rs.github.io/proptest/ ; https://insta.rs/docs/ ; https://bheisler.github.io/criterion.rs/book/
//!
//! TRY NEXT:
//!   - [S] Predict the minimal failing input for the buggy decoder before reading the assert.
//!   - [M] Add a `peek` operation to the stateful test and a bug that only it exposes.
//!   - [A] PROJECT: write proptest strategies for the Datalog or SQL recipes and let proptest shrink their failures.
#![allow(dead_code)]
use assert_cmd::Command;
use predicates::prelude::*;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestError, TestRng, TestRunner};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

// ---------------- code under test ----------------
pub fn rle_encode(s: &str) -> Vec<(char, u8)> {
    let mut out: Vec<(char, u8)> = vec![];
    for c in s.chars() {
        match out.last_mut() {
            Some((d, n)) if *d == c && *n < u8::MAX => *n += 1,
            _ => out.push((c, 1)),
        }
    }
    out
}
pub fn rle_decode(r: &[(char, u8)]) -> String {
    r.iter()
        .flat_map(|&(c, n)| std::iter::repeat_n(c, n as usize))
        .collect()
}
/// Deliberately wrong: runs longer than 9 lose characters.
fn buggy_decode(r: &[(char, u8)]) -> String {
    r.iter()
        .flat_map(|&(c, n)| std::iter::repeat_n(c, n.min(9) as usize))
        .collect()
}

/// A fixed-capacity queue that drops the OLDEST element when full (the stateful test's subject).
pub struct Ring {
    slots: Vec<u32>,
    head: usize,
    len: usize,
}
impl Ring {
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0);
        Ring {
            slots: vec![0; cap],
            head: 0,
            len: 0,
        }
    }
    pub fn push(&mut self, v: u32) {
        let cap = self.slots.len();
        self.slots[(self.head + self.len) % cap] = v;
        if self.len == cap {
            self.head = (self.head + 1) % cap
        } else {
            self.len += 1
        }
    }
    pub fn pop(&mut self) -> Option<u32> {
        (self.len > 0).then(|| {
            let v = self.slots[self.head];
            self.head = (self.head + 1) % self.slots.len();
            self.len -= 1;
            v
        })
    }
    pub fn len(&self) -> usize {
        self.len
    }
}

// ---------------- proptest ----------------
fn runner(cases: u32) -> TestRunner {
    TestRunner::new_with_rng(
        Config {
            cases,
            failure_persistence: None,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    )
}
/// Strings with long runs: a uniform random string would almost never contain one. GENERATOR DESIGN is most of the skill.
fn runs() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        (
            prop_oneof![4 => Just('a'), 4 => Just('b'), 1 => any::<char>()],
            1usize..40,
        ),
        0..8,
    )
    .prop_map(|v| {
        v.into_iter()
            .flat_map(|(c, n)| std::iter::repeat_n(c, n))
            .collect()
    })
}
prop_compose! {
    /// Two-stage composition: the second set of strategies may depend on values from the first.
    fn index_into_vec()(v in proptest::collection::vec(any::<i32>(), 1..50))
                       (i in 0..v.len(), v in Just(v)) -> (Vec<i32>, usize) {
        (v, i)
    }
}
#[derive(Debug, Clone, PartialEq)]
enum Json {
    Num(i32),
    List(Vec<Json>),
    Map(BTreeMap<String, Json>),
}
fn json() -> impl Strategy<Value = Json> {
    any::<i32>()
        .prop_map(Json::Num)
        .prop_recursive(3, 24, 4, |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..4).prop_map(Json::List),
                proptest::collection::btree_map("[a-z]{1,3}", inner, 0..4).prop_map(Json::Map),
            ]
        })
}
fn depth(j: &Json) -> usize {
    match j {
        Json::Num(_) => 0,
        Json::List(v) => 1 + v.iter().map(depth).max().unwrap_or(0),
        Json::Map(m) => 1 + m.values().map(depth).max().unwrap_or(0),
    }
}

fn properties() {
    runner(500)
        .run(&runs(), |s| {
            let enc = rle_encode(&s);
            prop_assert_eq!(rle_decode(&enc), s.clone()); // round trip
            prop_assert!(enc.iter().all(|r| r.1 > 0)); // invariants
            prop_assert!(
                enc.windows(2)
                    .all(|w| w[0].0 != w[1].0 || w[0].1 == u8::MAX)
            );
            Ok(())
        })
        .unwrap();
    runner(200)
        .run(&index_into_vec(), |(v, i)| {
            prop_assert!(v.get(i).is_some());
            Ok(())
        })
        .unwrap();
    runner(200)
        .run(&json(), |j| {
            prop_assert!(depth(&j) <= 3, "{:?}", j);
            Ok(())
        })
        .unwrap();
    // prop_filter (not used here) and prop_assume REJECT cases; too many rejections abort the test. Prefer constructing valid values.
    runner(200)
        .run(&(0u32..1000).prop_map(|n| n * 2), |even| {
            prop_assert_eq!(even % 2, 0);
            Ok(())
        })
        .unwrap();
    runner(100)
        .run(&(any::<i32>(), any::<i32>()), |(a, b)| {
            prop_assume!(b != 0);
            prop_assert_eq!(
                a.wrapping_div(b)
                    .wrapping_mul(b)
                    .wrapping_add(a.wrapping_rem(b)),
                a
            );
            Ok(())
        })
        .unwrap();
}

/// SHRINKING: after the first failure proptest searches for a simpler input that still fails.
fn shrinking() {
    let result = runner(500).run(&runs(), |s| {
        prop_assert_eq!(buggy_decode(&rle_encode(&s)), s);
        Ok(())
    });
    match result {
        Err(TestError::Fail(_, minimal)) => {
            println!("buggy decoder: minimal counterexample {minimal:?}");
            assert_eq!(
                minimal.chars().count(),
                10,
                "the smallest failing string is one run of length 10"
            );
            assert_eq!(rle_encode(&minimal).len(), 1);
        }
        other => panic!("the bug must be found: {other:?}"),
    }
}

/// STATEFUL testing without an extra crate: generate a SEQUENCE of operations and compare against a trivial model after
/// each step. Shrinking then removes irrelevant operations. (proptest-state-machine adds preconditions and transitions.)
#[derive(Debug, Clone)]
enum Op {
    Push(u32),
    Pop,
}
fn stateful() {
    let op = prop_oneof![3 => (0u32..100).prop_map(Op::Push), 2 => Just(Op::Pop)];
    let strategy = (1usize..6, proptest::collection::vec(op, 0..60));
    runner(500)
        .run(&strategy, |(cap, ops)| {
            let (mut real, mut model) = (Ring::new(cap), VecDeque::new());
            for op in ops {
                match op {
                    Op::Push(v) => {
                        real.push(v);
                        model.push_back(v);
                        if model.len() > cap {
                            model.pop_front();
                        }
                    }
                    Op::Pop => prop_assert_eq!(real.pop(), model.pop_front()),
                }
                prop_assert_eq!(real.len(), model.len());
            }
            Ok(())
        })
        .unwrap();
}

// The macro form you use inside test modules. It expands to #[test] functions, so it only runs under `cargo test`.
#[cfg(test)]
proptest! {
    #[test]
    fn rle_never_grows_the_char_count(s in ".{0,60}") {
        prop_assert!(rle_encode(&s).len() <= s.chars().count());
    }
}

// ---------------- insta ----------------
fn render_table(rows: &[(&str, u32)]) -> String {
    let w = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
    rows.iter()
        .map(|(n, v)| format!("{n:<w$} | {v:>4}\n"))
        .collect()
}
fn snapshots() {
    // Inline snapshots keep the expected value next to the assertion; `cargo insta review` rewrites it in place.
    insta::assert_snapshot!(render_table(&[("apples", 3), ("kiwi", 1200)]), @r"
    apples |    3
    kiwi   | 1200
    ");
    insta::assert_debug_snapshot!(rle_encode("aaab"), @r"
    [
        (
            'a',
            3,
        ),
        (
            'b',
            1,
        ),
    ]
    ");
    // Make output deterministic BEFORE snapshotting: sort maps, replace timestamps and random ids with placeholders.
}

// ---------------- assert_cmd + predicates ----------------
fn cli_black_box() {
    // In a real tests/ file: Command::cargo_bin("your-binary") or env!("CARGO_BIN_EXE_your-binary").
    // An example has no binary of its own, so the subject here is the cargo that built it.
    Command::new(env!("CARGO"))
        .arg("--version")
        .timeout(std::time::Duration::from_secs(60))
        .assert()
        .success()
        .stdout(
            predicate::str::starts_with("cargo ")
                .and(predicate::str::is_match(r"\d+\.\d+\.\d+").unwrap()),
        )
        .stderr(predicate::str::is_empty());
    Command::new(env!("CARGO"))
        .arg("--definitely-not-a-flag")
        .assert()
        .failure()
        .code(predicate::ne(0))
        .stderr(predicate::str::contains("--definitely-not-a-flag"))
        .stdout(predicate::str::contains("cargo ").not());
    // Predicates are ordinary values.
    let p = predicate::str::contains("error").and(predicate::str::contains("warning").not());
    assert!(p.eval("error: x") && !p.eval("error and warning"));
}

// ---------------- fakes without a mocking library ----------------
/// Put the nondeterministic or external thing behind a small trait and pass it in.
trait Clock {
    fn now_secs(&self) -> u64;
}
trait Mailer {
    fn send(&self, to: &str, body: &str) -> Result<(), String>;
}
struct Reminder<'a> {
    clock: &'a dyn Clock,
    mailer: &'a dyn Mailer,
}
impl Reminder<'_> {
    /// Sends to everyone whose deadline has passed; returns who FAILED.
    fn run<'u>(&self, due: &[(&'u str, u64)]) -> Vec<&'u str> {
        due.iter()
            .filter(|(_, at)| *at <= self.clock.now_secs())
            .filter(|(who, at)| self.mailer.send(who, &format!("due at {at}")).is_err())
            .map(|(who, _)| *who)
            .collect()
    }
}
struct FixedClock(u64);
impl Clock for FixedClock {
    fn now_secs(&self) -> u64 {
        self.0
    }
}
#[derive(Default)]
struct FakeMailer {
    sent: RefCell<Vec<(String, String)>>,
    fail_for: Option<&'static str>,
}
impl Mailer for FakeMailer {
    fn send(&self, to: &str, body: &str) -> Result<(), String> {
        if self.fail_for == Some(to) {
            return Err("smtp down".into());
        }
        self.sent.borrow_mut().push((to.into(), body.into()));
        Ok(())
    }
}
fn fakes() {
    let mailer = FakeMailer {
        fail_for: Some("bob"),
        ..Default::default()
    };
    let service = Reminder {
        clock: &FixedClock(100),
        mailer: &mailer,
    };
    assert_eq!(
        service.run(&[("ann", 99), ("bob", 100), ("cy", 101), ("di", 1)]),
        ["bob"]
    );
    // Assert on observable effects (what was sent), not on a script of expected calls.
    assert_eq!(
        *mailer.sent.borrow(),
        [
            ("ann".to_string(), "due at 99".to_string()),
            ("di".to_string(), "due at 1".to_string())
        ]
    );
}

// ---------------- proptest: dependent values, rejection, how shrinking works ----------------
fn generator_design() {
    use proptest::sample::{Index, select};
    // prop_flat_map: the second strategy depends on the first value (here: a matrix with consistent dimensions).
    let matrix = (1usize..6, 1usize..6).prop_flat_map(|(r, c)| {
        (
            Just(c),
            proptest::collection::vec(proptest::collection::vec(0u8..10, c), r),
        )
    });
    runner(200)
        .run(&matrix, |(cols, m)| {
            prop_assert!(m.iter().all(|row| row.len() == cols));
            Ok(())
        })
        .unwrap();
    // sample::Index: "a position in a collection whose length I do not know yet". Always in range, shrinks toward 0.
    runner(200)
        .run(
            &(
                proptest::collection::vec(any::<u8>(), 1..20),
                any::<Index>(),
            ),
            |(v, i)| {
                prop_assert!(i.index(v.len()) < v.len());
                Ok(())
            },
        )
        .unwrap();
    // select picks from a fixed list; a regex literal is a String strategy.
    runner(200)
        .run(
            &(select(vec!["GET", "PUT"]), "/[a-z]{1,8}(/[0-9]{1,3})?"),
            |(method, path)| {
                prop_assert!(["GET", "PUT"].contains(&method));
                prop_assert!(path.starts_with('/') && path.len() <= 13, "{}", path);
                Ok(())
            },
        )
        .unwrap();
    // PITFALL: a filter that rejects nearly everything does not fail the property, it ABORTS the run.
    let mut strict = TestRunner::new_with_rng(
        Config {
            cases: 100,
            failure_persistence: None,
            max_local_rejects: 200,
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    let hopeless = any::<u32>().prop_filter("must be a multiple of 1000", |x| x % 1000 == 0);
    let outcome = strict.run(&hopeless, |_| Ok(()));
    assert!(matches!(outcome, Err(TestError::Abort(_))), "{outcome:?}");
    // The fix is to CONSTRUCT valid values.
    runner(100)
        .run(&(0u32..4_000_000).prop_map(|n| n * 1000), |x| {
            prop_assert_eq!(x % 1000, 0);
            Ok(())
        })
        .unwrap();
}

/// What the runner does when a case fails, written out with the ValueTree API: binary search over a tree of values.
fn shrinking_by_hand() {
    use proptest::strategy::ValueTree;
    let fails = |x: u32| x >= 1234;
    let mut r = runner(1);
    let strategy = 0u32..1_000_000;
    let mut tree = std::iter::repeat_with(|| strategy.new_tree(&mut r).unwrap())
        .find(|t| fails(t.current()))
        .unwrap();
    let mut smallest = tree.current();
    let mut steps = 0;
    // simplify() moves to a simpler value; if that one passes, complicate() backs off toward the last failure.
    let mut more = tree.simplify();
    while more {
        steps += 1;
        assert!(steps < 100, "binary search, not a linear walk");
        more = if fails(tree.current()) {
            smallest = smallest.min(tree.current());
            tree.simplify()
        } else {
            tree.complicate()
        };
    }
    assert_eq!(smallest, 1234, "the exact boundary");
}

/// REGRESSION FILES, shown for real in a temporary directory: a failure is saved and replayed FIRST on later runs.
fn regression_files() {
    use proptest::test_runner::FileFailurePersistence;
    let dir = tempfile::tempdir().unwrap();
    // The variant wants a &'static str; leaking one small path is fine in a test.
    let path: &'static str = Box::leak(
        dir.path()
            .join("seeds.txt")
            .to_str()
            .unwrap()
            .to_owned()
            .into_boxed_str(),
    );
    let with_file = |cases| {
        TestRunner::new_with_rng(
            // max_shrink_iters defaults to a multiple of `cases`, so set it: this runner is also used with cases = 0.
            Config {
                cases,
                max_shrink_iters: 10_000,
                failure_persistence: Some(Box::new(FileFailurePersistence::Direct(path))),
                ..Config::default()
            },
            TestRng::deterministic_rng(RngAlgorithm::ChaCha),
        )
    };
    let property = |x: u32| {
        prop_assert!(x < 1234, "too big");
        Ok(())
    };
    let first = with_file(500).run(&(0u32..1_000_000), property);
    assert!(matches!(first, Err(TestError::Fail(_, 1234))), "{first:?}");
    let saved = std::fs::read_to_string(path).unwrap();
    let seeds: Vec<&str> = saved.lines().filter(|l| l.starts_with("cc ")).collect();
    assert_eq!(seeds.len(), 1, "{saved}");
    // Zero new random cases: only the saved seed runs. It regenerates the ORIGINAL failing input and shrinks it again.
    let replay = with_file(0).run(&(0u32..1_000_000), property);
    assert!(
        matches!(replay, Err(TestError::Fail(_, 1234))),
        "{replay:?}"
    );
    // Once the bug is fixed, the replay passes. Keep the file anyway: it guards against the bug coming back.
    with_file(0).run(&(0u32..1_000_000), |_| Ok(())).unwrap();
    // The file stores a SEED, not the value: if the strategy changes, the seed may no longer produce the old input.
    // Copy important counterexamples into an ordinary example test as well.
    assert!(
        saved.contains("shrinks to"),
        "the value appears only in a comment: {saved}"
    );
}

// ---------------- insta settings ----------------
fn snapshot_filters() {
    // Redact unstable parts with regex filters (insta feature "filters") before comparing.
    let report = |id: u64, ms: u128| format!("job id={id:016x} finished in {ms}ms");
    let mut settings = insta::Settings::new();
    settings.add_filter(r"id=[0-9a-f]{16}", "id=[ID]");
    settings.add_filter(r"\d+ms", "[DURATION]");
    settings.set_description("ids and timings differ on every run");
    // Settings apply only inside bind(), so they cannot leak into other tests.
    // PITFALL: insta panics if the same inline assertion runs twice (a loop), unless wrapped in allow_duplicates!.
    insta::allow_duplicates! {
        for (id, ms) in [(1, 3), (u64::MAX, 4_000_000)] {
            settings.bind(|| insta::assert_snapshot!(report(id, ms), @"job id=[ID] finished in [DURATION]"));
        }
    }
    // A mismatching snapshot is not demonstrated: besides panicking, insta may write a pending-snapshot file next to
    // the source (depending on INSTA_UPDATE and CI), and a cookbook run must not modify its own sources.
}

// ---------------- more assert_cmd and predicates ----------------
#[cfg(unix)]
fn cli_stdin_env_cwd() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("data.txt"), "from-a-file\n").unwrap();
    // The shell stands in for "your binary": it reads stdin, an env var and a file relative to the working directory.
    let script = r#"read line; echo "stdin=$line mode=$MODE home=${HOME:-unset}"; cat data.txt; echo copy > out.txt; exit 3"#;
    let assert = Command::new("/bin/sh")
        .args(["-c", script])
        .env_clear() // nothing inherited: the test cannot depend on the developer's shell
        .env("PATH", "/usr/bin:/bin")
        .env("MODE", "test")
        .current_dir(dir.path())
        .write_stdin("hello\nignored\n")
        .timeout(std::time::Duration::from_secs(60))
        .assert()
        .code(3);
    assert
        .stdout("stdin=hello mode=test home=unset\nfrom-a-file\n")
        .stderr("");
    // Check effects, not only output.
    use predicate::path;
    assert!(
        path::exists()
            .and(path::is_file())
            .eval(&dir.path().join("out.txt"))
    );
    assert!(path::is_dir().eval(dir.path()) && path::missing().eval(&dir.path().join("nope")));
    assert!(path::eq_file(dir.path().join("data.txt")).eval(dir.path().join("data.txt").as_path()));
    // output() gives the raw bytes when you want ordinary assertions, and does not fail on a non-zero status.
    let out = Command::new("/bin/sh")
        .args(["-c", "printf abc; exit 1"])
        .output()
        .unwrap();
    assert_eq!(
        (out.status.code(), out.stdout.as_slice()),
        (Some(1), &b"abc"[..])
    );
}
#[cfg(not(unix))]
fn cli_stdin_env_cwd() {}

fn predicate_values() {
    assert!(predicate::eq(5).eval(&5) && predicate::lt(10).and(predicate::ge(0)).eval(&3));
    assert!(
        predicate::in_iter(["a", "b"]).eval(&"b") && !predicate::in_iter(["a", "b"]).eval(&"c")
    );
    // Floating point: never eq.
    assert_ne!(0.1 + 0.2, 0.3);
    assert!(predicate::float::is_close(0.3).eval(&(0.1 + 0.2)));
    assert!(!predicate::float::is_close(0.3).eval(&0.3001));
    let even = predicate::function(|x: &u32| x % 2 == 0);
    assert!(even.eval(&4) && !even.eval(&5));
    // The advantage over a bare bool: a predicate can explain which part failed.
    let p = predicate::str::contains("alpha").and(predicate::str::contains("beta"));
    let why = p
        .find_case(false, "alpha only")
        .expect("a case explaining the false result");
    let mut shown = vec![];
    fn walk(c: &predicates::reflection::Case<'_>, out: &mut Vec<String>) {
        out.extend(c.predicate().map(|p| p.to_string()));
        c.children().for_each(|k| walk(k, out));
    }
    walk(&why, &mut shown);
    assert!(shown.iter().any(|s| s.contains("beta")), "{shown:?}");
    assert!(p.find_case(false, "alpha beta").is_none());
}

pub fn main() {
    properties();
    shrinking();
    stateful();
    snapshots();
    cli_black_box();
    fakes();
    generator_design();
    shrinking_by_hand();
    regression_files();
    snapshot_filters();
    cli_stdin_env_cwd();
    predicate_values();
    println!("testing tools: all sections passed");
}
