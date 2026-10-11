//! A leaf whose deleted entries stay behind as tombstones until it is compacted.

pub struct TombstoneLeaf {
    // @begin 2d-c2
    /// (key, value) in key order; a value of `None` is a tombstone.
    slots: Vec<(i64, Option<u64>)>,
    //~ _leaf: (),
    // @end
}

impl TombstoneLeaf {
    /// An empty leaf.
    pub fn new() -> TombstoneLeaf {
        // @begin 2d-c2
        TombstoneLeaf { slots: Vec::new() }
        //~ todo!("2d-c2: an empty leaf")
        // @end
    }

    /// Stores `value` under `key`; returns the value it replaced if the key was live.
    pub fn insert(&mut self, key: i64, value: u64) -> Option<u64> {
        // @begin 2d-c2
        match self.slots.binary_search_by_key(&key, |s| s.0) {
            Ok(i) => self.slots[i].1.replace(value),
            Err(i) => {
                self.slots.insert(i, (key, Some(value)));
                None
            }
        }
        //~ todo!("2d-c2: put the key in order, reusing its slot if it has one (live or dead)")
        // @end
    }

    /// Marks the key dead; returns its value if it was live.
    pub fn remove(&mut self, key: i64) -> Option<u64> {
        // @begin 2d-c2
        let i = self.slots.binary_search_by_key(&key, |s| s.0).ok()?;
        self.slots[i].1.take()
        //~ todo!("2d-c2: leave a tombstone")
        // @end
    }

    /// The live value of `key`; `None` if the key is absent or deleted.
    pub fn get(&self, key: i64) -> Option<u64> {
        // @begin 2d-c2
        let i = self.slots.binary_search_by_key(&key, |s| s.0).ok()?;
        self.slots[i].1
        //~ todo!("2d-c2: the live value")
        // @end
    }

    /// The number of slots, live or dead.
    pub fn slots(&self) -> usize {
        // @begin 2d-c2
        self.slots.len()
        //~ todo!("2d-c2: all slots, live or dead")
        // @end
    }

    /// The number of dead slots (tombstones).
    pub fn tombstones(&self) -> usize {
        // @begin 2d-c2
        self.slots.iter().filter(|s| s.1.is_none()).count()
        //~ todo!("2d-c2: dead slots")
        // @end
    }

    pub fn live_len(&self) -> usize {
        self.slots() - self.tombstones()
    }

    pub fn should_compact(&self) -> bool {
        self.tombstones() * 2 > self.slots()
    }

    /// Drops every tombstone, keeping the live keys in order.
    pub fn compact(&mut self) {
        // @begin 2d-c2
        self.slots.retain(|s| s.1.is_some());
        //~ todo!("2d-c2: remove the dead slots")
        // @end
    }

    /// The live keys in order.
    pub fn keys(&self) -> Vec<i64> {
        // @begin 2d-c2
        self.slots.iter().filter(|s| s.1.is_some()).map(|s| s.0).collect()
        //~ todo!("2d-c2: the keys of the live slots")
        // @end
    }
}

impl Default for TombstoneLeaf {
    fn default() -> Self {
        TombstoneLeaf::new()
    }
}
