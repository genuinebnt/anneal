A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`ToItem` in `src/concurrency/timestamp_ordering.rs`: one data item under **basic timestamp ordering**. Each transaction has a timestamp fixed at begin; the item remembers the largest timestamp that read it (`rts`) and wrote it (`wts`). `read(ts)` aborts when a younger transaction has already written (`ts < wts`); `write(ts, v)` aborts when a younger transaction has already read or written (`ts < rts` or `ts < wts`). Otherwise the operation proceeds and updates the timestamps.

## Why

Locking is one way to get serializability; ordering by timestamp is another, with no waiting and no deadlock: a transaction that arrives "too late" is simply aborted and restarted with a new timestamp. It is the ancestor of MVCC's read timestamps, and a good place to see what *serial equivalent* means: the order of the timestamps.

## The contract

- `ToItem::new(initial)`; `read(ts) -> Result<i64, Abort>`; `write(ts, value) -> Result<(), Abort>`.
- `read(ts)`: `Err` if `ts < wts`, else `rts = max(rts, ts)` and the current value.
- `write(ts, v)`: `Err` if `ts < rts` or `ts < wts`, else `wts = ts` and the value is set.
- A refused operation changes nothing.

## Invariants

These must hold after every step, whatever the input:

- `rts` and `wts` never decrease.
- The current value is the value written by the accepted write with the largest timestamp (or the initial value).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Every accepted read returned the value of the accepted write with the largest timestamp not greater than the reader's.
- Running the accepted operations in timestamp order gives the same results (the serial equivalent).
- A transaction with a timestamp larger than all others is never aborted.

## Examples

Worked cases (the tests include them):

```text
write(5, 1); read(3) -> abort; read(7) -> 1; write(6, 2) -> abort (7 has read); write(8, 3) ok
```

## What the tests check

- The four abort cases and the accepted ones.
- Refused operations leave no trace.
- A property: accepted reads see the right version in a serial replay.

## Done when

All the `s4a_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Non-locking concurrency control](/cookbook/p/timestamp-ordering-occ): basic timestamp ordering with read and write timestamps per item.
- [Timestamp ordering and OCC](/cookbook/p/timestamp-ordering-and-occ): basic T/O rules and the Thomas write rule.
