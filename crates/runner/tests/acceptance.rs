//! End-to-end runs against real problems: the fixtures in `crates/content/fixtures/runnable` (a slice of the old D9 Graphs track and
//! one L2 Borrowing problem).
//!
//! Host tests run with plain `cargo test`. The Docker test needs the sandbox image
//! (`docker build -t anneal-runner:1.98 -f docker/runner.Dockerfile docker`) and runs
//! with `cargo test -p anneal-runner -- --ignored`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anneal_content::{Catalog, Problem};
use anneal_runner::{
    Outcome, Perf, RunResult, RunStatus, Runner, RunnerConfig, Sandbox, Submission, Suite,
};

/// The workspace from the WorkspaceSplit design: the `u32::MAX` sentinel leaks out.
const NETWORK_DELAY_SENTINEL_BUG: &str = r#"use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub fn network_delay(times: &[(usize, usize, u32)], n: usize, k: usize) -> Option<u32> {
    let mut adj = vec![Vec::new(); n + 1];
    for &(u, v, w) in times {
        adj[u].push((v, w));
    }

    let mut dist = vec![u32::MAX; n + 1];
    let mut heap = BinaryHeap::new();
    dist[k] = 0;
    heap.push(Reverse((0, k)));

    while let Some(Reverse((d, u))) = heap.pop() {
        if d > dist[u] { continue; }
        for &(v, w) in &adj[u] {
            let nd = d + w;
            if nd < dist[v] {
                dist[v] = nd;
                heap.push(Reverse((nd, v)));
            }
        }
    }

    dist[1..].iter().copied().max()
}
"#;

fn content_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../content/fixtures/runnable")
}

fn problem(catalog: &Catalog, id: &str) -> (String, String, String, String) {
    let (_, p): (_, &Problem) = catalog
        .problem(id)
        .unwrap_or_else(|| panic!("no problem {id}"));
    let f = &p.files;
    (
        f.starter.clone().expect("starter"),
        f.solution.clone().expect("solution"),
        f.visible_tests.clone().expect("visible tests"),
        f.hidden_tests.clone().expect("hidden tests"),
    )
}

fn runner(sandbox: Sandbox) -> Runner {
    let mut cfg = RunnerConfig::new(
        sandbox,
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("anneal"),
    );
    cfg.test_timeout = Duration::from_secs(20);
    Runner::new(cfg)
}

fn catalog() -> Catalog {
    let loaded = Catalog::load(&content_root()).expect("content loads");
    assert!(
        loaded.issues.is_empty(),
        "content issues: {:?}",
        loaded.issues
    );
    loaded.catalog
}

async fn submit(r: &Runner, id: &str, lib_rs: &str, visible: &str, hidden: &str) -> RunResult {
    r.run(
        id,
        &Submission {
            lib_rs,
            visible_tests: visible,
            hidden_tests: Some(hidden),
        crates: &[], perf: Perf::default(),
        },
    )
    .await
    .expect("runner")
}

fn failing(r: &RunResult) -> Vec<(Suite, &str)> {
    r.tests
        .iter()
        .filter(|t| t.outcome != Outcome::Passed)
        .map(|t| (t.suite, t.name.as_str()))
        .collect()
}

#[tokio::test]
async fn network_delay_sentinel_bug_scores_9_of_14() {
    let c = catalog();
    let (_, _, visible, hidden) = problem(&c, "d9-network-delay-time");
    let r = submit(
        &runner(Sandbox::Host),
        "d9-network-delay-time",
        NETWORK_DELAY_SENTINEL_BUG,
        &visible,
        &hidden,
    )
    .await;

    assert_eq!(r.status, RunStatus::Failed);
    assert_eq!((r.passed, r.total), (9, 14));
    assert_eq!(
        failing(&r),
        vec![
            (Suite::Hidden, "dense_n100"),
            (Suite::Hidden, "edges_are_directed"),
            (Suite::Hidden, "only_source_reachable"),
            (Suite::Hidden, "random_vs_bellman_ford"),
            (Suite::Visible, "unreachable_returns_none")
        ]
    );
    let unreachable = r
        .tests
        .iter()
        .find(|t| t.name == "unreachable_returns_none")
        .unwrap();
    let check = unreachable.check.as_ref().expect("check! report");
    assert_eq!(
        (check.expected.as_str(), check.got.as_str()),
        ("None", "Some(4294967295)")
    );
    assert_eq!(check.input, "times = [(1,2,1)], n = 3, k = 1");
}

