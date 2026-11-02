Now the store gets its transactions. `begin` logs a `Begin`; `insert`, `update` and `delete` each become one **change record** (the slot, its state before, its state after) **appended to the log first**, and only then applied to the page with `set_state`: the **write-ahead rule**. `commit` logs a `Commit` and waits until the log is durable up to it: that is the moment the transaction is safe, whatever happens to the pages. `abort` undoes the transaction's changes newest first, **logging each undo** as a change with the two states swapped, then logs `Abort`. And a page may be written to disk by `flush_page` only after the log is durable up to the last record that touched it.

**The store.** The module's tests work on a small transactional **store** (`src/recovery/store.rs`): a few heap pages in the buffer pool holding fixed-size records of 16 bytes, each addressed by a `Rid`. Transactions insert, update and delete records; `Store::get` and `scan` read them. Changes are applied to the pages at once (no isolation: that was module 4a and 4b), and a record written by an active transaction cannot be written by another until the first one ends.

> [!CHECK] A transaction updates a record and the page holding it is written to disk, but the log record of that update is still in the buffer. The machine dies. What does the restarted system find, and why can it not repair it? Then: why does `commit` have to flush the log but not the pages? And two transactions: A has written record R and not finished. B wants to update R. What should happen, and which crash scenario does the answer protect?
> ||The page on disk has a value that no durable log record explains: if its transaction had not committed, recovery would have to undo it, but the undo needs the *before* image, which was in the lost record: the database is silently corrupt. The rule forbids exactly this by flushing the log up to the page's last record before writing the page. A commit needs only its log records durable, since recovery can rebuild the pages from them; writing pages at commit would be hundreds of random writes. B must be refused until A ends (strictness): otherwise B could commit a change on top of A's uncommitted value and a crash would leave recovery unable to undo A without destroying B's committed work.||
>
> - What does the log record of an insert say about the record's state before?
> - In which order do you append the record and call `set_state`, and what if `set_state` fails?
> - What does `abort` log, and in what order?

## The task

In `src/recovery/store.rs` (`create`, `get`, `scan`, `set_state`, `open` and `flush_all_pages` are given or from stage 3):

- `begin(&self) -> TxnId`: the next transaction id; logs `Begin`; remembers the transaction as active with no changes yet.
- a private `change(txn, rid, before, after)` that every write goes through: refuse if the transaction is not active, or if **another active transaction has written this record** (a hold, released when the writer ends); append `LogRecord::Change { txn, rid, before, after }` **first**; then `set_state(rid, &after)`; then remember the change for rollback, the hold, and the page's last LSN.
- `insert(txn, bytes)`: the first page whose next slot fits the record: its rid is that slot (`None -> Some`). `update(txn, rid, bytes)`: the record must be live (`Some -> Some`). `delete(txn, rid)`: the record must be live (`Some -> None`). Wrong lengths, missing records and records held by others are errors that change nothing and log nothing.
- `commit(txn)`: log `Commit` and make the log durable up to it (`flush_to`); the transaction ends and its holds are released.
- `abort(txn)`: for each of the transaction's changes, **newest first**: append the swapped change (`before` and `after` exchanged) and apply the `before` state; then log `Abort`; release the holds. Nothing needs to be flushed.
- `flush_page(page)`: **first** make the log durable up to the page's last change (`flush_to`), **then** write the page (`bpm.flush_page`).

