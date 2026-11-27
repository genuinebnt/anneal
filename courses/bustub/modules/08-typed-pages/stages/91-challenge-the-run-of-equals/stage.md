A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/storage/page/bound_search.rs` has `lower_bound` and `upper_bound` for a sorted slice that may hold duplicates: the first position a key could be inserted at, and the first position after all its copies. It looks right, and it has one bug. Find it and fix it.

## Why

Binary search is the most commonly mis-written algorithm there is, and nearly every mistake is in the boundary: strict or not, which half to keep. Duplicates are where it shows, and a B+ tree with non-unique keys depends on both bounds being exactly right.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`upper_bound` compared with `<` where it needs `<=`: it stopped at the first copy of the key instead of after the last, so with duplicates it returned the lower bound.||

## The contract

- `lower_bound(s, k)` is the first index `i` with `s[i] >= k` (or `len`).
- `upper_bound(s, k)` is the first index `i` with `s[i] > k` (or `len`).
- `count_of` is the difference.

## Invariants

These must hold after every step, whatever the input:

- `lower_bound <= upper_bound <= len`.
- Every element before `lower_bound` is `< k`; every element from `upper_bound` on is `> k`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `upper_bound(s, k) == lower_bound(s, k + 1)`.
- A key that is absent has equal bounds.
- Both equal `partition_point` with `<` and `<=`.

## Examples

Worked cases (the tests include them):

```text
[1,3,3,3,7], key 3 -> lower 1, upper 4
[1,3,3,7], key 5 -> 3, 3
[5;9], key 5 -> 0, 9
```

## What the tests check

- The bounds of a run of equal keys.
- Absent keys, empty and all-equal slices.
- Negative keys and the extremes of `i64`.
- A property against `partition_point`.

## Done when

All the `s2a_c2` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Sorting and searching toolkit, differential-tested against std](/cookbook/p/sorting-searching): lower_bound and upper_bound tested against std's partition_point.
- [Slice look-up page](/cookbook/p/std-slice-search-sort-binary): binary_search and partition_point on slices with duplicates.
