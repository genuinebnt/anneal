A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`RangeLocks` in `src/concurrency/range_locks.rs`: a table of locks on **half-open key ranges** `[lo, hi)` in `Shared` or `Exclusive` mode. `try_lock(txn, lo, hi, mode)` grants the lock unless another transaction holds an **overlapping** range in an incompatible mode (shared with shared is fine); `unlock_all(txn)` frees everything a transaction holds.

## Why

A row lock cannot stop a *phantom*: a scan of `age BETWEEN 30 AND 40` under row locks does not prevent another transaction from inserting age 35. Locking the **range** does, including the keys that do not exist yet. This is the idea behind next-key locks in InnoDB and gap locks everywhere, reduced to its core: intervals and compatibility.

## The contract

- Ranges are `[lo, hi)`; an empty range (`lo >= hi`) is refused with `Err(EmptyRange)`.
- Two ranges overlap iff `a.lo < b.hi && b.lo < a.hi`. A request conflicts with a lock of **another** transaction that overlaps and is not Shared-vs-Shared.
- `try_lock` returns `Ok(true)` (granted), `Ok(false)` (conflict, nothing changes). A transaction never conflicts with its own locks.

## Invariants

These must hold after every step, whatever the input:

- Granted locks of different transactions never overlap in an incompatible way.
- `unlock_all` removes exactly that transaction's locks.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Shrinking a requested range never turns a grant into a refusal.
- Two shared requests are granted regardless of overlap; an exclusive request overlapping any other transaction's lock is refused.
- Adjacent ranges `[0,5)` and `[5,9)` do not overlap.

## Examples

Worked cases (the tests include them):

```text
T1 S[0,10); T2 S[5,15) ok; T3 X[9,12) refused; T3 X[10,12) refused (T2 holds up to 15); unlock T2; T3 X[10,12) ok
```

## What the tests check

- Overlap and adjacency.
- Compatible and incompatible modes.
- Own locks.
- A property against a brute-force point check.

## Done when

All the `s4d_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Timestamp ordering and OCC](/cookbook/p/timestamp-ordering-and-occ): phantoms, and key-range and gap locks that prevent them.