The tests: exact scenarios (inserts fill pages slot by slot; a write is described to the log before it is made, in the exact shape above; commit makes the transaction and all records before it durable; a page reaches the disk only after its log records; abort undoes newest first and logs each undo; another transaction's record is refused until it ends; bad calls leave no trace in the log), and two properties: **transactions beginning, writing, committing and rolling back in any order with page and log flushes in between** keep the store equal to a model of committed records overlaid with the active transactions' writes; and **no record on a disk page is missing an explanation in the durable log** (the write-ahead rule).

## Your freedom

How you keep the bookkeeping (one `Mutex` around a struct is the intended design: writes one at a time), where the page LSNs live, and how you pick a page for an insert.

## The Rust toolbox

**A struct behind one lock.** `Mutex<Inner>` holds the transaction table, the holds and the page LSNs; every write locks it, so the log order is the page order.

**`HashMap<TxnId, Vec<Change>>`.** The change list of a transaction, newest last; `into_iter().rev()` walks it newest first for rollback.

**Clone before you move.** `before.clone()` into the log record and the original into the change list: the record owns its bytes.

**`Option` states in records.** `LogRecord::Change { before: Option<Vec<u8>>, after: Option<Vec<u8>> }`: an insert is `None -> Some`; the undo of any change is the same record with the fields swapped (`std::mem::swap` or a struct literal).

**Errors that change nothing.** Check everything that can fail (active transaction, hold, record exists, length) **before** the first `append`.

## If this is new

- [C1 Threads & shared state](/t/c1-threads-shared-state): `Mutex` around bookkeeping; why the check and the change are one critical section.
- [S4 Maps & sets](/t/s4-maps-sets): `HashMap`, `retain` to release holds.
- L8 Error design: `StoreResult` with a message.
- The optional *write-ahead logging* and *rolling back with undo logs* concepts.
- S9 I/O & filesystem: Understand: append-only files, `fsync`, what is durable when a call returns.
- Y5 Testing & verification: Build it: crash a disk at a random point, then recover; a model of the committed state.

## Tests

- Insert placement; the exact log records of insert, update and delete; commit durability; the write-ahead rule; abort; holds; bad calls.
- Properties: a model of committed and pending writes; the write-ahead rule over random runs.

## Hints

### The order is the whole point

`append` the record, then `set_state`. If you apply first and log second, a crash between them leaves a change on a page that the log never heard of: the second property finds it with a page flush at the right moment.

### Remember the page LSN for every change, including the undo records

`flush_page` has to know the LSN of the last record that touched the page. An abort's undo records are changes too.

### An aborted insert leaves a deleted slot

Slots are never reclaimed: the undo of an insert is a delete, and the slot stays (the next insert goes to the next slot).

## Performance

A write costs one lock, one append to the log buffer (a `Vec` push) and one page write in memory; none of it touches the disk. A commit costs one log flush. A page flush costs one log flush (if behind) and one page write. The point of the design is that a transaction touching a hundred pages commits with **one** sequential write.

**Measure it.** Run 1 000 transactions of 10 updates each, committing each, then committing every 10th: count log writes.

## Experiment

Optional. Predict first, then run.

1. **Apply first, log second.** Swap the two lines in `change`. Which property finds it, and with which sequence of operations?
2. **Flush pages without the log.** Remove the `flush_to` from `flush_page`. Which test fails first?
3. **Commit without a flush.** What do the commit test and the recovery stages say?

## Other designs

- **Log first, then apply, under one lock (ours).**
- **No-steal / force** (pages are written at commit, uncommitted pages never): no undo or redo needed after a crash but commits are slow.
- **Steal / no-force (ours, ARIES):** pages may be written before commit and need not be written at commit: needs undo and redo.
- **Group commit** and **asynchronous commit** (`synchronous_commit = off` in PostgreSQL): trade durability of the last few commits for speed.

## In BusTub

BusTub's 2025 projects stop before recovery: Project 4 is concurrency control. The lectures on logging and recovery (CMU 15-445) teach the write-ahead log and ARIES, and BusTub's older years had a `LogManager` and `LogRecovery` that students filled in. This module is new in this course: a small store that logs before it writes, and recovery you can crash at every step.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `log_manager_->AppendLogRecord(&record)` then `page->...` | `self.log.append(&record)` then `self.set_state(..)` |
| a transaction object with a `prev_lsn_` and a write set | a `Vec` of changes per transaction in a `HashMap` |
| `bpm_->FlushPage(id)` after `log_manager_->Flush(page_lsn)` | `flush_page` doing both in this order |

**Port rule:** the write-ahead rule is a call order; keep it in one function so that it cannot be bypassed.

## Learn more

- PostgreSQL's [WAL reliability](https://www.postgresql.org/docs/current/wal-reliability.html) · CMU 15-445 lecture "Database logging"
