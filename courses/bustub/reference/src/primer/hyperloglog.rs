//! **HyperLogLog** estimates how many *distinct* items a stream contains, in a few kilobytes. Hash every item to 64 bits. The first `b` bits pick
//! one of `m = 2^b` registers; in the other bits, the run of zeros ahead of the first (or after the last) 1 bit is rare: a run of `r` happens with
//! probability `2^-r`, so seeing run length `r` suggests about `2^r` distinct items went by. Each register keeps the longest run it has seen, and
//! the registers' estimates are combined with a harmonic mean, which resists one lucky register.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::sync::Mutex;

/// The bias correction of the estimator.
const CONSTANT: f64 = 0.79402;

/// A 64-bit hash with well-mixed bits: `DefaultHasher`, then a final avalanche so that every bit position is usable.
fn hash64<K: Hash>(val: &K) -> u64 {
    let mut hasher = DefaultHasher::new();
    val.hash(&mut hasher);
    let mut h = hasher.finish();
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

pub struct HyperLogLog<K> {
    // @begin 0d-02
    n_bits: i16,
    registers: Mutex<Vec<u8>>,
    cardinality: Mutex<u64>,
    _key: PhantomData<fn(&K)>,
    //~ _key: PhantomData<fn(&K)>,
    //~ // TODO(0d-02): your fields: the number of register bits, the registers, the last estimate (behind locks, so many threads can add)
    // @end
}

// @begin 0d-02
/// The estimate from the registers: `CONSTANT * m^2 / sum(2^-register)`, rounded down. No registers: 0.
fn estimate(registers: impl Iterator<Item = u32>, m: usize) -> u64 {
    if m == 0 {
        return 0;
    }
    let sum: f64 = registers.map(|r| 2f64.powi(-(r as i32))).sum();
    (CONSTANT * (m as f64) * (m as f64) / sum) as u64
}
//~ // TODO(0d-02): the estimator as a function of its own: the Presto-style sketch (stage 3) needs the same one
// @end

impl<K: Hash> HyperLogLog<K> {
    /// A sketch with `2^n_bits` registers. A negative `n_bits` gives no registers (cardinality 0 forever).
    pub fn new(n_bits: i16) -> HyperLogLog<K> {
        // @begin 0d-02
        let m = if n_bits < 0 { 0 } else { 1usize << n_bits };
        HyperLogLog { n_bits, registers: Mutex::new(vec![0; m]), cardinality: Mutex::new(0), _key: PhantomData }
        //~ todo!("0d-02: 2^n_bits registers, all zero (none for a negative n_bits), and the estimate 0")
        // @end
    }

    /// The last estimate (see `compute_cardinality`).
    pub fn cardinality(&self) -> u64 {
        // @begin 0d-02
        *self.cardinality.lock().unwrap()
        //~ todo!("0d-02: the stored estimate")
        // @end
    }

    /// The registers: the longest run seen in each.
    pub fn registers(&self) -> Vec<u8> {
        // @begin 0d-02
        self.registers.lock().unwrap().clone()
        //~ todo!("0d-02: a copy of the registers")
        // @end
    }

    /// The register a hash belongs to: its first `n_bits` bits.
    pub fn register_index(&self, hash: u64) -> usize {
        // @begin 0d-02
        if self.n_bits <= 0 {
            0
        } else {
            (hash >> (64 - self.n_bits as u32)) as usize
        }
        //~ todo!("0d-02: the top n_bits bits of the hash as a number (0 when n_bits is 0)")
        // @end
    }

    /// The run length of the hash: the **position of the leftmost 1** among the 64 - n_bits bits after the register bits, counted from 1.
    /// All zeros: `64 - n_bits + 1`.
    pub fn position_of_leftmost_one(&self, hash: u64) -> u8 {
        // @begin 0d-02
        let used = self.n_bits.max(0) as u32;
        let rest = if used >= 64 { 0 } else { hash << used };
        (rest.leading_zeros().min(64 - used) + 1) as u8
        //~ todo!("0d-02: drop the first n_bits bits (shift left), count the leading zeros of what is left (at most 64 - n_bits), plus one")
        // @end
    }

    /// Takes `val` into account.
    pub fn add_elem(&self, val: &K) {
        // @begin 0d-02
        let hash = hash64(val);
        let mut registers = self.registers.lock().unwrap();
        if registers.is_empty() {
            return;
        }
        let idx = self.register_index(hash);
        let rank = self.position_of_leftmost_one(hash);
        if rank > registers[idx] {
            registers[idx] = rank;
        }
        //~ todo!("0d-02: hash the value; the register is register_index; keep the larger of its value and position_of_leftmost_one (under the lock; do nothing if there are no registers)")
        // @end
    }

    /// Recomputes the estimate from the registers and stores it.
    pub fn compute_cardinality(&self) {
        // @begin 0d-02
        let registers = self.registers.lock().unwrap();
        let value = estimate(registers.iter().map(|r| *r as u32), registers.len());
        *self.cardinality.lock().unwrap() = value;
        //~ todo!("0d-02: CONSTANT * m * m / the sum of 2^-register over the registers, rounded down; 0 with no registers; store it")
        // @end
    }
}

/// HyperLogLog as Presto stores it: the run length is the number of **trailing** zeros of the bits after the register bits, and a
/// register keeps its low 4 bits in a dense array and the rest (up to 3 bits) in a sparse overflow map, since most registers stay small.
pub struct HyperLogLogPresto<K> {
    // @begin 0d-03
    n_leading_bits: i16,
    /// Low 4 bits of each register.
    dense: Mutex<Vec<u8>>,
    /// Bits 4 to 6 of the registers that need them.
    overflow: Mutex<HashMap<u16, u8>>,
    cardinality: Mutex<u64>,
    _key: PhantomData<fn(&K)>,
    //~ _key: PhantomData<fn(&K)>,
    //~ // TODO(0d-03): your fields: the dense low bits, the sparse overflow bits, the last estimate
    // @end
}

/// The hash the sketch uses for a key: integers hash to themselves, so a caller can choose the bits; anything else is hashed.
pub trait PrestoHash {
    fn presto_hash(&self) -> u64;
}

impl PrestoHash for i64 {
    fn presto_hash(&self) -> u64 {
        *self as u64
    }
}

impl PrestoHash for String {
    fn presto_hash(&self) -> u64 {
        hash64(self)
    }
}

impl<K: PrestoHash> HyperLogLogPresto<K> {
    /// A sketch with `2^n` registers, all zero (none for a negative `n`).
    pub fn new(n_leading_bits: i16) -> HyperLogLogPresto<K> {
        // @begin 0d-03
        let m = if n_leading_bits < 0 { 0 } else { 1usize << n_leading_bits };
        HyperLogLogPresto { n_leading_bits, dense: Mutex::new(vec![0; m]), overflow: Mutex::new(HashMap::new()), cardinality: Mutex::new(0), _key: PhantomData }
        //~ todo!("0d-03: 2^n registers, all zero, no overflow bits (none at all for a negative n)")
        // @end
    }

    /// The stored estimate, as of the last [`compute_cardinality`](Self::compute_cardinality).
    pub fn cardinality(&self) -> u64 {
        // @begin 0d-03
        *self.cardinality.lock().unwrap()
        //~ todo!("0d-03: the stored estimate")
        // @end
    }

    /// The dense array: the low 4 bits of every register.
    pub fn dense_bucket(&self) -> Vec<u8> {
        // @begin 0d-03
        self.dense.lock().unwrap().clone()
        //~ todo!("0d-03: a copy of the dense array")
        // @end
    }

    /// The overflow bits (bits 4 to 6) of register `idx`; 0 if it has none.
    pub fn overflow_bucket_of(&self, idx: u16) -> u8 {
        // @begin 0d-03
        self.overflow.lock().unwrap().get(&idx).copied().unwrap_or(0)
        //~ todo!("0d-03: the overflow bits stored for the register, or 0")
        // @end
    }

    // @begin 0d-03
    /// The whole value of register `idx`: dense bits plus overflow bits shifted in above them.
    fn register(&self, idx: usize) -> u32 {
        (self.dense.lock().unwrap()[idx] as u32) | ((self.overflow_bucket_of(idx as u16) as u32) << 4)
    }
    //~ // TODO(0d-03): helpers of your own (the whole value of a register)
    // @end

    /// Takes `val` into account. The register is the top `n` bits of its hash; the run is the number of trailing zeros of the other bits (all zero:
    /// `64 - n`). A register only grows.
    pub fn add_elem(&self, val: &K) {
        // @begin 0d-03
        // both locks are held from reading the register to writing it: a read and a write under separate locks could let a smaller
        // run overwrite a larger one that another thread stored in between
        let mut dense = self.dense.lock().unwrap();
        let mut overflow = self.overflow.lock().unwrap();
        let m = dense.len();
        if m == 0 {
            return;
        }
        let hash = val.presto_hash();
        let b = self.n_leading_bits as u32;
        let idx = if b == 0 { 0 } else { (hash >> (64 - b)) as usize };
        // the trailing zeros of the bits after the register bits; all zero: all 64 - b of them
        let rest_bits = 64 - b;
        let tz = if rest_bits == 64 { hash.trailing_zeros() } else { (hash & ((1u64 << rest_bits) - 1)).trailing_zeros().min(rest_bits) };
        let current = dense[idx] as u32 | ((overflow.get(&(idx as u16)).copied().unwrap_or(0) as u32) << 4);
        if tz > current {
            dense[idx] = (tz & 0xF) as u8;
            let high = ((tz >> 4) & 0x7) as u8;
            if high > 0 {
                overflow.insert(idx as u16, high);
            } else {
                overflow.remove(&(idx as u16));
            }
        }
        //~ todo!("0d-03: the register is the top n bits of the hash; its run is the trailing zeros of the other 64 - n bits (all zero: 64 - n); under both locks, held together: if larger than the register's value store the low 4 bits densely and bits 4 to 6 in the overflow map")
        // @end
    }

    /// Recomputes the estimate from the registers, dense and overflow parts joined, as [`HyperLogLog`] does, and stores it.
    pub fn compute_cardinality(&self) {
        // @begin 0d-03
        let m = self.dense.lock().unwrap().len();
        let value = estimate((0..m).map(|i| self.register(i)), m);
        *self.cardinality.lock().unwrap() = value;
        //~ todo!("0d-03: the same estimate as HyperLogLog, over the registers rebuilt from dense and overflow bits")
        // @end
    }
}
