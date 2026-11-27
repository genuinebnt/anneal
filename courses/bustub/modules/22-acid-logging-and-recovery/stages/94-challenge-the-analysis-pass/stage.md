A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`analyze` in `src/recovery/analysis.rs`: the first pass of ARIES-style recovery. Starting from the state recorded in the last **checkpoint** (the active transactions and the dirty page table at that moment) and scanning the log records after it, rebuild what was true at the crash: which transactions were still active (their work must be undone) and which pages were possibly dirty, with the LSN of the first record that dirtied each (where redo must start).

## Why

Recovery cannot trust anything in memory; it has the log. Analysis is the cheap pass that reads the log tail once and produces the two tables the redo and undo passes run from. It is also the pass most easily broken by a missing case: a transaction that began after the checkpoint, one that ended after it, a page dirtied again after being dirtied before.

## The contract

- Records: `Begin(txn)`, `Update(txn, page)`, `Commit(txn)`, `Abort(txn)` (an abort finished its rollback and ends the transaction).
- Start with `checkpoint.active` and `checkpoint.dirty` (page -> rec_lsn). `Begin` adds the transaction. `Update` adds the transaction if unknown and records `page -> lsn` if the page is **not already** in the dirty table. `Commit` and `Abort` remove the transaction.
- `analyze(checkpoint, records_after)` returns `Analysis { active, dirty }`.

## Invariants

These must hold after every step, whatever the input:

- `dirty[page]` is always the LSN of the first update that dirtied it since it was last known clean (the checkpoint's value if present).
- A committed or aborted transaction is not in `active`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Analyzing the whole log from an empty checkpoint equals analyzing a suffix from the checkpoint taken at its start.
- Appending a `Commit` removes exactly one transaction and leaves `dirty` unchanged.
- Re-dirtying a page never changes its rec_lsn.

## Examples

Worked cases (the tests include them):

```text
checkpoint: active {1}, dirty {A: 10}; records: Update(1,A)@12, Begin(2)@13, Update(2,B)@14, Commit(1)@15 -> active {2}, dirty {A: 10, B: 14}
```

## What the tests check

- Each record type; checkpoint state carried over.
- Unknown transactions that appear in updates.
- A property against replaying the whole log.

## Done when

All the `s4c_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Build WAL and ARIES](/cookbook/p/build-wal-and-aries): the analysis pass rebuilding the transaction and dirty page tables from a checkpoint.
- [ARIES crash recovery](/cookbook/p/aries-recovery): analysis, redo and undo from a fuzzy checkpoint.
- [Crash recovery (ARIES)](/cookbook/p/crash-recovery-aries): the three ARIES passes and the tables they use.
