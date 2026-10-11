//! A cuckoo hash set: two tables, two hash functions, two possible homes for every key.

fn mix(mut x: u64, seed: u64) -> u64 {
    x ^= seed;
    x = (x ^ (x >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    x = (x ^ (x >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

pub struct CuckooSet {
    _cuckoo: (),
}

impl CuckooSet {
    /// An empty set.
    pub fn new() -> CuckooSet {
        todo!("2b-c4: two small tables")
    }

    /// The number of keys.
    pub fn len(&self) -> usize {
        todo!("2b-c4: how many keys")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The two homes of `key` as (table, slot), for the current table size.
    pub fn homes(&self, key: u64) -> [(usize, usize); 2] {
        todo!("2b-c4: slot h1(key) of table 0 and h2(key) of table 1")
    }

    /// Where `key` is stored right now.
    pub fn position(&self, key: u64) -> Option<(usize, usize)> {
        todo!("2b-c4: look only at the two homes")
    }

    pub fn contains(&self, key: u64) -> bool {
        self.position(key).is_some()
    }

    /// Adds `key`; returns false if it was already there.
    ///
    /// If displacing keys takes too long, or the tables are half full, the tables grow and every key is placed again.
    pub fn insert(&mut self, key: u64) -> bool {
        todo!("2b-c4: place the key, displacing others; grow and place everything again if that takes too long or the tables are half full")
    }

    // TODO(2b-c4): helpers of your own (displacement walk, growing)

    /// Removes `key`; returns whether it was there.
    pub fn remove(&mut self, key: u64) -> bool {
        todo!("2b-c4: clear the key's slot")
    }
}

impl Default for CuckooSet {
    fn default() -> Self {
        CuckooSet::new()
    }
}
