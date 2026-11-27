Tables with a primary key have an index that must stay consistent with versions. Entries are **never removed** by a delete: a deleted tuple is a tombstone, and the next insert of its key finds the entry and reuses the rid. The index also arbitrates races: two transactions that insert the same new key at once, one of them must fail. And a primary key has no NULL: a NULL key could never be found again, so it is refused before anything is written, as a constraint error, not a conflict.

Every other index follows transactional writes too (stages 1 and 3), and keeps the entries of a row's older keys for the readers that still see them. So an index scan inside a transaction must hand each entry's rid to the same version-reading code as the sequential scan, and then **re-check**: the entry counts only if the version this transaction sees has the entry's key. Without it, a row whose indexed column was updated comes back twice in an `ORDER BY` through the index, once under each key.

Changing a primary key in place would break the rule of Part 1: a rid belongs to one key for life. So an update that changes the key columns is executed as **a delete of the old tuple and an insert of the new one**. The order matters: `UPDATE t SET k = k + 1` over keys 1 to 4 deletes 1, 2, 3, 4 and inserts 2, 3, 4, 5. If each tuple were deleted and inserted in turn, inserting 2 would hit the live tuple with key 2 and fail. Do all the deletes first.

## The task

**Part 1: inserting under a primary key.** In `src/execution/execution_common.rs`, `insert_mvcc`, the region marked `4b-06` (before the `4b-01` fallback), for a table with a primary-key index (`is_primary_key`):

- **A key with a NULL is refused before anything is written**, with the error of `pk.check_primary_key(&key)`; the transaction is not tainted.
- **A key that a live tuple holds is a duplicate**, whoever committed or wrote it: the insert fails with `write_write_conflict(txn)`.
- **A key whose tuple is deleted is reused**: the new row goes into that tuple's rid as a new version (stage 2/3's path), so the heap never holds two tuples for one key.
- **A key nobody has gets a new tuple and an index entry.** If another transaction adds the key between your lookup and your entry, your new tuple is left deleted and the insert fails like a duplicate.
- **A row that went in, new or reused, gets its entries in the other indexes too** (as in stage 1).

In `src/execution/executors/index_scan_executor.rs`, the region marked `4b-06` at the top of `next`: **inside a transaction (`self.txn` is `Some`), every entry the index finds gives the version of its row that the transaction may see** (`read_visible_version`, given); a row with no visible version gives nothing, and **an entry whose key is not the key of that version gives nothing**, so each row comes back once, under the key the transaction sees; the filter predicate applies as before.

**Part 2: updating the key.** In `src/execution/executors/update_executor.rs`, `update_by_delete_and_insert(txn, txn_mgr, changes)` (region `4b-06`; `changes` is the list of `(rid, new tuple)` from stage 3, and `changes_primary_key` already decided this path is needed):

- **A new key with a NULL fails before any change is made.**
- **The statement behaves as if every old row were deleted first and every new row inserted after**: `UPDATE t SET k = k + 1` over keys 1 to 4 succeeds, a new key that equals a key just deleted reuses that tombstone's rid, a key nobody had gets a new tuple and entry, and a new key that a live row not in `changes` holds is a duplicate.

