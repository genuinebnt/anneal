A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`KMerge` in `src/storage/index/kmerge.rs`: an iterator that merges any number of sorted runs into one sorted stream, lazily (it never builds the whole output), yielding `(key, run_index)`. Among equal keys, the lower run index comes first.

## Why

Merging sorted inputs is under a range scan over several indexes, under the merge half of an external sort, under a compaction in an LSM tree and under a `UNION ALL ... ORDER BY`. A heap makes it `O(log k)` per item, and the stability rule is what makes results deterministic.

## The contract

- `KMerge::new(runs)` takes `Vec<Vec<i64>>`, each run sorted ascending.
- `next()` yields the smallest remaining key and the index of the run it came from; ties go to the lower run index, and within a run in order.
- It does not copy all runs into one vector and sort.

## Invariants

These must hold after every step, whatever the input:

- The output is sorted by (key, run index).
- Every input element is yielded exactly once.
- Each run's elements come out in their original order.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The output equals concatenating all runs and stable-sorting by key.
- Adding an empty run changes nothing.
- Merging the output of two merges equals merging all four runs.

## Examples

Worked cases (the tests include them):

```text
[[1,4],[2,4]] -> (1,0) (2,1) (4,0) (4,1)
```

## What the tests check

- Small merges and ties.
- Empty runs and no runs.
- A property against a stable sort.

## Done when

All the `s2c_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [The k-way merge family](/cookbook/p/kway-merge-family): a lazy heap-based k-way merge, stable on ties by source index.
