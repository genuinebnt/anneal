//! A cuckoo hash set: two tables, two hash functions, two possible homes for every key.

fn mix(mut x: u64, seed: u64) -> u64 {
    x ^= seed;
    x = (x ^ (x >> 33)).wrapping_mul(0xff51_afd7_ed55_8ccd);
    x = (x ^ (x >> 33)).wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

pub struct CuckooSet {
    // @begin 2b-c4
    tables: [Vec<Option<u64>>; 2],
    len: usize,
    max_kicks: usize,
    //~ _cuckoo: (),
    // @end
}

impl CuckooSet {
    /// An empty set.
    pub fn new() -> CuckooSet {
        // @begin 2b-c4
        CuckooSet { tables: [vec![None; 8], vec![None; 8]], len: 0, max_kicks: 32 }
        //~ todo!("2b-c4: two small tables")
        // @end
    }

    /// The number of keys.
    pub fn len(&self) -> usize {
        // @begin 2b-c4
        self.len
        //~ todo!("2b-c4: how many keys")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The two homes of `key` as (table, slot), for the current table size.
    pub fn homes(&self, key: u64) -> [(usize, usize); 2] {
        // @begin 2b-c4
        let n = self.tables[0].len() as u64;
        [(0, (mix(key, 0x1234_5678) % n) as usize), (1, (mix(key, 0x9abc_def0) % n) as usize)]
        //~ todo!("2b-c4: slot h1(key) of table 0 and h2(key) of table 1")
        // @end
    }

    /// Where `key` is stored right now.
    pub fn position(&self, key: u64) -> Option<(usize, usize)> {
        // @begin 2b-c4
        self.homes(key).into_iter().find(|&(t, s)| self.tables[t][s] == Some(key))
        //~ todo!("2b-c4: look only at the two homes")
        // @end
    }

    pub fn contains(&self, key: u64) -> bool {
        self.position(key).is_some()
    }

    /// Adds `key`; returns false if it was already there.
    ///
    /// If displacing keys takes too long, or the tables are half full, the tables grow and every key is placed again.
    pub fn insert(&mut self, key: u64) -> bool {
        // @begin 2b-c4
        if self.contains(key) {
            return false;
        }
        self.len += 1;
        if let Err(loose) = self.place(key) {
            let mut keys = self.stored();
            keys.push(loose);
            self.rehash(keys);
        }
        if self.len > self.tables[0].len() {
            let keys = self.stored();
            self.rehash(keys);
        }
        true
        //~ todo!("2b-c4: place the key, displacing others; grow and place everything again if that takes too long or the tables are half full")
        // @end
    }

    // @begin 2b-c4
    fn stored(&self) -> Vec<u64> {
        self.tables.iter().flatten().flatten().copied().collect()
    }

    /// Walks the displacement chain; `Err(k)` names the key that was left without a home after `max_kicks` displacements.
    fn place(&mut self, key: u64) -> Result<(), u64> {
        let mut cur = key;
        let mut t = 0;
        for _ in 0..self.max_kicks {
            let s = self.homes(cur)[t].1;
            match self.tables[t][s].replace(cur) {
                None => return Ok(()),
                Some(old) => {
                    cur = old;
                    t = 1 - t;
                }
            }
        }
        Err(cur)
    }

    /// Doubles the tables and places every key again, doubling further if a placement fails.
    fn rehash(&mut self, mut keys: Vec<u64>) {
        loop {
            let n = self.tables[0].len() * 2;
            self.tables = [vec![None; n], vec![None; n]];
            let mut failed = None;
            for (i, &k) in keys.iter().enumerate() {
                if let Err(loose) = self.place(k) {
                    failed = Some((i, loose));
                    break;
                }
            }
            match failed {
                None => return,
                Some((i, loose)) => {
                    let mut all = self.stored();
                    all.push(loose);
                    all.extend_from_slice(&keys[i + 1..]);
                    keys = all;
                }
            }
        }
    }
    //~ // TODO(2b-c4): helpers of your own (displacement walk, growing)
    // @end

    /// Removes `key`; returns whether it was there.
    pub fn remove(&mut self, key: u64) -> bool {
        // @begin 2b-c4
        match self.position(key) {
            Some((t, s)) => {
                self.tables[t][s] = None;
                self.len -= 1;
                true
            }
            None => false,
        }
        //~ todo!("2b-c4: clear the key's slot")
        // @end
    }
}

impl Default for CuckooSet {
    fn default() -> Self {
        CuckooSet::new()
    }
}
