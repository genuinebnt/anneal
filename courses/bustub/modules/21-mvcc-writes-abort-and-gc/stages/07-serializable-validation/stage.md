Snapshot isolation allows write skew: two transactions read the same snapshot and write different rows. The **serializable** level adds a check at commit time: if anything the transaction read has been changed by a transaction that committed since it began, the transaction aborts. To run the check the transaction must remember what it read, as **predicates**, and the check evaluates them on the before and after images of every tuple recently committed transactions wrote.

## The task

**Recording.** In `init` of `SeqScanExecutor` and of `IndexScanExecutor` (regions `4b-08`): if the transaction's isolation level is `Serializable`, record the scan with `txn.append_scan_predicate(table oid, the scan's filter predicate or true_predicate())` (`true_predicate` is given: a scan with no filter read everything).

**Checking.** In `src/concurrency/transaction_manager.rs`, `verify_txn(txn) -> bool`, the region marked `4b-08` (called by `commit` for serializable transactions; a `false` aborts the transaction and makes `commit` return `false`):

- a transaction with no writes or no recorded predicates passes (it read one consistent snapshot);
- otherwise take the transactions that are `Committed` with `commit_ts > txn.read_ts()`; for each tuple in their write sets of a table `txn` scanned, rebuild the tuple **as of `commit_ts - 1`** and **as of `commit_ts`** (use a throwaway `Transaction::new(INVALID_TXN_ID, ..)` with `set_read_ts(ts)`, `collect_undo_logs`, `reconstruct_tuple`); if any of `txn`'s predicates on that table is true for either version, return `false`;
- if nothing matches, `true`.

The tests: exact scenarios (a serializable transaction whose reads changed fails to commit; a failed validation undoes the transaction's writes; transactions that read different things both commit; snapshot isolation does not validate; a full scan conflicts with any change in the table; of two concurrent swaps exactly one commits; a row deleted after it was read still fails the reader), and a property: **serializable transactions that each read one key and write another, interleaved at random**: whichever commit (the others fail on a conflict or at validation) read what they read and leave the table in the state of **some serial order** of just those transactions. The oracle tries every order.

## Your freedom

How you record predicates (one per scan, as given) and how you evaluate them (before and after images, as described); the commit outcome is fixed by the serial-order property.

## The Rust toolbox

**A throwaway reader.** `let reader = Transaction::new(INVALID_TXN_ID, IsolationLevel::SnapshotIsolation); reader.set_read_ts(ts);` lets you reuse `collect_undo_logs` and `reconstruct_tuple` to see a tuple as of any timestamp.

**Both images.** For each tuple written by a recently committed transaction, evaluate your predicates on the version at `commit_ts - 1` (what it was) and at `commit_ts` (what it became): a row that was *deleted* only matches before, a row *inserted* only after.

**`any` with a lenient error.** `preds.iter().any(|p| p.evaluate(..).map_or(true, |v| v.as_bool() == Some(true)))` treats an evaluation error as 'might match'.

```rust
let reader = Transaction::new(INVALID_TXN_ID, IsolationLevel::SnapshotIsolation);
reader.set_read_ts(ts);                                         // pub(crate): you are inside the crate
let logs = collect_undo_logs(rid, &meta, &tuple, link, &reader, self)?;
let version = reconstruct_tuple(&table.schema, &tuple, &meta, &logs)?;   // None: did not exist at ts
pred.evaluate(&version, &table.schema)?.as_bool() == Some(true)
```

## Design notes

**Two images.** The before image catches a changed row that *used* to match (an update away from `a = 0` removes a row the reader counted); the after image catches one that *now* matches (a phantom). An insert has only an after image and a delete only a before image: `reconstruct_tuple` returns `None` for the missing one.

**Timestamps are dense.** Commit timestamps count 1, 2, 3 without gaps, so `commit_ts - 1` is the version just before. (The reconstruction also yields the transaction's own predecessors' versions in a chain of several changes by the same transaction: one log per tuple per transaction.)

**An error is a conflict.** If a predicate cannot be evaluated (a type error), treating it as a match is the safe answer for a validator; the reference does that.

**Why read-only transactions skip.** They read a snapshot at their read timestamp; they can be serialised there. No later commit can invalidate that.

**Cost.** The check is `O(committers since begin x tuples written x predicates)`. The concurrent test makes both transactions write 1 000 rows; the commit mutex serialises the two checks, so exactly one of them sees the other's commit.

## If this is new

- L7 Enums & pattern matching: `Option` ladders in the validation.
- S6 Iterators: `any`, `filter`, nested loops over write sets.
- Y5 Testing & verification: an oracle that tries every serial order.
- The optional *serializable validation by predicates* concept.
- [S1 Option & Result](/t/s1-option-result): Understand it: `Option` chains for links and logs.

## Tests

- Reads that changed; undone writes; independent readers; no validation under snapshot isolation; full scans; swaps; a read row deleted since.
- Property: committed transactions equal some serial order.

## Hints

### Check both images

If you only test the after image, a transaction that moved rows *out of* a scanned range goes unnoticed.

### Use the same predicate object

`append_scan_predicate` deduplicates by pointer, so an inner side of a join that is initialised many times records its predicate once. Clone the `Arc`, don't rebuild the expression.

### Don't hold locks you don't need

`commit` holds `commit_mutex` while it validates (that is what makes exactly one of two conflicting commits win). `verify_txn` only needs read access to `txn_map` and the catalog.

## Performance

Validation re-evaluates predicates on up to two versions of every tuple that recently committed transactions wrote. With many committers, long-running serializable transactions and wide write sets it dominates commit; snapshot-isolation transactions skip it, so the cost is only paid by those who ask for serializability.

**Measure it.** Commit a serializable transaction after 1, 10 and 100 other transactions each wrote 100 tuples; the time grows linearly with the number of recent writes.

## Experiment

Optional. Predict first, then run.

1. **Only the after image.** Check just `commit_ts`. Which test fails (a deleted row)?
2. **Validate everything.** Validate read-only transactions too. Which test fails, and was it wrong or just slower?

## Other designs

- **Backward validation with predicates (ours, BusTub's).**
- **Serializable snapshot isolation** (PostgreSQL): track read-write dependencies and abort on dangerous structures.
- **Two-phase locking:** readers lock what they read.
- **Timestamp ordering.**

## In BusTub

`transaction_manager.cpp`: "`/** @brief Verify if a txn satisfies serializability. We will not test this function and you can change / remove it as you want. */ auto TransactionManager::VerifyTxn(Transaction *txn) -> bool { return true; }`" and in `Commit`: "`if (txn->GetIsolationLevel() == IsolationLevel::SERIALIZABLE) { if (!VerifyTxn(txn)) { commit_lck.unlock(); Abort(txn); return false; } }`". `Transaction` has `AppendScanPredicate`/`GetScanPredicates`. The tests are `SerializableTest` and `ConcurrentSerializableTest` ("`EXPECT_EQ(success_cnt, 1)`").

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `txn->AppendScanPredicate(oid, pred)` | `txn.append_scan_predicate(oid, pred)` |
| `pred->Evaluate(&tuple, schema).GetAs<bool>()` | `pred.evaluate(&tuple, schema)?.as_bool() == Some(true)` |
| a fake `Transaction` for reading as of a timestamp | the same: `Transaction::new` and `set_read_ts` |

**Port rule:** reading "as of a timestamp" is the visibility rule with a made-up reader.

## Learn more

- [Precision locking and validation (Neumann et al. 2015, section 3.5)](https://db.in.tum.de/~muehlbau/papers/mvcc.pdf) · [PostgreSQL: Serializable isolation level](https://www.postgresql.org/docs/current/transaction-iso.html#XACT-SERIALIZABLE)
