**Interview prompt. Time box: 120 minutes** (core in about 30). The tests are named `core_`, `part2_`, `part3_` and
`part4_`. The problem counts as solved when all parts pass.

A metrics service keeps one number per time slot. Slots change while people ask about ranges. All ranges are
half-open, `l..r`, with `l <= r <= len`; indices passed in are always valid. Sums fit in `i64`.

**Every update and every query, in all parts, must take `O(log n)`.** The tests run 200 000 operations on 200 000 slots.

## Core

`PrefixSums`:

- `new(values)` (this may take `O(n log n)`);
- `add(i, delta)` adds `delta` to slot `i`;
- `prefix(end)` is the sum of slots `0..end`;
- `sum(l, r)` is the sum of slots `l..r` (given in terms of `prefix`).

## Part 2: when did we reach it?

`first_reaching(target)` returns the smallest index `i` such that slots `0..=i` add up to **at least** `target`, or
`None`. When it is called, every slot is zero or positive. A target of 0 or less is reached at index 0 if there is a slot.

## Part 3: any way of combining

`Combine<T>` answers range queries for any type with the given trait `Summary`: an `empty()` value and a
`join(left, right)` that is **associative but not necessarily commutative**.

- `new(values)`, `set(i, value)`, `query(l, r)` (the values in order, joined; `empty()` for an empty range).

## Part 4: change a whole range

`RangeAdder`: `new(values)`, `add_range(l, r, delta)` adds `delta` to **every** slot in `l..r`, and `sum(l, r)`.
