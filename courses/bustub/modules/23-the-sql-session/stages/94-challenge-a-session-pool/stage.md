A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`SessionPool` in `src/common/session_pool.rs`, built on **your** `Session`: `with(|session| ...)` lends a session to a closure, waiting if the maximum number are already out, and when the closure returns takes the session back after **resetting it**: an open transaction is rolled back (even a failed one), and its default isolation level is back to the default. A pool that hands the next client a session in the middle of someone else's transaction is a data leak.

## Why

Connection pools are in front of nearly every database in production, and their worst bugs are not about speed but about what a session remembers. The classic one: client A leaves a transaction open or sets a session variable, the pool gives the same connection to client B, and B's statements run inside A's transaction or at A's isolation level. PostgreSQL poolers have a `server_reset_query` for exactly this.

## The contract

- `new(db, max)`: at most `max` sessions are created, lazily, over the pool's life.
- `with(f)`: waits until a session is free (an idle one, or room to create one), runs `f(&mut session)`, resets the session and returns it to the pool, and returns what `f` returned.
- The reset: if `in_transaction()`, run `rollback`; then `set default_transaction_isolation = 'snapshot'`.
- `created()` is how many sessions exist, `idle()` how many are waiting in the pool.

## Invariants

These must hold after every step, whatever the input:

- At most `max` closures run at once.
- A session handed out has no open transaction and the default level, whatever the previous borrower did.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Sequential `with` calls reuse one session (`created()` stays 1).
- A transaction left open by one `with` is gone in the next (the watermark is free).

## Examples

Worked cases (the tests include them):

```text
max 2, three threads calling `with`: never more than two inside at once; created() <= 2
```

## What the tests check

- Reuse and the bound.
- A leftover transaction (healthy and failed) is rolled back.
- A changed default level is reset.
- Many threads.

## Done when

All the `s4e_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A channel from a mutex and condition variables](/cookbook/p/mutex-condvar-channel): a semaphore from a mutex and condvar: wait for a free permit.
