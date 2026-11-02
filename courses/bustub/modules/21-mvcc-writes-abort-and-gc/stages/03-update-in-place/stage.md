An update changes the table's bytes in place, so the old values must go into an undo log first. The first change of a tuple adds a log (stage 2's path); every later change by the same transaction **widens** that log (module 4a's `generate_updated_undo_log`). This stage connects the update executor to the same write path and adds the widening.

> [!CHECK] Two transactions begin at the same time. Both run `UPDATE t SET v = v + 1 WHERE k = 7`. What does the table hold at the end, and what happens to the loser? Which classic anomaly would you get without the conflict rule, and what would the final value be?
> ||The winner's value; the other fails with a write-write conflict and is tainted (it must abort and retry). Without the rule both read the same old value and both write old + 1, so one increment is lost (a **lost update**) and the final value is old + 1 instead of old + 2.||
>
> - Which of the two reaches the tuple first, and what does the second one see in the tuple's timestamp?
> - What happens to a transaction that updated the same row twice itself?
> - What does a reader that began earlier see in the meantime?

## The task

In `src/execution/execution_common.rs`, `update_own_undo_log(txn, schema, own, base, target)` (region `4b-03`): `own` is the link to this transaction's log for the tuple. Read that log with `txn.get_undo_log(own.prev_log_idx)`, build the widened log with `generate_updated_undo_log`, and store it back with `txn.modify_undo_log`.

In `src/execution/executors/update_executor.rs`, the region marked `4b-03` at the top of `next`: when `self.txn` is `Some`:

- read **all** of the child's output first, and make the new tuple for each (`make_new_tuple`, given from module 3e) with its rid;
- if `changes_primary_key` (given) is true, call `update_by_delete_and_insert` (stage 7); otherwise call `modify_tuple(.., Some(&new))` for every change;
- answer one row with the number of tuples.

The tests: exact scenarios (an update is in place and the old version stays for older readers; a tuple the transaction inserted is updated without a log; changing the same tuple again widens its log instead of adding one; a delete after updates makes the log cover every column; updates over a version chain keep every older snapshot; a conflicting updater is tainted and the winner commits), and a property: **a session of interleaved transactions**, a session of interleaved transactions (inserts, updates that add a number, deletes, commits) run through SQL against a model of versions that says, for every transaction at every step, which rows it must see. A writer that meets a newer or uncommitted version of a row must be tainted; everything else must succeed.

## Your freedom

Whether the update executor builds all new tuples first (the intended design) or streams, and how `update_own_undo_log` finds the log (via the link, as given).

## The Rust toolbox

**Collect, then write.** `let changes: Vec<(Rid, Tuple)> = ..collect()` before the first write: the scan sees the table as it was before the statement.

**Widening a log.** `txn.get_undo_log(i)`, `generate_updated_undo_log(..)`, `txn.modify_undo_log(i, new)`.

**Same-length rule.** The page overwrites in place only a tuple of the same length, so an update of a variable-length column that changes the size fails.

```rust
let idx = own.prev_log_idx as usize;
let widened = generate_updated_undo_log(schema, base, target, &txn.get_undo_log(idx));
txn.modify_undo_log(idx, widened);
```

## Design notes

**Own log or not.** In `modify_tuple`, the tuple's link is *this transaction's* when `link.prev_txn == txn.id()`; then the transaction already has a log for it and calls `update_own_undo_log`. A tuple the transaction inserted itself has a timestamp of its own and **no** log: nothing to widen.

**Why collect first.** An in-place update does not add tuples, so the child scan could be consumed while writing; but the decision between "in place" and "delete + insert" (stage 7) needs to know whether *any* change touches the key, and a failure half-way must not leave you unable to explain it. Collecting also means the scan reads the table before any of this statement's own writes, so `SET a = a + 1` adds exactly one per row.

**No real change.** `UPDATE t SET a = 1` where `a` is already 1 still writes the tuple's metadata (the transaction now owns it) and creates a log with no modified columns. That is intended: the tuple is part of the write set, other writers conflict with it, and BusTub's tests expect it.

**Same length.** The table page can only overwrite a tuple with one of the same length. For integer, decimal and boolean columns that always holds; an update that changes the length of a string fails with `Tuple size mismatch`. (A real system would store the tuple elsewhere and leave a forwarding pointer.)

## If this is new

- [S1 Option & Result](/t/s1-option-result): collecting `Result`s, early returns in a loop.
- [S3 Vec & slices](/t/s3-vec-slices): building a vector of changes first.
- Y5 Testing & verification: a session of interleaved transactions against a model.
- The optional *version chains and undo logs* concept.
- L8 Error design: Custom errors: a conflict as an error that also taints the transaction.

## Tests

- In-place updates with readable old versions; no log for own inserts; widened logs; deletes after updates; chains and snapshots; conflicts.
- Property: sessions of writers against a model of versions.

## Hints

### Widening starts from the log you already have

Do not recompute from the tuple's current bytes: they hold this transaction's own earlier writes. The log's columns keep their first values; only new columns take the base tuple's values.

### The base for widening is the tuple *before this statement*

`base` is the table's current tuple (or `None` if the transaction deleted it earlier); `target` is the new one. Both are what `modify_tuple` already has.

### An empty log is still a log

A statement that changes nothing must not skip `modify_tuple`: the next statement relies on the transaction owning the tuple.

## Performance

An update writes one tuple in place (a latched memcpy of the tuple's bytes) and builds or widens one small log; with `k` changed columns, the log holds `k` values. Statements that update the same tuple again and again stay at one log. An update implemented as delete + insert writes two tuples, grows the heap and the indexes for every update, and makes scans walk past tombstones.

**Measure it.** Update one row 10 000 times in one transaction and print the heap's tuple count and the transaction's log count: both stay at 1. Updating all 20 000 rows of a table takes about 14 ms in release mode, round after round, however many committed versions already sit behind them.

## Experiment

Optional. Predict first, then run.

1. **Update the scan's rows while scanning.** Write while iterating the child. What does `SET a = a + 1` do to a table of 3 rows?
2. **Lose an update.** Disable the conflict check. Which property finds it, with how few steps?

## Other designs

- **In-place updates with undo logs (ours, BusTub's).**
- **New version as a new tuple, old one marked** (PostgreSQL): no undo logs, vacuum cleans up.
- **Delta storage** (Oracle undo segments, InnoDB): the same idea with a separate log area.

## In BusTub

`TxnExecutorTest.UpdateTest1` ("no undo log": every update of the transaction's own insert, `CheckUndoLogNum(.., 0)`), `UpdateTest2` ("update applied on insert": `CheckUndoLogColumn(.., 1)` after the first, `2` after the real third update, `3` after all columns) and `UpdateTestWithUndoLog` (a chain below) drive this stage; each ends with `TableHeapEntryNoMoreThan(.., 1)`: "`hint: this is likely because ... you might have implemented your update as delete + insert, so that a new tuple is created every time a tuple gets updated.`".

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `txn->GetUndoLog(idx)`, `txn->ModifyUndoLog(idx, log)` | the same methods, with `usize` indexes |
| the executor keeps a vector of `(RID, Tuple)` | `Vec<(Rid, Tuple)>` collected from the child before writing |

**Port rule:** "read the whole input, then write" is two loops, not one; the first ends before the second begins.

## Learn more

- [In-place updates with undo buffers](https://db.in.tum.de/~muehlbau/papers/mvcc.pdf) (section 3.1) · [`Vec::iter().zip()`](https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.zip)
