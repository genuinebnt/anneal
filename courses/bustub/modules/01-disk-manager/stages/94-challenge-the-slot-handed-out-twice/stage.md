A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/storage/disk/slot_allocator.rs` hands out slot numbers and takes them back through a free list. It looks right, and under some sequences it hands the same slot to two different owners. Find the bug and fix it.

## Why

A free list that is not defended against misuse eventually corrupts data: two pages that think they own the same place overwrite each other, and nothing reports it. The invariant (every live slot has one owner) is simple; what is hard is realising which call can break it.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`free` accepted any slot: one never handed out, or one already free. Freeing the same slot twice put it on the free list twice, so two later allocations received it. The fix refuses both.||

## The contract

- `allocate()` returns a slot not currently in use: a freed one if there is one, otherwise the next fresh one.
- `free(slot)` returns a slot that was in use; freeing a slot that is not in use (never allocated, or already freed) is ignored and returns false.
- `in_use()` is the number of slots handed out and not freed.

## Invariants

These must hold after every step, whatever the input:

- No slot is in use twice: `allocate` never returns a slot that is in use.
- `in_use()` equals allocations minus successful frees.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Freeing a slot twice has the same effect as freeing it once.
- Freeing a slot that was never allocated changes nothing.

## Examples

Worked cases (the tests include them):

```text
allocate -> 0, 1; free(0); free(0) (ignored); allocate -> 0; allocate -> 2 (not 0 again)
```

## What the tests check

- Allocation and reuse in the ordinary case.
- A double free followed by allocations.
- A free of a slot never handed out.
- A property: no slot is ever in use twice.

## Done when

All the `s1a_c5` tests pass, and you can say in one sentence what the bug was.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Slab generational](/cookbook/p/slab-generational): a free list through vacant slots, and why reused slots need generations.
- [A toy general-purpose allocator](/cookbook/p/size-class-allocator): free lists that refuse invalid and double frees.