> [!ASIDE] The steps, if you would rather not work them out
> **Part 1.** In `src/execution/execution_common.rs`, `insert_mvcc`, the region marked `4b-06` (before the `4b-01` fallback): if `indexes` contains a primary-key index (`is_primary_key`):
>
> - the key is `tuple.key_from_tuple(&table.schema, &pk.key_schema, pk.index.metadata().get_key_attrs())`; a key with a NULL fails with the error of `pk.check_primary_key(&key)` before anything is written (the transaction is not tainted);
> - `pk.index.scan_key(&key)`: if an entry exists, look at the tuple at its rid (`get_tuple_and_undo_link`). **Live** (`!is_deleted`): duplicate key, fail with `write_write_conflict(txn)`. **Deleted**: reuse the rid, `modify_tuple(txn, txn_mgr, table, rid, Some(tuple))` (stage 2/3's path);
> - no entry: insert the tuple (temporary timestamp), add it to the write set, then `insert_entry(&key, rid)`. If that returns `false` another transaction got the key between your lookup and now: mark your new tuple deleted (`update_tuple_meta`) and fail the same way.
> - a row that went in, new or reused, gets its entries in every index: `insert_index_entries(table, indexes, tuple, rid)`.
>
> In `src/execution/executors/index_scan_executor.rs`, the region marked `4b-06` at the top of `next`: when `self.txn` is `Some`, for every entry `(key, rid)` use `read_visible_version(txn, txn_mgr, table, rid)` (given: tuple + link under one latch, `collect_undo_logs`, `reconstruct_tuple`) instead of reading the heap directly; skip `None`; skip the entry if the version's key (`key_from_tuple` with the index's key schema and key attributes) is not `key`; apply the filter predicate as before.
>
> **Part 2.** In `src/execution/executors/update_executor.rs`, `update_by_delete_and_insert(txn, txn_mgr, changes)` (region `4b-06`; `changes` is the list of `(rid, new tuple)` from stage 3, and `changes_primary_key` already decided this path is needed):
>
> 1. a new key with a NULL fails (`check_primary_key`) before any change is made;
> 2. for every change, `modify_tuple(txn, txn_mgr, table, rid, None)`: delete the old tuple (its index entry stays);
> 3. for every change, `insert_mvcc(txn, txn_mgr, table, &self.indexes, new)`: insert the new tuple. A new key that equals a key just deleted finds the tombstone and reuses its rid; a key nobody had gets a new tuple and entry.

The tests: exact scenarios (a duplicate primary key fails and taints; a key committed by somebody else is a duplicate too; a deleted key is reused without a second tuple; index scans see the version of their snapshot; aborting an insert keeps the entry and the next insert reuses it; delete then insert in one transaction reuses the tuple; updating the key moves the row; shifting every key reuses the tombstones it just made; older snapshots still see the old keys; a non-key update stays in place; updating a key onto a live key fails; a NULL primary key is refused before anything is written; a lookup on another index inside a transaction finds the rows written in transactions; an update of an indexed column leaves every snapshot's lookups and `ORDER BY` right, repeated and NULL keys included), and a property: **transactions one after another on a table with a primary key and an index on its value**, with inserts, deletes, value updates and key moves, committed or aborted, against a `BTreeMap`: every duplicate fails and taints, every other statement succeeds, the table, every point lookup by key or by value and the order by value equal the map after each transaction, every earlier snapshot still reads its state through both indexes, and the heap never holds more tuples than there are different keys.

## Your freedom

How you detect a live key (through the index and the heap), how you order the two phases of a key update (the intended design deletes first, then inserts), and how the index scan compares a version's key with an entry's (key tuples of equal values have equal bytes).

## The Rust toolbox

**Look up, then decide.** `pk.index.scan_key(&key)` gives the rid of a tuple with this key, live or dead; the heap's metadata tells which.

**Reuse a rid.** A tombstone is turned back into a live tuple with `modify_tuple(.., Some(new))`, leaving an undo log that says 'it was deleted'.

**The index arbitrates races.** `insert_entry` returns `false` if another transaction added the key first: undo your new tuple (mark it deleted) and fail like a duplicate.

**Two phases.** Collect the changes, delete all old keys, then insert all new ones: `UPDATE t SET k = k + 1` over keys 1 to 4 only works that way.

**Re-check through bytes.** `tuple.key_from_tuple(&schema, &index.key_schema, attrs).data() == key.data()` compares the version's key with the entry's: key tuples of the same values are built the same way.

**The calls you need, by shape.** `Tuple::key_from_tuple(&self, schema, key_schema, key_attrs) -> Tuple` projects a row onto an index's key; `Index::scan_key(&key) -> Vec<Rid>` lists the rids under a key; `Index::insert_entry(&key, rid) -> bool` is `false` if the key is already there; `read_visible_version(..) -> Result<Option<Tuple>>` is the version a transaction may see, if any.

## Design notes

**In the primary key, a rid belongs to one key for life.** Entries are never removed and a key's rid is reused, so the tuple at a rid always has (or had) that key, and the re-check never rejects a primary-key entry. In any other index a row may have had several keys, one entry each, and the re-check is what picks the one this reader's version has.

**Why look before inserting, and still handle the failure.** The lookup tells you whether to reuse; it cannot prevent another transaction from inserting the same new key right after. `insert_entry` is atomic and returns `false` for the loser, which is the real decision. The loser's heap tuple is a harmless buried tuple (deleted, temp timestamp, in the write set so commit or abort cleans up).

**Tombstone reuse is an ordinary in-place change.** `modify_tuple` with a deleted base creates a log with `is_deleted = true` and the tombstone's timestamp (readers between the delete and now see "no tuple"), links it in front of the older chain (readers before the delete see the old row), and checks for conflicts: a tombstone whose delete was committed after your snapshot, or deleted by someone still uncommitted, is a conflict, not a reuse.

**Every index, and entries that outlive their version.** Transactional writes add entries to every index and remove none: a row updated from `b = 2` to `b = 5` has the entries `(2, rid)` and `(5, rid)`, and an abort or a later update adds no cleanup. A lookup of 2 by a reader that began after the update reads the row, finds `b = 5` and drops it; a reader that began before finds `b = 2` and keeps it. Garbage collection does not remove the entries no version can reach any more (stage 5), so they cost a heap read per lookup until then. This is how PostgreSQL's indexes behave between vacuums.

**Why two loops.** Everything about the order follows from Part 1's rule: an insert onto a key whose tuple is *live* is a duplicate, and onto a key whose tuple is a tombstone is a reuse. The first loop turns all old keys into tombstones (owned by this transaction, with logs), so the second loop's inserts find tombstones for the keys that are being vacated and shifted into.

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
- A NULL primary key is refused without a conflict; another index finds rows written in transactions, and an update of its column leaves every snapshot right.
- Property: a primary-key table with an index on its value against a `BTreeMap` with commits and aborts, and the tuple-count bound.

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

### A row twice in an ORDER BY

If `ORDER BY b` through an index returns a row twice after `b` was updated, the scan used both of the row's entries. The version you read has one `b`; keep the entry with that key and drop the other.

### Read the old key from the table, not from the new tuple

`changes_primary_key` needs both keys. The old one is the table's tuple at the rid (the child's visible tuple equals it when there is no conflict).

