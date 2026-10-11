//! Longest-prefix matching over bit strings.

use std::collections::BTreeMap;

pub struct PrefixTable<V> {
    _table: std::marker::PhantomData<V>,
}

impl<V> PrefixTable<V> {
    /// An empty table.
    pub fn new() -> PrefixTable<V> {
        todo!("0a-c1: an empty table")
    }

    /// Stores `value` for `prefix`, returning the value it replaced.
    pub fn insert(&mut self, prefix: &str, value: V) -> Option<V> {
        todo!("0a-c1: store the entry, returning the value it replaced")
    }

    /// Deletes the entry for `prefix`, returning its value.
    pub fn remove(&mut self, prefix: &str) -> Option<V> {
        todo!("0a-c1: delete the entry")
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        todo!("0a-c1: how many entries")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The entry with the longest prefix of `address`.
    pub fn lookup(&self, address: &str) -> Option<(&str, &V)> {
        todo!("0a-c1: try the longest prefix of the address first, then shorter ones")
    }
}

impl<V> Default for PrefixTable<V> {
    fn default() -> Self {
        PrefixTable::new()
    }
}
