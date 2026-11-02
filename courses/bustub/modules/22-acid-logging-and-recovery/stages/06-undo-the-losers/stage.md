After redo the pages hold everything that was logged, including the changes of transactions that never finished. **Undo** removes them: walk the log **backwards**, and for every change of a loser put its slot back in the `before` state. Two details make this a recovery and not a rollback. Each undo is **logged** (a change with the two states swapped), so that if the machine dies again halfway through recovery, the next recovery repeats history, undo steps included, and only has the rest left to do. And after the last undo of a loser an `Abort` record is logged and the log is flushed, which makes the transaction *finished*: a second recovery finds nothing to undo. Finally `recover` writes every page to disk, so that the pages themselves are now a complete, correct database.

> [!CHECK] Transaction T inserted a record and then updated it, and never committed. Redo has applied both. In what order does undo apply them, and what does it append to the log? Why does a *second* recovery after this one find no losers? Then: a transaction was rolled back at run time (its undo steps and `Abort` are in the log), and afterwards another transaction updated one of the same records and committed. Why is it essential that the first transaction is *not* a loser?
> ||Newest first: the update is undone (the record goes back to its inserted value), then the insert is undone (the record goes back to no live record); the log gets those two swapped changes, then `Abort` for T. After that T has an `Abort` record, so analysis counts it finished. If the run-time rollback were treated as a loser, recovery would undo its (already undone) changes again, writing the *old* values over the record that the second transaction committed: durable data destroyed by recovery. Logging every undo and ending with `Abort` is what prevents it.||
>
> - What if two losers both changed different records in the same page?
> - What if the crash came after some of a rollback's undo records but before its `Abort`?
> - Which LSN order must the undo records follow?

## The task

In `src/recovery/recover.rs`:

- `undo(store, log, records, losers) -> StoreResult<usize>`: walk `records` **newest first**; for every `Change` of a transaction in `losers`: append to the log the change with `before` and `after` swapped (same `txn` and `rid`), and put the slot in its `before` state (`set_state`); count it. Afterwards append an `Abort` record for each loser and **flush the log**. Return how many changes were undone.

(`recover(store, log)`, given, is: read the durable records, `analyse`, `redo` from `analysis.redo_from`, `undo` the losers, flush every page.)

The tests: exact scenarios (a loser's insert, update and delete are all rolled back; undo logs what it does and ends each loser with `Abort`; several losers and a winner in the same pages; recovery leaves the pages on disk so that a second crash needs no log; recovering twice changes nothing the second time; a rollback that was half logged when the crash came is finished), and a property: **any run, then a crash at that moment** (whichever pages and log records had reached the disk): recovery leaves **exactly the records of the transactions that committed**, every committed transaction is reported finished, no committed transaction is a loser, and recovering again is a no-op.

## Your freedom

How you iterate (`records.iter().rev()` once for all losers is the intended design) and how you record the undo (append inside the loop).

## The Rust toolbox

**Backwards iteration.** `records.iter().rev()` walks the slice from the end without copying.

**Struct update syntax for a swap.** `LogRecord::Change { txn: *txn, rid: *rid, before: after.clone(), after: before.clone() }`: the record is built from the one you are undoing with the two fields exchanged.

**`contains` on a small slice.** `losers.contains(txn)` is fine here; for many losers build a `HashSet`.

**Convert the error.** `log.flush().map_err(|e| refused(e.to_string()))?` turns an I/O error into the store's error type.

**Count what you did.** The returned number is part of the contract (the tests compare it with the number of changes of the losers).

## If this is new

- S6 Iterators: `rev()`, `iter()` over slices.
- [S1 Option & Result](/t/s1-option-result): `map_err` and `?` across error types.
- Y5 Testing & verification: a model of the committed state, crash at every moment.
- The optional *rolling back with undo logs* concept.
- L8 Error design: Errors at scale: a recovery that never panics on a damaged log; `io::Result` and `?`.
- L7 Enums & pattern matching: Enums as design: a log record as an enum, matched in analysis, redo and undo.

## Tests

- A loser's insert, update and delete; the logged undo and `Abort`; several losers; pages written after recovery; idempotence; a half-logged rollback.
- Property: a crash anywhere leaves exactly the committed state, and recovery repeats harmlessly.

## Hints

### Undo *after* redo, never instead

Undo trusts that the pages hold what the log says. Run it only after redo, and always walk the whole slice backwards: a loser's changes are interleaved with the others'.

### A half-finished rollback

If a loser's log already holds some undo records (swapped changes), treat them like any other change: undoing an undo re-applies the original. The end state is right, and the extra records are harmless.

### Flush at the end

If the `Abort` records are not durable, the next recovery repeats the same undo: correct but wasted. Flush so that it is done once.

## Performance

Undo reads the log backwards: every loser's changes are touched once. A real system chains each transaction's records through a "previous LSN" and follows the chain, so that a loser with three changes in a log of millions costs three reads; and logs a *next-undo LSN* in each undo record so that a repeated undo skips what was undone.

**Measure it.** Recover a log with a million changes of finished transactions and one loser with ten changes; time undo.

## Experiment

Optional. Predict first, then run.

1. **Undo oldest first.** Reverse the order. Which test fails, and on which kind of loser (one that changed the same record twice)?
2. **Do not log the undo.** Skip the appended records. Which property catches it, and what has to happen in a run for it to show?
3. **No `Abort`.** Skip the abort records. What does a second recovery do?

## Other designs

- **Undo with compensation records and an `Abort` (ours).**
- **ARIES's** next-undo-LSN chain: skips what was already undone.
- **No undo at all:** MVCC engines like PostgreSQL leave aborted versions invisible and clean them up later.
- **Shadow paging:** changes go to new pages and a commit swaps a root pointer; nothing to undo.

## In BusTub

BusTub's 2025 projects stop before recovery: Project 4 is concurrency control. The lectures on logging and recovery (CMU 15-445) teach the write-ahead log and ARIES, and BusTub's older years had a `LogManager` and `LogRecovery` that students filled in. This module is new in this course: a small store that logs before it writes, and recovery you can crash at every step.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `for (auto it = log.rbegin(); it != log.rend(); ++it)` | `for (_, r) in records.iter().rev()` |
| a compensation record with `undo_next_lsn_` | a swapped change record and an `Abort` |
| `LOG_INFO` and asserts after recovery | the test's model of the committed state |

**Port rule:** reverse iterators become `.rev()`.

## Learn more

- ARIES paper, section "Undo and compensation log records" · PostgreSQL's [crash recovery](https://www.postgresql.org/docs/current/wal-intro.html)
