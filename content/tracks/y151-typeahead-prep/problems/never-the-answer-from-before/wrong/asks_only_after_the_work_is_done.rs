use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;

pub struct Shelf<K, V> {
    state: Mutex<(HashMap<K, V>, u64)>,
}

impl<K: Eq + Hash + Clone, V: Clone> Shelf<K, V> {
    pub fn new() -> Shelf<K, V> {
        Shelf {
            state: Mutex::new((HashMap::new(), 0)),
        }
    }

    pub fn get_or_work_out(&self, key: &K, work: impl FnOnce() -> V) -> (V, bool) {
        if let Some(answer) = self.state.lock().unwrap().0.get(key) {
            return (answer.clone(), true);
        }
        let answer = work();
        let before = self.state.lock().unwrap().1;
        let mut state = self.state.lock().unwrap();
        if state.1 == before {
            state.0.insert(key.clone(), answer.clone());
        }
        (answer, false)
    }

    pub fn changed(&self, touches: impl Fn(&K) -> bool) {
        let mut state = self.state.lock().unwrap();
        state.0.retain(|key, _| !touches(key));
        state.1 += 1;
    }
}
