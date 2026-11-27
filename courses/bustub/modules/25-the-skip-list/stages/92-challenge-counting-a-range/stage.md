A challenge: no walkthrough, no hints, no solution. It adds to something you built in this module, using what you learned there. It is extra practice and does not count towards the course.

## What to build

`range_count` and `floor` in `src/primer/skiplist_extras.rs`: using only the public view of **your** `SkipList<i32>` (`level(0)` is every key in order), `range_count(list, lo, hi)` counts the keys in `lo..=hi` and `floor(list, k)` returns the largest key `<= k`.

## Why

`BETWEEN`, `ORDER BY ... LIMIT` and predecessor queries are what ordered structures are *for*. A skip list can answer them by descending to the start and walking; with only the public view you get the same answers by binary search on the bottom level. Seeing the difference between what the structure *can* do fast (a descent) and what the interface lets you do (a walk) is the lesson.

## The contract

- `range_count(list, lo, hi)`: the number of keys `k` with `lo <= k <= hi` (0 if `hi < lo`).
- `floor(list, k)`: the largest stored key `<= k`, or `None`.

## Invariants

These must hold after every step, whatever the input:

- The results depend only on the keys stored, not on their heights.
- Both read only.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `range_count(l, a, b) == range_count(l, a, m) + range_count(l, m + 1, b)` for `a <= m < b`.
- `floor(l, k)` is in the list when it exists, and no key lies strictly between it and `k`.
- Inserting a key inside the range raises the count by one.

## Examples

Worked cases (the tests include them):

```text
keys {10, 20, 30}: range_count(15, 30) = 2; floor(25) = 20; floor(5) = None
```

## What the tests check

- Counting and floor on a small list.
- Empty ranges and the empty list.
- A property against a `BTreeSet`.

## Done when

All the `s0b_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Skip list](/cookbook/p/skiplist): predecessor search and range scans by walking level 0.
