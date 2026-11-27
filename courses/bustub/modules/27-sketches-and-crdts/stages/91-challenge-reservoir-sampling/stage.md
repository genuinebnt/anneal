A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`Reservoir` in `src/primer/reservoir.rs`: keep a uniform random sample of `k` items from a stream you see once and whose length you do not know (Algorithm R). The first `k` items fill the reservoir; item number `i` (1-based) after that replaces a uniformly random slot with probability `k / i`. A seeded generator (given) makes runs reproducible.

## Why

`ORDER BY random() LIMIT k` sorts everything; reservoir sampling reads the data once and holds `k` items, which is how `TABLESAMPLE`, statistics collection (`ANALYZE`) and log sampling work. The remarkable part is the claim that every item ends up in the sample with the *same* probability `k / n`: a statement a test can check by repetition.

## The contract

- `Reservoir::new(k, seed)`; `offer(item)`; `sample()` returns the current items (any order); `seen()` is the number offered.
- The reservoir never holds more than `k` items, and holds `min(k, seen)`.
- Item `i > k` replaces slot `rng % i` if that is below `k` (Algorithm R); the generator is `XorShift` (given).

## Invariants

These must hold after every step, whatever the input:

- `sample().len() == min(k, seen())`.
- Every sampled item was offered, and an item appears at most as many times as it was offered.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A stream no longer than `k` is returned whole.
- The same seed and stream give the same sample.
- Across many runs, each of `n` items is in the sample about `k / n` of the time.

## Examples

Worked cases (the tests include them):

```text
k 3, stream 1..=3 -> {1,2,3}
k 1, 10 items, 20 000 runs: each item about 2 000 times
```

## What the tests check

- Small streams, size bounds, determinism.
- Uniformity over many seeded runs.
- A property over streams.

## Done when

All the `s0d_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Sampling](/cookbook/p/sampling): reservoir sampling (Algorithm R) and weighted variants.
