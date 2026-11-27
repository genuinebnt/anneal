A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`parse_sql_int` in `src/types/int_parse.rs`: turn text into an `i64` the way a `CAST('...' AS BIGINT)` should: surrounding ASCII whitespace is ignored, an optional `+` or `-`, then one or more decimal digits and nothing else. Every other input is an error that says which kind.

## Why

`str::parse::<i64>` is close, but a database must decide the edges on purpose: is `" 7 "` a number, is `"+5"`, is `"1_000"`, is `"9223372036854775808"`? Casts run on user data, so every one of these happens, and the error kind decides whether the query fails or the row is skipped.

## The contract

- `Ok(value)` for `[ws][+-]digits[ws]` that fits in `i64` (including `-9223372036854775808`).
- `Err(Empty)` for an empty or all-whitespace string, `Err(Invalid)` for anything with another character (a sign alone, `1_0`, `0x1`, `1.0`, internal spaces), `Err(OutOfRange)` for valid digits that do not fit.

## Invariants

These must hold after every step, whatever the input:

- Every `i64` printed with `to_string()` parses back to itself.
- The function never panics.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding surrounding whitespace never changes the result.
- `parse("+" + digits) == parse(digits)` for non-negative values.
- A value that does not fit is `OutOfRange`, never a wrapped number.

## Examples

Worked cases (the tests include them):

```text
"  42 " -> 42
"-9223372036854775808" -> MIN
"9223372036854775808" -> OutOfRange
"4 2" -> Invalid
"+" -> Invalid
"" -> Empty
```

## What the tests check

- Each kind of input.
- The edges of `i64`.
- A property against `str::parse` on well-formed input and against 128-bit arithmetic.

## Done when

All the `s3a_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Number parsing and formatting end to end](/cookbook/p/std-num-parsing-formatting): FromStr and IntErrorKind: naming every way an integer parse fails.
