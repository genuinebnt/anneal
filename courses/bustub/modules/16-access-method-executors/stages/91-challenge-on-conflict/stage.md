A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`upsert` in `src/execution/upsert.rs`: `INSERT ... ON CONFLICT` over a table keyed by `i64` with an `i64` value. Rows of the batch are applied **in order**; a row whose key already exists (including a key inserted earlier in the same batch) is handled by the conflict policy: `DoNothing`, `Replace` (the new value) or `Add` (the sum of old and new value).

## Why

`INSERT` that fails on the first duplicate is useless for loading data, and read-then-insert in the application is a race. Upsert is the standard answer, and its semantics are all in the order and in what is reported: how many rows were inserted, how many changed an existing row, how many were skipped.

## The contract

- `upsert(table, rows, policy)` returns `Counts { inserted, updated, ignored }`.
- A new key is inserted. An existing key: `DoNothing` ignores the row; `Replace` sets the value and counts as updated; `Add` adds the value (wrapping) and counts as updated.
- Rows are applied one after the other, so a second row for the same key in the batch is a conflict with the first.

## Invariants

These must hold after every step, whatever the input:

- `inserted + updated + ignored` equals the number of rows in the batch.
- The table has exactly one entry per key.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- With `DoNothing`, the table afterwards is the old table plus the first row for each new key.
- With `Replace`, it is the old table overridden by the last row for each key.
- With `Add`, each key's value is its old value plus the sum of its rows.

## Examples

Worked cases (the tests include them):

```text
table {1: 10}; rows (1, 5), (2, 7), (2, 1); Add -> {1: 15, 2: 8}; inserted 1, updated 2, ignored 0
```

## What the tests check

- Each policy on a small batch.
- Duplicates inside the batch.
- A property against a model.

## Done when

All the `s3e_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Sqlx with SQLite](/cookbook/p/sqlx-sqlite-queries-transactions): upserts with ON CONFLICT from the client side, against SQLite.
