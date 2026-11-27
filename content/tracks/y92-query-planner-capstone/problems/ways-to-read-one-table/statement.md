This is capstone step 6. There is often more than one way to read a table. The planner builds each way and prices it.

`leaf_plans(catalog, table, conditions)` returns the ways to get the rows of `table` for which **all** the conditions hold. Every condition reads only this table.

**The first plan** is `Scan(table)`. If there are conditions, a `Filter` sits on it. When there is one condition the filter holds it; otherwise it holds `All` of them in the given order. (This is how every filter in this step is written.)

**Then one plan for each indexed column** that has at least one usable condition, in the order of `indexed`. Usable means a `Compare` on that column with anything but `<>`. The plan is an `IndexScan` that covers exactly the overlap of those ranges.

The other conditions go into a `Filter` above it, in their order. With no other conditions there is no filter. If the overlap is empty, the scan must read no rows.

**Rows.** The topmost node of every plan has `rows = table rows × selectivity(All(conditions))`. Under a filter, the `Scan` keeps `rows` = table rows. The `IndexScan` keeps `rows = m`, where `m = between(low, high)` from the column's summary.

**Costs** (constants are in the starter). A table has `pages` = rows ÷ 100, rounded up, and at least 1.

| Node | `cost` |
|---|---|
| `Scan` | `pages × SEQ_PAGE + rows × CPU_ROW` |
| `IndexScan` | `(INDEX_DESCENT + min(m, pages)) × RANDOM_PAGE + m × CPU_ROW` |
| `Filter` | input cost + input `rows` × `CPU_CHECK` × number of conditions it holds |

A filter's "number of conditions" counts the entries of the list it was made from.

**Order.** An index plan has `order = Some(the indexed column)`, on the filter too. A scan plan has none.

**About the starter.** It holds reference code for the earlier capstone steps, so this step can be solved and checked on its own. Reading it gives those steps away. You write only the parts marked `todo!()`.
