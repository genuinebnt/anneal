A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/execution/set_laws.rs` has the six set operations on lists of integers (`union`, `union_all`, `intersect`, `intersect_all`, `except`, `except_all`), each returning a sorted list. Five of them are right. One returns a duplicate for some inputs and passes most examples. Find the bug and fix it.

## Why

Set operations are specified by counts, and an implementation by sorting and scanning is short enough to look obviously right. The bugs that survive are right for the inputs you thought of, and a property that states the law finds them in a second.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`union` removed duplicates before sorting, and `dedup` removes only *adjacent* equal values, so duplicates that were not next to each other survived. Sort first, then dedup.||

## The contract

- `union_all`: every element of both lists. `union`: every distinct element of either.
- `intersect_all`: each element as often as the smaller of its counts; `intersect`: each element that is in both, once.
- `except_all`: each element as often as its count on the left exceeds its count on the right; `except`: each element that is on the left and not on the right, once.
- Every result is sorted ascending.

## Invariants

These must hold after every step, whatever the input:

- `union`, `intersect` and `except` never contain a duplicate.
- No element appears in a result more often than the operation allows (the counts of the contract).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `union(l, r)` equals the distinct elements of `union_all(l, r)`.
- `union(l, r) == union(r, l)`, and the same for `intersect`.
- `intersect_all` and `except_all` of the same pair add up to the left list.

## Examples

Worked cases (the tests include them):

```text
union([1, 2, 1], [2]) -> [1, 2]
union([3, 1], [2, 3, 1]) -> [1, 2, 3]
except_all([1, 1, 1, 2], [1, 1, 3]) -> [1, 2]
```

## What the tests check

- Hand-picked cases, including unsorted inputs.
- The laws above on random lists.

## Done when

All the `s3j_c3` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Relational model and algebra](/cookbook/p/relational-algebra-engine): UNION ALL, INTERSECT and EXCEPT defined by bag counts (sum, min, monus).
