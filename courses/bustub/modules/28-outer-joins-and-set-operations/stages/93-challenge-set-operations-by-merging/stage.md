A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`merge_set_op` in `src/execution/merge_set_ops.rs`: `UNION`, `INTERSECT` and `EXCEPT`, with and without `ALL`, on two **sorted** streams of integers, returning a lazy iterator that is also sorted. Where stage 3j-05 hashed whole rows, this one walks both inputs once, counting each run of equal values.

## Why

A hash table of every distinct row is the right answer when the input is unordered and fits in memory. When both inputs are already sorted (they came from an index, or from an external sort that spilled to disk), the same operations need no table at all: look at the heads, take the smaller value, count its run on each side, and decide with the six formulas how many copies to emit. Memory is constant and the first row arrives before the inputs are read, which is what makes `LIMIT 10` on a billion-row union cheap.

## The contract

- `merge_set_op(left, right, op, all)` takes two iterators over `i64`, each in ascending order (duplicates adjacent), and returns an iterator.
- The output is in ascending order and has, for each value, the number of copies the operation and `all` give from the counts `na` and `nb` of that value.
- The returned iterator is lazy: asking for the first item reads a bounded number of items from the inputs, and it works on infinite inputs.
- It stores no more than a few numbers: no `Vec` of the inputs.

## Invariants

These must hold after every step, whatever the input:

- The output is sorted.
- An input is read at most once, and never beyond the run of the value being counted plus one item.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- For `all = false` the output equals the output with `all = true` with duplicates removed (for `Union`, `Intersect`, and `Except` after taking `min(na, 1)`-style logic, as in stage 3j-05).
- `Union` with `all = true` is the merge of the two inputs.

## Examples

Worked cases (the tests include them):

```text
Union all of [1, 1, 3] and [1, 2] -> [1, 1, 1, 2, 3]
Intersect all of [1, 1, 1, 2] and [1, 1, 3] -> [1, 1]
Except all of the same pair -> [1, 2]; Except of the same pair -> [2]
```

## What the tests check

- The six operations against a counting model on random sorted inputs.
- Laziness on infinite inputs.
- A bound on how many items are pulled to produce the first output.

## Done when

All the `s3j_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Relational model and algebra](/cookbook/p/relational-algebra-engine): bag semantics for set operations, the counts a merge must reproduce.
