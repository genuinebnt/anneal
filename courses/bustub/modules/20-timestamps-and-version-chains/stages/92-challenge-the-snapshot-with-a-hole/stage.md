A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/concurrency/ts_oracle.rs` hands out timestamps: `reserve()` gives the next commit timestamp to a transaction that is about to commit, `finish(ts)` says that commit's writes are all in place, and `begin()` gives a new reader its read timestamp. It looks right, and a reader can be given a timestamp that sees commit 6 but not commit 5. Find the bug and fix it.

## Why

A snapshot must be a **prefix** of the commit order: everything up to the read timestamp, nothing after. A snapshot with a hole in it shows a state that never existed.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`begin` used the largest *finished* commit timestamp. Commits finish out of order, so with 6 finished and 5 not, a reader at 6 saw 6 without 5. The read timestamp is the end of the unbroken run of finished commits.||

## The contract

- `reserve()` returns 1, 2, 3, ... in order; `finish(ts)` marks that commit as complete (finishing twice or an unknown ts is ignored).
- `begin()` returns the largest `t` such that **every** reserved timestamp `<= t` has finished (0 if none have, and `reserved` if all have).

## Invariants

These must hold after every step, whatever the input:

- For the read timestamp `t` returned by `begin`, every reserved `ts <= t` is finished.
- No finished commit `<= t` is missing from the snapshot: `t` is the largest such value.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `begin()` never decreases as commits finish.
- `begin()` is at most the smallest unfinished reserved timestamp minus one.
- Finishing in any order ends with `begin() == reserved`.

## Examples

Worked cases (the tests include them):

```text
reserve 1, 2, 3; finish 2 -> begin 0; finish 1 -> begin 2; finish 3 -> begin 3
```

## What the tests check

- Out-of-order finishes.
- Nothing reserved yet.
- A property against the definition.

## Done when

All the `s4a_c3` tests pass, and you can say in one sentence what the bug was.
