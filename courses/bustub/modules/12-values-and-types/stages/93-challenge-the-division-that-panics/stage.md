A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/types/int_ops.rs` has the checked integer operations of a SQL engine: they return an error instead of wrapping or panicking. One of them still panics on a value a user can send. Find it with the tests and fix it.

## Why

A query engine must never crash on data: every statement a user can type returns a result or an error. Rust panics on integer overflow in debug builds and wraps in release, and both are wrong for a database. Finding which operation is the exercise.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Division: `i32::MIN / -1` (`SELECT -2147483648 / -1`) is the one integer division that overflows, and plain `/` panics on it. `checked_div` turns it into an error.||

## The contract

- `checked_add_i32`, `checked_neg_i32`, `checked_abs_i32`, `checked_div_i32` return `Ok(result)` or `Err(IntOpError::Overflow)`, and `Err(IntOpError::DivisionByZero)` for a zero divisor.
- No operation ever panics, whatever the arguments.

## Invariants

These must hold after every step, whatever the input:

- A result that is `Ok` is the mathematical result.
- Overflow is reported exactly when the mathematical result does not fit in `i32`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `neg(neg(x)) == x` whenever the first negation succeeds.
- `abs(x) >= 0` whenever it succeeds.
- `div(a, b) * b + (a % b) == a` whenever `b != 0` and the division succeeds.

## Examples

Worked cases (the tests include them):

```text
div(MIN, -1) -> Overflow
div(5, 0) -> DivisionByZero
abs(MIN) -> Overflow
div(-7, 2) -> -3
```

## What the tests check

- Ordinary values.
- Every extreme: `i32::MIN` and `i32::MAX` with `0`, `1` and `-1`.
- A property against 64-bit arithmetic.

## Done when

All the `s3a_c4` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Integer methods look-up page](/cookbook/p/std-integer-methods-overflow): checked_div, and why MIN / -1 panics in every build.
