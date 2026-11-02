Every expression you have written so far evaluates **all** its children and then combines them. `CASE` cannot: `case when b = 0 then 0 else a / b end` exists so that the division is *not* evaluated when `b` is zero, and an engine that evaluated both branches first would turn the guard into an error. This is the first expression of the engine with an evaluation *order* that is part of its meaning, and it has a type problem of its own: a CASE has one type, decided at plan time, made of branches that may include a bare `NULL` that has no type at all.

> [!CHECK] `select case when a > 1 then 'x' else null end from t`. The binder types a bare NULL literal as INTEGER (module 3d). What is the type of this CASE, and what should happen to the NULL branch? What would the output column look like if the NULL branch were allowed to make the CASE an INTEGER?
> ||The CASE is VARCHAR, because the one branch that has a real type says so. The NULL branch must be re-typed as a NULL of that type (a `Value::null(Varchar)`), not rejected as a type mismatch and not allowed to win. If the INTEGER NULL won, the column would be INTEGER and the `'x'` rows would fail to store or print; and a CASE of `1` and `NULL` would become a VARCHAR if the rule were "last branch wins". The rule is: all non-NULL-literal branches must agree, and NULL literals take that type.||
>
> - What is the type of a CASE whose results are all NULL?
> - Which branch's expression is evaluated for a row that matches the second `when`?
> - If a condition is NULL, which branch is taken?

## The task

- `case when c1 then r1 [when c2 then r2 ...] [else e] end`: the first condition that is TRUE decides; a NULL condition is not TRUE; no `else` gives NULL of the result type. **Only the chosen result is evaluated.**
- `case x when v1 then r1 ...`: the operand form, meaning `when x = v1`. (`when null` never matches.)
- `coalesce(a, b, ...)`: the first argument that is not NULL (one or more arguments); `nullif(a, b)`: NULL if `a = b`, else `a`. Both are rewritten by the **binder** into a CASE, so nothing after it knows they exist.
- Type rules: every condition is a BOOLEAN (else `MismatchType`); all results share one type (else `MismatchType`); a bare NULL literal result takes the type of the others.
- An aggregate inside a CASE and a CASE around an aggregate both work (`sum(case when a > 1 then 1 else 0 end)`, `case when count(*) > 3 then ...`).

Where to work: the parser's `primary` (`case` is a reserved word; the AST variant `Expr::Case` is given and the operand form should be rewritten to the searched form there), the binder's `Expr::Case` arm and its `Expr::Function` arm (names `coalesce` and `nullif`), `plan_expression` in `src/planner/planner.rs`, and `CaseExpression` in `src/execution/expressions/case_expression.rs`.

## Your freedom

How the branches are stored (pairs, or a flat list of children as BusTub stores expression children), where you check the types (the constructor is the natural place), and whether `COALESCE` is rewritten to `is not null` tests or to a dedicated evaluation. The tests check results, errors and laziness; they do not look at the tree.

## The Rust toolbox

**Lazy by construction.** Hold the branches as expressions and call `evaluate` on a branch only after its condition said TRUE. A `for` loop with an early `return` is the whole control flow.

**`downcast_ref` to recognise a NULL literal.** `e.as_any().downcast_ref::<ConstantValueExpression>().is_some_and(|c| c.val.is_null())`, the same pattern the optimizer uses.

**`let ... else` for arity.** `let [a, b] = args.as_slice() else { return Err(...) };` pins `nullif` to exactly two arguments.

## If this is new

- L7 Enums and patterns: matching on shapes, not types.
- Y5 Testing & verification: a model written as a `match`.

## Tests

- Searched and operand forms; first TRUE wins; no ELSE is NULL; a NULL condition falls through.
- Only the chosen branch is evaluated (a division by zero in an untaken branch); the guard without CASE does fail.
- COALESCE and NULLIF, including `a / nullif(b, 0)`.
- Branch types must agree and a NULL adopts them; CASE inside and around aggregates; a model property.

## Hints

### Evaluate the condition, then decide, then evaluate the result

Do not evaluate a branch's condition *and* result together. The result of a branch whose condition is not TRUE must not run, because the guard idiom depends on it.

### Fix the type in the constructor, not in `evaluate`

`return_type()` is called by the planner before any row exists. If the type is computed lazily from the data, the output schema is wrong by the time the first tuple is built.

### The aggregate order is a contract

The planner finds aggregate calls in one order (`collect_aggregates`) and meets them again in the same order when it plans the select list. A CASE has more places an aggregate can hide (the conditions, the results, the else): plan them in the order the collector visits them, or `sum(...)` will be replaced by `count(...)`'s column.

## Performance

A CASE costs the conditions evaluated up to the winner plus one result. Order the conditions by how often they are true and how cheap they are: that is a tuning knob real queries use. The rewrite of COALESCE evaluates an argument twice (`a is not null`, then `a`); for a column reference that is free, for an expensive expression it is a cost the binder could avoid by evaluating once.

**Measure it.** `coalesce(a, b, c)` over a million rows where `a` is mostly NULL and where it is mostly set. How much does the first-argument shortcut save?

## Experiment

Optional. Predict first, then run.

1. **Evaluate all branches, then pick.** Which test fails? What is the failure a user would see in production?
2. **Make a NULL condition match like FALSE for NOT.** (`when not (a > 1)` on a NULL `a`.) Does your CASE pick the else, and what does `where not (a > 1)` do for the same row?

## Other designs

- **A real `Coalesce` node:** evaluates arguments in order and stops at the first non-NULL, without evaluating the condition twice; PostgreSQL has one.
- **Vectorised CASE:** evaluate each condition on the whole batch, then each result only on the selected rows (selection vectors), which is how columnar engines keep the laziness.

## In BusTub

BusTub's binder rejects `CASE` ("Expr of type PGCaseExpr not implemented") and has no `COALESCE` or `NULLIF`; its only conditional-looking expression is `AND`/`OR` short-circuit semantics through three-valued logic, which does not need laziness because both sides are cheap and error-free.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `cond ? a : b` | `if cond { a } else { b }` (an expression) |
| `std::optional<T>` for "no else" | `Option<ExprRef>` |
| a `switch` over `PGCaseExpr` fields | a `match` on `Expr::Case { branches, otherwise }` |

**Port rule:** an expression's evaluation order is part of its meaning: do not let a generic "evaluate all children" helper run for CASE.

## Learn more

- [PostgreSQL: conditional expressions](https://www.postgresql.org/docs/current/functions-conditional.html) · [Rust: `let`–`else`](https://doc.rust-lang.org/rust-by-example/flow_control/let_else.html)
