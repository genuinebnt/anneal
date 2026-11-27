A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`RankedSet` in `src/storage/page/ranked_array.rs`: a set of `u64`s that answers `rank(x)` (how many are smaller) and `select(i)` (the i-th smallest) without looking at every number, plus `count_range(lo, hi)`.

## Why

"How many keys are below this one?" is the question behind `OFFSET`, percentiles, and range-count estimates in an optimiser. A sorted array answers it with the binary search you wrote for pages; a B+ tree can answer it when each inner entry also remembers how many keys lie below.

## The contract

- `insert` and `remove` report whether the set changed.
- `rank(x)` counts the numbers strictly smaller than `x`, whether or not `x` is in the set.
- `select(i)` is the i-th smallest (0 is the smallest) or `None`.
- `count_range(lo, hi)` counts numbers in `lo..hi`, 0 when `hi <= lo`.

## Invariants

These must hold after every step, whatever the input:

- The numbers are always in strictly increasing order, no duplicates.
- `len()` equals the number of successful inserts minus removes.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `select(rank(x)) == Some(x)` for every `x` in the set.
- `rank(select(i).unwrap()) == i` for every `i < len`.
- `count_range(a, b) == rank(b) - rank(a)` for `a < b`.
- Inserting `x` raises `rank(y)` by one exactly for `y > x`.

## Examples

Worked cases (the tests include them):

```text
{10,20,30,40}: rank(15) = 1, rank(10) = 0, rank(41) = 4
select(3) = 40, select(4) = None
count_range(20, 50) = 3
```

## What the tests check

- Rank and select on small sets, and that they are inverses.
- Duplicate inserts, missing removes.
- Half-open ranges and reversed ranges.
- 200 000 ranks over 20 000 numbers finish quickly.
- A property against a `BTreeSet`.

## Done when

All the `s2a_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Sorting and searching toolkit, differential-tested against std](/cookbook/p/sorting-searching): lower and upper bound, and quickselect for the k-th smallest.
- [A split/merge treap with pluggable augmentation](/cookbook/p/treap-interval): order statistics (rank and k-th) from subtree sizes in a balanced tree.
