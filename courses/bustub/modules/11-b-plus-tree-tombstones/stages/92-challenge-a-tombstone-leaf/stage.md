A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`TombstoneLeaf` in `src/storage/index/tombstone_leaf.rs`: one leaf as a sorted list of slots where `remove` only **marks** a slot dead (a tombstone) and `insert` of a key that has a tombstone **revives** it in place. `should_compact()` says when more than half the slots are tombstones, and `compact()` drops them.

## Why

Tombstones make deletes cheap and concurrency-friendly, and their cost is paid by every scan that has to step over them. The two details that go wrong are inserting a key whose tombstone is still there (a duplicate slot, so the key appears twice or is never found) and compacting without preserving order.

## The contract

- `insert(key, value)` returns the old live value if the key was live; revives a tombstone in place (no new slot); otherwise adds a slot in order.
- `remove(key)` marks the slot dead and returns the value it had, `None` if the key was not live.
- `get(key)` is the live value or `None`; `live_len()`, `slots()` (total including tombstones), `tombstones()`.
- `should_compact()` is `tombstones * 2 > slots`; `compact()` removes every tombstone slot.

## Invariants

These must hold after every step, whatever the input:

- Keys in the slots are strictly increasing: a key has at most one slot.
- `slots() == live_len() + tombstones()`.
- `get` never returns a removed key's value.

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- Removing a key and inserting it again does not grow `slots()`.
- `compact()` changes no `get` answer and no ordering of live keys.
- After `compact()`, `tombstones() == 0` and `should_compact()` is false.

## Examples

Worked cases (the tests include them):

```text
insert 1,2,3; remove 2 -> slots 3, live 2, tombstones 1; insert 2 again -> slots 3, live 3
```

## What the tests check

- Insert, remove, revive; ordering.
- The compaction rule and compaction.
- A property against a `BTreeMap`, with the slot invariants.

## Done when

All the `s2d_c2` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Delete and tombstones](/cookbook/p/delete-and-tombstones): BusTub's leaf tombstone buffer, where re-inserting a tombstoned key revives it.
