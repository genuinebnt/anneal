BusTub's skip list tests, ported: insert/contains/clear, and the four concurrent tests: ten threads inserting disjoint ranges, ten erasing, a mix of both, and eight readers walking a list of 80 000 keys at once.

## The task

Make the stage's tests pass: `cargo test --test stages_0b s0b_03`.

The tests: BusTub's own (insert, contains and clear; concurrent inserts, erases and mixed workloads; readers sharing the list; a big list dropped without overflowing the stack), and a new one where **four threads insert and erase overlapping keys** and the final list is strictly increasing, its size equals its length and every key it lists is found.

## Your freedom

None new: a failure belongs to stage 1 or 2.

## The Rust toolbox

**A big list is dropped without overflow.** If the nodes were linked by `Box`, dropping a long chain recurses; an arena of values has no such problem.

**Reading a failure in a concurrent test.** The test prints the thread and the key; run the same operations single-threaded in a loop first, then add the threads.

## Design notes

**Readers must share.** `contains` takes the **read** lock; if it took the write lock, eight readers would take turns and BusTub's test would time out ("`You will see a timeout if your reads cannot share access to the skip list`").

**What a race looks like here.** A lost insert (fewer successes than threads × keys), a key present after its erase, or a panic from an out-of-range link: all mean two threads were inside a write at once, or a read overlapped a write. With one `RwLock` around everything that cannot happen unless a method forgets its lock.

**Memory.** The big-list drop test builds 200 000 nodes; in C++ this is the case the iterative `Drop` exists for. An arena is dropped without recursion.

## If this is new

- Everything is in the earlier stages of this module.
- [C1 Threads & shared state](/t/c1-threads-shared-state): Understand it: `RwLock` around the whole structure.

## Tests

- BusTub's skip list tests; the overlapping-writers test; the stage 1 and 2 properties.

## Hints

### Take the lock inside each public method

`insert`, `erase`, `contains`, `size`, `clear` each take their own guard; do not call one public method from another while holding a guard (a read guard then write lock on the same thread deadlocks).

### A failing concurrent test: shrink it

Two threads and ten keys reproduce most races with a readable failure.

## Performance

The concurrent tests run a few thousand operations across threads and finish in well under a second.

## Experiment

Optional. Predict first, then run.

1. **Drop the write lock in `erase`.** Which concurrent test fails first?
2. **A million keys.** How deep is the list, and how long does a lookup take?

## Other designs

None for this stage. The *Other designs* sections of 0b-01 and 0b-02 list the alternatives to compare with yours.

## In BusTub

`test/primer/skiplist_test.cpp`: `InsertContainsTest1`, `ConcurrentInsertTest`, `ConcurrentEraseTest`, `ConcurrentInsertAndEraseTest`, `ConcurrentReadTest`.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::atomic<int> successful_insertions` | `Arc<AtomicUsize>` |
| `std::thread` with a lambda capturing by reference | `thread::spawn(move \|\| ...)` with a cloned `Arc` |

**Port rule:** threads cannot borrow locals in Rust (without scoped threads), so shared state is an `Arc`.

## Learn more

- [`std::thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html) · [`AtomicUsize`](https://doc.rust-lang.org/std/sync/atomic/index.html)
