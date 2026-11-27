A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`resolve_column` in `src/sql/resolve.rs`: given the tables in the `FROM` clause (each with an alias and its column names), find which table and column a name refers to. A name may be qualified (`t.x`) or not (`x`); matching is case-insensitive. An unqualified name that exists in more than one table is **ambiguous**.

## Why

`SELECT id FROM a JOIN b` is an error in SQL, and the error message is the binder's job: *which* column, in *which* tables. Name resolution is small but it is where `unknown column`, `ambiguous column` and `missing FROM-clause entry` come from, and getting the three apart is what makes errors usable.

## The contract

- `resolve_column(tables, qualifier, name)` returns `Ok((table_index, column_index))`.
- With a qualifier: the table with that alias, else `Err(UnknownTable)`; then the column in it, else `Err(UnknownColumn)`.
- Without: every table that has the column; none is `Err(UnknownColumn)`, two or more is `Err(Ambiguous(table indexes))`.
- A table that lists the same column name twice makes an unqualified or qualified lookup of it ambiguous too (`Ambiguous` with that table's index repeated).

## Invariants

These must hold after every step, whatever the input:

- A successful answer names a column that exists, in a table in scope.
- The answer does not depend on the case of the input.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a table that lacks the name never changes the answer.
- Adding a second table that has the name turns a success into `Ambiguous`, unless the lookup is qualified.
- A qualified lookup is never ambiguous across tables.

## Examples

Worked cases (the tests include them):

```text
a(id, x), b(id, y): id -> Ambiguous([0, 1]); a.id -> (0, 0); y -> (1, 1); z -> UnknownColumn; c.id -> UnknownTable
```

## What the tests check

- Qualified, unqualified, ambiguous, unknown.
- Case-insensitivity.
- A property against a brute-force search.

## Done when

All the `s3d_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Catalog, types, expressions](/cookbook/p/catalog-types-expressions): name resolution that fails on unknown and ambiguous column references.
