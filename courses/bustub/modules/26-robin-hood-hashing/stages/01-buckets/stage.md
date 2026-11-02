Before any key is stored, the table needs its vocabulary: how many buckets, which bucket a key *wants* (its **home**), and how far from home a bucket is (the **probe distance**, counted forward and wrapping around the end of the table). Everything in the next stages is built from these.

## The task

In `src/primer/robin_hood_hash_set.rs` the public API is fixed (`new`, `home_bucket`, `probe_distance`, `capacity`, `bucket_count`, `size`, `load_factor`, `insert`, `contains`, `get_bucket`, `remove`, `clear`, `max_probe_distance`); how the buckets are stored is yours: a bucket is empty, **removed** (a tombstone: stage 3) or holds a key.

- `new(capacity)`: `Err(Invalid)` for capacity 0; otherwise `capacity` empty buckets and size 0, behind a reader-writer lock.
- `home_bucket(key)`: `key.robin_hood_hash() % capacity` (the hash trait is given: for integers it is `key × 0x9e3779b97f4a7c15` with wrapping arithmetic, BusTub's constant).
- `probe_distance(home, bucket)`: the number of steps forward from `home` to `bucket`, wrapping around the end of the table.
- `capacity()` and `bucket_count()`: the number of buckets; `size()`: live keys (read lock); `load_factor()`: `size / capacity`.

The tests: exact scenarios (a set has the capacity it was made with; capacity zero is an error; the home bucket is the hash modulo the capacity; the probe distance counts steps forward and wraps; the integer hash is the course's multiplicative one).

## Your freedom

How buckets are represented and what the table is made of; the three states of a bucket are the only thing the later stages need.

## The Rust toolbox

**A trait for the hash.** `trait RobinHoodHash { fn robin_hood_hash(&self) -> usize; }` (given) lets the set work for integers and strings without `std::hash`: tests can pick keys with a given home bucket.

**Modular arithmetic on `usize`.** `(bucket + capacity - home) % capacity` is the forward distance with wrap-around, without underflow.

**`Result` for a constructor.** `Err(Exception::new(ExceptionType::Invalid, ..))` for a capacity of zero (a modulo by zero would panic).

**An enum with data for a bucket.** `enum Slot<K> { Empty, Tombstone, Live(K) }`: the compiler makes you handle all three everywhere.

```rust
(bucket + self.capacity - home) % self.capacity      // no negative numbers with unsigned types
key.robin_hood_hash() % self.capacity
vec![Slot::Empty; capacity]                          // needs Slot: Clone
```

## Design notes

**Wraparound without subtraction underflow.** `bucket - home` fails when `bucket < home`. Adding `capacity` first keeps the arithmetic non-negative; the final `% capacity` handles the case where no wrap happened.

**Why a multiplicative hash.** Sequential integer keys would otherwise land in sequential buckets and look perfectly distributed; multiplying by a large odd constant scatters them. Tests pick keys *by home bucket* using exactly this hash, which is why it is pinned.

**Slots.** `Empty`, `Tombstone` and `Live(key)`: a three-state enum instead of a flag next to a possibly uninitialised key.

## If this is new

- S8 The core traits: a trait for the hash, `PartialEq` on keys.
- L7 Enums & pattern matching: `Slot<K>` and `match`.
- [D13 Matrix, bits & math](/t/d13-matrix-bits-math): modular arithmetic.
- The optional *Robin Hood hashing and open addressing* concept.
- [F4 Hashing & purpose-built structures](/t/f4-hashing-structures): Hashing; Right structure for the job: open addressing, probe distance, tombstones.

## Tests

- Capacity, home bucket, probe distance with wrap, the integer hash.

## Hints

### Cast carefully

The hash is a `usize`; keep the arithmetic in `usize` throughout.

### Test the wrap by hand

`probe_distance(6, 0)` in a table of 8 is 2 (buckets 7 and 0).

## Performance

All four functions are constant time. `% capacity` is a division; real tables use a power-of-two capacity and a mask, which BusTub's header mentions with its mixing function for strings.

**Measure it.** Compare `% capacity` with `& (capacity - 1)` over 100 million keys; the mask is several times faster.

## Experiment

Optional. Predict first, then run.

1. **Underflow.** Write the distance as `bucket - home` for a wrapped pair. What does debug mode do?
2. **A power-of-two capacity.** Replace `%` with `&`. Which keys change buckets?

## Other designs

- **Hash modulo capacity (ours):** any capacity.
- **Power-of-two capacity and a mask:** faster, needs a better hash.
- **Fibonacci hashing** (multiply, take the top bits): spreads sequential keys.
- **Separate chaining:** a list per bucket; no probe distance.

## In BusTub

`robin_hood_hash_set.h`: "`/** A fixed-capacity, concurrent hash set using Robin Hood open addressing. */`" and `RobinHoodHash<int>`: "`return static_cast<size_t>(static_cast<uint64_t>(static_cast<int64_t>(key)) * 0x9e3779b97f4a7c15ULL);`". The test helper `FindKeysWithHomeBucket` selects colliding keys with it.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `throw std::invalid_argument` in the constructor | `new(...) -> Result<Self, Exception>` |
| `std::vector<Bucket>` with a `state_` byte | `Vec<Slot<K>>` with an enum |

**Port rule:** a constructor that can reject its arguments returns `Result`.

## Learn more

- [Fibonacci hashing](https://probablydance.com/2018/06/16/fibonacci-hashing-the-optimization-that-the-world-forgot-or-a-better-alternative-to-integer-modulo/) · [`u64::wrapping_mul`](https://doc.rust-lang.org/std/primitive.u64.html#method.wrapping_mul)
