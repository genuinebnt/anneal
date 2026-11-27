Every program that talks to a database eventually builds a query out of a string and a user-supplied value, and the way it does so is either a prepared statement or a security incident. `select * from users where name = ?` is parsed once; the value is handed over separately and becomes a *node in the syntax tree*. A name that contains a quote cannot change the shape of the statement, because the shape was fixed before the name existed. The mechanism is small: a `?` token, a placeholder in the syntax tree, a pass that counts them, and a pass that replaces each with a literal. Its consequences (no injection, no re-parsing, a place to cache plans) are why every database driver has it.

> [!CHECK] The value `x'; drop table users; --` is bound to `where name = ?`. Describe exactly what the statement the engine runs looks like. At which step could the quote have mattered, and why does it not?
> ||The statement is `select * from users where name = <string literal x'; drop table users; -->`: one comparison whose right side is a string constant that happens to contain those characters. The quote would matter to the **lexer**, which finds where a string literal ends by looking for a closing quote. Binding happens after lexing and parsing, on the tree: the value is placed in a node that already *is* a literal, so no tokenisation of its contents ever takes place.||
>
> - How is the number of placeholders known, and when is it checked?
> - What should `select ... where a = ?` do when run as plain SQL, with no values?
> - Where in a statement can a placeholder not stand?

## The task

- **Lexing and parsing:** `?` is a token and a placeholder expression; placeholders are numbered from 0 in the order they appear in the text, across all statements of one `prepare`. (The AST variant `Expr::Param(i)` is given, and so is `visit_exprs_mut`, which calls a closure on every expression of a statement, subqueries and CTEs included.)
- **`BusTubInstance::prepare(sql)`** parses once and returns a `PreparedStatement` with `param_count()`.
- **`BusTubInstance::execute_prepared(&prepared, &values, writer)`** runs the statements with `values[i]` in place of the `i`th `?`. The number of values must equal `param_count()` (else an `Invalid` error and nothing runs). INTEGER types become integer literals, DECIMAL a decimal literal, VARCHAR a string literal, BOOLEAN a boolean literal, and a NULL of any type a NULL literal; other types are `NotImplemented`. The prepared statement itself is not changed: it can be executed again.
- **Plain SQL with `?`:** running it through `execute_sql` is an error that says no value was given (the binder's `Expr::Param` arm); it must not be treated as NULL.

Where to work: the lexer's symbol table and the parser's `primary` (the stage's regions are marked), and `src/common/prepared.rs`.

## Your freedom

How the placeholder count is kept (a counter in the parser, or a pass over the tree: the tests only observe `param_count()`), how a value becomes a literal, and whether you substitute into a clone of the tree or into the tree itself (the tests execute one prepared statement many times).

## The Rust toolbox

**A visitor as a closure.** A walker with a signature like `fn visit_mut(tree: &mut [Node], f: &mut dyn FnMut(&mut Leaf))` hands your closure every leaf mutably, and `*leaf = replacement` swaps one in place. The SQL syntax tree (`src/sql/ast.rs`) has such a walker for expressions.

**Capturing state in a closure.** A counter or an `Option<Error>` captured by the visitor is how an error leaves a callback that cannot return one.

**Clone before you change.** A value that must be used again with other inputs is never edited in place: `let mut draft = template.clone();` and the changes go into the copy.

## If this is new

- [L2 Borrowing](/t/l2-borrowing): a closure that mutates captured state while it is itself borrowed.
- [S1 Option and Result](/t/s1-option-result): an error that has to escape a `FnMut`.

## Tests

- Placeholders are counted in order, in VALUES, BETWEEN, LIMIT and OFFSET.
- Values take their place left to right; reuse with other values.
- A string that looks like SQL stays a string.
- A wrong number of values is an error; NULL, decimal and boolean values; UPDATE and DELETE with placeholders; `?` as plain SQL fails.

## Hints

### Count and bind with the same walk

Both are "visit every expression, look for `Param`". If counting and binding disagree about what a placeholder is, you get off-by-one values in the middle of a statement.

### Check the count first, mutate second

A statement bound halfway and then failed on a bad value is harder to reason about than one that was never touched. Validate everything, then substitute.

### The lexer is the first place a new symbol must go

`?` has to be a token before the parser can see it. If you wrote your lexer in 3d-03, its symbol table is where the new character goes; the regions of this stage mark the spot in the reference.

## Performance

A prepared statement saves tokenising and parsing, which is a small part of a query's cost: microseconds against the planner's and executor's milliseconds. What it really saves for a client that runs the same statement thousands of times is network and parsing round trips, and what it makes *possible* is plan reuse, which this engine does not do (every execution plans again with the values in the tree). The cost model to know is the one that bites later: a cached plan was chosen without seeing the parameter, so it may be wrong for some of them.

**Measure it.** Execute `select ... where a = ?` 10 000 times through `execute_sql` with formatted text and through `execute_prepared`. How much of the difference is parsing?

## Experiment

Optional. Predict first, then run.

1. **Bind by formatting the value into the SQL text and parsing again.** Which test fails (the one with a quote), and what does the failing statement run?
2. **Number the placeholders in the order the planner visits them** instead of the order of appearance in the text (for `select ? from t where a = ? order by ?`, where a planner may meet the WHERE first). What goes wrong?

## Other designs

- **Plan caching:** keep the optimized plan and re-evaluate it with parameters bound at execution time (PostgreSQL's generic plans); fast for the same plan, wrong for skewed parameters.
- **Named parameters** (`:name`, `$1`): the same value can be used twice and order does not matter; needs a map from name to position.
- **Server-side prepared statements over a wire protocol** (Parse/Bind/Execute): the statement lives in the session; the client sends only the values.

## In BusTub

BusTub's shell executes text and has no `PREPARE`. Its parser (libpg_query) would accept `$1` parameter references, and the binder has no handling for them. This stage adds the whole path.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `sqlite3_prepare_v2` / `sqlite3_bind_int` / `sqlite3_step` | `prepare` / `execute_prepared(&p, &[Value])` |
| a visitor class with `Visit(Expr&)` overloads | a function taking `&mut dyn FnMut(&mut Expr)` |
| `snprintf` into a buffer, "escaping" quotes | never: the value is not in the text |

**Port rule:** data and code travel on separate channels. The moment a value is formatted into the program text, it can change the program.

## Learn more

- [PostgreSQL: PREPARE](https://www.postgresql.org/docs/current/sql-prepare.html) · [SQLite: prepared statements](https://www.sqlite.org/c3ref/stmt.html)
