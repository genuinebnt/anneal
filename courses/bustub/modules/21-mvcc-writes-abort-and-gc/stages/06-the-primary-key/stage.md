Tables with a primary key have an index that must stay consistent with versions. Entries are **never removed** by a delete: a deleted tuple is a tombstone, and the next insert of its key finds the entry and reuses the rid. The index also arbitrates races: two transactions that insert the same new key at once, one of them must fail. And an index scan must hand each rid to the same version-reading code as the sequential scan.

Changing a primary key in place would break the rule from stage 6: a rid belongs to one key for life. So an update that changes the key columns is executed as **a delete of the old tuple and an insert of the new one**. The order matters: `UPDATE t SET k = k + 1` over keys 1 to 4 deletes 1, 2, 3, 4 and inserts 2, 3, 4, 5. If each tuple were deleted and inserted in turn, inserting 2 would hit the live tuple with key 2 and fail. Do all the deletes first.

## The task

**Part 1.** In `src/execution/execution_common.rs`, `insert_mvcc`, the region marked `4b-06` (before the `4b-01` fallback): if `indexes` contains a primary-key index (`is_primary_key`):

- the key is `tuple.key_from_tuple(&table.schema, &pk.key_schema, pk.index.metadata().get_key_attrs())`;
- `pk.index.scan_key(&key)`: if an entry exists, look at the tuple at its rid (`get_tuple_and_undo_link`). **Live** (`!is_deleted`): duplicate key, fail with `write_write_conflict(txn)`. **Deleted**: reuse the rid, `modify_tuple(txn, txn_mgr, table, rid, Some(tuple))` (stage 2/3's path);
- no entry: insert the tuple (temporary timestamp), add it to the write set, then `insert_entry(&key, rid)`. If that returns `false` another transaction got the key between your lookup and now: mark your new tuple deleted (`update_tuple_meta`) and fail the same way.

In `src/execution/executors/index_scan_executor.rs`, the region marked `4b-06` at the top of `next`: when `self.txn` is `Some`, for every rid use `read_visible_version(txn, txn_mgr, table, rid)` (given: tuple + link under one latch, `collect_undo_logs`, `reconstruct_tuple`) instead of reading the heap directly; skip `None`; apply the filter predicate as before.

**Part 2.** In `src/execution/executors/update_executor.rs`, `update_by_delete_and_insert(txn, txn_mgr, changes)` (region `4b-07`; `changes` is the list of `(rid, new tuple)` from stage 3, and `changes_primary_key` already decided this path is needed):

1. for every change, `modify_tuple(txn, txn_mgr, table, rid, None)`: delete the old tuple (its index entry stays);
2. for every change, `insert_mvcc(txn, txn_mgr, table, &self.indexes, new)`: insert the new tuple. A new key that equals a key just deleted finds the tombstone and reuses its rid; a key nobody had gets a new tuple and entry.

The tests: exact scenarios (a duplicate primary key fails and taints; a key committed by somebody else is a duplicate too; a deleted key is reused without a second tuple; index scans see the version of their snapshot; aborting an insert keeps the entry and the next insert reuses it; delete then insert in one transaction reuses the tuple; updating the key moves the row; shifting every key reuses the tombstones it just made; older snapshots still see the old keys; a non-key update stays in place; updating a key onto a live key fails), and a property: **transactions one after another on a table with a primary key**, with inserts, deletes, value updates and key moves, committed or aborted, against a `BTreeMap`: every duplicate fails and taints, every other statement succeeds, the table and every point lookup equal the map after each transaction, every earlier snapshot still reads its state, and the heap never holds more tuples than there are different keys.

## Your freedom

How you detect a live key (through the index and the heap) and how you order the two phases of a key update (the intended design deletes first, then inserts).

## The Rust toolbox

**Look up, then decide.** `pk.index.scan_key(&key)` gives the rid of a tuple with this key, live or dead; the heap's metadata tells which.

**Reuse a rid.** A tombstone is turned back into a live tuple with `modify_tuple(.., Some(new))`, leaving an undo log that says 'it was deleted'.

**The index arbitrates races.** `insert_entry` returns `false` if another transaction added the key first: undo your new tuple (mark it deleted) and fail like a duplicate.

**Two phases.** Collect the changes, delete all old keys, then insert all new ones: `UPDATE t SET k = k + 1` over keys 1 to 4 only works that way.

```rust
let key = tuple.key_from_tuple(&table.schema, &pk.key_schema, pk.index.metadata().get_key_attrs());
pk.index.scan_key(&key)            // Vec<Rid>: the rid(s) under the key
pk.index.insert_entry(&key, rid)   // bool: false if the key is there
read_visible_version(txn, txn_mgr, table, rid)?   // Result<Option<Tuple>>
```

```rust
for (rid, _) in changes { modify_tuple(txn, txn_mgr, self.table_info, *rid, None)?; }
for (_, new) in changes { insert_mvcc(txn, txn_mgr, self.table_info, &self.indexes, new)?; }
```

## Design notes

**A rid belongs to one key for life.** Entries are never removed and a key's rid is reused, so the tuple at a rid always has (or had) that key. An index scan therefore never needs to re-check the key against the reconstructed tuple.

**Why look before inserting, and still handle the failure.** The lookup tells you whether to reuse; it cannot prevent another transaction from inserting the same new key right after. `insert_entry` is atomic and returns `false` for the loser, which is the real decision. The loser's heap tuple is a harmless buried tuple (deleted, temp timestamp, in the write set so commit or abort cleans up).

**Tombstone reuse is an ordinary in-place change.** `modify_tuple` with a deleted base creates a log with `is_deleted = true` and the tombstone's timestamp (readers between the delete and now see "no tuple"), links it in front of the older chain (readers before the delete see the old row), and checks for conflicts: a tombstone whose delete was committed after your snapshot, or deleted by someone still uncommitted, is a conflict, not a reuse.

**Only the primary key.** This course's transactions maintain the primary-key index only. Other indexes are not touched by transactional writes (BusTub's own tests use primary keys); a secondary index in a transactional table can return stale rids.

