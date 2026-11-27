Programmers write `LEFT JOIN` by habit, then add `WHERE b.z > 250`. The padded rows have a NULL `b.z`; `NULL > 250` is unknown; the filter removes every padded row; and what is left is exactly what an inner join would have returned. The outer join was never needed. The optimizer can see this, and acting on it matters: an inner join may be reordered with other joins, run by a hash join, and receive filters on both sides (the previous stage), none of which a LEFT join allows. This stage teaches the optimizer to ask one question: **does this filter fail for every row that the join padded?**

> [!CHECK] For `a left join b on a.x = b.x`, decide for each filter whether the join can become an inner join: (1) `b.z > 250`, (2) `b.z is null`, (3) `b.z > 250 or a.y > 5`, (4) `b.z > 250 and a.y > 5`, (5) `coalesce(b.z, 0) > 250`, (6) `b.z is not null`.
> ||(1) yes: a padded row has `b.z` NULL, the comparison is unknown, the row is dropped. (2) no: that is *true* for padded rows; the filter is how you find them. (3) no: a padded row can pass through the `a.y > 5` side. (4) yes: `AND` fails when either side fails. (5) no: the default turns the NULL into a value that may pass. (6) yes. The pattern: `AND` rejects if **either** part does, `OR` only if **both** do, `IS NULL` and `COALESCE` never.||
>
> - Which join types can be simplified by a filter on the left side, and into what?
> - A FULL join is filtered by `b.z > 250`. What does the join become? And by `a.y > 15 and b.z > 250`?
> - Why is answering "yes, this rejects NULLs" when the truth is "no" much worse than the opposite mistake?

## The task

Implement the rule `optimize_outer_join_simplification` in `src/optimizer/optimizer.rs`, with its three helpers (`is_null_when`, `rejects_nulls`, `simplified_join_type`):

- A filter directly above a LEFT, RIGHT or FULL join looks at the join's output. If some conjunct of the filter is certainly not true whenever every column of the right input is NULL, the right padding is pointless; likewise for the left input.
- A LEFT join whose right padding is pointless becomes INNER; a RIGHT join whose left padding is pointless becomes INNER; a FULL join loses each pointless side (right: it becomes a RIGHT join; left: a LEFT join; both: INNER).
- A comparison, `LIKE`, arithmetic or `||` is NULL when an operand is. `x IS NOT NULL` rejects what makes `x` NULL. `AND` rejects if either part does, `OR` only if both. `IS NULL`, `COALESCE`, `CASE` and anything you cannot analyse are answered "no".
- The rule runs **before** the pushdown rule, so the filters of a join that became inner can move to both inputs.
- The rows returned never change.

## Your freedom

How precisely you analyse conditions. "No" is always safe; a more precise "yes" (for example for `NOT`) is optional. How you represent "the columns of the right input" (a predicate on the column index, a set).

## The Rust toolbox

**A conservative analysis.** When an analysis says "yes" the optimizer changes the plan; when it says "no" nothing happens. Make "I don't know" the default branch of every `match`.

**Closures as arguments.** The question "is column `i` one of the padded ones?" is passed as `&dyn Fn(u32) -> bool`, so the same function answers it for the left and for the right input.

**`downcast_ref` as pattern matching on a trait object.** The expression tree is made of trait objects; you ask each node "are you an `AND`?" with `as_any().downcast_ref::<LogicExpression>()`.

## If this is new

- S6 Trait objects: downcasting a `dyn Trait` with `Any`.
- Y5 Testing & verification: a property that compares optimized and unoptimized plans.

## Tests

- A LEFT join filtered on the right side with a NULL-rejecting condition becomes INNER (comparison, `IS NOT NULL`, arithmetic, `AND` with an unrelated part), and the rows are unchanged.
- Conditions that a padded row can pass (`IS NULL`, `OR` with a free side, `COALESCE`) leave the join alone.
- RIGHT join: a filter on the left side makes it INNER; a filter on the right does not.
- FULL join: each side is judged separately (RIGHT, LEFT, INNER, or unchanged).
- After simplification the filters reach the scans.
- A property over random tables, join kinds and conditions: the optimized query returns what the plain plan returns.

## Hints

### Start from the table in the check above

Write six small tests, one per row of the table, with the expected join type. If a test fails, you have found a case for `rejects_nulls`.

### The dangerous direction

A condition wrongly judged NULL-rejecting turns an outer join into an inner one and the query loses its padded rows with no error. Prefer to answer "no" for anything you have not thought through; add `NOT` only after you can say what `NOT unknown` is.

### One column set per side

`rejects_nulls` takes "which columns are NULL" as a parameter. Call it once for the left input's columns and once for the right input's, and combine the answers with `simplified_join_type`.

## Performance

The rule is a single walk over the filter's expression. What it buys is much larger: an inner join can be executed by a hash join with `|left| + |right|` work instead of a nested loop with `|left| × |right|` (this engine's FULL and RIGHT joins use the nested loop), and the filters become available for pushdown.

**Measure it.** Run `a left join b on a.x = b.x where b.z > 250` over 2 000-row tables with and without the rule (`set force_optimizer_starter_rule = yes`) and compare the plans and the times.

## Experiment

Optional. Predict first, then run.

1. **Treat `NOT` like a comparison** (reject when the operand is NULL-rejecting): which test fails? Is `NOT (b.z is not null)` a counterexample?
2. **Let `OR` reject if either part does.** Which query shows it?

## Other designs

- **Null-rejection as a type of property.** PostgreSQL calls such conditions "strict": a function is strict if it returns NULL on any NULL input, and the planner uses the flag to reason about outer joins.
- **Reordering outer joins:** with simplification done, associativity rules for inner joins become available.
- **Doing it in the binder** (while the query is analysed) instead of as an optimizer rule: simpler, but misses filters that appear after other rewrites.

## In BusTub

BusTub's optimizer does not simplify outer joins. PostgreSQL does (`reduce_outer_joins`), based on exactly this "strict condition" test.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a virtual `IsStrict()` on every expression class | a function that matches on the kinds of node it understands, with a default of `false` |
| `std::function<bool(uint32_t)>` | `&dyn Fn(u32) -> bool` |

**Port rule:** a conservative analysis returns the safe answer in the default arm.

## Learn more

- [PostgreSQL source: reduce_outer_joins](https://github.com/postgres/postgres/blob/master/src/backend/optimizer/prep/prepjointree.c) · [PostgreSQL: strict functions](https://www.postgresql.org/docs/current/sql-createfunction.html)
