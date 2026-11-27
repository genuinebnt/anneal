A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`stream_group_sums` in `src/execution/stream_group.rs`: given rows `(key, value)` **sorted by key**, return one `(key, sum, count)` per group, in key order, using memory for one group only. If the keys ever decrease, return `Err(NotSorted { at })` with the index of the first row that is out of order.

## Why

When the input is already sorted (an index scan, the output of a sort or of a merge join), grouping does not need a hash table: a group ends when the key changes. It uses constant memory and can start emitting at once. The price is a precondition, and a good implementation checks it rather than quietly returning wrong groups.

## The contract

- Equal keys must be adjacent; a key that decreases is an error at that row's index.
- The output has one entry per distinct key, in input order.
- Sums wrap on overflow.

## Invariants

These must hold after every step, whatever the input:

- The counts add up to the number of rows; each key appears once in the output.
- Output keys are strictly increasing.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The result equals a hash group-by followed by sorting by key.
- Splitting the input between two groups and grouping each part gives the same entries (apart from the shared key).
- An unsorted input is reported at its first decrease, never grouped.

## Examples

Worked cases (the tests include them):

```text
[(1,5),(1,7),(2,1)] -> [(1,12,2),(2,1,1)]
[(2,1),(1,1)] -> NotSorted { at: 1 }
```

## What the tests check

- Groups of various sizes.
- Unsorted input.
- A property against a hash group-by.

## Done when

All the `s3f_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Sorting and aggregation](/cookbook/p/sorting-and-aggregation): hash versus sort aggregation, and when sorted input makes grouping cheap.
- [External sort hash aggregation](/cookbook/p/external-sort-hash-aggregation): sort-based aggregation that combines neighbours and keeps output sorted.
