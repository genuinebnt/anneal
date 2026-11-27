A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`SlottedPage` in `src/storage/page/slotted_page.rs`: a fixed-size page that stores variable-length byte records. `insert` returns a slot number, `get` reads a record by slot, `delete` frees it. A record costs its length plus **4 bytes** of slot entry; the page has `size` bytes in all. Space freed by deletes must be reusable even when it is scattered.

## Why

Heap pages hold rows of different lengths, and deletes leave holes. A page that cannot reuse its holes fills up with garbage while claiming to be full. The design question is how the bytes are laid out; the contract is that an insert succeeds whenever the *total* free space is enough.

## The contract

- `new(size)`; `insert(record)` returns the lowest unused slot number holding the record, or `None` if `record.len() + 4 > free_space()`.
- `get(slot)` is the record, or `None` for a free or unknown slot; `delete(slot)` frees it (false if it was not live).
- `free_space()` is `size` minus the sum of `len + 4` over live records; `len()` the number of live records.

## Invariants

These must hold after every step, whatever the input:

- `free_space() + sum(live len + 4) == size`.
- Every live record reads back exactly the bytes inserted, whatever was inserted and deleted since.
- Slot numbers of live records are unique and stable: a record keeps its slot until it is deleted.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `insert` succeeds iff the record fits in the *total* free space, however fragmented.
- Deleting a record and inserting one of the same length succeeds.
- Delete then insert the same bytes returns the same (lowest free) slot.

## Examples

Worked cases (the tests include them):

```text
size 20: insert 4 bytes -> slot 0 (free 12); insert 4 -> slot 1 (free 4); insert 1 -> None
delete(0); insert 4 -> slot 0
```

## What the tests check

- Insert, get, delete and the space accounting.
- Fragmentation: freed holes are reusable.
- Slot reuse is lowest first.
- A property against a model of slots and a byte budget.

## Done when

All the `s2a_c3` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Slotted pages and a heap file](/cookbook/p/slotted-pages-heap-file): slotted pages with stable record ids, tombstoned slots and compaction.
- [Pages and tuples](/cookbook/p/pages-and-tuples): designing a slotted page: header, slot array and reclaiming dead bytes.
