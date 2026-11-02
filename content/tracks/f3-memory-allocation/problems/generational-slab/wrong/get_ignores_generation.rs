/// A handle to a slot: the slot number plus the generation it had when it was handed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub idx: u32,
    pub gen: u32,
}

enum Slot<T> {
    Full(T),
    Empty { next_free: Option<u32> },
}

struct Entry<T> {
    gen: u32,
    slot: Slot<T>,
}

/// Values in a `Vec`, addressed by `Key`. Freed slots are reused (last freed, first reused); each reuse bumps the
/// slot's generation, so a key to a removed value can never reach the value that took its place.
pub struct Slab<T> {
    entries: Vec<Entry<T>>,
    free_head: Option<u32>,
    len: usize,
}

impl<T> Slab<T> {
    pub fn new() -> Self {
        Slab { entries: Vec::new(), free_head: None, len: 0 }
    }

    pub fn insert(&mut self, value: T) -> Key {
        self.len += 1;
        match self.free_head {
            Some(idx) => {
                let e = &mut self.entries[idx as usize];
                let Slot::Empty { next_free } = e.slot else { unreachable!("free list points at a full slot") };
                self.free_head = next_free;
                e.slot = Slot::Full(value);
                Key { idx, gen: e.gen }
            }
            None => {
                let idx = self.entries.len() as u32;
                self.entries.push(Entry { gen: 0, slot: Slot::Full(value) });
                Key { idx, gen: 0 }
            }
        }
    }

    pub fn get(&self, key: Key) -> Option<&T> {
        match self.entries.get(key.idx as usize) {
            Some(Entry { slot: Slot::Full(v), .. }) => Some(v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: Key) -> Option<&mut T> {
        match self.entries.get_mut(key.idx as usize) {
            Some(Entry { gen, slot: Slot::Full(v) }) if *gen == key.gen => Some(v),
            _ => None,
        }
    }

    pub fn remove(&mut self, key: Key) -> Option<T> {
        let e = self.entries.get_mut(key.idx as usize)?;
        if e.gen != key.gen || matches!(e.slot, Slot::Empty { .. }) {
            return None;
        }
        let old = std::mem::replace(&mut e.slot, Slot::Empty { next_free: self.free_head });
        e.gen += 1; // every key handed out so far for this slot is now stale
        self.free_head = Some(key.idx);
        self.len -= 1;
        match old {
            Slot::Full(v) => Some(v),
            Slot::Empty { .. } => unreachable!(),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Slots ever allocated (full or free). Reuse keeps this from growing under churn.
    pub fn slots(&self) -> usize {
        self.entries.len()
    }

    /// Live entries in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (Key, &T)> {
        self.entries.iter().enumerate().filter_map(|(i, e)| match &e.slot {
            Slot::Full(v) => Some((Key { idx: i as u32, gen: e.gen }, v)),
            Slot::Empty { .. } => None,
        })
    }
}
