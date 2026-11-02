A count-min sketch estimates how many times each item was seen, in a fixed amount of memory. It is a `depth × width` matrix of counters with one hash function per row. **Insert** adds one to the item's counter in every row; **count** returns the smallest of the item's counters. Collisions can only inflate a counter, so the estimate is **never too low**. Sketches are useful because they combine: two sketches with the same dimensions that counted two streams, added counter by counter, are the sketch of both streams. And since a sketch does not remember what it counted, finding the **top k** items takes a list of candidates to rank.

> [!CHECK] A sketch has width 4 and depth 2. Item `x` was inserted 5 times, `y` 3 times, and they collide in row 0 but not in row 1. What are `count(x)` and `count(y)`, and could either be lower than the truth? Why is the minimum over rows the right combination, and what would the *maximum* or the *sum* do?
> ||`count(x)` is 5 and `count(y)` is 3: row 0 holds 8 for both, row 1 holds 5 and 3, the minimum is the true count. Counters only ever go up by collisions, so every counter is at least the truth, and the minimum is the least inflated: never below the truth, as close as the luckiest row. The maximum would pick the *most* inflated counter (always a bad estimate); the sum would multiply the error by the depth.||
>
> - What does `merge` do with two sketches of different sizes?
> - Why can the sketch not list its own heaviest items?
> - Why is `insert` allowed to take `&self`?

## The task

In `src/primer/count_min_sketch.rs` the public API is fixed (`new`, `column`, `insert`, `count`, `clear`, `merge`, `top_k`); the inside (the counters) is yours; atomic counters let many threads insert without a lock.

