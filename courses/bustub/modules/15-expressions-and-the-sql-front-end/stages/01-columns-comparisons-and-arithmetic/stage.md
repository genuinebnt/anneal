From here on the course runs SQL. You type `select lower(name) from people where id = 3;` and a pipeline turns the text into a **plan**, a tree of operators (scan, filter, projection, ...). The part of the plan that computes with values (`lower(name)`, `id = 3`, `price - discount`) is an **expression tree**, and the nodes of that tree are this module's job.

```text
  SQL text  →  parser  →  binder  →  planner  →  optimizer  →  executors  →  rows
              (syntax)   (names →   (a tree of    (rewrites    (pull tuples,
                          objects)   operators)    the plan)    evaluate expressions)
```

This stage writes the leaves of the tree and the first nodes above them. A **constant** (`5`, `'hello'`) is itself; a **column value** is "the third column of the row". A **comparison** (`price > 10`) evaluates its two children and answers true, false or **unknown**; an **arithmetic** node (`price - discount`) produces a number, and numbers can **overflow**. Module 3a's rules (`CmpBool`, `checked_*`, the NULL encoding) do the real work; the nodes decide what to evaluate and how to combine it. BusTub supports `+` and `-` on INTEGERs only (`*`, `/` and `%` parse, but the planner refuses them).

> [!CHECK] `price - discount > 10` is built once and evaluated for a million rows of a join, with `discount` written `#1.0`. What is each node given on every evaluation, and how does `#1.0` know which row to read? For one row `discount` is NULL: is the comparison false, and does `WHERE NOT (price - discount > 10)` keep the row? For another, `price` is `i32::MAX` and `discount` is -1: what does the subtraction return?
> ||Each evaluation is given the rows and their schemas: the tree was built from names, but by now `#1.0` means "column 0 of input 1", and a tuple alone does not say where column 0 starts. Input 1 is the right row of the join. With a NULL discount the subtraction is the INTEGER NULL and the comparison is unknown, not false; NOT of unknown is unknown, so neither `WHERE` keeps the row, which is how SQL says "I cannot tell". `i32::MAX - (-1)` overflows: it is an `OutOfRange` error, never the wrapped-around number, because a database that wraps returns a wrong answer with no sign that it is wrong.||
>
> - What does a constant ignore?
> - Which `CmpBool` does a comparison return when either side is NULL?
> - What does `checked_sub` return on overflow, and what if the result is exactly `i32::MIN`?

## The task

In `src/execution/expressions/` (the `Expression` trait, the constructors with their type checks, `to_string`, `cmp_to_value` and the arithmetic result helper are given). After this stage:

- **The leaves read their input.** A constant is its literal for any input, in `evaluate` and in `evaluate_join`. A column is column `col_idx` of the row, read with the row's schema (module 3b's `Tuple::get_value`); in a join, `tuple_idx` 0 reads the left row with the left schema and anything else the right row with the right schema.
- **A comparison is three-valued.** `perform_comparison(lhs, rhs) -> Result<CmpBool>` is the comparison of module 3a's `Value` chosen by `comp_type` (six of them); `evaluate` and `evaluate_join` evaluate both children, compare, and answer with `cmp_to_value`, the BOOLEAN NULL when either side is NULL.
- **Arithmetic is checked.** `perform_computation(lhs, rhs) -> Result<Option<i32>>` is `None` (a NULL result) when either side is NULL; otherwise the sum or difference. An overflow is an `OutOfRange` error, and so is the result `i32::MIN`, which module 3a reserves as the stored INTEGER NULL. `evaluate` and `evaluate_join` combine both children and answer an INTEGER, NULL for `None`.
- **A child's error is the node's error.** Both children are evaluated before either value is looked at; nothing panics on a bad input.

> [!ASIDE] The steps, if you would rather not work them out
> 1. A constant returns a clone of its value from both methods.
> 2. A column calls `get_value` on the tuple with its schema and `col_idx`; in a join it first picks the tuple and the schema by `tuple_idx`.
> 3. `perform_comparison`: a `match` on `comp_type` that calls the matching `Value` comparison (`compare_equals`, `compare_less_than`, ...).
> 4. `perform_computation`: if either value is not a non-NULL integer, `Ok(None)`; otherwise `checked_add` or `checked_sub`, reject `None` and `i32::MIN` with `OutOfRange`.
> 5. Every `evaluate`: evaluate child 0 and child 1 with `?`, then combine; every `evaluate_join` does the same with `evaluate_join` on the children.

