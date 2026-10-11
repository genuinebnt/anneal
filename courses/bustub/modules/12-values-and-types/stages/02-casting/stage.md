SQL is forgiving about types: `WHERE id = '32'` compares a number with a string, and `SELECT 7 + '5'` adds them. Underneath is a cast, and a cast can fail in several ways: the number does not fit the narrower type, the text has no digits, the types are simply not convertible. This stage is `Value::cast_as(to)`, with exactly BusTub's rules, and it is where you learn Rust's own conversion rules (`as` silently truncates; `try_from` does not) by *not* using `as` for the range check.

> [!CHECK] `300_i32 as i8` in Rust gives 44 without complaint. What does `i8::try_from(300_i32)` give, which of the two does a database cast need, and why is the answer different for a SQL `CAST(300 AS TINYINT)` than for a systems-programming bit-twiddle?
> ||`as` wraps: 300 modulo 256 is 44, silently. `try_from` returns an error for a value that does not fit. A database must report the error (`OutOfRange`): silently storing 44 for 300 would corrupt data with no trace. `as` is right when you mean the bit pattern (hashing, packing), and wrong whenever the *number* must survive.||
>
> - What does `as` do to a negative number cast to an unsigned type?
> - What does `f64 as i32` do for NaN and for 1e20?
> - Which cast needs a range check against the reserved NULL number?

## The task

`cast_as(&self, to: TypeId) -> Result<Value>`, BusTub's rules:

- A cast must be **allowed**: numbers (and decimals) to numbers and to `Varchar`; `Varchar` to anything but `Timestamp`; a boolean and a timestamp to themselves and to `Varchar`; otherwise an `Invalid` error ("X is not coercable to Y").
- A **NULL** becomes the NULL of the target (if the cast is allowed at all).
- A number to `Varchar` is its text (`to_string`).
- An integer to a narrower integer type: **`OutOfRange`** unless it fits in `[MIN + 1, MAX]` of the target (the reserved number is not a value). A decimal to an integer type **truncates toward zero** if in range, else `OutOfRange`. An integer to `Decimal` keeps its number.
- A string to a number reads the **leading number** (optional white space, a sign, digits; for decimals a fraction and an exponent); anything after it is ignored; no digits is a `Conversion` error; out of range is `OutOfRange`.
- A string to a boolean: `true`, `1`, `t` or `false`, `0`, `f`, any case; other text is an error.

The tests are properties: an integer cast to another integer type keeps its number or is `OutOfRange` exactly when it does not fit (random numbers of every width, random targets); a decimal truncates toward zero and checks the range; a number survives a trip through text; text converts by its leading number; text without digits is a `Conversion` error; the boolean spellings; and an example test of the NULL and impossible-cast rules.

## Your freedom

How you organise the conversions (one function per source type, one per target), how you parse (hand-written scanning, `str::parse` on a trimmed prefix), and whether range checks use `i128`, `try_from` or comparisons.

## The Rust toolbox

**`TryFrom` for a checked narrowing.** `i8::try_from(x)` returns `Result<i8, TryFromIntError>`; `.map_err(|_| out_of_range())` turns it into the module's error. Remember the reserved number: the range of a `TINYINT` is `[-127, 127]`, not `[-128, 127]`, so `try_from` plus an explicit `!= i8::MIN` check, or compare against `BUSTUB_INT8_MIN..=BUSTUB_INT8_MAX`.

**`str::parse::<i64>()` and its limits.** It rejects trailing junk and leading spaces; you need the *leading number* of `"12abc"`. Find the prefix yourself (skip spaces, sign, digits with `char::is_ascii_digit`) and parse that slice.

**`f64::trunc` and `as`.** `d.trunc() as i64` truncates toward zero; Rust's float-to-int `as` saturates (and maps NaN to 0), so check the range **before** you cast.