- `CountMinSketch::new(width, depth)`: `Err(Invalid)` if either is zero; otherwise `width × depth` counters, all zero.
- `column(row, item)`: the column of the item in a row: a hash of the item seeded by the row, modulo `width`. Every row must hash differently (the same item lands in different columns of different rows) and always the same way (the same row and item: the same column).
- `insert(&item)`: add one to the item's counter in every row. `count(&item)`: the minimum of the item's counters over the rows.
- `clear()`: every counter to zero.
- `merge(&other)`: an error (`Invalid`) for different `width` or `depth` (the dimension check is given); otherwise add each of `other`'s counters to the same counter here.
- `top_k(k, candidates)`: estimate every candidate, sort by count descending with a **stable** sort (equal counts keep the candidates' order), keep the first `k`; at most `candidates.len()` pairs.

The tests: exact scenarios (a zero dimension is an error; counts of strings and of integers; width one makes everything collide and the minimum is the total; each row hashes differently but always the same way; clear; merge with and without collisions and with incompatible sketches; top-k orders, truncates and keeps up with a sketch that keeps counting), and three properties: **an estimate is never below the truth** (any size, counts only ever grow, no count exceeds the number of insertions); **merging the sketch of one stream into another counts exactly like the sketch of both streams** (and in either order), and `clear` empties; and **`top_k` is the best candidates in order** (at most `k`, each with the sketch's own count, sorted, ties in candidate order, nobody left out beats the last).

## Your freedom

How you store the counters (a `Vec<AtomicU32>` row after row, a `Vec<Vec<AtomicU32>>`, a boxed slice) and how you hash; `column` is the only function that fixes anything, and only by the two rules above.

## The Rust toolbox

**Atomics for counters.** `AtomicU32::fetch_add(1, Ordering::Relaxed)` increments without a lock; `Relaxed` is enough because no other memory depends on the counter's value.

**`min` of an iterator.** `(0..depth).map(|row| cell(row, item).load(Relaxed)).min().unwrap_or(0)`.

**Seeding a hasher.** `let mut h = DefaultHasher::new(); (SEED, row).hash(&mut h); item.hash(&mut h); h.finish()` gives a different hash function for each row from one hasher type.

**A generic key bound.** `impl<K: Hash> CountMinSketch<K>` and `PhantomData<fn(&K)>` make the type depend on `K` without owning one (so the sketch is `Send + Sync` for any `K`).

**Stable sort.** `sort_by(|a, b| b.1.cmp(&a.1))` is stable: equal counts keep the order of the candidates.

```rust
self.cell(row, item).fetch_add(1, Ordering::Relaxed);                 // atomic increment through &self
(0..self.depth).map(|row| self.cell(row, item).load(Ordering::Relaxed)).min().unwrap_or(0)
(0..width as usize * depth as usize).map(|_| AtomicU32::new(0)).collect()
```

```rust
for (mine, theirs) in self.counters.iter().zip(&other.counters) {
    mine.fetch_add(theirs.load(Ordering::Relaxed), Ordering::Relaxed);
}
counted.sort_by(|a, b| b.1.cmp(&a.1));   // stable
counted.truncate(k as usize);
```

## Design notes

**Why `&self`.** Counters are atomics, so the methods take a shared reference and many threads can call `insert` at once with no lock; `fetch_add` makes each increment indivisible. `Relaxed` ordering suffices because nothing else is published through the counters; threads joined before `count` is read see the final totals.

**Why the minimum, not the average.** Every counter holds the true count plus noise that is never negative. The smallest has the least noise. The average would include the noisiest rows.

**Matrix layout.** A flat vector indexed `row × width + column` keeps rows contiguous. A `Vec<Vec<..>>` would work but costs an extra indirection.

**Merge needs the same hashes.** An item's columns depend on `width` and on the row's seed. Same dimensions, same hash functions, same columns: the counters line up. Different dimensions would add unrelated counters; hence the error.

**Merging a sketch with itself.** `a.merge(&a)` doubles every counter; with atomics that is fine in Rust (both `&` are shared).

**Why stable.** The tests list candidates in an order and expect ties to keep it; Rust's `sort_by` is stable, `sort_unstable_by` is not.

## If this is new

- [C3 Atomics & lock-free](/t/c3-atomics-lock-free): `fetch_add`, `load`, `Ordering::Relaxed`.
- S8 The core traits: `Hash`, `Hasher`.
- [L5 Generics & associated types](/t/l5-generics): `PhantomData`, bounds on an `impl`.
- Y5 Testing & verification: 'never below the truth' as a property.
- The optional *count-min sketch and frequency estimation* concept.
- [F4 Hashing & purpose-built structures](/t/f4-hashing-structures): Hashing: independent hash functions per row: a seeded hasher.

## Tests

- Dimensions; counts; collisions; row hashing; clear; merge; top-k.
- Properties: never below the truth; merge equals the combined stream; top-k ordering.

## Hints

### Index with the row

The same item lands in a different column in every row; compute the column per row.

### Do not forget the empty case

`depth` is at least 1 after validation, so `min()` always has a value, but keep the `unwrap_or(0)` to satisfy the type.

### Check dimensions first

Return the error before touching any counter, so a failed merge leaves `self` unchanged.

### Candidates with the same count

Sort by count only; do not add the item as a secondary key (items need not be ordered).

## Performance

Insert and count cost `depth` hash computations and `depth` memory accesses, independent of how many items were seen. Memory is `4 × width × depth` bytes. An atomic increment is a few nanoseconds uncontended; contended counters (a very hot item) serialise on the cache line.

**Measure it.** Eight threads inserting one hot item: slower per operation than eight threads inserting different items.

## Experiment

Optional. Predict first, then run.

1. **Maximum instead of minimum.** Which property fails first?
2. **One row.** With `depth = 1` how often is the estimate exact on a stream of 50 distinct items and width 100?
3. **Replace `merge` by overwrite.** Which test notices?

## Other designs

- **Count-min (ours):** never underestimates; error grows with the stream.
- **Count sketch:** signed counters, unbiased, error symmetric.
- **Space-saving / Misra-Gries:** keeps the `k` heaviest items exactly, no candidates needed.
- **A hash map of counts:** exact, memory proportional to distinct items.

## In BusTub

`count_min_sketch.h`: "`Constructs a count-min sketch with specified dimensions`" (`@param width Number of buckets`, `@param depth Number of hash functions`) and the constructor's `@throws std::invalid_argument if width or depth are zero.`. The hash is fixed by the starter ("`PLEASE DO NOT MODIFY THE FOLLOWING`"); the course gives `column` the same role.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::vector<std::vector<std::atomic<uint32_t>>>` | `Vec<AtomicU32>` of `depth × width` |
| `std::function<size_t(const KeyType &)>` per row | one `column(row, item)` method with the row as a seed |
| `throw std::invalid_argument` | `Err(Exception)` |

**Port rule:** `atomic<T>` fields mutated through `const` references are `AtomicT` fields mutated through `&self`.

## Learn more

- [`AtomicU32`](https://doc.rust-lang.org/std/sync/atomic/index.html) · [The paper](https://dsf.berkeley.edu/cs286/papers/countmin-latin2004.pdf)
- [`slice::sort_by`](https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by) · [Sketch mergeability](https://arxiv.org/abs/1103.0510)
