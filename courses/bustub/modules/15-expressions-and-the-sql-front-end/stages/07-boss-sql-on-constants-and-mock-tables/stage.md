**Where this fits.** Your lexer, your parser, your expression nodes and your function factory, joined by the given binder and planner, are a SQL calculator. This stage checks the whole path against two judges: an **oracle** written for the occasion (random integer and boolean expressions against a 64-bit and a three-valued model) and **BusTub's own SQL tests**, run by a port of its `sqllogictest` runner.

> [!CHECK] A random expression like `3 - -2 + null > 4 or true` goes through five layers. If the answer is wrong, how do you tell whether the lexer, the parser, the planner or an expression node is at fault, without a debugger? What would you print, and at which layer?
> ||Bisect by layer, outside in: tokenize the text and read the tokens; `parse_expr` it and read the tree (wrong precedence shows here); `explain` the statement to see the planned nodes (a wrong operator or type shows here); evaluate the planned expression alone with constants (a wrong value shows here). The first layer whose output disagrees with what you expect from the layer before is the culprit.||
>
> - Which layer owns `3 - -2`: is the second minus an operator or a sign?
> - Where does `null > 4` become a BOOLEAN NULL?
> - Which layer reports an overflow?

## The task

Nothing new to write. Make both pass:

- **`stages_3d::s3d_07`**: for random integer expressions over `+`, `-`, constants, NULLs and large numbers, `select <expr>` answers what a 64-bit oracle does (the value, `integer_null`, or an overflow error); for random boolean expressions (comparisons of such integers combined with `and`/`or`) it answers what Kleene's logic does; a syntax error is reported as a parse error; comments, case and white space do not matter.
- **`slt_expressions_test`** (`cargo test --test slt_expressions_test`): five tests, one per BusTub file (`p0.01-lower-upper.slt`, `p0.02-function-error.slt`, `p0.03-string-scan.slt`, `baby_arithmetic.slt`, `intro.slt`): expressions on constants (`select 1 + 2 + 3 + null;`), `lower`/`upper` with right and wrong arguments, and scans of the built-in `__mock_*` tables with expressions over their columns. Each starts a fresh database (`tests/slt/mod.rs`, given), creates the mock tables and runs the file's records in order.

## Your freedom

None new: if a test fails, the fix belongs in one of your earlier stages.

## The Rust toolbox

**A `.slt` file is a list of records separated by blank lines:**

```text
statement ok                      # must run without an error  ("statement error": must fail)
create table t(a int);

query rowsort                     # must return exactly these rows; rowsort: in any order
select a + 1 from t;
----
2
3
```

Cells are separated by one space; NULLs print as `integer_null`, `varlen_null`, ...; a decimal has six digits (`3.140000`).

**Reading a failure.** The runner stops at the first failing record and prints `file:line`, the SQL, and the first rows you produced and expected. Run the same SQL in the shell (`cargo run --bin bustub_shell`), put `explain` in front, and compare.

**A proptest counterexample** prints the SQL text of the shrunk expression: paste it into the shell and bisect by layer as in the question above.

## If this is new

- Everything is in the earlier stages of this module.
- Y5 Testing & verification: Build it: a model for three-valued logic; print-then-parse round trips; fuzzing a lexer and a parser.

## Tests

- Random integer expressions against a 64-bit oracle (values, NULL, overflow).
- Random boolean expressions against three-valued logic.
- A syntax error is a parse error; comments and case do not matter.
- BusTub's five `.slt` files.

## Hints

### The oracle disagrees only for large numbers

The generator mixes small and near-`i32::MAX` literals. A wrong answer only for big ones is an overflow rule (stage 02), not a parse problem.

### Everything is a bit off by one

Check unary minus (`1 - -5`, `-5 * 2`): it is the lexer or the parser's treatment of the sign, not the arithmetic.

## Performance

Each query parses, binds, plans and executes in well under a millisecond for these inputs; the oracle test runs sixty-four of them. The fresh database (`BusTubInstance::new`) dominates.

## Experiment

Optional. Predict first, then run.

1. **Another operator.** Make the planner accept `*` for integers (`ArithmeticType::Multiply`). Which stage's tests, and which oracle, would you extend?
2. **Fuzz the shell.** Feed random token soup to `execute_sql`. Does anything panic, and in which layer?

## Other designs

None for this stage. The *Other designs* sections of 3d-01 to 3d-06 list the alternatives to compare with yours.

## In BusTub

The `.slt` files are BusTub's own (`test/sql/`), run by its `sqllogictest` runner (`tools/sqllogictest`). The oracle tests are new: BusTub has no property tests for expressions.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `make sqllogictest && ./bin/bustub-sqllogictest ../test/sql/p0.01-lower-upper.slt --verbose` | `cargo test --test slt_expressions_test` |
| `ExecuteSql(sql, writer, ...)` | `BusTubInstance::execute_sql` |

**Port rule:** the test runner is the same idea (records of statements and expected rows) in Rust.

## Learn more

- [sqllogictest: the test file format](https://www.sqlite.org/sqllogictest/doc/trunk/about.wiki) · BusTub's [sqllogictest runner](https://github.com/cmu-db/bustub/blob/master/tools/sqllogictest/sqllogictest.cpp)
