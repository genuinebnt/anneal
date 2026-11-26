use std::hash::Hash;
use std::marker::PhantomData;

pub struct UrgencyQueue<K, P> {
    entries: PhantomData<(K, P)>,
}

impl<K: Hash + Eq + Clone, P: Ord + Clone> Default for UrgencyQueue<K, P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone, P: Ord + Clone> UrgencyQueue<K, P> {
    pub fn new() -> Self {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn push(&mut self, key: K, priority: P) -> bool {
        todo!()
    }

    pub fn pop(&mut self) -> Option<(K, P)> {
        todo!()
    }

    pub fn peek(&self) -> Option<(&K, &P)> {
        todo!()
    }

    pub fn priority(&self, key: &K) -> Option<&P> {
        todo!()
    }

    pub fn change_priority(&mut self, key: &K, priority: P) -> Option<P> {
        todo!()
    }

    pub fn remove(&mut self, key: &K) -> Option<P> {
        todo!()
    }
}
