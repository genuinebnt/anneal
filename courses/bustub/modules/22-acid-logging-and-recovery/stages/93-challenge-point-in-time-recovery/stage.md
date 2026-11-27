A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`recover_until` in `src/recovery/pitr.rs`: given a log of `Begin`, `Set(txn, key, value)`, `Commit` and `Abort` records (each with an increasing LSN), rebuild the database **as of** `upto_lsn`: the effect of every transaction whose `Commit` record has `lsn <= upto_lsn`, applied in log order. Transactions that had not committed by then (still running, aborted, or committed later) have no effect.

## Why

"Restore the database to just before the bad `DROP TABLE`" is point-in-time recovery, and it is nothing more than recovery that stops early. The base backup plus the log up to a chosen LSN gives the state at that moment; the one subtle point is that a transaction either committed at or before the point, and counts completely, or it does not count at all.

## The contract

- `recover_until(log, upto_lsn)` returns the key-value map.
- A transaction's `Set` records take effect, in LSN order among all committed transactions, iff its `Commit` is in the log at an LSN `<= upto_lsn`.
- Records after `upto_lsn` are ignored. A transaction with `Abort`, or with no outcome by `upto_lsn`, has no effect.

## Invariants

These must hold after every step, whatever the input:

- The result depends only on the records with `lsn <= upto_lsn`.
- Every key in the result was set by a transaction committed by `upto_lsn`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `recover_until(log, a)` followed by the committed effects between `a` and `b` equals `recover_until(log, b)`.
- Raising `upto_lsn` past the end of the log gives the full recovery.
- An aborted transaction never appears, at any `upto_lsn`.

## Examples

Worked cases (the tests include them):

```text
log: B1 S1(k=1) C1@4 B2 S2(k=1,v=2) C2@8: recover_until(5) -> {k: 1}; recover_until(9) -> {k: 2}
```

## What the tests check

- Commits before and after the point.
- Aborted and unfinished transactions.
- A property: monotone extension and equality at the end.

## Done when

All the `s4c_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Cloud-native OLTP](/cookbook/p/cloud-native-oltp): point-in-time recovery as a read of the database at an older LSN.
