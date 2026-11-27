A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`LinearHashSet` in `src/container/hash/linear_hash_set.rs`: a hash set that grows **one bucket at a time**, in a fixed order, with no directory at all. The table has `round_size` buckets at the start of a round; buckets already split use hash mod `2 * round_size`; a split takes the next bucket in order, adds a bucket at the end, and moves the keys that now hash there.

## Why

Extendible hashing doubles a directory and splits only the full bucket; linear hashing never doubles anything and splits buckets in order, full or not. It trades some overflow for a table that grows smoothly and a lookup that needs no directory, and it is what some real systems use.

## The contract

- `insert` adds a key (false if present) and splits **one** bucket when the load (keys per bucket) goes over `max_load`.
- `bucket_of(key)` is `hash mod round_size`, or `hash mod 2 * round_size` for buckets already split this round.
- `remove` and `contains` use the same rule; buckets are never merged.

## Invariants

These must hold after every step, whatever the input:

- Every key is in the bucket `bucket_of` names for it, after every operation.
- After an insert, the load is at most `max_load` (up to one split's worth).
- `bucket_count` only ever grows, by at most one per insert.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Inserting a key and then removing it leaves the set as it was (except the buckets).
- The set of keys is the same as a `HashSet`'s after any operations.
- A split moves only the keys of one bucket.

## Examples

Worked cases (the tests include them):

```text
1 bucket, max_load 1.0: insert 1 -> 1 bucket; insert 2 -> 2 buckets
500 inserts at max_load 2.0 -> more than 2 buckets, all keys found
```

## What the tests check

- Keys are found after any number of splits.
- One split per insert at most, and the load bound.
- Duplicates and removals.
- A property against a `HashSet`.

## Done when

All the `s2b_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Hash indexes](/cookbook/p/hash-and-bitmap-indexes): linear hashing with a split pointer and no directory.
- [Hash tables](/cookbook/p/hash-tables): linear hashing measured against extendible, cuckoo and probing tables.
