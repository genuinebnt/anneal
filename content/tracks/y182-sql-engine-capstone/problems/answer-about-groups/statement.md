This is the last of the 5 steps of the small SQL engine. Add `GROUP BY` and the aggregates `COUNT`, `SUM`, `MIN` and `MAX`.

The starter holds **the reference code of steps 3 and 4**, which gives them away, so solve them first. Everything works except grouped queries. Write `grouped`, and any types it needs. You may change anything else as long as steps 3 and 4 keep their behaviour. With your parser from step 2, `db.run(&parse(sql)?)` is then your whole engine.

**A query is grouped** if it has a `GROUP BY` or at least one aggregate item.

**Checks**, all before any row is touched, in this order:

1. The table, then the `WHERE` expression, as before (the starter does both).
2. The `GROUP BY` column, in the table: `UnknownColumn`.
3. The items, left to right:
   - `*`: `StarInGroups`.
   - An expression is allowed only if it is exactly the `GROUP BY` column, as an `Expr::Column`. Anything else, a calculation on that column included, is `NotGrouped(name)` with the item's `name`.
   - An aggregate of a column: the column must exist (`UnknownColumn`). `SUM` needs an `Int` column, else `WrongType { needed: Int, found }`. `COUNT`, `MIN` and `MAX` take any type.
   - An aggregate of `*`: only `COUNT` is allowed. The others are `NeedsColumn(which)`.
4. The `ORDER BY` name must be the title of an **output** column: `UnknownColumn`. It means the first output column with that title.

**Titles.** The group column keeps its item's `name`. An aggregate is called `count(*)`, `count(x)`, `sum(x)`, `min(x)` or `max(x)`, in small letters.

**Values.**

- Rows that pass `WHERE` are grouped by the value of the group column. All NULLs form **one** group.
- Without `GROUP BY` there is exactly **one** group, even when no row passes. With `GROUP BY` and no rows, there are no groups.
- `count(*)` counts the group's rows. `count(x)` counts those where `x` is not NULL.
- `sum`, `min` and `max` skip NULL. With no value left, they give NULL.
- `min` and `max` use step 4's ascending order.
- `sum` adds in insertion order. If the running total leaves `i64`, the query fails with `Overflow`, even if later values would bring it back.

**The result** has one row per group. The groups come in ascending order of their key, NULL first. Then `ORDER BY`, if any, sorts these rows; it is stable, as before. Then `LIMIT` cuts.

**What runs.** With `LIMIT 0`, nothing runs after the checks. Otherwise every row is read, in insertion order, whatever the limit: for each, the condition, then the row's group takes it in. A fault ends the query. If one row overflows several sums, the answer is `Overflow` either way.

A table may have 100 000 rows, and there may be 20 000 groups.
