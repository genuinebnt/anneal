**Capstone, step 4 of 7.** Now the pieces become a store. The starter holds the reference code of steps 1 to 3. `Store::open` gets a new or empty directory in this step; reopening comes next.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 5, *An append that fails half-way*: the solution, in the log
- Part 7, *Save a file that a crash cannot tear*: the publishing, in the table code
- Part 8, *A bundle you read from the back*: the footer reading, in the table code
- Part 10, *One block is enough*: the lookup, in the table code
- Part 12, *Fix: a saved filter that forgets its keys*: the solution, in the table code

Recent writes live in memory, sorted by key (the **memtable**), and in the log `wal.log`. Older writes live in tables. `flush` moves the memtable into a new table.

**Writes.** `put` and `delete` add the change to the log first, and only then to memory. If the log write fails, the error is returned and nothing else changes.

**Reads.** `get` and `scan` answer as if all writes had gone to one map. The newest mention of a key decides, also when it is a delete. A table that cannot be read is an error, never a reason to answer from an older one. `scan(from, to)` covers `from <= key < to`. An empty or reversed range gives nothing.

**`flush()`** does nothing when the memtable is empty. Otherwise it writes the memtable as the next table (ids 1, 2, 3, and so on), and only after that empties the log. If the table cannot be written, it returns the error, nothing is lost, and the id is not used up. The store keeps no files besides the log and the tables.

**Automatic flush.** `memtable_bytes()` adds `key length + value length + 16` for **every** write since the last flush (a delete has value length 0). After a write, if it is at least `flush_threshold`, the store flushes. A failure of **this** flush does not make the write fail, because the write has already happened. The next write tries again.

**`table_count()`** is the number of tables.
