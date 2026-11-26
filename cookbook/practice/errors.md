# Practice: errors

**How to use this page.** Run each recipe (`cargo run --release --example <name>`), then answer **Prove you got it** by experiment before reading the code again.
Errors are the part of Rust you learn by breaking things: for every task, force the failure on purpose and read what the user would see.
Keep a scratch crate; never edit the recipe files. Finish with a Chapter project.

---

## error_enum
**Prove you got it**
1. The recipe is marked `EXPECT-EXIT: 1` and defaults to `/definitely/missing.json` when run with no args. Predict the stderr line (it comes from `CookError::Read`'s `{path}: {source}` format) and the exit code; then run it with a file containing `{"name":"soup","steps":[]}` and predict which variant fires (`NoSteps("soup")`). Check with `echo $?`.
2. Remove `#[from]` from `CookError::Parse`. Predict which line in `run` stops compiling (the `serde_json::from_str(&text)?`) and the error code (E0277, `From<serde_json::Error>` not implemented).

**Try next**
- [S] Make `main` return exit code 2 for `CookError::Usage` (currently the demo pushes a default path, so remove that push first) and 1 for the rest. Verify with `cargo run --example error_enum; echo $?` for both cases.
- [M] Add a variant `UnknownStep { index: usize, name: String }` raised when a step's `"name"` is missing or empty. Failure mode: the message must show the zero-based index; assert with `assert_eq!(err.to_string(), "step 1 has no name")`-style text for the input `{"steps":[{"name":"a"},{}]}`.
- [A] Apply the same shape to a tool you use (e.g. a script that renames files): `run(&[String]) -> Result<_, Error>` plus a thin `main`. Verify: each failure path prints one `error: ...` line, has a documented exit code, and `grep -c 'unwrap()'` outside tests is 0.

**Use it in:** any CLI or service where only `main` maps errors to exit codes or HTTP statuses. Real: `thiserror` generates this boilerplate <https://github.com/dtolnay/thiserror>.

**Pitfall:** a catch-all `Other(String)` variant that every `?` falls into, so callers cannot match on kinds.

---

## anyhow_context
**Prove you got it**
1. `load` is called with a missing file, a bad JSON file, and an empty-tasks file. Predict, for each, the `{e:#}` one-line output and the number of entries in `e.chain()` (the bad-JSON case has three: `while loading`, `parsing workflow JSON`, serde's message). Run and compare.
2. The loop prints "root cause is io::Error" only for one input. Predict which, then predict whether `e.downcast_ref::<std::io::Error>()` would still work if `read_file` used `.map_err(|e| anyhow!("{e}"))` instead of `with_context` (it would not: the type is erased into a string).

**Try next**
- [S] Add `ensure!(tasks.len() <= 1000, "too many tasks: {}", tasks.len())` to `check` and write a test file with 1001 elements; assert the error text.
- [M] Add a typed error `struct TooDeep(usize)` implementing `std::error::Error`, return it via `bail!`-like `Err(TooDeep(n).into())` when JSON nesting exceeds 3; in `main` use `downcast_ref::<TooDeep>()` to print "fatal" vs other errors "retry later". Test with a 5-deep file.
- [A] Take a small CLI of yours that uses `unwrap`/`expect` on IO and parsing, convert it to `anyhow`, and for 5 injected failures (missing file, permission denied, bad UTF-8, bad number, empty input) check that the printed chain names the file and the operation. Keep the 5 outputs in a test using `assert!(msg.contains(path))`.

**Use it in:** the `main` of an application. Real: `cargo` and many CLIs report errors with `anyhow` chains <https://github.com/dtolnay/anyhow>.

**Pitfall:** returning `anyhow::Error` from a library's public functions, so callers cannot match on kinds.

---

## diagnostics_spans
**Prove you got it**
1. The source is `"upload -> validate\nvalidate -> transcod\n  thumbnail -> pubish\n"` and the recipe asserts two diagnostics and that `&src[diags[0].span]` is `"transcod"`. Predict the `line:col` printed in the `-->` line for each (second line, col 13; third line, col 18 because of two leading spaces) and the help text for `pubish` (`did you mean 'publish'?` only if the first letter matches). Run and compare.
2. `render` computes `col` with `chars().count()` but slices with bytes. Insert a non-ASCII char (`é`) before `transcod` on line 2 and predict whether the caret lines up; then replace `chars().count()` with `.len()` for `col` and see the caret drift.

**Try next**
- [S] Make `find_unknown` also flag a trailing `->` with no right-hand side; assert the span's `start..end` covers exactly the arrow.
- [M] Make `render` handle a span that crosses a newline (use `span.end > line_end`): underline to end of the first line and print "..." marker; test with a span from the end of line 1 into line 2.
- [A] Add a `Vec<Diagnostic>` sorting by `span.start` and a summary line `2 errors`; then drive it from a real parse of your own mini-language (e.g. `key = value` configs) and snapshot the rendered text of three bad inputs as string literals in assertions.

**Use it in:** a linter or config validator. Real: Biome's diagnostics <https://github.com/biomejs/biome> (`crates/biome_diagnostics`); for lexers that keep spans see `text_and_parsing/zero_copy_lexer`.

**Pitfall:** storing preformatted strings instead of byte spans; you can never re-render for another output format.

---

## error_chain_retryable
**Prove you got it**
1. The recipe prints five cases with `retry()`. Predict the five `Yes`/`No` results from the `match` (TimedOut: Yes; PermissionDenied: No; 503: Yes; 404: No; BadInput: No), then change `Rejected { status: 429 }` into the `cases` list and predict.
2. `print_chain` walks `source()`. Predict how many lines are printed for the boxed `StepError` wrapping `Rejected{503}` (2: step failed, server rejected) and for one wrapping `Network(...)` (3: includes the io error). Verify by adding that case.

**Try next**
- [S] Add `CallError::Timeout { after_ms: u64 }` and classify it `Retry::Yes`; extend the `cases` array and assert with `assert_eq!`.
- [M] Write `fn run_with_retry(max: u32, f: impl FnMut(u32) -> Result<(), CallError>) -> Result<(), StepError>`; failure mode: after `max` attempts return a `StepError` whose `source()` is the last `CallError`. Test with a closure that fails twice with 503 then succeeds (3 calls), and one that always returns `BadInput` (exactly 1 call, no retry).
- [A] Feed 100 seeded jobs (30% transient `Rejected{503}`, 5% `BadInput`, rest ok) through `run_with_retry` with `max = 3`; assert the counts: successes + dead-lettered == 100, and no job with `BadInput` was attempted more than once. See `distributed/retry_backoff_jitter` for adding backoff.

**Use it in:** a worker deciding retry vs dead-letter. Real: `hyper` errors expose `is_timeout`/`is_connect` instead of strings <https://github.com/hyperium/hyper>.

**Pitfall:** classifying by `to_string().contains(..)`; one message tweak silently changes retry behaviour.

---

## option_result_toolbox
**Prove you got it**
1. Predict the output of the `slot` block: after `take()`, `replace(8)` and `get_or_insert(1)` what are `taken`, `old`, `slot` and `filled` (`Some(3)`, `None`, `Some(8)`, `8`)? Then predict the line printing `parse_age("300").is_err_and(..contains("too large"))` (the u8 parse error message is "number too large to fit in target type").
2. `["1","z","3"].iter().flat_map(|s| parse_age(s))` yields which values and why no error surfaces? Predict `[1, 3]`, then replace `flat_map` with `map(..).collect::<Result<Vec<_>,_>>()` and predict the error text.

**Try next**
- [S] Write `fn first_odd_cube(xs: &[i32]) -> Option<i32>` in the style of `first_even_square` and assert `[2,4,3]` gives `Some(27)`, `[2,4]` gives `None`.
- [M] Write `fn parse_pair(s: &str) -> Result<(u8, u8), String>` using only combinators and `?` (no `match`): split on `,`, parse both with `parse_age`; the error must include the offending part. Table-test 6 inputs: `"1,2"`, `"1"`, `"1,x"`, `"x,1"`, `" 3 , 4 "`, `"1,300"`.
- [A] Take 20 lines of `unwrap`-heavy code of yours, rewrite with the combinators here, and prove equivalence on 50 seeded inputs: where the old code panicked the new one must return `Err`/`None` (use `catch_unwind` on the old).

**Use it in:** all fallible lookups and parsers. Cheat sheet: <https://doc.rust-lang.org/std/option/enum.Option.html>.

**Pitfall:** `unwrap_or(expensive())` evaluates the argument eagerly; `unwrap_or_else` defers.

---

## Chapter projects

### 1. Small (~2 h): `kvcheck` config validator
Combine `error_enum`, `option_result_toolbox`, `language/conversions`, `language/common_traits`.
- Milestones: (1) parse `key=value` lines; (2) typed errors with line numbers; (3) exit codes 0/1/2 (ok / invalid / usage).
- Acceptance: 12-case table test; a seeded fuzz of 10k random byte lines never panics.
- Stretch: `--format json` output of errors.

### 2. Intermediate (~weekend): diagnostics for a mini DSL
Combine `diagnostics_spans`, `anyhow_context`, `error_chain_retryable`, plus `text_and_parsing/pratt_parser` and `dev_tools/testing_patterns`.
- Milestones: (1) parse arithmetic with variables, spans kept on tokens; (2) 5 error kinds rendered rustc-style; (3) `anyhow` at the CLI layer; (4) snapshot tests.
- Acceptance: every error kind has a test where the caret column is asserted; multi-line input supported; a fuzz test of 100k random strings never panics.
- Stretch: error recovery (report 3 errors in one run).

### 3. Advanced (~week): resilient job runner error model
Combine `error_chain_retryable`, `error_enum`, `diagnostics_spans`, `language/drop_raii`, `patterns/tower_service_layer`, plus `distributed/retry_backoff_jitter` and `distributed/saga_orchestrator`.
- Milestones: (1) typed error taxonomy (transient / permanent / cancelled); (2) retry layer with jittered backoff and budget; (3) compensation (saga) on permanent failure; (4) structured chain logging.
- Acceptance: seeded simulation of 1000 jobs; invariants: no job both succeeds and compensates; retries per job <= budget; error chain never loses the root cause (assert via `source()` walk).
- Stretch: expose classification as a trait so users plug in their own rules.
