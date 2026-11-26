**Interview prompt. Time box: 60 minutes** (core in about 25). Parts build on each other; the tests are named
`core_`, `part2_`, `part3_` and `part4_`. The problem counts as solved when all parts pass.

Build a priority queue yourself. Do not use `std::collections::BinaryHeap`, `BTreeMap` or `BTreeSet`.

## Core

`PriorityQueue<T>` hands out the **greatest** item first.

- `new()` makes an empty queue ordered by `Ord`.
- `push(item)` adds an item. Equal items are all kept.
- `pop()` removes and returns the greatest item, or `None`.
- `peek()` shows what `pop` would return.
- `len()` and `is_empty()` (given).

`push` and `pop` must take `O(log n)` comparisons and move `O(log n)` items. The tests count comparisons, and one
test pushes 50 000 large items. `T` does not have to be `Clone`. The order among equal items is not specified.

## Part 2: build from a vector

`from_vec(items)` makes a queue from all the items using **at most `4n` comparisons**.

## Part 3: any ordering

`with_comparator(cmp)` makes an empty queue in which the item that `cmp` calls greatest comes out first. `cmp` may be
a closure that captures values.

## Part 4: the k largest of a stream

`top_k(items, k)` returns the `k` greatest items of an iterator, **greatest first** (all of them if there are fewer
than `k`). The stream may be far larger than memory: **never keep more than `k + 1` of its items alive** at once.
