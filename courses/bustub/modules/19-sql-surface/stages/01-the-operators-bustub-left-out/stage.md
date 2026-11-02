`select 6 * 7;` fails in your engine with `binary op * not supported in planner yet`, and so do `/`, `%`, `||`, `not x`, `-x` and `x is null`. The parser has understood all of them since module 3d (the precedence climbing you wrote has a level for each), and the binder passes most of them through; the gap is the **last two steps of the pipeline**: turning a bound operator into an expression node the executors can evaluate, with a type the rest of the plan can rely on. BusTub's expression layer stops at `+`, `-`, comparisons, `and` and `or` on INTEGERs. Everyday SQL needs the rest.

> [!CHECK] `select a * b from t` where `a` is a TINYINT column and `b` a DECIMAL column. Before any row is read, the planner has to say what type the result column has. What is it, and what rule gives it? What would go wrong if the planner left the type to be discovered from the first row?
> ||DECIMAL: if either side is DECIMAL the result is DECIMAL; otherwise the result is the wider of the two integer types. The type has to be known at plan time because the output schema, the sort keys, the aggregate that sums the column and the tuple layout of the next operator are all fixed before the first row exists. A row-by-row discovery would also give a column whose type changes with the data (a TINYINT result for small rows and an INTEGER one for large ones), which no tuple layout can store.||
>
> - What type does `1 * null` have? What does the value look like?
> - Which operators accept a string, and which refuse it?
> - Does `x is null` have a result type that depends on `x`'s type?

## The task

Make these work end to end through SQL:

- **Arithmetic:** `*`, `/` (integers divide toward zero) and `%` on any numeric types; `+` and `-` on every numeric type too (BusTub's `ArithmeticExpression` accepts only INTEGER, and the planner uses it for INTEGER pairs only, so existing behaviour is unchanged). A NULL operand gives a NULL of the result type. Overflow is an `OutOfRange` error, division and remainder by zero a `DivideByZero` error: the same errors as `Value`'s arithmetic from module 3a.
- **Strings:** `a || b` on two VARCHARs; NULL if either is NULL.
- **Unary:** `-x` on a number, `not x` on a boolean (NULL stays NULL).
- **`x is null` and `x is not null`:** the one operator that never returns NULL, on any type.
- **Types:** the planner must refuse operands of the wrong type with a `NotImplemented` error that names the operator (`'a' * 2`, `1 || 2`, `not 1`).

Where to work: the new `OperatorExpression` in `src/execution/expressions/operator_expression.rs` (its `Operator` enum is given; the type rules and the computation are yours), the binder's `Expr::IsNull` arm in `src/binder/binder.rs` (it returns `Expr of type PGNullTest not implemented`), and `get_binary_expression_from_factory` and `plan_expression` in `src/planner/planner.rs`.

## Your freedom

How you split type checking from evaluation, whether `compute` reuses `Value::add` and its relatives or does its own checked arithmetic, and how the result type of a mixed expression is worked out. The tests use only SQL and the public constructors of `OperatorExpression`.

## The Rust toolbox

**One enum, one `match`.** `Operator` tags one expression type; `match self.op` in two places (type rules and computation) keeps each rule next to its siblings. The compiler's exhaustiveness check is how you find the operator you forgot.

**`Value` already does the arithmetic.** Module 3a's `add`, `subtract`, `multiply`, `divide` and `modulo` handle widening, NULL and the two errors. Using them is reuse, not cheating: the stage is about *where* the types are decided.

**`?` through a `collect`.** Evaluating all children and stopping at the first error is `children.iter().map(|c| c.evaluate(..)).collect::<Result<Vec<_>>>()?`.

## If this is new

- [S1 Option and Result](/t/s1-option-result): carrying "no value" and "failed" separately.
- Y5 Testing & verification: comparing an engine with a small model.

## Tests

- Precedence, truncating division, remainder, and NULL in every operator.
- Division by zero and overflow are errors; decimals and mixed types work.
- `NOT`, `IS NULL`, `||`, and the operators inside aggregates and filters.
- Wrong operand types are refused by name; the expression types its result.

## Hints

### Decide the result type before you write `evaluate`

Write `new` first: a table from (operator, operand types) to a result type or an error. `evaluate` then only has to produce a value of that type, and a NULL becomes `Value::null(result type)` without a special case per operator.

### NULL handling has one exception

Almost every operator is "if any operand is NULL the result is NULL". `IS NULL` and `IS NOT NULL` are the only ones that look *at* NULL. Check them before the generic rule, not after.

### Reuse the factory's shape

`+` and `-` on two INTEGERs must keep building the `ArithmeticExpression` from 3d (its tests and the EXPLAIN output depend on it); only the other combinations go to your new type.

## Performance

An operator node costs one virtual call per row plus its children, like every other expression. The avoidable cost is **allocation**: collecting the operands into a `Vec` per row is fine for correctness and visible in a profile for a two-operand operator; a `match` on the arity with two locals avoids it. Checked arithmetic is a compare and a branch.

**Measure it.** `select sum(a * b + c) from t` over a table of a million rows, with `*` implemented through `Value::multiply` and again with an `i32` fast path when both sides are INTEGER. How much of the time is the `Value` boxing?

## Experiment

Optional. Predict first, then run.

1. **Let `-5 % 3` use Rust's `rem_euclid`.** Which test fails and what does SQL (and C, and Rust's `%`) say the answer is?
2. **Type NULL literals as BOOLEAN instead of INTEGER in the binder.** Which of the earlier stages' tests (3d) break, and which new ones start passing?

## Other designs

- **One expression type per operator** (BusTub's `ArithmeticExpression`, `ComparisonExpression`, `LogicExpression`): each file is small and the optimizer can downcast to the one it wants; the cost is boilerplate and a longer factory.
- **A function table**: operators registered as functions with signatures, and the planner resolving overloads by operand types, as PostgreSQL does (`pg_operator`). The right design once there are dozens of types.

## In BusTub

`ArithmeticExpression` in BusTub supports `Plus` and `Minus` on integers only ("ONLY SUPPORT INTEGER FOR NOW" is in the source), and the planner's `GetBinaryExpressionFromFactory` knows nothing about `*`, `/`, `%` or unary operators. BusTub's `sqllogictest` files never use them, which is why a course built on those files never notices.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `a / b` with `b == 0` is undefined behaviour for integers | `checked_div` returns `None`; or `Value::divide` returns `Err(DivideByZero)` |
| `INT_MAX * 2` wraps (signed overflow is undefined) | `checked_mul`, `i64::try_from(i128)`: an error you chose |
| `-5 % 3` is `-2` since C++11 | the same: `%` truncates toward zero |
| `dynamic_cast<const ArithmeticExpression *>(e)` in the optimizer | `e.as_any().downcast_ref::<OperatorExpression>()` |

**Port rule:** an arithmetic failure is a value in the result channel (`Result`), never a panic and never a silently wrapped number.

## Learn more

- [PostgreSQL: mathematical functions and operators](https://www.postgresql.org/docs/current/functions-math.html) · [Rust: integer overflow behaviour](https://doc.rust-lang.org/book/ch03-02-data-types.html#integer-overflow)
