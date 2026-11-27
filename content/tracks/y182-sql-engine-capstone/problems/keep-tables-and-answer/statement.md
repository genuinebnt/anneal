This is step 4 of 5 of the small SQL engine. Now it keeps tables. Write `Database::run`, which runs one `Statement` from step 2's tree.

The starter holds **step 3's reference code**: `compile`, `Compiled`, and helpers such as `column` and `need`. It gives that step away, so solve step 3 first. Tests build statements directly. This step has no aggregates and no `GROUP BY`; step 5 adds them.

A statement that returns a fault **changes nothing**.

**CREATE TABLE.** If the table name is taken: `TableExists(name)`. Otherwise, if a column name appears twice: `DuplicateColumn(name)`, for the first name that repeats an earlier one. Answer `Created`.

**INSERT.** If there is no such table: `NoSuchTable(name)`. Then take the rows in order. A row with the wrong number of values is `WrongCount { needed, found }`. Take its values in order, each one fully before the next:

1. Compile it against **no columns**, so a column name in it is `UnknownColumn`.
2. Its type must be the column's type, or `None`. Otherwise it is `WrongType { needed, found }`. The **type** decides, not the value: `NULL = 1` is a `Bool`, even though its value is NULL.
3. Compute it.

Either every row is added, at the end of the table, or none. Answer `Inserted(number of rows)`. One insert may have 100 000 rows.

**SELECT**, first the checks. They all happen before any row is touched, in this order:

1. The table: `NoSuchTable`.
2. The `WHERE` expression, compiled against the table's columns. Its type must be `Bool` or `None`.
3. The `ORDER BY` name, which must be a column of the **table** (`UnknownColumn`). It need not be selected.
4. The items, left to right. `*` stands for all columns of the table, in their order, and may be mixed with other items.

The answer is `Rows { columns, rows }`. `columns` has an item's `name`, or the column names for `*`.

**Then the rows.** A row is kept when there is no `WHERE`, or its value is TRUE. FALSE and NULL drop it.

Run the condition and the items **only when the rules below say so**. A fault from them (such as a division by zero) ends the query. Which faults can happen is part of the answer, and the tests check it.

- With `LIMIT 0`, run nothing.
- **Without `ORDER BY`:** take rows in insertion order. For each one, run the condition; if it keeps the row, run the items at once, left to right. Stop for good when the result has `LIMIT` rows.
- **With `ORDER BY`:** first run the condition on every row, in insertion order. Sort the kept rows by that column of the table. Then run the items only for the first `LIMIT` rows, in sorted order.

**Sorting.** Ascending puts NULL first, then the values in step 3's order. Descending is the reverse order, so NULL comes last. Rows with equal keys stay in insertion order, **in both directions**.

A `LIMIT` may be as large as `u64::MAX`. A table may have 100 000 rows.
