# Practice: dev_tools

**How to use this page.** Run each recipe, then do **Prove you got it** by injecting a bug on purpose and predicting which tool notices it first (unit test, property, snapshot, fuzz, loom, compiler).
Tooling is learned by catching real bugs: for every [M] and [A] task you must first write the bug, then show the tool finding it. Finish with a Chapter project.

---

## clap_cli
**Prove you got it**
1. `main` runs six argument vectors through `Cli::try_parse_from`. Predict which two print `OK` with `verbose: true` and which four-way error cases appear: `--minutes 0` (range 1..=600), a missing `path`, and `frobnicate`. Write the first stderr line clap prints for each, then run.
2. `-v` is `global = true`. Predict whether `["recipebook","-v","list"]` and `["recipebook","list","-v"]` both parse with `verbose: true`; then remove `global = true` and predict which one fails.

**Try next**
- [S] Add `--minutes` upper-bound error text check: assert `try_parse_from([... "--minutes","601"])` is an `Err` whose `kind()` is `ErrorKind::ValueValidation`.
- [M] Add a `Remove { name: String, #[arg(long)] force: bool }` subcommand and make `--force` conflict with `--dry-run` (`conflicts_with`); table-test 5 argument vectors and assert the `ErrorKind::ArgumentConflict` case.
- [A] Build a real tool (`todo add/list/done/rm`) with a JSON store written atomically (`language/io_files_paths`), exit codes 0 ok / 1 runtime error / 2 usage, and black-box tests using `env!("CARGO_BIN_EXE_todo")` that assert stdout, stderr and the exit code for every subcommand.

**Use it in:** any multi-command CLI. Real: clap itself <https://github.com/clap-rs/clap> and ripgrep's flag handling <https://github.com/BurntSushi/ripgrep>.

**Pitfall:** doing the work inside parsing or calling `std::process::exit` mid-function; keep `main` thin (`errors/error_enum`) so everything stays testable.

---

## shrinking
**Prove you got it**
1. `buggy_sort` sorts then `dedup`s, and the property is "length is unchanged". Predict the minimal counterexample length (2 elements, two equal values such as `[0, 0]` or `[-1, -1]`) and confirm `assert!(small.len() <= 2)` passes. Then change the bug to `out.truncate(5)` and predict the minimal length (6 elements) and values (all shrunk toward 0).
2. The shrinker tries removal first, then halving values. Swap their order in `shrink` and predict whether the final result changes (same length, maybe different values); print both.

