A challenge: no walkthrough, no hints, no solution. Solve it with what this module and the ones before it taught. It is extra practice and does not count towards the course.

## What to build

`ShiftSet` in `src/primer/shift_set.rs`: a fixed-capacity hash set of `u64` using **linear probing** and **backward-shift deletion**: removing a key moves the following keys of its probe run back one slot, so no tombstones are ever needed and lookups never get slower after deletes. `insert` fails with `Full` when no slot is free.

## Why

Tombstones keep a table correct and let it rot: after enough deletes every lookup probes through a graveyard. Backward-shift deletion repairs the run at the moment of the delete (the idea Robin Hood hashing makes cheap) so the table is always exactly as if the deleted key had never been inserted. The invariant to keep is reachability: a key must be findable from its home slot without crossing an empty one.

## The contract

- `new(capacity)`; `home(key)` is given. `insert(key)` returns `Ok(true)` if added, `Ok(false)` if present, `Err(Full)` when the table has no empty slot.
- `contains`, `len`. `remove(key)` returns whether it was there and shifts later members of its run back so that every remaining key is still reachable.
- `probe_len(key)` is the number of slots from the key's home to where it sits (0 when at home), `None` if absent.

## Invariants

These must hold after every step, whatever the input:

- Every stored key can be found by probing from its home slot without passing an empty slot.
- `len()` equals the number of stored keys; there are no tombstones (a slot is empty or holds a key).

## Relations between input and output

How the answer must change when the input changes. The property tests check these on random inputs:

- The set of keys equals a `HashSet`'s after any inserts and removes.
- After removing a key, the arrangement of the others equals the arrangement had it never been inserted *when it was inserted last* (checked through reachability).
- A table filled and then emptied has every slot empty again.

## Examples

Worked cases (the tests include them):

```text
capacity 8, keys with the same home h: insert a, b, c in order -> slots h, h+1, h+2; remove a -> b at h, c at h+1
```

## What the tests check

- Insert, find, remove, full.
- Wrap-around at the end of the table.
- A property against a `HashSet`, checking reachability after every step.

## Done when

All the `s0c_c1` tests pass.

## Further reading

In the cookbook. Some of these pages show a complete solution, so they are best read after your own attempt.

- [Open addressing map](/cookbook/p/open-addressing-map): the same backward-shift deletion with Robin Hood linear probing.
