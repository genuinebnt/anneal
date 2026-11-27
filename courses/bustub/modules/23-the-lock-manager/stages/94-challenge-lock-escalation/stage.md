A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`Escalator` in `src/concurrency/escalation.rs`: bookkeeping for **lock escalation**. `row_lock(txn, table, row)` records a row lock; when a transaction holds **more than `threshold`** row locks on one table, they are replaced by a single table lock: the call returns `Escalate { table, released }` listing the row locks given up. Once a transaction holds the table lock, further row requests on that table are `Covered`.

## Why

Every lock costs memory and bookkeeping; a transaction that updates a million rows should not keep a million lock entries. Escalation is the standard answer (SQL Server, DB2, Oracle's alternatives), and the cost is concurrency: other transactions can no longer touch the table's other rows. The mechanism is a counter and a rule about what is subsumed.

## The contract

- `row_lock(txn, table, row)` returns `Granted` (recorded; count at most `threshold` after it), `Covered` (the transaction already holds the table lock), or `Escalate { table, released }` when the count would exceed `threshold`: the rows are released, the table lock is now held and the new row is covered too.
- `holds_table(txn, table)`, `row_count(txn, table)`; `release_all(txn)` forgets everything.
- Duplicate row locks are not counted twice.

## Invariants

These must hold after every step, whatever the input:

- After any call, `row_count(txn, table) <= threshold`.
- A transaction that holds the table lock holds no row locks on that table.
- `released` contains exactly the rows that were held before the escalation.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Escalation happens exactly when the `threshold + 1`-th distinct row is requested.
- Escalating one table never affects the same transaction's rows on another table, or another transaction.
- After escalation every further request on the table is `Covered`.

## Examples

Worked cases (the tests include them):

```text
threshold 2: rows 1, 2 granted; row 3 -> Escalate { released [1, 2] }; row 4 -> Covered
```

## What the tests check

- Counting, escalating, covering.
- Independent tables and transactions.
- A property against a model.

## Done when

All the `s4d_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Two-phase locking](/cookbook/p/two-phase-locking): lock escalation, intention locks, and escalation under concurrency.
