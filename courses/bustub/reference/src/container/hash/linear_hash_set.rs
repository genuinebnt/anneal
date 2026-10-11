//! Linear hashing: a hash set that grows **one bucket at a time**, in a fixed order, instead of doubling a directory (extendible hashing) or
//! rebuilding the table. It needs no directory at all.

/// A set of `u64`s in buckets. The table has `n` buckets; bucket `i` of the first `round_size` uses hash mod `round_size`, and the buckets
/// that have already been split in this round use hash mod `2 * round_size`. A split takes the next bucket in order, adds a bucket at the end,
/// and moves the keys that now hash there.
pub struct LinearHashSet {
    // @begin 2b-c1
    buckets: Vec<Vec<u64>>,
    /// The size of the table when this round of splitting began.
    round_size: usize,
    /// The next bucket to split in this round.
    next_split: usize,
    len: usize,
    max_load: f64,
    //~ _linear: (),
    // @end
}

/// A cheap, deterministic mixing function: callers choose the keys, the table spreads them.
pub fn hash(key: u64) -> u64 {
    let mut x = key.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

impl LinearHashSet {
    /// Starts with `initial_buckets` (at least 1) buckets and splits one bucket whenever the number of keys per bucket goes above `max_load`.
    pub fn new(initial_buckets: usize, max_load: f64) -> LinearHashSet {
        // @begin 2b-c1
        let n = initial_buckets.max(1);
        LinearHashSet { buckets: vec![Vec::new(); n], round_size: n, next_split: 0, len: 0, max_load }
        //~ todo!("2b-c1: the starting buckets")
        // @end
    }

    /// The bucket `key` lives in: its hash modulo the round's size, or modulo twice that size for the buckets already split this round.
    fn bucket_of(&self, key: u64) -> usize {
        // @begin 2b-c1
        let h = hash(key);
        let low = (h % self.round_size as u64) as usize;
        if low < self.next_split {
            (h % (2 * self.round_size) as u64) as usize
        } else {
            low
        }
        //~ todo!("2b-c1: which bucket a key is in: hash mod the round's size, or twice that for buckets already split")
        // @end
    }

    /// Whether `key` is in the set.
    pub fn contains(&self, key: u64) -> bool {
        // @begin 2b-c1
        self.buckets[self.bucket_of(key)].contains(&key)
        //~ todo!("2b-c1: look in the key's bucket")
        // @end
    }

    /// Adds `key`; false if it was there. May split a bucket afterwards: one, and only one, per insert that pushes the load over the limit.
    pub fn insert(&mut self, key: u64) -> bool {
        // @begin 2b-c1
        let b = self.bucket_of(key);
        if self.buckets[b].contains(&key) {
            return false;
        }
        self.buckets[b].push(key);
        self.len += 1;
        if self.load() > self.max_load {
            self.split();
        }
        true
        //~ todo!("2b-c1: add the key; split the next bucket when the load is over the limit")
        // @end
    }

    // @begin 2b-c1
    fn split(&mut self) {
        let victim = self.next_split;
        let old = std::mem::take(&mut self.buckets[victim]);
        self.buckets.push(Vec::new());
        self.next_split += 1;
        if self.next_split == self.round_size {
            self.round_size *= 2;
            self.next_split = 0;
        }
        for k in old {
            let b = self.bucket_of(k);
            self.buckets[b].push(k);
        }
    }
    //~ // TODO(2b-c1): splitting the next bucket (your own helper)
    // @end

    /// Removes `key`; returns whether it was there. Buckets are never merged.
    pub fn remove(&mut self, key: u64) -> bool {
        // @begin 2b-c1
        let b = self.bucket_of(key);
        match self.buckets[b].iter().position(|&k| k == key) {
            Some(at) => {
                self.buckets[b].swap_remove(at);
                self.len -= 1;
                true
            }
            None => false,
        }
        //~ todo!("2b-c1: remove it from its bucket (buckets are never merged here)")
        // @end
    }

    /// The number of keys.
    pub fn len(&self) -> usize {
        // @begin 2b-c1
        self.len
        //~ todo!("2b-c1: how many keys")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The number of buckets.
    pub fn bucket_count(&self) -> usize {
        // @begin 2b-c1
        self.buckets.len()
        //~ todo!("2b-c1: how many buckets")
        // @end
    }

    /// Keys per bucket on average.
    pub fn load(&self) -> f64 {
        // @begin 2b-c1
        self.len as f64 / self.bucket_count() as f64
        //~ todo!("2b-c1: keys per bucket")
        // @end
    }
}
