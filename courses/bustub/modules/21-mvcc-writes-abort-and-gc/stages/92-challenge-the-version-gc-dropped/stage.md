A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`gc_chain` in `src/concurrency/version_gc.rs` removes the versions of one row that no transaction can read any more, given the **watermark**: the smallest read timestamp of any active transaction. It looks right, and after a collection a reader at the watermark gets the wrong value. Find the bug and fix it.

## Why

A version is garbage only if no reader, now or later, can pick it. Collecting one version too many gives a database that works until the one transaction holding back the watermark reads.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||It dropped every version at or below the watermark. The newest of those is the version a reader at the watermark sees, so it stays; only the versions older than it are garbage.||

## The contract

- `chain` lists `(commit_ts, value)` **newest first**. `gc_chain(chain, watermark)` keeps every version with `commit_ts > watermark` and the single newest version with `commit_ts <= watermark`; everything older is removed.
- Returns the number of versions removed.

## Invariants

These must hold after every step, whatever the input:

- For every read timestamp `t >= watermark`, reading the chain gives the same answer before and after.
- The chain is still newest first, with strictly decreasing timestamps.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- A second collection with the same watermark removes nothing.
- A higher watermark never keeps more versions.
- The chain never ends up empty if it was not empty.

## Examples

Worked cases (the tests include them):

```text
[(9,c),(5,b),(2,a)], watermark 6 -> keeps (9,c),(5,b); removes (2,a)
```

## What the tests check

- Watermark between, below and above the versions.
- Idempotence.
- A property: reads at or above the watermark are unchanged.

## Done when

All the `s4b_c3` tests pass, and you can say in one sentence what the bug was.