**Why two loops.** Everything about the order follows from stage 6's rule: an insert onto a key whose tuple is *live* is a duplicate, and onto a key whose tuple is a tombstone is a reuse. The first loop turns all old keys into tombstones (owned by this transaction, with logs), so the second loop's inserts find tombstones for the keys that are being vacated and shifted into.

**What the logs say.** The rid under key 2 is deleted by the first loop (its log keeps the whole old row) and written again by the second loop, in the same transaction. The log is kept as the delete made it, so a reader from before the update who asks for key 2 is given the old row for key 2, which is right for that reader; a reader from after it gets the new row now stored there (the one that came from old key 1). The rid under key 1 stays a tombstone: new readers find nothing there, old readers find the old row.

**Detecting a key change.** `changes_primary_key` (given) compares the primary-key bytes of the old and the new tuple of each change. Comparing the *bytes* of the key tuples is enough because key tuples are built the same way for equal values.

**Failure half-way.** If an insert fails (a live key), the statement errors, the transaction is tainted, and abort undoes the deletes and the inserts so far through the write set.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): calling the index through `Box<dyn Index>`.
- [S4 Maps & sets](/t/s4-maps-sets): the model as a `BTreeMap`.
- Y5 Testing & verification: a model of a key-value table; checking a bound (tuples at most distinct keys).
- The optional *unique indexes and tombstone reuse* concept.

## Tests

- Duplicate keys, reuse, snapshots through the index, aborts, key moves, shifts, in-place non-key updates.
- Property: a primary-key table against a `BTreeMap` with commits and aborts, and the tuple-count bound.

## Hints

### Decide on the tuple's *state*, not on the visible version

