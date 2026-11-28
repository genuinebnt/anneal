pub struct OpenMap<V> {
    items: Vec<(u64, V)>,
}

impl<V> OpenMap<V> {
    pub fn new() -> Self {
        OpenMap { items: Vec::new() }
    }

    pub fn insert(&mut self, key: u64, value: V) -> Option<V> {
        if let Some((_, v)) = self.items.iter_mut().find(|(k, _)| *k == key) {
            return Some(std::mem::replace(v, value));
        }
        self.items.push((key, value));
        None
    }

    pub fn get(&self, key: u64) -> Option<&V> {
        self.items.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn remove(&mut self, key: u64) -> Option<V> {
        let i = self.items.iter().position(|(k, _)| *k == key)?;
        Some(self.items.swap_remove(i).1)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn capacity(&self) -> usize {
        self.items.capacity().max(8)
    }

    pub fn empty_slots(&self) -> usize {
        self.capacity() - self.items.len()
    }
}
