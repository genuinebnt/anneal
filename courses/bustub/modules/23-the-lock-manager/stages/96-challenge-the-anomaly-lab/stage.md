A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

A small database `LabDb` (given, in `src/concurrency/lab_db.rs`) that runs at one of five isolation levels and is driven one step at a time, so a transaction that would have to wait says `Blocked` instead of waiting. In `src/concurrency/anomaly_lab.rs` you write five **scenarios**, one per anomaly: dirty read, non-repeatable read, lost update, write skew and phantom. Each plays a few transactions against the database and returns whether the anomaly **actually happened**, whatever the level. Then you write `possible(level, anomaly)`: your prediction of the table, which the tests compare with what the scenarios observe.

## Why

Isolation levels are easy to recite and hard to apply: "repeatable read prevents non-repeatable reads" is a tautology until you can name the interleaving, and the surprising entries (a locking repeatable read stops write skew, a snapshot does not; a snapshot stops phantoms, row locks do not) are exactly the ones the names hide. Building each anomaly as an interleaving is how the mechanisms stick: every "no" in the table is a lock that blocks or a version that is not visible.

## The contract

- The levels, mechanically: **read uncommitted** reads other transactions' uncommitted writes and takes no read locks; **read committed** reads the newest committed value; **repeatable read** takes a shared row lock on every row it reads and holds it to the end; **snapshot** reads as of its start and aborts a writer whose row was committed over since it began (first committer wins); **serializable** is repeatable read plus a lock on the whole scanned range, so inserts into a scanned range are blocked.
- Writes take an exclusive row lock at every level. A step that conflicts returns `Blocked` and changes nothing; a transaction that is aborted (by snapshot's rule) returns `Aborted` and is finished.
- Starting data (the tests load it): dirty read and non-repeatable read: `{1: 100}`; lost update: `{1: 0}`; write skew: `{1: 1, 2: 1}` (two doctors on call; at least one must stay); phantom: `{1: 10, 2: 20}`.
- Each scenario must finish every transaction it starts (commit or abort) and must cope with `Blocked` and `Aborted`: a step that was prevented means the anomaly did not happen, not that the scenario failed.
- `possible(level, anomaly)` is true when that level lets the anomaly happen.

## Invariants

These must hold after every step, whatever the input:

- A scenario returns `true` only if the interleaving really produced the anomaly (the two reads differ, the update is gone, the constraint is broken).
- No transaction is left open when a scenario returns.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- At read uncommitted every anomaly is possible; each level above prevents at least one more.
- If `possible(level, a)` is false then the scenario for `a` returns false at `level`, and the other way round.
- Lock-based repeatable read and snapshot differ in exactly two anomalies, in opposite directions.

## Examples

Worked cases (the tests include them):

```text
dirty read at read committed: the reader sees 100, not the 999 that was written and rolled back -> false
phantom at snapshot: both scans return the same two rows, the inserted row 50 is not in the snapshot -> false
```

## What the tests check

- Each anomaly at each of the five levels (25 observations).
- Your table against what the scenarios saw.
- Every transaction finished.

## Done when

All the `s4d_c7` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Transaction theory](/cookbook/p/transaction-theory-serializability): Adya's anomalies, and which isolation level forbids each.
- [Concurrency control theory](/cookbook/p/concurrency-control-theory): dirty reads, unrepeatable reads and lost updates as conflicts.
- [MVCC snapshot isolation](/cookbook/p/mvcc-snapshot-isolation): write skew under snapshot isolation, and SSI preventing it.
