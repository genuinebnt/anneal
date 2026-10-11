//! A Bloom filter over u64 keys.

fn mix(mut x: u64, seed: u64) -> u64 {
    x ^= seed;
    x = (x ^ (x >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    x = (x ^ (x >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

pub struct BloomFilter {
    // @begin 0d-c1
    bits: Vec<u64>,
    m: usize,
    k: u32,
    //~ _bloom: (),
    // @end
}

impl BloomFilter {
    /// An empty filter of `m_bits` bits (at least 1) and `k` hash positions per key (at least 1).
    pub fn new(m_bits: usize, k: u32) -> BloomFilter {
        // @begin 0d-c1
        let m = m_bits.max(1);
        BloomFilter { bits: vec![0; m.div_ceil(64)], m, k: k.max(1) }
        //~ todo!("0d-c1: m cleared bits")
        // @end
    }

    /// The `k` bit positions of a key (double hashing).
    pub fn positions(&self, key: u64) -> Vec<usize> {
        // @begin 0d-c1
        let (h1, h2) = (mix(key, 0x9E37_79B9), mix(key, 0x7F4A_7C15) | 1);
        (0..self.k as u64).map(|i| (h1.wrapping_add(i.wrapping_mul(h2)) % self.m as u64) as usize).collect()
        //~ todo!("0d-c1: h1 + i * h2 modulo m, for i in 0..k")
        // @end
    }

    /// Adds `key`: sets its `k` bits.
    pub fn insert(&mut self, key: u64) {
        // @begin 0d-c1
        for p in self.positions(key) {
            self.bits[p / 64] |= 1 << (p % 64);
        }
        //~ todo!("0d-c1: set the k bits")
        // @end
    }

    /// Whether `key` may have been added. `false` is certain; `true` can be a false positive, never a false negative.
    pub fn contains(&self, key: u64) -> bool {
        // @begin 0d-c1
        self.positions(key).into_iter().all(|p| self.bits[p / 64] >> (p % 64) & 1 == 1)
        //~ todo!("0d-c1: all k bits set")
        // @end
    }

    /// The number of bits set.
    pub fn bits_set(&self) -> usize {
        // @begin 0d-c1
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
        //~ todo!("0d-c1: how many bits are set")
        // @end
    }

    /// The filter holding the keys of both, or `None` when the two do not have the same size and `k`.
    pub fn union(&self, other: &BloomFilter) -> Option<BloomFilter> {
        // @begin 0d-c1
        if self.m != other.m || self.k != other.k {
            return None;
        }
        Some(BloomFilter { bits: self.bits.iter().zip(&other.bits).map(|(a, b)| a | b).collect(), m: self.m, k: self.k })
        //~ todo!("0d-c1: the bitwise or, for filters of the same shape")
        // @end
    }
}
