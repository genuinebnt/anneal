use std::collections::HashMap;
use std::hash::Hash;

pub struct UrgencyQueue<K, P> {
    heap: Vec<(P, u64, u64, K)>,
    live: HashMap<K, (P, u64, u64)>,
    stamp: u64,
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
            live: HashMap::new(),
            stamp: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.live.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, key: K, priority: P) -> bool {
        if self.live.contains_key(&key) {
            return false;
        }
        self.stamp += 1;
        self.live
            .insert(key.clone(), (priority.clone(), self.stamp, self.stamp));
        self.heap_push((priority, self.stamp, self.stamp, key));
        true
    }

    pub fn pop(&mut self) -> Option<(K, P)> {
        self.clean();
        let (p, _, _, key) = self.heap_pop()?;
        self.live.remove(&key);
        self.clean();
        Some((key, p))
    }

    pub fn peek(&self) -> Option<(&K, &P)> {
        self.heap.first().map(|e| (&e.3, &e.0))
    }

    pub fn priority(&self, key: &K) -> Option<&P> {
        self.live.get(key).map(|e| &e.0)
    }

    pub fn change_priority(&mut self, key: &K, priority: P) -> Option<P> {
        let entry = self.live.get_mut(key)?;
        self.stamp += 1;
        let old = std::mem::replace(&mut entry.0, priority.clone());
        entry.2 = self.stamp;
        let pushed = entry.1;
        self.heap_push((priority, pushed, self.stamp, key.clone()));
        self.clean();
        Some(old)
    }

    pub fn remove(&mut self, key: &K) -> Option<P> {
        let entry = self.live.remove(key)?;
        self.clean();
        Some(entry.0)
    }

    fn clean(&mut self) {
        while self
            .heap
            .first()
            .is_some_and(|e| self.live.get(&e.3).is_none_or(|l| l.2 != e.2))
        {
            self.heap_pop();
        }
    }

    fn less(&self, a: usize, b: usize) -> bool {
        (&self.heap[a].0, self.heap[a].1) < (&self.heap[b].0, self.heap[b].1)
    }

    fn heap_push(&mut self, e: (P, u64, u64, K)) {
        self.heap.push(e);
        let mut i = self.heap.len() - 1;
        while i > 0 && self.less(i, (i - 1) / 2) {
            self.heap.swap(i, (i - 1) / 2);
            i = (i - 1) / 2;
        }
    }

    fn heap_pop(&mut self) -> Option<(P, u64, u64, K)> {
        if self.heap.is_empty() {
            return None;
        }
        let top = self.heap.swap_remove(0);
        let mut i = 0;
        loop {
            let mut best = i;
            for c in [2 * i + 1, 2 * i + 2] {
                if c < self.heap.len() && self.less(c, best) {
                    best = c;
                }
            }
            if best == i {
                break;
            }
            self.heap.swap(i, best);
            i = best;
        }
        Some(top)
    }
}