## Your freedom

How the nodes share their work. Every binary node evaluates two children, once with one row and once with two: one helper that evaluates both sides for either case (a function that takes a closure evaluating one child, say), a small trait, or plain copies. How a comparison picks its method: a `match`, or a table of function pointers indexed by the operator. And how an `Option<i32>` becomes a `Value`. The tests use only the public nodes.

## The Rust toolbox

**A trait object behind `Arc`.** `ExprRef = Arc<dyn Expression>`: operators hold expressions as shared trait objects, as BusTub holds `shared_ptr<AbstractExpression>`. Sharing matters: the optimizer builds new plans out of parts of old ones without copying whole trees. `Arc::clone(&e)` copies a pointer and bumps a counter. Later modules ask "is this a column?" with `expr.as_any().downcast_ref::<ColumnValueExpression>()`, which returns `Some` or `None`.

**Pick a side.** `if self.tuple_idx == 0 { left.get_value(left_schema, i) } else { right.get_value(right_schema, i) }` is the whole join logic of a column.

**`match` on an enum to pick the method.** `match self.comp_type { Equal => lhs.compare_equals(rhs), LessThan => lhs.compare_less_than(rhs), ... }`: one arm per variant, and the compiler tells you when you forget one.

**`let else` to leave early.** `let (Value::Integer(l), Value::Integer(r)) = (lhs, rhs) else { return Ok(None) };` handles every NULL case at once.

**Checked arithmetic.** `l.checked_add(*r)` is an `Option<i32>`, `None` on overflow. Chain: `.filter(|v| *v != i32::MIN).ok_or_else(|| Exception::new(ExceptionType::OutOfRange, "Integer overflow."))`.

**`?` on both children.** `let l = self.children[0].evaluate(tuple, schema)?;` returns early with the error from a child.

**An oracle in the test, not in your code.** The test does the arithmetic in `i64`, where overflow cannot happen, and checks the range afterwards: the cheapest correct implementation to compare with.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): `dyn Trait`, `Arc<dyn Trait>`, why a trait needs `Send + Sync` to be shared across threads.
- S7 Smart pointers & interior mutability: `Arc`, `Box`.
- L7 Enums & pattern matching: `match` on an enum with no missing arm, `let else`.
- S8 The core traits: `PartialOrd`, comparison results as values.
- L8 Error design: `Result`, `?`, an error type with a kind.
- [D13 Matrix, bits & math](/t/d13-matrix-bits-math): `checked_*`, `wrapping_*`, overflow in debug and release.
- Y5 Testing & verification: Build it: a model for three-valued logic; print-then-parse round trips; fuzzing a lexer and a parser.

## Tests

- A constant is itself for any row, alone or in a join; a column reads its value (a NULL and a VARCHAR included), in a join from the side and with the schema it names; random rows read back exactly.
- The six comparisons; a NULL operand gives NULL; strings and mixed numbers compare by value; on random integers and NULLs, comparisons agree with Rust's operators.
- Plus and minus agree with a 64-bit oracle (a value, NULL, or `OutOfRange` for an overflow and for `i32::MIN`); only INTEGER children are accepted.
- Nodes over columns in a row, in a join and nested; expressions describe themselves (`#0.3`, `5`, `((#0.0-#0.2)+100)`) and know their return type.

## Hints

### Tuple index versus column index

`tuple_idx` chooses *which row*, `col_idx` chooses *which column of that row's schema*. A bug that reads `col_idx` of the left row for a right-side column returns plausible numbers; the join test uses two different schemas to catch it. `evaluate` has one row, so nothing in it looks at `tuple_idx`.

### Evaluate both sides first

Neither node short-circuits: an error in the right child is an error even if the left is NULL. Evaluate both, then look at the values.

### NULL is not an error

`1 + NULL` is the INTEGER NULL, with a type, because every column has one. That is why `perform_computation` returns `Option<i32>` inside its `Result`: `Err` is "the query fails", `Ok(None)` is "the answer is unknown".

