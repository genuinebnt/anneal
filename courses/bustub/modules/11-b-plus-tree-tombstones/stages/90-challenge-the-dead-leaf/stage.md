A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`LiveKeys` in `src/storage/index/live_iter.rs` walks a chain of leaves in which deleted keys are left as tombstones and yields the live keys in order. It looks right, and it has one bug. Find it and fix it.

## Why

With tombstones, the shape of the live data is no longer the shape of the leaves. Scans that end early do not fail: they return fewer rows.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||On reaching the next leaf the iterator looked only at that leaf's first slot and ended the scan if it was a tombstone, so a leaf starting with a tombstone (or entirely dead) cut the scan short. One loop that skips tombstones and exhausted leaves alike fixes it.||

## The contract

- Yield every live key, in order, skipping tombstones, across all leaves.
- The iterator stays finished once it is finished.

## Invariants

These must hold after every step, whatever the input:

- Keys are yielded in the order they appear.
- No tombstone is ever yielded; no live key is skipped.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The output equals flattening all leaves and filtering the live slots.
- Adding a leaf of only tombstones anywhere does not change the output.

## Examples

Worked cases (the tests include them):

```text
[[1, x, 3], [4, 5, x]] -> 1, 3, 4, 5
[[1], [x, x, x], [9]] -> 1, 9
```

## What the tests check

- Tombstones inside a leaf.
- A dead leaf in the middle, at the ends, several in a row.
- Empty input and empty leaves.
- A property against a flat filter.

## Done when

All the `s2d_c1` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Index iterator](/cookbook/p/index-iterator): range scans over the leaf chain that skip tombstoned entries.
