When a transaction changes a committed tuple it must save the old version, or readers at older timestamps lose it. It saves a **delta**: only the columns that changed, with their old values, stamped with the **old version's timestamp** and linked to the log that was at the head of the chain. That is `generate_new_undo_log`. If the same transaction changes the same tuple **again**, a second log would be wrong: no other transaction ever sees the version in between (it is uncommitted), so the one log must go on restoring the version from *before the transaction*, covering every column the transaction has touched so far. That is `generate_updated_undo_log`: keep the old log's values for the columns it already restores, add the old values of newly changed columns.

> [!CHECK] A committed row is `(1, 2.0, true)`. Transaction T sets `a = 5`, then `b = 7.0`, then sets `a = 1` again (the original value). What does the log hold after each step, which columns does it list as modified after the third, and why is `a` still in it? What are the log's timestamp and previous-version link, and do they change between steps? What does the log look like if the row did not exist before T inserted it?
> ||After step 1: modified `a`, value `1`. After step 2: modified `a, b`, values `1, 2.0` (the old log's value of `a` is kept; `b`'s value from before this change, 2.0, is added). After step 3: still `a, b` with `1, 2.0`: once a column is in the log it stays, even though `a` equals its original value now (the log is a delta from the version before T; cheaper to keep than to remove, and still correct). The timestamp (that of the version T replaced) and the previous link (the old head of the chain) never change after the first log. A row T created has a log that says only "did not exist" (`is_deleted`, no columns), and later changes by T leave it as it is.||
>
> - How is a delete recorded, as a base-to-target change?
> - Which tuple do you compare, base with target, to find the changed columns?
> - Two NULLs: changed or not? A NULL and a value?

## The task

In `src/execution/execution_common.rs`:

`generate_new_undo_log(schema, base_tuple: Option<&Tuple>, target_tuple: Option<&Tuple>, ts, prev_version) -> UndoLog`: `base_tuple` is the tuple **before** the change (`None`: it did not exist), `target_tuple` the tuple **after** (`None`: the change is a delete); `ts` is the timestamp of the base version and `prev_version` the log that was the head of the chain. No base: `is_deleted = true`, no modified columns, an empty tuple (`Tuple::empty()`). No target (a delete): every column is modified and the tuple is the whole base tuple. Otherwise a column is modified when its value differs (`same_value`, given: two NULLs are equal, a NULL and a value differ), and the tuple holds the base tuple's values of exactly the modified columns under `get_undo_log_schema`. Always set `ts` and `prev_version`.

