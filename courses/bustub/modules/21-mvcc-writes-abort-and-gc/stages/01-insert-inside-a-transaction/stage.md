Until now an `INSERT` wrote tuples with timestamp 0: everybody sees them at once. Inside a transaction a new tuple must be invisible to everyone else until its writer commits. The tool is the **temporary timestamp** from module 4a: the tuple is stamped with its transaction's id, no reader can see it (the id is at least 2^62), and `commit` later replaces it with the commit timestamp for every rid in the **write set**.

## The task

In `src/execution/execution_common.rs`, `insert_mvcc(txn, txn_mgr, table, indexes, tuple)`: the region marked `4b-01` (the last lines of the function):

- insert `tuple` into the table heap with `TupleMeta { ts: txn.temp_ts(), is_deleted: false }`;
- add the new rid to the transaction's write set (`txn.append_write_set(table.oid, rid)`).

In `src/execution/executors/insert_executor.rs`, the region marked `4b-01` at the top of `next`: when `self.txn` is `Some`, produce the same one-row answer as the non-transactional path (the number of rows inserted), but insert each tuple with `insert_mvcc` instead of writing the heap and the indexes directly. (Only tables **without** a primary key go through the simple path here; stage 6 adds the index.)

The tests: exact scenarios (an inserted tuple is visible only to its transaction; insert reports how many rows it inserted; the tuple carries the temporary timestamp until commit; a commit makes it visible to later transactions only; an `INSERT .. SELECT` does not see its own new rows). Sessions of interleaved writers come in stage 3, where updates and deletes join in.

## Your freedom

Almost none: two small regions. The design is in the timestamp rule of module 4a; here you only apply it.

## The Rust toolbox

**Routing on the transaction.** `if let Some((txn, mgr)) = &self.txn { .. return Ok(..) }` handles the transactional case first and leaves the module 3e path below untouched.

**The write set is a map of sets.** `txn.append_write_set(table.oid, rid)`; adding a rid twice is harmless.

**A temporary timestamp.** `TupleMeta { ts: txn.temp_ts(), is_deleted: false }`: bigger than any commit timestamp, so only the writer reads it as its own.

```rust
table.table.insert_tuple(&TupleMeta { ts: txn.temp_ts(), is_deleted: false }, tuple)?   // Result<Rid>
txn.append_write_set(table.oid, rid);
if let Some((txn, txn_mgr)) = &self.txn { ... }                                          // the transaction of this statement
```

## Design notes

**Why no undo log.** A new tuple has no previous version: a reader that cannot see the tuple (its timestamp is too new) finds an empty chain and concludes "did not exist", which is correct. An undo log is only needed when a version is *replaced*.

**Why the write set.** Commit and abort must find every tuple the transaction touched without scanning tables. The write set is a map from table oid to a set of rids; inserting the same rid twice is harmless.

**The Halloween guard comes for free.** The sequential scan below an `INSERT ... SELECT` stops at the table's end as it was when the scan began (module 3c), so the rows being inserted are not read again, whether or not there is a transaction.

**One statement, many tuples, one failure.** If inserting row 3 of 5 fails (stage 6: a duplicate key), the first two are already in the write set. The statement returns an error; the transaction is tainted; abort will undo the two.

## If this is new

- S7 Smart pointers & interior mutability: `Arc<Transaction>` and its `&self` methods.
- L7 Enums & pattern matching: `Option<(Arc<Transaction>, &TransactionManager)>` as the 'is there a transaction' switch.
- The optional *multi-version concurrency control* concept.

## Tests

- A new tuple is invisible to everyone but its writer until commit; the count is reported; the tuple carries the temporary timestamp and joins the write set; commit publishes it; `INSERT .. SELECT` ignores its own rows.

## Hints

### The temporary timestamp is the transaction's id

`txn.temp_ts()`, not its read timestamp: a read timestamp is an ordinary small number that other transactions may compare as "old enough".

### Register the rid even if you think nothing needs it

Commit stamps only the rids in the write set. A tuple that is not in it keeps the temporary timestamp forever and stays invisible to every later transaction.

### Keep the dispatch at the top of `next`

The simple path below it must stay for statements outside transactions; check `self.txn` first and return from the region.

## Performance

An insert in a transaction costs what a plain one did, plus a hash-set insert into the write set (amortised constant). The write set grows with the transaction: a transaction that inserts a million rows keeps a million rids until it ends, and commit walks them all once.

**Measure it.** Insert 20 000 rows in one transaction (about 20 ms in release mode, including parsing and planning), then commit (about 1 ms): the commit is a latched metadata update per rid. For the same rows inserted by 20 000 separate transactions the per-commit overhead dominates.

## Experiment

Optional. Predict first, then run.

1. **Forget the write set.** Skip `append_write_set`. Which test fails first (hint: commit)?
2. **Stamp with zero.** Insert with `ts: 0`. Who sees the row, and which test says so?

## Other designs

- **Temporary timestamp in the tuple (ours, BusTub's).**
- **A separate 'uncommitted' area** merged at commit (some in-memory engines).
- **Commit-time stamping lazily** at the first read (PostgreSQL's hint bits).

## In BusTub

The starter's `insert_executor.cpp` is the Project 3 executor; Project 4 changes it: tuples are inserted with the transaction's temporary timestamp as `TupleMeta`'s `ts_`, and "`txn->AppendWriteSet(plan_->GetTableOid(), rid)`" records them. `TxnExecutorTest.InsertTest` and `InsertCommitTest` check exactly the visibility rules above.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `exec_ctx_->GetTransaction()` | `ctx.txn()` (an `Option`: `None` runs without versions) |
| `TupleMeta{txn->GetTransactionTempTs(), false}` | `TupleMeta { ts: txn.temp_ts(), is_deleted: false }` |
| `txn->AppendWriteSet(oid, *rid)` | `txn.append_write_set(oid, rid)` |

**Port rule:** an executor that behaves differently inside a transaction branches once on `Option<(Arc<Transaction>, &TransactionManager)>`, taken from the context in `new`.

## Learn more

- [`Option::zip`](https://doc.rust-lang.org/std/option/enum.Option.html#method.zip) · [PostgreSQL: how a row version is stamped (xmin)](https://www.postgresql.org/docs/current/ddl-system-columns.html)