#[tokio::test]
async fn network_delay_reference_passes_14_of_14() {
    let c = catalog();
    let (_, solution, visible, hidden) = problem(&c, "d9-network-delay-time");
    let r = submit(
        &runner(Sandbox::Host),
        "d9-network-delay-time",
        &solution,
        &visible,
        &hidden,
    )
    .await;
    assert_eq!(r.status, RunStatus::Passed, "{r:#?}");
    assert_eq!((r.passed, r.total), (14, 14));
}

#[tokio::test]
async fn run_without_hidden_tests_reports_visible_only() {
    let c = catalog();
    let (_, _, visible, _) = problem(&c, "d9-network-delay-time");
    let sub = Submission {
        lib_rs: NETWORK_DELAY_SENTINEL_BUG,
        visible_tests: &visible,
        hidden_tests: None,
        crates: &[], perf: Perf::default(),
    };
    let r = runner(Sandbox::Host)
        .run("d9-network-delay-time", &sub)
        .await
        .unwrap();
    assert_eq!((r.passed, r.total), (4, 5));
    assert!(r.tests.iter().all(|t| t.suite == Suite::Visible));
}

#[tokio::test]
async fn fix_this_starter_reports_e0499_with_both_borrows() {
    let c = catalog();
    let (starter, _, visible, hidden) = problem(&c, "l2-two-mutable-borrows-of-self");
    let r = submit(
        &runner(Sandbox::Host),
        "l2-two-mutable-borrows-of-self",
        &starter,
        &visible,
        &hidden,
    )
    .await;

    assert_eq!(r.status, RunStatus::CompileError);
    assert!(r.tests.is_empty());
    let e = r
        .diagnostics
        .iter()
        .find(|d| d.code.as_deref() == Some("E0499"))
        .expect("E0499");
    // Borrow lanes are drawn from these: the loop's borrow on line 13, the conflict on line 15.
    let lines: Vec<(u32, bool)> = e.spans.iter().map(|s| (s.line_start, s.primary)).collect();
    assert!(lines.contains(&(15, true)), "{lines:?}");
    assert!(lines.contains(&(13, false)), "{lines:?}");
}

#[tokio::test]
async fn fix_this_solution_passes_visible_and_hidden() {
    let c = catalog();
    let (_, solution, visible, hidden) = problem(&c, "l2-two-mutable-borrows-of-self");
    let r = submit(
        &runner(Sandbox::Host),
        "l2-two-mutable-borrows-of-self",
        &solution,
        &visible,
        &hidden,
    )
    .await;
    assert_eq!(r.status, RunStatus::Passed, "{r:#?}");
    assert_eq!((r.passed, r.total), (13, 13));
}

#[tokio::test]
async fn prints_from_passing_tests_are_kept() {
    let catalog = catalog();
    let (_, solution, visible, _) = problem(&catalog, "d9-network-delay-time");
    let noisy = solution.replacen(
        "    let max = ",
        "    println!(\"dist = {:?}\", &dist[1..]);\n    dbg!(k);\n    let max = ",
        1,
    );
    let r = runner(Sandbox::Host);
    let sub = Submission { lib_rs: &noisy, visible_tests: &visible, hidden_tests: None, crates: &[], perf: Perf::default() };
    let result = r.run("prints", &sub).await.unwrap();
    assert_eq!(result.status, RunStatus::Passed, "{result:#?}");
    let out: Vec<&str> = result.tests.iter().map(|t| t.stdout.as_str()).collect();
    assert!(out.iter().any(|s| s.contains("dist = [") && s.contains("k = ")), "{out:?}");
}

