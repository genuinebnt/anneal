**Where this fits.** The end of the plain B+ tree: a long random run against a `BTreeSet`, then BusTub's own insert, delete, scale and concurrent tests, ported.

> [!CHECK] BusTub's `OptimisticInsertTest` finds a leaf with room, inserts into it and asserts that **exactly one** write latch was taken. Which of your design decisions could make that test fail although your tree is correct? Name two.
> ||Taking a write latch on the header or on the parent "just in case" (for instance write-latching down the whole path before checking whether the leaf is safe) makes it 2 or more; counting a failed optimistic attempt followed by the pessimistic retry as two writes when the leaf was in fact safe does too. The test exercises the *fast path*: a leaf that cannot split must be inserted into holding only its own write latch.||
>
> - Which latches does the fast path take?
> - What does the test count?
> - What does it not check?

## The task

Nothing new to design.

- **A long run.** Four tree shapes (leaf and internal sizes (2,3), (3,4), (5,5), (8,3)); 6 000 random inserts, removes and lookups each against a `BTreeSet`; the shape checker every 500 steps and at the end; no page left pinned.
- **BusTub's tests.** `b_plus_tree_insert_test.rs` (basic insert, optimistic insert, insert tests with and without the iterator), `b_plus_tree_delete_test.rs` (delete, optimistic delete, sequential edge mix), `b_plus_tree_sequential_scale_test.rs` (5 000 keys in random order) and `b_plus_tree_concurrent_test.rs` (inserts, deletes and mixes over several threads). The C++ tests read leaf pages directly; here they use the tree's observers.

## Your freedom

The same as before. A failure here may be a latching bug that the smaller tests never provoked.

## The Rust toolbox

**Run a concurrency test many times.** `for i in $(seq 50); do cargo test --test b_plus_tree_concurrent_test || break; done`.

**Release mode for the scale test.** `cargo test --release --test b_plus_tree_sequential_scale_test`.

**Reading a shape failure.** The checker names the rule: "a leaf holds N pairs, fewer than the minimum", "a lookup latched K pages". Reproduce on a small tree of your own by printing `leaf_sizes()` and `depth()` after each step.

## If this is new

- Everything is in the earlier stages of this module.

## Tests

- Long runs on four tree shapes agree with a model.
- BusTub's insert, delete, sequential-scale and concurrent tests.

## Hints

### The optimistic tests fail

Latch counts: print `tree.bpm.get_reads()` and `get_writes()` around one insert. Two writes means the fast path was not taken: is your "safe" check off by one (`size + 1 < max_size`)?

### A concurrent test fails once in a while

A race. Re-run in a loop, look at which key is missing or duplicated, and ask which latch was released before the page was safe.

## Performance

The scale test inserts 5 000 keys with leaf size 2 and internal size 3: a tree of depth 12 or more. Run it in release mode and compare debug and release; that is the cost of bounds checks and debug assertions in your page code.

## Experiment

Optional. Predict first, then run.

1. **Bigger pages.** Run the scale test with leaf size 100: how much faster, and what does the depth become?
2. **Under eviction.** Run a long test with a pool of only as many frames as the depth plus a few. What does the pool's eviction rate say about your guards?

## Other designs

None for this stage. The *Other designs* sections of 2c-01 to 2c-05 list the alternatives to compare with yours.

## In BusTub

These are the tests of Project 2 tasks 1 to 4 (checkpoints 1 and 2); once they pass, the index is ready for module 2d (tombstones) and for the catalog's indexes in Project 3.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::thread` + `LaunchParallelTest` | `thread::scope` |
| `std::shuffle` with a default engine | a fixed-seed Fisher-Yates shuffle |
| `tree.GetRootPageId() == INVALID_PAGE_ID` | `tree.get_root_page_id() == PageId::INVALID` |

**Port rule:** a test that reaches into pages becomes a test that asks the tree through an observer.

## Learn more

- BusTub's [project 2 page](https://15445.courses.cs.cmu.edu/) · [`thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html)
