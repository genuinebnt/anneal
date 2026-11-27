A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`CuckooSet` in `src/container/hash/cuckoo_set.rs`: a hash set with **two tables** and two hash functions; a key is always in slot `h1(key)` of table 0 or slot `h2(key)` of table 1, so a lookup looks in at most two places. An insert into an occupied slot kicks the old key out to its other home, and so on; if that goes on too long the table grows and everything is placed again.

## Why

Chaining and open addressing can degrade to long probes on a bad day. Cuckoo hashing gives a hard bound of two probes for every lookup, which is why it appears in network hardware and in-memory stores, and the price is a more careful insert. The invariant is crisp, which makes it a good test target.

## The contract

- `insert(key)` returns false if the key is already present; otherwise places it, displacing keys as needed, and growing when displacement exceeds `max_kicks`.
- `contains(key)` looks only at the key's two homes; `remove(key)` clears it.
- `homes(key)` returns the two (table, slot) positions for the table's current size; `position(key)` says where the key actually is.

## Invariants

These must hold after every step, whatever the input:

- Every stored key is at one of its two homes, `homes(key)`.
- No key is stored twice; the number of keys is `len()`.
- The load factor never exceeds one half after an insert.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- `contains` is true exactly for the keys inserted and not removed.
- Growing never loses a key.
- Every lookup inspects at most two slots.

## Examples

Worked cases (the tests include them):

```text
insert 1..1000 -> all contained, every key at one of its two homes
remove a key -> not contained, the others still are
```

## What the tests check

- Insert, contains, remove; duplicates.
- Many inserts force displacement and growth.
- Every key stays at one of its homes.
- A property against a `HashSet`.

## Done when

All the `s2b_c4` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Cuckoo hashing](/cookbook/p/cuckoo-hashing): two-table cuckoo hashing with a kick limit and rebuild on cycles.
- [Hash tables](/cookbook/p/hash-tables): cuckoo hashing measured against the other hash schemes.
