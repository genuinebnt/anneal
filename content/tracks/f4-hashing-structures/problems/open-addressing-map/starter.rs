use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};

/// An open-addressing hash map: one flat array, linear probing, tombstones for removal.
/// The capacity is a power of two (starting at 8); keep the table under 7/8 full, counting tombstones.
/// Probe from `hash & (capacity - 1)` to the next slot (wrapping) until you find the key or an empty slot.
pub struct OaMap<K, V, S = RandomState> {
    // TODO: a Vec of slots (empty / tombstone / full), the live count, the tombstone count, the hasher
    _marker: std::marker::PhantomData<(K, V, S)>,
}

impl<K: Hash + Eq, V> OaMap<K, V, RandomState> {
    pub fn new() -> Self {
        todo!()
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> OaMap<K, V, S> {
    pub fn with_hasher(hasher: S) -> Self {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Number of slots (always a power of two).
    pub fn capacity(&self) -> usize {
        todo!()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        todo!()
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        todo!()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        todo!()
    }

    /// Inserts or replaces; returns the old value. Removed entries leave tombstones that `insert` may reuse.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        todo!()
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        todo!()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        std::iter::empty()
    }
}
