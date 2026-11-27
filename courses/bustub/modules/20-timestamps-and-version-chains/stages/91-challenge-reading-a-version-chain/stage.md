A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`read_version` in `src/concurrency/version_read.rs`: a row's versions are kept **newest first** as `Version { writer, stamp, value }` where `stamp` is `Commit(ts)` for a committed version or `Pending` for one its writer has not committed; `value` is `Some(v)` or `None` (a delete). Return what a reader with a **read timestamp** and its own transaction id sees: the newest version that is committed with `ts <= read_ts`, or is its own pending one.

## Why

This is the core of snapshot isolation in twenty lines. A reader never blocks and never sees a half-finished write; it sees the newest committed state as of its timestamp, plus whatever it wrote itself. Everything else in MVCC (garbage collection, conflicts, validation) is built on knowing exactly which version this function picks.

## The contract

- Walk the chain from the newest version. A `Pending` version of another writer is skipped. A `Pending` version of the reader itself is visible. A `Commit(ts)` is visible iff `ts <= read_ts`.
- The first visible version decides: its `value` is the answer (`None` for a delete: the row does not exist for this reader).
- If no version is visible, the row does not exist for this reader.

## Invariants

These must hold after every step, whatever the input:

- The answer is the value of at most one version of the chain.
- A reader's own pending write always wins over committed versions.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Raising `read_ts` never shows an older version than before.
- Another transaction's pending versions never change a reader's answer.
- Committing a reader's own pending version at `ts <= read_ts` does not change its answer.

## Examples

Worked cases (the tests include them):

```text
chain [P(txn 7, 'c'), C(ts 5, 'b'), C(ts 2, 'a')]: reader ts 4 -> 'a'; ts 5 -> 'b'; txn 7 at ts 5 -> 'c'
chain [C(ts 5, delete), C(ts 2, 'a')]: ts 3 -> 'a'; ts 6 -> none
```

## What the tests check

- Committed, pending, own and deleted versions.
- Boundaries of the timestamp.
- A property against a scan.

## Done when

All the `s4a_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Project 4 · MVOCC](/cookbook/p/project-4-mvocc): reconstructing a tuple for a read timestamp, including own writes and deletes.
- [MVCC snapshot isolation](/cookbook/p/mvcc-snapshot-isolation): version chains and which version a snapshot sees.
