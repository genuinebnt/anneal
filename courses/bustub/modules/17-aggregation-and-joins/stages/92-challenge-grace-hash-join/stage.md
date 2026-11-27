A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`partition` and `grace_join` in `src/execution/grace_join.rs`: `partition(rows, k)` splits rows of `(key, payload)` into `k` buckets by a hash of the key (given: `bucket_of(key, k)`); `grace_join(left, right, k)` joins corresponding buckets of the two inputs with an in-memory hash table and returns all pairs of payloads with equal keys.

## Why

When a hash table does not fit in memory, a hash join spills: both inputs are partitioned to disk by the same hash function, and then each pair of partitions (which does fit) is joined on its own. The one fact that makes it work is that rows with equal keys land in the same partition on both sides.

## The contract

- `bucket_of(key, k)` is deterministic and below `k`.
- `partition(rows, k)` returns `k` vectors; row order within a bucket is the input order.
- `grace_join(left, right, k)` returns every `(left payload, right payload)` with equal keys, once, in any order.

## Invariants

These must hold after every step, whatever the input:

- Every input row is in exactly one bucket, the one `bucket_of` names.
- Rows with equal keys are in the same bucket.
- `grace_join` never builds a hash table over more than one bucket's rows at a time.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The result (as a multiset) equals a nested-loop join, for every `k >= 1`.
- Changing `k` changes the buckets but never the result.
- The sizes of the buckets add up to the input size.

## Examples

Worked cases (the tests include them):

```text
k = 3: rows with key 5 on both sides are in the same bucket and joined there
```

## What the tests check

- Partitioning properties.
- Join results for several `k`.
- A property against a nested loop.

## Done when

All the `s3f_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Grace hash join and spilling](/cookbook/p/grace-hash-join-and-spilling): Grace partitioning, joining partition by partition, and recursion on skew.
- [Join algorithms compared](/cookbook/p/join-algorithms-code): Grace hash join tested against the other join algorithms.
