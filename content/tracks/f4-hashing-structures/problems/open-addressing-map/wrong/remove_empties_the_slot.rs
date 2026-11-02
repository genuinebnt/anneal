use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};

enum Slot<K, V> {
    Empty,
    Tombstone, // a removed entry: probe chains must continue past it
    Full(K, V),
}

fn empty_slots<K, V>(n: usize) -> Vec<Slot<K, V>> {
    (0..n).map(|_| Slot::Empty).collect()
}

/// An open-addressing hash map: one flat array, linear probing, tombstones for removal.
/// The capacity is a power of two (starting at 8); the table is kept under 7/8 full counting tombstones.
pub struct OaMap<K, V, S = RandomState> {
    slots: Vec<Slot<K, V>>,
    len: usize,
    tombs: usize,
    hasher: S,
}

impl<K: Hash + Eq, V> OaMap<K, V, RandomState> {
    pub fn new() -> Self {
        OaMap::with_hasher(RandomState::new())
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> OaMap<K, V, S> {
    pub fn with_hasher(hasher: S) -> Self {
        OaMap { slots: empty_slots(8), len: 0, tombs: 0, hasher }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of slots (always a power of two).
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    fn start(&self, key: &K) -> usize {
        (self.hasher.hash_one(key) as usize) & (self.slots.len() - 1)
    }

    fn find(&self, key: &K) -> Option<usize> {
        let mask = self.slots.len() - 1;
        let mut i = self.start(key);
        loop {
            match &self.slots[i] {
                Slot::Empty => return None, // an empty slot ends every probe chain
                Slot::Full(k, _) if k == key => return Some(i),
                _ => {}
            }
            i = (i + 1) & mask;
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        match self.find(key) {
            Some(i) => match &self.slots[i] {
                Slot::Full(_, v) => Some(v),
                _ => unreachable!(),
            },
            None => None,
        }
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        match self.find(key) {
            Some(i) => match &mut self.slots[i] {
                Slot::Full(_, v) => Some(v),
                _ => unreachable!(),
            },
            None => None,
        }
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.find(key).is_some()
    }

    /// Rebuilds the table: doubles it if it is more than half full of live entries, otherwise rebuilds at the same size
    /// (which is what clears the tombstones).
    fn rehash(&mut self) {
        let cap = self.slots.len();
        let new_cap = if (self.len + 1) * 2 > cap { cap * 2 } else { cap };
        let old = std::mem::replace(&mut self.slots, empty_slots(new_cap));
        let mask = new_cap - 1;
        self.tombs = 0;
        for slot in old {
            if let Slot::Full(k, v) = slot {
                let mut i = (self.hasher.hash_one(&k) as usize) & mask;
                while !matches!(self.slots[i], Slot::Empty) {
                    i = (i + 1) & mask;
                }
                self.slots[i] = Slot::Full(k, v);
            }
        }
    }

    /// Inserts or replaces; returns the old value.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if (self.len + self.tombs + 1) * 8 > self.slots.len() * 7 {
            self.rehash();
        }
        let mask = self.slots.len() - 1;
        let mut i = self.start(&key);
        let mut first_tomb = None;
        loop {
            match &mut self.slots[i] {
                Slot::Full(k, v) if *k == key => return Some(std::mem::replace(v, value)),
                Slot::Full(..) => {}
                Slot::Tombstone => {
                    if first_tomb.is_none() {
                        first_tomb = Some(i);
                    }
                }
                Slot::Empty => break,
            }
            i = (i + 1) & mask;
        }
        // The key is absent: reuse the first tombstone passed, else the empty slot that ended the chain.
        let target = match first_tomb {
            Some(t) => {
                self.tombs -= 1;
                t
            }
            None => i,
        };
        self.slots[target] = Slot::Full(key, value);
        self.len += 1;
        None
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let i = self.find(key)?;
        let old = std::mem::replace(&mut self.slots[i], Slot::Empty);
        self.len -= 1;
        match old {
            Slot::Full(_, v) => Some(v),
            _ => unreachable!(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.slots.iter().filter_map(|s| match s {
            Slot::Full(k, v) => Some((k, v)),
            _ => None,
        })
    }
}
