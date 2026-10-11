//! The times of a page's last `k` accesses: the history behind LRU-K.

use std::collections::VecDeque;

pub struct KHistory {
    k: usize,
    /// The times of the last `k` accesses, oldest first.
    times: VecDeque<u64>,
}

impl KHistory {
    /// `k` is at least 1.
    pub fn new(k: usize) -> KHistory {
        assert!(k >= 1, "k is at least 1");
        KHistory { k, times: VecDeque::new() }
    }

    /// An access at time `ts` (times never go backwards).
    pub fn record(&mut self, ts: u64) {
        self.times.push_back(ts);
        // @begin 1d-c2
        if self.times.len() > self.k {
            self.times.pop_front();
        }
        //~ // TODO(1d-c2): this looks right and has a bug; find it and fix it
        //~ if self.times.len() > self.k + 1 {
        //~     self.times.pop_front();
        //~ }
        // @end
    }

    /// The backward k-distance at time `now`: how long ago the k-th most recent access was. A page with fewer than `k` accesses has an infinite
    /// distance (`None`): it is the first to be evicted.
    pub fn k_distance(&self, now: u64) -> Option<u64> {
        if self.times.len() < self.k {
            return None;
        }
        self.times.front().map(|&oldest| now - oldest)
    }
}
