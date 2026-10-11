//! A lazy k-way merge of sorted runs.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct KMerge {
    // @begin 2c-c4
    runs: Vec<std::vec::IntoIter<i64>>,
    heap: BinaryHeap<Reverse<(i64, usize)>>,
    //~ _merge: (),
    // @end
}

impl KMerge {
    /// A merge over sorted `runs`, with a heap that holds the next key of each run.
    pub fn new(runs: Vec<Vec<i64>>) -> KMerge {
        // @begin 2c-c4
        let mut iters: Vec<_> = runs.into_iter().map(|r| r.into_iter()).collect();
        let mut heap = BinaryHeap::new();
        for (i, it) in iters.iter_mut().enumerate() {
            if let Some(k) = it.next() {
                heap.push(Reverse((k, i)));
            }
        }
        KMerge { runs: iters, heap }
        //~ todo!("2c-c4: a heap holding the first key of each run")
        // @end
    }
}

impl Iterator for KMerge {
    type Item = (i64, usize);

    fn next(&mut self) -> Option<(i64, usize)> {
        // @begin 2c-c4
        let Reverse((key, run)) = self.heap.pop()?;
        if let Some(next) = self.runs[run].next() {
            self.heap.push(Reverse((next, run)));
        }
        Some((key, run))
        //~ todo!("2c-c4: pop the smallest, refill from the run it came from")
        // @end
    }
}
