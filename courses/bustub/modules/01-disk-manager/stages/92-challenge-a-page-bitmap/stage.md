A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`PageBitmap` in `src/storage/disk/page_bitmap.rs`: a bitmap that hands out page numbers `0..capacity`: `allocate` gives the **lowest** free page, `free` gives one back, and both refuse nonsense instead of corrupting the map.

## Why

The free-space map of a file is the other half of a disk manager. Handing out the lowest free page keeps the file compact, and a bitmap makes `allocate` cheap and `free` constant time. The real work is what happens on a double free: a bitmap that quietly accepts one will later hand the same page to two owners.

## The contract

- `allocate()` returns the lowest page that is free and marks it used, or `None` when all are used.
- `free(page)` marks it free; `Err(DoubleFree)` if it is already free, `Err(OutOfRange)` if `page >= capacity`; the map is unchanged on error.
- `is_allocated(page)` and `used()` report the state.

## Invariants

These must hold after every step, whatever the input:

- `used()` equals the number of allocated pages.
- No page is allocated twice without a `free` in between.
- Every allocated page is below the capacity.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- After `free(p)`, the next `allocate()` returns `min(p, any lower free page)`.
- Allocating until `None` then freeing everything restores the map.
- `allocate` order is always increasing when nothing has been freed.

## Examples

Worked cases (the tests include them):

```text
capacity 3: allocate -> 0, 1, 2, None
free(1); allocate -> 1
free(1) twice -> Err(DoubleFree)
free(3) -> Err(OutOfRange)
```

## What the tests check

- The order of allocation and reuse of the lowest page.
- Errors leave the map unchanged.
- A property against a `BTreeSet` of free pages.

## Done when

All the `s1a_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [A toy UNIX filesystem on a block device](/cookbook/p/toy-filesystem): a block bitmap allocator that reports a double free as corruption.
- [Bitset rank select roaring](/cookbook/p/bitset-rank-select-roaring): scanning u64 words with trailing_zeros to find a bit quickly.