**`to_lowercase()` for case-insensitive words.** `match s.to_lowercase().as_str() { "true" | "1" | "t" => .. }`.

**`?` through helpers.** Small functions returning `Result<Value>` composed with `?` read better than one huge `match`.

## If this is new

- [S1 Option & Result](/t/s1-option-result): `map_err`, `?`, `ok_or`.
- [S2 Strings & text](/t/s2-strings-text): `trim_start`, byte vs char indexing, `parse`.
- [D13 Matrix, bits & math](/t/d13-matrix-bits-math): overflow, `checked_*`, why `as` is lossy.
- The optional concepts *integers and casts* and *parsing numbers from text* have the rules and the traps.
- [S8 The core traits](/t/s8-core-traits): Implement by hand: `PartialOrd`/`Ord` with `total_cmp`, `From`/`TryFrom`.
- [L8 Error design](/t/l8-error-design): Custom errors: overflow and cast errors as values.

## Tests

- Integer to integer: keeps the number or `OutOfRange` exactly when it does not fit (any widths); integer to decimal keeps it.
- Decimal to integer truncates toward zero and checks the range, including the end points of each type.
- A number survives text; text converts by its leading number (a decimal reads its fraction and exponent and is range-checked); no digits is a `Conversion` error; boolean spellings.
- NULL and impossible casts.

## Hints

### Write the allowed table first

The first few lines of `cast_as` should be "is this cast allowed at all" and "is it a NULL": every other branch can then assume a real value of an allowed pair.

### Ranges with the reserved number

Test `cast_as(TinyInt)` on 127, 128, -127, -128 by hand. -128 is an error, not a NULL.

### Spaces and signs

`"  -7"` is -7; `"+7"` is 7; `"-"` has no digits; `"12abc"` is 12; `"abc12"` has no *leading* number. Write these as your own tests before you code.

## Performance

A cast per value per row is on the hot path of an executor: a text-to-number parse is tens of nanoseconds, an integer widening is free. Executors avoid casting by binding types once, at plan time (module 3d).

**Measure it.** Cast one million strings to integers with your parser and with `str::parse`; predict whether a hand-written scanner or the standard library is faster, and why.

## Experiment

Optional. Predict first, then run.

1. **Use `as`.** Replace one range check with `as` and run the property: how short is the shrunk counterexample?
2. **Decimal edge.** Cast 127.9 to TINYINT and 128.0 to TINYINT. Which is an error, and why does the truncation happen *after* the range check in one spot and before in another?

## Other designs

- **`i128` for every integer check (ours).** One range test for all widths.
- **A macro per width** generating the checks: compact, less readable.
- **`num_traits::NumCast`**: a ready-made checked cast between numeric types.
- **Lazy conversion**: store the text and parse only when needed (some engines do this for CSV scans).

## In BusTub

BusTub has one `Type` class per SQL type (`IntegerType`, `VarlenType`, ...) with virtual `Add`, `CompareEquals`, `CastAs` and friends, and a `Value` that holds a tagged union. This course keeps the data model (the `TypeId` enum and the `Value` enum are given) and puts every operation in `Value`'s methods, matching on the variants.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `static_cast<int8_t>(x)` (wraps silently) | `i8::try_from(x)` (an error) |
| `std::stoi(s)` (leading number, throws) | a scanner for the prefix plus `parse` |
| `throw OutOfRangeException` | `Err(Exception::new(ExceptionType::OutOfRange, ..))` |
| `std::to_string(d)` | `format!("{d:.6}")` |

**Port rule:** an implicit C++ narrowing conversion becomes an explicit `try_from` and a handled error.

## Learn more

- [`TryFrom`](https://doc.rust-lang.org/std/convert/trait.TryFrom.html) · [`f64::trunc`](https://doc.rust-lang.org/std/primitive.f64.html#method.trunc) · [Numeric casts in the Reference](https://doc.rust-lang.org/reference/expressions/operator-expr.html#numeric-cast)
