use std::collections::BTreeMap;

pub struct IntervalMap<V> {
    /// start → (end, value)
    by_start: BTreeMap<u32, (u32, V)>,
}

impl<V> IntervalMap<V> {
    pub fn new() -> Self {
        IntervalMap {
            by_start: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, start: u32, end: u32, value: V) -> Result<(), V> {
        if start >= end {
            return Err(value);
        }
        let before_overlaps = self
            .by_start
            .range(..=start)
            .next_back()
            .is_some_and(|(_, (e, _))| *e > start);
        let after_overlaps = self
            .by_start
            .range(start..)
            .next()
            .is_some_and(|(s, _)| *s < end);
        if before_overlaps || after_overlaps {
            return Err(value);
        }
        self.by_start.insert(start, (end, value));
        Ok(())
    }

    pub fn get(&self, point: u32) -> Option<&V> {
        self.by_start
            .range(..=point)
            .next_back()
            .filter(|(_, (end, _))| point < *end)
            .map(|(_, (_, v))| v)
    }
}

impl<V> Default for IntervalMap<V> {
    fn default() -> Self {
        Self::new()
    }
}
