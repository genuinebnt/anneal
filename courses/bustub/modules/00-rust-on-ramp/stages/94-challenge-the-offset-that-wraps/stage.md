A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/rust_primer/offsets.rs` has the arithmetic of a paged file: the byte offset of a page, and how many pages a number of bytes needs. It looks right on every ordinary input, and it has one bug that only shows on very large ones. Find it and fix it.

## Why

Overflow bugs are invisible for years and then corrupt a file at the 4 GiB mark or the 16 EiB mark. In a storage engine the result is a write to the wrong place. Rust's `checked_*` operations make the intent explicit; the exercise is to notice where it was missing.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||Rounding up computed `bytes + page_size - 1`, which overflows near `u64::MAX`; `wrapping_add` hid the overflow by wrapping to a small number, so the page count came out tiny instead of failing. `div_ceil` rounds up without the addition.||

## The contract

- `page_offset(page, page_size)` is `page * page_size`, or `None` if it does not fit in a `u64` or `page_size` is 0.
- `pages_for(bytes, page_size)` is the number of pages that hold `bytes` bytes (rounded up), `None` if `page_size` is 0.

## Invariants

These must hold after every step, whatever the input:

- A result that is `Some` is exactly the mathematical answer (checked with 128-bit arithmetic).
- A result is never a wrapped value.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `pages_for(b, s) * s >= b` and `(pages_for(b, s) - 1) * s < b` for `b > 0`.
- `page_offset(p, s)` grows with `p` until it overflows, and then stays `None`.

## Examples

Worked cases (the tests include them):

```text
page_offset(3, 4096) = Some(12288)
page_offset(u64::MAX, 2) = None
pages_for(4097, 4096) = Some(2)
pages_for(u64::MAX, 4096) = Some(4503599627370496)
```

## What the tests check

- Ordinary values.
- Values at and past the overflow boundary.
- A page size of 0.
- A property against 128-bit arithmetic.

## Done when

All the `sr_c5` tests pass, and you can say in one sentence what the bug was.
