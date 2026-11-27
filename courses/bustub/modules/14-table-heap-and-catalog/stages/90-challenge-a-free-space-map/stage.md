A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`FreeSpaceMap` in `src/storage/table/free_space_map.rs`: for each page of a heap file, how many bytes are free. `set(page, free)` records it; `find(need)` returns the **lowest-numbered** page with at least `need` free bytes, in `O(log n)`.

## Why

An insert into a heap must find a page with room. Scanning every page's header is how a table with a million pages makes every insert slow; PostgreSQL keeps a *free space map* for exactly this. A tree of maxima answers "leftmost page with at least `need`" with one descent, and updating a page is one path up.

## The contract

- `new(pages)` starts with every page at 0 free bytes.
- `set(page, free)` updates one page; `get(page)` reads it back.
- `find(need)` is the smallest page index whose free space is at least `need`, or `None`; `need == 0` is page 0 (if there is any page).

## Invariants

These must hold after every step, whatever the input:

- `get` returns what `set` last stored.
- `find` returns a page that really has enough room, and no lower page does.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Raising a page's free space can only move `find` results to a lower or equal page.
- `find(n)` is never lower than `find(m)` for `n >= m`... in the other direction: a larger need never finds an earlier page.
- With all pages at 0, `find(1)` is `None`.

## Examples

Worked cases (the tests include them):

```text
free [0, 50, 20, 80]: find(10) = 1, find(60) = 3, find(100) = None
set(0, 70) -> find(60) = 0
```

## What the tests check

- Finding after updates.
- Edge cases: no pages, need 0, need larger than any page.
- A property against a linear scan, and a large map finishing quickly.

## Done when

All the `s3c_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Slotted pages and a heap file](/cookbook/p/slotted-pages-heap-file): a heap file with a free-space map so inserts find a page without IO.
- [Segment fenwick](/cookbook/p/segment-fenwick): a monoid segment tree that gives range max in O(log n).
