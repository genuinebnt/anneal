A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`TimedLatch` in `src/common/timed_latch.rs`: an exclusive latch with `try_acquire`, `acquire_timeout(d)` (waits at most `d` for the latch, returns whether it got it), and `release`. The latch is not owned by a thread: any thread may release it.

## Why

A latch that can be waited on for ever is how a buffer pool deadlocks, and the usual defence is a bounded wait that fails loudly instead. Doing it with a condition variable has two traps: a spurious wake-up must not count as success, and a timeout that expires just as the latch is released must not lose the grant.

## The contract

- `try_acquire()` takes the latch if it is free and returns whether it did.
- `acquire_timeout(d)` returns true as soon as the latch is acquired, and false if `d` passes first; it never returns true without holding the latch.
- `release()` frees the latch and wakes a waiter; returns false if the latch was not held.

## Invariants

These must hold after every step, whatever the input:

- At most one holder at any time.
- A `true` from `acquire_timeout` or `try_acquire` means the caller holds the latch until `release`.
- A wait that times out leaves the latch state unchanged.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `acquire_timeout(0)` behaves like `try_acquire`.
- Releasing within the timeout makes a waiter succeed; not releasing makes it fail after about `d`.
- Of N threads waiting, exactly one gets the latch per release.

## Examples

Worked cases (the tests include them):

```text
held; acquire_timeout(50ms) -> false after ~50ms
held; another thread releases after 20ms; acquire_timeout(1s) -> true
```

## What the tests check

- Free, held and timed-out cases.
- Release within the timeout.
- Release of a latch that is not held.
- Many waiters, one grant per release.

## Done when

All the `s1g_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [`std::sync` and thread coordination by use case](/cookbook/p/std-sync-primitives-use-cases): Condvar::wait_timeout_while and timed_out for waits with a deadline.
- [A channel from a mutex and condition variables](/cookbook/p/mutex-condvar-channel): rechecking the condition after every wake-up, spurious or not.
