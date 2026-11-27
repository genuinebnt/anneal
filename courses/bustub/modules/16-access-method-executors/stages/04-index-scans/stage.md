An index does not hold rows; it holds **entries** `(key, rid)` in key order, one per row (module 3c). An **index scan** asks the index for entries, fetches each row from the heap, and answers like any other scan. It has two jobs. With no keys it scans the whole index: the rows come back **in key order**, rows of one key in rid order and rows with a NULL key first, which is how `ORDER BY a` can be answered without a sort. With a list of keys (a **point lookup**, `WHERE a = 5` or `a = 5 OR a = 9`) it asks the index for each key in turn and fetches **every row of that key**, which is why an indexed lookup in a million-row table is a handful of page reads.

> [!CHECK] A row is deleted, its heap tuple is flagged, and (a bug in some earlier stage) its index entry survives. What does an index scan return, and what should it return? Separately: `where a = 5 and b > 3` uses the index for `a = 5`. Where is `b > 3` applied, and why must it still be applied to rows the index found?
> ||The scan must consult the heap tuple's metadata and skip a deleted row (the index is a hint about where rows are, not the truth about which exist). `b > 3` is applied to each fetched row by the scan's own filter: an index only knows about its key, so every other condition is a filter on the rows it finds.||
>
> - In which order do rows come back with no keys, and with keys?
> - What does `scan_key` return for a key that is not there, and for a key three rows share?
> - Where do you evaluate the key expressions, and against which tuple?

## The task

In `src/execution/executors/index_scan_executor.rs` (the struct and `new` are given):

- **`init` fixes the entries to visit.** With no `pred_keys`, every entry of the index, in order. With `pred_keys` (constant expressions), the entries of each key (every row the key finds, in rid order), keys in the order given; a key given twice is looked up **once**: `v1 = 4 or v1 = 4` is one row per match, not two.
- **`next` returns those rows in that order**, each with its rid, at most `batch_size` per call. A row that is deleted, or that the plan's `filter_predicate` (when there is one) does not make TRUE, does not come back. It returns `true` exactly when the batch is not empty.

> [!ASIDE] The steps, if you would rather not work them out
> - `collect_entries()` (called by `init`): with **no** `pred_keys`, every entry of the index (`index.scan_all()`); with `pred_keys`, for each key expression **in order**: evaluate it (it is a constant: use an empty tuple and an empty schema), make a one-column **key tuple** with the index's key schema (`Tuple::new(&[value], &self.index_info.key_schema)`), skip it if an earlier key was the same, look it up with `index.scan_key(&key)` and append `(key, rid)` for every rid found.
> - `init`: remember the entries to visit and start at the first.
> - `next`: fill the batch from the entries' rids: fetch each row with `table.get_tuple(rid)?`, skip it if it is deleted or if the plan's `filter_predicate` (when there is one) is not TRUE, push the tuple and its rid; stop at `batch_size` or the end. Return `true` if the batch is not empty.

The tests: exact scenarios (rows come back in key order, not storage order; negative keys sort as numbers; the batch size is respected; an empty table gives nothing; deleted rows do not come back; `order by` on an indexed column becomes an index scan, with NULL keys first and repeated keys in rid order; a key finds its row; a key finds every row that has it; several keys in the order given; a key listed twice finds its row once; a deleted row is not found; the filter applies to what the index found; a lookup sees an update from the same session; a batch of lookups larger than the batch size), and a property: **for random keys with repeats**, a full index scan returns the rows by key and then in insertion (rid) order, a filtered one returns the rows passing the filter, and a list of probes returns every row of each present key, keys in the order given (absent keys find nothing, a probe given twice finds its rows once).

## Your freedom

Collecting all entries at `init` (simple, holds a list in memory) or asking the index lazily as `next` goes; how you evaluate keys.

## The Rust toolbox

**Evaluating a constant.** `expr.evaluate(&Tuple::empty(), &Schema::new(vec![]))?`: the key expressions are constants, so no row is needed.