**Try next**
- [S] Change the generator range from `-5..=5` to `-1000..=1000` and predict how the shrunk values change (they halve to the smallest failing duplicates, e.g. `[0,0]`).
- [M] Add a second property `sorted_output_is_sorted` and a second bug (`out.sort_by(|a,b| b.cmp(a))`); make the runner report which property failed and shrink with respect to that property only (the recipe's shrinker is tied to `holds`; parameterise it). Assert the shrunk input still fails the same property.
- [A] Write `check(prop, gen, shrink, cases, seed)` that prints the failing seed and case index so a failure replays exactly; plant three bugs in a `MyMap` wrapper you write (loses a key after resize, double-counts `len` on overwrite, `remove` returns the wrong value) and show each is found and minimised to fewer than 8 operations. Compare with <https://github.com/proptest-rs/proptest> afterwards.

**Use it in:** any invariant test suite. Real: <https://github.com/BurntSushi/quickcheck> and proptest do this for you.

**Pitfall:** a shrinker that accepts any failure rather than the same one, so the counterexample drifts to a different bug.

---

## testing_patterns
**Prove you got it**
1. Run `cargo test --release --example testing_patterns -- --list`. Predict the number of tests (about 8) and read their names; then predict which one fails if you change `slugify`'s `.trim()` away (the `"  spaced   out  "` row of `slugify_cases`).
2. `expiry_uses_injected_clock` asserts `!is_expired(&Fixed(9), 10)` and `is_expired(&Fixed(10), 10)`. Predict which assertion flips if you change `>=` to `>` in `is_expired`, then check.

**Try next**
- [S] Add 3 rows to `slugify_cases` (unicode `"Crème Brûlée"`, digits-only, a string of 100 dashes) and decide what `slugify` should do for the unicode case; make the failure message print the input as the existing ones do.
- [M] Add a test that `Percent::new(100)` succeeds and `Percent::new(101)` panics with the exact message `"percent out of range: 101"` using `#[should_panic(expected = ...)]`; then convert `Percent::new` to return `Result` and rewrite the tests with `?` and `matches!`.
- [A] Write tests for a function that needs a clock and a fetcher (`fn refresh_if_stale(clock: &impl Clock, fetch: &mut impl FnMut() -> String, ...)`); acceptance: mutate three operators in the function by hand (`>=`->`>`, `+`->`-`, `&&`->`||`) and show each mutation is killed by at least one test. Later run `cargo mutants` to compare.

**Use it in:** every codebase; unit tests next to code, black-box tests in `tests/`. See `crates/cb-testing-tools` for the wider toolbox.

**Pitfall:** tests that depend on `HashMap` iteration order or the wall clock.

---

## tracing_logs
**Prove you got it**
1. `process(1, b"hello")` and `process(2, b"")` are instrumented with `skip(payload), fields(bytes = payload.len())`. Predict which lines show `bytes=5` and `bytes=0`, whether the `debug!("starting")` line appears (subscriber max level is DEBUG: yes), and the level of the `job failed` line (ERROR).
2. The `warn!` for queue depth is emitted inside the `batch` span. Predict whether the line carries `batch_id=99` and whether the `info!(n, "done")` line after `drop(_enter)` still does (it does not).

**Try next**
- [S] Change `with_max_level(Level::DEBUG)` to `INFO` and predict which lines vanish (the two `debug!`-level lines from `process`); run and check the count.
- [M] Add a custom `tracing_subscriber::Layer` that counts `ERROR` events and assert after running `process(2, b"")` that the count is 1 (use `tracing::subscriber::with_default` so the test does not depend on a global subscriber).
- [A] Instrument a small worker pool (`async/worker_pool`-style) so every job has `job_id`, `attempt` and outcome fields on one span; switch the subscriber to JSON output and write a script (your own, Rust or shell) that reads the log and reports any job that has a `started` event with no `finished` event; inject one such job to prove it is found.

**Use it in:** production services answering "what happened to job X?". Real: <https://github.com/tokio-rs/tracing>.

**Pitfall:** interpolating values into the message string instead of using structured fields.

---

## cb-testing-tools (crate: `crates/cb-testing-tools`)
**Prove you got it**
1. `tests/fuzz_style.rs` plants a bug: `buggy_target` crashes when two open parens are adjacent, and asserts the minimiser returns exactly `b"(("`. Predict the minimised output if the bug instead required a `(` followed by `)` anywhere later (`"()"` is the reduced reproducer or a longer one?), then change the target and run.
2. `tests/loom_model.rs` asserts the spinlock with `Acquire`/`Release` passes loom and the `Relaxed` version is caught; `handoff_relaxed_fails` also asserts that `Release` store with `Relaxed` load still fails. Predict which of the four (Release/Acquire, Release/Relaxed, Relaxed/Acquire, Relaxed/Relaxed) loom accepts (only the first), then add the other cases yourself.

**Try next**
- [S] Add a new file in `tests/corpus/` with deeply nested parentheses (depth 100) and check that `fuzz_target` rejects it with `MAX_DEPTH` (64) instead of overflowing the stack; the assertion lives in `fuzz_target_handles_adversarial_shapes`.
- [M] Add a proptest law to `tests/proptest_properties.rs`: for `parse_duration` / `format_duration`, `format_duration(parse_duration(s)) == format_duration(parse_duration(format_duration(...)))` (canonical-form idempotence). Introduce a bug in `format_duration` for the exactly-zero case and let proptest shrink it; record the minimal counterexample.
- [A] Write a bounded semaphore in `src/counter.rs` style (the existing `Bounded` is the template) and a loom model with 3 threads and 2 permits asserting that at most 2 threads are in the critical section; then weaken one ordering to `Relaxed` and show loom failing it (`RUSTFLAGS="--cfg loom"`).

**Use it in:** a library with parsers and lock-free counters. Real: insta <https://github.com/mitsuhiko/insta> and loom <https://github.com/tokio-rs/loom>; cargo-fuzz <https://github.com/rust-fuzz/cargo-fuzz>.

**Pitfall:** accepting an `insta` snapshot without reading the diff; a snapshot test then only proves "the output is whatever it is".

---

## cb-macros and cb-macros-demo (crates: `crates/cb-macros`, `crates/cb-macros-demo`)
**Prove you got it**
1. `tests/behaviour.rs` asserts `Point::describe() == "struct Point { x: i32, y: Vec<u8> }"` and `Wrap::<u8>::describe() == "struct Wrap(T)"`. Predict what `describe` returns for a unit struct `struct Unit;` and for an enum variant with fields, then add them and check the exact strings.
2. `fib(30)` with `#[memoize]` calls the body 31 times (`CALLS == 31`) and the second `fib(30)` adds 0. Predict the count if the memo table were `thread_local` per thread and you call `fib(30)` from two threads; test by spawning.

**Try next**
- [S] Add a derive `Name` that implements `fn type_name() -> &'static str` for structs and enums (generics included); assert `Wrap::<u8>::type_name() == "Wrap"`.
- [M] Add attribute `#[retry(3)]` for functions returning `Result<T, E>`; failure mode: applied to a function that returns `i32` must produce a spanned compile error (add a `tests/ui/retry_non_result.rs` with a `.stderr` snapshot via trybuild). Verify the behaviour test: a fn failing twice then succeeding runs exactly 3 times.
- [A] Write `#[derive(FromEnv)]` that reads each field from an environment variable named after the field (uppercase), with `Option<T>` optional and `#[env(default = "...")]` defaults; acceptance: behaviour tests plus three trybuild failures (tuple struct, unknown attribute argument, unsupported field type).

**Use it in:** removing boilerplate in a CLI or service (builders, config loading). Real: <https://github.com/dtolnay/syn> and <https://github.com/dtolnay/trybuild>.

**Pitfall:** panicking inside a macro instead of returning `syn::Error::to_compile_error()` with a span, which yields unreadable errors for users.

---

## cb-features-workspace (crate: `crates/cb-features-workspace`)
**Prove you got it**
1. `tests/feature_tests.rs::modules_match_features` asserts `f.contains(&"json") == cfg!(feature = "json")`. Predict whether `cargo test --no-default-features` still compiles the test file (it does; `cfg!` is just a bool, but the `json::to_json` call must itself be gated) and which line would fail to compile if it were not gated.
2. The README says mutually exclusive features break `--all-features`. Predict what `backend()` returns when both backend features are on, and why precedence (rather than `compile_error!`) keeps `cargo test --all-features` green.

**Try next**
- [S] Add a feature `stats` method `median` that is only present with the feature; add a test using `#[cfg(feature = "stats")]` and run `cargo test --no-default-features --features stats` and without it; record both outputs.
- [M] Add an optional dependency feature using `dep:` syntax and prove the implicit feature name is not exposed: `cargo metadata --format-version 1 | grep -c '"name":"<dep>"'`-style check or a failed `--features <dep>`; keep the error text.
- [A] Extend `tests/driver.rs` to run `cargo test` for every subset of 4 features (16 builds) via `std::process::Command`, and plant a non-additive feature (one that removes an existing function) to show the driver catches it; the acceptance is a failing driver run that names the bad subset.

**Use it in:** libraries with optional heavy dependencies (`serde`, `tokio`). Real: <https://github.com/tokio-rs/tokio> has dozens of features, additive by policy.

**Pitfall:** features that change behaviour instead of adding API break feature unification for downstream crates.

---

## Chapter projects

### 1. Small (~2 h): tested `todo` CLI
Combine `clap_cli`, `testing_patterns`, `language/io_files_paths`, `errors/error_enum`.
- Milestones: (1) subcommands; (2) store with atomic writes; (3) black-box binary tests.
- Acceptance: each subcommand has a test via `CARGO_BIN_EXE_`; exit codes 0/1/2 asserted; fake clock for due dates.
- Stretch: `--json` output.

### 2. Intermediate (~weekend): property-testing a data structure
Combine `shrinking`, `cb-testing-tools`, `testing_patterns`, plus `data_structures/lru_cache` and `concurrency/atomics_ordering`.
- Milestones: (1) write your own LRU (or use the recipe as oracle); (2) proptest model-based test; (3) snapshot of eviction order; (4) a loom test for a shared counter.
- Acceptance: injected bug found by proptest with a counterexample of length <= 5 ops; loom fails on the wrong ordering.
- Stretch: `cargo fuzz` target (<https://github.com/rust-fuzz/cargo-fuzz>) on the parser.

### 3. Advanced (~week): observable job runner with its own proc macro
Combine `tracing_logs`, `clap_cli`, `cb-macros`, `cb-features-workspace`, `cb-build-script`, plus `async/worker_pool` and `errors/error_chain_retryable`.
- Milestones: (1) CLI `run`/`status`; (2) `#[traced_job]` attribute macro adding a span with the job id; (3) cargo features `json-logs`, `metrics`; (4) build script embedding git hash shown by `--version`.
- Acceptance: trybuild tests for macro misuse; feature matrix of 4 builds passes; log lines parse as JSON and every job has start+finish events (checked by a test).
- Stretch: OpenTelemetry export behind a feature.
