use std::hash::Hash;
use std::marker::PhantomData;

pub struct Shelf<K, V> {
    // your fields; `entries` only lets the starter compile
    entries: PhantomData<std::sync::Mutex<(K, V)>>,
}

impl<K: Eq + Hash + Clone, V: Clone> Shelf<K, V> {
    pub fn new() -> Shelf<K, V> {
        todo!()
    }

    /// The stored answer and `true`, or the result of `work` and `false`.
    pub fn get_or_work_out(&self, key: &K, work: impl FnOnce() -> V) -> (V, bool) {
        todo!()
    }

    /// The facts have changed. `touches(key)` says whether the answer for `key` may be different now.
    pub fn changed(&self, touches: impl Fn(&K) -> bool) {
        todo!()
    }
}
