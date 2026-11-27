This is capstone step 8, the heart of the planner. Joining the same tables in a different order can cost a thousand times more or less. Find the cheapest order.

A `JoinGraph` describes the problem.

`leaves[i]` is the plan set for table `i`, and `leaf_rows[i]` its row count. An `Edge` is a condition between tables, with its selectivity. Bit `i` of `edge.leaves` is `leaves[i]`. Every edge touches at least two tables.

**`rows_of(graph, set)`** is the product of `leaf_rows` for the tables in `set`, times the selectivity of every edge whose tables all lie inside `set`.

**`best_order(graph, join)`** returns the plan set for all tables together. `join(outer, inner, conditions, rows)` returns the ways to join two plans. The tests supply their **own** `join`, with made-up costs and orders.

- A tree of joins splits a set into an outer part and an inner part, again and again, down to single tables. **Every** shape is allowed, and either part may be the outer one.
- Call `join` with plans of the two parts. Pass the conditions of exactly the edges that lie inside the two parts together, but inside neither part alone. Pass `rows_of` of the whole set.
- **No pairing without a condition:** use only splits that have at least one condition. Only if no tree can be built that way, allow **every** split instead.
- For every output order that such a tree can end with, the result holds one plan, the cheapest with that order.

You may assume that `join` behaves well. Its results depend on the two plans and its arguments only. Making an input cheaper, without changing its order, never makes a result dearer.

With no tables, or more than `MAX_TABLES` (10), return an empty set without calling `join`.

**The call budget.** The tests count calls of `join`. You may call it at most twice as often as this: once for each pair of **kept** plans, for each allowed split of each set. Searching the same set again and again goes far past that.

The made-up plans only say which tables they read. Use `plan_tables`, or better, your own set numbers.

**About the starter.** It holds reference code for the earlier capstone steps, so this step can be solved and checked on its own. Reading it gives those steps away. You write only the parts marked `todo!()`.
