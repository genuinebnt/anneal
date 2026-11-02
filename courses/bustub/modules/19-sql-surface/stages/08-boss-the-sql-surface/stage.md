Nothing new to write. The last stage of the module runs the features **together**: a script in the BusTub test format that uses every construct of the module on one small table, and two property tests that generate random expressions and conditions out of operators, `CASE`, `COALESCE`, `NULLIF`, `BETWEEN`, `IN`, `IS NULL`, `AND`, `OR` and `NOT`, run them through the engine, and compare every answer with a model written in a dozen lines of Rust. The model is the definition of SQL's three-valued logic as a `match`; the engine is everything you built from the lexer to the executors. If they disagree, proptest shrinks the expression to the smallest one that does and prints it as SQL.

## The task

Make the tests of this stage pass. They are not unit tests:

- **The script** (`tests/sql/sql_surface.slt`): arithmetic, `IS NULL`, `BETWEEN`, `IN` and `NOT IN` with NULLs, `CASE`, `COALESCE`, `NULLIF`, `LIKE`, `LIMIT ... OFFSET`, `DISTINCT` aggregates.
- **Random integer expressions:** `select <expr> from t`, row by row, against the model. Expressions are built from the columns, constants, `+ - *`, unary minus, `x / nullif(y, 0)`, `coalesce`, `nullif` and `case when <condition> then x else y end`.
- **Random conditions:** `select a, b from t where <condition>` returns exactly the rows for which the model says TRUE: comparisons, `is null`, `between`, `in` with NULLs in the list, and `and`, `or`, `not` over those.

## Your freedom

Everything. A failure here is a bug in one of the six stages before: use the shrunk expression to decide which.

## The Rust toolbox

**Reading a shrunk counterexample.** The failure prints SQL such as `select (case when (a is null) then (-b) else (a * 2) end) from t`. Run it on the table by hand, then run each sub-expression: the first one that disagrees with the model is the broken operator.

**`EXPLAIN` the expression.** `explain select ... from t` shows the tree the parser built, which is where a precedence or rewrite bug becomes visible.

## If this is new

- Y5 Testing & verification: generating inputs from a grammar, shrinking, comparing with a model.

## Tests

- The SQL surface script.
- Random integer expressions agree with the model on every row.
- Random conditions select the rows the model selects.

## Hints

### A boolean that is NULL is not a boolean that is false

Most disagreements are `NOT`: `not (a > b)` for a NULL `a` is NULL, not TRUE. If the engine selects the row, the `NOT` (or the comparison under it) forgot the third value.

### The CASE that fails only sometimes

A CASE that errors on some row but not on others is evaluating a branch it did not choose. Shrink to the row (`where` on the values) and look at which division or cast raised.

## Performance

The properties run a few dozen expressions over nine rows; the whole stage finishes in about a second. If it does not, look for something per-row that should be per-statement (a catalog lookup, a `Vec` rebuilt in `evaluate`).

**Measure it.** Run the random expression with 100 000 rows instead of nine: which operator shows up first in a profile?

## Experiment

Optional. Predict first, then run.

1. **Add `LIKE` to the generator** over a string column that is sometimes NULL (built with `nullif`). What new disagreements appear?
2. **Break the model on purpose** (make `NOT NULL` TRUE). Which feature's script lines catch it, and which only the random tests?

## Other designs

- **Differential testing against a real database**: run the same random SQL on SQLite or PostgreSQL and compare results. Finds the cases where the *model* is wrong, which a hand-written model cannot.
- **A SQL fuzzer** that generates statements, not expressions, and checks only "no panic, a sensible error"; cheaper and finds crashes the model never reaches.

## In BusTub

BusTub's `sqllogictest` files check results of fixed queries (the same format as this stage's script). Nothing in BusTub generates queries; the model-based check is this course's addition.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a hand-written list of cases in a `.slt` file | the same, and generated expressions from `proptest` strategies |
| `assert` on one result | `prop_assert_eq!` with the SQL text in the message, and automatic shrinking |

**Port rule:** a model is a second implementation; when engine and model disagree, one of them is wrong, and finding out which is the work.

## Learn more

- [sqllogictest](https://www.sqlite.org/sqllogictest/doc/trunk/about.wiki) · [proptest book](https://proptest-rs.github.io/proptest/intro.html)
