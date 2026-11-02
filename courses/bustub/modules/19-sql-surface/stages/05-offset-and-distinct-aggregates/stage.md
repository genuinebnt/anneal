Two features, both of which the planner refuses by name today: `select ... limit 3 offset 2` ends with `OFFSET clause is not supported yet.`, and `count(distinct x)` with `distinct agg is not implemented yet`. They have nothing in common except that each needs the *planner* to make a different shape of plan than before, and that each has an obvious implementation with a catch. OFFSET needs a new plan node and a new executor, and the catch is that batches do not line up with the offset. `DISTINCT` inside an aggregate needs *each value counted once per group*, and the catch is that the aggregation executor you wrote in 3f keeps one number per group and cannot know what it has already seen.

> [!CHECK] `select g, count(distinct x), sum(x) from t group by g`. Both aggregates read the same column, but one wants every value once and the other wants every row. Can one hash table of one number per group answer both? What are your options, and what does each cost?
> ||No: `count(distinct x)` needs to know which values the group has already seen, which a single running number cannot say, while `sum(x)` needs every row, duplicates included. Options: (1) a per-group set of seen values for the distinct aggregates alongside the running numbers for the plain ones (state grows with the number of distinct values); (2) two aggregations stacked: an inner one that groups by `(g, x)` to remove duplicates, an outer one that aggregates the survivors: correct for distinct aggregates alone, but `sum(x)` would see the deduplicated rows, so mixing them needs the plain aggregates computed separately and joined by group; (3) sort by `(g, x)` and count changes. Real engines choose (1) or (3) per aggregate.||
>
> - What does `count(distinct x)` do with NULLs?
> - Over an empty input without GROUP BY, what do `count(x)` and `count(distinct x)` return in your engine?
> - Which of the three options is the cheapest for a column with ten distinct values and a billion rows?

## The task

- **`OFFSET n`**, alone or with `LIMIT`: `order by x limit 3 offset 2` skips two rows, then returns three. An offset past the end returns nothing; `limit 0` returns nothing; `offset 0` changes nothing. The plan shows `Limit` over a new `Offset` node over the rest.
- **`count(distinct e)`, `sum(distinct e)`, `min`/`max(distinct e)`**: each value of `e` once per group, NULLs ignored as ever, per group and in HAVING and in expressions around them.
- A query that mixes DISTINCT and plain aggregates (`count(distinct x), count(*)`) is either answered correctly or refused with a `NotImplemented` error that says why: never answered wrongly.

Where to work: `plan_select` and `plan_select_agg` in `src/planner/planner.rs` (the two error messages above are where to start), the new `PlanKind::Offset` (given, with its plan type and `EXPLAIN` text), `create_executor` in `src/execution/executor_factory.rs` (an `Offset` plan has no executor yet: wire yours in, as 3d-06 wired the functions), and `OffsetExecutor` in `src/execution/executors/offset_executor.rs`.

## Your freedom

For DISTINCT: a planner rewrite into two aggregations (what the reference solution does, and why it refuses mixing), per-group sets inside your own aggregation executor (you own that file from 3f), or sorting. For OFFSET: the executor is small; the freedom is in where the node sits and in what it asks its child for.

## The Rust toolbox

**A batch that crosses the offset.** The child hands you batches of up to 20 tuples; the offset may end in the middle of one. `Vec::drain(k..)` takes the tail of a batch without copying the head.

**`BTreeSet` or `HashSet` for "seen".** If you keep sets in the executor, a `HashSet<Value>` needs `Hash` for `Value`; the planner rewrite avoids the question by letting a hash aggregation do the deduplication.

**Shadowing to swap a subtree.** In the planner, `let (child, group_bys) = if distinct { (inner, outer_keys) } else { (child, group_bys) };` swaps the input of the aggregation without touching the code below it.

## If this is new

- S5 Queues and heaps: why a bounded heap beats sorting for the first rows.
- [S4 Maps and sets](/t/s4-maps-sets): sets as "have I seen this".

## Tests

- OFFSET with and without LIMIT; past the end; across many batches; with ORDER BY.
- DISTINCT aggregates: count, sum, min and max; NULLs; per group and in HAVING.
- Empty input and expressions around DISTINCT aggregates; mixing is right or refused.
- A property against slicing a sorted list and a set.

## Hints

### Count what you skip, not what you pass on

`skipped` is the executor's whole state. When a batch contains the offset boundary, pass on the part after it; when it is all before the boundary, ask for the next batch.

### Deduplicate by grouping

`select count(distinct x) from t group by g` is `select count(x) from (select g, x from t group by g, x) group by g`. An aggregation with no aggregate functions *is* a DISTINCT. The inner output's columns are `g` and `x` in that order.

### Plan the arguments against the right input

After the rewrite the aggregate's argument is a column of the *inner* aggregation's output, not an expression over the table. Build a column reference by position, not by planning the original expression again.

## Performance

OFFSET is not free: the engine must produce and throw away `n` rows, so `offset 1000000` costs a million rows even though it returns ten. That is the argument for keyset pagination (`where id > last_seen order by id limit 10`). DISTINCT aggregates cost an extra hash table with one entry per distinct `(group, value)`: memory proportional to the number of distinct values, and the reason `count(distinct)` on a high-cardinality column is slow on every engine.

**Measure it.** `select x from big order by x limit 10 offset k` for `k` = 0, 10 000 and 1 000 000: time grows with `k`. Then `count(distinct x)` on a column with 10 and with 1 000 000 distinct values.

## Experiment

Optional. Predict first, then run.

1. **Look at `EXPLAIN` for `order by x limit 3 offset 2`.** Is the optimizer's top-N rule applied? Why not, and what would it take (top-N with `n = limit + offset`)?
2. **Apply the offset before the sort** by mistake. Which test fails?

## Other designs

- **Top-N with an offset:** keep the best `limit + offset` rows in the heap and drop the first `offset` on output; the right plan for `order by ... limit ... offset ...` when the offset is small.
- **A `Limit` node with both numbers**, as PostgreSQL has: one node, one pass, and the optimizer sees them together.
- **Approximate distinct counts** (HyperLogLog, module 0d): constant memory and a small error, what analytics engines use for `count(distinct)` on huge columns.

## In BusTub

BusTub's binder reads `OFFSET`, and its planner refuses it ("OFFSET clause is not supported yet"); `LIMIT` is planned to a `Limit` node. `DISTINCT` aggregates are refused the same way ("distinct agg is not implemented yet"); `SELECT DISTINCT` is planned as a group-by of all output columns, which is the trick the rewrite reuses.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::unordered_set<Value, ValueHash>` per group | a hash aggregation over `(group, value)` pairs |
| `batch.erase(batch.begin(), batch.begin() + k)` | `batch.drain(..k)` |
| `size_t` arithmetic that wraps | `usize` with `saturating_sub` / `min` where a count can pass zero |

**Port rule:** an operator is correct when it is correct for every batch size, including one and one larger than the input.

## Learn more

- [PostgreSQL: LIMIT and OFFSET](https://www.postgresql.org/docs/current/queries-limit.html) · [Use The Index, Luke: paging through results](https://use-the-index-luke.com/sql/partial-results/fetch-next-page)
