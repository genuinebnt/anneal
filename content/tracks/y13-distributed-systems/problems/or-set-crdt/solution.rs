use std::collections::BTreeSet;

/// Names one addition: the replica that made it and that replica's running number.
type Tag = (u32, u64);

#[derive(Debug, Clone)]
pub struct OrSet<T> {
    replica: u32,
    next: u64,
    /// Every addition ever seen. Grows only.
    added: BTreeSet<(T, Tag)>,
    /// Every addition that has been removed. Grows only.
    removed: BTreeSet<Tag>,
}

impl<T: Ord + Clone> OrSet<T> {
    pub fn new(replica: u32) -> Self {
        OrSet {
            replica,
            next: 0,
            added: BTreeSet::new(),
            removed: BTreeSet::new(),
        }
    }

    pub fn add(&mut self, item: T) {
        self.next += 1;
        self.added.insert((item, (self.replica, self.next)));
    }

    fn live_tags<'a>(&'a self, item: &'a T) -> impl Iterator<Item = Tag> + 'a {
        self.added
            .range((item.clone(), (0, 0))..=(item.clone(), (u32::MAX, u64::MAX)))
            .map(|(_, tag)| *tag)
            .filter(|tag| !self.removed.contains(tag))
    }

    pub fn remove(&mut self, item: &T) -> bool {
        let seen: Vec<Tag> = self.live_tags(item).collect();
        let was_present = !seen.is_empty();
        self.removed.extend(seen);
        was_present
    }

    pub fn merge(&mut self, other: &Self) {
        self.added.extend(other.added.iter().cloned());
        self.removed.extend(other.removed.iter().copied());
    }

    pub fn contains(&self, item: &T) -> bool {
        self.live_tags(item).next().is_some()
    }

    pub fn items(&self) -> Vec<T> {
        let mut out: Vec<T> = self
            .added
            .iter()
            .filter(|(_, tag)| !self.removed.contains(tag))
            .map(|(item, _)| item.clone())
            .collect();
        out.dedup();
        out
    }
}