A live tuple under the key means "taken", whether or not the inserting transaction could see it. Judging by what *you* can see would let an invisible uncommitted insert be overwritten.

### Add the write-set entry before the index insert

If the index insert fails and you bury the tuple, commit or abort must still find it. Registering after the failure path is a leak.

### The loser must fail the statement

Burying the tuple and returning `Ok` would let the statement report success with nothing inserted. Taint and return the error.

### Do not delete and insert row by row

The second row's insert would run against a table in which the first row's delete has already happened but the second's has not. For a shift this makes the insert collide with a live tuple.

### An insert after your own delete reuses your own tombstone

`modify_tuple` sees `meta.ts == txn.temp_ts()` and a log of its own, so the log is kept as the delete made it. Check `undo_log_columns` in a test: one log, all columns.

### Read the old key from the table, not from the new tuple

`changes_primary_key` needs both keys. The old one is the table's tuple at the rid (the child's visible tuple equals it when there is no conflict).

## Performance

A transactional insert into a table with a primary key costs one index lookup (a B+ tree descent) and either one tuple append plus one index insert, or one in-place change of a tombstone. Reuse keeps both table and index from growing under delete/insert churn, which would otherwise make every scan walk dead tuples. An index scan costs one lookup per key plus one version read per rid.

**Measure it.** Delete and re-insert the same 1 000 keys 100 times and watch the heap's tuple count stay at 1 000.

## Experiment

Optional. Predict first, then run.

1. **Delete entries.** Remove the index entry when a tuple is deleted. Which test fails, and what does an older snapshot's index scan lose?
2. **Insert instead of reuse.** Always insert a new tuple. Which property fails (the bound)?

## Other designs

- **Never remove entries, reuse tombstones (ours, BusTub's).**
- **Index entries per version** (PostgreSQL): the index points at every version; visibility is checked in the heap.
- **Index holds only the newest key, old keys in version storage** (InnoDB's secondary indexes with delete marks).

## In BusTub

`txn_index_test.cpp`: `IndexInsertTest` ("`WithTxn(txn1, ExecuteTxnTainted(*bustub, _var, _txn, "INSERT INTO maintable VALUES (1, 1)"))`"), `InsertDeleteTest` (via `QueryIndex`, which runs `SELECT ... WHERE col1 = k` and needs `EnsureIndexScan`'s plan to contain `IndexScan`) and `AbortIndexTest`; all end with `TableHeapEntryNoMoreThan`.

`txn_index_test.cpp`, `UpdatePrimaryKeyTest`: "`UPDATE maintable SET col1 = col1 + 1`" over keys 1 to 4, then `QueryIndex(..., std::vector<int>{1, 2, 3, 4, 5}, IntResult{{}, {2, 0}, {3, 0}, {4, 0}, {5, 0}})` (an empty row for key 1), then "`UPDATE maintable SET col1 = col1 - 2`" and keys 0 to 5. The writeup suggests handling primary-key updates as delete + insert in two passes.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `index->ScanKey(key, &rids, txn)` | `index.scan_key(&key)` returns a `Vec<Rid>` |
| `index->InsertEntry(key, rid, txn)` returning false on a duplicate | `index.insert_entry(&key, rid)` returns `bool` |
| `GetTupleAndUndoLink` plus `CollectUndoLogs` plus `ReconstructTuple` in each executor | `read_visible_version` (given) for the index scan |

**Port rule:** when two executors need the same visible-version logic, one function serves both.

## Learn more

- [PostgreSQL: unique indexes and concurrent inserts](https://www.postgresql.org/docs/current/index-unique-checks.html) · [`Option::first`/`slice::first`](https://doc.rust-lang.org/std/primitive.slice.html#method.first)
- [PostgreSQL: how UPDATE creates a new row version](https://www.postgresql.org/docs/current/mvcc-intro.html) (a key change there is a new tuple and new index entries) · [Halloween problem](https://en.wikipedia.org/wiki/Halloween_Problem)
