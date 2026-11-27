**Capstone, step 7 of 8.** Now put the bank to work: several writers and a courier, taking turns in any order, with a test that decides the order. The starter holds working code for steps 1 to 4 and 6.

Each writer has a `Script`: commands to do in order, each on a stream. Command number `i` of writer `w`, both from 0, always uses `command_id(w, i)`.

**`run_day(ledger, scripts, choose)`** repeats:

1. List who can move: the indexes of writers with commands left, in rising order. Then add the **courier**, listed last, if the log has lines at or after its position. The courier is a new `Courier` for this day, starting at position 0.
2. If the list is empty, the day is over. Return the report.
3. Call `choose(n)` once, where `n` is the length of the list. It returns an index below `n`. Move that one.

**A writer's step** is one of two:

- with no account loaded, `load` the stream of its current command, and keep the account;
- otherwise, `try_commit` with that account, which is then dropped.

A conflict keeps the same command, to be loaded again. `Applied`, `Duplicate` and a refusal finish the command.

**The courier's step** is one `tick`.

**The report** counts `applied`, `duplicates` and `conflicts`. It lists each refusal as `(writer, index, why)`, in order. It adds up `received` and `bounced` over all ticks. Stuck transfers are not reported.

A store error stops the day and is returned.

Accounts may already hold history when the day begins. The same choices must always give the same journal.
