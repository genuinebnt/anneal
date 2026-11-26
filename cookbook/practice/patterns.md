# Practice: patterns

**How to use this page.** Run the recipe, then do **Prove you got it** by editing one line and predicting the result before you run.
Patterns only stick when you use them on a different domain than the recipe: the [A] tasks ask for that. Do not copy structure from the recipe into the [A] task until you have tried it from a blank file.
Finish with a Chapter project. For anything that touches a FlowForge stage: attempt your stage first, then compare.

---

## any_event_bus
**Prove you got it**
1. Two handlers subscribe to `UserCreated` and one to `OrderPaid`. Predict the numbers `publish` returns for `UserCreated`, `OrderPaid` and `&"just a string"` (2, 1, 0: it returns how many handlers ran), and the final `log.borrow().len()` (3).
2. `reg.insert(100u32)` after an earlier `insert(42u32)` and a `+= 1` through `get_mut`: predict what `insert` returns (the old value, `Some(43)`) and what `reg.get::<u32>()` is afterwards (`Some(100)`).

**Try next**
- [S] Subscribe a handler for `&'static str` and publish `&"just a string"`: predict whether the type id of `&'static str` matches the one used by `publish(&"...")` (the recipe prints `type_name::<&str>()` for a reason) and assert the handler count.
- [M] Make a handler publish a second event from inside `publish` (re-entrancy). Provoke the first failure (`already borrowed` panic or a borrow-checker error depending on your design), then fix it with a queue drained after the handlers return; assert the order of `log` lines.
- [A] Build a plugin host: three plugins each subscribe to their own events and keep private state in `Registry`. Acceptance: a seeded run of 10k mixed events gives the same per-plugin counters as a plain `match`-based dispatcher you write as the oracle.

**Use it in:** a UI or game event system and service locators. Real: Bevy's resources and events <https://github.com/bevyengine/bevy>.

**Pitfall:** `Any` requires `'static`, so a handler for borrowed event data cannot be registered; also, if you know all event types, the closing lesson of the recipe says an `enum` is better.

---

## design_patterns_high
**Prove you got it**
1. The strategy demo prints "closure=0 bytes, Box<dyn Fn>=16 bytes". Predict `size_of_val` of a closure capturing one `u64` and of `&dyn Fn(i32)->i32`; verify with `println!`.
2. The observer demo claims "Weak subscribers pruned automatically". Predict how many subscribers remain after you drop one `Rc` listener and publish twice; add the print to `demo_observer`'s copy and check.

**Try next**
- [S] In `demo_builder` copy the consuming builder and call `.build()` twice on the same value: provoke E0382 (use of moved value) and keep it as a comment; explain why the `&mut self` builder variant allows a second `build()`.
- [M] Add a third strategy (tiered pricing) to `demo_strategy` and a table test of 5 amounts across the three strategies; failure mode: negative amounts return `Err`.
- [A] Reimplement the chain-of-responsibility (`demo_chain`) once with `Vec<Box<dyn Fn>>` middleware and once as an enum pipeline; assert both produce the same trace for 100 seeded inputs and write down which is easier to extend (count lines changed to add a step).

**Use it in:** application architecture. Real: tokio's `runtime::Builder` is a builder with validation in `build()` <https://github.com/tokio-rs/tokio>.

**Pitfall:** porting GoF class hierarchies literally into trait-object trees when an enum plus `match` is smaller and exhaustive.

---

## design_patterns_low
**Prove you got it**
1. Section 1 says `Option<NodeId>` is 8 bytes. Predict `size_of::<Option<NodeId>>()` if `NodeId` wraps `NonZeroU32` instead of `u32` (4 bytes: niche optimisation) and check with `assert_eq!`.
2. Section 10 shows "enum vs dyn: 1 vs 1001 allocations". Predict the number of allocations for 1000 `Box<dyn Shape>` plus the Vec, then why the enum version needs only the Vec's single allocation.

