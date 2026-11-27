A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`like` in `src/types/like_match.rs`: SQL's `text LIKE pattern`. `%` matches any run of characters (including none), `_` matches exactly one character, and an optional **escape** character makes the next pattern character literal. A pattern that ends with the escape is an error.

## Why

`LIKE` is in nearly every report query, and a naive recursive matcher takes exponential time on `%a%a%a%a%b` against a long string of `a`s: a denial of service sent as a search term. A two-pointer matcher with one backtrack point is linear in the common case and polynomial always.

## The contract

- Characters are Unicode scalar values (`_` matches one `char`, not one byte).
- `like(text, pattern, escape)` is `Ok(true/false)`, or `Err(TrailingEscape)` when the pattern ends with an unescaped escape character.
- Matching is over the whole text, case-sensitive.

## Invariants

These must hold after every step, whatever the input:

- The result does not depend on how the pattern is written when the meaning is the same (`%%` is `%`).
- A pattern with no wildcards matches only itself.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `text LIKE '%'` is true for every text.
- `text LIKE text` (with wildcards in `text` escaped) is true.
- Replacing a `%` by any substring of the text keeps a match a match.
- Linear time on `%a%a%a...%b` against a long run of `a`.

## Examples

Worked cases (the tests include them):

```text
'hello' LIKE 'h%o' -> true
'hello' LIKE 'h_llo' -> true
'100%' LIKE '100\\%' ESCAPE '\\' -> true
'ab' LIKE 'a\\' ESCAPE '\\' -> error
```

## What the tests check

- Wildcards, literals, escapes and the error.
- Unicode text.
- A 20 000-character adversarial case finishes quickly.
- A property against a brute-force recursive matcher.

## Done when

All the `s3a_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [SQL front end](/cookbook/p/sql-front-end): SQL LIKE with escapes, matched without exponential backtracking.
- [A regex engine](/cookbook/p/regex-nfa-engine): why backtracking matchers go exponential, and a linear-time alternative.
