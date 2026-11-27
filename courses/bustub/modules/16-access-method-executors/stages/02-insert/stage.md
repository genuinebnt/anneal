`INSERT INTO t VALUES (1, 'a'), (2, 'b')` and `INSERT INTO t SELECT ...` are executed by an `InsertExecutor` whose **child** produces the rows (a `Values` node, or any query) and which itself answers with a **single row holding the number of rows inserted**. It is the first executor that changes things, and two rules come with that. The work happens **once**: calling `next` again after the count was produced must say "nothing more", or a parent that asks twice would insert twice. And a table has **indexes**: every row inserted into the heap must also be added to each index on the table, with the record id the heap gave it, or an index scan would not find the row (the index would lie). Most indexes take any row (module 3c: keys may repeat); a **primary key** is unique, so some rows cannot go in at all.

> [!CHECK] A table has an index on `a` and a primary key on `id`. `insert into t values (1, 10), (2, 10)` (columns `id, a`): how many entries does the key 10 get in the index on `a`, and what tells them apart? Then `insert into t values (1, 30)`: what must happen, and why is "insert the row, and see whether the primary key's `insert_entry` refuses it" not enough? Finally, in what order do you do the two writes for each row (heap, index)?
> ||Two: the entries `(10, rid of the first row)` and `(10, rid of the second)`, told apart by the rid. The third row repeats the primary key 1, so the statement must fail at that row with `duplicate key value violates unique constraint`, and the row must be in neither the heap nor any index. The primary key's refusal comes only after the heap insert (an entry needs the row's rid), so a row refused that late is already in the heap, and maybe in other indexes, without its primary-key entry: the failure itself breaks the invariant unless the row is undone. Checking the primary key before writing avoids that. Heap first, then the indexes, because every entry needs the rid the heap returns; between the two a reader scanning the table sees the row and one using an index does not, which is the window a real engine closes with transactions and a write-ahead log (module 4c).||
>
> - Where does the rid come from?
> - What does the executor return the second time `next` is called?
> - Which tuple do you build the key from, and with which schema?

## The task

In `src/execution/executors/insert_executor.rs` (the struct and `new` are given), the executor behind `INSERT`:

- `next` answers **once** with one tuple holding the number of rows inserted (an INTEGER, in the plan's output schema, with a default rid), and `false` after that; `init` makes it run again.
- Every row of the child is stored in the heap (`TupleMeta { ts: 0, is_deleted: false }`) and gets its entry `(key, rid)` in **every index** of the table, under the rid the heap gave it. The key of a row in an index is `tuple.key_from_tuple(&table_schema, &index.key_schema, key_attrs)`. An index that is not a primary key takes any key: repeated, or with a NULL in it.
- **A primary key is unique and has no NULL.** A row whose primary key a live row already has fails the statement with `unique_violation(index name)` (in `index.rs`); a row whose primary key has a NULL fails with the error of `IndexInfo::check_primary_key`. The failing row is in neither the heap nor any index; the rows before it stay.
- The invariant after every statement, failed or not: **every live row has exactly one entry in each index of its table**, and every entry leads to a live row with that key.

`check_constraints` and `insert_into_indexes` are a suggested split; the tests use only SQL and the indexes' public methods.

The tests: exact scenarios (`insert values` returns how many; `insert .. select` copies a query; inserting nothing still answers with `0`; the count is produced once; `init` makes the executor run again; every index of the table gets the row; rows that share a key or have a NULL key each get an entry; a taken primary key fails at that row and leaves the rows before it; a NULL primary key fails; a type mismatch is an error), and a property: **random inserts** (repeated and NULL keys, with the index created before or after) leave the table holding exactly the rows a vector holds and the index one entry per row, in key order; with a primary key instead, the inserts stop at the first taken or NULL key.

## Your freedom

How you iterate the child's batches, whether you insert row by row or collect first, whether you check a row's primary key before writing it or write and undo, and whether you update the indexes row by row or batch by batch.

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
- Every index of the table has an entry for every inserted row, repeated and NULL keys included; a taken or NULL primary key fails at that row; wrong types are errors.
- Property: random inserts (repeated and NULL keys, the index before or after, or a primary key) agree with a vector; every index has one entry per row.

## Hints

### Where does the key come from?

The *index* knows which columns form its key (`get_key_attrs`) and its key schema; the *table* knows the row's schema. `key_from_tuple` takes all three (module 3b).

### The order of the two writes

Heap first: the index entry holds the rid, and the rid exists only after the heap insert.

### A refused row must leave nothing behind

The heap insert comes before the index insert, so a row whose primary key is found taken only when `insert_entry` returns `false` is already in the heap, and perhaps in the other indexes, with no primary-key entry: the failure itself breaks the invariant. Either decide that a row can go in before writing anything (the primary key's `scan_key` of its key), or undo what you wrote (mark it deleted, remove the entries already added).

### Only once

If a test says the count is `0` the second time, that is correct; if it says the rows are inserted twice, `done` is not set or not checked.

## Performance

Each inserted row costs a heap insert (a latch, a copy) and one B+ tree insert per index (a descent, a latch crab, possibly a split); checking the primary key first adds one lookup, a second descent to the same leaf, which a real engine folds into the insert itself. With three indexes a row costs about four times a row without. Real engines batch the index inserts or sort them by key first.

**Measure it.** Insert 100 000 rows into a table with 0, 1 and 3 indexes; predict the ratio before you run.

## Experiment

Optional. Predict first, then run.

1. **Skip the index.** Comment out the index insert. Which tests fail first, and which fail only under the property?
2. **Index first.** Insert into the index before the heap, with a fake rid. What does the index scan return?
3. **Write, then undo.** Drop the primary-key check and undo a row whose primary-key entry is refused. Count the heap tuples after 1 000 refused inserts: what did the undo leave behind?

## Other designs

- **Row at a time, heap then index (ours).**
- **Uniqueness as an index property (ours):** a unique index is a tree that refuses a key it holds; PostgreSQL does the same for `UNIQUE` and primary keys, checking the heap for a live row before raising the error.
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
