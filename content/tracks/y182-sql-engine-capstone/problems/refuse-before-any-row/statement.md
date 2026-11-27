This is step 3 of 5 of the small SQL engine. A `WHERE` condition runs once for each row. If it is wrong, the user must hear it **before** the first row, also when the table is empty. This step checks an expression against the columns of a table, and then runs it.

The expression tree is the one from step 2. Tests build it directly, so this step does not need your parser. A tree is at most 100 nodes tall.

Write `compile(expr, columns)`, and the two methods of `Compiled`. The fields of `Compiled` are yours. `eval` gets only a row, whose values have the columns' types or are NULL.

**Types.** There are three: `Int`, `Text`, `Bool`. The literal NULL has no type (`None`), and it is accepted wherever any type is asked for.

| Expression | Operands | Result |
|---|---|---|
| literal, column | | its own type |
| `NOT x` | `Bool` | `Bool` |
| `- x` | `Int` | `Int` |
| `x IS NULL`, `x IS NOT NULL` | any type | `Bool` |
| `AND`, `OR` | both `Bool` | `Bool` |
| `+ - * /` | both `Int` | `Int` |
| `= <> < <= > >=` | two of one type, whichever | `Bool` |

**Which fault `compile` returns.** For a node with two operands: check everything inside the left one, then everything inside the right one. Only then check the left operand's type, then the right's.

A column name that the table does not have is `UnknownColumn(name)`; names match exactly. A type that does not fit is `WrongType { needed, found }`. In a comparison, the left operand's type is what is `needed` from the right.

`compile` reports only these two. It never computes a value, so `1 / 0` compiles.

**Running.** TRUE, FALSE and NULL follow three-valued logic:

- `NOT NULL` is NULL.
- `AND`: FALSE if either side is FALSE. Otherwise TRUE if both are TRUE, else NULL.
- `OR`: TRUE if either side is TRUE. Otherwise FALSE if both are FALSE, else NULL.
- `IS NULL` and `IS NOT NULL` give TRUE or FALSE, never NULL.

**What runs, and when.** The left operand runs first.

- `AND` does **not** run its right side when the left is FALSE. `OR` does not run it when the left is TRUE. When the left is NULL, the right side runs.
- Every other operator with two operands runs **both** sides, even if the left one is NULL. Then, if either value is NULL, the result is NULL.

Comparisons order integers by size, texts byte by byte, and `FALSE` before `TRUE`. Division drops the fraction, rounding toward zero. `DivisionByZero` is dividing an integer by 0. `Overflow` is any result of `+ - * /` or unary `-` outside `i64`. The first fault met ends the evaluation. `eval` must never panic.

`ty()` is the type of the whole expression.
