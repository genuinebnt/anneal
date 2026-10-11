//! Batching commit acknowledgements behind one log flush.

use std::collections::VecDeque;

pub struct GroupCommit {
    // @begin 4c-c2
    max_batch: usize,
    max_wait: u64,
    /// (lsn, submitted at), oldest first.
    queue: VecDeque<(u64, u64)>,
    //~ _gc: (),
    // @end
}

impl GroupCommit {
    /// An empty queue. A batch is released when `max_batch` commits wait (at least 1) or the oldest has waited `max_wait`.
    pub fn new(max_batch: usize, max_wait: u64) -> GroupCommit {
        // @begin 4c-c2
        GroupCommit { max_batch: max_batch.max(1), max_wait, queue: VecDeque::new() }
        //~ todo!("4c-c2: an empty queue with the two limits")
        // @end
    }

    /// Queues the commit with log sequence number `lsn` that arrived at time `now`.
    pub fn submit(&mut self, lsn: u64, now: u64) {
        // @begin 4c-c2
        self.queue.push_back((lsn, now));
        //~ todo!("4c-c2: queue the commit with the time it arrived")
        // @end
    }

    /// The commits to acknowledge together, oldest first, once the queue holds `max_batch` of them or the oldest has waited `max_wait` by `now`;
    /// `None` before that. They leave the queue.
    pub fn poll(&mut self, now: u64) -> Option<Vec<u64>> {
        // @begin 4c-c2
        let (_, oldest) = *self.queue.front()?;
        if self.queue.len() >= self.max_batch || now.saturating_sub(oldest) >= self.max_wait {
            Some(self.queue.drain(..).map(|(l, _)| l).collect())
        } else {
            None
        }
        //~ todo!("4c-c2: a batch when the queue is full enough or the oldest has waited long enough")
        // @end
    }

    /// Takes everything that waits, oldest first.
    pub fn flush_all(&mut self) -> Vec<u64> {
        // @begin 4c-c2
        self.queue.drain(..).map(|(l, _)| l).collect()
        //~ todo!("4c-c2: everything that waits")
        // @end
    }

    /// The number of commits waiting.
    pub fn pending(&self) -> usize {
        // @begin 4c-c2
        self.queue.len()
        //~ todo!("4c-c2: how many commits wait")
        // @end
    }
}
