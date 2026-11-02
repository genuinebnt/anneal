The heart of the set. **Insert** probes forward from the home bucket carrying a key; at each bucket: if it is free, put the carried key there; if it holds a key *closer to its own home* than the carried key is to its, **swap** (the carried key takes the bucket, the resident becomes the carried key and keeps probing). **Lookup** probes forward from the home until it finds the key or an empty bucket.

> [!CHECK] Capacity 8, keys a, b, c all have home bucket 2. You insert them in that order. Where do they end up, and what are their probe distances? Then d, whose home is 3, arrives: where does it go? And a lookup of d: how many buckets does it look at? Why is the *longest* probe distance kept small by taking a bucket from a key that is closer to its home?
> ||a in 2, b in 3, c in 4 (distances 0, 1, 2). d wants bucket 3, which holds b (distance 1, home 2): d is at distance 0 there, closer to home than b, so d does **not** take it; it moves on to bucket 4 (c, distance 2: d would be at distance 1, still closer to home: no), then bucket 5, empty: d goes there at distance 2. A lookup of d probes 3, 4, 5: three buckets. Taking from the rich (close to home) and giving to the poor evens out the distances, which keeps the worst probe short: the variance of probe lengths is what makes long lookups.||
>
> - When does the carried key swap with the resident?
> - Why `size == capacity` is checked before touching anything?
> - What does lookup do at a tombstone?

## The task

- `insert(&key) -> bool`: write lock; if an equal key is already there, replace it and return `true`; if the table is **full** (`size == capacity`) return `false` **before touching anything**; otherwise probe forward from the key's home bucket carrying `(key, its home)`: an empty bucket takes the carried key (size + 1, `true`); a live resident whose probe distance is **smaller** than the carried key's is swapped out (the carried key takes the bucket, the resident becomes the carried key and keeps probing); go on to the next bucket, wrapping.
- `contains(&key)` (read lock), `get_bucket(&key)`: the bucket that holds the key, or `bucket_count()` if it is absent.
- the lookup itself: probe from the home bucket for at most `capacity` steps, wrapping: an empty bucket ends the search (a key cannot be beyond one); a live equal key is the answer; tombstones and other keys are probed past.

The tests: exact scenarios (insert, duplicate, replace, lookup and statistics; strings; a single-bucket table holds one key; a key further from home takes the bucket of a closer one; insert fails only when every bucket is taken; probing wraps around the end; an absent key is reported absent in a crowded table; every key is found after many displacements), and a property: **after inserts alone the table is a proper Robin Hood table**: keys at distinct buckets, no gap between a key's home and its place, and along a run of taken buckets a key is never more than one step further from home than the key before it.

## Your freedom

How you probe (an index loop with a modulo, `cycle().skip(home)`), how you carry the displaced key, and whether lookup also stops early at a closer key (it must not, once tombstones exist: see the notes).

## The Rust toolbox

**`std::mem::replace`.** `let Slot::Live(displaced) = std::mem::replace(&mut slots[b], Slot::Live(carried)) else { unreachable!() };` swaps a bucket's contents and gives you the old one: the Robin Hood swap in one line.

**A loop with a bounded number of steps.** `for _ in 0..capacity { .. bucket = (bucket + 1) % capacity; }` cannot loop forever on a full table.

**`match` on a reference to the slot.** `match &table.slots[bucket] { Slot::Empty => .., Slot::Live(resident) => .., Slot::Tombstone => .. }` borrows, so decide what to do and only then mutate.

**Write lock then read lock.** `insert` takes `write()`, `contains` and `get_bucket` take `read()`; a private `find(&table, key)` serves both without locking again.

```rust
let Slot::Live(displaced) = std::mem::replace(&mut table.slots[bucket], Slot::Live(carried)) else { unreachable!() };
carried = displaced;
home = resident_home;
bucket = (bucket + 1) % self.capacity;
```

## Design notes

**Why `find` does not stop early.** The textbook rule "stop at a resident closer to home than I would be" is only valid if every key stays where Robin Hood insertion put it. Stage 3 adds tombstones, and a tombstone can be reused by a key with a short probe distance, sitting in front of keys that probed further. A lookup that stops there misses them. So lookups go to an empty bucket; displacement during insert still keeps the longest probe short.

