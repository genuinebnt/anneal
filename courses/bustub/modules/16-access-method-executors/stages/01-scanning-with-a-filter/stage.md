The plan the planner builds is a tree of operators; the **executors** run it. Each executor has the same two-step protocol: `init()` once, then `next(&mut tuples, &mut rids, batch_size)` again and again; each call replaces the two vectors with up to `batch_size` rows (and their record ids) and returns `true`, until it returns `false` for "nothing more". Parents pull from children, so a query is a chain of `next` calls. The leaf of almost every plan is the **sequential scan**: read the whole table in storage order and keep the rows that satisfy the `WHERE` filter.

> [!CHECK] A scan with `WHERE price > 10` meets a row whose `price` is NULL. Is it returned? And a row whose `price` is 5? Both evaluate the predicate to something that is not TRUE: what does a scan do with each, and why do `WHERE x` and `WHERE NOT x` not partition the table? Separately: why does the scan return deleted tuples' *slots* from the heap iterator but not their rows?
> ||Neither is returned: only a predicate that evaluates to TRUE keeps a row; FALSE and unknown (NULL) are dropped, which is why a row with a NULL `x` appears in neither `WHERE x` nor `WHERE NOT x`. The heap iterator (module 3c) returns *every* slot with its metadata, deleted ones included, because layers above (transactions, MVCC) need them; this scan is a layer above, and a deleted tuple is not part of the table any more, so it skips it.||
>
> - What does `evaluate` return for a NULL operand?
> - Where does the `batch_size` stop: after how many *scanned* rows or *kept* rows?
> - Who owns the iterator between `next` calls?

## The task

In `src/execution/executors/seq_scan_executor.rs` (the struct, `new` and the plan are given):

- `passes_filter(filter, schema, tuple) -> Result<bool>`: no predicate keeps everything; a predicate is evaluated on the tuple with the scan's output schema and keeps the row only if the answer is the BOOLEAN **true**. An error from `evaluate` is passed on.
- `init`: start a table iterator over the table with `make_iterator()` (module 3c's: the one that stops where the table ended when the scan began) and keep it.
- `next`: replace the batch with up to `batch_size` rows: for each tuple the iterator gives, skip it if its metadata says `is_deleted`, keep it if `passes_filter` says so; push the tuple and its rid. Return `true` if the batch is not empty.

The tests: exact scenarios (an empty table has no batches; every row comes back in storage order across pages; the batch size is respected; deleted rows are skipped; comparisons, `AND`, NULL is not TRUE), and two properties: **for random rows with NULLs and random predicates** (comparisons of sums and differences of columns, combined with `and`/`or`) `select * from t where p` returns exactly the rows a Rust model of three-valued logic keeps, in storage order; and **the batch size changes how many rows come back per call, never which**.

## Your freedom

How you hold the iterator (an `Option` set by `init`, built lazily in `next`), whether you copy rows into the batch one at a time or build a batch and swap it in, and whether you check the batch size before or after reading a row.

## The Rust toolbox

**An `Option` for "not started".** `self.iter: Option<TableIterator<'e>>`: `init` sets `Some(table.make_iterator())`; `next` takes `self.iter.as_mut().expect("init was not called")`.

**The iterator is an `Iterator`.** `for (meta, tuple) in iter` would consume it; here you want to stop and resume, so call `iter.next()` by hand inside a `while` loop: `while batch.len() < batch_size { let Some((meta, tuple)) = iter.next() else { break }; ... }`.

**Clearing and filling vectors.** `tuple_batch.clear(); rid_batch.clear();` at the start of `next`; the callers pass the same vectors again and again.

**`?` inside the loop.** `if passes_filter(&self.filter, schema, &tuple)? { ... }` returns an `Err` from `next` for a predicate that fails to evaluate.

**The rid of a tuple.** The heap's iterator yields tuples that know their rid: `tuple.get_rid()`.

**`as_bool`.** `value.as_bool() == Some(true)` is "TRUE, not NULL, not FALSE" in one comparison.

## If this is new

- S6 Iterators: pulling from an iterator by hand, `let else`.
- [L3 Lifetimes](/t/l3-lifetimes): `TableIterator<'e>` borrows the heap for as long as the executor lives.
- [S1 Option & Result](/t/s1-option-result): `Option<Iterator>`, `?` in a loop.
- The optional *the iterator model* and *three-valued logic* concepts.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): Static vs dynamic: `Box<dyn Executor>` children, `Box<dyn Index>` calls.
- Y5 Testing & verification: Build it: a model of SQL in plain vectors; random sessions of statements.

## Tests

- An empty table has no batches; a scan returns every row in storage order across many pages; batches have at most `batch_size` rows; the last call leaves the vectors empty.
- Deleted rows are skipped; the predicate keeps TRUE and drops FALSE and NULL; `AND`/`OR` of comparisons; a column compared with a column.
- Property: predicates over random rows with NULLs agree with a three-valued model; the batch size never changes the rows.

## Hints

### Count kept rows, not scanned ones

A batch is full when it holds `batch_size` *kept* rows; scanned rows that were filtered out do not count. A scan with a selective filter reads many rows per batch.

### What if the table grows during the scan?

The iterator you start in `init` stops where the table ended at that moment. That is the Halloween protection of module 3c: `update` (stage 3) scans the table it is inserting new versions into.

### The last call

`next` returns `false` when the batch is empty. It must also leave the vectors empty: the callers rely on it.

## Performance

A scan is dominated by page reads and tuple decoding; the batch size amortises the cost of the virtual call to the child (one call per batch instead of per row) and lets the next operator work on a cache-friendly array. BusTub's `BUSTUB_BATCH_SIZE` is 128.

**Measure it.** Scan a million-row table with batch sizes 1, 16, 128 and 1024 and plot rows per second: where does the curve flatten?

## Experiment

Optional. Predict first, then run.

1. **Keep NULLs.** Change `== Some(true)` to `!= Some(false)`. Which tests fail, and what do you learn about `WHERE`?
2. **A live iterator.** Use `make_eager_iterator()` and `insert into t select * from t`: does it terminate?

## Other designs

- **Batched pull (ours, BusTub 2025):** `next` fills a batch.
- **Tuple at a time** (the classic Volcano model): simplest, one virtual call per row.
- **Push-based pipelines** (HyPer, DuckDB): the scan pushes batches into the next operator; no `next` calls.
- **Columnar scans:** only the columns the query needs are read.

## In BusTub

The executors of Project 3, task 1 (`seq_scan_executor.cpp`, `insert_executor.cpp`, `update_executor.cpp`, `delete_executor.cpp`, `index_scan_executor.cpp`) are stubs (`UNIMPLEMENTED("TODO(P3): Add implementation.")`) with the contract in the header comments. The 2025 BusTub executors are **batched**: `Next(std::vector<Tuple> *tuple_batch, std::vector<RID> *rid_batch, size_t batch_size)` fills the vectors with up to `batch_size` rows and returns whether it produced any.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `auto Next(std::vector<Tuple> *tuple_batch, std::vector<RID> *rid_batch, size_t batch_size) -> bool` | `fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool>` |
| `std::optional<TableIterator> iter_` | `Option<TableIterator<'e>>` |
| `throw` from `Evaluate` | `Err` through `?` |

**Port rule:** an out-parameter pointer becomes a `&mut` argument; an exception becomes a `Result`.

## Learn more

- PostgreSQL's [executor README](https://github.com/postgres/postgres/blob/master/src/backend/executor/README) · [Volcano: an extensible and parallel query evaluation system](https://paperhub.s3.amazonaws.com/dace52a42c07f7f8348b08dc2b186061.pdf)
