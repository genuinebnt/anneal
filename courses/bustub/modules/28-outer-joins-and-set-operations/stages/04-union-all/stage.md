Joins put rows side by side. A **set operation** puts two result sets one on top of the other: this month's orders and last month's, the active users and the archived ones. `UNION ALL` is the plain version, and its meaning is easy to say: the rows of the left query, then the rows of the right query, every duplicate kept. The work in this stage is not the concatenation. It is making a whole query *be* a table: the parser must read the operator, the planner must check that the two sides fit, and the executor must produce the rows. The other three operators of the next stage then need only the executor.

> [!CHECK] `select x, y from a union all select x from b` and `select x from a union all select 'x'`. What should each do, and where in the pipeline (parser, planner, executor) can each be detected? What is the name of the second column of `select x as k, y from a union all select x, z from b`?
> ||Both are errors, detected by the **planner**, which is the first stage that knows the types: the first has 2 columns against 1; the second pairs an integer with a string, which this engine refuses rather than converting (PostgreSQL would also refuse it, but would convert an integer to a decimal). The result's columns are named after the **left** query: `k` and `y`; the names on the right are ignored.||
>
> - Do the rows come in a defined order? What if one side is empty?
> - Can a set operation be used where a table can: in `FROM`, as the source of an `INSERT ... SELECT`, as the inner side of a join?
> - Where does the result get its column names, when several queries could have given them?

## The task

Make `select ... union all select ...` work, with any number of parts (`a union all b union all c`), as a table anywhere a subquery can stand:

- The parser still answers "set operations are not supported" when it meets `UNION`, `INTERSECT` or `EXCEPT` after a query. Make it read them: a reader for the chain `q1 op q2 op q3 ...` that groups from the left is given (stage 3j-06 replaces it with one that knows the precedence), the operator keywords and their AST nodes are given; what is missing is the call that makes a query continue into the chain.
- The planner (`plan_set_op`) plans both sides and checks that they have the same number of columns and the same column types; otherwise it returns an error that names the operator and the two column counts (`Invalid`), or a type mismatch (`MismatchType`). The result's columns are named after the left side.
- The executor (`SetOpExecutor`) reads both sides completely when it is initialised and then hands out the left rows followed by the right rows, in batches.
- The result is usable as a table: in `FROM` with an alias, in a `GROUP BY` query, filtered, joined, and as the source of an `INSERT`.

## Your freedom

Whether the executor streams or buffers (the later operators need to buffer, so the stage's version does), how you store the node in the AST, and how the message of the error is worded beyond naming the operator and the counts.

## The Rust toolbox

**An enum for a family of operations.** `SetOperator::{Union, Intersect, Except}` plus an `all` flag describes six behaviours; one `match` on the pair is the executor's entire dispatch.

**Recursion in the AST.** A query can contain queries; a set operation holds two `Query` values (boxed). Binding and planning recurse over them like any subquery.

**Batches.** `next` is asked for at most `batch_size` rows. Keep an index into the buffered result and return the next slice.

## If this is new

- S5 Enums and pattern matching: an operator and a flag as one `match` on a tuple.
- S4 Smart pointers: `Box` for a recursive type.

## Tests

- `UNION ALL` of two tables returns every row of both, duplicates included, left rows first.
- Chains of three parts and constants (`select 1 union all select 2`).
- Different column counts, and an integer against a string, are errors that name the operator.
- The result as a table: with `GROUP BY`, filtered by `WHERE`, as an `INSERT ... SELECT` source, joined with a table.
- Columns are named after the left side.
- More rows than a batch, and an empty side.
- A property over random tables: the output is the concatenation.

## Hints

### A set operation is a table with two children

In the binder and planner a `UNION` looks like a subquery that has two select statements instead of one. If you have a path for `(select ...) as t`, make this one a sibling of it and reuse what the other does with the alias and the column names.

### Name columns by the left side

The names the user can write after the alias (`t.k`) come from the left query's select list. The right side's names are only checked for count and type.

### The ambiguity trap

`(select x from a) as t join a on t.x = a.x` is ambiguous in this engine (the inner column is called `a.x`). That is the engine's existing behaviour for subqueries; give the column an alias (`select x as k ...`) in your tests.

## Performance

`UNION ALL` is the cheapest operator in the module: no comparison, no hashing; the cost is the cost of the two inputs. This implementation buffers both sides; a streaming version would hand out the left batches as they arrive and use no memory at all, which is how real engines do it.

**Measure it.** Union a 10 000-row table with itself and compare peak memory of the buffered version with a streaming one if you write it as an experiment.

## Experiment

Optional. Predict first, then run.

1. **Take the column names from the right side.** Which test notices, and how would a user notice?
2. **Skip the type check.** What does a union of an integer and a string column print, and where would the error appear instead?

## Other designs

- **Streaming concatenation:** `next` drains the left child, then the right; no buffering.
- **Parallel union:** two threads each produce one side (what a parallel engine does).
- **Type coercion:** accept integer against decimal by converting the narrower type, as PostgreSQL does.

## In BusTub

BusTub's parser rejects set operations. This is an extension that reuses the pieces you built: subquery binding, planner schema renaming and the executor interface.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a class hierarchy `SetOperation : Statement` with `unique_ptr` children | an enum variant holding `Box<Query>` |
| `switch (op)` plus a flag | `match (op, all)` |

**Port rule:** an enum plus one `match` replaces a subclass per variant when the variants share their data.

## Learn more

- [PostgreSQL: combining queries](https://www.postgresql.org/docs/current/queries-union.html) · [Rust book: recursive types with Box](https://doc.rust-lang.org/book/ch15-01-box.html)
