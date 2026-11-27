Presto's HyperLogLog stores each register compactly. The run length is the number of **trailing** zeros of the bits after the register bits (the whole remainder zero gives all `64 - b` of them). It is split in two: the low 4 bits go in a **dense** array (one small value per register), and the next 3 bits go in a sparse **overflow** map that has an entry only for registers whose run is 16 or more.

## The task

In `src/primer/hyperloglog.rs`, `HyperLogLogPresto` (the hash is given: integers hash to themselves, so a test can choose the bits). The public API is fixed (`new`, `add_elem`, `compute_cardinality`, `cardinality`, `dense_bucket`, `overflow_bucket_of`); the inside is yours.

- **`new(n_leading_bits)` has `2^n` dense registers, all zero, and no overflow entries**; none at all for a negative `n`.
- **`add_elem(&val)` keeps, in the register the top `n_leading_bits` bits of the hash select (0 if there are none), the longest run of trailing zeros seen in the other `64 - n` bits** (all of them if they are zero). A register is stored split: its low 4 bits in the dense array (`run & 0xF`), the next 3 in the overflow map (`(run >> 4) & 0x7`, no entry when they are zero). A register only ever grows, also when threads add at once. With no registers it does nothing.
- **`dense_bucket()` is a copy of the dense array; `overflow_bucket_of(idx)` is a register's overflow bits, or 0.**
- **`compute_cardinality()` is the same estimate as HyperLogLog** (your estimator function from stage 2), over the registers reassembled from the dense and overflow bits.

> [!ASIDE] The steps, if you would rather not work them out
> - `new(n_leading_bits)`: `2^n` dense registers, all zero, no overflow entries; none at all for a negative `n`.
> - `add_elem(&val)`: with no registers do nothing; the register is the top `n_leading_bits` bits of the hash (0 if there are none); the run is the **trailing** zeros of the other `64 - b` bits (all of them if the rest is zero); if the run exceeds the register's current value (dense bits plus overflow bits shifted left by 4) store `run & 0xF` in the dense array and `(run >> 4) & 0x7` in the overflow map (removing the entry if it is zero). Hold both locks across the read and the write.
> - `dense_bucket()` (a copy of the dense array), `overflow_bucket_of(idx)` (0 if none).
> - `compute_cardinality()`: the same estimate as HyperLogLog (your estimator function from stage 2), over the registers reassembled from the dense and overflow bits.

The tests: exact scenarios (a run of 18 trailing zeros is split into dense and overflow bits; zero has all the remaining bits as trailing zeros; a second register and the estimate for long runs; one register uses all 64 bits; threads adding in any order leave the same registers as one thread; a negative size and strings), and a property: **for random 64-bit hashes the registers rebuilt from the dense and overflow bits are the longest run of trailing zeros of the values that fall in each register**, the dense part is below 16 and the overflow part below 8.

## Your freedom

How you lock (both locks together is the intended design), and how you store the overflow bits (a map keyed by register, as given, or a second small array).

## The Rust toolbox

**`u64::trailing_zeros`.** One instruction, over the whole word: mask off the bits that are not data first (`x & ((1u64 << k) - 1)` keeps the low `k` bits; `1u64 << 64` overflows, so `k = 64` is a case of its own), and remember that `trailing_zeros` of 0 is 64, more than `k`.

**Two locks in one order.** Take `dense` then `overflow` everywhere; a different order in another function would deadlock.

**A sparse map.** `HashMap<u16, u8>`: `entry`, `insert` and `remove` keep only registers whose run needs more than 4 bits.

**Packing and unpacking.** A 7-bit value `v` splits into `v & 0xF` (the low four bits) and `(v >> 4) & 0x7` (the next three), and comes back as `low | (high << 4)`; cast each part to `u8` once it is masked.

## Design notes

**Where the numbers come from.** With `b = 1` and value `-9151314442816847872` = 2^63 + 2^56: the top bit is 1 (register 1) and bit 56 is the lowest set bit of the rest, so the run is 56: `56 = 0b11_1000`, dense `8`, overflow `3`. The estimate is `0.79402 × 4 / (2^-63 + 2^-56)`, which is 227 086 569 448 168 320 as a `u64`. Every such expected value in the tests follows from this definition; use them to check your reading of "trailing zeros".

