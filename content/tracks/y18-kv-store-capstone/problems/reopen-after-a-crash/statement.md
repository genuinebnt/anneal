**Capstone, step 5 of 7.** The process can die at any moment. The starter holds the reference code of steps 1 to 4, in which `Store::open` only works on a new directory. Make it open any directory the store has used.

**Spoiler warning.** The starter gives away work from these parts of the series. Solve them first if you have not.

- Part 5, *An append that fails half-way*
- Part 7, *Save a file that a crash cannot tear*
- Part 8, *A bundle you read from the back*
- Part 10, *One block is enough*
- Part 12, *Fix: a saved filter that forgets its keys*
- Part 14, *Unsaved changes over a read-only catalogue*: the way to keep deletes over older data

**What a crash can leave:**

- tables under their final names, which are complete;
- a half-written table under its temporary name;
- a log with a torn or zero-filled end;
- a log that still holds writes which a table already has, if the crash came during a flush.

**`open(dir, flush_threshold)`** creates the directory if needed. After it, `get` and `scan` answer as just before the crash, apart from writes whose frame did not reach the log whole.

- A table is a file whose name is exactly what the store would write: `sst_<id>.sst`. A temporary table is the same with `.tmp` in place of `.sst`: `sst_<id>.tmp`. **Remove** temporary tables. Leave every other file alone.
- A table or log that is `Corrupt` makes `open` fail. Do not skip it.
- A new table's id is one more than the highest id present, and never reuses one.
- `memtable_bytes()` counts the writes replayed from the log, by the rule of step 4.

Ids may grow past six digits.
