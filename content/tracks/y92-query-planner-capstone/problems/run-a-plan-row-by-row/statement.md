This is capstone step 4. A **physical plan** says exactly how to get the rows. Your job is to run one.

A `Plan` has a `node`, its `inputs`, and three notes for later steps (`rows`, `cost`, `order`). Running a plan ignores the notes. The data sits behind the `Store` trait. **Each call of `row` reads one row, and the tests count the calls per table.**

**`schema(plan, store)`** gives the columns of the rows a plan produces, in order. A scan has all columns of its table. A join has the left input's columns, then the right's.

**`holds(condition, row, schema)`** says whether a row passes. A missing value (`None`) satisfies **no** comparison, `<>` included. It equals nothing, not even another missing value. An empty `All` holds, and an empty `Any` does not.

**`open(plan, store)`** starts a plan, and `next` hands out its rows one by one. `run` collects them all.

| Node | Rows, in order |
|---|---|
| `Scan` | the table's rows by row number |
| `IndexScan` | rows whose value in the column is in `low..=high`, in the order of `store.index` |
| `Filter` | the input rows that pass |
| `Project` | the input rows, cut and reordered to the listed columns (repeats allowed) |
| `NestedLoop` | for each outer row, in order: each inner row, in order, glued to it; kept if the condition passes or there is none |

**How reading is checked**

- `open` reads nothing.
- Rows are read only when needed to produce the next row. A scan or filter stops at the row it hands out.
- An index scan reads only the rows it hands out. An empty range reads nothing.
- A nested loop reads its whole inner input **once**. It may do so at the first `next`. It reads outer rows one at a time, as needed.
- After `next` has returned `None`, it keeps returning `None` and reads nothing more.

When the outer input is empty, the inner one may be read or not. Plans are well formed: every column a node mentions is in its input.

**About the starter.** It holds reference code for the earlier capstone steps, so this step can be solved and checked on its own. Reading it gives those steps away. You write only the parts marked `todo!()`.
