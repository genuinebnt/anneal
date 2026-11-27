use std::collections::{BTreeMap, BTreeSet, HashMap};
use tokio::sync::mpsc::{self, error::TrySendError, Receiver, Sender};

pub type MemberId = u64;

#[derive(Debug, PartialEq, Eq)]
pub struct Delivery {
    /// How many members now have the message in their queue.
    pub delivered: usize,
    /// Members removed by this call because their queue was full, in id order.
    pub evicted: Vec<MemberId>,
}

struct Member<M> {
    queue: Sender<M>,
    topics: BTreeSet<String>,
}

pub struct Hub<M> {
    last_id: MemberId,
    members: HashMap<MemberId, Member<M>>,
    topics: HashMap<String, BTreeMap<MemberId, ()>>,
}

impl<M: Clone> Hub<M> {
    pub fn new() -> Hub<M> {
        Hub {
            last_id: 0,
            members: HashMap::new(),
            topics: HashMap::new(),
        }
    }

    pub fn join(&mut self, capacity: usize) -> (MemberId, Receiver<M>) {
        let (queue, receiver) = mpsc::channel(capacity);
        self.last_id += 1;
        let member = Member {
            queue,
            topics: BTreeSet::new(),
        };
        self.members.insert(self.last_id, member);
        (self.last_id, receiver)
    }

    pub fn subscribe(&mut self, id: MemberId, topic: &str) -> Option<usize> {
        let member = self.members.get_mut(&id)?;
        member.topics.insert(topic.to_string());
        let count = member.topics.len();
        self.topics
            .entry(topic.to_string())
            .or_default()
            .insert(id, ());
        Some(count)
    }

    pub fn unsubscribe(&mut self, id: MemberId, topic: &str) -> Option<usize> {
        let member = self.members.get_mut(&id)?;
        member.topics.remove(topic);
        let count = member.topics.len();
        self.forget(id, topic);
        Some(count)
    }

    /// Takes `id` off one topic's list, and drops the list when it empties.
    fn forget(&mut self, id: MemberId, topic: &str) {
        if let Some(list) = self.topics.get_mut(topic) {
            list.remove(&id);
            if list.is_empty() {
                self.topics.remove(topic);
            }
        }
    }

    pub fn leave(&mut self, id: MemberId) -> bool {
        let Some(member) = self.members.remove(&id) else {
            return false;
        };
        for topic in &member.topics {
            self.forget(id, topic);
        }
        true
    }

    pub fn publish(&mut self, topic: &str, message: M) -> Delivery {
        let mut delivered = 0;
        let (mut evicted, mut closed) = (Vec::new(), Vec::new());
        for &id in self.topics.get(topic).into_iter().flat_map(|l| l.keys()) {
            let Some(member) = self.members.get(&id) else {
                continue;
            };
            match member.queue.try_send(message.clone()) {
                Ok(()) => delivered += 1,
                Err(TrySendError::Full(_)) => evicted.push(id),
                Err(TrySendError::Closed(_)) => closed.push(id),
            }
        }
        for &id in evicted.iter().chain(&closed) {
            self.forget(id, topic);
        }
        Delivery { delivered, evicted }
    }

    pub fn members(&self) -> usize {
        self.members.len()
    }

    pub fn topics(&self) -> usize {
        self.topics.len()
    }
}

impl<M: Clone> Default for Hub<M> {
    fn default() -> Self {
        Hub::new()
    }
}
