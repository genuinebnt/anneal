A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`MovingHeap` in `src/storage/table/moving_heap.rs`: a heap of fixed-size pages (`page_size` bytes of row data each) that stores byte-string rows under **row ids that never change**. `update(rid, row)` may need more room than the row's page has: the row then moves to another page, and the id still works.

## Why

Indexes store row ids, so an id must survive every update, however much the row grows. PostgreSQL leaves a forwarding pointer in the old slot (a HOT chain or a redirect), SQLite rewrites the b-tree cell, and most engines have an indirection. Doing it yourself shows what the invariants of a heap are: where bytes live, what points where, and when a page is full.

## The contract

- `insert(row)` returns a new `Rid` (a `u32`, increasing); it fails with `None` if the row is larger than a whole page.
- `get(rid)`, `delete(rid)` (false if not live) and `update(rid, row)` (false if not live or the row is larger than a page).
- `page_of(rid)` is the page currently holding the row; `used(page)` is the bytes of rows in that page, never more than `page_size`; `pages()` is the number of pages in use.

## Invariants

These must hold after every step, whatever the input:

- `used(page) <= page_size` for every page, always.
- `get(rid)` returns the bytes last stored under `rid`, wherever the row lives now.
- A deleted `rid` is never reused.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The heap behaves exactly like a map from `rid` to row, whatever the sizes.
- An update that fits in place leaves `page_of(rid)` alone.
- The sum of `used` over all pages is the total length of the live rows.

## Examples

Worked cases (the tests include them):

```text
page size 10: insert 6 bytes -> rid 0 on page 0; insert 6 bytes -> rid 1 on page 1; update rid 0 to 8 bytes -> still rid 0, on page 0 if it fits, else another page
```

## What the tests check

- Insert, get, delete.
- Updates that fit and updates that need to move.
- Oversized rows.
- A property against a map, with the page invariant.

## Done when

All the `s3c_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Slotted pages and a heap file](/cookbook/p/slotted-pages-heap-file): updates that relocate a tuple and leave a forward pointer so its rid survives.