`generate_updated_undo_log(schema, base_tuple, target_tuple, log: &UndoLog) -> UndoLog`: `base_tuple` is the tuple before *this* change (the transaction's own earlier version), `target_tuple` after it (`None` for a delete), `log` the log the transaction already has. If `log.is_deleted`, or `base_tuple` is `None` (the transaction deleted the tuple earlier, so its log already covers every column), return the log unchanged. Otherwise the new log restores every column the old log restores **with the old log's values**, and also every column this change modifies that the old log did not cover (for a delete, all of them) with `base_tuple`'s values; `ts` and `prev_version` stay as in `log`.

The tests: exact scenarios (changing some columns logs those columns with their old values; the log remembers the timestamp and previous version; deleting logs every column; a tuple that did not exist gets a "did not exist" log; a NULL that stays NULL is not a change; to or from NULL is; a log reconstructs the version it was made from; a second change adds the new columns and keeps the old values; changing a column again keeps its original value; the timestamp and link do not change; deleting after a partial change covers every column; a "did not exist" log stays; changing a tuple the transaction deleted leaves the full log), and a property: **for random chains of versions** (cells that go to and from NULL, creations and deletions), the log of the first change restores the version it replaced, and after any number of further changes by the same transaction the single folded log still restores the original, with the timestamp and link unchanged.

## Your freedom

How you compare (`same_value` per column) and how you build the partial tuple (collect the values of modified columns).

## The Rust toolbox

**`filter` and `collect` for the partial tuple.** `(0..n).filter(|&i| modified[i as usize]).map(|i| base.get_value(schema, i)).collect::<Vec<_>>()` builds the values of the changed columns in table order.

**Struct update syntax.** `UndoLog { modified_fields, tuple, ..old.clone() }` copies the other fields from the old log.

**A walk with a read cursor.** To merge the old log's values (in partial order) with new ones (in table order), keep `next_old` that advances only when the old log modifies the column.

**`Option` arguments.** `base_tuple: Option<&Tuple>`; `let Some(base) = base_tuple else { return log.clone() };` returns early for "the transaction deleted it earlier".

**`clone` of a log.** `log.clone()` copies the deltas; they are small.

## If this is new

- [S1 Option & Result](/t/s1-option-result): `let else`, `Option<&T>` arguments.
- [S3 Vec & slices](/t/s3-vec-slices): building vectors by `filter`/`map`/`collect`.
- S8 The core traits: why `same_value` is not `==`.
- The optional *rolling back with undo logs* concept (an abort uses these logs).
- L7 Enums & pattern matching: Patterns in depth: undo log entries; `Option<Vec<_>>` as a three-way answer.
- Y5 Testing & verification: Build it: a model of versions; random histories and sessions.

## Tests

- First log: some columns; timestamp and link; delete; did-not-exist; NULL rules; round trip.
- Updated log: new columns added, old values kept, original values survive repeated changes, fixed timestamp and link, deletes, did-not-exist logs.
- Property: first and folded logs restore the original over random change sequences.

## Hints

### Compare the right pair

For the first log compare *base* with *target*: the columns that this change modifies. For an updated log compare this change's base with its target, and only add columns the old log does not already have.

### The old value wins

For a column the log already restores, keep the logged value (from before the transaction), not `base_tuple`'s value (which is the transaction's own earlier write).

### A delete restores everything

If the new version is a delete, every column must be restorable: add all the columns the log does not cover, with `base_tuple`'s values.

## Performance

A log costs a copy of the changed columns only, which is why wide rows with narrow updates are cheap under this design. Folding a transaction's repeated writes into one log keeps chains short: a transaction that updates a row a hundred times leaves one log, not a hundred.

**Measure it.** Update one column of a 50-column row in a loop of 1 000 updates in one transaction, and compare the size of the log with whole-tuple logs.

## Experiment

Optional. Predict first, then run.

1. **A log per change.** Make every change a new log with its own timestamp. Which test and which property fail, and what would a reader at an older timestamp see?
2. **Drop columns that went back.** Remove from the folded log a column whose original value was restored. Is the log still correct? Is it still *cheaper*?

## Other designs

- **Fold repeated writes into one log (ours, BusTub's).**
- **A log per statement** (with a statement counter for savepoints and partial rollback).
- **Whole-tuple logs:** simpler, larger.
- **Redo/undo records in the write-ahead log** (module 4c) as the only history.

## In BusTub

Project 4 of the 2025 course: `watermark.cpp`, `transaction_manager.cpp` (`Begin`, `Commit`, `Abort`), `execution_common.cpp` (`ReconstructTuple`, `CollectUndoLogs`, `GenerateNewUndoLog`, `GenerateUpdatedUndoLog`) and the transactional path of `seq_scan_executor.cpp`. BusTub's MVCC keeps the newest version in the table and the older ones as **undo logs**, each a delta that turns a version into the previous one, chained from the tuple.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::optional<Tuple> base_tuple` | `Option<&Tuple>` |
| `UndoLog{is_deleted, modified_fields, tuple, ts, prev_version}` aggregate | the struct literal, `..old.clone()` for the unchanged fields |
| a loop building `std::vector<Value>` | `.filter(..).map(..).collect()` |

**Port rule:** an aggregate-initialised struct stays a struct literal; a loop that pushes into a vector may become an iterator chain.

## Learn more

- BusTub's [Project 4 page](https://15445.courses.cs.cmu.edu/fall2025/project4/) · Oracle's undo segments, for a real implementation of the same idea
