This is capstone step 2. A query keeps the rows for which a condition holds. The planner needs the **fraction** it keeps, called the selectivity, from 0 to 1.

`Cond` is a tree.

`Compare` sets a column against a number. `Same` equates two **different** columns. `OneOf` lists allowed numbers. `All` needs every part and `Any` needs at least one.

**`range_of(cmp, v)`** returns the inclusive range of numbers allowed by `x cmp v`. When no number is allowed, it returns a range with `low > high`. `Ne` is not one range, so it returns `None`.

**`conjuncts(condition, out)` is already written** in the starter, and this step does not grade it. It appends the parts that must all hold, in order, opening up nested `All` lists.

**`selectivity(condition, catalog)`** follows these rules. "Share of n" means `n / table rows`, kept between 0 and 1, and 0 for a table with no rows.

| Condition | Selectivity |
|---|---|
| `x = v` | share of `equal_to(v)` |
| `x <> v` | share of `present() - equal_to(v)` |
| `<`, `<=`, `>`, `>=` | share of `between` over the range from `range_of` |
| `Same(a, b)` | `1 / max(distinct of a, distinct of b)`; 0 if either has no value |
| `OneOf` | the sum of the `=` shares over the **different** values, taken smallest first; at most 1 |
| `Any` | 1 minus the product of `(1 - s)` over its parts; empty: 0 |
| `All` | see below |

For `All`, first open it up with `conjuncts`.

All the **ordering** comparisons (`<`, `<=`, `>`, `>=`) on the same column become one range, their overlap, estimated once with `between`. An empty overlap gives 0.

 Every other part, `=` and `<>` included, is estimated on its own. Multiply everything. An empty `All` is 1.

**`q_error(estimate, actual)`** is how far off an estimate is, as a factor. Count both numbers as at least 1, then divide the larger by the smaller.

Results must match to within 0.000000001. A list can hold 20 000 parts.

**About the starter.** It holds reference code for the earlier capstone steps, so this step can be solved and checked on its own. Reading it gives those steps away. You write only the parts marked `todo!()`.
