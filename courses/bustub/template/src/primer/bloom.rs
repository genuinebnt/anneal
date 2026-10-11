//! A Bloom filter over u64 keys.

fn mix(mut x: u64, seed: u64) -> u64 {
    x ^= seed;
    x = (x ^ (x >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    x = (x ^ (x >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

pub struct BloomFilter {
    _bloom: (),
}

impl BloomFilter {
    /// An empty filter of `m_bits` bits (at least 1) and `k` hash positions per key (at least 1).
    pub fn new(m_bits: usize, k: u32) -> BloomFilter {
        todo!("0d-c1: m cleared bits")
    }

    /// The `k` bit positions of a key (double hashing).
    pub fn positions(&self, key: u64) -> Vec<usize> {
        todo!("0d-c1: h1 + i * h2 modulo m, for i in 0..k")
    }

    /// Adds `key`: sets its `k` bits.
    pub fn insert(&mut self, key: u64) {
        todo!("0d-c1: set the k bits")
    }

    /// Whether `key` may have been added. `false` is certain; `true` can be a false positive, never a false negative.
    pub fn contains(&self, key: u64) -> bool {
        todo!("0d-c1: all k bits set")
    }

    /// The number of bits set.
    pub fn bits_set(&self) -> usize {
        todo!("0d-c1: how many bits are set")
    }

    /// The filter holding the keys of both, or `None` when the two do not have the same size and `k`.
    pub fn union(&self, other: &BloomFilter) -> Option<BloomFilter> {
        todo!("0d-c1: the bitwise or, for filters of the same shape")
    }
}
