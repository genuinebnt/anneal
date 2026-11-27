A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`TopN` in `src/execution/top_n.rs`: `ORDER BY key LIMIT n` without sorting everything. `push(key, payload)` offers a row; the structure never holds more than `n` rows; `finish()` returns the `n` smallest by key (ties by arrival: earlier rows win), in key order.

## Why

A full sort of a hundred million rows to return ten is the classic way to make a query slow and out of memory. A bounded max-heap of size `n` reads each row once, throws almost all of them away at once, and needs `O(n)` memory. The tie-break matters because `ORDER BY` without a total key must still be deterministic.

## The contract

- `TopN::new(n)`; `push(key, payload)` returns nothing; `len()` is the number of rows held (at most `n`).
- `finish()` returns `(key, payload)` for the `n` smallest keys, ascending by key; among equal keys, rows that arrived earlier come first. With `n == 0` it returns nothing.

## Invariants

These must hold after every step, whatever the input:

- `len() <= n` at all times.
- The rows held are always the `n` smallest seen so far (ties: earliest).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The result equals stable-sorting everything by key and taking `n`.
- Pushing a row larger than everything held when `len() == n` changes nothing.
- The result does not depend on how the stream is split across `push` calls.

## Examples

Worked cases (the tests include them):

```text
n = 2; push (5,a) (1,b) (5,c) (1,d) -> [(1,b), (1,d)]
```

## What the tests check

- Small cases; `n = 0`; ties.
- The bound on `len()`.
- A property against a stable sort.

## Done when

All the `s3g_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Sort, top-N, limit, window](/cookbook/p/sort-top-n-limit-window): a top-N heap executor for ORDER BY ... LIMIT.
- [Sorting and aggregation](/cookbook/p/sorting-and-aggregation): top-N with a heap, including how ties are handled.
