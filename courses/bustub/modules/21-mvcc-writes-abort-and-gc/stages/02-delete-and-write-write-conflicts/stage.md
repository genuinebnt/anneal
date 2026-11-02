A delete in a transaction does not remove anything. It marks the tuple deleted **in place**, stamps it with the transaction's temporary timestamp, and leaves an undo log with the whole tuple so that readers with older snapshots still see it. Before it does any of that it must make sure nobody else wrote the tuple first: two writers of one tuple is a **write-write conflict**, and snapshot isolation resolves it by making the second writer fail.

> [!CHECK] Transaction A reads at timestamp 5 and later tries to update a tuple. Is that a write-write conflict if another transaction B committed an update to it at timestamp 7? And if B has updated it but not committed yet? And if A updated it itself earlier?
> ||Conflict in both of the first two cases: 7 is greater than A's read timestamp, and an uncommitted write carries B's temporary timestamp (a huge number), also greater. Not a conflict in the third: the tuple's timestamp is A's own temporary timestamp.||
>
> - Compare the tuple's timestamp with the read timestamp and with your own temporary one.
> - Why is an uncommitted timestamp always larger than any read timestamp?
> - Why is the check repeated under the page latch?

## The task

In `src/execution/execution_common.rs`:

`is_write_write_conflict(meta, txn) -> bool`: true if the tuple's timestamp is not the transaction's own temporary timestamp and is greater than its read timestamp (either another transaction's uncommitted write, or a commit after this transaction began).

`modify_tuple(txn, txn_mgr, table, rid, target)`, the region marked `4b-02` (`target` is the new tuple, or `None` to delete; stage 3 uses the other half). It reads the tuple, its metadata and its undo link together (given, above the region), then.

`modify_tuple` makes the change visible to this transaction only, and leaves a way back. A write-write conflict (checked on the tuple as read, and again under the page latch when it is written) returns `Err(write_write_conflict(txn))`, which taints the transaction. The first change by this transaction to the tuple records an undo log (`generate_new_undo_log`, whose `prev_version` is the tuple's previous head) and makes it the tuple's new link; a later change keeps the link, and `update_own_undo_log` widens the log (stage 3). The tuple is written with `ts = temp_ts`, `is_deleted = target.is_none()` (a delete keeps the old bytes) and the new link, and its rid joins the write set.

> [!ASIDE] The steps, if you would rather not work them out
> 1. On a conflict return `Err(write_write_conflict(txn))` (given: it taints the transaction and builds the error).
> 2. The version being replaced is `base = None` if the tuple is already deleted, else the tuple.
> 3. If the tuple was **not** written by this transaction: this is the first change. Add `generate_new_undo_log(schema, base, target, meta.ts, previous head)` to the transaction (`txn.append_undo_log`) and use the returned link as the tuple's new link. If it **was** (`meta.ts == txn.temp_ts()`), keep the link; when the transaction already has a log for the tuple (stage 3) `update_own_undo_log` widens it.
> 4. Write the tuple with `update_tuple_and_undo_link`: metadata `ts = temp_ts`, `is_deleted = target.is_none()`, the new bytes (a delete keeps the old bytes), the new link, and a check closure that repeats the conflict test under the page latch.
> 5. If that returned `false`, fail with a conflict. Otherwise add the rid to the write set.

In `src/execution/executors/delete_executor.rs`, the region marked `4b-02`: when `self.txn` is `Some`, call `modify_tuple(.., None)` for every rid the child produces and answer the count. Index entries are **not** removed.

The tests: exact scenarios (the conflict rule for old, current, future and own timestamps; a delete hides the tuple from its deleter only; one full undo log for a deleted committed tuple; none for a tuple the transaction inserted; a second deleter and a deleter that began before a commit conflict and are tainted; a tainted transaction cannot commit or run more statements). The first-updater-wins rule is checked over random interleavings in stage 3.

## Your freedom

How you order the steps inside `modify_tuple` (the check before or after building the log) and how you structure the check closure; the visible behaviour is fixed by the rule above.

## The Rust toolbox

**A closure that runs under a latch.** `update_tuple_and_undo_link(.., Some(&|meta, _, _, _| !is_write_write_conflict(meta, txn)))` passes a closure the page runs while it holds the write latch.

**Early return with a custom error.** `return Err(write_write_conflict(txn))` (taints, then builds the error).

**`Option` chains for the link.** `link.filter(|l| l.is_valid() && l.prev_txn == txn.id())` is 'my own log, if there is one'.

```rust
let (meta, base_tuple, link) = get_tuple_and_undo_link(txn_mgr, table, rid)?;   // given, above the region
let base = if meta.is_deleted { None } else { Some(&base_tuple) };
let new_link = Some(txn.append_undo_log(generate_new_undo_log(schema, base, target, meta.ts, link.unwrap_or_default())));
update_tuple_and_undo_link(txn_mgr, table, rid, new_link, &new_meta, new_tuple,
    Some(&|m, _tuple, _rid, _link| !is_write_write_conflict(m, txn)))?                // Result<bool>
```

## Design notes

**Why check twice.** The first check, on what you read, lets you skip building a log for a doomed write. It is not enough: between your read and your write another transaction can take the tuple. `update_tuple_and_undo_link` runs your `check` under the page's **write latch**, where nothing can change, so the second check is the one that decides. (A log built and then abandoned stays in the transaction: harmless, the transaction is tainted and will abort.)

**Taint, don't just fail.** The error alone makes the *statement* fail. Tainting makes the *transaction* unable to commit: its earlier statements may have written tuples that only make sense with this one. The client's correct response is to abort and retry.

**The link points to the newest log.** `prev_version` of the new log is the old head (`link`), so the chain stays newest-first. For a tuple with no history the previous link is `UndoLink::default()` (invalid), which ends the chain.

**Deleting keeps the bytes.** A tombstone's bytes are never read as data (reconstruction ignores them when it applies a restoring log). Keeping them avoids a length mismatch with the in-place update, which requires the same length.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): why a check must be repeated under the lock that makes it true.
- L8 Error design: errors that also change state (taint).
- [L4 Traits & dispatch](/t/l4-traits-dispatch): closures passed as `&dyn Fn`.
- The optional *snapshot isolation* concept.

## Tests

- The conflict rule; delete visibility; one full undo log; none for own inserts; conflicts taint; tainted transactions cannot proceed.

## Hints

### The check closure sees the tuple as it is *now*

It receives the current metadata. Use that, not the `meta` you read earlier, and do not hold any other lock inside it.

### A delete of a tuple this transaction inserted has no log

`meta.ts == txn.temp_ts()` and no link of its own: skip the log, mark the tuple deleted, keep the link as it is (none).

### `prev_version` is the old head, not "nothing"

Passing the default link for every first change makes the chain forget older versions: readers with older snapshots would find "did not exist". `link.unwrap_or_default()` is `None` only for a tuple with no history.

## Performance

A delete takes the page's read latch to read, builds a log (a tuple copy), and takes the write latch once to write. Two writers of different tuples on one page contend only for the latch, held for the duration of one tuple update. A writer that loses a conflict has done work (a log) that is thrown away: conflicts are cheap but not free, and tests such as `IndexConcurrentUpdateAbortTest` expect a modest abort rate.

**Measure it.** Delete 10 000 distinct rows from 8 threads in 8 transactions and compare with 1 thread; then make all 8 target the same row and count how many fail.

## Experiment

Optional. Predict first, then run.

1. **Check once.** Drop the check under the latch. Which test needs two threads to fail, and can you write it?
2. **Fail without tainting.** Return the error but do not taint. What can the client then do wrong?

## Other designs

- **First updater wins (ours, BusTub's, PostgreSQL's repeatable read).**
- **First committer wins:** check at commit instead of at write (Oracle's older mode); more wasted work, fewer blocked writers.
- **Locks:** a writer waits for the other to finish (two-phase locking).

## In BusTub

`execution_common.h`: "`// * IsWriteWriteConflict`" is among the helper names the reference solution has; `transaction_manager_impl.cpp` provides `UpdateTupleAndUndoLink` ("`Update the tuple and its undo link in the table heap atomically.`") with the `check` callback you use. The tests `TxnExecutorTest.InsertDeleteTest` and `InsertDeleteConflictTest` run these steps; the helper `ExecuteTxnTainted` insists on both: "`hint: if you want to taint a txn, you should both SetTainted + throw ExecutionException.`".

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `txn->SetTainted(); throw ExecutionException("write-write conflict");` | `return Err(write_write_conflict(txn))` (taints, then returns the error) |
| a `std::function` check passed to `UpdateTupleAndUndoLink` | `Some(&\|meta, tuple, rid, link\| ...)`, a `&dyn Fn` |
| `UndoLink{}` for "no previous version" | `UndoLink::default()` (`is_valid()` is false) |

**Port rule:** an exception that ends a statement is an `Err`; the extra side effect (tainting) is done where the error is made, not by the caller.

## Learn more

- [A Critique of ANSI SQL Isolation Levels](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/tr-95-51.pdf) (lost update, first-committer/first-updater wins) · [PostgreSQL: concurrent updates under repeatable read](https://www.postgresql.org/docs/current/transaction-iso.html#XACT-REPEATABLE-READ)
