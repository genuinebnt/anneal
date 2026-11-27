A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`SavepointTxn` in `src/concurrency/savepoints.rs`: a transaction over a `BTreeMap<i64, i64>` with `set(k, v)`, `delete(k)`, `savepoint()` (returns an id), `rollback_to(id)` (undo everything done after the savepoint, keeping the transaction open), `release(id)` and `commit()` (returns the final map). Undo is done by keeping, for every change, what was there before.

## Why

`SAVEPOINT` is how a client retries part of a transaction after a constraint violation without losing the rest, and how a stored procedure with an exception handler undoes just its own work. It needs the same undo information as a full abort, used selectively: roll back to a mark, drop the marks above it, and leave the transaction running.

## The contract

- `savepoint()` returns an increasing id. `rollback_to(id)` restores the state at that savepoint and removes every savepoint created after it (the savepoint itself stays usable). `release(id)` forgets the savepoint and all after it (their changes stay).
- An unknown or already released id is `Err(NoSuchSavepoint)` and changes nothing.
- `rollback_all()` restores the state at the start of the transaction; `commit()` returns the current map.

## Invariants

These must hold after every step, whatever the input:

- After `rollback_to(id)` the map equals the map at the moment `savepoint()` returned `id`.
- Savepoint ids are never reused.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `savepoint(); changes...; rollback_to` leaves no trace of the changes.
- Nested savepoints roll back independently, innermost first.
- The final map equals replaying only the changes that were not rolled back.

## Examples

Worked cases (the tests include them):

```text
set 1=1; sp A; set 1=2; sp B; set 2=9; rollback_to A -> {1: 1}; B is gone
```

## What the tests check

- Set, delete, rollback, nested savepoints.
- Releasing and unknown ids.
- A property against full snapshots.

## Done when

All the `s4b_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Undo/redo for a text editor](/cookbook/p/undo-redo-editors): undo stacks of inverse operations, the mechanism behind savepoints.
- [Sqlx with SQLite](/cookbook/p/sqlx-sqlite-queries-transactions): savepoints used from application code.
