This is capstone step 9. All the pieces exist. Put them together, refuse bad queries, and show the plan.

**`naive_plan(query)`** is `Filter(condition, tables paired with Cross, from left to right)`.

**`optimize(query, catalog)`** first checks the query. Report the first failure in this order:

1. `NoTables`: the table list is empty.
2. `TooManyTables`: more than `MAX_TABLES`.
3. For each table in order: `UnknownTable` if it is not in the catalog or is 64 or more; `RepeatedTable` if it appeared before.
4. `TooDeep`: `All` and `Any` nested so that the depth is above `MAX_DEPTH` (64). A condition that is no list has depth 1. A list is 1 deeper than its deepest part.
5. `UnknownColumn`: a column of the condition, then of the output, whose table is not in the query or lacks that column. The tests have one such column per query.

A condition may be 200 000 deep. It must be refused without crashing.

**The plan.** Apply `push_down` to the naive plan. For each table, its filter's parts are its own conditions.

Build its `PlanSet` from `leaf_plans`; its row count is that of those plans. Each part of a `Join` is an edge with its `selectivity`. Order the joins with `best_order` and `join_plans`, and take the cheapest plan, `best`.

On top goes `Project(output)` with `best.rows`, cost `best.cost + best.rows × CPU_CHECK`, and `best.order` if that column is in the output.

**`explain(plan, catalog)`** prints one line per node, each ending in a newline. Inputs follow their node, first input first, indented two more spaces. A line is the label, then ` (rows=R cost=C)`, with R rounded to a whole number (`{:.0}`) and C to one decimal (`{:.1}`).

| Node | Label |
|---|---|
| scans | `Scan users`, `IndexScan users.age [5..9]`; an open end prints `-inf` or `inf` (for `i64::MIN` and `i64::MAX`) |
| filter, loop | `Filter C`, `NestedLoop C`, or just `NestedLoop` |
| keyed joins | `HashJoin a.x = b.y`, `MergeJoin a.x = b.y`, each followed by ` where C` if there is a rest |
| others | `Project a.x, b.y`, `Sort a.x` |

A condition `C` is written as `a.x < 5` (signs `= <> < <= > >=`), `a.x = b.y`, `a.x in (3, 1)`, or parts joined by ` and ` / ` or `. A list that is a part of a longer list goes in brackets. A list of one prints exactly as its part would in its place. An empty `All` is `true`, and an empty `Any` is `false`.

**About the starter.** It holds reference code for the earlier capstone steps, so this step can be solved and checked on its own. Reading it gives those steps away. You write only the parts marked `todo!()`.
