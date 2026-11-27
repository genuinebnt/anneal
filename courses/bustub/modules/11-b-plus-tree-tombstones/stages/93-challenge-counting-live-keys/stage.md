A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`LiveCounts` in `src/storage/index/live_counts.rs`: per-leaf counts of live keys with `O(log n)` operations: `add(leaf, delta)` when a key is inserted or tombstoned, `prefix(i)` (live keys in leaves before `i`), `range(l, r)`, and `find(k)` (which leaf holds the k-th live key, counting from 0, and how far into that leaf).

## Why

`OFFSET 100000` and `COUNT(*) WHERE key BETWEEN a AND b` on a tree full of tombstones should not walk every leaf. If each inner entry (or a Fenwick tree beside the leaves) knows how many live keys lie below it, both become a logarithmic descent. This is that structure, free of the tree around it.

## The contract

- `new(counts)` starts from the live count of each leaf.
- `add(leaf, delta)` changes a leaf's count (delta may be negative; a count never goes below 0).
- `prefix(i)` is the sum of counts of leaves `0..i`; `range(l, r)` of leaves `l..r`; `total()`.
- `find(k)` is `Some((leaf, offset))` such that `prefix(leaf) + offset == k` and `offset < count(leaf)`, or `None` if `k >= total()`.

## Invariants

These must hold after every step, whatever the input:

- `prefix(i + 1) - prefix(i) == count(i)`.
- `prefix(n) == total()`.
- `find` and `prefix` are consistent: `find(prefix(i) + j) == Some((i, j))` when `j < count(i)`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- After `add(i, d)`, `prefix(j)` changes by `d` exactly for `j > i`.
- `find` is monotone: a larger `k` never lands in an earlier leaf.
- Empty leaves are never returned by `find`.

## Examples

Worked cases (the tests include them):

```text
counts [2,0,3]: prefix(3) = 5; find(2) = (2, 0); find(5) = None
add(1, 2) -> counts [2,2,3]; find(2) = (1, 0)
```

## What the tests check

- Counts, prefixes and ranges.
- Finding the k-th live key, skipping empty leaves.
- A property against recomputing sums from scratch.

## Done when

All the `s2d_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Segment fenwick](/cookbook/p/segment-fenwick): a Fenwick tree with prefix sums and a k-th element search.
