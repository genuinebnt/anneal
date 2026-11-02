`DELETE FROM t WHERE ...` and `UPDATE t SET b = b + 1 WHERE ...` have a **child** (a scan with the filter) that produces the rows to change together with their record ids, and each answers with one row: how many it changed. A delete does not erase bytes: it flags the tuple's metadata `is_deleted` (module 3b's table page, 3c's heap), so slots never move and record ids stay valid. An update in this engine is a **delete of the old tuple plus an insert of a new one**: simple, and it makes the index maintenance a delete of the old key and an insert of the new one under the new rid.

> [!CHECK] `update t set a = b, b = a`: what does every row end up as, and what does that tell you about which row the new values are computed from? Then: the update's child scans the table while the update inserts new tuples into the same table. Why does the scan not see the new tuples and update them again, forever?
> ||Every row swaps its two columns: all target expressions are evaluated on the **old** row, then the new tuple is built (otherwise `a = b` would change `a` before `b = a` read it). The loop does not run forever because the child's scan was started with the table iterator of module 3c that stops where the table ended when the scan began (the Halloween problem): the newly inserted versions lie beyond that point.||
>
> - Which rid does the new tuple get, and where does the old one's slot go?
> - What do you do to the index entries of the old tuple, and in what order do you add the new ones?
> - What does `delete from t` with no `WHERE` return on an empty table?

## The task

In `src/execution/executors/delete_executor.rs` (the struct and `new` are given):

- `delete_from_indexes(tuple)`: for each index of the table, build the key from the **deleted tuple's values** (`key_from_tuple`, as in stage 2) and `delete_entry(&key)`.
- `init`: forget that the count was produced and initialise the child.
- `next`: once, for every `(tuple, rid)` the child produces: mark the heap tuple deleted with `table.update_tuple_meta(&TupleMeta { ts: 0, is_deleted: true }, rid)?`, remove its index entries, count it; then answer with one tuple holding the count; every later call returns `false`.

In `src/execution/executors/update_executor.rs` (the struct and `new` are given):

- `make_new_tuple(old)`: evaluate each of `self.target_expressions` on the old tuple with the child's output schema, and build a tuple of those values with the **table's** schema.
- `init`, and `next` as for delete, but for each `(old, old_rid)`: build the new tuple; flag the old heap tuple deleted and remove its entries from every index; insert the new tuple and add its key to every index under the **new rid**; count.

The tests: exact scenarios (delete returns how many and the rows disappear; deleting nothing returns `0`; delete with no `WHERE`; the indexes lose the entries and a key can be inserted again; update returns how many and changes them; `set b = b + a` uses the old values; `set a = b, b = a` swaps; every row is updated exactly once even in a table of thousands; the indexes follow an update, including a change of the key), and a property: **random sessions of deletes and updates** (set `b` or `c` to a sum of columns, or shift the key `a`) agree with a vector of rows after every statement, each statement answers with the number of rows it touched, and the index of `a` has exactly one entry for every live row and no entry for a dead one.

## Your freedom

Whether delete and update share code, whether the update computes all new tuples before changing anything or one at a time, and the order of the heap and index writes.

## The Rust toolbox

**Evaluate on the old row.** `self.target_expressions.iter().map(|e| e.evaluate(old, child_schema)).collect::<Result<Vec<Value>>>()?`: collecting an iterator of `Result` into a `Result<Vec>` stops at the first error.

**Build a tuple of the table's schema.** `Tuple::new(&values, &self.table_info.schema)`; a value of the wrong type is a bug in the planner and panics, not an `Err`.

**Marking, not erasing.** `TupleMeta { ts: 0, is_deleted: true }` is `Copy`; `update_tuple_meta` writes it back under the page's write latch.

**Reusing `insert_into_indexes`-style code.** A small private function `fn key_of(&self, index: &IndexInfo, tuple: &Tuple) -> Tuple` removes the three-line repetition.

**Counting once.** The same `done` flag as insert, set just before you answer.

