A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`union` in `src/primer/skiplist_extras.rs`: build a **new** `SkipList<i32>` holding every key of two lists (`a` and `b` are left untouched), using your list's public `insert` and `level(0)`. A key in both appears once.

## Why

Set union is the simplest compound operation on ordered sets and the one every merge, replication catch-up and index rebuild depends on. With only `insert` available it costs `O((n + m) log (n + m))`; knowing that a sorted merge would be `O(n + m)` and why a skip list built by repeated insert cannot use it is part of choosing a structure.

## The contract

- `union(a, b)` returns a new list whose keys are the set union; sizes add up minus the common keys.
- `a` and `b` are not modified.

## Invariants

These must hold after every step, whatever the input:

- The result's `level(0)` is strictly increasing and equals the union of the keys.
- The result passes the structure's own invariants (heights between 1 and the maximum).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `union(a, b)` has the same keys as `union(b, a)`.
- `union(a, a)` has the keys of `a`.
- `union(a, empty)` has the keys of `a`.

## Examples

Worked cases (the tests include them):

```text
a {1, 3, 5}, b {3, 4}: union {1, 3, 4, 5}
```

## What the tests check

- Small unions, empty inputs.
- Inputs are untouched.
- A property against `BTreeSet`.

## Done when

All the `s0b_c5` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [The k-way merge family](/cookbook/p/kway-merge-family): merging sorted sequences in one pass, with two-way merge variants.
