A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/buffer/fifo_replacer.rs` is a complete first-in-first-out policy: the frame that has been in the pool longest is evicted first, however often it was used since. It looks right, and it has one bug. Find it and fix it.

## Why

When a structure keeps the same fact in two places, every change has to update both, and the bug is always in the path someone forgot. A property that compares *every* observable after *every* step finds it.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Removing a frame took it out of the evictable set but did not lower the evictable count, so `size()` disagreed with what `evict` could find. The count drops only when the frame really was evictable.||

## The contract

- The victim is the evictable frame that arrived first; using a frame again does not move it.
- A frame that is in use is skipped but keeps its place.
- `size` is the number of evictable frames, always.

## Invariants

These must hold after every step, whatever the input:

- `size()` equals the number of evictable frames after every call.
- Arrival order never changes except by removal.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Pinning and unpinning a frame never changes the order of the others.
- Saying "evictable" or "not evictable" twice is the same as once.

## Examples

Worked cases (the tests include them):

```text
arrive 0,1,2; use 0 ten times; evict -> 0
pin 0; evict -> 1; unpin 0; evict -> 0
```

## What the tests check

- Arrival order beats use; a skipped frame keeps its place.
- `size` through every kind of change.
- A property against a plain model.

## Done when

All the `s1c_c2` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A cache replacement policy zoo](/cookbook/p/cache-policy-zoo): FIFO and the other policies, each checked against the same traces.
- [A buffer pool](/cookbook/p/buffer-pool-clock): a buffer pool whose evictable set is checked against the frames after every operation.