#[tokio::test]
async fn prelude_rng_is_seeded_and_in_range() {
    let tests = r#"use solution::*;

#[test]
fn seeded() {
    let (mut a, mut b) = (anneal_prelude::Rng::new(42), anneal_prelude::Rng::new(42));
    let xs: Vec<u64> = (0..100).map(|_| a.next_u64()).collect();
    let ys: Vec<u64> = (0..100).map(|_| b.next_u64()).collect();
    assert_eq!(xs, ys);
    // splitmix64's first output for seed 0.
    assert_eq!(anneal_prelude::Rng::new(0).next_u64(), 0xE220_A839_7B1D_CDAF);
}

#[test]
fn ranges() {
    let mut r = anneal_prelude::Rng::new(7);
    for _ in 0..10_000 {
        assert!((-3..=3).contains(&r.int(-3, 3)));
        assert!(r.below(5) < 5);
    }
    let v: Vec<u8> = r.vec(50, 0, 255);
    assert_eq!(v.len(), 50);
    let mut p: Vec<u32> = (0..20).collect();
    r.shuffle(&mut p);
    p.sort();
    assert_eq!(p, (0..20).collect::<Vec<_>>());
    assert!(r.string(30, "ab").chars().all(|c| c == 'a' || c == 'b'));
    check!("add(2, 3)", add(2, 3), 5);
}
"#;
    let r = runner(Sandbox::Host);
    let sub = Submission { lib_rs: "pub fn add(a: i32, b: i32) -> i32 { a + b }", visible_tests: tests, hidden_tests: None, crates: &[], perf: Perf::default() };
    let result = r.run("rng", &sub).await.unwrap();
    assert_eq!(result.status, RunStatus::Passed, "{result:#?}");
    assert_eq!(result.passed, 2);
}

const PERF_LIB: &str = r#"
pub fn total(xs: &[u32]) -> u32 {
    xs.iter().fold(0, |a, &x| a.wrapping_add(x))
}

pub fn odd_slots(xs: &[u32], n: usize) -> u32 {
    let mut s = 0u32;
    for i in 0..n {
        s = s.wrapping_add(xs[i * 2 + 1]);
    }
    s
}

pub struct Grid(pub Vec<u32>);

impl Grid {
    pub fn row_sum(&self, w: usize, r: usize) -> u32 {
        self.0[r * w..(r + 1) * w].iter().sum()
    }
}

pub fn squares(n: u32) -> Vec<u32> {
    (0..n).map(|i| i * i).collect()
}
"#;

/// `[perf]`: release build, the library's assembly (both manglings, methods, small functions rustc would
/// otherwise leave to callers), the counting allocator and relative timing.
#[tokio::test]
async fn perf_helpers_measure_the_release_build() {
    let tests = r#"use solution::*;

#[test]
fn assembly() {
    let fns = anneal_prelude::asm::functions();
    let total = anneal_prelude::asm::function("total");
    assert!(!anneal_prelude::asm::instructions(&total).is_empty(), "{fns:?}");
    assert!(!anneal_prelude::asm::calls(&total, "panic_bounds_check"));
    assert!(anneal_prelude::asm::calls(&anneal_prelude::asm::function("odd_slots"), "panic_bounds_check"));
    assert!(!anneal_prelude::asm::function("Grid::row_sum").is_empty(), "{fns:?}");
}

#[test]
#[should_panic(expected = "no function \"missing\"")]
fn unknown_function() {
    anneal_prelude::asm::function("missing");
}

#[test]
fn allocations() {
    let (v, a) = anneal_prelude::allocs(|| squares(1000));
    assert_eq!(v.len(), 1000);
    assert_eq!(a.count, 1);
    assert_eq!(a.bytes, 4000);
    let ((), none) = anneal_prelude::allocs(|| ());
    assert_eq!(none.count, 0);
}

#[test]
fn timing() {
    let xs: Vec<u32> = (0..200_000).collect();
    let slow = |xs: &[u32]| xs.iter().map(|&x| std::hint::black_box(x)).fold(0u32, |a, x| a.wrapping_add(x));
    anneal_prelude::assert_faster("sum", 1.0, 5, || slow(&xs), || total(&xs));
    assert!(anneal_prelude::median_time(3, || total(&xs)) < std::time::Duration::from_secs(1));
}
"#;
    let r = runner(Sandbox::Host);
    let perf = Perf { release: true, asm: true, count_allocs: true };
    let sub = Submission { lib_rs: PERF_LIB, visible_tests: tests, hidden_tests: None, crates: &[], perf };
    let result = r.run("perf", &sub).await.unwrap();
    assert_eq!(result.status, RunStatus::Passed, "{result:#?}");
    assert_eq!(result.passed, 4);
}

/// Timing a debug build would pass or fail at random, so `assert_faster` refuses to.
#[tokio::test]
async fn assert_faster_refuses_debug_builds() {
    let tests = r#"#[test]
fn timing() {
    anneal_prelude::assert_faster("noop", 1.0, 3, || 1, || 1);
}
"#;
    let r = runner(Sandbox::Host);
    let sub = Submission { lib_rs: "", visible_tests: tests, hidden_tests: None, crates: &[], perf: Perf::default() };
    let result = r.run("perf-debug", &sub).await.unwrap();
    assert_eq!(result.status, RunStatus::Failed, "{result:#?}");
    assert!(result.tests[0].panic.as_deref().unwrap_or("").contains("needs [perf] release = true"), "{result:#?}");
}

