A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`radix_sort` in `src/execution/radix_sort.rs`: sort `u32` keys (each with a `u32` payload) with a least-significant-digit radix sort, 8 bits per pass, **stable**: rows with equal keys keep their input order.

## Why

Comparison sorts cannot beat `n log n`; sorting fixed-width integers by digit takes four linear passes for `u32`, and is what high-throughput engines (and GPU sorts) use for integer keys. Stability is what makes it composable: sort by the least significant column first and the result is a multi-column sort.

## The contract

- `radix_sort(rows)` sorts `(key, payload)` ascending by key, stably, in place or by returning a new vector.
- Each pass is a counting sort on one byte of the key, least significant byte first.

## Invariants

These must hold after every step, whatever the input:

- The output is a permutation of the input.
- Keys are non-decreasing; payloads of equal keys are in input order.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The result equals the standard library's stable sort by key.
- Sorting twice changes nothing.
- Sorting by a second key first and then by the main key (stable) gives a lexicographic order.

## Examples

Worked cases (the tests include them):

```text
[(3,a),(1,b),(3,c),(2,d)] -> [(1,b),(2,d),(3,a),(3,c)]
```

## What the tests check

- Small inputs and equal keys.
- Keys that differ only in a high byte.
- A property against `sort_by_key`.

## Done when

All the `s3g_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Parallel sample sort and parallel LSD/MSD radix sort](/cookbook/p/parallel-sample-sort-radix): LSD radix sort by per-digit histogram and prefix-sum scatter.
- [Sorting and searching toolkit, differential-tested against std](/cookbook/p/sorting-searching): LSD radix and counting sort tested against std's sort.
