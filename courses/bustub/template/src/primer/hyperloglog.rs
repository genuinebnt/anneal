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
    _key: PhantomData<fn(&K)>,
    // TODO(0d-02): your fields: the number of register bits, the registers, the last estimate (behind locks, so many threads can add)
}

// TODO(0d-02): the estimator as a function of its own: the Presto-style sketch (stage 3) needs the same one

impl<K: Hash> HyperLogLog<K> {
    /// A sketch with `2^n_bits` registers. A negative `n_bits` gives no registers (cardinality 0 forever).
    pub fn new(n_bits: i16) -> HyperLogLog<K> {
        todo!("0d-02: 2^n_bits registers, all zero (none for a negative n_bits), and the estimate 0")
    }

    /// The last estimate (see `compute_cardinality`).
    pub fn cardinality(&self) -> u64 {
        todo!("0d-02: the stored estimate")
    }

    /// The registers: the longest run seen in each.
    pub fn registers(&self) -> Vec<u8> {
        todo!("0d-02: a copy of the registers")
    }

    /// The register a hash belongs to: its first `n_bits` bits.
    pub fn register_index(&self, hash: u64) -> usize {
        todo!("0d-02: the top n_bits bits of the hash as a number (0 when n_bits is 0)")
    }

    /// The run length of the hash: the **position of the leftmost 1** among the 64 - n_bits bits after the register bits, counted from 1.
    /// All zeros: `64 - n_bits + 1`.
    pub fn position_of_leftmost_one(&self, hash: u64) -> u8 {
        todo!("0d-02: drop the first n_bits bits (shift left), count the leading zeros of what is left (at most 64 - n_bits), plus one")
    }

    /// Takes `val` into account.
    pub fn add_elem(&self, val: &K) {
        todo!("0d-02: the register is the top bits of the hash and keeps the longest run seen; nothing to do without registers")
    }

    /// Recomputes the estimate from the registers and stores it.
    pub fn compute_cardinality(&self) {
        todo!("0d-02: CONSTANT * m * m / the sum of 2^-register over the registers, rounded down; 0 with no registers; store it")
    }
}

/// HyperLogLog as Presto stores it: the run length is the number of **trailing** zeros of the bits after the register bits, and a
/// register keeps its low 4 bits in a dense array and the rest (up to 3 bits) in a sparse overflow map, since most registers stay small.
pub struct HyperLogLogPresto<K> {
    _key: PhantomData<fn(&K)>,
    // TODO(0d-03): your fields: the dense low bits, the sparse overflow bits, the last estimate
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
        todo!("0d-03: 2^n registers, all zero, no overflow bits (none at all for a negative n)")
    }

    /// The stored estimate, as of the last [`compute_cardinality`](Self::compute_cardinality).
    pub fn cardinality(&self) -> u64 {
        todo!("0d-03: the stored estimate")
    }

    /// The dense array: the low 4 bits of every register.
    pub fn dense_bucket(&self) -> Vec<u8> {
        todo!("0d-03: a copy of the dense array")
    }

    /// The overflow bits (bits 4 to 6) of register `idx`; 0 if it has none.
    pub fn overflow_bucket_of(&self, idx: u16) -> u8 {
        todo!("0d-03: the overflow bits stored for the register, or 0")
    }

    // TODO(0d-03): helpers of your own (the whole value of a register)

    /// Takes `val` into account. The register is the top `n` bits of its hash; the run is the number of trailing zeros of the other bits (all zero:
    /// `64 - n`). A register only grows.
    pub fn add_elem(&self, val: &K) {
        todo!("0d-03: the register is the top n bits of the hash and keeps the longest run seen (trailing zeros of the rest; all zero: 64 - n), stored as dense and overflow bits")
    }

    /// Recomputes the estimate from the registers, dense and overflow parts joined, as [`HyperLogLog`] does, and stores it.
    pub fn compute_cardinality(&self) {
        todo!("0d-03: the same estimate as HyperLogLog, over the registers rebuilt from dense and overflow bits")
    }
}
