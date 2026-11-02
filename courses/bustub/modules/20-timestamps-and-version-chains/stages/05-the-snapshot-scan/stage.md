All the pieces meet in the sequential scan. Inside a transaction (`execute_sql_txn`) a scan does not return what is in the table; it returns what the transaction **can see**: for every tuple, read it together with its undo link, collect the logs the transaction needs, reconstruct the visible version, skip the tuple if there is none or it is a deletion, and only then apply the `WHERE` filter, to the *reconstructed* values (a row's old value may satisfy a filter that its new one does not). Outside a transaction, the module 3e path stays as it is.

> [!CHECK] A transaction begins, then another transaction updates a row and commits. The first transaction scans the table twice, before and after the update. What does each scan return, and why is this called a *snapshot*? Why read the tuple and its undo link under one latch, and what could the scan return if it read the tuple, then the link a moment later?
> ||Both scans return the row as it was when the first transaction began: its read timestamp is fixed and the update's commit timestamp is newer, so the new version is skipped and the old one is rebuilt from the undo log. Reading the tuple and the link separately risks a torn read: another transaction could update the tuple (changing both the table version and the head of the chain) between the two reads, and the scan would pair a new table version with the link that belongs to an older one, rebuilding a version that never existed. One page read latch around both gives a consistent pair.||
>
> - What does the scan do with a tuple that did not exist for this transaction?
> - Which tuple does the filter see?
> - What rid does the output carry?

## The task

In `src/execution/executors/seq_scan_executor.rs`, `SeqScanExecutor::next` begins with a region marked `4a-05`, taken when `self.txn` is `Some((txn, txn_mgr))` (the statement runs inside a transaction: `BusTubInstance::execute_sql_txn`). Fill it in.

For each tuple of the table iterator, until the batch is full or the table ends, the scan returns the version this transaction can see: the tuple, its metadata and its undo link read at one moment (`get_tuple_and_undo_link`: one page read latch), the logs `collect_undo_logs` says are needed, and the tuple rebuilt with `reconstruct_tuple`. A tuple with no visible version, and a version that is deleted, are skipped. The plan's `filter_predicate` is applied to the **reconstructed** tuple (`passes_filter`, from module 3e), and what is returned carries its rid. The iterator advances even for a skipped tuple. `next` returns `true` when the batch is not empty. The non-transactional path below stays as it is for `execute_sql`.

> [!ASIDE] The steps, if you would rather not work them out
> 1. Take its rid and advance the iterator.
> 2. Read the tuple, its metadata and its undo link **together** with `get_tuple_and_undo_link(txn_mgr, self.table_info, rid)` (given: one page read latch, so the tuple and its link belong to the same moment).
> 3. `collect_undo_logs(...)` for the transaction; skip the tuple if it returns `None`.
> 4. `reconstruct_tuple(&self.table_info.schema, ...)`; skip it if that returns `None` (a deleted version).
> 5. Apply the plan's `filter_predicate` to the **reconstructed** tuple (`passes_filter`), then set its rid and push it with its rid.

The tests: exact scenarios (a transaction sees tuples committed before it began; not those committed after; uncommitted tuples are visible only to their writer; an older version is rebuilt from the undo logs; a deleted tuple is gone only for those who see the delete; the filter sees the rebuilt values; each tuple is judged on its own chain), and a property: **several rows, each with its own random history and sometimes an uncommitted version, scanned by a reader at every timestamp** return exactly the rows that exist in that reader's snapshot (and the writer sees its own changes).

## Your freedom

How you structure the loop (a `while` over the iterator with early `continue`s) and whether you build the whole batch before returning.

## The Rust toolbox

**`let Some(..) = .. else { continue };`** skips a tuple in one line, keeping the loop flat.

**One latch for a pair.** `get_tuple_and_undo_link(txn_mgr, table, rid)?` returns `(meta, tuple, link)` read under one latch; use it instead of two calls.

**Setting a rid.** A reconstructed tuple is a new `Tuple`; `tuple.set_rid(rid)` before you return it.

**Two paths in one method.** `if let Some((txn, txn_mgr)) = &self.txn { ... return Ok(..) }` handles the transactional case first; the rest of `next` is the module 3e code.

**Reusing a helper.** `passes_filter(&self.filter_predicate, &self.plan.output_schema, &tuple)?` from module 3e.

## If this is new

- [S1 Option & Result](/t/s1-option-result): `let else`, `Option` chains.
- S6 Iterators: pulling by hand, as in module 3e.
- [L3 Lifetimes](/t/l3-lifetimes): executor fields that borrow the catalog.
- The optional *snapshot isolation* concept.
- Y5 Testing & verification: Build it: a model of versions; random histories and sessions.

## Tests

- Visibility: committed before and after the begin, uncommitted by others and by the writer, rebuilt old versions, deletes, filters on rebuilt values, per-tuple chains.
- Property: random histories, readers at every timestamp.

## Hints

### Filter after rebuilding

A row that satisfied `a = 1` yesterday and has `a = 2` today is returned to a transaction from yesterday if the filter is `a = 1`: the filter sees the *reconstructed* tuple.

### The rid

An index or a later update needs the rid of the row, not of anything else: set it on the reconstructed tuple and return it in the rid batch too.

### Skipped rows still count as scanned

The iterator must advance past a skipped tuple, or the scan loops on it.

## Performance

A transactional scan costs a latch round trip and, for tuples changed since the snapshot, a few log fetches per tuple. If most of a table changed recently and a reader is old, the scan spends most of its time following chains: that is the reason long transactions are expensive in MVCC systems.

**Measure it.** Scan a 10 000-row table with a fresh reader, then after updating every row ten times with an old reader still open.

## Experiment

Optional. Predict first, then run.

1. **Filter first.** Apply the filter to the table's tuple before reconstructing. Which test and which property fail?
2. **Separate reads.** Read the tuple and the link with two calls. Can you make a test that shows a torn read with a second thread?

## Other designs

- **Reconstruct per tuple (ours).**
- **Page-at-a-time visibility checks** (skip pages whose newest timestamp is old enough).
- **Visibility maps** (PostgreSQL) to skip all-visible pages.
- **Snapshots of the whole table** (copy-on-write): readers never reconstruct.

## In BusTub

Project 4 of the 2025 course: `watermark.cpp`, `transaction_manager.cpp` (`Begin`, `Commit`, `Abort`), `execution_common.cpp` (`ReconstructTuple`, `CollectUndoLogs`, `GenerateNewUndoLog`, `GenerateUpdatedUndoLog`) and the transactional path of `seq_scan_executor.cpp`. BusTub's MVCC keeps the newest version in the table and the older ones as **undo logs**, each a delta that turns a version into the previous one, chained from the tuple.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `auto [meta, tuple, link] = GetTupleAndUndoLink(txn_mgr, table_heap, rid)` | `let (meta, tuple, link) = get_tuple_and_undo_link(txn_mgr, table, rid)?;` |
| `if (!logs.has_value()) continue;` | `let Some(logs) = .. else { continue };` |
| `tuple.SetRid(rid)` | `tuple.set_rid(rid)` |

**Port rule:** structured bindings become tuple destructuring; `continue` on an empty optional becomes `let else`.

## Learn more

- BusTub's [Project 4 page](https://15445.courses.cs.cmu.edu/fall2025/project4/) · PostgreSQL's [snapshots](https://www.postgresql.org/docs/current/mvcc-intro.html)
