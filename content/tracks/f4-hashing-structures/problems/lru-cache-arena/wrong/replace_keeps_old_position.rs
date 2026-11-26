use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, PartialEq, Eq)]
pub enum Put<K, V> {
    Inserted,
    Replaced(V),
    Evicted(K, V),
}

struct Node<K, V> {
    key: K,
    value: V,
    newer: Option<usize>,
    older: Option<usize>,
}

pub struct LruCache<K, V> {
    capacity: usize,
    slots: Vec<Option<Node<K, V>>>,
    free: Vec<usize>,
    index: HashMap<K, usize>,
    newest: Option<usize>,
    oldest: Option<usize>,
}

impl<K: Hash + Eq + Clone, V> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        LruCache {
            capacity,
            slots: Vec::new(),
            free: Vec::new(),
            index: HashMap::new(),
            newest: None,
            oldest: None,
        }
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    fn node(&self, i: usize) -> &Node<K, V> {
        self.slots[i].as_ref().expect("linked slot is occupied")
    }

    fn node_mut(&mut self, i: usize) -> &mut Node<K, V> {
        self.slots[i].as_mut().expect("linked slot is occupied")
    }

    fn unlink(&mut self, i: usize) {
        let (newer, older) = (self.node(i).newer, self.node(i).older);
        match newer {
            Some(n) => self.node_mut(n).older = older,
            None => self.newest = older,
        }
        match older {
            Some(o) => self.node_mut(o).newer = newer,
            None => self.oldest = newer,
        }
    }

    fn push_newest(&mut self, i: usize) {
        let next = self.newest;
        let node = self.node_mut(i);
        node.newer = None;
        node.older = next;
        match next {
            Some(n) => self.node_mut(n).newer = Some(i),
            None => self.oldest = Some(i),
        }
        self.newest = Some(i);
    }

    fn touch(&mut self, i: usize) {
        if self.newest != Some(i) {
            self.unlink(i);
            self.push_newest(i);
        }
    }

    /// Unlinks slot `i`, frees it and hands the node back.
    fn take(&mut self, i: usize) -> Node<K, V> {
        self.unlink(i);
        self.free.push(i);
        self.slots[i].take().expect("linked slot is occupied")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let i = *self.index.get(key)?;
        self.touch(i);
        Some(&self.node(i).value)
    }

    pub fn peek(&self, key: &K) -> Option<&V> {
        self.index.get(key).map(|&i| &self.node(i).value)
    }

    pub fn put(&mut self, key: K, value: V) -> Put<K, V> {
        if let Some(&i) = self.index.get(&key) {
            return Put::Replaced(std::mem::replace(&mut self.node_mut(i).value, value));
        }
        if self.capacity == 0 {
            return Put::Evicted(key, value);
        }
        let evicted = match self.oldest {
            Some(old) if self.len() == self.capacity => {
                let node = self.take(old);
                self.index.remove(&node.key);
                Some(node)
            }
            _ => None,
        };
        let node = Node {
            key: key.clone(),
            value,
            newer: None,
            older: None,
        };
        let i = match self.free.pop() {
            Some(i) => {
                self.slots[i] = Some(node);
                i
            }
            None => {
                self.slots.push(Some(node));
                self.slots.len() - 1
            }
        };
        self.index.insert(key, i);
        self.push_newest(i);
        match evicted {
            Some(node) => Put::Evicted(node.key, node.value),
            None => Put::Inserted,
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let i = self.index.remove(key)?;
        Some(self.take(i).value)
    }

    pub fn keys_by_recency(&self) -> Vec<K> {
        std::iter::successors(self.newest, |&i| self.node(i).older)
            .map(|i| self.node(i).key.clone())
            .collect()
    }
}