#[tokio::test]
async fn infinite_loop_times_out_and_is_killed() {
    let c = catalog();
    let (_, _, visible, _) = problem(&c, "d9-network-delay-time");
    let spin = "pub fn network_delay(_: &[(usize, usize, u32)], _: usize, _: usize) -> Option<u32> { loop { std::hint::spin_loop() } }";
    let mut cfg = RunnerConfig::new(
        Sandbox::Host,
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("anneal"),
    );
    cfg.test_timeout = Duration::from_secs(3);
    let r = Runner::new(cfg)
        .run(
            "d9-network-delay-time-spin",
            &Submission {
                lib_rs: spin,
                visible_tests: &visible,
                hidden_tests: None,
        crates: &[], perf: Perf::default(),
            },
        )
        .await
        .unwrap();
    assert_eq!(r.status, RunStatus::Timeout);
    assert!(
        r.tests.iter().any(|t| t.outcome == Outcome::TimedOut),
        "{r:#?}"
    );
}

#[tokio::test]
#[ignore = "needs Docker and the anneal-runner:1.98 image"]
async fn docker_sandbox_matches_host() {
    let c = catalog();
    let (_, solution, visible, hidden) = problem(&c, "d9-network-delay-time");
    let docker = runner(Sandbox::docker("anneal-runner:1.98"));
    let bug = submit(
        &docker,
        "d9-network-delay-time",
        NETWORK_DELAY_SENTINEL_BUG,
        &visible,
        &hidden,
    )
    .await;
    assert_eq!(
        (bug.status, bug.passed, bug.total),
        (RunStatus::Failed, 9, 14),
        "{bug:#?}"
    );
    let ok = submit(
        &docker,
        "d9-network-delay-time",
        &solution,
        &visible,
        &hidden,
    )
    .await;
    assert_eq!(
        (ok.status, ok.passed, ok.total),
        (RunStatus::Passed, 14, 14),
        "{ok:#?}"
    );
}

const CRATES_LIB: &str = r#"
/// Reads `n` from a JSON object.
pub fn parse(s: &str) -> anyhow::Result<u64> {
    let v: serde_json::Value = serde_json::from_str(s)?;
    v["n"].as_u64().ok_or_else(|| anyhow::anyhow!("no n"))
}

/// Doubles `x` after an hour (of paused test time).
pub async fn double_later(x: u64) -> u64 {
    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    x * 2
}
"#;

const CRATES_TESTS: &str = r#"use solution::*;

#[test]
fn parses_json() {
    check!("{\"n\": 4}", parse("{\"n\": 4}").unwrap(), 4);
}

#[tokio::test(start_paused = true)]
async fn paused_time() {
    check!("3", double_later(3).await, 6);
}
"#;

fn crate_set() -> Vec<String> {
    ["serde_json", "anyhow", "tokio"].map(String::from).to_vec()
}

async fn run_with_crates(sandbox: Sandbox) -> RunResult {
    let crates = crate_set();
    let sub = Submission { lib_rs: CRATES_LIB, visible_tests: CRATES_TESTS, hidden_tests: None, crates: &crates, perf: Perf::default() };
    runner(sandbox).run("crates-demo", &sub).await.unwrap()
}

#[tokio::test]
async fn problems_can_use_crates_from_the_set() {
    let r = run_with_crates(Sandbox::Host).await;
    assert_eq!((r.status, r.passed, r.total), (RunStatus::Passed, 2, 2), "{r:#?}");
}

#[tokio::test]
async fn crates_outside_the_set_are_refused() {
    let crates = vec!["left-pad".to_string()];
    let sub = Submission { lib_rs: "", visible_tests: "", hidden_tests: None, crates: &crates, perf: Perf::default() };
    let err = runner(Sandbox::Host).run("crates-unknown", &sub).await.unwrap_err();
    assert!(matches!(err, anneal_runner::RunnerError::UnknownCrate(ref c) if c == "left-pad"), "{err}");
}