**Check, then act, under one lock.** Raising a register is "read it, compare, write it". If the read and the write take the locks separately, two threads can both read 3, one writes 20, and the other then writes 5 over it: a larger run is lost for good. Take both locks (always dense, then overflow) before reading and release them after writing. The thread test in this stage adds the same values from eight threads and expects exactly the registers a single thread leaves.

**Half the estimate.** The run here is the *number* of trailing zeros, one less than the *position* of the first 1 that plain HyperLogLog counts. Every register's `2^-r` term is therefore twice as large, and the estimate about half as large. That is a property of the specified design, kept so that BusTub's exact values hold; the string test accepts ratios between 0.35 and 0.8.

**Why split.** Runs rarely reach 16; most registers need 4 bits, and only the rare big one needs its overflow bits. A dense array of 4-bit values plus a small map saves memory over 7 bits for every register.

## If this is new

- [D13 Matrix, bits & math](/t/d13-matrix-bits-math): masks and shifts, `trailing_zeros`.
- [C1 Threads & shared state](/t/c1-threads-shared-state): two locks held together, lock order.
- [S4 Maps & sets](/t/s4-maps-sets): a sparse `HashMap` next to a dense array.
- The optional *HyperLogLog and distinct counting* concept.

## Tests

- Splitting a run; zero; second register; one register; threads; negative size and strings.
- Property: dense and overflow bits equal the longest run per register.

## Hints

### Hold the locks across the whole update

A `register()` call that locks, returns, and unlocks followed by a store that locks again is two critical sections. The compare must be inside the one that writes.

### Mask before counting

Counting trailing zeros of the whole hash would include the register bits when the remainder is zero. Mask to `64 - b` bits first and cap at that.

### The all-zero remainder is 64 - b, not 64

`trailing_zeros` of 0 is 64; with `b = 1` the expected run is 63.

### Remove empty overflow entries

If the new run is 15 or less after having been 16 or more... it cannot be (runs only grow). But storing a run below 16 must not leave a stale overflow value from before.

## Performance

Dense storage is `m × 4 bits`; overflow entries exist only for registers with runs of 16 or more (probability about `2^-16` per hash), so for typical data the map is empty. The estimate sums over `m` registers rebuilt from two arrays: `O(m)`.

**Measure it.** Count overflow entries after adding a million values with `b = 12`: a handful.

## Experiment

Optional. Predict first, then run.

1. **Take the locks one at a time.** Which concurrent test fails, and how often?
2. **Store the whole run densely** (a byte per register). What does the sparse layout save for a million registers whose runs are mostly below 16?

## Other designs

- **Dense low bits plus sparse overflow (ours, Presto's):** about 4 bits per register for typical data.
- **A byte per register:** simplest, twice the memory.
- **Sparse then dense** (HyperLogLog++): a list of (register, rank) pairs until it is larger than the dense array.

## In BusTub

`hyperloglog_presto.h`: "`DENSE_BUCKET_SIZE = 4`, `OVERFLOW_BUCKET_SIZE = 3`", "`Testing framework will use the GetDenseBucket and GetOverflow function, hence SHOULD NOT be deleted. It's essential to use the dense_bucket_ data structure.`" and the tests `PrestoCase1`, `PrestoCase2`, `PrestoEdgeCase` whose expected values appear above.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::vector<std::bitset<4>>` and `std::bitset<3>` | `Vec<u8>` (0 to 15) and `u8` (0 to 7) |
| `bitset.to_ullong()` | the integer itself |
| `std::unordered_map<uint16_t, std::bitset<3>>` | `HashMap<u16, u8>` |

**Port rule:** small bitsets are small integers; the mask and shift are explicit.

## Learn more

- [Presto's HyperLogLog](https://engineering.fb.com/2018/12/13/data-infrastructure/hyperloglog/) · [`u64::trailing_zeros`](https://doc.rust-lang.org/std/primitive.u64.html#method.trailing_zeros)
