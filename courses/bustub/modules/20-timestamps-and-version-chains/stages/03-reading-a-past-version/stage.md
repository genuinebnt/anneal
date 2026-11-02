In this MVCC the table holds the **newest** version of every tuple, with the timestamp of the transaction that wrote it. Older versions live in **undo logs**^[An undo log is stored next to the table, not inside it, so the table page stays one tuple per slot and a reader that wants the newest version never touches the logs.]: each is a delta that, applied to a version, gives the previous one: the columns that changed, with their old values (or "the tuple did not exist", or "the tuple was deleted and here is what it was"). The logs of a tuple form a **chain**^[A link of the chain is not a pointer but a pair (transaction id, index in that transaction's log list), so a log can be found again after the transaction has moved or finished.], newest first. Two functions turn that structure into what a reader sees. `collect_undo_logs` walks the chain and decides which logs a transaction needs: none if the table's version is already visible to it; otherwise the logs up to and including the first one old enough. `reconstruct_tuple` applies those logs, in order, to the table's version.

> [!CHECK] A tuple was written at timestamp 2, 5 and 9. The table holds the version of 9; the chain holds logs with timestamps 5 then 2. What does a reader at 6 collect, and what does it see? A reader at 1? A reader at 9? What does a reader at 3 see if the version at 5 *deleted* the tuple? Why does `reconstruct_tuple` apply logs front first (newest first) and let later logs overwrite earlier ones?
> ||At 6: the table's version (9) is too new; the first log has ts 5 ≤ 6, so it collects just that log and sees the version of 5 (the table's tuple with the 5-log applied: the log restores the version from before 9, i.e. the one written at 5). At 1: every version is newer: the chain ends without an old-enough log, the tuple did not exist (`None`). At 9: the table's version is visible, no logs. If the version at 5 was a delete, a reader at 6 collects the log of ts 5 which is a "deleted" marker or a full restore of the deleted state, and the reconstruction says the tuple does not exist; a reader at 3 collects both logs and sees the version of 2. Logs apply newest first because each describes how to get from a version to the one before it: applying them in the order of the chain walks back in time, and when two logs restore the same column the older log's value is the older version's value, so the later application must win.||
>
> - How does the chain end: an invalid link, or a log whose transaction was garbage collected?
> - What does `None` mean from `collect_undo_logs`, and what does `None` mean from `reconstruct_tuple`?
> - What if the table's version was written by the reader itself?

## The task

In `src/execution/execution_common.rs`:

`reconstruct_tuple(schema, base_tuple, base_meta, undo_logs) -> Option<Tuple>`: start from the values of `base_tuple`; the tuple does not exist if `base_meta.is_deleted`; apply **every** log in `undo_logs`, front first, whatever its timestamp; a log with `is_deleted` makes the tuple not exist; any other log writes its old values into the columns where `modified_fields[i]` is true (the log's tuple holds *only* those columns, in table order, under `get_undo_log_schema(schema, &log.modified_fields)`, given); if the tuple does not exist when a restoring log comes (a deleted base, or after a deleting log) start from all NULLs (`null_values(schema)`, given) and then apply it; return `None` if the tuple does not exist at the end, else `Some`.

`collect_undo_logs(rid, base_meta, base_tuple, undo_link, txn, txn_mgr) -> Option<Vec<UndoLog>>`: if the table's version is visible to `txn` (`base_meta.ts <= txn.read_ts()`, or `base_meta.ts == txn.temp_ts()`: it wrote it itself) return `Some(vec![])`; otherwise follow the chain from `undo_link` (`None` or an invalid link is an empty chain): fetch each log with `txn_mgr.get_undo_log_optional(link)`, add it to the result and, if its `ts <= txn.read_ts()`, stop and return the logs, else continue with its `prev_version`; if the chain ends without a visible version, or a log cannot be fetched (garbage collected), return `None`: the tuple did not exist for this transaction.

> [!ASIDE] Why not rebuild the tuple after every log?
> Applying the logs to a list of values and building the tuple once at the end is cheaper and simpler than creating a tuple per log:
>
> ```rust
> let mut values: Vec<Value> = (0..n).map(|i| base.get_value(schema, i)).collect();
> for log in undo_logs { /* overwrite the columns this log restores */ }
> Tuple::new(values, schema)
> ```

The tests: exact scenarios for `reconstruct_tuple` (no logs gives the base; a deleted base with no logs does not exist; a full log brings a deleted tuple back; stale bytes in the slot of a deleted tuple do not leak; partial logs restore only their columns; the last log wins; a deleting log) and `collect_undo_logs` (committed at or before the read timestamp needs no logs; the transaction's own write needs none; a newer tuple without a chain did not exist yet; another transaction's uncommitted tuple is not visible; the chain is followed until a version old enough; a chain of only newer versions; a garbage-collected log ends the search), and a property: **for a row with a random history** (updates, deletes, re-inserts, columns going to and from NULL, optionally an uncommitted version) **a reader at every timestamp collects and reconstructs exactly the newest version at or before its timestamp**, a reader before the first version sees nothing, and the writer sees its own uncommitted version while everyone else sees the history.

## Your freedom

How you walk the chain (a loop, recursion), and how you build the tuple (a `Vec<Value>` modified in place and turned into a tuple at the end).

## The Rust toolbox

**A loop on an `Option`.** `let mut link = undo_link; while let Some(l) = link.filter(|l| l.is_valid()) { let log = txn_mgr.get_undo_log_optional(l)?; link = Some(log.prev_version); ... }`: `?` on an `Option` returns `None` for the whole function when a log is gone.

**Early `return Some(..)` inside the loop** when a log is old enough; `None` after the loop for "the chain ended".

**Partial tuples.** `get_undo_log_schema(schema, &modified)` is the schema of the columns that changed; `log.tuple.get_value(&partial, k)` reads the k-th changed column; keep a counter `k` that advances only for modified columns.

**Building values.** `let mut values: Vec<Value> = (0..n).map(|i| base.get_value(schema, i)).collect();` then overwrite entries; `Tuple::new(&values, schema)` at the end.

**`Option` as "did it exist".** Return `None` for a deleted result; callers use `?` or `match`.

## If this is new

- [S1 Option & Result](/t/s1-option-result): `?` on `Option`, `filter`, `while let`.
- [S3 Vec & slices](/t/s3-vec-slices): building and overwriting `Vec<Value>`.
- L7 Enums & pattern matching: `Option<Vec<UndoLog>>` as three-way answer (visible / logs / nothing).
- The optional *version chains and undo logs* concept.
- Y5 Testing & verification: Build it: a model of versions; random histories and sessions.

## Tests

- Reconstruct: the base, a deleted base, full and partial logs, stale bytes, order, deletes.
- Collect: visible bases, own writes, newer tuples, other transactions' uncommitted tuples, chain walking, chains of only newer versions, collected logs.
- Property: a reader at every timestamp sees the newest version at or before it, over random histories.

## Hints

### "Visible" has two meanings

The table's version is visible to you if it is committed no later than your read timestamp *or* it is yours (temporary timestamp). Both give "no logs".

### Stop at the first old-enough log, including it

The log whose `ts` is at or below your read timestamp is the one that restores the version you want: it is part of the answer, then you stop.

### Two kinds of nothing

`collect_undo_logs` returning `None` means no version of the tuple existed for you (or its history was freed). `reconstruct_tuple` returning `None` means a version existed and it was a delete. The scan treats them alike; the garbage collector does not.

## Performance

A read of an old version costs one log fetch per version between the table and the reader's snapshot: long chains slow old readers, which is the price of non-blocking reads and the reason garbage collection (module 4b) matters.

**Measure it.** Update one row 1 000 times with an old reader waiting, then time the reader's read of it; repeat after garbage collection.

## Experiment

Optional. Predict first, then run.

1. **Strict inequality.** Change `<=` to `<` in the visibility test. Which tests and which property fail first?
2. **No NULL reset.** Remove the restart from all NULLs after a delete. What breaks, and why does a well-formed log hide it?

## Other designs

- **Newest in the table, deltas in undo logs (ours, BusTub's, Oracle-like).**
- **Newest in the table, old versions as whole tuples in a separate area** (PostgreSQL keeps every version in the heap).
- **Delta chains oldest first** (append-only): readers walk forward from a snapshot.
- **Version tables per column** for wide rows.

## In BusTub

Project 4 of the 2025 course: `watermark.cpp`, `transaction_manager.cpp` (`Begin`, `Commit`, `Abort`), `execution_common.cpp` (`ReconstructTuple`, `CollectUndoLogs`, `GenerateNewUndoLog`, `GenerateUpdatedUndoLog`) and the transactional path of `seq_scan_executor.cpp`. BusTub's MVCC keeps the newest version in the table and the older ones as **undo logs**, each a delta that turns a version into the previous one, chained from the tuple.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::optional<std::vector<UndoLog>> CollectUndoLogs(...)` | `Option<Vec<UndoLog>>` |
| `txn_mgr->GetUndoLogOptional(link)` returning `std::optional<UndoLog>` | `get_undo_log_optional(link)?` |
| `std::optional<Tuple> ReconstructTuple(...)` | `Option<Tuple>` |

**Port rule:** `std::optional` is `Option`, and its early-out checks are `?`.

## Learn more

- *An Empirical Evaluation of In-Memory Multi-Version Concurrency Control* (Wu et al., VLDB 2017) · BusTub's [Project 4 page](https://15445.courses.cs.cmu.edu/fall2025/project4/)
