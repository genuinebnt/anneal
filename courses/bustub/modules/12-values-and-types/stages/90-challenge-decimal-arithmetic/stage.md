A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`Decimal` in `src/types/decimal.rs`: an exact decimal number with **two** fractional digits (money), stored as a count of hundredths. `parse`, `Display`, checked `add`/`sub`, and `mul`/`div` that round **half away from zero** to two places.

## Why

Floating point cannot hold 0.10 exactly, which is why databases have `DECIMAL`. The type is small, and every detail is a decision a database must make once and apply everywhere: how many digits are accepted, what rounds which way, what happens on overflow and on division by zero.

## The contract

- `Decimal::parse(s)`: optional `+`/`-`, digits, optionally `.` and **one or two** digits; no exponent, no spaces. Anything else is `None`. `-0.00` is zero.
- `Display` prints `[-]int.dd` with exactly two digits.
- `add`, `sub` return `None` on `i128` overflow; `mul` is `a * b / 100` and `div` is `a * 100 / b`, both rounded half away from zero; `div` by zero is `None`.

## Invariants

These must hold after every step, whatever the input:

- `from_cents(c).cents() == c`.
- `parse(&d.to_string()) == Some(d)` for every value.
- Zero has a single representation.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `add` and `mul` are commutative; `a + b - b == a` when nothing overflows.
- `mul` by 1.00 and `div` by 1.00 are the identity.
- Negating the inputs of `mul` or `div` negates (never changes the magnitude of) the rounded result.

## Examples

Worked cases (the tests include them):

```text
"12.5" -> 12.50
"0.1" + "0.2" -> 0.30
1.25 * 1.25 -> 1.56 (1.5625 rounds up)
-1.25 * 1.25 -> -1.56
1.00 / 3.00 -> 0.33; 2.00 / 3.00 -> 0.67
"1.234" -> None
```

## What the tests check

- Parsing and printing.
- Add, sub and overflow.
- Rounding of mul and div, including negatives and exact halves.
- A property against `i128` arithmetic.

## Done when

All the `s3a_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Money done right](/cookbook/p/decimal-fixed-point-money): fixed-point decimals with checked arithmetic and rounding modes, half away from zero included.
