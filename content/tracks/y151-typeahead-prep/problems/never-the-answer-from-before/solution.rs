use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// A piece of work in progress.
struct Making<K> {
    key: K,
    outdated: bool,
}

struct State<K, V> {
    stored: HashMap<K, V>,
    making: HashMap<u64, Making<K>>,
    next: u64,
}

pub struct Shelf<K, V> {
    state: Mutex<State<K, V>>,
}

/// Takes the work off the list when it ends, even by panic.
struct Ticket<'a, K: Eq + Hash, V> {
    shelf: &'a Shelf<K, V>,
    number: u64,
}

impl<K: Eq + Hash, V> Drop for Ticket<'_, K, V> {
    fn drop(&mut self) {
        let mut state = self
            .shelf
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.making.remove(&self.number);
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Shelf<K, V> {
    pub fn new() -> Shelf<K, V> {
        Shelf {
            state: Mutex::new(State {
                stored: HashMap::new(),
                making: HashMap::new(),
                next: 0,
            }),
        }
    }

    // `touches` runs under the lock and may panic; the state stays consistent if it does
    fn lock(&self) -> MutexGuard<'_, State<K, V>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The stored answer and `true`, or the result of `work` and `false`.
    pub fn get_or_work_out(&self, key: &K, work: impl FnOnce() -> V) -> (V, bool) {
        let ticket = {
            let mut state = self.lock();
            if let Some(answer) = state.stored.get(key) {
                return (answer.clone(), true);
            }
            let number = state.next;
            state.next += 1;
            let making = Making {
                key: key.clone(),
                outdated: false,
            };
            state.making.insert(number, making);
            Ticket {
                shelf: self,
                number,
            }
        };
        let answer = work(); // no lock is held here
        {
            // checking the mark and storing happen under one lock, so no change can fall between them
            let mut state = self.lock();
            if state
                .making
                .get(&ticket.number)
                .is_some_and(|m| !m.outdated)
            {
                state.stored.insert(key.clone(), answer.clone());
            }
        }
        (answer, false)
    }

    /// The facts have changed. `touches(key)` says whether the answer for `key` may be different now.
    pub fn changed(&self, touches: impl Fn(&K) -> bool) {
        let mut state = self.lock();
        state.stored.retain(|key, _| !touches(key));
        for making in state.making.values_mut() {
            if touches(&making.key) {
                making.outdated = true;
            }
        }
    }
}
