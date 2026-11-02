If the right table has an **index on the join column**, there is a third way: for each row of the outer (left) input, ask the index for the matching inner rids, fetch those rows from the heap and output the pairs. No scan of the inner table per row (stage 3) and no hash table to build (stage 4): each outer row costs one tree descent and a few page reads. It is the fastest join for a small outer input and a big indexed inner table, and the optimizer (given) chooses it when it finds an equality on an indexed column.

> [!CHECK] The index finds a rid for the key `5`, but the heap tuple at that rid is marked deleted. What does the join output for that outer row, inner and left? Then: how many index lookups does a left table of one million rows cost, and when is a nested index join worse than a hash join?
> ||The deleted row is not a match (the heap says which rows are alive; module 3e's delete removes the index entry, but a reader must still check): an inner join outputs nothing for it, a left join outputs the outer row padded with NULLs. A million lookups at about a microsecond each is a second or so, which beats building a hash table of a big inner table; but if the outer side is nearly as large as the inner side, a hash join does one pass over each side and avoids the random heap reads.||
>
> - What if the outer key is NULL?
> - What if several inner rows have the key (the index of this course has unique keys)?
> - Which table is indexed?

## The task

In `src/execution/executors/nested_index_join_executor.rs` (the struct, `new` (it looks up the inner table and the index in the catalog) and the `TupleStream` over the outer child are given):

- `probe(outer) -> Result<Vec<Tuple>>`: evaluate `self.key_predicate` on the outer tuple (with the child's schema) to get the key value (a NULL key matches nothing); make a one-column key tuple with the index's key schema; look it up (`self.index.index.scan_key(&key)`); fetch each rid from the inner table (`get_tuple(rid)?`) and keep the tuples that are **not deleted**.
- `init`: forget pending output and initialise the child.
- `next`: until the batch is full: hand out `pending` first; take the next outer tuple (none: stop); `probe` it; for every inner tuple queue `outer values ++ inner values` (laid out with the plan's output schema); if there is none and the join is a **LEFT** join, queue the outer values followed by NULLs for the inner columns (`nulls_for(&self.inner_schema)`).

The tests (SQL with `set force_optimizer_starter_rule=yes`, which makes the optimizer pick the index join): the optimizer turns an equality join on an indexed column into an index join; inner and left joins; either side of the `=` may be written first; a deleted inner row is not joined; rows inserted after the index was made are found; a NULL outer key matches nothing; and a property: **for random tables with a unique indexed key on the right**, inner and left joins return exactly the naive join's rows, and the plan says `NestedIndexJoin`.

## Your freedom

Whether you probe row by row or batch the probes, and how you handle the heap fetch.

## The Rust toolbox

**Evaluating the outer key.** `self.key_predicate.evaluate(outer, child_schema)?` gives a `Value`; `value.is_null()` is the NULL test.

**Key tuples for the index.** `Tuple::new(&[value], &self.index.key_schema)`: the same shape module 3e's index scan used.

**Through the trait object.** `self.index.index.scan_key(&key)` returns `Vec<Rid>`; the index is a `Box<dyn Index>` and you do not know which kind.

**The heap says what is alive.** `let (meta, tuple) = self.inner_table.table.get_tuple(rid)?; if meta.is_deleted { continue }`.

**Queueing a row.** `self.pending.push_back(self.joined(&outer, &inner))` as in stages 3 and 4.

## If this is new

- [L4 Traits & dispatch](/t/l4-traits-dispatch): `Box<dyn Index>`.
- [S1 Option & Result](/t/s1-option-result): `?` through a loop, early `continue`.
- The optional *access paths* concept.
- [F4 Hashing & purpose-built structures](/t/f4-hashing-structures): Right structure for the job: hash tables for grouping and joining; when a scan or an index is better.
- Y5 Testing & verification: Build it: differential tests: three algorithms against each other and a naive one.

## Tests

- The optimizer chooses the index join for an equality on an indexed column.
- Inner and left joins; either side of `=`; a deleted inner row; rows inserted later; a NULL outer key; more outer rows than a batch.
- Property: random tables against the naive join.

## Hints

### The key expression

`key_predicate` is the expression over the *outer* child whose value is looked up in the inner index: for `outer.x = inner.y` it is `outer.x`.

### Deleted rows

Index entries are removed when a row is deleted (module 3e), but a scan that started before may still see a stale entry; always consult the heap's flag.

## Performance

One descent (`depth + 1` page latches) and one heap fetch per outer row: a few microseconds. With a warm buffer pool a million probes take seconds; with a cold one each probe may be two page reads from disk.

**Measure it.** Join 1 000 outer rows to a million-row indexed table, then the other way round with a hash join; compare.

## Experiment

Optional. Predict first, then run.

1. **No index.** Drop the index and run the same SQL with the flag. Which join does the plan show now?
2. **Batch the probes.** Sort the outer keys of a batch before probing. What improves, and what happens to the output order?

## Other designs

- **Index nested loops, row by row (ours).**
- **Batched probes sorted by key:** better locality in the tree and the heap.
- **Index-only joins** when the index holds all needed columns.
- **Hash join** (stage 4) when there is no useful index.

## In BusTub

`aggregation_executor.cpp`, `nested_loop_join_executor.cpp`, `hash_join_executor.cpp` and `nested_index_join_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`); the header comments carry the contract. The executors are batched, as in module 3e.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `index_info->index_->ScanKey(key, &rids, txn)` | `index.index.scan_key(&key)` returning `Vec<Rid>` |
| `table_info->table_->GetTuple(rid)` returning `pair<TupleMeta, Tuple>` | `get_tuple(rid)?` returning the same pair |
| `key_predicate_->Evaluate(...)` | `key_predicate.evaluate(..)?` |

**Port rule:** an out-parameter list of rids becomes a returned `Vec`.

## Learn more

- PostgreSQL's [index scans in joins](https://www.postgresql.org/docs/current/indexes-types.html) · module 3e's index scan
