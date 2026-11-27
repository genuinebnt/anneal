A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`EvolvingTable` in `src/catalog/evolving_table.rs`: a table of integer rows whose schema can change. `add_column(default)` appends a column that existing rows read as `default`, **without touching them**; `drop_column(i)` removes a column from every row's view. `get(rid)` always returns rows in the current schema.

## Why

`ALTER TABLE ADD COLUMN ... DEFAULT 0` on a table of a billion rows must not take hours. PostgreSQL and MySQL both made it a metadata-only change for exactly this reason: old rows are stored short and the reader fills in the default. The cost moves from the write to every read, which is cheap, and the bookkeeping is what you build.

## The contract

- `insert(row)` stores a row (its length must equal the current column count; else `None`) and returns a row id.
- `add_column(default)` raises the column count by one; rows inserted before read `default` in the new column, rows inserted after store their own value.
- `drop_column(i)` removes column `i` from every row (false if out of range). `get(rid)` returns the row in the current schema; `columns()` is the current count.

## Invariants

These must hold after every step, whatever the input:

- Every row read has exactly `columns()` values.
- A value read from a column is the one written there, or the column's default if the row predates it.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a column never changes the values of the columns that existed.
- Adding then dropping the last column restores every row's view.
- The result equals rewriting every row eagerly at each schema change.

## Examples

Worked cases (the tests include them):

```text
insert [1,2]; add_column(7); insert [3,4,5]; get(0) = [1,2,7]; drop_column(0); get(0) = [2,7]
```

## What the tests check

- Defaults for old rows; new rows with their own values.
- Drops, in any position.
- A property against an eager model.

## Done when

All the `s3c_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Change data capture and logical replication](/cookbook/p/cdc-logical-replication-log): ADD COLUMN ... DEFAULT applied lazily on read, with no table rewrite.
- [Delta Lake style table format](/cookbook/p/delta-lake-transaction-log): schema evolution where files written before ADD COLUMN read the column as NULL.
