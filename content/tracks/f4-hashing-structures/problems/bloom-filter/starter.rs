use std::hash::Hash;

/// A Bloom filter: "definitely not present" or "probably present", in a fixed number of bits.
pub struct Bloom {
    // TODO: a bit array (Vec<u64>), its size in bits, the number of hash functions, an insert counter
}

impl Bloom {
    /// Sized for `expected` items at false-positive rate `fp`:
    /// m = ceil(-n ln p / (ln 2)^2) bits, k = round(m / n * ln 2) hashes (at least 1).
    pub fn new(expected: usize, fp: f64) -> Self {
        todo!()
    }

    /// True if the item was not definitely present before (at least one bit changed).
    /// Derive the k bit positions from two hashes by double hashing: position_i = (h1 + i * h2) mod m.
    pub fn insert<T: Hash + ?Sized>(&mut self, item: &T) -> bool {
        todo!()
    }

    pub fn contains<T: Hash + ?Sized>(&self, item: &T) -> bool {
        todo!()
    }

    pub fn bit_count(&self) -> usize {
        todo!()
    }

    pub fn hash_count(&self) -> u32 {
        todo!()
    }

    /// How many `insert` calls were made (duplicates count).
    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Estimated current false-positive probability: (fraction of bits set)^k.
    pub fn estimated_fp_rate(&self) -> f64 {
        todo!()
    }

    /// Merges another filter built with the same parameters; `Err(())` if the shapes differ.
    pub fn union_with(&mut self, other: &Bloom) -> Result<(), ()> {
        todo!()
    }
}
