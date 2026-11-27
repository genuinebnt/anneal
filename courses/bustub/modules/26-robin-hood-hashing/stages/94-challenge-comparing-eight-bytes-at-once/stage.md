A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`match_byte` and `first_match` in `src/primer/swar.rs`: given 8 control bytes packed in a `u64`, `match_byte(group, byte)` returns a mask with the **high bit of each matching byte set** (and no other bit), computed with a handful of arithmetic operations and no loop over bytes. `first_match(mask)` is the index (0..8) of the lowest matching byte, or `None`.

## Why

Modern hash tables (Swiss tables, F14) keep one control byte per slot and compare a whole group of them with one instruction to decide which slots to inspect. With no SIMD available, the same idea works inside one 64-bit register. It is the cleanest example of why bit tricks are not decoration: the loop over eight bytes disappears.

## The contract

- Bytes are numbered from the least significant (byte 0).
- `match_byte(group, b)`: bit `8 * i + 7` is set exactly for the bytes `i` equal to `b`; no other bit is set. No false positives for any input.
- `first_match(mask)`: the index of the first (lowest) set byte, `None` for 0.

## Invariants

These must hold after every step, whatever the input:

- The mask only ever has bits at positions 7, 15, 23, ... 63.
- `first_match(match_byte(g, b))` is the position of the first byte of `g` equal to `b`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `match_byte(g, b)` has as many bits as `g` has bytes equal to `b`.
- `match_byte(g, b) == 0` when `b` does not occur.
- Replacing a byte of `g` by `b` sets exactly its flag.

## Examples

Worked cases (the tests include them):

```text
g = bytes [1, 7, 7, 0, 255, 7, 2, 9]: match_byte(g, 7) flags bytes 1, 2, 5; first_match -> 1
```

## What the tests check

- Matches and non-matches.
- The classic false-positive cases (0x01 next to the target, 0x80, 0xFF).
- A property against a per-byte comparison.

## Done when

All the `s0c_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Bit-twiddling toolbox](/cookbook/p/bit-tricks): SWAR zero-byte detection and other word-at-a-time tricks.
- [Hash tables](/cookbook/p/hash-tables): SwissTable-style control bytes in a linear-probing table, measured.
