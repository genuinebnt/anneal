A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`ThrottledDisk` in `src/storage/disk/throttled_disk.rs`: a `DiskIo` that wraps another disk and lets writes through only at a limited rate (a **token bucket**). A write over the limit is refused at once with `ErrorKind::WouldBlock` and never reaches the disk underneath; reads and deletes are never limited. Time comes from a `Clock` you are given, so a test can move it by hand.

## Why

A database that can write faster than its disk can take them, or that must leave room for reads, needs a brake. A token bucket is the standard one: constant memory, and it allows the short bursts real workloads have. Building it against an injected clock is also how time-dependent code is made testable: nothing here sleeps.

## The contract

- The bucket holds up to `burst` tokens and starts full. Tokens are added continuously at `writes_per_second`, never above `burst`. A write takes one whole token.
- Partial tokens add up between calls.
- Many threads may write at once; together they never get more writes than there are tokens.

## Invariants

These must hold after every step, whatever the input:

- The tokens are always between 0 and `burst`.
- The number of writes accepted in any window is at most `burst + rate * window`.
- A refused write has no effect on the disk underneath.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Moving the clock forward never makes a write that was accepted be refused.
- Doubling the rate and halving the time between calls gives the same decisions.
- After a long idle time exactly `burst` writes are accepted in a row, then none.

## Examples

Worked cases (the tests include them):

```text
rate 1, burst 3: write, write, write -> ok; write -> WouldBlock
rate 2, burst 1: write ok; +250ms write refused; +250ms write ok
```

## What the tests check

- A burst goes through and the next write is refused, and it is not on the disk.
- Tokens return continuously with time and never beyond the burst.
- Reads and deletes are never limited; threads share the bucket correctly.
- A property against a model that counts tokens exactly.

## Done when

All the `s1b_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Token-bucket rate limiter with an injectable clock](/cookbook/p/token-bucket): the same token bucket with an injectable clock for exact tests.
- [A deterministic laboratory comparing six rate-limit algorithms](/cookbook/p/rate-limiter-lab): token bucket compared with five other rate limiters on one deterministic clock.
