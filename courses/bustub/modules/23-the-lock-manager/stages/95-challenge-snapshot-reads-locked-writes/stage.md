A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`HybridStore` in `src/concurrency/hybrid_store.rs`: a key-value store that takes the best of both halves of module 4. **Reads** are snapshot reads: a transaction sees the committed state as of the moment it began, plus its own writes, and a read never waits for anyone. **Writes** take an exclusive row lock from **your** `LockManager` and wait if another writer holds it, instead of aborting on conflict. `get_for_update` locks the row first and reads the newest committed value, the way `SELECT ... FOR UPDATE` does.

## Why

PostgreSQL and InnoDB are neither pure MVCC nor pure locking: readers use versions so they never block or get blocked, writers use row locks so they queue instead of failing. The two mechanisms answer different questions: versions answer "what did the world look like when I started", locks answer "who may change this row now". The trap is `FOR UPDATE`: after waiting for a lock, the value your snapshot shows may already be stale, and a read-modify-write on it loses an update.

## The contract

- `new(locks)` takes the lock manager to use. `begin()` returns a `HybridTxn` (given: its id, its read timestamp, its lock-manager transaction at repeatable read).
- `get(txn, key)` returns the value as of `txn.read_ts` with the transaction's own uncommitted writes on top. It never takes a lock and never waits.
- `put(txn, key, value)` takes IX on the table and X on the row (waiting if it must) and buffers the write. It returns the lock error if the transaction was chosen as a deadlock victim.
- `get_for_update(txn, key)` takes the same locks, then returns the newest committed value (or the transaction's own write), not the snapshot value.
- `commit(txn)` installs the writes at a new commit timestamp, then releases every lock, and returns the timestamp. `abort(txn)` discards the writes and releases every lock.

## Invariants

These must hold after every step, whatever the input:

- A transaction never sees another transaction's uncommitted write, and never sees a commit that happened after it began (except through `get_for_update`).
- Two writers are never inside the same row at once; versions of a key are installed in commit order.
- Locks are held until commit or abort.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A read-only transaction reads the same values however many others commit meanwhile.
- A reader's answer does not depend on whether a writer holds the row.
- `get_for_update` then `put` of value + 1, from any number of threads, adds exactly the number of increments.
- An aborted transaction leaves no trace.

## Examples

Worked cases (the tests include them):

```text
x = 0. A begins and reads 0; B writes 5 and commits; A reads 0 (snapshot), `get_for_update` gives 5
two writers on one key: the second `put` returns only after the first commits
```

## What the tests check

- Snapshots across commits.
- A reader while a writer holds the row (watchdog).
- A second writer waits, then proceeds.
- Abort frees the row.
- Counter increments from several threads.
- A deadlock is broken and the survivor commits.
- A property against a model of sequential transactions.

## Done when

All the `s4d_c6` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [MVCC protocols compared](/cookbook/p/mvcc-protocols-compared): MV2PL: writers lock and wait instead of aborting, compared with SI.
- [Two-phase locking](/cookbook/p/two-phase-locking): SELECT ... FOR UPDATE to avoid the read-then-upgrade deadlock.
