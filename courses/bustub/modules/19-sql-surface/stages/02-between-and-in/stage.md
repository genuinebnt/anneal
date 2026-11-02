`where a between 2 and 4` and `where a in (1, 4, 99)` are the most common conditions after `=`, and neither needs a new idea in the engine: `x between lo and hi` *means* `x >= lo and x <= hi`, and `x in (a, b)` *means* `x = a or x = b`. The right place to say so is the **parser**: it rewrites the sugar into the comparisons and logic the rest of the pipeline already runs, and the binder, planner, optimizer (which can still push a range into an index scan) and executors never hear about it. The interesting part is not the rewrite. It is what the rewrite does with NULL, which is where `NOT IN` earns its reputation.

> [!CHECK] A table has `a` values `1, 2, NULL`. How many rows does `select * from t where a not in (1, null)` return? Work it out row by row with `not (a = 1 or a = null)`, before running it.
> ||None. For `a = 1`: `1 = 1` is TRUE, so the OR is TRUE and NOT makes it FALSE. For `a = 2`: `2 = 1` is FALSE and `2 = NULL` is NULL, so the OR is NULL, and NOT NULL is NULL: not TRUE, so the row is not selected. For `a = NULL`: both comparisons are NULL: NULL. A single NULL in the list makes `NOT IN` unable to ever say TRUE: it can say FALSE (found) or "unknown" (not found, but it might equal the NULL). This is the reason `NOT IN (subquery)` over a nullable column is a classic bug, and `NOT EXISTS` is the recommended spelling.||
>
> - What is `null in (1, 2)`? What is `1 in (2, null)`?
> - Which of `IN` and `NOT IN` can be TRUE for a row whose `a` is NULL?
> - If the rewrite were `a <> 1 and a <> null`, would the answers differ?

## The task

Make these parse and run (in `src/sql/parser.rs`, inside the expression grammar you wrote in 3d-05, in the loop that handles comparisons and `IS NULL`):

- `x [not] between lo and hi`: both bounds inclusive; the `and` after `lo` belongs to BETWEEN, not to the enclosing condition. `not between` negates the whole test.
- `x [not] in (e1, e2, ...)`: one or more expressions. Each item is an expression of the row, not only a constant.
- `x in (select ...)` is the next module's business: refuse it with a parse error rather than misparse it.

Both forms must produce the **same syntax tree** a person would have written by hand (`>=`, `<=`, `and`, `=`, `or`, `not`), so the whole engine, including the optimizer's rules, benefits without change.

## Your freedom

Where the operand's tokens are split (you parse the left operand once, and the rewrite may copy its tree), the shape of the OR chain (left-nested like the grammar, or a balanced tree), and how `NOT` is expressed (`Unary not` around the test, or De Morgan applied at once). The tests compare results, and `EXPLAIN` on a BETWEEN must still show two comparisons.

## The Rust toolbox

**`clone` a tree you rewrite twice.** `x between lo and hi` uses `x` twice; `Expr` is `Clone`. For a pure expression this is a second evaluation, nothing more.

**Folding a list into a chain.** `items.into_iter().map(|i| eq(x.clone(), i)).reduce(|a, b| or(a, b))` builds the OR chain; `reduce` returns `None` for an empty list, which is a syntax error here.

**Peeking before consuming.** `at_word_n(1, "between")` looks past a leading `not` without moving; use it to decide before you consume the `not`.

## If this is new

- [S1 Option and Result](/t/s1-option-result): `reduce` and `?` on parse results.
- Y5 Testing & verification: a property test against a three-valued model.

## Tests

- BETWEEN includes both bounds, skips NULLs, swapped bounds select nothing.
- IN on integers, strings and expressions; a one-item list.
- NOT IN with a NULL in the list selects nothing; a NULL on the left is neither IN nor NOT IN.
- The AND of BETWEEN and the precedence around it; a subquery is refused; a model property.

## Hints

### Parse the bounds one level tighter than AND

If you parse `lo` with the full `expr()`, it swallows `and hi` as a conjunction and the statement fails or means something else. Use the level just above comparison for both bounds.

### Do not "optimize" the NULL case away

The temptation is `x not in (1, null)` => "remove the NULL from the list". That is exactly the bug the rewrite is supposed to have: the language says the answer is unknown.

### Test the rewrite, not the operator

Write the expected syntax tree for `a between 1 and 3` by hand and compare. The engine tests then only have to confirm that the tree means what it should.

## Performance

The rewrite costs nothing at run time beyond what the comparisons cost: `between` evaluates `x` twice (once per comparison), and an `IN` list of `n` items evaluates `x` up to `n` times and does up to `n` comparisons per row. For a long constant list that is linear per row.

**Measure it.** `where a in (...)` with 10, 100 and 1000 constants over a million rows. Where does the time start to hurt, and what would a hash set of the constants change?

## Experiment

Optional. Predict first, then run.

1. **Rewrite `NOT IN` as `a <> e1 and a <> e2`.** Does any test change? (It should not: De Morgan holds in three-valued logic.) Why is it still the same?
2. **Rewrite `BETWEEN` with `>` and `<`.** Which test fails first?

## Other designs

- **A dedicated `InList` expression** evaluating `x` once and probing a hash set built from the constant items: much faster for long lists, and the shape PostgreSQL uses (`ScalarArrayOpExpr`, `= ANY(array)`).
- **A range node** (`Between`) kept as one node so a cost model can see a range, and an index scan can use it directly instead of recognising two comparisons.

## In BusTub

BusTub's parser (libpg_query) already produces `PGAExpr` with kind `AEXPR_BETWEEN` and `AEXPR_IN`; its binder does not handle them (they return "not implemented"), so this is a place where PostgreSQL's front end understands more than BusTub's back end.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a `switch` on the AEXPR kind in the binder | a rewrite in the parser: the AST has no `Between` variant |
| `std::vector<std::unique_ptr<Expr>>` and a loop | `Vec<Expr>` and `reduce` |
| copying a subtree: `expr->Copy()` | `expr.clone()` |

**Port rule:** syntactic sugar is removed at the earliest stage that sees it; each later stage has fewer cases.

## Learn more

- [PostgreSQL: row and array comparisons (IN, NOT IN)](https://www.postgresql.org/docs/current/functions-comparisons.html) · [PostgreSQL: comparison functions (BETWEEN)](https://www.postgresql.org/docs/current/functions-comparison.html)
