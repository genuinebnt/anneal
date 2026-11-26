use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct OrSet<T> {
    replica: u32,
    items: BTreeSet<T>,
}

impl<T: Ord + Clone> OrSet<T> {
    pub fn new(replica: u32) -> Self {
        OrSet {
            replica,
            items: BTreeSet::new(),
        }
    }

    pub fn add(&mut self, item: T) {
        self.items.insert(item);
    }

    pub fn remove(&mut self, item: &T) -> bool {
        self.items.remove(item)
    }

    pub fn merge(&mut self, other: &Self) {
        self.items.extend(other.items.iter().cloned());
    }

    pub fn contains(&self, item: &T) -> bool {
        self.items.contains(item)
    }

    pub fn items(&self) -> Vec<T> {
        self.items.iter().cloned().collect()
    }
}
