//! Batching commit acknowledgements behind one log flush.

use std::collections::VecDeque;

pub struct GroupCommit {
    _gc: (),
}

impl GroupCommit {
    /// An empty queue. A batch is released when `max_batch` commits wait (at least 1) or the oldest has waited `max_wait`.
    pub fn new(max_batch: usize, max_wait: u64) -> GroupCommit {
        todo!("4c-c2: an empty queue with the two limits")
    }

    /// Queues the commit with log sequence number `lsn` that arrived at time `now`.
    pub fn submit(&mut self, lsn: u64, now: u64) {
        todo!("4c-c2: queue the commit with the time it arrived")
    }

    /// The commits to acknowledge together, oldest first, once the queue holds `max_batch` of them or the oldest has waited `max_wait` by `now`;
    /// `None` before that. They leave the queue.
    pub fn poll(&mut self, now: u64) -> Option<Vec<u64>> {
        todo!("4c-c2: a batch when the queue is full enough or the oldest has waited long enough")
    }

    /// Takes everything that waits, oldest first.
    pub fn flush_all(&mut self) -> Vec<u64> {
        todo!("4c-c2: everything that waits")
    }

    /// The number of commits waiting.
    pub fn pending(&self) -> usize {
        todo!("4c-c2: how many commits wait")
    }
}
