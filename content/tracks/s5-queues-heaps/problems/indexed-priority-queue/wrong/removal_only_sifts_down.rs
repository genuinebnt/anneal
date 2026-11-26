use std::collections::HashMap;
use std::hash::Hash;

struct Entry<K, P> {
    priority: P,
    pushed: u64,
    key: K,
}

pub struct UrgencyQueue<K, P> {
    heap: Vec<Entry<K, P>>,
    position: HashMap<K, usize>,
    pushes: u64,
}

impl<K: Hash + Eq + Clone, P: Ord + Clone> Default for UrgencyQueue<K, P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone, P: Ord + Clone> UrgencyQueue<K, P> {
    pub fn new() -> Self {
        UrgencyQueue {
            heap: Vec::new(),
            position: HashMap::new(),
            pushes: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, key: K, priority: P) -> bool {
        if self.position.contains_key(&key) {
            return false;
        }
        let last = self.heap.len();
        self.position.insert(key.clone(), last);
        self.heap.push(Entry {
            priority,
            pushed: self.pushes,
            key,
        });
        self.pushes += 1;
        self.sift_up(last);
        true
    }

    pub fn pop(&mut self) -> Option<(K, P)> {
        if self.heap.is_empty() {
            return None;
        }
        let entry = self.take(0);
        Some((entry.key, entry.priority))
    }

    pub fn peek(&self) -> Option<(&K, &P)> {
        self.heap.first().map(|e| (&e.key, &e.priority))
    }

    pub fn priority(&self, key: &K) -> Option<&P> {
        self.position.get(key).map(|&i| &self.heap[i].priority)
    }

    pub fn change_priority(&mut self, key: &K, priority: P) -> Option<P> {
        let i = *self.position.get(key)?;
        let old = std::mem::replace(&mut self.heap[i].priority, priority);
        self.repair(i);
        Some(old)
    }

    pub fn remove(&mut self, key: &K) -> Option<P> {
        let i = *self.position.get(key)?;
        Some(self.take(i).priority)
    }

    /// Takes the entry at `i` out and restores both structures.
    fn take(&mut self, i: usize) -> Entry<K, P> {
        let last = self.heap.len() - 1;
        self.swap(i, last);
        let entry = self.heap.remove(last);
        self.position.remove(&entry.key);
        if i < self.heap.len() {
            self.sift_down(i);
        }
        entry
    }

    fn repair(&mut self, i: usize) {
        let i = self.sift_up(i);
        self.sift_down(i);
    }

    fn before(&self, a: usize, b: usize) -> bool {
        let (a, b) = (&self.heap[a], &self.heap[b]);
        (&a.priority, a.pushed) < (&b.priority, b.pushed)
    }

    /// The only place that reorders entries, so the positions stay right.
    fn swap(&mut self, a: usize, b: usize) {
        self.heap.swap(a, b);
        for i in [a, b] {
            if let Some(at) = self.position.get_mut(&self.heap[i].key) {
                *at = i;
            }
        }
    }

    fn sift_up(&mut self, mut i: usize) -> usize {
        while i > 0 && self.before(i, (i - 1) / 2) {
            self.swap(i, (i - 1) / 2);
            i = (i - 1) / 2;
        }
        i
    }

    fn sift_down(&mut self, mut i: usize) {
        loop {
            let mut best = i;
            for child in [2 * i + 1, 2 * i + 2] {
                if child < self.heap.len() && self.before(child, best) {
                    best = child;
                }
            }
            if best == i {
                return;
            }
            self.swap(i, best);
            i = best;
        }
    }
}
