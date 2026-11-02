In Rust, `1 == 2` is `false` and `NULL` does not exist. In SQL, `NULL = 1` is neither true nor false: it is **unknown**, and so is `NULL = NULL`. A `WHERE` clause keeps only rows for which the condition is *true*, so rows with an unknown answer vanish silently: the most common source of "my query returns too few rows". This stage builds `CmpBool`, the three-valued answer (`True`, `False`, `Null`), and the six comparison operators on `Value`, and then checks the laws a comparison must obey.

> [!CHECK] `SELECT * FROM t WHERE x <> 5` and `SELECT * FROM t WHERE NOT (x = 5)` look the same. A row has `x = NULL`. Does either return it? And `WHERE x = 5 OR x <> 5`: does it return every row? What does this say about "excluded middle" in SQL?
> ||Neither returns the row: `NULL <> 5` is unknown and `NOT unknown` is unknown, and a row is kept only on *true*. `x = 5 OR x <> 5` is `unknown OR unknown` = unknown for the NULL row, so it does **not** return every row. Two-valued logic's law that a statement is true or false does not hold: SQL has three values, and the third one (unknown) is neither.||
>
> - What does `unknown AND false` give? `unknown OR true`?
> - Why is `NULL = NULL` not true?
> - Which operation treats two NULLs as the same?

## The task

`Value::compare_equals`, `compare_not_equals`, `compare_less_than`, `compare_less_than_equals`, `compare_greater_than`, `compare_greater_than_equals` each return `Result<CmpBool>`; plus `compare_exactly_equals(&other) -> bool`.

- If the two types cannot be compared (`check_comparable` is false) the result is an **error**.
- If either side is NULL the answer is **`CmpBool::Null`** for all six operators (even `NULL = NULL`).
- Numbers compare by value across types: two integers exactly; anything with a decimal as a float.
- A number compared with a string converts the string to the number's type first; a boolean and a string: the string to a boolean; a string and anything else: the other side to text, compared byte by byte.
- `compare_exactly_equals` is for grouping and hashing, not for SQL: two NULLs are equal (even of different types); otherwise true iff `compare_equals` says `True`; an error counts as false.

The tests are properties: all six operators on random integers of random types agree with `i128`'s `Ord`; a NULL on either side gives `Null` for every operator; the six answers agree with each other (`=`/`<>` opposites, `<`/`>=` opposites, `<=`/`>` opposites, `a < b` is `b > a`); `<` is transitive; decimals compare as floats; strings compare bytewise; a number and a string compare as numbers; incomparable types are errors; exact equality.

## Your freedom

One private function returning `Option<Ordering>` and six one-liners (the reference), or six independent matches; how you convert the mixed cases; where the comparability check goes.

## The Rust toolbox

**A three-valued enum.** `enum CmpBool { False, True, Null }` plus `CmpBool::from_bool(b)`. A `match` on it forces you to decide what each of the three means for every caller.

**`Option<Ordering>` as the core.** `a.partial_cmp(&b)` returns `Option<Ordering>`: `None` is "unknown". A `compare` that returns `Result<Option<Ordering>>` (error for incomparable, `None` for NULL, `Some(order)` otherwise) lets all six operators be one line each: `Ok(match ord { None => CmpBool::Null, Some(o) => CmpBool::from_bool(o.is_lt()) })`.

**`Ordering`'s helpers.** `is_eq`, `is_lt`, `is_le`, `is_gt`, `is_ge`, `is_ne`, `reverse`.

**Byte-wise string comparison.** `a.as_bytes().cmp(b.as_bytes())` is what C++'s `strcmp` does; Rust's `str::cmp` is the same for UTF-8.

**`f64` is not `Ord`.** NaN has no order, so `f64::partial_cmp` returns an `Option`; decide what a NaN comparison does here (the reference treats it as equal) and write it down.

## If this is new

- S8 The core traits: `PartialOrd`, `Ord`, `Ordering`, and why `f64` only has the first.
- L7 Enums & pattern matching: matching on pairs `(a, b)`.
- [S1 Option & Result](/t/s1-option-result): `Option<Ordering>`.
- [S2 Strings & text](/t/s2-strings-text): Understand: UTF-8, case mapping, comparing strings.

## Tests

- Integers across types: all six operators equal `i128`'s.
- A NULL on either side: all six are `Null`.
- The six answers are consistent; `<` is transitive.
- Decimals as floats; strings bytewise; number versus string as numbers.
- Incomparable types are errors; exact equality for grouping.

## Hints

### Make the NULL case disappear early

Check comparability, then NULL, then you can assume two real values for the rest. Every other branch is simpler.

### The mixed cases

Say it in a table: (number, string) converts the string; (string, number) converts the number to text; (boolean, string) converts the string. Which of these can fail (a string that is not a number)? What does the comparison do then?

### A property that fails with NULL

If the consistency property fails only when a NULL is involved, remember that `Null` is its own answer: `not(Null) = Null`.

## Performance

A comparison on two integers is a handful of instructions after the dispatch; an executor that compares millions of values per second pays for the `match` on both tags and the `Result` wrapping. Real engines bind a comparison function per column pair at plan time to avoid dispatching per row.

**Measure it.** Time 100 million integer comparisons through `compare_less_than` and through plain `<`. Predict the ratio; then see what the optimiser does with a `match` that the compiler can see through.

## Experiment

Optional. Predict first, then run.

1. **Two-valued by mistake.** Return `False` for NULL comparisons. Which property fails first, and what real query would give a wrong answer?
2. **NaN.** Compare `decimal(NaN)` with `decimal(1.0)`. What does yours say, and what does PostgreSQL say?

## Other designs

- **`Option<Ordering>` core (ours).** One comparison, six projections.
- **Six separate matches.** Verbose, easy to make inconsistent: the consistency property exists for this.
- **A `Ord` implementation on `Value`** for sorting: a total order where NULLs sort first or last (module 3g) is a different thing from SQL's comparison; keep both.
- **Typed comparison functions** chosen at plan time (a function pointer per expression).

## In BusTub

BusTub has one `Type` class per SQL type (`IntegerType`, `VarlenType`, ...) with virtual `Add`, `CompareEquals`, `CastAs` and friends, and a `Value` that holds a tagged union. This course keeps the data model (the `TypeId` enum and the `Value` enum are given) and puts every operation in `Value`'s methods, matching on the variants.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `enum CmpBool { CmpFalse, CmpTrue, CmpNull }` | `enum CmpBool { False, True, Null }` |
| `Value::CompareEquals` with a `switch` per type | `match (self, other)` |
| `strcmp` | `a.as_bytes().cmp(b.as_bytes())` |
| `throw` for a type mismatch | `Err(Exception)` |

**Port rule:** a C++ three-valued enum stays one; a family of near-identical comparison methods becomes one comparison and several projections.

## Learn more

- [`Ordering`](https://doc.rust-lang.org/std/cmp/enum.Ordering.html) · [`PartialOrd`](https://doc.rust-lang.org/std/cmp/trait.PartialOrd.html)
- [Three-valued logic](https://en.wikipedia.org/wiki/Three-valued_logic#SQL) in SQL
