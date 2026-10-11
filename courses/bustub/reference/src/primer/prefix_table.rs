//! Longest-prefix matching over bit strings.

use std::collections::BTreeMap;

pub struct PrefixTable<V> {
    // @begin 0a-c1
    entries: BTreeMap<String, V>,
    //~ _table: std::marker::PhantomData<V>,
    // @end
}

impl<V> PrefixTable<V> {
    /// An empty table.
    pub fn new() -> PrefixTable<V> {
        // @begin 0a-c1
        PrefixTable { entries: BTreeMap::new() }
        //~ todo!("0a-c1: an empty table")
        // @end
    }

    /// Stores `value` for `prefix`, returning the value it replaced.
    pub fn insert(&mut self, prefix: &str, value: V) -> Option<V> {
        // @begin 0a-c1
        self.entries.insert(prefix.to_owned(), value)
        //~ todo!("0a-c1: store the entry, returning the value it replaced")
        // @end
    }

    /// Deletes the entry for `prefix`, returning its value.
    pub fn remove(&mut self, prefix: &str) -> Option<V> {
        // @begin 0a-c1
        self.entries.remove(prefix)
        //~ todo!("0a-c1: delete the entry")
        // @end
    }

    /// The number of entries.
    pub fn len(&self) -> usize {
        // @begin 0a-c1
        self.entries.len()
        //~ todo!("0a-c1: how many entries")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The entry with the longest prefix of `address`.
    pub fn lookup(&self, address: &str) -> Option<(&str, &V)> {
        // @begin 0a-c1
        (0..=address.len()).rev().find_map(|n| self.entries.get_key_value(&address[..n]).map(|(k, v)| (k.as_str(), v)))
        //~ todo!("0a-c1: try the longest prefix of the address first, then shorter ones")
        // @end
    }
}

impl<V> Default for PrefixTable<V> {
    fn default() -> Self {
        PrefixTable::new()
    }
}
