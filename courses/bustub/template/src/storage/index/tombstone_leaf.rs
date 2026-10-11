//! A leaf whose deleted entries stay behind as tombstones until it is compacted.

pub struct TombstoneLeaf {
    _leaf: (),
}

impl TombstoneLeaf {
    /// An empty leaf.
    pub fn new() -> TombstoneLeaf {
        todo!("2d-c2: an empty leaf")
    }

    /// Stores `value` under `key`; returns the value it replaced if the key was live.
    pub fn insert(&mut self, key: i64, value: u64) -> Option<u64> {
        todo!("2d-c2: put the key in order, reusing its slot if it has one (live or dead)")
    }

    /// Marks the key dead; returns its value if it was live.
    pub fn remove(&mut self, key: i64) -> Option<u64> {
        todo!("2d-c2: leave a tombstone")
    }

    /// The live value of `key`; `None` if the key is absent or deleted.
    pub fn get(&self, key: i64) -> Option<u64> {
        todo!("2d-c2: the live value")
    }

    /// The number of slots, live or dead.
    pub fn slots(&self) -> usize {
        todo!("2d-c2: all slots, live or dead")
    }

    /// The number of dead slots (tombstones).
    pub fn tombstones(&self) -> usize {
        todo!("2d-c2: dead slots")
    }

    pub fn live_len(&self) -> usize {
        self.slots() - self.tombstones()
    }

    pub fn should_compact(&self) -> bool {
        self.tombstones() * 2 > self.slots()
    }

    /// Drops every tombstone, keeping the live keys in order.
    pub fn compact(&mut self) {
        todo!("2d-c2: remove the dead slots")
    }

    /// The live keys in order.
    pub fn keys(&self) -> Vec<i64> {
        todo!("2d-c2: the keys of the live slots")
    }
}

impl Default for TombstoneLeaf {
    fn default() -> Self {
        TombstoneLeaf::new()
    }
}
