A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`Histogram` in `src/optimizer/histogram.rs`: an equi-width histogram of integer values: `build(values, buckets)` divides `[min, max]` into `buckets` equal-width ranges and counts the values in each; `estimate_le(x)` estimates how many values are `<= x`, assuming values are spread evenly **inside** a bucket; `estimate_range(lo, hi)` how many are in `lo..=hi`.

## Why

The optimiser cannot count rows to decide how to run a query: it has a few kilobytes of statistics per column. A histogram turns `WHERE age BETWEEN 30 AND 40` into a row estimate, and that estimate decides between an index scan and a table scan. The interesting question is how wrong it can be, and the answer is bounded and testable.

## The contract

- Buckets cover `[min, max]` in equal widths (the last bucket includes `max`); `n` is the number of values.
- `estimate_le(x)` is 0 below `min`, `n` at or above `max`, and between them the counts of the buckets entirely below `x` plus a **linear share** of the bucket containing `x`.
- `estimate_range(lo, hi)` is `estimate_le(hi) - estimate_le(lo - 1)` (0 if `hi < lo`).

## Invariants

These must hold after every step, whatever the input:

- The bucket counts add up to `n`.
- `estimate_le` is non-decreasing in `x` and between 0 and `n`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- At a bucket boundary, the estimate is exact for values that fall on the boundaries.
- The estimate of `<= x` is within one bucket's count of the true count.
- `estimate_range(a, b) + estimate_le(a - 1) == estimate_le(b)`.

## Examples

Worked cases (the tests include them):

```text
values 0..100, 10 buckets: estimate_le(49) = 50 (exact), estimate_le(54) is about 55
```

## What the tests check

- Exact counts at boundaries and the edges.
- Monotonicity.
- The error bound against true counts.

## Done when

All the `s3h_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Optimizer statistics](/cookbook/p/statistics-histograms-sketches): equi-width and other histograms, and the uniformity-within-bucket assumption.
- [Statistics](/cookbook/p/statistics): equi-width histograms built and measured with q-error on skewed data.
