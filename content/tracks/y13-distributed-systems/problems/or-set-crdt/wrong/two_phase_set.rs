use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct OrSet<T> {
    replica: u32,
    added: BTreeSet<T>,
    removed: BTreeSet<T>,
}

impl<T: Ord + Clone> OrSet<T> {
    pub fn new(replica: u32) -> Self {
        OrSet {
            replica,
            added: BTreeSet::new(),
            removed: BTreeSet::new(),
        }
    }

    pub fn add(&mut self, item: T) {
        self.added.insert(item);
    }

    pub fn remove(&mut self, item: &T) -> bool {
        let was = self.contains(item);
        if was {
            self.removed.insert(item.clone());
        }
        was
    }

    pub fn merge(&mut self, other: &Self) {
        self.added.extend(other.added.iter().cloned());
        self.removed.extend(other.removed.iter().cloned());
    }

    pub fn contains(&self, item: &T) -> bool {
        self.added.contains(item) && !self.removed.contains(item)
    }

    pub fn items(&self) -> Vec<T> {
        self.added.difference(&self.removed).cloned().collect()
    }
}
