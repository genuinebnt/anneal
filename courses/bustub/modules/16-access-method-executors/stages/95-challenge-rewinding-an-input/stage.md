A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`Rewindable` in `src/execution/rewindable.rs`: wrap any iterator so that `mark()` remembers the current position and `reset()` goes back to it: the items seen since the mark are replayed, then the underlying iterator continues. It must keep **only** the items since the mark.

## Why

A nested loop join needs to read its inner side once per outer row, and an inner side that is an operator can only be read once. Re-executing it is expensive and sometimes wrong (a side-effecting or non-deterministic child); buffering the whole thing costs memory. Remembering what has been read since a mark is the middle road.

## The contract

- `next()` yields items in order. `mark()` records the position; items read after it are kept.
- `reset()` returns to the marked position: the next items are the buffered ones, then the underlying iterator's.
- `mark()` again drops what was buffered before the new mark; `buffered()` is the number of items held. Without a mark, nothing is kept (`buffered() == 0`).

## Invariants

These must hold after every step, whatever the input:

- The sequence read through `next()` and `reset()` is exactly the underlying sequence from the mark, repeated.
- The underlying iterator is pulled once per item, ever.
- `buffered()` never exceeds the number of items read since the mark.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `mark; read k; reset; read k` gives the same k items twice.
- A second `mark` after a `reset` costs nothing and keeps behaving.
- The union of what was read equals the underlying sequence's prefix.

## Examples

Worked cases (the tests include them):

```text
items 1..=5: mark; next=1, next=2; reset; next=1, next=2, next=3 (new from the source)
```

## What the tests check

- Mark, read, reset, replay, continue.
- Marks that move forward.
- Pull counts.
- A property against a vector model.

## Done when

All the `s3e_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Aggregation and nested-loop joins](/cookbook/p/aggregation-and-nested-loop-joins): why a nested loop join materializes its inner side: an executor cannot rescan.
- [Join algorithms compared](/cookbook/p/join-algorithms-code): merge join over duplicate groups, the job the mark/reset trick does.
