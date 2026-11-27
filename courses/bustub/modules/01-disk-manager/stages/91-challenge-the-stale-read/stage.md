A challenge: no walkthrough, no hints, no solution. It is extra practice and does not count towards the course.

## What to fix

`src/storage/disk/cached_disk.rs` is a complete write-through page cache in front of any disk. It is meant to be invisible: to everyone using it, it behaves exactly like the disk underneath. It looks right, and it has one bug. Find it and fix it.

## Why

A cache is a second copy of the truth, and every second copy can go stale. Cache bugs are rarely in the code that reads or writes; they are in the operations that someone forgot to tell the cache about. The same lesson returns with the buffer pool in module 1f.

> [!CHECK] Once the tests pass: what was the bug, in one sentence, and why did ordinary inputs not show it?
> ||`delete_page` passed the delete to the disk underneath but left the page in the cache, so a read after the delete returned the cached bytes of a page that no longer existed. Every operation that changes the truth has to tell the cache.||

## The contract

- A `CachedDisk` answers every read, write and delete exactly as the disk underneath would, whatever its capacity.
- It writes through: the disk underneath is always current.

## Invariants

These must hold after every step, whatever the input:

- Every page in the cache equals that page on the disk underneath.
- The cache never holds more than its capacity.
- A deleted page is in neither.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Replacing the cache by the plain disk never changes any answer.
- A bigger cache never changes answers, only the number of reads that reach the disk.

## Examples

Worked cases (the tests include them):

```text
write 7; read 7; delete 7; read 7 -> zeros
capacity 2, write pages 0..5, read all -> all correct
```

## What the tests check

- A page just written is served from memory.
- A full cache forgets old pages without losing data.
- A deleted page reads as zeros, and cache and disk agree.
- A property against a plain map of pages.

## Done when

All the `s1a_c2` tests pass, and you can say in one sentence what the bug was.
