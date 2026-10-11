//! Linear probing with backward-shift deletion.

#[derive(Debug, PartialEq, Eq)]
pub struct Full;

pub struct ShiftSet {
    // @begin 0c-c1
    slots: Vec<Option<u64>>,
    len: usize,
    //~ _shift: (),
    // @end
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
        // @begin 0c-c1
        ShiftSet { slots: vec![None; capacity.max(1)], len: 0 }
        //~ todo!("0c-c1: every slot empty")
        // @end
    }

    /// The number of slots.
    pub fn capacity(&self) -> usize {
        // @begin 0c-c1
        self.slots.len()
        //~ todo!("0c-c1: the number of slots")
        // @end
    }

    /// The number of keys.
    pub fn len(&self) -> usize {
        // @begin 0c-c1
        self.len
        //~ todo!("0c-c1: the number of keys")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // @begin 0c-c1
    fn find(&self, key: u64) -> Option<usize> {
        let n = self.slots.len();
        let h = home(key, n);
        for d in 0..n {
            let i = (h + d) % n;
            match self.slots[i] {
                None => return None,
                Some(k) if k == key => return Some(i),
                Some(_) => {}
            }
        }
        None
    }
    //~ // TODO(0c-c1): a probe helper of your own
    // @end

    /// Whether `key` is in the set.
    pub fn contains(&self, key: u64) -> bool {
        // @begin 0c-c1
        self.find(key).is_some()
        //~ todo!("0c-c1: probe from the home slot until the key or an empty slot")
        // @end
    }

    /// How far `key` sits from its home slot; `None` if it is absent.
    pub fn probe_len(&self, key: u64) -> Option<usize> {
        // @begin 0c-c1
        let i = self.find(key)?;
        let n = self.slots.len();
        Some((i + n - home(key, n)) % n)
        //~ todo!("0c-c1: how far the key sits from its home slot")
        // @end
    }

    /// Adds `key` in the first empty slot at or after its home slot. `Ok(false)` if it was already there, `Err(Full)` when no slot is left.
    pub fn insert(&mut self, key: u64) -> Result<bool, Full> {
        // @begin 0c-c1
        if self.find(key).is_some() {
            return Ok(false);
        }
        if self.len == self.slots.len() {
            return Err(Full);
        }
        let n = self.slots.len();
        let mut i = home(key, n);
        while self.slots[i].is_some() {
            i = (i + 1) % n;
        }
        self.slots[i] = Some(key);
        self.len += 1;
        Ok(true)
        //~ todo!("0c-c1: the first empty slot at or after the home slot")
        // @end
    }

    /// Removes `key`; returns whether it was there. Later keys of the run move back into the hole when their home allows it, so no tombstone is left.
    pub fn remove(&mut self, key: u64) -> bool {
        // @begin 0c-c1
        let Some(mut hole) = self.find(key) else { return false };
        let n = self.slots.len();
        self.slots[hole] = None;
        self.len -= 1;
        // shift back every following key of the run that would otherwise become unreachable
        let mut j = (hole + 1) % n;
        while let Some(k) = self.slots[j] {
            let h = home(k, n);
            // k may move into the hole only if its home is not strictly between the hole and j (cyclically)
            let dist_home_to_j = (j + n - h) % n;
            let dist_hole_to_j = (j + n - hole) % n;
            if dist_home_to_j >= dist_hole_to_j {
                self.slots[hole] = Some(k);
                self.slots[j] = None;
                hole = j;
            }
            j = (j + 1) % n;
        }
        true
        //~ todo!("0c-c1: empty the slot, then move later keys of the run back into the hole when their home allows it")
        // @end
    }
}
