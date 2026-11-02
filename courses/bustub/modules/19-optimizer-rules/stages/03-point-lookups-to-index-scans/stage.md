`SELECT * FROM t WHERE id = 5` on a table with an index on `id` should not read the table. This rule turns a **sequential scan with a filter** into an **index scan with keys** when the filter contains a **point lookup** on an indexed column: `id = 5`, `5 = id`, or an `OR` of such equalities on the same column (`id = 4 or id = 7`). Two details make it correct: the index scan keeps the **whole** predicate as its filter (the index finds candidate rows; the other conditions still have to hold), and a predicate that is not a point lookup on an *indexed* column leaves the scan alone.

> [!CHECK] Which of these become index scans on a table with an index on `v1` only: `v1 = 2`; `3 = v1`; `v1 = 4 or v1 = 5`; `v1 > 2`; `v2 = 20`; `v1 = 2 or v2 = 20`; `v1 = 5 and v3 = 445`; `v1 = null`? For the last, what rows must come back? For the `and`, which condition does the index use and which does the filter check?
> ||Index scans: the first, second, third and the `and` (the index uses `v1 = 5`; the filter `v1 = 5 and v3 = 445` is applied to what it finds). Sequential: `v1 > 2` (a range, not a point), `v2 = 20` (no index), `v1 = 2 or v2 = 20` (an OR over two columns cannot be answered by one index). `v1 = null` finds nothing: `NULL` is never equal to anything, and an index does not hold NULL keys.||
>
> - Why keep the whole predicate as the scan's filter?
> - What does `extract_point_lookup` return for `(v1 = 1) or (v1 = 2)`?
> - What if the table has two indexes on two columns of an `AND`?

## The task

In `src/optimizer/optimizer.rs`:

- `extract_point_lookup(predicate: &ExprRef) -> Option<(u32, Vec<ExprRef>)>`: an `Equal` comparison between a `ColumnValueExpression` and a `ConstantValueExpression`, in either order, gives `(the column's index, [the constant])`; an `OR` whose two sides are both point lookups **on the same column** gives that column and the keys of the left side followed by those of the right (nested `OR`s by recursion); anything else is `None`: another operator, column = column, constant = constant, an `OR` over different columns or with a non-lookup side, an `AND` (the rule deals with `AND` conjunct by conjunct).
- `optimize_seq_scan_as_index_scan(&self, plan) -> PlanRef`: optimise the children first; if the node is a `SeqScan` with a filter predicate: take the whole predicate and, if it is an `AND`, each of its conjuncts as candidates (`Optimizer::conjuncts`); for the first candidate for which `extract_point_lookup` succeeds **and** an index exists on that column (`self.match_index(table_name, column)`), return `PlanNode::new(the scan's output schema, vec![], PlanKind::IndexScan { table_oid, index_oid, filter_predicate: Some(the whole predicate), pred_keys: keys })`; otherwise the node unchanged.

The tests: exact scenarios (column = constant either way round; an OR of equalities gives all the keys in order; other predicates are not point lookups; an OR over different columns or with another condition is not one; a negative constant, a computation is refused; equality on an indexed column uses the index; an OR of equalities looks up each key; other predicates and other columns stay sequential scans; another condition is checked on what the index finds; comparing with NULL finds nothing; updates and deletes see the same rows as a scan; an empty table with an index), and a property: **for random tables with an index on `v1` and random AND-ed predicates (point lookups, ORs of lookups, comparisons, conditions on other columns)**, the plan has an `IndexScan` exactly when some part is a point lookup on `v1`, and the rows equal a plain filter on a vector.

## Your freedom

Which candidate you try first (the first usable one wins in the tests), and how you walk the predicate.

## The Rust toolbox

**Downcasting both children.** `cmp.children()[0].as_any().downcast_ref::<ColumnValueExpression>()` and the same for `ConstantValueExpression`; try both orders with a small helper.

