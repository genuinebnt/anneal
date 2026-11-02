`INSERT INTO t VALUES (1, 'a'), (2, 'b')` and `INSERT INTO t SELECT ...` are executed by an `InsertExecutor` whose **child** produces the rows (a `Values` node, or any query) and which itself answers with a **single row holding the number of rows inserted**. It is the first executor that changes things, and two rules come with that. The work happens **once**: calling `next` again after the count was produced must say "nothing more", or a parent that asks twice would insert twice. And a table has **indexes**: every row inserted into the heap must also be added to each index on the table, with the record id the heap gave it, or an index scan would not find the row (the index would lie).

> [!CHECK] A table has an index on `a`. `insert into t values (1, 10), (1, 20)`: what should happen to the second row's index entry, given that the index's keys are unique, and what does BusTub do? Then, in what order do you do the two writes for each row (heap, index), and what is visible to a concurrent reader if you crash between them?
> ||The index refuses the second entry (a B+ tree key is unique); BusTub ignores the refusal and the row is in the table without an index entry (a primary-key constraint, enforced in module 4b, is what turns this into an error). Heap first, then index, because the index entry needs the rid the heap returns; between the two a reader scanning the table sees the row and one using the index does not, which is the window a real engine closes with transactions and a write-ahead log (module 4c).||
>
> - Where does the rid come from?
> - What does the executor return the second time `next` is called?
> - Which tuple do you build the key from, and with which schema?

## The task

In `src/execution/executors/insert_executor.rs` (the struct and `new` are given):

- `insert_into_indexes(tuple, rid)`: for each index in `self.indexes`, build the **key tuple** of the row with `tuple.key_from_tuple(&table_schema, &index.key_schema, index.index.metadata().get_key_attrs())` and add `index.index.insert_entry(&key, rid)`. A refused entry (`false`: the key is already there) is ignored.
- `init`: forget that the count was produced (`done = false`) and initialise the child.
- `next`: if `done`, return `false`. Otherwise pull batches from the child until it is exhausted; store each tuple with `table.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, tuple)?`, call `insert_into_indexes` with the rid you got, and count. Then set `done`, put **one** tuple holding the count (an INTEGER, with the plan's output schema) in the batch and a default rid, and return `true`.

The tests: exact scenarios (`insert values` returns how many; `insert .. select` copies a query; inserting nothing still answers with `0`; the count is produced once; `init` makes the executor run again; every index of the table gets the row; a refused duplicate key does not stop the insert; a type mismatch is an error), and a property: **random inserts** (batches of rows with NULLs, with the index created before or after) leave the table holding exactly the rows inserted so far, each insert answers with its row count, and the index holds exactly one entry for every row.

## Your freedom

How you iterate the child's batches, whether you insert row by row or collect first, and whether you update the indexes row by row or batch by batch.

## The Rust toolbox

**The same batch protocol, as a client.** `while self.child.next(&mut tuples, &mut rids, BUSTUB_BATCH_SIZE)? { for tuple in &tuples { ... } }`: you are the parent now, pulling from `self.child`.

**A flag for "already answered".** `self.done: bool`; `if self.done { return Ok(false); }` first, `self.done = true` just before answering.

**`Result` from the heap.** `table.insert_tuple(&meta, tuple)?` returns `Err` for a tuple too big for any page: pass it up.

**Building the count tuple.** `Tuple::new(&[Value::integer(count)], &self.plan.output_schema)`.

**Indexes as trait objects.** `index.index.insert_entry(&key, rid)` is a call on `Box<dyn Index>` (module 3c): you do not know which index it is.

**Borrowing the table info and the indexes together.** `self.table_info` and `self.indexes` are separate fields, so `for index in &self.indexes { ... self.table_info.schema ... }` borrows two fields of `self` at once without trouble; a helper taking `&self` is fine too.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): `Box<dyn Index>`, calling through a trait object.
- [S1 Option & Result](/t/s1-option-result): `?` through a loop, building a single-row answer.
- [S3 Vec & slices](/t/s3-vec-slices): `clear`, `push`, `extend`.
- The optional *maintaining indexes on writes* concept.
- L8 Error design: Custom errors: execution errors.
- Y5 Testing & verification: Build it: a model of SQL in plain vectors; random sessions of statements.

## Tests

- `insert values` returns how many and the rows appear; `insert .. select` copies a query; inserting nothing returns `0`; the count is produced once and `init` rewinds.
- Every index of the table has an entry for every inserted row; a duplicate key does not stop the insert; wrong types are errors.
- Property: random inserts (with NULLs, with the index before or after) agree with a vector; the index has one entry per row.

## Hints

### Where does the key come from?

The *index* knows which columns form its key (`get_key_attrs`) and its key schema; the *table* knows the row's schema. `key_from_tuple` takes all three (module 3b).

### The order of the two writes

Heap first: the index entry holds the rid, and the rid exists only after the heap insert.

### Only once

If a test says the count is `0` the second time, that is correct; if it says the rows are inserted twice, `done` is not set or not checked.

## Performance

Each inserted row costs a heap insert (a latch, a copy) and one B+ tree insert per index (a descent, a latch crab, possibly a split). With three indexes a row costs about four times a row without. Real engines batch the index inserts or sort them by key first.

**Measure it.** Insert 100 000 rows into a table with 0, 1 and 3 indexes; predict the ratio before you run.

## Experiment

Optional. Predict first, then run.

1. **Skip the index.** Comment out the index insert. Which tests fail first, and which fail only under the property?
2. **Index first.** Insert into the index before the heap, with a fake rid. What does the index scan return?

## Other designs

- **Row at a time, heap then index (ours).**
- **Bulk load:** build the index after the data, sorted: the fast path for `CREATE INDEX` and large imports.
- **Deferred index maintenance** (LSM trees): the index change is appended to a log and merged later.
- **Insert into the index first** with a reserved slot (some engines allocate the rid before writing).

## In BusTub

The executors of Project 3, task 1 (`seq_scan_executor.cpp`, `insert_executor.cpp`, `update_executor.cpp`, `delete_executor.cpp`, `index_scan_executor.cpp`) are stubs (`UNIMPLEMENTED("TODO(P3): Add implementation.")`) with the contract in the header comments. The 2025 BusTub executors are **batched**: `Next(std::vector<Tuple> *tuple_batch, std::vector<RID> *rid_batch, size_t batch_size)` fills the vectors with up to `batch_size` rows and returns whether it produced any.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `table_heap_->InsertTuple(meta, tuple)` returning `std::optional<RID>` | `table.insert_tuple(&meta, &tuple)?` returning a `Rid` or an `Err` |
| `index->index_->InsertEntry(key, rid, txn)` | `index.index.insert_entry(&key, rid)` |
| `bool has_inserted_` | `done: bool` |

**Port rule:** an `optional` result becomes a `Result` (a failed insert carries the reason); a "has run" flag stays a flag.

## Learn more

- PostgreSQL's [`INSERT`](https://www.postgresql.org/docs/current/sql-insert.html) · [index maintenance](https://www.postgresql.org/docs/current/indexes-intro.html)
