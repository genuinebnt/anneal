An aggregation is the first executor that cannot answer until it has seen **everything**: the total of a column is unknown until the last row. So `init` (or the first `next`) drains the child into the hash table of stage 1, and `next` then hands out one output row per group, in batches. The output row is the group-by values followed by the aggregate values. `SELECT DISTINCT g` is the same thing with no aggregates, and `HAVING` is a filter the planner puts above.

> [!CHECK] `select count(*), sum(v) from t` on an empty table returns one row `0, NULL`; `select g, count(*) from t group by g` on the same empty table returns no rows. Why does the first have an answer and the second not, and what does your executor do differently in the two cases?
> ||With no `GROUP BY` there is exactly one group (the whole input), even if it is empty, so there is always a row (`count(*)` of nothing is 0). With `GROUP BY g`, groups come from the values that appear: no rows, no groups. The executor distinguishes them by whether the plan has group-by expressions: if the table is empty after reading the child and there are none, it adds the group of the empty key with its initial values.||
>
> - When does the child get drained: `init` or the first `next`?
> - What if `init` is called twice (a nested-loop join re-initialises its right child)?
> - In which order are the groups returned?

## The task

In `src/execution/executors/aggregation_executor.rs`:

- `make_aggregate_key(tuple)`: evaluate every group-by expression on the child's tuple (with the child's output schema) into an `AggregateKey`; `make_aggregate_value(tuple)`: evaluate every aggregate's input expression into an `AggregateValue`.
- `init`: initialise the child; empty the table; for every tuple of the child `insert_combine` its key and value; if the table is still empty and there is **no** `GROUP BY`, add the empty key's group with `insert_initial`; then build one output tuple per group (group-by values, then aggregate values, with the plan's output schema) and reset the cursor.
- `next`: hand out the next at-most-`batch_size` of those tuples (each with a default rid); `true` if the batch is not empty.

The tests: exact scenarios (group by one column; several columns and expressions; no `GROUP BY` is one row; empty input with and without `GROUP BY`; `HAVING`; more groups than a batch; `select distinct`; `init` can be repeated; an aggregation under another operator), and a property: **for random tables with NULLs in both columns**, `group by g` returns one row per group with the five aggregates a Rust fold gives, the no-`GROUP BY` form is one row, and `select distinct g` is the list of groups.

## Your freedom

Whether you build the output rows in `init` or lazily, how you iterate the table's groups (`entries()` is a `HashMap` iterator, so the order is unspecified: tests compare sorted rows), and whether you keep the results as tuples or as a `Vec<Vec<Value>>` converted per batch.

## The Rust toolbox

**Draining a child.** `while self.child.next(&mut tuples, &mut rids, BUSTUB_BATCH_SIZE)? { for t in &tuples { ... } }`: the same loop as the insert executor.

**Evaluating expressions into a key.** `AggregateKey { group_bys: self.plan.group_bys.iter().map(|e| e.evaluate(tuple, schema)).collect::<Result<Vec<_>>>()? }`.

**Cursor over a `Vec`.** `self.results[self.cursor..].iter().take(batch_size)` then `self.cursor += n`.

**Building an output tuple.** `Tuple::new(&[key_values, aggregate_values].concat(), &self.plan.output_schema)`.

**The hash table's `entries()`.** An iterator of `(&AggregateKey, &AggregateValue)`; clone what you keep.

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): iterating a `HashMap`, the order is not defined.
- S6 Iterators: `take`, `skip`, slicing as a cursor.
- [S1 Option & Result](/t/s1-option-result): collecting `Result`s.
- The optional *hash aggregation* concept.
- [F4 Hashing & purpose-built structures](/t/f4-hashing-structures): Right structure for the job: hash tables for grouping and joining; when a scan or an index is better.
- Y5 Testing & verification: Build it: differential tests: three algorithms against each other and a naive one.

## Tests

- Group by one column; by several columns and by an expression; no `GROUP BY` is one row; empty input with and without `GROUP BY`; `HAVING`; more groups than a batch; `select distinct`; `init` repeated; as input of another operator.
- Property: random tables against per-group folds, with NULLs.

## Hints

### Empty input

Look at the table *after* draining: empty and no group-by expressions means "add the one group with its initial values" (`count(*)` 0, the rest NULL).

### `init` twice

Clear the table at the start of `init`. The executor above you may call `init` again to rewind (a nested loop join does, once per left row).

## Performance

Building the table is `O(n)` hash operations; output is `O(groups)`. Memory is the number of groups times the size of a key and its running values: a group-by on a unique column holds the whole table in memory. Real engines spill partitions to disk when the table grows too big.

**Measure it.** Group a million rows by 10 and by 1 000 000 distinct values and compare memory and time.

## Experiment

Optional. Predict first, then run.

1. **Streaming.** If the child is sorted by `g`, fold runs instead of using a hash table. What do you need to know about the child, and who tells you?
2. **Spill.** Cap the table at 1 000 groups: what would you do with the rest?

## Other designs

- **Hash aggregation (ours, BusTub's).**
- **Sort aggregation:** sort, then fold runs (reuses module 3g's sort).
- **Hybrid with spilling** (Grace hashing): partition to disk, aggregate each partition.
- **Pre-aggregation** below a join or exchange.

## In BusTub

`aggregation_executor.cpp`, `nested_loop_join_executor.cpp`, `hash_join_executor.cpp` and `nested_index_join_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`); the header comments carry the contract. The executors are batched, as in module 3e.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `SimpleAggregationHashTable aht_; SimpleAggregationHashTable::Iterator aht_iterator_;` | `table: SimpleAggregationHashTable`, `results: Vec<Tuple>`, `cursor: usize` |
| `MakeAggregateKey` / `MakeAggregateValue` | the same helpers returning `Result<AggregateKey>` |
| `Next(std::vector<Tuple> *tuple_batch, ...)` | the same batch protocol with `&mut Vec` |

**Port rule:** an iterator member into a hash table becomes a materialised `Vec` and an index (the table's iterator would borrow it).

## Learn more

- PostgreSQL's [aggregate functions](https://www.postgresql.org/docs/current/functions-aggregate.html) · module 3g's sort
