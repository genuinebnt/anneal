Implement the transaction layer of a tiny in-memory database with **snapshot isolation**. Keys are strings and values
are `i64`. Everything is single-threaded: transactions overlap because their calls are interleaved.

- `begin()` returns a new `TxId`, never reused. The transaction's **snapshot** is the data committed at that moment.
- `get(tx, key)` returns the transaction's own latest write or delete of `key` if it has one, else the value in its
  snapshot. Later commits by others are invisible.
- `put(tx, key, value)` and `delete(tx, key)` are private until commit. Deleting a key that does not exist is still a
  write.
- `commit(tx)` makes all its writes visible at once to transactions that **begin afterwards**. It fails with
  `Err(Conflict)` if **any key it wrote** has been committed by another transaction since it began (first committer
  wins). A failed commit applies nothing and ends the transaction. Reads never cause conflicts.
- `abort(tx)` discards the writes.
- Any call with a transaction that has committed, failed or aborted (or was never begun) returns `Err(NotActive)`.

**Garbage collection.** `versions()` is the number of committed versions stored (a committed delete counts as a version).
`vacuum()` removes every version that no active or future transaction can need, and returns how many it removed.
Precisely: let the **horizon** be the oldest snapshot among active transactions, or the latest commit when none is
active. For each key, `vacuum` keeps every version committed after the horizon, plus the newest version at or before the
horizon **unless that one is a delete**. Everything else goes. `vacuum` never changes what any `get` returns, and never
changes whether a later `commit` succeeds.
