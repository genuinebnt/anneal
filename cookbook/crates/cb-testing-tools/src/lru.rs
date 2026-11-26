//! Bounded LRU map. Implemented with a HashMap + logical clock (O(n) eviction scan), so the
//! model-based test's oracle (a plain ordered Vec) is a genuinely different implementation.
use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug)]
pub struct LruMap<K, V> {
    cap: usize,
    tick: u64,
    map: HashMap<K, (V, u64)>,
}

impl<K: Hash + Eq + Clone, V> LruMap<K, V> {
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "capacity must be positive");
        LruMap {
            cap,
            tick: 0,
            map: HashMap::new(),
        }
    }
    fn touch(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    /// Reads and marks as most recently used.
    pub fn get(&mut self, k: &K) -> Option<&V> {
        let t = self.touch();
        self.map.get_mut(k).map(|(v, stamp)| {
            *stamp = t;
            &*v
        })
    }
    /// Inserts/overwrites (marking MRU); returns the evicted entry if the map was full.
    pub fn put(&mut self, k: K, v: V) -> Option<(K, V)> {
        let t = self.touch();
        let mut evicted = None;
        if !self.map.contains_key(&k) && self.map.len() == self.cap {
            let oldest = self
                .map
                .iter()
                .min_by_key(|(_, (_, s))| *s)
                .map(|(k, _)| k.clone());
            if let Some(ok) = oldest {
                evicted = self.map.remove(&ok).map(|(v, _)| (ok, v));
            }
        }
        self.map.insert(k, (v, t));
        evicted
    }
    pub fn remove(&mut self, k: &K) -> Option<V> {
        self.map.remove(k).map(|(v, _)| v)
    }
    /// Keys from most to least recently used.
    pub fn keys_mru(&self) -> Vec<K> {
        let mut e: Vec<_> = self.map.iter().map(|(k, (_, s))| (*s, k.clone())).collect();
        e.sort_by(|a, b| b.0.cmp(&a.0));
        e.into_iter().map(|(_, k)| k).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evicts_least_recently_used() {
        let mut m = LruMap::new(2);
        m.put("a", 1);
        m.put("b", 2);
        m.get(&"a");
        assert_eq!(m.put("c", 3), Some(("b", 2)));
        assert_eq!(m.keys_mru(), vec!["c", "a"]);
    }
}
