A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`is_retryable`, `backoff_ms` and `run_with_retry` in `src/common/retry.rs`: the client's half of optimistic concurrency. A transaction that lost a conflict is not a bug, it is the contract: run it again. `run_with_retry` calls a closure up to a limit, retrying only errors that are conflicts, and `backoff_ms` says how long to wait before attempt `n` so that two clients that collided do not collide again in lockstep.

## Why

Every system with optimistic concurrency control (snapshot isolation, serializable validation, compare-and-swap) pushes retrying onto the caller. Done badly it either gives up on the first conflict, retries errors that can never succeed (a syntax error, ten times), or retries in lockstep so that the same two transactions collide for ever. The three decisions are small and each is a classic bug. The first is made on the SQLSTATE code: a message is written for people and reworded at will, and a retry loop that matches words retries an internal error that happens to say "could not commit".

## The contract

- `is_retryable(e)`: true exactly when `e.code()` is `40001`, a serialization failure (a write conflict or a refused commit, from your session of 4e-04, with any context on top); false for everything else, whatever its message says.
- `run_with_retry(max_attempts, f)` calls `f(attempt)` with attempts numbered from 1. Success returns `Ok((value, attempts_used))`. A retryable error is retried while attempts remain; any other error, or the last failure, is returned at once. `max_attempts` of 0 behaves as 1.
- `backoff_ms(attempt, seed)`: the wait before attempt `attempt + 1`: an exponential base `min(2^(attempt - 1), 64)` milliseconds with jitter, a value in `[base / 2, base]` that depends only on `(attempt, seed)`.

## Invariants

These must hold after every step, whatever the input:

- The closure is called at most `max_attempts` times and never after a non-retryable error.
- `backoff_ms` is deterministic and never above 64.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Attempts used is 1 when the first call succeeds.
- The upper envelope of the backoff doubles until the cap.
- Different seeds give different jitter for some attempt.

## Examples

Worked cases (the tests include them):

```text
f fails with a conflict twice, then succeeds: Ok((v, 3))
f fails with a syntax error: Err after one call
an internal error worded "could not commit: it conflicts" (XX000): not retryable
```

## What the tests check

- Real conflicts and refused commits are retryable, under context too; nothing else is, whatever its words.
- Success, retried success, giving up; non-retryable errors stop at once.
- Backoff bounds and determinism.

## Done when

All the `s4e_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Exponential backoff with full jitter](/cookbook/p/retry-backoff-jitter): exponential backoff with full jitter.
- [Error chain retryable](/cookbook/p/error-chain-retryable): classifying errors as retryable or fatal.
