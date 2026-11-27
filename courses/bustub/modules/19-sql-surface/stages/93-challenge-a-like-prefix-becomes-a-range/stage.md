A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`prefix_range` in `src/optimizer/like_prefix.rs`: given a LIKE pattern, return what an optimizer can use: nothing (the pattern starts with a wildcard), an exact string (no wildcards at all), or a range `[prefix, upper)` that contains every string the pattern can match, together with whether the matches still have to be checked against the pattern afterwards.

## Why

`name LIKE 'abc%'` is not a scan in a good engine: it is the range `name >= 'abc' AND name < 'abd'` on a B+ tree, plus nothing to check. `LIKE '%abc'` is a full scan whatever you do. Telling the two apart, and computing `'abd'`, is a small exercise with two traps: escapes (`\\%` is a literal) and the last character (what comes after `z`, and after the largest character there is).

## The contract

- `Everything`: the pattern starts with `%` or `_` (or is empty after escapes are resolved to nothing): no usable prefix.
- `Exact(s)`: the pattern has no wildcard: `s` is the pattern with escapes resolved.
- `Range { prefix, upper, recheck }`: the literal prefix before the first wildcard; `upper` is the smallest string greater than every string that starts with `prefix` (increase the last character; a character that cannot be increased is dropped and the one before it is increased; `None` if nothing is left to increase); `recheck` is false only when the pattern is exactly the prefix followed by one `%`.
- A backslash makes the next character literal, also inside the prefix.

## Invariants

These must hold after every step, whatever the input:

- Every string that matches the pattern is `>= prefix` and `< upper` (when there is one).
- `upper` has no string between it and the strings that start with `prefix`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `abc%` and `abc%%` have the same range; `abc%d` has the same range and `recheck` true.
- The range of `a\\%b%` is the prefix `a%b`.
- A longer prefix gives a range inside the shorter one's.

## Examples

Worked cases (the tests include them):

```text
`abc%` -> Range { prefix: "abc", upper: Some("abd"), recheck: false }
`%abc` -> Everything
`abc` -> Exact("abc")
`ab_` -> Range { prefix: "ab", upper: Some("ac"), recheck: true }
```

## What the tests check

- Prefix, exact and no-prefix patterns.
- Escapes in the prefix.
- The upper bound at the end of the alphabet.
- A property against the matcher.

## Done when

All the `s3i_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [One key space](/cookbook/p/one-key-space): prefix_end: the upper bound of a prefix range scan.
- [SQL front end](/cookbook/p/sql-front-end): LIKE semantics (%, _, escape) that the range must cover.