## Performance

A transactional insert into a table with a primary key costs one index lookup (a B+ tree descent) and either one tuple append plus one index insert, or one in-place change of a tombstone, plus one insert per other index. Reuse keeps both table and index from growing under delete/insert churn, which would otherwise make every scan walk dead tuples. An index scan costs one lookup per key plus one version read per entry, and the entries of keys a row no longer has cost a version read each before they are dropped.

**Measure it.** Delete and re-insert the same 1 000 keys 100 times and watch the heap's tuple count stay at 1 000.

## Experiment

Optional. Predict first, then run.

1. **Delete entries.** Remove the index entry when a tuple is deleted. Which test fails, and what does an older snapshot's index scan lose?
2. **Insert instead of reuse.** Always insert a new tuple. Which property fails (the bound)?
3. **Skip the re-check.** Drop the key comparison in the index scan. Write the shortest session that returns a wrong answer, predict it, then run it: which query, which snapshot?

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
| an index condition re-checked against the heap tuple (PostgreSQL's `recheck`) | the version's key compared with the entry's key |

**Port rule:** when two executors need the same visible-version logic, one function serves both.

## Learn more

- [PostgreSQL: unique indexes and concurrent inserts](https://www.postgresql.org/docs/current/index-unique-checks.html) · [`Option::first`/`slice::first`](https://doc.rust-lang.org/std/primitive.slice.html#method.first)
- [PostgreSQL: how UPDATE creates a new row version](https://www.postgresql.org/docs/current/mvcc-intro.html) (a key change there is a new tuple and new index entries) · [Halloween problem](https://en.wikipedia.org/wiki/Halloween_Problem)
