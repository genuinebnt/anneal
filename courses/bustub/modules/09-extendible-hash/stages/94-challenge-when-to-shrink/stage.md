A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/container/hash/directory_shrink.rs` decides whether an extendible hash directory can be halved and builds the halved directory. It looks right, and it allows a shrink that would send keys to the wrong bucket. Find the bug and fix it.

## Why

Shrinking is the mirror image of doubling and the less-tested half of the structure. A wrong shrink does not fail when it happens: the table loses keys after a later delete, much later.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||The directory can be halved only when **every** bucket's local depth is below the global depth; the code asked whether **any** was. `any` for `all` is one word.||

## The contract

- `can_shrink(local_depths, global_depth)` is true exactly when the global depth is above 0 and **every** bucket's local depth is below it.
- `shrink_slots(slots)` takes a directory whose two halves are identical (the upper half mirrors the lower) and returns the lower half, or `None` if the halves differ or the length is odd.

## Invariants

These must hold after every step, whatever the input:

- A directory that can shrink has two identical halves.
- Shrinking by one never makes a local depth exceed the new global depth.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding a bucket at full depth makes `can_shrink` false.
- If `can_shrink` is true, `shrink_slots` of a doubled directory returns the original.

## Examples

Worked cases (the tests include them):

```text
depths [1,1,0], global 2 -> true
depths [2,1], global 2 -> false
global 0 -> false
```

## What the tests check

- The rule for typical and edge cases.
- A property: `can_shrink` equals 'global above 0 and all depths below it'.
- `shrink_slots` is the inverse of doubling.

## Done when

All the `s2b_c5` tests pass, and you can say in one sentence what the bug was.
