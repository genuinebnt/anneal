HyperLogLog estimates how many **distinct** items a stream contains in a few kilobytes. Hash every item to 64 bits. The first `b` bits choose one of `2^b` **registers**; in the remaining bits, the **position of the leftmost 1** (the run of zeros before it, plus one) says how rare the hash is: a run of `r` happens with probability about `2^-r`, so seeing run length `r` suggests about `2^r` distinct items went by. Each register keeps the largest rank it has seen. The registers are the evidence; the **estimate** combines them with a harmonic mean, `constant × m² / Σ 2^(-register)` (it resists one lucky register), rounded down. A register nobody touched is 0 and contributes `2^0 = 1`.

> [!CHECK] A sketch has 4 registers (`b = 2`). You add the same value a thousand times, then a thousand different values. How do the registers change in each case, and what does that say about why the sketch counts *distinct* values? Then: the estimate of an empty sketch is not 0 but `0.79402 × 16 / 4 = 3` for 4 registers: why does the formula do that, and what does the course's sketch say for an empty one with no registers?
> ||The same value always hashes to the same register and the same rank: after the first add nothing changes (a register only ever grows to a rank it has seen). A thousand different values spread over the registers and push each up to about `log2(250)`, so the estimate grows with the number of distinct values. The formula on all-zero registers gives `constant × m` (each contributes 1): for 4 registers about 3: small-range bias that real HyperLogLog corrects with linear counting; this course keeps the raw estimate. A sketch with a negative size has no registers and estimates 0.||
>
> - Which bits pick the register, and which give the rank?
> - What does the estimate do when one register is far above the others?
> - Why does the harmonic mean resist outliers?

## The task

In `src/primer/hyperloglog.rs` the public API is fixed (`HyperLogLog::new`, `add_elem`, `compute_cardinality`, `cardinality`, `registers`, `register_index`, `position_of_leftmost_one`); the inside is yours (`hash64`, a 64-bit hash with well-mixed bits, and `CONSTANT = 0.79402` are given).

- `new(n_bits)`: `2^n_bits` registers, all zero, and the estimate 0; a **negative** `n_bits` gives no registers (and an estimate of 0 forever).
- `register_index(hash)`: the top `n_bits` bits of the hash as a number; 0 when `n_bits` is 0.
- `position_of_leftmost_one(hash)`: drop the first `n_bits` bits (shift left), count the leading zeros of what is left (at most `64 - n_bits`), plus one. An all-zero rest gives `64 - n_bits + 1`.
- `add_elem(&val)`: hash it; with no registers do nothing; otherwise raise the register at `register_index` to `position_of_leftmost_one` if that is larger, under the lock.
- `compute_cardinality()`: lock the registers, compute `CONSTANT × m² / Σ 2^(-register)` rounded down (0 with no registers), store it; `cardinality()` returns the stored value; `registers()` a copy. Keep the estimator as a function of its own: stage 3 needs it too.

