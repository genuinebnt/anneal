This is capstone step 7. Now price the ways to join two plans.

**`plan_tables(plan)`** gives the tables a plan reads, one bit each: those of all its scans and index scans.

**`join_plans(left, right, conditions, rows)`** returns the ways to join `left` and `right`. All the `conditions` must hold, and the result has `rows` rows. Every returned plan has that `rows`.

The **key** is the first condition `Same(a, b)` with one column from each side, written either way round. The other conditions form `rest`. One condition is written as itself, two or more as `All` in their order, and none as `None`. The nested loop's own condition is written the same way from all conditions.

Return, in this order:

1. `NestedLoop` with all conditions, inputs `[left, right]`, order = `left.order`.
2. Only with a key: `HashJoin` with the left plan's column as `left_key`, inputs `[left, right]`, no order.
3. Only with a key: `MergeJoin`, order = `Some(left_key)`. Each input is the plan itself if its `order` is its key. Otherwise it is a `Sort` on that key over it, with the same `rows` and with `order` = the key.

**Costs.** `L` and `R` are the inputs, with their `cost` and `rows`.

| Node | `cost` |
|---|---|
| `NestedLoop` | `L.cost + R.cost + L.rows × R.rows × CPU_CHECK + rows × CPU_ROW` |
| `HashJoin` | `L.cost + R.cost + R.rows × HASH_BUILD + L.rows × HASH_PROBE + rows × CPU_ROW + extra` |
| `Sort` | input cost `+ n × log2(n) × SORT_STEP`, where `n = max(input rows, 2)` |
| `MergeJoin` | `L.cost + R.cost + (L.rows + R.rows) × CPU_CHECK + rows × CPU_ROW + extra`, where L and R are its inputs, sorts included |

`extra` is `rows × CPU_CHECK` when there is a `rest`, and 0 otherwise.

**`PlanSet`** keeps plans for one set of tables: for each different `order` (none is one too), only the cheapest. A new plan replaces a kept one of the same order only if it is **strictly** cheaper. `cheapest` is the cheapest of all, or `None` if the set is empty.

**About the starter.** It holds reference code for the earlier capstone steps, so this step can be solved and checked on its own. Reading it gives those steps away. You write only the parts marked `todo!()`.
