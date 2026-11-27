**Where this fits.** The end of the single-threaded B+ tree: a long random run against a `BTreeSet`, then BusTub's own insert, delete and scale tests, ported. The tests of the optimistic fast path and the concurrent tests come with module 2e.

> [!CHECK] BusTub's `SequentialEdgeMixTest` inserts eleven keys into trees with leaf sizes 2 to 5 and internal size 3, then removes them one by one, checking every lookup after every step. Which of your stages does a leaf size of 2 stress hardest, and why is a tree with `leaf_max_size = 2` a good test of the minimum-size rules?
> ||With `leaf_max_size = 2` a leaf holds exactly one pair at rest and its minimum is one, so almost every insert splits a leaf (stages 2c-02 and 2c-03) and almost every remove empties one and forces a borrow or a merge (2c-05), with internal pages of 3 children splitting and merging on top (2c-03 and 2c-06). There is no slack anywhere: an off-by-one in when a page splits or when it is short changes the shape on the next operation, so the lookups after each step find it at once.||
>
> - How many pairs does a leaf of `leaf_max_size = 2` hold at rest?
> - What is its minimum?
> - Which operation follows nearly every insert and every remove?

## The task

Nothing new to design.

- **A long run.** Four tree shapes (leaf and internal sizes (2,3), (3,4), (5,5), (8,3)); 6 000 random inserts, removes and lookups each against a `BTreeSet`; the shape checker every 500 steps and at the end; no page left pinned.
- **BusTub's tests.** `b_plus_tree_insert_test.rs` (basic insert, insert tests with and without the iterator), `b_plus_tree_delete_test.rs` (delete, sequential edge mix) and `b_plus_tree_sequential_scale_test.rs` (5 000 keys in random order). The C++ tests read leaf pages directly; here they use the tree's observers. The two files also hold BusTub's optimistic tests, which run with module 2e.

## Your freedom

The same as before. A failure here is a rule that the smaller tests never pushed to its edge.

## The Rust toolbox

**Release mode for the scale test.** `cargo test --release --test b_plus_tree_sequential_scale_test`.

**Reading a shape failure.** The checker names the rule: "a leaf holds N pairs, fewer than the minimum", "a lookup latched K pages". Reproduce it on a small tree of your own, printing `leaf_sizes()` and `depth()` after each step.

**Shrinking by hand.** The long run prints the step that broke. Re-run it with the same seed and stop a few steps earlier: the tree just before the bad step is the smallest case you have.

## If this is new

- Everything is in the earlier stages of this module.

## Tests

- Long random runs on four tree shapes agree with a model and keep every rule.
- BusTub's insert, delete and sequential-scale tests.

## Hints

### The edge mix fails for one leaf size

Run the test body for that leaf size alone, printing the leaf sizes after each call. The first step where they break a rule names the stage: a split (2c-02, 2c-03) or a borrow or merge (2c-05, 2c-06).

### The scale test is slow

5 000 keys in leaves of 2 under pages of 3 make a tree twelve or more levels deep, and debug builds check every slice index. Run it in release mode before you look for a bug.

## Performance

The scale test inserts 5 000 keys with leaf size 2 and internal size 3: a tree of depth 12 or more. Compare its time in debug and in release mode; the difference is the cost of bounds checks and debug assertions in your page code.

## Experiment

Optional. Write your prediction down first, then run.

1. **Bigger pages.** Run the scale test with leaf and internal sizes of 100. How much faster is it, and what is the depth now?
2. **Under eviction.** Run the long run with a pool of only as many frames as the depth plus a few. Does it still pass, and what does the pool's eviction count say about how long your guards live?

## Other designs

None for this stage. The *Other designs* sections of 2c-01 to 2c-06 list the alternatives to compare with yours.

## In BusTub

These are the single-threaded tests of Project 2's B+ tree tasks; the concurrent tests and the latch counts of `OptimisticInsertTest` and `OptimisticDeleteTest` are module 2e.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::shuffle` with a default engine | a fixed-seed shuffle |
| `tree.GetRootPageId() == INVALID_PAGE_ID` | `tree.get_root_page_id() == PageId::INVALID` |
| `IndexLeaves` reading leaf pages | the tree's `leaf_sizes` observer |

**Port rule:** a test that reaches into pages becomes a test that asks the tree through an observer.

## Learn more

- BusTub's [project 2 page](https://15445.courses.cs.cmu.edu/) · [B+ tree (Wikipedia)](https://en.wikipedia.org/wiki/B%2B_tree)
