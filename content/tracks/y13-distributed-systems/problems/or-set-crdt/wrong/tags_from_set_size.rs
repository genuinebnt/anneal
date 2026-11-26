use std::collections::BTreeSet;

type Tag = (u32, u64);

#[derive(Debug, Clone)]
pub struct OrSet<T> {
    replica: u32,
    added: BTreeSet<(T, Tag)>,
    removed: BTreeSet<Tag>,
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
        // The number of live additions goes down after a remove, so tags come round again.
        let n = self
            .added
            .iter()
            .filter(|(_, t)| !self.removed.contains(t))
            .count() as u64;
        self.added.insert((item, (self.replica, n)));
    }

    pub fn remove(&mut self, item: &T) -> bool {
        let seen: Vec<Tag> = self
            .added
            .iter()
            .filter(|(x, t)| x == item && !self.removed.contains(t))
            .map(|(_, t)| *t)
            .collect();
        let was = !seen.is_empty();
        self.removed.extend(seen);
        was
    }

    pub fn merge(&mut self, other: &Self) {
        self.added.extend(other.added.iter().cloned());
        self.removed.extend(other.removed.iter().copied());
    }

    pub fn contains(&self, item: &T) -> bool {
        self.added
            .iter()
            .any(|(x, t)| x == item && !self.removed.contains(t))
    }

    pub fn items(&self) -> Vec<T> {
        let mut out: Vec<T> = self
            .added
            .iter()
            .filter(|(_, t)| !self.removed.contains(t))
            .map(|(x, _)| x.clone())
            .collect();
        out.dedup();
        out
    }
}
