enum Slot<V> {
    Empty,
    Deleted,
    Full(u64, V),
}

pub struct OpenMap<V> {
    slots: Vec<Slot<V>>,
    len: usize,
    /// Full plus Deleted slots: what probe chains have to walk past.
    used: usize,
}

impl<V> OpenMap<V> {
    pub fn new() -> Self {
        Self::with_slots(8)
    }

    fn with_slots(n: usize) -> Self {
        OpenMap { slots: (0..n).map(|_| Slot::Empty).collect(), len: 0, used: 0 }
    }

    /// Fibonacci hashing: multiply, then take the high bits.
    fn home(&self, key: u64) -> usize {
        let bits = self.slots.len().trailing_zeros();
        (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - bits)) as usize
    }

    fn find(&self, key: u64) -> Option<usize> {
        let mask = self.slots.len() - 1;
        let mut i = self.home(key);
        for _ in 0..self.slots.len() {
            match &self.slots[i] {
                Slot::Empty => return None,
                Slot::Full(k, _) if *k == key => return Some(i),
                _ => i = (i + 1) & mask,
            }
        }
        None
    }

    pub fn insert(&mut self, key: u64, value: V) -> Option<V> {
        if (self.used + 1) * 4 > self.slots.len() * 3 {
            self.grow();
        }
        let mask = self.slots.len() - 1;
        let mut i = self.home(key);
        // Stops at the key, or at the first slot that isn't Full.
        loop {
            match &mut self.slots[i] {
                Slot::Full(k, v) if *k == key => return Some(std::mem::replace(v, value)),
                Slot::Full(..) => i = (i + 1) & mask,
                _ => break,
            }
        }
        if let Slot::Empty = self.slots[i] {
            self.used += 1;
        }
        self.slots[i] = Slot::Full(key, value);
        self.len += 1;
        None
    }

    pub fn get(&self, key: u64) -> Option<&V> {
        match &self.slots[self.find(key)?] {
            Slot::Full(_, v) => Some(v),
            _ => None,
        }
    }

    pub fn remove(&mut self, key: u64) -> Option<V> {
        let i = self.find(key)?;
        // A tombstone, not Empty: later keys in this probe chain must stay reachable.
        match std::mem::replace(&mut self.slots[i], Slot::Deleted) {
            Slot::Full(_, v) => {
                self.len -= 1;
                Some(v)
            }
            _ => None,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    pub fn empty_slots(&self) -> usize {
        self.slots.len() - self.used
    }

    /// Rebuilds the table without tombstones. It doubles only when live keys fill more than half
    /// the slots; otherwise the slots are mostly tombstones, and a rebuild at the same size is
    /// enough. Without that, insert-then-remove churn would double the table forever.
    fn grow(&mut self) {
        let n = self.slots.len();
        let new_n = if (self.len + 1) * 2 > n { n * 2 } else { n };
        let old = std::mem::replace(self, Self::with_slots(new_n));
        for slot in old.slots {
            if let Slot::Full(k, v) = slot {
                self.insert(k, v);
            }
        }
    }
}

impl<V> Default for OpenMap<V> {
    fn default() -> Self {
        Self::new()
    }
}
