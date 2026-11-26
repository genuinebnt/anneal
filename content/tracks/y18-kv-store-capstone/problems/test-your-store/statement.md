**Capstone, step 7 of 7.** The store you built passes its tests. Would the tests notice if it broke tomorrow? This time **you write the test**.

Write `check(make)`. It must **return normally for a correct store** and **panic for a broken one**. `make()` returns a new, empty store; call it as often as you like. The store never flushes by itself.

**A correct store** behaves like this:

- `get` reflects every `put` and `delete` so far, apart from the writes that a tear has removed.
- A write lives in the log until `flush` moves it, with the others, into a table. `flush` empties the log.
- `compact` never changes what `get` returns.
- `restart` changes nothing that `get` returns.
- `crash_tearing_last` loses **exactly** the newest write still in the log, if there is one, and restarts. Writes made afterwards are kept as usual. Tearing again removes the next newest write in the log.

The tests run your `check` on two correct stores that work differently inside, and on stores that each break one rule. Some bugs show only after a particular history: several tables, a restart at the wrong time, a long log, or a crash after a crash. Your check must be **deterministic** and fast enough for a debug build.