**Recursion for the OR.** `fn extract(p) -> Option<(u32, Vec<ExprRef>)>`: for a `LogicExpression` of type `Or`, `let (c1, mut k1) = extract(left)?; let (c2, k2) = extract(right)?; (c1 == c2).then(|| { k1.extend(k2); (c1, k1) })`.

**`bool::then`.** `(a == b).then(|| value)` is `Some(value)` or `None` in one expression.

**Candidates in order.** `let mut candidates = vec![]; Optimizer::conjuncts(predicate, &mut candidates);` plus the whole predicate; `for c in candidates { if let Some(..) = .. { return ... } }`.

**The catalog lookup.** `self.match_index(table_name, column)` returns the index oid if there is an index on exactly that column.

## If this is new

- L7 Enums & pattern matching: downcasts and `if let` ladders.
- [S1 Option & Result](/t/s1-option-result): `?` on options, `then`, `zip`.
- The optional *access paths* concept.
- Y5 Testing & verification: Build it: equivalence: the optimised plan and the plain plan return the same rows, on random tables.

## Tests

- Point lookups: either order; ORs with all keys in order; non-lookups; negative constants.
- Rule: indexed columns; ORs; sequential scans for ranges, other columns and ORs over columns; the whole predicate stays a filter; NULL; updates and deletes; empty tables.
- Property: random predicates against the plan shape and a plain filter.

## Hints

### The filter keeps everything

`v1 = 5 and v3 = 445`: the index narrows to the row with `v1 = 5`; `v3 = 445` still has to be checked. The scan's filter is the whole predicate, and the keys are only a way to find candidates.

### Indexes do not hold NULLs

A lookup of `NULL` finds nothing, which agrees with SQL (`v1 = NULL` is never true). A table with NULLs in the indexed column is still read correctly by a point lookup; reading it in key order through the index would miss those rows.

## Performance

A point lookup is one tree descent and a heap read: microseconds, independent of table size. A scan reads every page. On a million rows the rule is the difference between a millisecond and a second.

**Measure it.** Time `select * from t where v1 = 500000` on a million-row table with and without the index.

## Experiment

Optional. Predict first, then run.

1. **Ranges.** Extend the rule to `v1 > 5` using the leaf chain (module 2c's `begin_at`). What does the index scan executor need?
2. **Two indexes.** With indexes on `v1` and `v2` and `v1 = 1 and v2 = 2`, which conjunct should win?

## Other designs

- **First usable conjunct (ours).**
- **Cost-based:** estimate selectivity of each candidate (statistics, histograms) and pick the cheapest, or intersect two indexes.
- **Bitmap scans** (PostgreSQL): combine several indexes' results before reading the heap.
- **Index-only scans** when the index covers the query.

## In BusTub

The rules live in `src/optimizer/` (`nlj_as_hash_join.cpp`, `sort_limit_as_topn.cpp`, `seqscan_as_indexscan.cpp`) and are stubs in Project 3; `optimizer_custom.cpp` chains them. `EXPLAIN` shows the plan before and after the optimizer.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `dynamic_cast<const ColumnValueExpression *>(expr->GetChildAt(0).get())` | `expr.children()[0].as_any().downcast_ref::<ColumnValueExpression>()` |
| `std::optional<std::pair<uint32_t, std::vector<AbstractExpressionRef>>>` | `Option<(u32, Vec<ExprRef>)>` |
| `catalog_.GetTableIndexes(name)` then a loop | `self.match_index(table_name, column)` |

**Port rule:** an `optional` of a pair is an `Option` of a tuple; a loop over the catalog's indexes is a helper returning `Option`.

## Learn more

- PostgreSQL's [index scans](https://www.postgresql.org/docs/current/indexes-types.html) · BusTub's [`seqscan_as_indexscan.cpp`](https://github.com/cmu-db/bustub/blob/master/src/optimizer/seqscan_as_indexscan.cpp)