**Try next**
- [S] In `demo_intern`, intern the same string 3 times and assert the allocation counter delta is 0 after the first (the recipe's `Counting` allocator exposes `allocs`).
- [M] Add a section 12: bit-pack a `(level: u2, kind: u3, flag: bool)` into a `u8` with accessors; failure mode: out-of-range input is rejected (`Err`). Property: set/get round trip over all 2*8*4 combos.
- [A] Write an SoA vs AoS benchmark for updating only `x` of 1M particles (use `perf/bench_harness`); acceptance: SoA faster by a margin you measure, and you explain it with bytes per row from `perf/data_layout`.

**Use it in:** compilers and ECS-style game code. Real: oxc's arena-allocated AST <https://github.com/oxc-project/oxc> and rustc's arenas <https://github.com/rust-lang/rust/tree/master/compiler/rustc_arena>.

**Pitfall:** claiming speedups without measuring; SoA loses when every field is touched together.

---

## generic_state_machine
**Prove you got it**
1. Predict the three `replay::<Account>` printouts: the `ok` sequence (`Open, Deposit(500), Withdraw(200), Withdraw(300), Close`) ends at balance 0 closed; `bad` (`Withdraw(500)` with 100) returns `Err`; `[Deposit(1)]` alone returns `Err` (not open). Run and compare, noting the exact error variants.
2. The brute-force loop prints "N of M length-5 sequences are fully valid" with M = 4^5 = 1024. Predict whether N is more than 100; compute it by hand for sequences starting with `Open` (the only legal first event) and compare with the printed value.

**Try next**
- [S] Add `Tx::Freeze` that makes `Deposit`/`Withdraw` fail until `Unfreeze`; rerun the prefix-split law (`continued == full`) with these events added.
- [M] Make the alphabet `[Open, Deposit(5), Withdraw(5), Close, Freeze, Unfreeze]` and extend the brute force to length 6. Invariant to assert: no valid sequence has a negative or overflowed balance. Introduce a bug (let `Withdraw` skip the check) and prove the brute force finds it, printing the shortest failing sequence.
- [A] Model a build job (`Queued -> Running -> Passed/Failed -> Retrying`) as a `Machine`; acceptance: bounded model check of all sequences up to length 6 asserts terminal states are never left and `replay(prefix)+rest == replay(all)`.

**Use it in:** order/ticket lifecycles with event sourcing (replay events to rebuild state). Pair with `patterns/state_machine`.

**Pitfall:** putting IO, clocks or randomness inside `step`; replay then differs from the live run.

---

## sans_io_protocol
**Prove you got it**
1. `simulate(seed, 20, 10)` is asserted deterministic (`simulate(7,20,10).0` twice equal). Predict what changes if you replace the seeded RNG with `Instant::now()` jitter, and which assertion catches it first.
2. The test with exhausted retries asserts `stats.retransmits == 20` and `Event::Closed(Reason::RetriesExhausted)` after at least 4 virtual seconds. Predict the real elapsed time of that test (about 0) and why.

**Try next**
- [S] Add a request that is exactly one line below the line limit and one above; predict `Error::LineTooLong` (already asserted at 2000 bytes) for the second and success for the first.
- [M] Add a `Reason::BadVersion` close: a server banner of `WELCOME 2` must close the client. Test with a hand-fed server banner and assert `poll_event() == Some(Event::Closed(Reason::BadVersion))` plus that `poll_transmit()` is then `None`.
- [A] Write a different sans-IO protocol (a toy ping/pong keepalive with 3 missed pongs -> close) with the same `Sans` trait, and drive it with the existing simulator under 20% loss; invariant: never closes before 3 consecutive misses, always closes after.

**Use it in:** protocol crates that must be testable without sockets. Real: quinn-proto <https://github.com/quinn-rs/quinn> and `h2` in hyper <https://github.com/hyperium/hyper>.

**Pitfall:** calling `Instant::now()` inside the state machine, making timeouts untestable.

---

## state_machine
**Prove you got it**
1. `p.clone().on(Event::Return("late"))` after delivery and `Parcel::Created.on(Event::Deliver(..))` are both printed. Predict `Ok`/`Err` for each (the recipe says delivered parcels are terminal; `Created` cannot be delivered directly).
2. The brute-force loop asserts terminal states reject all three events. Predict the compile effect of adding `Event::Cancel`: which function(s) stop compiling (the exhaustive `match` in `on`)?

**Try next**
- [S] Add `Parcel::Lost` reachable from `Shipped` only; extend the terminal-states loop to include it and make it pass.
- [M] Write the full transition table as `fn legal(state: &Parcel, ev: &Event) -> bool` independently (from the spec, not copying `on`), then assert for every (state, event) pair that `legal == on(..).is_ok()`; introduce a typo in `on` and show the assertion catch it.
- [A] Port a lifecycle from your own work (e.g. a build or invoice) with timestamps in each state's data; acceptance: serialise state to JSON (`encoding_compression/serde_defaults`) and reload equal.

**Use it in:** order, payment or job lifecycles. For the compile-time version see `typestate_protocol`.

**Pitfall:** a `_ => Err(..)` wildcard that hides forgotten legal transitions when you add a variant.

---

## tower_service_layer
**Prove you got it**
1. The retry test asserts `mock.calls() == 3` for two failures then success, and `always.calls() == 4` for `max_attempts` bounding total calls. Predict the number of calls with `max_attempts = 1` and with the backoff schedule `[10,20,40,80,100,100]` which parameters produce it (base 10 ms, doubling, cap 100 ms).
2. The order test asserts the trace `["outer:in","inner:in","inner:out:ok","outer:out:ok"]`. Predict the trace if you swap the two layers in `ServiceBuilder`; then check.

**Try next**
- [S] Add a test that the breaker opens after exactly 3 failures (as the recipe asserts) and with `threshold = 2` opens after 2; assert `mock.calls()`.
- [M] Add a layer `Hedge` that after `t` ms of no reply sends a second request and returns the first success; failure mode: both fail returns the last error. Use the `ManualClock` and `Mock` scripted steps; assert call counts 1 (fast) and 2 (slow).
- [A] Stack timeout + retry + circuit breaker + rate limit around a fake HTTP client with 1000 seeded requests; invariants: inner calls <= requests * max_attempts; while the breaker is open the inner service sees 0 calls; rejected-by-rate-limit calls never reach the inner service (as the recipe's burst-of-3 test shows). Compare behaviour with `distributed/token_bucket`.

**Use it in:** HTTP clients and servers (axum, hyper, tonic). Real: <https://github.com/tower-rs/tower>.

**Pitfall:** layer order: retry outside timeout gives each attempt its own deadline, timeout outside retry bounds the total time.

---

## type_erasure_vtable
**Prove you got it**
1. The queue test runs 1000 tasks, runs the even ones and drops the rest, then asserts `(ran, drops) == (500, 1000)`. Predict `drops` if `ErasedTask::drop` forgets to drop the captures of tasks that were never run (500), and the assertion that fails.
2. `Square(3.0).map_area(|a| a as i32)` is a generic method so it is not on `dyn Shape`. Predict the compile error if you call `shapes[0].map_area(..)` on a `Box<dyn Shape>` (E0038-style or "method not found/where Self: Sized") and read it.

**Try next**
- [S] Add a third shape (`Triangle`) with `clone_box` and `as_any`; assert `total` area changes accordingly and `downcast_ref::<Triangle>` works.
- [M] Make `ErasedTask::run` consume `self` and prove double-run is impossible (keep E0382); add a `try_run(&mut self) -> bool` that runs only once, using a flag; assert second call returns `false`.
- [A] Write your own `BoxedFnOnce<Args, Ret>` with a hand-rolled vtable (call + drop fn pointers) and a 24-byte inline buffer for small closures; acceptance: the recipe's drop-count test (500/1000) passes unchanged, and `cargo +nightly miri run` is clean.

**Use it in:** task queues and plugin systems. Real: tokio's task header stores a vtable of function pointers <https://github.com/tokio-rs/tokio>.

**Pitfall:** forgetting to store drop/layout information in the vtable, leaking or double-freeing captures.

---

## typestate_builder
**Prove you got it**
1. Uncomment `RequestBuilder::new().build();`. Predict the exact error ("no method named `build` found for `RequestBuilder<NoUrl>`", E0599) and note which `impl` block holds `build`.
2. `retries(3)` is called before `url(...)` in `main`. Predict whether this compiles (yes: `retries` is in a generic `impl<S>` block); then move `retries` into the `HasUrl` impl only and predict the new error.

**Try next**
- [S] Add a `method("GET")` required step with a second marker type; assert that building without it is rejected at compile time (keep E0599 in a comment).
- [M] Add a runtime check: `retries(n)` with `n > 10` is an `Err` via `build() -> Result<Request, BuildError>`; add a table test of 4 values. Explain which errors are compile-time (missing url) and which runtime.
- [A] Typestate builder for a CLI command spec (name required, args optional, subcommands optional). Acceptance: two `compile_fail` doctests, one runtime validation test, and `size_of` of the builder in each state printed.

**Use it in:** HTTP clients and config objects where a step must not be skipped. Real: builders in hyper <https://github.com/hyperium/hyper>.

**Pitfall:** exploding numbers of marker types when many fields are required; use it for one or two critical steps only.

---

## typestate_protocol
**Prove you got it**
1. `s.login("ana","wrong")` returns `Err((s, e))`. Predict `s.state_name()` printed in the error branch (`Connected`) and why the session is not lost. Remove the returned session from the `Err` tuple type and predict the compile error in `main`.
2. Uncomment `impl State for String {}`. Predict the error (E0277: the trait bound `String: Sealed` is not satisfied) and verify.

**Try next**
- [S] Add a `Closing` state between `LoggedIn` and `Disconnected` and make `disconnect` unavailable from `LoggedIn`; compile-fail check by calling it.
- [M] Add a second failed login limit: after 3 failures `login` returns an error variant `AuthError::Locked` and the session transitions to `Disconnected`; test with 3 wrong passwords then the correct one.
- [A] Model a file transfer (`Idle -> Negotiating -> Transferring -> Done`) with typestate and drive it from the sans-IO client (`sans_io_protocol`); acceptance: three `compile_fail` doctests for illegal sequences and one passing end-to-end test.

**Use it in:** protocol clients and hardware drivers. Real: `embedded-hal` style pin types (<https://github.com/rust-embedded/heapless> for the embedded ecosystem).

**Pitfall:** consuming `self` on a fallible transition and losing the resource on error; return it in the `Err` as the recipe does.

---

## undo_redo_editors
**Prove you got it**
1. The recipe asserts `per_entry[0] < 200` (command), `per_entry[1] > 4000` (memento), `per_entry[2] * 3 < per_entry[1]` (rope). Predict which editor has the cheapest history entries and which one is the most expensive; then run and read the printed table.
2. Typing `"hello"` char by char is one undo step in all five designs (coalescing). Predict `ed.undo()` results across three consecutive `undo()` calls on that editor (`true`, `false`, `false`).

**Try next**
- [S] Add a `Replace` action built from delete+insert and assert one `undo()` restores the original text in all five designs.
- [M] Bound history to the last N entries in the command editor; failure mode: undo past the bound returns `false` and leaves text unchanged. Differential-test against `Model` with a bound.
- [A] Add a sixth design behind the `Editor` trait (e.g. a gap buffer with command inverses) and run the recipe's 20000-sequence differential test against the `Model`; it must stay equal to the model at every step.

**Use it in:** editors, drawing tools and spreadsheets. Real: rust-analyzer applies text edits with a similar edit model <https://github.com/rust-lang/rust-analyzer>.

**Pitfall:** inverse operations computed from mutable state at undo time rather than recorded when the edit happened.

---

## Chapter projects

### 1. Small (~2 h): traffic light with typestate and enum views
Combine `typestate_builder`, `state_machine`, `language/pattern_matching`.
- Milestones: (1) enum state machine with transition table; (2) typestate API for the same lights; (3) one test proving both agree.
- Acceptance: exhaustive (state, event) test; compile_fail doctest for illegal move.
- Stretch: serialise state with `encoding_compression/serde_defaults`.

### 2. Intermediate (~weekend): event-sourced inventory
Combine `generic_state_machine`, `any_event_bus`, `undo_redo_editors`, plus `errors/error_enum` and `dev_tools/shrinking`.
- Milestones: (1) pure `step`; (2) append-only event log; (3) bus notifying projections; (4) undo via inverse events.
- Acceptance: replay == live state for 10k seeded events; shrinking finds a minimal failing log for an injected bug.
- Stretch: snapshots every N events.

### 3. Advanced (~week): sans-IO resilient client stack
Combine `sans_io_protocol`, `tower_service_layer`, `typestate_protocol`, `type_erasure_vtable`, plus `async/select_cancel` and `distributed/retry_backoff_jitter`.
- Milestones: (1) sans-IO request/response protocol; (2) simulator with loss/delay; (3) tower-style layers (timeout, retry, breaker); (4) tokio driver.
- Acceptance: seeded sim with 10k requests: no request answered twice, none lost without an error; layered behaviour equals a model.
- Stretch: replay a captured trace deterministically.
