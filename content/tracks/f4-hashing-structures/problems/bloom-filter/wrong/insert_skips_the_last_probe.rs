use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A Bloom filter: "definitely not present" or "probably present", in a fixed number of bits.
pub struct Bloom {
    bits: Vec<u64>,
    m: usize, // number of bits
    k: u32,   // number of hash functions
    inserted: usize,
}

impl Bloom {
    /// Sized for `expected` items at false-positive rate `fp`:
    /// m = ceil(-n ln p / (ln 2)^2) bits, k = round(m / n * ln 2) hashes (at least 1).
    pub fn new(expected: usize, fp: f64) -> Self {
        let n = expected.max(1) as f64;
        let m = (-(n * fp.ln()) / std::f64::consts::LN_2.powi(2)).ceil().max(1.0) as usize;
        let k = ((m as f64 / n) * std::f64::consts::LN_2).round().max(1.0) as u32;
        Bloom { bits: vec![0; m.div_ceil(64)], m, k, inserted: 0 }
    }

    /// Two independent-enough 64-bit hashes from one `Hash` run; the k indices are `h1 + i * h2` (double hashing).
    fn hashes<T: Hash + ?Sized>(item: &T) -> (u64, u64) {
        let mut h = DefaultHasher::new();
        item.hash(&mut h);
        let a = h.finish();
        let mut z = a.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (a, z | 1) // odd, so the stride visits many distinct positions
    }

    fn index(&self, a: u64, b: u64, i: u32) -> usize {
        (a.wrapping_add((i as u64).wrapping_mul(b)) % self.m as u64) as usize
    }

    /// True if the item was not definitely present before (at least one bit changed).
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) -> bool {
        let (a, b) = Self::hashes(item);
        let mut changed = false;
        for i in 0..self.k.saturating_sub(1) {
            let idx = self.index(a, b, i);
            let (w, bit) = (idx / 64, idx % 64);
            if self.bits[w] >> bit & 1 == 0 {
                changed = true;
                self.bits[w] |= 1 << bit;
            }
        }
        self.inserted += 1;
        changed
    }

    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        let (a, b) = Self::hashes(item);
        (0..self.k).all(|i| {
            let idx = self.index(a, b, i);
            self.bits[idx / 64] >> (idx % 64) & 1 == 1
        })
    }

    pub fn bit_count(&self) -> usize {
        self.m
    }

    pub fn hash_count(&self) -> u32 {
        self.k
    }

    /// How many `insert` calls were made (duplicates count).
    pub fn len(&self) -> usize {
        self.inserted
    }

    pub fn is_empty(&self) -> bool {
        self.inserted == 0
    }

    /// Estimated current false-positive probability: (fraction of bits set)^k.
    pub fn estimated_fp_rate(&self) -> f64 {
        let ones: u32 = self.bits.iter().map(|w| w.count_ones()).sum();
        (ones as f64 / self.m as f64).powi(self.k as i32)
    }

    /// Merges another filter built with the same parameters; `Err(())` if the shapes differ.
    pub fn union_with(&mut self, other: &Bloom) -> Result<(), ()> {
        if self.m != other.m || self.k != other.k {
            return Err(());
        }
        for (a, b) in self.bits.iter_mut().zip(&other.bits) {
            *a |= b;
        }
        self.inserted += other.inserted;
        Ok(())
    }
}
