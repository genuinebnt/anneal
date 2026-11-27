A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`expired` and `next_check_in` in `src/common/idle_reaper.rs`: given what the server knows about its sessions (an id, whether a transaction is open, when it last did something) and the current time, say which sessions to abort, oldest first, and how many milliseconds until the earliest one that is not yet expired will be. The server's timer thread uses `next_check_in` to sleep exactly as long as it needs to.

## Why

A client that opens a transaction and goes to lunch holds back the garbage collector for the whole database: every version written since is retained, and every scan walks it. Production systems have a setting for it (`idle_in_transaction_session_timeout` in PostgreSQL) and the decision is a pure function of the session table and the clock, which is the right shape for a function that must be right and is hard to test with real time.

## The contract

- A session is expired when it has a transaction open and `now - last_active` is **strictly greater** than the timeout (use `saturating_sub`: a clock that went backwards expires nothing).
- `expired` returns the ids of expired sessions, the longest idle first, ties by smaller id first.
- Sessions with no open transaction never expire, however long they have been idle.
- `next_check_in` is the smallest `timeout - idle + 1` over sessions that have a transaction open and are not yet expired (the moment the earliest one becomes expired); `None` if there is none.

## Invariants

These must hold after every step, whatever the input:

- `expired` only names sessions that have a transaction open.
- Whatever `expired` names would also be named a millisecond later.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Raising the timeout never adds an id to the result.
- Advancing `now` by `next_check_in` makes at least one more session expired.

## Examples

Worked cases (the tests include them):

```text
timeout 100: a (txn, idle 150), b (txn, idle 100), c (no txn, idle 900) -> [a]; next check in 1 ms (b becomes expired at 101)
```

## What the tests check

- Which sessions expire and in which order.
- No transaction, no expiry.
- The next deadline.
- A property.

## Done when

All the `s4e_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Timing wheel](/cookbook/p/timing-wheel): timers for idle timeouts, and when a heap's next deadline is the better choice.
