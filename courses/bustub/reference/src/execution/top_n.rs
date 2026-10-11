//! ORDER BY ... LIMIT n with memory for n rows.

use std::collections::BinaryHeap;

pub struct TopN {
    // @begin 3g-c2
    n: usize,
    /// A max-heap on (key, arrival): the worst row held is on top.
    heap: BinaryHeap<(i64, u64, u32)>,
    arrivals: u64,
    //~ _topn: (),
    // @end
}

impl TopN {
    /// A selector of the `n` smallest keys pushed to it; among equal keys the earliest arrival wins.
    pub fn new(n: usize) -> TopN {
        // @begin 3g-c2
        TopN { n, heap: BinaryHeap::new(), arrivals: 0 }
        //~ todo!("3g-c2: an empty selector for `n` rows")
        // @end
    }

    /// Offers a row. It is kept if fewer than `n` rows are held or it sorts before the worst one held, which is then dropped.
    pub fn push(&mut self, key: i64, payload: u32) {
        // @begin 3g-c2
        let item = (key, self.arrivals, payload);
        self.arrivals += 1;
        if self.n == 0 {
            return;
        }
        if self.heap.len() < self.n {
            self.heap.push(item);
        } else if let Some(worst) = self.heap.peek() {
            if (item.0, item.1) < (worst.0, worst.1) {
                self.heap.pop();
                self.heap.push(item);
            }
        }
        //~ todo!("3g-c2: keep the `n` best seen so far")
        // @end
    }

    /// The number of rows held, at most `n`.
    pub fn len(&self) -> usize {
        // @begin 3g-c2
        self.heap.len()
        //~ todo!("3g-c2: rows held")
        // @end
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The held rows in key order, earlier arrivals first among equal keys.
    pub fn finish(self) -> Vec<(i64, u32)> {
        // @begin 3g-c2
        let mut v = self.heap.into_vec();
        v.sort();
        v.into_iter().map(|(k, _, p)| (k, p)).collect()
        //~ todo!("3g-c2: the held rows in key order, earlier arrivals first among equals")
        // @end
    }
}
