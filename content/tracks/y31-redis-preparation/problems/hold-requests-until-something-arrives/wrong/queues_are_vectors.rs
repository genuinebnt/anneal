use std::collections::{HashMap, VecDeque};

pub struct Waiting<T> {
    waiters: HashMap<u64, (u64, Vec<String>, Option<u64>, T)>,
    queues: HashMap<String, VecDeque<u64>>,
    parked: u64,
}

impl<T> Waiting<T> {
    pub fn new() -> Waiting<T> {
        Waiting { waiters: HashMap::new(), queues: HashMap::new(), parked: 0 }
    }
    pub fn park(&mut self, id: u64, wants: &[&str], deadline: Option<u64>, ticket: T) -> bool {
        if self.waiters.contains_key(&id) {
            return false;
        }
        let mut unique: Vec<String> = wants.iter().map(|w| w.to_string()).collect();
        unique.sort();
        unique.dedup();
        for w in &unique {
            self.queues.entry(w.clone()).or_default().push_back(id);
        }
        self.parked += 1;
        self.waiters.insert(id, (self.parked, unique, deadline, ticket));
        true
    }
    pub fn cancel(&mut self, id: u64) -> Option<T> {
        let (_, wants, _, ticket) = self.waiters.remove(&id)?;
        for w in &wants {
            if let Some(q) = self.queues.get_mut(w) {
                q.retain(|&x| x != id);
            }
        }
        Some(ticket)
    }
    pub fn wake(&mut self, item: &str) -> Option<(u64, T)> {
        let id = *self.queues.get(item)?.front()?;
        self.cancel(id).map(|t| (id, t))
    }
    pub fn expire(&mut self, now: u64) -> Vec<(u64, T)> {
        let mut due: Vec<(u64, u64, u64)> = self
            .waiters
            .iter()
            .filter_map(|(&id, w)| w.2.filter(|&d| d <= now).map(|d| (d, w.0, id)))
            .collect();
        due.sort();
        due.into_iter().filter_map(|(_, _, id)| self.cancel(id).map(|t| (id, t))).collect()
    }
    pub fn next_deadline(&self) -> Option<u64> {
        self.waiters.values().filter_map(|w| w.2).min()
    }
    pub fn contains(&self, id: u64) -> bool {
        self.waiters.contains_key(&id)
    }
    pub fn len(&self) -> usize {
        self.waiters.len()
    }
    pub fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }
}

impl<T> Default for Waiting<T> {
    fn default() -> Self {
        Waiting::new()
    }
}
