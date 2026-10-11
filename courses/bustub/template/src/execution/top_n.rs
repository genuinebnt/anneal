//! ORDER BY ... LIMIT n with memory for n rows.

use std::collections::BinaryHeap;

pub struct TopN {
    _topn: (),
}

impl TopN {
    /// A selector of the `n` smallest keys pushed to it; among equal keys the earliest arrival wins.
    pub fn new(n: usize) -> TopN {
        todo!("3g-c2: an empty selector for `n` rows")
    }

    /// Offers a row. It is kept if fewer than `n` rows are held or it sorts before the worst one held, which is then dropped.
    pub fn push(&mut self, key: i64, payload: u32) {
        todo!("3g-c2: keep the `n` best seen so far")
    }

    /// The number of rows held, at most `n`.
    pub fn len(&self) -> usize {
        todo!("3g-c2: rows held")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The held rows in key order, earlier arrivals first among equal keys.
    pub fn finish(self) -> Vec<(i64, u32)> {
        todo!("3g-c2: the held rows in key order, earlier arrivals first among equals")
    }
}