**Why the full-table check comes first.** With fewer live keys than buckets, a free bucket exists, and the probe loop finds it. Without the check, a full table makes the loop swap residents around the whole circle and then fail, having moved keys and lost one.

**Replacing an equal key.** For integers it changes nothing visible; for a key type with identity beyond equality it stores the new value. That is what `Insert` returning `true` for an existing key means in BusTub's header ("`true when a key is inserted or an existing equal key is replaced`").

## If this is new

- [S4 Maps & sets](/t/s4-maps-sets): open addressing, what a `HashSet` does inside.
- S7 Smart pointers & interior mutability: `std::mem::replace`.
- [C1 Threads & shared state](/t/c1-threads-shared-state): `RwLock`.
- Y5 Testing & verification: structural invariants of a table, not just its contents.
- The optional *Robin Hood hashing and open addressing* concept.
- [F4 Hashing & purpose-built structures](/t/f4-hashing-structures): Hashing; Right structure for the job: open addressing, probe distance, tombstones.
- [S3 Vec & slices](/t/s3-vec-slices): Understand it: a table in a `Vec`; `std::mem::replace`.

## Tests

- Insert, replace, lookup, strings; displacement; full tables; wrap-around; absent keys; many displacements.
- Property: the Robin Hood invariants after inserts.

## Hints

### Compute the resident's home from the resident

Its probe distance at this bucket is `probe_distance(home_of(resident), bucket)`; you cannot reuse the carried key's home.

### Swap, then keep going

After a swap the displaced key continues from the **next** bucket with its own home.

### A test to write yourself

Insert keys `0..n` into a table of `n + 1` buckets and check every key is found; any lost key points at the swap logic.

## Performance

At load factor 0.75, linear probing alone can leave some keys far from home; Robin Hood keeps the longest probe near `log n` (measured below). The cost is the extra comparison and occasional swap on insert. Lookups of present keys are as fast as ever; lookups of absent keys here run until an empty bucket, which at load 0.75 is a handful of probes.

**Measure it.** Insert 200 000 **random** keys into a table of 262 144 and print `max_probe_distance()` (stage 3): about 18 here, close to `log2(200 000) = 17.6`, in about 5 ms; 200 000 lookups of present or absent keys take about 5 ms each (release mode). Do not measure with the keys `0..200 000`: multiplying consecutive integers by an odd constant and taking a power of two modulo is a permutation, so no two keys collide and the longest probe is 0.

## Experiment

Optional. Predict first, then run.

1. **Swap in the other direction.** Take the bucket from the *poorer* key. Which test and which property show it?
2. **Check fullness late.** Test `size == capacity` after displacing. What is left in the table when the insert fails?

## Other designs

- **Robin Hood linear probing (ours).**
- **Plain linear probing:** the same without displacement; longer worst probes.
- **Quadratic probing / double hashing:** less clustering, worse locality.
- **Cuckoo hashing:** two choices, constant-time lookups.
- **Swiss tables** (Abseil, Rust's `hashbrown`): groups of buckets with a byte of metadata, probed with SIMD.

## In BusTub

`robin_hood_hash_set.cpp`: "`@TODO(student) Insert with linear probing, Robin Hood displacement, and safe concurrent access.`" and "`@TODO(student) Probe through tombstones and stop only where correctness permits.`". The tests `CollisionAndRobinHoodDisplacementTest`, `FullTableTest`, `WraparoundTest`.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::swap(carried, bucket.key)` | `std::mem::replace(&mut slots[i], Slot::Live(carried))` |
| `std::unique_lock<std::shared_mutex>` | `self.table.write().unwrap()` |
| `size_t` arithmetic that can underflow silently | `usize` arithmetic that panics in debug builds if you subtract too much |

**Port rule:** swapping two values that sit in different places is `mem::replace` or `mem::swap`, never a copy.

## Learn more

- [`mem::replace`](https://doc.rust-lang.org/std/mem/fn.replace.html) · [Robin Hood hashing](https://codecapsule.com/2013/11/11/robin-hood-hashing/)
