`a + b` in SQL has rules a systems language does not: the two operands may have different types (the result takes the wider), either may be NULL (the result is NULL), the right-hand side may be a string (converted to the left's type), an overflow is an *error* and not a wraparound, and dividing by zero is an error too. This stage is `add`, `subtract`, `multiply`, `divide`, `modulo`, `min`, `max`, `sqrt` and a few helpers, and the test oracle is the simplest possible: do the arithmetic in `i128`, where nothing overflows, and check the result type's range.

> [!CHECK] You compute `i64::MAX * 2` in Rust with `*`. In debug builds it panics, in release it wraps. SQL must do neither: what is the right behaviour, which std method gives it, and why can you not just call `checked_mul` on two `Value`s?
> ||The right behaviour is an `OutOfRange` error returned to the caller, and the query fails. `i64::checked_mul` returns `None` on overflow, which you map to the error. But a `Value` holds one of four integer widths, and the result type is the wider of the two operands, so you cannot call `checked_mul` until you know which width you are in: doing the arithmetic in `i128` (which cannot overflow for 64-bit operands) and then range-checking against the *result type* is simpler than four `checked_mul`s.||
>
> - Which type does `TINYINT + BIGINT` produce?
> - What is `127 + 1` in `TINYINT`: wrap, saturate or error?
> - What does `NULL + 5` give, and of which type?

## The task

- `add`, `subtract`, `multiply`, `divide`, `modulo`: `Result<Value>`.
  - A non-numeric left operand, or an operand pair that cannot be compared, is a `NotImplemented` error.
  - A NULL on either side gives the NULL of the **result type** (`operate_null`): DECIMAL if either operand is DECIMAL, else the wider integer type; a string on the right counts as the left's type.
  - A string on the right is converted (cast) to the left's type first.
  - Dividing or taking a modulo by zero is `DivideByZero` (also when the zero is the string `"0"`).
  - The result type is the wider of the two (`TINYINT < SMALLINT < INTEGER < BIGINT < DECIMAL`).
  - Integer results are exact and must fit the result type (`[MIN + 1, MAX]`), else `OutOfRange`. Integer division truncates toward zero; the remainder takes the sign of the dividend.
  - Decimal arithmetic is `f64`; a decimal modulo is `x - trunc(x / y) * y`.
- `min` and `max`: the smaller/larger by comparison; NULL if either side is NULL.
- `sqrt`: a DECIMAL; NULL for NULL; a negative number is a `Decimal` error.
- `is_zero` and `operate_null` are public helpers (non-numeric: `NotImplemented` / error).

The tests: integer arithmetic is exact or `OutOfRange` against an `i128` oracle for random integers of every width (result type is the wider); division and modulo follow the dividend and `DivideByZero`; commutativity and `(a + b) - b = a`; NULL gives a NULL of the result type; decimals are floats and a decimal makes the result a decimal; `min`/`max`; strings on the right; `sqrt` and `is_zero`.

## Your freedom

Whether you compute in `i128` or use four `checked_*` families, how you share code between the five operators (a private `Op` enum and one function, or a macro), and where the error cases are checked.

## The Rust toolbox

**`i128` as an oracle and as an implementation.** `(x as i128) * (y as i128)` cannot overflow for 64-bit operands, so range-check the exact result: `i64::try_from(exact)` and then the result type's range. The same trick is the test's oracle, so the property holds by construction when your code does the same: write the first version a different way (`checked_mul` per width) if you want the test to cross-check you.

**`checked_add`, `checked_sub`, `checked_mul`, `checked_div`, `checked_rem`.** Each returns an `Option`; `ok_or_else(out_of_range)?` turns it into an error. Note `checked_div` also returns `None` for `MIN / -1`, but here MIN is reserved, so it cannot happen.

**One function, five operators.** `fn arithmetic(&self, other: &Value, op: Op) -> Result<Value>` with `enum Op { Add, Subtract, .. }` and a `match op` inside keeps the checks (types, NULL, zero, result type) in one place; the five public methods are one-liners.

**`Ord` on an enum for "wider".** `#[derive(PartialOrd, Ord)]` on `TypeId` orders variants by declaration, so `a.max(b)` of two integer `TypeId`s is the wider if they are declared narrow to wide; relying on that is clever and fragile, say so in a comment or match explicitly.

**Floats are IEEE.** `x / 0.0` is infinity, not an error; the zero check comes first.

## If this is new

- [D13 Matrix, bits & math](/t/d13-matrix-bits-math): overflow, `checked_*`, `wrapping_*`, `saturating_*`.
- [S1 Option & Result](/t/s1-option-result): `ok_or_else`, `?`.
- L7 Enums & pattern matching: an `Op` enum.
- The optional *overflow and checked arithmetic* concept has the full table.
- L8 Error design: Custom errors: overflow and cast errors as values.

## Tests

- Integer `+ - *` is exact or `OutOfRange` for every pair of widths; the result type is the wider.
- Division and modulo agree with Rust's, and zero divisors are `DivideByZero`.
- Commutativity and `(a + b) - b = a`.
- NULL gives a NULL of the result type.
- Decimals: floating point, decimal result.
- `min`/`max` by comparison, NULL if either is NULL.
- A string on the right is converted; non-numeric operands are refused.
- `sqrt` and `is_zero`.

## Hints

### Order the checks

Types first (is it arithmetic at all?), then NULL (propagate), then zero (error), then convert the string, then compute. A different order passes some tests and fails the properties: for instance a NULL divisor must give NULL, not `DivideByZero`.

### The result type of NULL + 5

It is the NULL of the wider type, not of the left. Check: `TINYINT NULL + BIGINT 5`.

### Modulo of negatives

`-7 % 3` is `-1` in Rust and in C++; `rem_euclid` gives 2. SQL (and BusTub) use the first.

## Performance

Exact arithmetic costs a few instructions; the `match` on both operand types and the `Result` are the overhead. An expression evaluator spends most of its time here, which is why vectorised engines (module "advanced") process a whole column of one type at a time and skip the per-value dispatch.

**Measure it.** Add 100 million pairs of `Integer` values through `Value::add` and through raw `i32` addition; predict the ratio.

## Experiment

Optional. Predict first, then run.

1. **Wrap.** Replace the range check with `wrapping_add` for `TINYINT`. Which property catches it, and with which smallest counterexample?
2. **Saturate.** Some systems clamp to MAX instead of failing. What would that do to `(a + b) - b = a`?

## Other designs

- **Compute in `i128` and range-check (ours).** One code path for all widths.
- **Four `checked_*` implementations**, one per width, via a macro: no 128-bit arithmetic, more code.
- **Arbitrary precision** (`NUMERIC`): PostgreSQL's decimal type; slower, never overflows within limits.
- **Saturating or wrapping modes**: used in some analytical engines for speed, never in a transactional one.

## In BusTub

BusTub has one `Type` class per SQL type (`IntegerType`, `VarlenType`, ...) with virtual `Add`, `CompareEquals`, `CastAs` and friends, and a `Value` that holds a tagged union. This course keeps the data model (the `TypeId` enum and the `Value` enum are given) and puts every operation in `Value`'s methods, matching on the variants.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `__builtin_mul_overflow(a, b, &r)` | `a.checked_mul(b)` or compute in `i128` |
| signed overflow is undefined behaviour | overflow panics in debug, wraps in release; use `checked_*` to be explicit |
| `fmod(x, y)` | `x % y` (or the formula in the contract) |
| `throw DivideByZeroException` | `Err(ExceptionType::DivideByZero)` |

**Port rule:** C++ arithmetic that may overflow becomes `checked_*` or wider arithmetic plus a range check.

## Learn more

- [`i32::checked_add`](https://doc.rust-lang.org/std/primitive.i32.html#method.checked_add) and friends · [Integer overflow in the Book](https://doc.rust-lang.org/book/ch03-02-data-types.html#integer-overflow)
