A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`PinTable` in `src/buffer/pin_table.rs`: the pin counts of a buffer pool, on their own: how many users hold each page right now. `pin` adds one, `unpin` removes one and **refuses** to go below zero.

## Why

A pin count that goes negative (or wraps) means a page is unpinned while someone still uses it, and the buffer pool evicts it from under them. Making the table refuse that, and report it, turns a silent corruption into an error at the exact call that was wrong.

## The contract

- `pin(page)` returns the new count.
- `unpin(page)` returns the new count, or `Err(NotPinned)` when the page has no pins.
- `count(page)`, `is_pinned(page)` and `pinned_pages()` (sorted) report the state.

## Invariants

These must hold after every step, whatever the input:

- No page has a count of 0 in the table: a page with no pins is simply absent.
- Every count is the number of `pin`s minus successful `unpin`s of that page.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `pin` then `unpin` returns the table to its previous state.
- `unpin` of an unpinned page changes nothing.
- The counts of different pages are independent.

## Examples

Worked cases (the tests include them):

```text
pin 3, pin 3 -> 2; unpin 3 -> 1; unpin 3 -> 0 (page 3 no longer pinned); unpin 3 -> Err
```

## What the tests check

- Counting up and down for several pages.
- Unpinning what is not pinned.
- A property against a map of counts.

## Done when

All the `s1f_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Buffer pool manager](/cookbook/p/buffer-pool-manager): pins, unpins and the race where a page is evicted while still in use.
- [A buffer pool](/cookbook/p/buffer-pool-clock): pin counts held by an RAII guard so a pinned page is never evicted.
