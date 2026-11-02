`ORDER BY a DESC LIMIT 5` is planned as a `Limit` whose child is a `Sort`. Executed as written, it sorts the whole table to keep five rows. The top-N executor of module 3g does the same job with a heap of five rows. This rule finds the shape *Limit over Sort* and replaces the two nodes with one `TopN`. A sort without a limit and a limit without a sort are left alone.

> [!CHECK] Which output schema does the new `TopN` node have, the sort's or the limit's, and what are its children? In `select * from (select * from t order by a desc limit 5) order by a asc limit 3` how many rewrites happen, and in which order must the rule visit the nodes? What could change in the *result* if the top-N broke ties differently from sort-then-limit?
> ||The limit node's output schema (it is the node being replaced, and its parent expects it), and the *sort's* children (the sort's input becomes the top-N's input). Two rewrites, one for each Limit-over-Sort pair, found because the rule rewrites the children first and then looks at the node. If ties came out in another order, the rows returned by `ORDER BY a LIMIT 3` could differ from the ones the unoptimised plan returns, a visible change of answer: the executor of module 3g keeps the earlier row on ties, matching the stable sort.||
>
> - What if the `Limit`'s child is a `Projection` over a `Sort`?
> - What is `n`?
> - Is `limit 0` a special case?

## The task

In `src/optimizer/optimizer.rs`, `optimize_sort_limit_as_top_n(&self, plan) -> PlanRef`: optimise the children first; if the node is a `Limit` whose child is a `Sort`, return a `TopN` plan with the **limit node's output schema**, the **sort's children**, the sort's `order_bys` and `n` equal to the limit; otherwise the node unchanged.

The tests: exact scenarios (order by with a limit becomes a top-N; order by alone and limit alone are left alone; the rows equal sorting everything; ties come out as sort-then-limit gives; limit 0 and a limit larger than the table; two top-Ns in one query and a top-N inside a join), and a property: **for random tables and random `order by` / `limit` combinations, the plan contains a `TopN` exactly when both an order by and a limit are present, and the rows equal the stable sort's prefix**.

## Your freedom

How you destructure the plan (`if let` chains, `match` on `(&plan.kind, &child.kind)`).

## The Rust toolbox

**Matching two levels.** `if let PlanKind::Limit { limit } = &node.kind { if let PlanKind::Sort { order_bys } = &node.children[0].kind { ... } }`, or one `match` on both.

**Cloning pieces.** `order_bys.clone()` and `child.children.clone()`: plan data is cheap to clone (`Arc`s).

**Bottom-up.** The first line, `let node = self.optimize_children(plan, &Self::optimize_sort_limit_as_top_n);`, is what lets nested pairs be found.

## If this is new

- L7 Enums & pattern matching: `if let` and nested patterns on enums.
- S7 Smart pointers & interior mutability: rebuilding `Arc` trees.
- Y5 Testing & verification: Build it: equivalence: the optimised plan and the plain plan return the same rows, on random tables.

## Tests

- A limit over a sort becomes a top-N, alone it does not; same rows as sorting everything; ties; limit 0 and larger than the table; two top-Ns, and top-N inside a join.
- Property: the plan and the rows for random queries.

## Hints

### The new node's children

`TopN` reads the sort's *input*, not the sort: the sort node disappears.

### Where the order of ties comes from

Nothing to do in the rule: the executor of module 3g keeps earlier rows on ties, and the property compares with the stable sort.

## Performance

At execution the difference is `O(n log N)` time and `O(N)` memory against `O(n log n)` and a spill to disk. For `LIMIT 10` on a billion rows that is the difference between a scan and a catastrophe.

**Measure it.** Run `select * from t order by a limit 10` on 200 000 rows with the rule and without it.

## Experiment

Optional. Predict first, then run.

1. **Projection in between.** Make the rule see through a `Projection` between the limit and the sort. What must the new plan do with the projection?
2. **Offset.** Plan `LIMIT 10 OFFSET 5`. What are `n` and the executor's job?

## Other designs

- **Pattern-match rule (ours).**
- **Cost-based:** use top-N only when `N` is small against the estimated rows.
- **Index order:** if an index gives the order, drop the sort and keep the limit.


## In BusTub

The rules live in `src/optimizer/` (`nlj_as_hash_join.cpp`, `sort_limit_as_topn.cpp`, `seqscan_as_indexscan.cpp`) and are stubs in Project 3; `optimizer_custom.cpp` chains them. `EXPLAIN` shows the plan before and after the optimizer.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `plan->GetType() == PlanType::Limit` then `GetChildPlan()` | `if let PlanKind::Limit { limit } = &plan.kind` |
| `std::make_shared<TopNPlanNode>(schema, child, order_bys, n)` | `PlanNode::new(schema, children, PlanKind::TopN { order_bys, n })` |

**Port rule:** a type tag plus a downcast becomes a `match` on the plan enum.

## Learn more

- PostgreSQL's "top-N heapsort" (`EXPLAIN` shows `Sort Method: top-N heapsort`) · module 3g's top-N
