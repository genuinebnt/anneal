BusTub's remaining Project 4 tests, ported: the multi-transaction insert/delete scenario, the two garbage collection walks (with and without tainted transactions), the index conflict test, the three concurrent index tests (inserts racing for keys, updates racing on rows, updates and aborts racing on pairs of rows), and the abort test with ordered queries.

## The task

Make the stage's tests pass: `cargo test --test stages_4b s4b_09`.

The tests: BusTub's own scenarios, ported (insert and delete with conflicts across seven transactions; two garbage collection walks, one with tainted transactions; the index conflict test; concurrent index inserts, updates and update-with-abort over many threads; a simple abort with ordered queries), and a final property: **a long session** (up to 80 steps) **with every feature at once** (writers, conflicts, aborts, garbage collection) against the model, which also exercises a primary-key table in the stage-6 property.

## Your freedom

None new: a failure belongs to one of your earlier stages.

## The Rust toolbox

**Reading a failing scenario.** The ported tests print the SQL and the transaction ids (`txn{id}`); replay the shrunk session of a failing property by hand and print the table after each step with `txn_mgr_dbg` (module 4a).

**Threads.** The concurrent tests use `thread::scope` and several transactions at once; a hang is a lock held across a statement or a latch order mistake, not a slow test.

## Design notes

**Concurrency tests find races, not only bugs in the algorithm.** If one fails, run it several times (`cargo test --test stages_4b s4b_09_index_concurrent -- --test-threads=1 --nocapture`) and read the first difference: "exactly one winner" failing with 0 winners means a transaction failed that should not have (a spurious conflict, often from checking a timestamp against the wrong transaction); with 2 winners means the index let two inserters through (the `insert_entry` result was ignored). A wrong final value means a lost update: a conflict check that ran against a stale copy instead of under the page latch.

**Aborted transactions' leftovers.** The abort test deliberately leaves aborted transactions in the map; the final scan must show only committed increments. If a count is too high, an abort did not restore a tuple; if too low, a commit was lost.

**Time.** Each concurrent test runs a few seconds in debug mode. `--release` is faster and also more racy, which is the point.

## If this is new

- Everything is in the earlier stages of this module.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it: a check repeated under the lock that makes it true; latch order and deadlock.
- Y5 Testing & verification: Build it: sessions of interleaved transactions against a model; an oracle that tries every serial order.

## Tests

- BusTub's multi-transaction, garbage collection and concurrent index tests.
- The long session property.

## Hints

### A spurious conflict is a bug

A transaction that conflicts with nobody must never fail. Use the single-threaded tests to find the case, then think about what changes under threads.

### Look at the heap size

`heap_entries` too large means an update or insert created a tuple where it should have reused one; too small cannot happen.

### Reproduce with fewer threads

Two threads and one key give the same failure with a message you can read.

## Performance

The concurrent tests run a few hundred transactions across threads and take a few seconds in debug mode; `cargo test --release` is faster.

## Experiment

Optional. Predict first, then run.

1. **More threads.** Raise the thread counts of the concurrent index tests. Does a flake show up?
2. **Remove one rule.** Disable the tainting on conflict. Which tests fail first?

## Other designs

None for this stage. The *Other designs* sections of 4b-01 to 4b-07 list the alternatives to compare with yours.

## In BusTub

`test/txn/txn_executor_test.cpp`, `txn_index_test.cpp`, `txn_index_concurrent_test.cpp`, `txn_abort_serializable_test.cpp`. The original concurrent tests use 8 threads and 50 trials; the ports use fewer trials so that a debug run finishes in seconds. Some of BusTub's cases continue on the autograder ("`// test continues on Gradescope...`"); the visible part is what is ported.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::thread` and a mutex-protected result map | `std::thread::spawn` and the join handle's return value |
| `BUSTUB_ENSURE(cond, "cannot commit??")` | `assert!(cond, "cannot commit??")` |

**Port rule:** threads that return their results through `JoinHandle` need no shared result map.

## Learn more

- [`std::thread`](https://doc.rust-lang.org/std/thread/) · [BusTub's txn tests](https://github.com/cmu-db/bustub/tree/master/test/txn)
