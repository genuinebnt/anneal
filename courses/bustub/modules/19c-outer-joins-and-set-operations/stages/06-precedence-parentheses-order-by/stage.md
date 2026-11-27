With one operator, a chain is easy to read left to right. With three operators the question "what does `a UNION b INTERSECT c` mean?" has two answers that differ, and the standard picks one: `INTERSECT` binds tighter, the way `*` binds tighter than `+`. `UNION` and `EXCEPT` have the same rank and group from the left, which matters more than it looks: `a EXCEPT b EXCEPT c` is not `a EXCEPT (b EXCEPT c)`. This stage replaces the parser's flat left-to-right chain with a reader that knows the precedence, adds parenthesised operands, and settles which query an `ORDER BY` or a `LIMIT` belongs to.

> [!CHECK] `a = {1, 2}`, `b = {2, 3}`, `c = {3, 4}`. Evaluate `a union b intersect c`, `a intersect b union c`, and `(a union b) intersect c`. Then for `a = {1, 2, 3}`, `b = {2}`, `c = {3, 4}`: `a except b except c` and `a except (b except c)`.
> ||`a union (b intersect c)` = {1, 2} ∪ {3} = {1, 2, 3}. `(a intersect b) union c` = {2} ∪ {3, 4} = {2, 3, 4}. `(a union b) intersect c` = {3}. `(a except b) except c` = {1, 3} minus {3, 4} = {1}; `a except (b except c)` = {1, 2, 3} minus ({2} minus {3, 4}) = {1, 2, 3} minus {2} = {1, 3}. A parser that groups from the right gets the last one wrong, and one that ignores precedence gets the first two wrong.||
>
> - If the operator *after* the right operand binds tighter, who gets the right operand?
> - In `select x from a union all select x from b order by x limit 3`, what does the `ORDER BY` sort? How do you sort or limit only one side?
> - What do the names in `ORDER BY` refer to?

## The task

Replace the parser's chain reader (`set_ops`, with the helper you write beside it) and make the whole result orderable:

- `INTERSECT` has higher precedence than `UNION` and `EXCEPT`; `UNION` and `EXCEPT` are equal and group from the left; `INTERSECT` chains group from the left too.
- Parentheses around an operand: `(select ...) union (select ...)`. The reader of a single operand is given and accepts them: a parenthesised operand is a full query that may have its own `ORDER BY`, `LIMIT` and `WITH`, and may be another set operation. What you must get right is how a parenthesised operand takes part in the grouping.
- A trailing `ORDER BY`, `LIMIT` and `OFFSET` after the last operand apply to the whole chain; `ORDER BY` names the result's columns, which are named after the first select.
- A `WITH` before the first select is visible in every operand.
- `UNION DISTINCT` is `UNION`; `UNION ALL` keeps duplicates.

## Your freedom

The algorithm for precedence (climbing with a minimum level, two functions, a shunting yard) and the shape of the nodes you build, as long as the printed plan keeps the grouping.

## The Rust toolbox

**Precedence climbing in one function.** `parse(min)` reads an operand, then loops: peek the operator; if its rank is below `min` stop; otherwise read the right operand with `parse(rank + 1)` (left-associative) and combine. The same function that reads arithmetic can be written for any operator family.

**Peek before you consume.** The parser needs to look at the next operator without moving. A `peek_set_op` that returns the operator and its rank without advancing is the whole trick.

**Recursion for parentheses.** A `(` at an operand position means "a whole query, then `)`": call the top-level query reader again.

## If this is new

- S5 Enums and pattern matching: ranks as a `match`.
- [Y2 Functions and tooling](/t/y2-unsafe-rust): small private helpers.

## Tests

- `INTERSECT` before `UNION`, in both orders; `UNION` and `EXCEPT` left to right; `EXCEPT` chains.
- Parentheses override the grouping, and nest.
- `ORDER BY` / `LIMIT` / `OFFSET` after the last operand apply to everything; parenthesised operands keep their own.
- `ORDER BY` names the first select's aliases; an unknown name is an error.
- A `WITH` before the chain is visible on both sides.
- `UNION DISTINCT`.
- A property: a chain of three operands with random operators against the model that applies the precedence.

## Hints

### Two levels are enough

There are two ranks. Write the function for the lower one so that its operands come from a function for the higher one, and the loops fall out.

### Where does the `ORDER BY` go?

It is parsed once, after the whole chain, and attached to the outermost node. Do not let the operand reader consume it. Parentheses are the one place where an operand may carry its own: the recursive call to the query reader handles it naturally.

### Print the grouping

Make the AST (or the plan) display parenthesise the children. A failing test then shows you the tree you built next to the one the test expected.

## Performance

Parsing is linear in the length of the text. Precedence changes the plan, and with it the cost: `a union (b intersect c)` intersects two small tables before the final merge; `(a union b) intersect c` merges the two before intersecting. Evaluating `INTERSECT` first, whenever it may be, usually keeps the intermediate results small.

**Measure it.** Build three 10 000-row tables and compare `explain analyze` of `a union b intersect c` against `(a union b) intersect c`: rows at each node.

## Experiment

Optional. Predict first, then run.

1. **Group `EXCEPT` from the right.** Which test fails and what do the printed trees look like?
2. **Give `INTERSECT` and `UNION` the same rank.** Which two examples of the check differ?

## Other designs

- **Pratt parsing,** with a binding power per operator, the same code as for `+` and `*`.
- **Shunting yard** with two stacks.
- **Flat chain and fix later:** a pass that regroups a flat chain by rank after parsing.

## In BusTub

BusTub's parser never builds set operations. The arithmetic parser of module 3d is the ancestor of this one; a set operation is an operator whose operands are queries.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a table of precedences and a `while` with a `switch` | the same loop with a `match` returning the rank |
| a parser that recurses on `(` via a function pointer | a method that calls itself |

**Port rule:** precedence and associativity are two separate questions; write tests that change only one of them.

## Learn more

- [PostgreSQL: SELECT, UNION clause](https://www.postgresql.org/docs/current/sql-select.html#SQL-UNION) · [Pratt parsing, matklad](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html)
