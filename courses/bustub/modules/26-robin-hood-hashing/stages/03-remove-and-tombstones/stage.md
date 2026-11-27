Emptying a bucket on removal would cut the probe chains of keys that went past it. Instead `remove` leaves a **tombstone**: lookups probe past it, inserts may reuse it. This stage adds `remove`, `clear` and the statistic `max_probe_distance`, and checks the whole set against Rust's `HashSet` on a random workload.

> [!CHECK] Capacity 8; three keys a, b, c share home bucket 2 and sit in buckets 2, 3 and 4. You remove b. Why does the table put a tombstone in bucket 3 instead of marking it empty? What would `find(c)` do otherwise?
> ||`find` stops at the first empty bucket, because a key cannot be beyond one. With bucket 3 empty, `find(c)` would probe 2, see a, probe 3, stop, and report "absent" although c is in bucket 4. A tombstone says "something was here, keep probing".||
>
> - What ends a search?
> - Which keys were placed by probing past bucket 3?
> - What may a later insert do with a tombstone?

## The task

In `src/primer/robin_hood_hash_set.rs`:

- **`remove(&key)` returns whether the key was there.** Afterwards it is absent, `size()` is one less, and **every other key is still found**: a removed key's bucket must not stop the lookup of a key placed past it.
- **`clear()` empties the set**; the capacity stays.
- **`max_probe_distance()` is the largest probe distance over live keys**, 0 for an empty table; removed keys do not count.
- **A removed key's bucket is free for inserts again**: a table whose keys have all been removed accepts as many inserts as a new one.

> [!ASIDE] The steps, if you would rather not work them out
> - `remove(&key) -> bool`: write lock; find the key; absent: `false`; otherwise leave a **tombstone** in its bucket (not an empty one: a lookup that stops at an empty bucket would never reach keys that were placed past it), size - 1, `true`.
> - `clear()`: write lock; every bucket empty; size 0; the capacity stays.
> - `max_probe_distance()`: read lock; the largest probe distance over **live** buckets; 0 for an empty table. Tombstones do not count.
> - inserts may **reuse** a tombstone (and the lookup still probes past tombstones).

The tests: exact scenarios (remove reports whether the key was there; a lookup continues past a tombstone; an insert reuses a tombstone; the maximum probe distance ignores tombstones; a key stays reachable when a tombstone in front of it is reused; random inserts and removes agree with a hash set; clear keeps the capacity; a table full of tombstones accepts inserts again), and a property: **any inserts, removes, lookups and clears on a small table against a `HashSet`** (an insert fails exactly when the table is full of other keys, every key found is at a bucket inside the table, tombstones pile up and are reused).

## Your freedom

How you remember a removed key (a bucket state); nothing else is specified.

## The Rust toolbox

**A tombstone is a state, not a hole.** A removed key's bucket gets a third state, neither empty nor live: a lookup walks past it, so the probe sequence of every key that went past it stays intact.

**Statistics with `filter_map`.** `seats.iter().filter_map(|s| match s { Seat::Taken(g) => Some(g.age), _ => None }).max().unwrap_or(0)` keeps one variant, maps it to a number and takes the largest, with 0 for none; `enumerate()` first if the number depends on the position.

**Reuse on insert.** In the probe loop an `Empty` or a `Tombstone` bucket takes the carried key; lookup, though, may only stop at `Empty`.

**`fill`.** `slots.fill(Slot::Empty)` (for `Clone` slots) resets the table in one call.

## Design notes

**The size counts live keys only.** Tombstones are neither in `size` nor free of cost: they lengthen probes. A workload that inserts and removes many different keys fills the table with tombstones; BusTub's header says nothing about rebuilding, so this exercise does not either (the full-table check uses `size`, and inserts reuse tombstones, so the table never blocks while a free bucket exists).

**The model check.** A random workload mixing inserts and removes, checked against `HashSet` after *every* operation (size and membership), is a cheap way to find the bug that hand-written cases miss. The early-exit bug of stage 2 is found by it within a few hundred operations.

**Clear while others read.** `clear` takes the write lock; a concurrent `contains` sees either the old or the empty table, never a half-cleared one.

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): deletion in open addressing.
- S6 Iterators: `filter_map`, `max`.
- Y5 Testing & verification: a hash set as the oracle for a long random history.
- The optional *Robin Hood hashing and open addressing* concept.
- [F4 Hashing & purpose-built structures](/t/f4-hashing-structures): Hashing; Right structure for the job: open addressing, probe distance, tombstones.
- L7 Enums & pattern matching: Enums & exhaustiveness: `Slot<K>`: empty, removed or live.

## Tests

- Remove, tombstone behaviour, reuse, statistics, clear.
- Property: a `HashSet` model with a capacity, tombstones piling up.

## Hints

### Remove only what `find` found

Do not mark a bucket as a tombstone unless `find` returned it as holding the key.

### Compare against the model every step

Assert `size()` and a handful of `contains` after each operation; the first failing step is the bug.

### Tombstones are not live

`max_probe_distance` and `size` must skip them; `insert`'s duplicate check must not match them.

## Performance

Remove is a `find` plus one store. Tombstones cost probe length: after heavy churn a table needs a rebuild (not required here). `max_probe_distance` is a full scan, `O(capacity)`; it is a diagnostic, not a hot path.

**Measure it.** Insert and remove 1 000 000 different keys through a table of 1 024 buckets; watch lookups of absent keys slow as tombstones accumulate.

## Experiment

Optional. Predict first, then run.

1. **Empty instead of a tombstone.** Which test and which property find the broken probe chains?
2. **Stop at a closer key in lookup.** The textbook lookup does; why is it wrong here, and which test shows it?

## Other designs

- **Tombstones (ours, BusTub's).**
- **Backward-shift deletion** (the textbook Robin Hood): move the following keys back one step; no tombstones, more work per delete.
- **Rebuild when too many tombstones accumulate.**

## In BusTub

`robin_hood_hash_set.cpp`: "`@TODO(student) Mark an occupied matching slot as a tombstone without breaking concurrent operations.`". Tests: `TombstoneDeletionTest`, `TombstoneReuseTest`, `MaxProbeDistanceTest`, `ClearTest`.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| a `state_` byte set to `kTombstone` | `Slot::Tombstone` |
| `std::unordered_set` as the model in a randomised test | `std::collections::HashSet` |

**Port rule:** a model-based test drives the structure and a trusted one with the same operations and compares after each.

## Learn more

- [Model-based testing](https://en.wikipedia.org/wiki/Model-based_testing) · [Tombstones in hash tables](https://en.wikipedia.org/wiki/Open_addressing#Disadvantages)