### The one result you cannot have

`i32::MIN` is the encoding of the INTEGER NULL (3a-04 explained why). A subtraction that lands on it must be an error, or a stored result would read back as NULL. Which pair of operands produces it without overflowing?

## Performance

A leaf costs a schema lookup and a decode: an offset read for a fixed-size column, a follow-the-offset and a `String` allocation for a VARCHAR. A comparison or arithmetic node adds two child evaluations, a match and a checked operation: tens of nanoseconds. A scan pays that per row, which is why real engines evaluate expressions on whole columns at once (vectorised execution); for a million rows the evaluator is typically a few hundredths of a second and reading pages is the rest.

**Measure it.** Evaluate `(#0.0 - #0.2) > 10` over a million rows of ten integer columns, once with columns and once with constants: the difference is the cost of reading values from a tuple. Then read a VARCHAR column instead and see what the allocation costs.

## Experiment

Optional. Predict first, then run.

1. **Wrapping.** Replace `checked_add` by `wrapping_add` and run the oracle property: how quickly does it fail, and with which numbers?
2. **Short-circuit.** Let a comparison return early when the left side is NULL. Which test fails, and is the new behaviour wrong?
3. **Share or copy.** Replace `Arc<dyn Expression>` by `Box` in a tree you build by hand. What can you no longer do?

## Other designs

- **A tree of trait objects (ours, BusTub's).**
- **One `enum Expr`** with a big `match` in a single `eval` function: no vtable, easier to optimise, closed to extension.
- **A node per operator** (`PlusExpr`, `LessThanExpr`): no `match`, more types; or **operators as a table** of `fn(&Value, &Value) -> Result<Value>`: one node type, the table is data.
- **Compile to a closure or bytecode** and run that per row; **vectorised evaluation**, where each node produces a whole column per call.
- **Saturating or wrapping arithmetic** (some engines, by option): never an error, rarely what you want. PostgreSQL makes `int4 + int4` overflow an error but promotes `int4 + int8` to `int8`.

## In BusTub

`constant_value_expression.h` (`return val_;`), `column_value_expression.h` (`return tuple_idx_ == 0 ? left_tuple->GetValue(&left_schema, col_idx_) : right_tuple->GetValue(&right_schema, col_idx_);`), `comparison_expression.h` (`PerformComparison` switches on `comp_type_` and returns `CmpBool`) and `arithmetic_expression.h` (`ONLY SUPPORT INTEGER FOR NOW`, plus and minus on `int32_t`, with the same NULL handling). BusTub's `abstract_expression.h` says expressions are "modeled as trees, i.e. every expression may have a variable number of children".

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `class AbstractExpression { virtual Value Evaluate(...) const = 0; }` | `trait Expression { fn evaluate(&self, ...) -> Result<Value>; }` |
| `std::shared_ptr<AbstractExpression>` | `Arc<dyn Expression>` (`ExprRef`) |
| `dynamic_cast<const ColumnValueExpression *>(expr.get())` | `expr.as_any().downcast_ref::<ColumnValueExpression>()` |
| `switch (comp_type_) { case ComparisonType::Equal: return lhs.CompareEquals(rhs); ... }` | `match self.comp_type { Equal => lhs.compare_equals(rhs), ... }` |
| `__builtin_add_overflow`, `INT32_MIN` as the NULL marker | `i32::checked_add`, kept out of results by the `!= i32::MIN` check |
| `throw Exception(ExceptionType::OUT_OF_RANGE, ...)` | `Err(Exception::new(ExceptionType::OutOfRange, ...))` |

**Port rule:** a class hierarchy with virtual methods becomes a trait and `shared_ptr` of the base becomes `Arc<dyn Trait>`; a `switch` over an enum becomes a `match` with no `default`; an overflow builtin becomes a `checked_*` method.

## Learn more

- [Trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html) · [`Any`](https://doc.rust-lang.org/std/any/trait.Any.html) · [`i32::checked_add`](https://doc.rust-lang.org/std/primitive.i32.html#method.checked_add)
- [Crafting Interpreters: evaluating expressions](https://craftinginterpreters.com/evaluating-expressions.html) · PostgreSQL's [comparison functions and NULL](https://www.postgresql.org/docs/current/functions-comparison.html)