**Building a key tuple.** `Tuple::new(&[value], &self.index_info.key_schema)`: the same kind of tuple the index stores.

**`extend`.** `entries.extend(index.scan_key(&key).into_iter().map(|rid| (key.clone(), rid)))` appends the entries found.

**A cursor into a `Vec`.** `self.entries[self.cursor..].iter()` and `self.cursor += 1`; or `std::mem::take`/`drain` if you prefer to consume.

**Skipping with `continue`.** `let (meta, tuple) = table.get_tuple(rid)?; if meta.is_deleted { continue; }` keeps the loop flat.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): calling `scan_all` / `scan_key` on a `Box<dyn Index>`.
- [S1 Option & Result](/t/s1-option-result): `?` for the heap fetch.
- S6 Iterators: `extend`, slicing a `Vec` as a cursor.
- Y5 Testing & verification: Build it: a model of SQL in plain vectors; random sessions of statements.

## Tests

- A full index scan returns rows in key order (not storage order), negative keys first; batches respect the size; an empty table gives nothing; deleted rows do not come back; `order by` through the index puts NULL keys first.
- A key finds every row that has it; several keys in the order given; a missing key finds nothing; a deleted row is not found; the filter applies after the index; updates are seen.
- Property: random keys with repeats against sorted vectors, filters and probes.

## Hints

### No keys means everything

An empty `pred_keys` is a full scan, not an empty one. The property test skips empty probe lists for this reason.

### Do not trust the index about aliveness

The heap tuple's `is_deleted` flag is the truth. (Module 4's MVCC makes it a question of *which version*, which is why `get_tuple` returns the metadata.)

## Performance

A point lookup is one tree descent (`depth + 1` page latches) plus one heap page read: microseconds. A full index scan is a leaf-chain walk plus one heap read per row in *key* order, which for a table inserted in random order is a random read per row: slower than a sequential scan for large results, which is why the optimizer chooses an index only when a query is selective or needs the order.

**Measure it.** Count rows per second for a full index scan and a sequential scan of the same table when the keys were inserted in random order and in key order.

## Experiment

Optional. Predict first, then run.

1. **Lazy.** Ask the index for one key at a time inside `next` instead of at `init`. What changes if a row is inserted between two `next` calls?
2. **Duplicates.** Remove your de-duplication of repeated keys. Which test fails, and what query would show the bug to a user?
3. **Many rows per key.** Fill a table with a million rows over ten values of `a` and time `where a = 3` through the index and through a sequential scan. At what share of the table does the index stop paying?

## Other designs

- **Collect rids at `init` (ours).**
- **Lazy leaf-chain walk** with the iterator of module 2c: constant memory, sees concurrent changes.
- **Covering index scans:** the index holds all needed columns; the heap is never read.
- **Bitmap scans** (PostgreSQL): collect rids, sort them by page, read the heap sequentially.

## In BusTub

The executors of Project 3, task 1 (`seq_scan_executor.cpp`, `insert_executor.cpp`, `update_executor.cpp`, `delete_executor.cpp`, `index_scan_executor.cpp`) are stubs (`UNIMPLEMENTED("TODO(P3): Add implementation.")`) with the contract in the header comments. The 2025 BusTub executors are **batched**: `Next(std::vector<Tuple> *tuple_batch, std::vector<RID> *rid_batch, size_t batch_size)` fills the vectors with up to `batch_size` rows and returns whether it produced any.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::vector<RID> rids_;` and a cursor index | `Vec<(Tuple, Rid)>` of entries and `cursor: usize` |
| `index_->ScanKey(key, &result, txn)` | `index.scan_key(&key)` returning `Vec<Rid>` |
| `pred_key->Evaluate(nullptr, empty_schema)` | `expr.evaluate(&Tuple::empty(), &Schema::new(vec![]))?` |

**Port rule:** an out-parameter result vector becomes a returned `Vec`.

## Learn more

- PostgreSQL's [index scans and bitmap scans](https://www.postgresql.org/docs/current/indexes-bitmap-scans.html) · module 2c's iterator
