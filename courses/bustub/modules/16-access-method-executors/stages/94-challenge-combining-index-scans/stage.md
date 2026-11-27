A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`union_sorted`, `intersect_sorted` and `difference_sorted` in `src/execution/rid_sets.rs`: given row-id lists that are each **sorted and without duplicates**, produce the union, intersection and difference as sorted, duplicate-free lists in one linear pass (no sorting, no hashing).

## Why

`WHERE a = 4 OR b = 9` can be answered by two index scans whose results are combined, and `AND` by intersecting them. A row that satisfies both sides of an OR must appear **once**: returning it twice was a real bug in this course's own index scan, found by a test with `v1 = 4 OR v1 = 4`. The combination of sorted lists is the merge step of a merge join, small and exact.

## The contract

- Inputs are strictly increasing.
- `union_sorted(a, b)`: every id in either, once. `intersect_sorted(a, b)`: ids in both. `difference_sorted(a, b)`: ids in `a` and not in `b`.
- Each returns a strictly increasing list and looks at each input element once.

## Invariants

These must hold after every step, whatever the input:

- Outputs are strictly increasing.
- `|union| + |intersection| == |a| + |b|`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `union(a, a) == a`, `intersect(a, a) == a`, `difference(a, a) == []`.
- `difference(a, b) ∪ intersect(a, b) == a`.
- All three equal the corresponding `BTreeSet` operation.

## Examples

Worked cases (the tests include them):

```text
a = [1,3,5], b = [3,4,5,6]: union [1,3,4,5,6]; intersect [3,5]; difference [1]
```

## What the tests check

- Small lists, one empty, identical lists.
- A property against `BTreeSet`.

## Done when

All the `s3e_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Query execution](/cookbook/p/query-execution): multi-index scans: computing RID sets per index, then intersecting or uniting them.
- [A mini full-text search engine](/cookbook/p/search-engine): linear intersection of sorted posting lists for AND queries.
