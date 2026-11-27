A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`VersionedPages` in `src/storage/disk/versioned_pages.rs`: a page store (pages are `u64` values here) where `snapshot()` returns an id that reads the store **as it was at that moment**, however much it is written afterwards.

## Why

Backups, consistent reads and MVCC all need a view of the data that does not move. The simplest way to give one is to never overwrite: a write adds a new version, a snapshot remembers how far the versions had got. This is the idea under module 4a's version chains, in a form small enough to see whole.

## The contract

- `write(page, value)` makes `value` the current value of the page.
- `read(page)` is the current value (`None` if never written).
- `snapshot()` returns a new snapshot id; `read_at(snapshot, page)` is the value the page had when the snapshot was taken (`None` if unwritten then, or if the snapshot was dropped).
- `drop_snapshot(id)` forgets it (false if unknown).

## Invariants

These must hold after every step, whatever the input:

- A snapshot never changes after it is taken.
- Snapshot ids are never reused.
- `read` equals what `read_at` would give for a snapshot taken right now.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Writing after a snapshot changes `read` and never `read_at` of existing snapshots.
- Two snapshots taken with no write in between read identically.
- Dropping one snapshot does not affect another.

## Examples

Worked cases (the tests include them):

```text
write(1, 10); s = snapshot(); write(1, 20) -> read(1) = 20, read_at(s, 1) = 10
read_at(s, 2) = None if page 2 was written only after the snapshot
```

## What the tests check

- A snapshot stays fixed while the store changes.
- Several snapshots at different times.
- Dropped and unknown snapshots.
- A property against a model that clones the whole map.

## Done when

All the `s1a_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [MVCC implementation choices](/cookbook/p/mvcc-version-storage-gc): append-only version storage, time travel, and readers at an old snapshot.
- [More primers](/cookbook/p/more-primers): a copy-on-write trie whose new versions share every untouched node.
