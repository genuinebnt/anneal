//! Linear probing with backward-shift deletion.

#[derive(Debug, PartialEq, Eq)]
pub struct Full;

pub struct ShiftSet {
    _shift: (),
}

/// The slot a key starts probing at.
pub fn home(key: u64, capacity: usize) -> usize {
    let mut x = key.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 29;
    (x % capacity as u64) as usize
}

impl ShiftSet {
    /// An empty set of `capacity` slots (at least 1).
    pub fn new(capacity: usize) -> ShiftSet {
        todo!("0c-c1: every slot empty")
    }

    /// The number of slots.
    pub fn capacity(&self) -> usize {
        todo!("0c-c1: the number of slots")
    }

    /// The number of keys.
    pub fn len(&self) -> usize {
        todo!("0c-c1: the number of keys")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // TODO(0c-c1): a probe helper of your own

    /// Whether `key` is in the set.
    pub fn contains(&self, key: u64) -> bool {
        todo!("0c-c1: probe from the home slot until the key or an empty slot")
    }

    /// How far `key` sits from its home slot; `None` if it is absent.
    pub fn probe_len(&self, key: u64) -> Option<usize> {
        todo!("0c-c1: how far the key sits from its home slot")
    }

    /// Adds `key` in the first empty slot at or after its home slot. `Ok(false)` if it was already there, `Err(Full)` when no slot is left.
    pub fn insert(&mut self, key: u64) -> Result<bool, Full> {
        todo!("0c-c1: the first empty slot at or after the home slot")
    }

    /// Removes `key`; returns whether it was there. Later keys of the run move back into the hole when their home allows it, so no tombstone is left.
    pub fn remove(&mut self, key: u64) -> bool {
        todo!("0c-c1: empty the slot, then move later keys of the run back into the hole when their home allows it")
    }
}