## If this is new

- [S1 Option & Result](/t/s1-option-result): collecting `Result`s, `?` in a closure.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): calling `Expression::evaluate` through `Arc<dyn Expression>`.
- [S3 Vec & slices](/t/s3-vec-slices): `iter().map(..).collect()`.
- The optional *Halloween problem* and *maintaining indexes on writes* concepts.
- L6 Closures & functional Rust: Fn / FnMut / FnOnce: predicates and target expressions as callable trees.
- L8 Error design: Custom errors: execution errors.
- Y5 Testing & verification: Build it: a model of SQL in plain vectors; random sessions of statements.

## Tests

- Delete returns how many and the rows disappear; deleting nothing returns zero; a delete with no `WHERE` clears the table; the indexes lose their entries; a deleted key can be inserted again.
- Update returns how many and changes them; new values use the old row; a swap works; every row is updated exactly once; the indexes follow, including when the key changes.
- Property: random deletes and updates agree with a vector of rows and keep the index exact.

## Hints

### The swap

If `update t set a = b, b = a` leaves both columns equal, you are building the new tuple column by column from a tuple you have already changed. Evaluate every target on the *unchanged* old tuple.

### The index of a changed key

Delete the **old** key's entry and insert the **new** key under the **new** rid. An update that changes only `b` has the same key: it still gets a new rid, so the index entry must be replaced even though the key is the same.

### An index with a stale rid

If a lookup returns a deleted row, an entry was not removed; if it returns the *old* values, an entry still points at the old rid.

## Performance

An update costs a delete (a latch, a flag), an insert (a latch, a copy) and per index two tree operations. Updating a non-indexed column still replaces every index entry because the rid changes, which is why real engines try to update in place (`update_tuple_in_place`, module 3b) when the new tuple has the same size and no index key changes.

**Measure it.** Update one column of a million-row table with one index, with delete-and-insert and then in place; compare.

## Experiment

Optional. Predict first, then run.

1. **Update in place.** For updates that change no indexed column and keep the size, call `update_tuple_in_place`. What breaks about the Halloween protection?
2. **A forgotten index.** Skip the delete from the indexes in `update` only. Which tests catch it, and which sequence of statements does the property need before it fails?

## Other designs

- **Delete-and-insert (ours, BusTub's).**
- **Update in place** with an undo log (module 4a): the heap keeps the newest version, the log the older ones.
- **Append-only versions** (PostgreSQL): the old version stays and is marked; vacuum removes it later.
- **Index-organised tables:** the row lives in the index; there is no rid to change.

## In BusTub

The executors of Project 3, task 1 (`seq_scan_executor.cpp`, `insert_executor.cpp`, `update_executor.cpp`, `delete_executor.cpp`, `index_scan_executor.cpp`) are stubs (`UNIMPLEMENTED("TODO(P3): Add implementation.")`) with the contract in the header comments. The 2025 BusTub executors are **batched**: `Next(std::vector<Tuple> *tuple_batch, std::vector<RID> *rid_batch, size_t batch_size)` fills the vectors with up to `batch_size` rows and returns whether it produced any.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `table_info->table_->UpdateTupleMeta({0, true}, rid)` | `table.update_tuple_meta(&TupleMeta { ts: 0, is_deleted: true }, rid)?` |
| `std::vector<Value> values; for (auto &expr : target_expressions_) values.push_back(expr->Evaluate(...))` | `.iter().map(|e| e.evaluate(..)).collect::<Result<Vec<_>>>()?` |
| `index->index_->DeleteEntry(key, rid, txn)` | `index.index.delete_entry(&key)` |

**Port rule:** a loop that pushes into a vector and may throw becomes an iterator chain collected into a `Result`.

## Learn more

- PostgreSQL's [MVCC and updates](https://www.postgresql.org/docs/current/mvcc-intro.html) · [the Halloween problem](https://en.wikipedia.org/wiki/Halloween_Problem)
