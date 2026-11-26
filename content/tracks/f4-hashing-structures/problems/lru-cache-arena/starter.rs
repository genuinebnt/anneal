use std::hash::Hash;
use std::marker::PhantomData;

#[derive(Debug, PartialEq, Eq)]
pub enum Put<K, V> {
    Inserted,
    Replaced(V),
    Evicted(K, V),
}

pub struct LruCache<K, V> {
    capacity: usize,
    entries: PhantomData<(K, V)>,
}

impl<K: Hash + Eq + Clone, V> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        LruCache {
            capacity,
            entries: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        todo!()
    }

    pub fn peek(&self, key: &K) -> Option<&V> {
        todo!()
    }

    pub fn put(&mut self, key: K, value: V) -> Put<K, V> {
        todo!()
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        todo!()
    }

    pub fn keys_by_recency(&self) -> Vec<K> {
        todo!()
    }
}
