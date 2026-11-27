use std::marker::PhantomData;
use tokio::sync::mpsc::Receiver;

pub type MemberId = u64;

#[derive(Debug, PartialEq, Eq)]
pub struct Delivery {
    /// How many members now have the message in their queue.
    pub delivered: usize,
    /// Members removed by this call because their queue was full, in id order.
    pub evicted: Vec<MemberId>,
}

pub struct Hub<M> {
    // replace with your own fields
    messages: PhantomData<M>,
}

impl<M: Clone> Hub<M> {
    pub fn new() -> Hub<M> {
        Hub {
            messages: PhantomData,
        }
    }

    /// A new member whose queue holds at most `capacity` messages (at least 1).
    pub fn join(&mut self, capacity: usize) -> (MemberId, Receiver<M>) {
        todo!()
    }

    /// The number of topics the member is on afterwards.
    pub fn subscribe(&mut self, id: MemberId, topic: &str) -> Option<usize> {
        todo!()
    }

    pub fn unsubscribe(&mut self, id: MemberId, topic: &str) -> Option<usize> {
        todo!()
    }

    /// Removes the member. `false` if there is none.
    pub fn leave(&mut self, id: MemberId) -> bool {
        todo!()
    }

    /// Sends `message` to everyone on `topic`, without ever waiting.
    pub fn publish(&mut self, topic: &str, message: M) -> Delivery {
        todo!()
    }

    /// How many members the hub has.
    pub fn members(&self) -> usize {
        todo!()
    }

    /// How many topics have at least one member.
    pub fn topics(&self) -> usize {
        todo!()
    }
}

impl<M: Clone> Default for Hub<M> {
    fn default() -> Self {
        Hub::new()
    }
}
