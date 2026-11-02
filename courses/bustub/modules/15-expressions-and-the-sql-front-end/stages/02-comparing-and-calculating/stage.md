`price > 10` and `price - discount`: two nodes with two children each. A **comparison** answers true, false or **unknown**; an **arithmetic** node produces a number, and numbers can **overflow**. Both evaluate their children first and then combine the answers, with the rules of module 3a (`CmpBool`, `checked_*`, the NULL encoding) doing the real work. BusTub supports `+` and `-` on INTEGERs only (`*`, `/` and `%` parse, but the planner refuses them).

> [!CHECK] `a < b` where `b` is NULL: is the answer false? What does `WHERE a < b` then do with the row, and what does `WHERE NOT (a < b)` do? Separately: `i32::MAX - (-1)` overflows. What does your arithmetic return, and why is it never the wrapped-around number?
> ||Unknown (NULL), not false: NOT of unknown is unknown, so neither `WHERE a < b` nor `WHERE NOT (a < b)` keeps the row, which is exactly how SQL says "I cannot tell". Overflow is an `OutOfRange` error: a database that wraps around returns a wrong sum with no sign that it is wrong, and the stored encoding of the INTEGER NULL (`i32::MIN`) means that value cannot be a result either.||
>
> - Which `CmpBool` does your comparison return when either side is NULL?
> - What does `checked_add` return on overflow, and how do you turn that into an error?
> - What if the result is exactly `i32::MIN`?

## The task

In `src/execution/expressions/comparison_expression.rs` (`ComparisonType`, the constructor, `to_string` and `cmp_to_value` are given):

