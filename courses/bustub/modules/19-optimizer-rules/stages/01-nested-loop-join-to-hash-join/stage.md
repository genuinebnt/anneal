The planner turns SQL into the simplest correct plan: every join is a nested loop. The **optimizer** rewrites that plan into a faster one that returns **the same rows**. A rule is a function from a plan to a plan: look at a node, and if it has the shape the rule knows, replace it. This first rule recognises a join whose condition is a set of **equalities between a column of each side** (`a.x = b.p and a.y = b.q`) and turns the nested loop (`O(n × m)`) into a hash join (`O(n + m)`, module 3f). It must be careful in the other direction: if even one part of the condition is not such an equality (`a.x < b.p`, `a.y = 5`, an `OR`), a hash join would return the wrong rows, so the node stays as it is.

> [!CHECK] For each condition say whether it can be a hash join, and why: `a.x = b.p`; `b.p = a.x`; `a.x = b.p and a.y = b.q`; `a.x = b.p and a.y > 5`; `a.x = b.p or a.y = b.q`; `a.x + 1 = b.p`; `a.x = a.y`. Then: after the rewrite a left join keeps its unmatched rows. What property would you test to be sure the rewrite never changes the answer?
> ||Yes; yes (either order); yes with two keys; no (a part that is not an equality needs a residual filter, which this rule does not build); no (an OR of equalities is not a conjunction of keys); no (an expression is not a column key); no (both columns are from the left side: it is a filter, not a join key). Test **equivalence**: run the same query with the rule and without it on random tables, including NULLs and duplicates, and compare the multisets of rows; also compare with a plain nested loop over vectors.||
>
> - What does each side's key refer to after the rewrite: tuple 0 or tuple 1?
> - What do you do with the children of the node: rewrite them first?
> - Why only `Inner` and `Left`?

## The task

In `src/optimizer/optimizer.rs` (`Optimizer::conjuncts(expr, &mut out)`, which splits nested `AND`s into a list, is given):

- `extract_equi_join_keys(predicate: &ExprRef) -> Option<(Vec<ExprRef>, Vec<ExprRef>)>`: split the predicate into conjuncts; **every** conjunct must be a `ComparisonExpression` of type `Equal` whose two children are both `ColumnValueExpression`s, one reading tuple 0 (the left input) and the other tuple 1 (the right input), in either order; return `(left keys, right keys)`: the left columns and the right columns in the same order, each rebuilt as `ColumnValueExpression::new(0, col_idx, return_type)` (each key is evaluated on a tuple of its own side, so both read "tuple 0"); anything else makes the whole answer `None`.
- `optimize_nlj_as_hash_join(&self, plan) -> PlanRef`: optimise the children first (`self.optimize_children(plan, &Self::optimize_nlj_as_hash_join)`); if the node is a `NestedLoopJoin` of type **Inner** or **Left** whose predicate gives equi-join keys, return a `HashJoin` with the same output schema, children and join type and those keys; otherwise the node unchanged.

The tests: exact scenarios (one equality gives a key pair; either way round; several in any nesting give keys in order; one part that is not such an equality rules it out; both columns on one side or an OR is not a key; `conjuncts` flattens ANDs and keeps ORs whole; an equality join becomes a hash join; a cross join with a `WHERE` too; several conditions make several keys; other conditions keep the nested loop; left joins become hash joins; results equal the nested loop's; a three-way join becomes two hash joins), and two properties: **a predicate is a list of keys exactly when every part of it is a cross equality** (random mixes of equalities, same-side equalities, comparisons, constants and ORs, nested to any depth), and **for random tables with NULLs and duplicates and random join conditions, the plan contains a hash join exactly when every part is a cross equality, and the rows equal the naive join's with the rule and without it** (inner, left, and the cross join with a `WHERE`).

## Your freedom

How you walk the conjuncts (a loop with `?`, an iterator `collect::<Option<Vec<_>>>()`), and how you build the new node.

## The Rust toolbox

**Downcasting an expression.** `part.as_any().downcast_ref::<ComparisonExpression>()?` is `Some` only for that node type; `?` turns `None` into an early `return None` for the whole function.

**Matching a pair of indexes.** `match (a.tuple_idx(), b.tuple_idx()) { (0, 1) => (a, b), (1, 0) => (b, a), _ => return None }` handles both orders and rejects same-side pairs in one `match`.

**Rebuilding a node.** `PlanNode::new(schema.clone(), children.clone(), PlanKind::HashJoin { .. })` creates a new plan node; plan nodes are shared (`Arc`), so you clone pointers, not trees.

**Rewriting bottom-up.** `self.optimize_children(plan, &Self::rule)` applies the rule to every child and returns the node with new children; then you look at the node itself.

**Collecting options.** `iter.map(|p| key_of(p)).collect::<Option<Vec<_>>>()` is `None` if any element is `None`: "every part must be a key" in one line.

## If this is new

- L7 Enums & pattern matching: `PlanKind` is an enum; `matches!`, `if let`.
- S7 Smart pointers & interior mutability: plans are `Arc` trees you rebuild, never mutate.
- [S1 Option & Result](/t/s1-option-result): `?` on `Option`, collecting options.
- The optional *rule-based query optimization* concept.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): Define & implement: a rule as a function from plan to plan (a trait, if you like).
- Y5 Testing & verification: Build it: equivalence: the optimised plan and the plain plan return the same rows, on random tables.

