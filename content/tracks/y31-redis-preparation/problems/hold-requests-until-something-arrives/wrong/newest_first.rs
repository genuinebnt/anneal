use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug)]
struct Waiter<T> {
    /// When it started waiting, as a counter.
    order: u64,
    wants: Vec<String>,
    deadline: Option<u64>,
    ticket: T,
}

/// Requests that wait for the first of several items, or until a deadline.
#[derive(Debug)]
pub struct Waiting<T> {
    parked: u64,
    waiters: HashMap<u64, Waiter<T>>,
    /// For each item, its waiters by the order they came.
    queues: HashMap<String, BTreeMap<u64, u64>>,
    deadlines: BTreeSet<(u64, u64, u64)>,
}

impl<T> Waiting<T> {
    pub fn new() -> Waiting<T> {
        Waiting {
            parked: 0,
            waiters: HashMap::new(),
            queues: HashMap::new(),
            deadlines: BTreeSet::new(),
        }
    }

    pub fn park(&mut self, id: u64, wants: &[&str], deadline: Option<u64>, ticket: T) -> bool {
        if self.waiters.contains_key(&id) {
            return false;
        }
        let order = self.parked;
        self.parked += 1;
        let wants: Vec<String> = wants.iter().map(|w| w.to_string()).collect();
        for item in &wants {
            self.queues
                .entry(item.clone())
                .or_default()
                .insert(order, id);
        }
        if let Some(at) = deadline {
            self.deadlines.insert((at, order, id));
        }
        let waiter = Waiter {
            order,
            wants,
            deadline,
            ticket,
        };
        self.waiters.insert(id, waiter);
        true
    }

    pub fn cancel(&mut self, id: u64) -> Option<T> {
        let waiter = self.waiters.remove(&id)?;
        for item in &waiter.wants {
            if let Some(queue) = self.queues.get_mut(item) {
                queue.remove(&waiter.order);
                if queue.is_empty() {
                    self.queues.remove(item);
                }
            }
        }
        if let Some(at) = waiter.deadline {
            self.deadlines.remove(&(at, waiter.order, id));
        }
        Some(waiter.ticket)
    }

    pub fn wake(&mut self, item: &str) -> Option<(u64, T)> {
        let id = *self.queues.get(item)?.values().next_back()?;
        self.cancel(id).map(|ticket| (id, ticket))
    }

    pub fn expire(&mut self, now: u64) -> Vec<(u64, T)> {
        let mut out = Vec::new();
        while let Some(&(at, _, id)) = self.deadlines.first() {
            if at > now {
                break;
            }
            if let Some(ticket) = self.cancel(id) {
                out.push((id, ticket));
            }
        }
        out
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.deadlines.first().map(|&(at, _, _)| at)
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
