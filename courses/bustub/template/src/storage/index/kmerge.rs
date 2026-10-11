//! A lazy k-way merge of sorted runs.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub struct KMerge {
    _merge: (),
}

impl KMerge {
    /// A merge over sorted `runs`, with a heap that holds the next key of each run.
    pub fn new(runs: Vec<Vec<i64>>) -> KMerge {
        todo!("2c-c4: a heap holding the first key of each run")
    }
}

impl Iterator for KMerge {
    type Item = (i64, usize);

    fn next(&mut self) -> Option<(i64, usize)> {
        todo!("2c-c4: pop the smallest, refill from the run it came from")
    }
}