#[tokio::test]
#[ignore = "needs the runner image: docker build -t anneal-runner:1.98 -f docker/runner.Dockerfile docker"]
async fn docker_sandbox_uses_vendored_crates_offline() {
    let r = run_with_crates(Sandbox::docker("anneal-runner:1.98")).await;
    assert_eq!((r.status, r.passed, r.total), (RunStatus::Passed, 2, 2), "{r:#?}");
}

#[tokio::test]
async fn scratch_main_runs_against_the_library() {
    use anneal_runner::{Scratch, ScratchStatus};
    let r = runner(Sandbox::Host);
    // An unused parameter gives a compiler warning, which must not end up in the program's stderr.
    let lib = "pub fn double(x: i32) -> i32 { x * 2 }\npub fn unused(y: i32) {}\n";
    let ok = Scratch { lib_rs: lib, main_rs: "use solution::*;\nfn main() { println!(\"{}\", double(21)); eprintln!(\"to stderr\"); }\n", crates: &[] };
    let out = r.run_scratch("scratch-demo", &ok).await.unwrap();
    assert_eq!((out.status, out.stdout.as_str(), out.stderr.trim(), out.exit_code), (ScratchStatus::Ok, "42\n", "to stderr", Some(0)), "{out:#?}");

    let panics = Scratch { main_rs: "fn main() { let v: Vec<i32> = vec![]; println!(\"before\"); v[3]; }\n", ..ok };
    let out = r.run_scratch("scratch-demo", &panics).await.unwrap();
    assert_eq!((out.status, out.stdout.as_str()), (ScratchStatus::Exited, "before\n"));
    assert!(out.stderr.contains("index out of bounds"), "{}", out.stderr);

    let broken = Scratch { main_rs: "use solution::*;\nfn main() { let s: String = double(1); }\n", ..ok };
    let out = r.run_scratch("scratch-demo", &broken).await.unwrap();
    assert_eq!(out.status, ScratchStatus::CompileError);
    assert!(out.diagnostics.iter().any(|d| d.code.as_deref() == Some("E0308")), "{:?}", out.diagnostics);
}

/// The runner asks cargo and rustc for colour, and keeps the learner's: everything it reads from that text still reads.
#[tokio::test]
async fn coloured_output_is_kept_and_still_parsed() {
    use anneal_runner::strip_ansi;
    let sub = Submission {
        lib_rs: "pub fn f(v: &mut Vec<i32>) -> i32 { let a = &v[0]; v.push(1); *a }\n",
        visible_tests: "use solution::*;\n#[test]\nfn t() { f(&mut vec![1]); }\n",
        hidden_tests: None,
        crates: &[],
        perf: Perf::default(),
    };
    let r = runner(Sandbox::Host).run("colour-demo", &sub).await.unwrap();
    assert_eq!(r.status, RunStatus::CompileError, "{r:#?}");
    let d = r.diagnostics.iter().find(|d| d.code.as_deref() == Some("E0502")).expect("a borrow error");
    assert!(d.rendered.contains('\x1b') && !d.message.contains('\x1b'), "{d:?}");
    assert!(strip_ansi(&d.rendered).starts_with("error[E0502]: cannot borrow"), "{}", d.rendered);

    // A print that leaves its colour open and has no newline sits in front of the check! report.
    let sub = Submission {
        lib_rs: "pub fn two() -> i32 { print!(\"\\x1b[31m\"); 3 }\n",
        visible_tests: "use solution::*;\n#[test]\nfn visible() { check!(\"two()\", two(), 2); }\n",
        hidden_tests: Some("use solution::*;\n#[test]\nfn secret() { let n = two(); assert_eq!(n, 2, \"\\x1b[33mwrong\"); }\n"),
        ..sub
    };
    let r = runner(Sandbox::Host).run("colour-demo", &sub).await.unwrap();
    assert_eq!((r.status, r.total), (RunStatus::Failed, 2), "{r:#?}");
    let test = |name: &str| r.tests.iter().find(|t| t.name == name).unwrap();
    assert_eq!((test("visible").suite, test("secret").suite), (Suite::Visible, Suite::Hidden), "cargo's coloured Running lines still name the suites");
    assert_eq!(test("visible").check.as_ref().map(|c| c.got.as_str()), Some("3"), "{r:#?}");
    assert!(test("secret").panic.as_deref().is_some_and(|p| p.starts_with("assertion `left == right` failed: \x1b[33mwrong")), "{r:#?}");
    assert_eq!(test("secret").stdout, "\x1b[31m");
}
