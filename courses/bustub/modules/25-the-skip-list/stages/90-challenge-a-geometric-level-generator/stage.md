A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`LevelGenerator` in `src/primer/level_gen.rs`: the height draw of a skip list, on its own. `next_level()` returns `1 + (the number of consecutive successes)` where each trial succeeds with probability `1 / branching` (`branching` of 2 is a fair coin, 4 is what BusTub uses), never more than `max_level`. Driven by a seeded xorshift generator (given), so a seed always gives the same sequence.

## Why

A skip list is balanced by chance, and the *distribution* of heights is what makes it fast: half the nodes only at the bottom, a quarter one level up, and so on. A generator that is off by one level (taller towers, or never reaching the cap) leaves the list correct and slow, and nothing but a statistical test shows it.

## The contract

- `LevelGenerator::new(seed, branching, max_level)`; `branching >= 2`, `max_level >= 1`.
- `next_level()` is in `1..=max_level`. It makes trials until one fails or `max_level` is reached; each trial succeeds when the next random value is divisible by `branching`.
- The same seed gives the same sequence.

## Invariants

These must hold after every step, whatever the input:

- Every level is between 1 and `max_level`.
- The generator is deterministic given its seed.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- About `1 / branching` of the draws are at least 2, about `1 / branching^2` at least 3.
- With `max_level = 1` every draw is 1.
- Two generators with the same seed produce the same draws; different seeds almost always differ.

## Examples

Worked cases (the tests include them):

```text
max_level 1 -> always 1
branching 2, 20 000 draws: about 10 000 are >= 2
```

## What the tests check

- The bounds and determinism.
- The tail of the distribution over many draws.
- A property over seeds and parameters.

## Done when

All the `s0b_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Skip list](/cookbook/p/skiplist): random tower heights with promotion probability p, capped at a max level.
- [A PRNG zoo](/cookbook/p/prng-zoo): seeded xorshift and other generators, and how to test them.
