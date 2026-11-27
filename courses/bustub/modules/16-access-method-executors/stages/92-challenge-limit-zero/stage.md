A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`limit_offset` in `src/execution/limit_offset.rs` implements `LIMIT n OFFSET m` over a vector of rows (`limit: None` means no limit). It looks right, and `LIMIT 0` returns rows. Find the bug and fix it.

## Why

`LIMIT 0` is not rare: it is how clients ask for the *shape* of a result without the rows, and how an optimiser proves a query empty.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||A helper treated a limit of 0 as no limit, the C sentinel `0 = unlimited` coming back through a typed `Option`. Only `None` means no limit.||

## The contract

- `limit_offset(rows, limit, offset)` skips `offset` rows and then yields at most `limit` rows (all the rest when `limit` is `None`).
- An offset past the end gives no rows; `limit == Some(0)` gives no rows whatever the offset.

## Invariants

These must hold after every step, whatever the input:

- The output is a contiguous slice of the input, in order.
- Its length is `min(limit, len - offset)` (or `len - offset` without a limit), never negative.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `limit_offset(r, Some(0), m)` is empty for every `m`.
- `limit_offset(r, None, 0)` is the whole input.
- `limit_offset(r, Some(a), m)` is a prefix of `limit_offset(r, Some(b), m)` for `a <= b`.

## Examples

Worked cases (the tests include them):

```text
[1,2,3] LIMIT 0 -> []
[1,2,3] LIMIT 2 OFFSET 1 -> [2,3]
[1,2,3] OFFSET 5 -> []
```

## What the tests check

- The usual cases and the zero.
- Offsets at and past the end.
- A property against slicing.

## Done when

All the `s3e_c3` tests pass, and you can say in one sentence what the bug was.
