**Capstone, step 6 of 7.** Tables pile up. Reads get slower, and old values and delete markers take space. The starter holds the reference code of steps 1 to 5.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 5, *An append that fails half-way*
- Part 7, *Save a file that a crash cannot tear*: also its restart rules (file names, their order and leftovers), in step 5's `open`
- Part 8, *A bundle you read from the back*
- Part 10, *One block is enough*
- Part 12, *Fix: a saved filter that forgets its keys*
- Part 14, *Unsaved changes over a read-only catalogue*

**`compact()`** replaces all tables with at most one. It is `compact_with(|path| fs::remove_file(path))`.

**`compact_with(remove)`** removes old files only through `remove`.

- With fewer than two tables it does nothing.
- The new table holds the newest version of every live key, and **no delete markers**. It takes the next id. If no key is live, no table is written.
- The memtable and the log are not touched.
- If a table cannot be read, or the new one cannot be written, nothing is removed.
- If `remove` fails, return its error at once.

**The rule that matters.** At every moment, a crash may stop compaction. So after **any** call to `remove`, whether it worked or failed:

1. reopening the directory gives the same answers as before compaction;
2. if `compact_with` has returned an error, **this** store also still gives the same answers, `table_count()` equals the number of table files, and a later `compact` works.

A deleted key must never come back. The new table does not say that it was deleted.
