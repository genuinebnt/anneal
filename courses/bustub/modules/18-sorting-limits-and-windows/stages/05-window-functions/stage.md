A **window function** is an aggregate that does not collapse rows: `sum(v) OVER (PARTITION BY g)` adds a column holding, for every row, the sum over that row's group. With an `ORDER BY` inside the window, the aggregate becomes **running**: for each row, the aggregate over the rows of its partition from the first up to the row and its **peers** (the rows equal to it on the order key). `rank()` numbers the peer groups: ties share a rank and leave gaps (1, 1, 3). The executor reads its child fully, sorts if some function has an `ORDER BY`, then for every window function computes one value per row.

> [!CHECK] Partition `g = 1` has `v = 10, 20, 20, 30` ordered by `v`. What are `sum(v) over (partition by g)`, `sum(v) over (partition by g order by v)` and `rank() over (partition by g order by v)` for each row? Why do the two `20` rows get the same running sum, and what would a `rows between unbounded preceding and current row` frame give them instead?
> ||Whole partition: 80 for all four. Running with peers: 10, 50, 50, 80 (both `20`s see the sum up to the *last* peer: 10 + 20 + 20 = 50). Rank: 1, 2, 2, 4. A `ROWS` frame counts physical rows, not peers: it would give 10, 30, 50, 80 (the course refuses explicit frames, since the default `RANGE` frame is what the tests ask for).||
>
> - What is the frame when there is no `ORDER BY`?
> - How is a NULL partition key treated?
> - Where in the output do the rows come: sorted, or in the input order?

## The task

In `src/execution/executors/window_function_executor.rs` (`init`, which reads the child, sorts if some function has an `order by`, calls the two functions below for each window function and builds the output tuples, is given; so are `aggregation_of` and the planner's rules):

- `partition_rows(&self, wf, rows) -> Result<Vec<Vec<usize>>>`: evaluate the function's `partition_by` expressions on each row into an `AggregateKey` and group the **indexes** of the rows by key: one `Vec<usize>` per distinct key, in order of first appearance, indexes in row order. No `partition by` means one partition with every row.
- `compute_whole_partitions(&self, wf, rows, partitions) -> Result<Vec<Value>>`: for each partition fold the function's value on each row into one running aggregate (`SimpleAggregationHashTable`, from module 3f) and give **every row** of the partition the final value. One value per row, indexed like `rows`.
- `compute_ordered(&self, wf, rows, keys, partitions, cmp) -> Result<Vec<Value>>` (`init` has sorted the rows by the common `order by`): walk each partition one **group of peers** at a time (rows whose keys compare `Equal`); for an **aggregate** fold the whole peer group into the running aggregate and give every row of the group the running value; for **rank** give every row of the group the 1-based index of the group's first row in the partition. One value per row.

The tests: exact scenarios (an empty window covers the whole table on every row; `partition by` gives each group its aggregate; NULLs in the argument are ignored and a NULL partition is a partition; several functions with different partitions are independent; the input order is kept without an `order by`; empty tables and expressions in the select list; the planner refuses a `group by` next to a window function; with an `order by` the aggregate runs up to the current row; peers share the value of the last peer; rank leaves gaps; `partition by` with `order by` restarts in each partition; descending order and NULLs; one `order by` serves every function that has one; `rank` needs an `order by` and other frames are refused), and a property: **against an `O(n²)` oracle that looks at the whole partition for every row**, on random tables with NULLs in the partition, order and value columns, the whole-partition aggregates, the running aggregates with peers and `rank` agree.

## Your freedom

How you group rows (a `HashMap` of indexes or sorting), how you fold peers, and how you carry the running aggregate.

## The Rust toolbox

**Indexes instead of rows.** Work with `Vec<usize>` positions into `rows`; the results are written to `out[i]` at the row's own position, so the output is independent of the partition order.

**`HashMap` plus order of first appearance.** `Vec<(AggregateKey, Vec<usize>)>` with a map from key to position in the `Vec` keeps the groups in order.

**Reuse stage 3f's table.** `SimpleAggregationHashTable::new(vec![agg_type])`, `generate_initial_aggregate_value`, `combine_aggregate_values`: the window aggregate is a one-aggregate group.

**Peers.** `while end < n && cmp.compare_keys(&keys[p[start]], &keys[p[end]]) == Ordering::Equal { end += 1 }` finds a peer group.

**Cloning values out.** `out[i] = value.clone();` for every row of the peer group.

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): grouping with a `HashMap`.
- S6 Iterators: slices of indexes, `while` loops with two cursors.
- [S3 Vec & slices](/t/s3-vec-slices): indexing `out[i]`.
- The optional *window functions* concept.
- Y5 Testing & verification: Build it: a sorted vector as the oracle for sort, limit, top-N and rank.

## Tests

- Whole-partition functions: empty window; `partition by`; NULLs; several functions; input order; empty tables; the planner's refusals.
- Ordered functions: running aggregates; peers; rank with gaps; restarts per partition; descending and NULLs; shared `order by`; refusals.
- Property: aggregates, running aggregates and rank against a quadratic oracle.

## Hints

### Two functions, one idea

Whole-partition is "fold everything, give everyone the result". Ordered is the same, one peer group at a time, giving the group the running result so far.

### Rank is a position, not a count

`rank` is `start + 1` of the peer group inside the partition (rows before the group, plus one), so ties leave gaps.

### Output order

Rows come out in the order `init` left them: sorted by the window's `order by` if there is one, otherwise the input order. Tests that mix functions compare sorted rows.

## Performance

Partitioning is one hash per row; the ordered pass is linear after the sort. The sort dominates: `O(n log n)` per distinct `order by`. A naive implementation that rescans the partition for every row is `O(n²)`, which is the oracle the property uses.

**Measure it.** Run `sum(v) over (partition by g order by o)` on a million rows with 10 and 100 000 partitions; where does the time go?

## Experiment

Optional. Predict first, then run.

1. **Exclude peers.** Make the running aggregate stop at the current row instead of the last peer. Which tests fail?
2. **`dense_rank`.** Number peer groups without gaps. How many lines change?

## Other designs

- **Sort once, walk partitions (ours).**
- **Segment trees** for arbitrary frames in `O(log n)` per row.
- **Incremental frames** (add the rows entering, remove those leaving) for `ROWS BETWEEN n PRECEDING AND CURRENT ROW`.
- **Hash-partition first** when partitions are many and small.

## In BusTub

`external_merge_sort_executor.cpp`, `limit_executor.cpp`, `topn_executor.cpp` and `window_function_executor.cpp` are stubs in Project 3 (`UNIMPLEMENTED("TODO(P3): Add implementation.")`). The 2025 version of the project asks for an external merge sort (`MergeSortRun`, `ExternalMergeSortExecutor<K>`), a top-N executor with a bounded heap, and window functions.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unordered_map<AggregateKey, std::vector<size_t>>` | `HashMap<AggregateKey, usize>` into a `Vec<Vec<usize>>` |
| `SimpleAggregationHashTable` per window function | the same type from module 3f |
| `cmp.Compare(a, b) == 0` for peers | `cmp.compare_keys(a, b) == Ordering::Equal` |

**Port rule:** a map from key to a vector of row positions stays a map from key to an index into a vector of vectors, to keep first-appearance order.

## Learn more

- PostgreSQL's [window functions](https://www.postgresql.org/docs/current/tutorial-window.html) · [window function calls](https://www.postgresql.org/docs/current/sql-expressions.html#SYNTAX-WINDOW-FUNCTIONS)
