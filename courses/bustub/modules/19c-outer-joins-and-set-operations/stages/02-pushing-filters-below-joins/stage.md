The cheapest row is the one that is never read. A query that joins two million-row tables and then keeps the rows with `a.y > 15` does the join work for every row of `a`, including the ones the filter throws away a moment later. If the filter moves *below* the join, onto the scan of `a`, the join never sees those rows. The optimizer rule that does this is called **filter pushdown**, and for inner joins it is a free win. For outer joins it is a trap: one side of a LEFT join may take a filter and the other side may not, and a rule that does not know the difference returns wrong answers faster.

> [!CHECK] `select a.x from a left join b on a.x = b.x where b.z is null` finds the rows of `a` that have no partner in `b`. Suppose the optimizer moves `b.z is null` onto the scan of `b`. What does the join then return, and why is it different from the original? Now do the same for `where a.y > 15`: why is that move safe?
> ||With the filter on `b`'s scan, only `b` rows with a NULL `z` survive (probably none); every `a` row now finds no partner and is padded, so the query returns **all** of `a` instead of the unmatched rows. In the original the NULLs in `b.z` were *made by the join* as padding, and the filter looked at them afterwards; pushed down, it looks at `b` before any padding exists. `a.y > 15` is safe: dropping a left row before the join drops exactly the output rows built from it, and the filter would have dropped them afterwards anyway.||
>
> - Which join types let which side take a filter? Why does a FULL join take none?
> - A filter says `a.y > 15 AND a.y + b.z > 100`. What moves and what stays?
> - A conjunct moves onto the right input. Its column numbers referred to the joined row. What do they have to refer to now?

## The task

Implement `optimize_filter_pushdown` in `src/optimizer/optimizer.rs` (it is run by `optimize`, before the merge rules):

- A filter directly above a join is split into its `AND` parts. Each part that reads columns of **one** input only moves onto that input, as a new filter directly above it (later rules merge it into a scan), if the join type allows it: an inner join takes parts on either side; a LEFT join takes parts on the left only; a RIGHT join on the right only; a FULL join takes none.
- Parts that read both inputs, or that may not move, stay in a filter above the join; if nothing is left, the filter disappears.
- A part that reads no column at all (`1 = 2`) stays where it is.
- The rule also applies to the new filters (a filter can move through several joins to its table) and to every other node of the plan.
- The answer of the query must not change: whatever the rule does, `explain` shows a different plan and the rows are the same.

## Your freedom

How you split and rebuild conjunctions (a left-deep chain of `AND`, a list), how you find the columns an expression reads, and whether a part that reads no columns goes up, down or stays. The tests look at the plan shapes listed below and at the rows.

## The Rust toolbox

**Tree rewriting.** A rule is a function from a plan to a plan: handle the node you are interested in, call yourself on the children, and rebuild the node with the new children. The given `optimize_children` does the last part.

**`Vec::partition`-style splitting.** Walk the conjuncts once and push each into one of three vectors: left, right, stay. Which vector depends on the columns it reads and on the join type.

**Renumbering.** In the joined row the right input's columns come after the left input's. A conjunct that moves onto the right input needs its column numbers reduced by the number of left columns; the given `shift_columns` does it.

## If this is new

- [S1 Option and Result](/t/s1-option-result): plans are rebuilt as new values, not edited in place.
- S6 Trait objects: expressions are trait objects; the given `columns_read` walks them.

## Tests

- Both sides of an inner join (and of a join written with a comma) receive their own parts; nothing is left above.
- A part that reads both sides stays above the join; the part about one side still moves.
- LEFT: parts on the left move; a part on the padded side stays (`b.z is null` returns the unmatched rows); RIGHT: the mirror image.
- FULL: nothing moves.
- A filter reaches its table through two joins.
- A condition without columns, and a random property: with the rule and with only the starter rules, the same rows come out, for every join type and a set of conditions.

## Hints

### The rule for each join type is one question

"If I drop rows from this input before the join, do I drop exactly the output rows that the filter would drop afterwards?" For a left row of a LEFT join, yes. For a right row of a LEFT join, no: dropping it can turn a matched left row into a padded one that the filter above would never have produced.

### Test the nasty case first

Write the `b.z is null` query before anything else, and run it with the rule switched off and on. If the two differ you have found the bug the stage is about.

### Do not forget the second round

After moving a part below a join, the new filter sits above another join, or above a scan. Apply the rule to the new children, or the filter stops at the first join.

## Performance

How much a pushed filter saves depends on how many rows it removes and how expensive the join is per row. If the filter removes 99% of `a` and the join compares every pair, the join does 1% of the comparisons: a hundred-fold gain from moving one node. The cost of the rule itself is one walk over the plan.

**Measure it.** Join two 1 000-row tables with `a.x = b.x` and the filter `a.y < 10`. Compare `explain analyze` row counts with and without the rule, at the join and at the scan.

## Experiment

Optional. Predict first, then run.

1. **Let a LEFT join take filters on its right side.** Which test fails first? Which rows does the query that looks for unmatched rows return?
2. **Let a FULL join take filters on both sides.** Which query shows it?

## Other designs

- **Pull filters up as well as down:** transitive predicates (`a.x = b.x AND a.x > 5` implies `b.x > 5`) let a filter on one side become a filter on the other.
- **Move the condition into the join:** for an inner join, a filter above is equivalent to extra `ON` terms; for an outer join it is not.
- **Cost-based placement:** an expensive predicate (a function call) is sometimes better applied after the join if the join removes more rows than it does.

## In BusTub

BusTub's optimizer has a rule that merges a filter into a nested loop join and, in project 3, rules that turn joins into hash joins; it does not ask you to push filters further down, and it has no outer-join rules. This stage is an extension, and the reason the rule needs the care of the check above is exactly what BusTub's tests never exercise.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a recursive function that mutates the plan through `shared_ptr` | a recursive function that returns a new `Arc<PlanNode>` |
| `dynamic_cast<ColumnValueExpression *>` to find columns | `as_any().downcast_ref::<ColumnValueExpression>()` |

**Port rule:** an optimizer rule that returns a new tree cannot corrupt the tree another thread is executing.

## Learn more

- [PostgreSQL: the planner's treatment of outer joins](https://www.postgresql.org/docs/current/explicit-joins.html) · CMU 15-445: query optimization
