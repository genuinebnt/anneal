A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`reduce` in `src/primer/range_reduce.rs`: map a 32-bit hash `h` onto `0..n` as `(h * n) >> 32` (done in 64 bits), the "fastrange" reduction. It needs no division, works for any `n` (not just powers of two), and is monotone in `h`.

## Why

`hash % n` is a division (slow on the critical path of every probe) and uses the **low** bits of the hash, which are the weakest bits of many hash functions. Multiplying and keeping the high bits is faster and uses the strongest bits. The trade-off to understand: results are *buckets of consecutive hash values*, so a bad hash that clusters in value clusters in buckets.

## The contract

- `reduce(h, n)` returns a value `< n` for `n >= 1`; `n == 0` returns 0.
- It is `((h as u64 * n as u64) >> 32) as u32`.
- `reduce_pow2(h, bits)` takes the top `bits` bits (`h >> (32 - bits)`) and equals `reduce(h, 1 << bits)`.

## Invariants

These must hold after every step, whatever the input:

- The result is always below `n`.
- `reduce` is non-decreasing in `h`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `reduce(h, 1)` is 0 and `reduce(u32::MAX, n)` is `n - 1`.
- Each bucket receives a number of consecutive hash values that differs by at most one from the others.
- `reduce(h, n)` for `n` a power of two equals the top bits of `h`.

## Examples

Worked cases (the tests include them):

```text
reduce(0, 10) = 0; reduce(u32::MAX, 10) = 9; reduce(2^31, 10) = 5
```

## What the tests check

- Range and monotonicity.
- Even sharing between buckets.
- Power-of-two equivalence.

## Done when

All the `s0c_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A PRNG zoo](/cookbook/p/prng-zoo): Lemire's multiply-shift for a bounded integer instead of % n.