## Tests

- Keys: one equality, either way round, several in any nesting, a part that is not a cross equality, both columns on one side, an OR, a constant; `conjuncts`.
- Rule: equality joins, cross joins with a `WHERE`, several keys, other conditions keep the nested loop, left joins, same rows as the nested loop, three-way joins.
- Properties: keys exist exactly for cross equalities; the plan and the rows for random joins.

## Hints

### Both keys read tuple 0

After the rewrite the left key is evaluated on a *left* tuple and the right key on a *right* tuple, so each is a column of "tuple 0" of its own side, whatever it was in the combined predicate. Rebuild with `ColumnValueExpression::new(0, col_idx, ty)`.

### `a.x = b.p and a.y > 5` is not a hash join

A hash join would ignore `a.y > 5`. A smarter optimizer would split it into a hash join plus a filter; this rule is deliberately all-or-nothing, and the property tests it.

### Rewrite the children first

A three-way join is a join whose child is a join: the rule has to reach the inner one too.

## Performance

The rule runs once per query on a tree of a few nodes: microseconds. What it buys is orders of magnitude at execution: two tables of 100 000 rows go from ten billion predicate evaluations to two hundred thousand hash operations.

**Measure it.** Run an equality join of two 5 000-row tables with and without the rule (`set force_optimizer_starter_rule=yes` turns the rule off) and compare the times.

## Experiment

Optional. Predict first, then run.

1. **Left only.** Make the rule fire only for `Inner`. Which test and which property notice?
2. **Residual filters.** Extend the rule: equalities become keys, other conjuncts become a `Filter` above the hash join. Which cases of the property change?

## Other designs

- **All-or-nothing equi-join rule (ours).**
- **Keys plus a residual predicate:** a hash join whose probe also checks the rest of the condition.
- **Cost-based choice:** pick hash, merge or nested loop from table statistics.
- **Join reordering** (module 4's lectures): which table builds, which probes.

## In BusTub

The rules live in `src/optimizer/` (`nlj_as_hash_join.cpp`, `sort_limit_as_topn.cpp`, `seqscan_as_indexscan.cpp`) and are stubs in Project 3; `optimizer_custom.cpp` chains them. `EXPLAIN` shows the plan before and after the optimizer.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `dynamic_cast<ComparisonExpression *>(expr.get())` | `expr.as_any().downcast_ref::<ComparisonExpression>()` |
| `std::make_shared<HashJoinPlanNode>(schema, left, right, left_keys, right_keys, type)` | `PlanNode::new(schema, children, PlanKind::HashJoin { .. })` |
| `plan->CloneWithChildren(children)` | `optimize_children` then `PlanNode::new` |

**Port rule:** a class hierarchy of plan nodes becomes one `PlanKind` enum; `dynamic_cast` becomes a downcast (expressions) or a `match` (plans).

## Learn more

- PostgreSQL's [planner and optimizer](https://www.postgresql.org/docs/current/planner-optimizer.html) · CMU 15-445 lecture notes on query optimization
