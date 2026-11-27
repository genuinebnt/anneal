A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`RequestQueue` in `src/storage/disk/request_queue.rs`: the queue a smarter disk scheduler would sit on. `push(priority, item)` adds an item; `pop` returns the item with the **highest priority**, and among equal priorities the one that arrived **first**.

## Why

A scheduler that treats a user's read and a background flush alike makes the user wait. Priorities fix that, but a plain priority queue (a binary heap) does not keep arrival order among equals, and starving the oldest request of a class is a bug too. The tie-break is the exercise.

## The contract

- `push(priority, item)`: a higher number is more urgent.
- `pop()` removes and returns the most urgent item, the oldest among equals; `None` when empty.
- `len()`, `is_empty()`, and `peek_priority()` (the priority `pop` would return).

## Invariants

These must hold after every step, whatever the input:

- `len()` equals pushes minus pops.
- Every item pushed is popped exactly once.
- `peek_priority()` equals the priority of the item the next `pop` returns.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The pop order is the push order stably sorted by descending priority.
- Pushing an item of lower priority than everything in the queue never changes which item pops next.
- Two queues given the same pushes pop in the same order.

## Examples

Worked cases (the tests include them):

```text
push (1,a) (3,b) (3,c) (2,d); pop -> b, c, d, a
push (5,x); pop -> x; pop -> None
```

## What the tests check

- The order of pops for mixed priorities and ties.
- Empty queues.
- A property against a stable sort of everything pushed.

## Done when

All the `s1b_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [BinaryHeap as a min-heap with a deterministic tie-break](/cookbook/p/heap-tiebreak): BinaryHeap with a sequence number so equal priorities pop in arrival order.
