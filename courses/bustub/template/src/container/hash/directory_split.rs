//! The directory of an extendible hash table reduced to its bookkeeping: which bucket each slot points at, and how deep each bucket is.

/// A directory of `2^global_depth` slots. Slot `i` is reached by the **low `global_depth` bits** of a hash. A bucket of local depth `d` is
/// pointed at by every slot that agrees with it on its low `d` bits (so by `2^(global_depth - d)` slots).
pub struct Directory {
    global_depth: u32,
    /// slot -> bucket id
    slots: Vec<usize>,
    /// bucket id -> local depth
    depth: Vec<u32>,
}

impl Directory {
    /// One bucket (id 0), depth 0, a directory of one slot.
    pub fn new() -> Directory {
        Directory { global_depth: 0, slots: vec![0], depth: vec![0] }
    }

    pub fn global_depth(&self) -> u32 {
        self.global_depth
    }

    pub fn bucket_count(&self) -> usize {
        self.depth.len()
    }

    pub fn local_depth(&self, bucket: usize) -> u32 {
        self.depth[bucket]
    }

    /// The bucket a hash value is in.
    pub fn bucket_for(&self, hash: u32) -> usize {
        self.slots[(hash & ((1u32 << self.global_depth) - 1)) as usize]
    }

    /// The slots that point at `bucket`, in increasing order.
    pub fn slots_of(&self, bucket: usize) -> Vec<usize> {
        (0..self.slots.len()).filter(|&i| self.slots[i] == bucket).collect()
    }

    /// Splits `bucket`: its depth grows by one, a new bucket is created (and returned) and takes the slots whose next bit is 1. When the
    /// bucket is already as deep as the directory, the directory doubles first (the new half is a copy of the old one).
    pub fn split(&mut self, bucket: usize) -> usize {
        if self.depth[bucket] == self.global_depth {
            // TODO(2b-c2): this looks right and has a bug; find it and fix it
            let copy: Vec<usize> = self.slots.iter().rev().copied().collect();
            self.slots.extend(copy);
            self.global_depth += 1;
        }
        let new = self.depth.len();
        self.depth[bucket] += 1;
        self.depth.push(self.depth[bucket]);
        let bit = 1usize << (self.depth[bucket] - 1);
        for i in 0..self.slots.len() {
            if self.slots[i] == bucket && i & bit != 0 {
                self.slots[i] = new;
            }
        }
        new
    }
}

impl Default for Directory {
    fn default() -> Self {
        Directory::new()
    }
}
