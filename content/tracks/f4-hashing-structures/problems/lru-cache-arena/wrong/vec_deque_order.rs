use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

#[derive(Debug, PartialEq, Eq)]
pub enum Put<K, V> {
    Inserted,
    Replaced(V),
    Evicted(K, V),
}

/// Correct, but every use searches the order from one end.
pub struct LruCache<K, V> {
    capacity: usize,
    values: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Hash + Eq + Clone, V> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        LruCache {
            capacity,
            values: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    fn forget(&mut self, key: &K) {
        if let Some(i) = self.order.iter().position(|k| k == key) {
            self.order.remove(i);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.values.contains_key(key) {
            self.forget(key);
            self.order.push_front(key.clone());
        }
        self.values.get(key)
    }

    pub fn peek(&self, key: &K) -> Option<&V> {
        self.values.get(key)
    }

    pub fn put(&mut self, key: K, value: V) -> Put<K, V> {
        if self.values.contains_key(&key) {
            self.forget(&key);
            self.order.push_front(key.clone());
            return Put::Replaced(self.values.insert(key, value).unwrap());
        }
        if self.capacity == 0 {
            return Put::Evicted(key, value);
        }
        let mut out = Put::Inserted;
        if self.values.len() == self.capacity {
            let old = self.order.pop_back().unwrap();
            let v = self.values.remove(&old).unwrap();
            out = Put::Evicted(old, v);
        }
        self.order.push_front(key.clone());
        self.values.insert(key, value);
        out
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let v = self.values.remove(key)?;
        self.forget(key);
        Some(v)
    }

    pub fn keys_by_recency(&self) -> Vec<K> {
        self.order.iter().cloned().collect()
    }
}
