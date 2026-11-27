A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`random_height` in `src/primer/height.rs` draws a node's height from a source of coin flips and must never exceed `max`. It works in nearly every run, and once in a while it returns `max + 1` (and a list built on it indexes past its tower array). Find the bug and fix it.

## Why

The loop that draws a height is half the code of a skip list, and the half most often written with its bound wrong. The failure is rare (at 14 levels and a fair coin, about once in sixteen thousand nodes), and a test that controls the coin makes it a certainty.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||The bound was checked as `level <= max`, so a run of heads could reach `max + 1`. It must be `level < max`, checked before flipping.||

## The contract

- `random_height(flip, max)`: start at 1; while `flip()` returns true **and** the height is below `max`, add one. The result is in `1..=max`.
- `flip` is called at most `max` times, and not at all when `max == 1`.

## Invariants

These must hold after every step, whatever the input:

- The height is never above `max`.
- The height is `1 + the number of leading heads`, capped at `max`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- With a coin that is always heads the result is exactly `max`.
- With a coin that is always tails the result is 1 and `flip` was called once.
- `flip` is never called after the cap is reached.

## Examples

Worked cases (the tests include them):

```text
max 4, always heads -> 4
max 4, H H T -> 3
max 1, any coin -> 1 with no flips
```

## What the tests check

- Always heads, always tails and mixed sequences.
- The number of flips.
- A property over all coin sequences.

## Done when

All the `s0b_c4` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Skip list](/cookbook/p/skiplist): drawing a tower height with a hard MAX_LEVEL.
