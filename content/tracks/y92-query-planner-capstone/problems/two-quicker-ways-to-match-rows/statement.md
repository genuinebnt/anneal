This is capstone step 5. A nested loop compares every pair. When two columns must be equal, there are two much better ways. Add them, and add a sort.

`left_key` is a column of the first input, and `right_key` of the second. Both joins keep the pairs whose keys are **equal and present**, and for which `rest` also passes, if there is one.

**Order of output.** Both joins give pairs in the order of the first input. Pairs of the same first row follow the order of the second input.

**`HashJoin`**

- Nothing is read at `open`.
- The whole second input is read once, at the first `next`.
- Rows of the first input are read one at a time, only as needed.

**`MergeJoin`**

- Both inputs arrive **sorted by their key**, missing keys first.
- Nothing is read at `open`. Neither input may be read to the end first.
- When a row is handed out, the first input has been read no further than that pair's first-input row.
- At that moment, count the second-input rows whose key is not above the pair's key, missing ones included. The second input has been read at most that many rows, **plus one**.
- No row of either input is read twice.

**`Sort(column)`** gives the input rows ordered by that column, missing values first. Rows with equal values keep their input order. It reads nothing until the first `next`, and then everything, once.

As before, after `None` a join or sort keeps returning `None` and reads nothing more. The tests count reads. They do **not** grade how fast the matching is.

**About the starter.** It holds reference code for the earlier capstone steps, so this step can be solved and checked on its own. Reading it gives those steps away. You write only the parts marked `todo!()`.
