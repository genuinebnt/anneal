Nothing new to write. The last stage of the module tests what the six before it do **together**, with the laws that relate the operators to each other. A LEFT join is the inner join plus the unmatched left rows. A RIGHT join is a LEFT join of the swapped tables. A FULL join is a LEFT join plus the unmatched right rows. `UNION` is the distinct rows of `UNION ALL`. The rows of `INTERSECT` and of `EXCEPT` add up to the distinct rows of the left side. If any of these fails for some pair of random tables, one of the earlier stages has a bug that its own tests missed, and the failing input, shrunk to a few rows, points at it.

## The task

Make the tests of this stage pass. They are:

- **A script** (`tests/sql/sql_joins_and_sets.slt`) with joins of all four kinds, a filter that selects unmatched rows, and each set operation, with expected output including NULL rows.
- **The join identities** on random tables with NULLs and duplicates: LEFT = INNER + unmatched left rows; `a RIGHT JOIN b` = `b LEFT JOIN a`; FULL = LEFT + unmatched right rows.
- **The set identities:** `UNION` = distinct `UNION ALL`; `(a INTERSECT b) + (a EXCEPT b)` = distinct `a`; `INTERSECT` is symmetric.
- **The optimizer:** random filters over random joins of all four kinds, joined by `UNION ALL` to another query, return the same rows as the plan with only the starter rules.

## Your freedom

Everything. A failure here is a bug in an earlier stage, and the minimal failing input says which.

## The Rust toolbox

**Reading a shrunk counterexample.** proptest prints the smallest input it found. Re-run it by hand in the SQL shell with and without `set force_optimizer_starter_rule = yes`: if the two differ, the optimizer is wrong; if they agree and both differ from the model, the executor is.

**Model first.** Every property compares the engine with a few lines of Rust over vectors. If the model and the engine disagree, decide who is right by working the smallest case on paper.

## If this is new

- Y5 Testing & verification: properties, models and shrinking.

## Tests

- The script, line by line.
- Identities for the join kinds and for the set operations.
- Optimized equals plain for random queries.

## Hints

### Shrink, then bisect the layers

A failing property gives you two or three rows. Run the query without the rules, then with only pushdown, then with simplification as well. The first run that changes the answer names the rule.

### Totals first

If a row count is off, count the pieces the identity names (matched pairs, padded left, padded right). The piece that is wrong names the executor path.

## Performance

The property tests run hundreds of small queries in well under a second. The same queries on 100 000-row tables show what the module bought: a filter pushed below a join cuts the nested loop's comparisons by the selectivity of the filter.

**Measure it.** Build `a` and `b` with 3 000 rows each; time `a left join b on a.x = b.x where a.y < 30` with and without `set force_optimizer_starter_rule = yes`.

## Experiment

Optional. Predict first, then run.

1. **Break one rule on purpose** (let a LEFT join take a filter on its right side). Which test is the first to fail, and how small is the counterexample?
2. **Make `INTERSECT ALL` use `max` instead of `min`.** Which identity catches it, if any? What property would?

## Other designs

- **SQLite as an oracle:** run the same random queries in SQLite and compare (the idea of differential testing; it supports every operator here, `RIGHT` and `FULL` joins since 3.39).
- **Metamorphic testing:** rewrite the query in an equivalent way (swap the operands of a `UNION`, turn a LEFT join into a RIGHT join of the swapped tables) and compare the two answers.

## In BusTub

BusTub's grading compares the output of SQL scripts with expected output. The identities here are the kind of test one writes when no oracle is available.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a hand-picked list of inputs and expected outputs | a generator and an invariant |
| `gtest` parameterised tests | `proptest!` with shrinking |

**Port rule:** if you can state the law, a generator will find the counterexample you did not think of.

## Learn more

- [proptest book](https://proptest-rs.github.io/proptest/intro.html) · [PostgreSQL: joins and set operations](https://www.postgresql.org/docs/current/queries-table-expressions.html)
