enum Slot<V> {
    Empty,
    Deleted,
    Full(u64, V),
}

pub struct OpenMap<V> {
    slots: Vec<Slot<V>>,
    len: usize,
}

impl<V> OpenMap<V> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn insert(&mut self, key: u64, value: V) -> Option<V> {
        todo!()
    }

    pub fn get(&self, key: u64) -> Option<&V> {
        todo!()
    }

    pub fn remove(&mut self, key: u64) -> Option<V> {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    /// The number of slots.
    pub fn capacity(&self) -> usize {
        todo!()
    }

    /// The number of `Empty` slots (neither full nor a tombstone).
    pub fn empty_slots(&self) -> usize {
        todo!()
    }
}
