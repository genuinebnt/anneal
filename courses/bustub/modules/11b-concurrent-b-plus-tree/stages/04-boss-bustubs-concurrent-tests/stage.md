**Where this fits.** The end of the index modules: BusTub's concurrent B+ tree tests for a plain tree and for a tree with tombstones, its two latch-count tests of the optimistic path, and a threaded workload of inserts, deletes and lookups on a tree with tombstones.

> [!CHECK] BusTub's `OptimisticInsertTest` finds a leaf with room, inserts into it and asserts that **exactly one** write latch was taken. Which of your design decisions could make that test fail although your tree is correct? Name two.
> ||Taking a write latch on the header or on the parent "just in case", for instance write-latching down the whole path before checking whether the leaf is safe, makes it two or more. So does counting a failed optimistic attempt followed by the pessimistic retry when the leaf did have room: an off-by-one in "has room" (`size + 1 < max_size`) sends a safe insert down the slow path. The test checks the fast path: a leaf that cannot split must be inserted into holding only its own write latch.||
>
> - Which latches does the fast path take?
> - What does the test count?
> - What does it not check?

## The task

Nothing new to design.

- **Threads on a tree with tombstones.** Six threads on a `TOMBS = 3` tree: two insert 360 contested keys, two delete them, two read 40 preserved keys; five rounds. At the end every preserved key is in the scan, `check_shape_t` holds for whatever set of keys is live, no page stays pinned, and the whole run finishes under a timeout.
- **BusTub's tests.** `b_plus_tree_concurrent_test.rs` (concurrent inserts, deletes and mixes over several threads), `optimistic_insert_test` and `optimistic_delete_test` from the insert and delete test files, and the threaded tombstone variants and the optimistic latch count with tombstones from `b_plus_tree_tombstone_variants_test.rs`.

## Your freedom

The same as before. A failure here is usually a latching bug that the smaller tests never provoked.

## The Rust toolbox

**Run a concurrency test many times.** `for i in $(seq 50); do cargo test --test b_plus_tree_concurrent_test || break; done`.

**Release mode for the variants.** Each threaded tombstone scenario runs 20 to 50 times: `cargo test --release --test b_plus_tree_tombstone_variants_test mix_test`.

**A watchdog of your own.** When a test hangs, wrap your reproduction in a thread and `recv_timeout`, then print which latches each thread holds when the time runs out.

## If this is new

- Everything is in the earlier stages of this module and of 2c and 2d.

## Tests

- Threads deleting, inserting and reading on a tree with tombstones keep every preserved key and a valid structure.
- BusTub's concurrent tests, its optimistic latch counts, and the threaded tombstone variants.

## Hints

### A concurrent test fails once in a while

A race. Re-run it in a loop, look at which key is missing or found twice, and ask which latch was released before the page was safe. Then check the same scenario with `TOMBS = 0`: if only the tombstone tree fails, a tombstone path takes a latch the plain path does not, or lets one go early.

### The optimistic tests fail

Print `tree.bpm.get_reads()` and `get_writes()` around the one insert or remove. Two writes means the fast path was not taken or gave up: is the "has room" or "stays above its minimum" check off by one?

## Performance

The concurrent tests repeat each scenario many times; in debug mode the tombstone variants take tens of seconds, in release mode a few. The interesting number is not the time but how it scales with threads: run the mix tests with 1, 2, 4 and 8 threads in a test of your own.

## Experiment

Optional. Write your prediction down first, then run.

1. **Bigger buffers under threads.** Run the threaded tombstone test with `TOMBS = 16`. Does the fast path win more or less often, and why?
2. **Contention on one leaf.** Make all writer threads use keys of a single leaf. Which latch do they all queue at now, and what does that say about hot keys in real indexes?

## Other designs

None for this stage. The *Other designs* sections of 2e-01 to 2e-03 list the alternatives to compare with yours.

## In BusTub

These are Project 2's concurrent index tests (`b_plus_tree_concurrent_test.cpp`, run for a plain tree and with tombstones) and the latch-count tests of its optimistic latching task; passing them is the evidence that your index is ready for the query engine of Project 3.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `LaunchParallelTest(num_threads, ...)` | `thread::scope` with one spawn per thread |
| `std::thread` + `join` | scoped threads that join at the end of the scope |
| `GetReads()`, `GetWrites()` on the traced pool | `tree.bpm.get_reads()`, `get_writes()` |

**Port rule:** threads that borrow the tree become scoped threads, so the borrow checker knows the tree outlives them.

## Learn more

- [`thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html) · [`Receiver::recv_timeout`](https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.recv_timeout)
- BusTub's [project 2 page](https://15445.courses.cs.cmu.edu/)