- `perform_comparison(lhs, rhs) -> Result<CmpBool>` chosen by `comp_type` (the six comparisons of module 3a's `Value`), and `evaluate` / `evaluate_join`: evaluate both children, compare, answer with `cmp_to_value` (the BOOLEAN NULL for `CmpBool::Null`).

In `src/execution/expressions/arithmetic_expression.rs` (`ArithmeticType`, the constructor with its type check, `to_string` and the result helper are given):

- `perform_computation(lhs, rhs) -> Result<Option<i32>>`: `None` (a NULL result) if either side is NULL; otherwise the sum or difference as a **checked** `i32` operation. An overflow is an `OutOfRange` error, and so is the result `i32::MIN`, which module 3a reserves as the stored INTEGER NULL.
- `evaluate` and `evaluate_join`: evaluate both children, combine, answer `Value::integer` (or `Value::null(TypeId::Integer)` for `None`).

The tests: exact scenarios (the six comparisons, NULL gives NULL, strings and mixed numbers compare by value, columns in a row and in a join, plus, minus, the largest results that fit, overflow, only INTEGER children accepted), and two properties: comparisons on random integers or NULLs agree with Rust's own operators, and arithmetic agrees with a 64-bit oracle (a value, an overflow error, or NULL).

## Your freedom

How the two nodes share code (one helper for evaluating both children and one for the join variant, or two copies), and how you turn `Option<i32>` into a `Value`.

## The Rust toolbox

**`match` on an enum to pick the method.** `match self.comp_type { Equal => lhs.compare_equals(rhs), LessThan => lhs.compare_less_than(rhs), ... }`: one arm per variant, and the compiler tells you when you forget one.

**`let else` to leave early.** `let (Value::Integer(l), Value::Integer(r)) = (lhs, rhs) else { return Ok(None) };` handles every NULL case at once.

**Checked arithmetic.** `l.checked_add(*r)` is `Option<i32>`: `None` on overflow. Chain: `.filter(|v| *v != i32::MIN).ok_or_else(|| Exception::new(ExceptionType::OutOfRange, "Integer overflow."))`.

**`?` on both children.** `let l = self.children[0].evaluate(tuple, schema)?;` returns early with the error from a child (a bad child is an error, not a panic).

**An oracle in the test, not in your code.** The test does the arithmetic in `i64`, where overflow cannot happen, and checks the range afterwards. It is the cheapest correct implementation to compare with.

## If this is new

- L7 Enums & pattern matching: `match` on an enum with no missing arm, `let else`.
- S8 The core traits: `PartialOrd`, comparison results as values.
- L8 Error design: `Result`, `?`, an error type with a kind.
- [D13 Matrix, bits & math](/t/d13-matrix-bits-math): `checked_*`, `wrapping_*`, overflow in debug and release.
- Y5 Testing & verification: Build it: a model for three-valued logic; print-then-parse round trips; fuzzing a lexer and a parser.

## Tests

- The six comparisons on integers; a NULL operand makes NULL; strings and mixed numbers compare by value; comparisons describe themselves and return a BOOLEAN.
- Plus and minus including negatives; a NULL operand gives the INTEGER NULL; overflow (and the result `i32::MIN`) is an `OutOfRange` error; the largest results that fit work; only INTEGER children are accepted.
- Columns in a row, in a join and nested, and the text `((#0.0-#0.2)+100)`.
- Properties: comparisons agree with Rust; arithmetic agrees with a 64-bit oracle.

## Hints

### Evaluate both sides first

Evaluate both children before looking at either value: neither node short-circuits, and an error in the right child is an error even if the left is NULL.

### NULL is not an error

`1 + NULL` is the INTEGER NULL, with a type, because every column has one. That is why `perform_computation` returns `Option<i32>`.

### The one result you cannot have

`i32::MIN` is the encoding of the INTEGER NULL (3a-04 explained why). A subtraction that lands on it must be an error, or a stored result would read back as NULL.

## Performance

Both nodes do two child evaluations, a match and a checked operation: tens of nanoseconds. For a million-row scan with `price - discount > 10` that is a few hundredths of a second in the expression evaluator; the rest of the time goes to reading pages.

**Measure it.** Evaluate `(#0.0 - #0.2) > 10` over a million rows in a loop, once with columns and once with constants; the difference is the cost of reading values from a tuple.

## Experiment

Optional. Predict first, then run.

1. **Wrapping.** Replace `checked_add` by `wrapping_add` and run the oracle property: how quickly does it fail, and with which numbers?
2. **Short-circuit.** Let a comparison return early when the left side is NULL. Which test fails, and is the new behaviour wrong?

## Other designs

- **A node per operator** (`PlusExpr`, `LessThanExpr`): no `match`, more types.
- **Operators as functions in a table** (`fn(&Value, &Value) -> Result<Value>`): one node type, the table is data.
- **Saturating or wrapping arithmetic** (some engines, by option): never an error, rarely what you want.
- **Wider intermediate types** (PostgreSQL promotes `int4 + int4` overflow to an error, but `int4 + int8` to `int8`).

## In BusTub

`comparison_expression.h` (`PerformComparison` switches on `comp_type_` and returns `CmpBool`) and `arithmetic_expression.h` (`ONLY SUPPORT INTEGER FOR NOW`, plus and minus on `int32_t`, with the same NULL handling).

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `switch (comp_type_) { case ComparisonType::Equal: return lhs.CompareEquals(rhs); ... }` | `match self.comp_type { Equal => lhs.compare_equals(rhs), ... }` |
| `INT32_MIN` as the NULL marker | the same, kept out of results by the `!= i32::MIN` check |
| `__builtin_add_overflow` | `i32::checked_add` |
| `throw Exception(ExceptionType::OUT_OF_RANGE, ...)` | `Err(Exception::new(ExceptionType::OutOfRange, ...))` |

**Port rule:** a `switch` over an enum becomes a `match` with no `default`; an overflow builtin becomes a `checked_*` method.

## Learn more

- [`i32::checked_add`](https://doc.rust-lang.org/std/primitive.i32.html#method.checked_add) · PostgreSQL's [comparison functions and NULL](https://www.postgresql.org/docs/current/functions-comparison.html)
