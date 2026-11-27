A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`prefix_end(prefix)` in `src/storage/index/prefix_range.rs`: the smallest byte string that is greater than **every** string that starts with `prefix`, or `None` if there is none (the prefix is empty or all `0xFF`). With it, `LIKE 'abc%'` becomes the range `["abc", prefix_end("abc"))` on an ordered index.

## Why

A B+ tree answers ranges, not patterns. Turning a string prefix into a range is how `LIKE 'abc%'`, a path prefix or a tuple prefix of a composite key becomes an index scan. The calculation is a few lines and has one famous edge: `0xFF` bytes carry.

## The contract

- `prefix_end(p)` strips trailing `0xFF` bytes and adds one to the last remaining byte; `None` when nothing remains.
- Byte strings compare lexicographically.

## Invariants

These must hold after every step, whatever the input:

- `prefix <= prefix_end(prefix)` (when it exists) and they are not equal.
- `prefix_end` is never itself a string with that prefix.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A key `k` starts with `p` exactly when `p <= k` and (`prefix_end(p)` is `None` or `k < prefix_end(p)`).
- `prefix_end` is the *smallest* such bound: nothing strictly between the prefixed strings and it starts with `p`.
- A longer prefix gives a bound no larger than a shorter one's.

## Examples

Worked cases (the tests include them):

```text
"abc" -> "abd"
[0x61, 0xFF] -> [0x62]
[0xFF, 0xFF] -> None
[] -> None
```

## What the tests check

- Ordinary prefixes, carries and the unbounded cases.
- A property: membership in the range equals `starts_with`.

## Done when

All the `s2c_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [One key space](/cookbook/p/one-key-space): prefix_end: the smallest key above a prefix, used for prefix range scans.
