**Where this fits.** The end of the tombstone tree: BusTub's four tombstone scenarios, the single-threaded tombstone variants of its delete tests, and long random runs for every buffer size. The threaded tombstone tests come with module 2e, where the tree is shared between threads.

> [!CHECK] BusTub's `TombstoneBasicTest` deletes three keys from one leaf of a tree with `NumTombs = 2`, after the leaf was found to hold more than its minimum, and then asserts that the tree holds two tombstones, the second and the third key, and that the first key is not found. Trace it: which delete retires which pair, and why does the test choose a leaf above its minimum?
> ||The first two deletes fill the buffer with the first and the second key. The third finds the buffer full, so the oldest tombstone, the first key's, is retired: that pair is removed for real, and the third key becomes the newest tombstone. The buffer then holds the second and the third key, in that order, and the first key is gone from the leaf. The test picks a leaf above its minimum so that the one physical removal cannot make the leaf short: a short leaf would purge its own tombstones (2d-02) and the buffer would no longer show the order the test checks.||
>
> - How many tombstones fit in the buffer?
> - What happens to the oldest one when a third delete arrives?
> - What would a short leaf do to its buffer?

## The task

Nothing new to design.

- **Long runs.** 1 500 random operations for leaf sizes and fan-outs `(2,3) (3,3) (4,4) (5,4) (6,7)`, buffer sizes 1, 2 and 3, three seeds each: the live keys equal a `BTreeSet` and `check_shape_t` holds every 25 steps.
- **BusTub's tests.** `b_plus_tree_tombstone_test.rs` (`tombstone_basic_test`, `tombstone_split_test`, `tombstone_borrow_test`, `tombstone_coalesce_test`), and from `b_plus_tree_tombstone_variants_test.rs` the sequential edge mix with tombstones and a random workload for every buffer size. The same file holds the threaded variants, which run with module 2e.

## Your freedom

The same as before.

## The Rust toolbox

**Generic tests over a const.** The variants file instantiates one scenario function for several buffer sizes: `random_workload::<1>(..)`, `::<2>`, `::<3>`.

**Running one scenario.** `cargo test --test b_plus_tree_tombstone_test tombstone_split` runs the tests whose names contain the filter.

## If this is new

- Everything is in the earlier stages of this module and of 2c.

## Tests

- Long random runs for every buffer size agree with a set and keep every rule.
- BusTub's four tombstone scenarios, its sequential edge mix with tombstones, and a random workload for every buffer size.

## Hints

### A scenario test fails

Print `leaves_t(&tree)` after every call of the scenario and compare it with your reading of the rules: the stage tests of 2d-01 to 2d-03 render the same scenarios as text, so the first difference names the rule.

### A long run fails for one buffer size only

Re-run that size and seed and stop a few steps before the failure. A rule that only `TOMBS = 1` breaks is usually a buffer that is full after a single delete: a borrow that must drop the pair, or a merge that must retire the oldest.

## Performance

The random runs take seconds in debug mode because the shape checker walks the whole tree every 25 steps. Run them with `--release` to see how much of that is the checker and how much your tree.

## Experiment

Optional. Write your prediction down first, then run.

1. **A big buffer.** Run the long runs with `TOMBS = 16` (add the instantiation). Do leaves merge more or less often, and how many dead pairs does a scan step over at the end?
2. **Tombstone-heavy workload.** Delete 95% of a tree's keys and compare scan time with `TOMBS = 0` and `TOMBS = 3`.

## Other designs

None for this stage. The *Other designs* sections of 2d-01 to 2d-03 list the alternatives to compare with yours.

## In BusTub

The tombstone tests are the optional extension of Project 2 in recent years; passing them is the evidence that your index handles logical deletes through every structural operation.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `TEST(BPlusTreeTombstoneTest, TombstoneBasicTest)` | `#[test] fn tombstone_basic_test()` |
| a test instantiated per `NumTombs` | a const-generic function called for each size |
| `GetTombstones()` on a leaf page | `tree.leaf_tombstones()` |

**Port rule:** an accessor on a page class becomes an observer on the tree.

## Learn more

- BusTub's tombstone tests in [`b_plus_tree_tombstone_test.cpp`](https://github.com/cmu-db/bustub/tree/master/test/storage) · [Const generics](https://doc.rust-lang.org/reference/items/generics.html#const-generics)
