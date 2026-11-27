A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`split_keys` in `src/storage/index/node_split.rs` splits the sorted keys of a full node in two and chooses the separator for the parent. A leaf keeps every key (the separator is a copy of the right node's first key); an inner node moves the separator *up*, so it is in neither node. It looks right, and it has one bug. Find it and fix it.

## Why

A wrong split loses or duplicates a key, and the tree still looks plausible until a lookup for that key fails, much later. The difference between leaf and inner splits is the classic place for it.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||An inner node kept the separator in its right half. In an inner split the separator moves up and is in neither node, so the right half starts at `mid + 1`; only a leaf keeps a copy.||

## The contract

- The left node gets the first `(len + 1) / 2` keys.
- Leaf: separator is the right node's first key, which stays in the right node.
- Inner: separator is the first key that would have gone right, and it is in neither node.

## Invariants

These must hold after every step, whatever the input:

- Left keys are all `<` the separator.
- Leaf: right keys are all `>=` the separator; inner: right keys are all `>` it.
- Nothing is lost or invented.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Concatenating left, (separator for an inner node), right gives the original keys.
- A leaf's two nodes together hold as many keys as before; an inner node's hold one fewer.

## Examples

Worked cases (the tests include them):

```text
leaf [1,2,3,4] -> ([1,2], 3, [3,4])
inner [10,20,30,40,50] -> ([10,20,30], 40, [50])
```

## What the tests check

- Leaf and inner splits, odd and even counts.
- No key is lost or invented, for all sizes.
- A property over random sorted keys.

## Done when

All the `s2c_c2` tests pass, and you can say in one sentence what the bug was.
