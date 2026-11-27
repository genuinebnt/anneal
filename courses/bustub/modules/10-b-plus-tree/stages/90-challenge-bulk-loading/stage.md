A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`leaf_sizes` and `level_widths` in `src/storage/index/bulk_layout.rs`: when all keys are known and sorted, a B+ tree can be built bottom-up in one pass instead of by `n` inserts and the splits they cause. This is the planning half: how many keys go in each leaf, and how many nodes each level has.

## Why

Building an index over an existing table (`CREATE INDEX`) is the common case, and inserting a million keys one at a time does a million root-to-leaf descents and a hundred thousand splits. A bulk load writes each page once, full. The layout rules are small and easy to get subtly wrong at the right-hand edge.

## The contract

- Leaves hold at most `max` keys; every leaf except a single leaf holds at least `max / 2`.
- The leaves are as few as possible and full from the left; if the last would be too small, the last two share their keys, the second to last getting the extra one.
- `level_widths` is the node count of each level from the leaves to the root; an inner node has at most `fanout` children.

## Invariants

These must hold after every step, whatever the input:

- The sizes add up to `n`.
- Every size is between 1 and `max`, and at least `max / 2` unless there is one leaf.
- The last level has exactly one node.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- More keys never need fewer leaves.
- `leaf_sizes(n).len() == ceil(n / max)`.
- Each level has `ceil(children / fanout)` nodes.

## Examples

Worked cases (the tests include them):

```text
leaf_sizes(9, 4) = [4, 3, 2]
leaf_sizes(5, 4) = [3, 2]
level_widths(17, 4, 4) = [5, 2, 1]
level_widths(1000, 10, 5) = [100, 20, 4, 1]
```

## What the tests check

- Exact plans for small cases and the uneven right edge.
- Levels up to a single root.
- A million keys planned without building anything.
- Properties: valid and minimal plans; shrinking levels.

## Done when

All the `s2c_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Production B+tree features](/cookbook/p/btree-bulk-load-prefix-compression): bulk loading a B+ tree from sorted input, leaf by leaf.
- [Search and insert](/cookbook/p/search-and-insert): BusTub's B+ tree insert path, with a section on bulk loading.
