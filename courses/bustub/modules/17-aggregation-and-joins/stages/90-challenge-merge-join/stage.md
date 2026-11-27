A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`merge_join` in `src/execution/merge_join.rs`: join two lists of `(key, payload)` that are both **sorted by key** (duplicates allowed) and return the pairs of payloads whose keys are equal. A key that appears `a` times on the left and `b` times on the right produces `a * b` pairs. Output order: by left row, and within one left row by right row.

## Why

A merge join needs no hash table and no random access, which is why it wins when the inputs are already sorted (an index scan, the output of a sort) and why every database has one. The part that goes wrong is the duplicates: a naive two-pointer merge advances both sides on a match and misses the cross product.

## The contract

- Both inputs are non-decreasing in key.
- The output lists `(left_payload, right_payload)` for every pair of rows with equal keys, ordered as a nested loop over the left then the right would produce them.
- It runs in `O(|left| + |right| + |output|)`.

## Invariants

These must hold after every step, whatever the input:

- Every output pair has equal keys; every equal-key pair is output exactly once.
- The output length is the sum over keys of `count_left * count_right`.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The result equals a nested-loop join on the same inputs, in the same order.
- Swapping the inputs swaps each pair.
- Adding a row with a key the other side lacks changes nothing.

## Examples

Worked cases (the tests include them):

```text
L=[(1,a),(2,b),(2,c)], R=[(2,x),(2,y),(3,z)] -> (b,x) (b,y) (c,x) (c,y)
```

## What the tests check

- Matches with duplicates on one and both sides.
- Empty inputs and disjoint keys.
- A large merge finishing quickly.
- A property against a nested loop.

## Done when

All the `s3f_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Join algorithms compared](/cookbook/p/join-algorithms-code): sort-merge join emitting the cross product of each equal-key group.
- [Join algorithms](/cookbook/p/join-algorithms): sort-merge join and its cost next to nested loop and hash join.