The tests: exact scenarios (the register is the top bits of the hash; the run length is the position of the leftmost one after the register bits; with no register bits all 64 bits count; adding a value raises exactly one register to its rank; the same value twice changes nothing; a register only ever grows; a negative size ignores everything; adding is thread-safe; an empty sketch estimates the formula on all-zero registers; zero register bits is one register; the estimate is close for many distinct values and plausible for a small set), and two properties: **the registers depend only on the set of values** (any order, with every value repeated, gives the same registers and the same estimate; registers never go down); and **the estimate tracks the number of distinct values** across sizes (a band of ±40%, since the course's constant runs about 10% high).

## Your freedom

How you lock (one `Mutex` per field, one around both), how you hold the registers (`Vec<u8>`), and whether to compute the estimate on demand or keep it up to date.

## The Rust toolbox

**Bit tricks.** `hash >> (64 - b)` is the top `b` bits; `(hash << b).leading_zeros()` counts zeros after them; `u64::leading_zeros` is one instruction.

**`min` to cap a run.** `rest.leading_zeros().min(64 - b) + 1` handles an all-zero remainder (whose `leading_zeros` is 64).

**`f64::powi`.** `2f64.powi(-(r as i32))` is `2^-r`; sum over registers with `iter().map(..).sum::<f64>()`.

**Casting a float to an integer rounds toward zero.** `(x as u64)` truncates (and saturates): that is the 'rounded down' of the estimate.

**Lock before you read-modify-write.** `registers[idx] = registers[idx].max(rank)` under one lock; a read under one lock and a write under another can let a smaller rank overwrite a larger one.

```rust
(hash >> (64 - self.n_bits as u32)) as usize           // careful: a shift by 64 is an overflow
let rest = hash << used;                                // drop the register bits
(rest.leading_zeros().min(64 - used) + 1) as u8
```

```rust
let registers = self.registers.lock().unwrap();
let value = estimate(registers.iter().map(|r| *r as u32), registers.len());
*self.cardinality.lock().unwrap() = value;
```

## Design notes

**The edge shifts.** `hash >> 64` and `hash << 64` overflow for a `u64`; handle `n_bits == 0` for the first and `used >= 64` for the second explicitly.

**The `min` matters.** After shifting out `used` bits, the low `used` bits of `rest` are zeros that were not in the hash. An all-zero `rest` has 64 leading zeros, but only `64 - used` of them are real; the cap gives the "all zeros" rank.

**Registers store the maximum.** A smaller rank never lowers a register; that is also why two sketches merge by taking element-wise maxima.

**Why a harmonic mean.** Averaging the registers' estimates `2^r` arithmetically would let one register with `r = 40` dominate the result. The harmonic mean divides by the sum of `2^-r`: a big `r` adds almost nothing to the sum, so one outlier barely moves the estimate.

**Where the bias comes from.** The constant corrects the systematic over-estimate of the raw formula. This course uses the same constant as BusTub, 0.79402; the usual textbook value for large `m` is `0.7213/(1 + 1.079/m)`, about 10% lower. The tests therefore accept a ratio up to 1.3; the point is the shape of the estimator, not a tuned one.

**Small counts.** For fewer distinct values than registers, many registers are zero; real implementations switch to "linear counting" there. This exercise stops at the basic estimator, as BusTub does.

## If this is new

- [D13 Matrix, bits & math](/t/d13-matrix-bits-math): shifts, `leading_zeros`, the top bits of a hash.
- [C1 Threads & shared state](/t/c1-threads-shared-state): a `Mutex` around read-modify-write.
- S8 The core traits: `Hash` and a final mixing step.
- Y5 Testing & verification: estimator accuracy as a statistical test with a wide band.
- The optional *HyperLogLog and distinct counting* concept.

## Tests

- Register index and rank; growth; thread safety; empty and tiny sketches; the estimate for many distinct values.
- Properties: order and repeats do not matter; the estimate tracks the truth.

## Hints

### Test the edges first

`register_index(u64::MAX)` for `b = 3` is 7; `position_of_leftmost_one(1 << 63)` for `b = 0` is 1.

### Compare as numbers

Registers are `u8`; the rank can be up to 65. Do not accidentally truncate.

### Convert before powering

`r` is a `u8`; `-(r as i32)` avoids unsigned negation.

### Empty registers are 1, not 0

`2^0 = 1` per untouched register: that is why an empty sketch is not 0 (and why the small-count correction exists).

## Performance

`add_elem` is a hash, two shifts, one `leading_zeros` instruction and a compare under a lock: tens of nanoseconds. The sketch's size is `2^b` bytes however many items are added.

**Measure it.** Add 10 million values to a `b = 14` sketch: 16 KB, a second or less.

## Experiment

Optional. Predict first, then run.

1. **Count the trailing zeros instead.** Which exact tests fail, and does the estimate change on average? (This is stage 3's variant.)
2. **Fewer bits.** Use 4 register bits for a million values. How far off is the estimate, and how does the error scale with the register count?

## Other designs

- **HyperLogLog (ours):** about `1.04 / sqrt(m)` relative error.
- **LogLog / probabilistic counting:** older, less accurate.
- **HyperLogLog++** (Google): a 64-bit hash, bias correction and a sparse representation for small counts.
- **A hash set:** exact, memory proportional to distinct values.

## In BusTub

`hyperloglog.h`: "`Calculates Hash of a given value.`", `ComputeBinary(hash) -> std::bitset<64>`, `PositionOfLeftmostOne(bset)` ("`Function that computes leading zeros.`") and `AddElem`. The BusTub version builds a `bitset`; with a `u64` the same work is `leading_zeros`.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::bitset<64>` built from the hash, scanned bit by bit | `u64::leading_zeros()` |
| `std::mutex` guarding `registers_` | `Mutex<Vec<u8>>` |

**Port rule:** bit-by-bit loops over a `bitset` are `leading_zeros`, `trailing_zeros` and shifts on an integer.

## Learn more

- [`u64::leading_zeros`](https://doc.rust-lang.org/std/primitive.u64.html#method.leading_zeros) · [HyperLogLog (Wikipedia)](https://en.wikipedia.org/wiki/HyperLogLog)
- [The HyperLogLog paper](https://algo.inria.fr/flajolet/Publications/FlFuGaMe07.pdf) · [`f64::powi`](https://doc.rust-lang.org/std/primitive.f64.html#method.powi)
