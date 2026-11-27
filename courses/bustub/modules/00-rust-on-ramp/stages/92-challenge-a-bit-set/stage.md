A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`BitSet` in `src/rust_primer/bits.rs`: a fixed-capacity set of small integers stored one bit each in `u64` words, with `set`, `clear`, `test`, `count`, an ascending iterator, `first_clear`, and `union_with`.

## Why

Free-space maps, null bitmaps, visibility maps and Bloom filters are all bit sets. A bit set uses an eighth of the memory of a `Vec<bool>` and does set operations a word at a time, and the details (the last word, indexes past the end) are where the bugs are.

## The contract

- `new(capacity)` holds indexes `0..capacity`; every bit starts clear.
- `set(i)` and `clear(i)` return true if the bit changed. An index at or past the capacity changes nothing and returns false; `test` of such an index is false.
- `count()` is the number of set bits; `iter()` yields set indexes in increasing order; `first_clear()` is the smallest index below the capacity that is clear.
- `union_with(&other)` sets every bit that is set in `other` (indexes of `other` past this capacity are ignored).

## Invariants

These must hold after every step, whatever the input:

- `count()` equals the length of `iter()`.
- No bit at or past the capacity is ever set (even in the unused part of the last word).
- `iter()` is strictly increasing.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `set(i)` then `test(i)` is true; `clear(i)` then `test(i)` is false.
- `set` twice reports a change only the first time.
- `union_with` equals setting each index of the other set.

## Examples

Worked cases (the tests include them):

```text
capacity 70: set(0), set(63), set(64), set(69) -> count 4, iter [0, 63, 64, 69]
set(70) -> false, count unchanged
first_clear on a full set -> None
```

## What the tests check

- Bits across word boundaries (63, 64) and at the capacity edge.
- Out-of-range indexes change nothing.
- `first_clear` on empty, partly full and full sets.
- A property against a `BTreeSet`.

## Done when

All the `sr_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Bitset rank select roaring](/cookbook/p/bitset-rank-select-roaring): a u64-word bit set with word-at-a-time set algebra and trailing_zeros iteration.
- [Bit-twiddling toolbox](/cookbook/p/bit-tricks): the shift, mask and lowest-set-bit tricks a bit set is built from.
