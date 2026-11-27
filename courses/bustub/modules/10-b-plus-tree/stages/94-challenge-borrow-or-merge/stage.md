A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`after_delete` in `src/storage/index/rebalance.rs` decides what to do with a B+ tree node that has fallen below its minimum: borrow a key from a sibling, or merge with one. It looks right, and it merges when borrowing would have been enough. Find the bug and fix it.

## Why

Merging is the more expensive and more disruptive repair: it removes a node and a separator from the parent, and may cascade upwards. A tree that merges whenever it *can* instead of only when it *must* is correct, slow, and shaped differently from the one the tests describe.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||The merge checks came before the borrow checks, so a node merged whenever the sizes allowed it, even when a sibling had a key to spare. Borrow first; merge only when no sibling is above its minimum.||

## The contract

- Given the node's key count, the minimum and maximum, and the key counts of its left and right siblings (if any):
- Nothing if the node has at least `min` keys.
- Otherwise borrow from the **left** sibling if it has more than `min`, else from the **right** if it has more than `min`.
- Otherwise merge with the left sibling if it exists and the two fit in `max`, else with the right if it exists and they fit; `Underfull` if none applies (the root's case).

## Invariants

These must hold after every step, whatever the input:

- A merge result never exceeds `max` keys.
- A borrow leaves the sibling with at least `min` keys.
- The decision never merges while a borrow is possible.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Adding keys to a sibling can only change a merge into a borrow, never the reverse.
- With no siblings and an underfull node the answer is `Underfull`.

## Examples

Worked cases (the tests include them):

```text
min 2, max 4, node 1, left 3 -> BorrowLeft
node 1, left 2, right 3 -> BorrowRight
node 1, left 2, right 2 -> MergeLeft
```

## What the tests check

- Each branch.
- Borrow preferred over merge, left preferred over right.
- A property over all small counts.

## Done when

All the `s2c_c5` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Delete and tombstones](/cookbook/p/delete-and-tombstones): the four delete fix-ups: borrow left or right before merging.
- [Bplus tree](/cookbook/p/bplus-tree): delete that borrows from a sibling with spare keys, else merges.
