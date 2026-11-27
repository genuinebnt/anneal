A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`probe_distance` in `src/primer/probe_dist.rs` is how many slots forward from a key's home slot to where it sits, in a circular table. It is right for every key that does not wrap around the end of the table and wrong for those that do. Find the bug and fix it.

## Why

Almost every key in a half-empty table sits before the end, so the code works in every small test, and then one probe run crosses the end.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||For a key that wrapped (its slot before its home), it returned `home - slot`, the distance backwards. Circular distance is `(slot + capacity - home) % capacity`.||

## The contract

- `probe_distance(home, slot, capacity)` is the number of steps forward (wrapping from `capacity - 1` to 0) to get from `home` to `slot`; both are below `capacity`.
- The result is in `0..capacity`.

## Invariants

These must hold after every step, whatever the input:

- `(home + probe_distance) % capacity == slot`.
- `probe_distance(h, h, c) == 0`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Distances of consecutive slots from one home increase by one until the wrap, then continue.
- Distance from `h` to `s` plus distance from `s` to `h` is `capacity` (for `h != s`).
- A key at its home has distance 0 whatever the capacity.

## Examples

Worked cases (the tests include them):

```text
capacity 8: home 6, slot 1 -> 3
home 2, slot 5 -> 3
home 4, slot 4 -> 0
```

## What the tests check

- Distances before and after the wrap.
- All pairs for small capacities.
- A property: stepping forward from home reaches the slot after exactly that many steps.

## Done when

All the `s0c_c3` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Open addressing map](/cookbook/p/open-addressing-map): distance from home in a circular probing table.
