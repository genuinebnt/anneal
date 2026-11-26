//! k-way merge of sorted runs.

use crate::codec::Entry;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// `sources` are sorted runs, NEWEST FIRST. The heap is a min-heap on (key, source index), so for equal keys the newest source
/// pops first and every later copy is an older, shadowed version to discard. Tombstones may be dropped only when the merge
/// covers every older table, otherwise the delete would bring an old value back.
pub fn merge_sources(sources: Vec<Vec<Entry>>, drop_tombstones: bool) -> Vec<Entry> {
    let mut iters: Vec<_> = sources.into_iter().map(|s| s.into_iter()).collect();
    let mut heap = BinaryHeap::new();
    for (i, it) in iters.iter_mut().enumerate() {
        if let Some((k, v)) = it.next() {
            heap.push(Reverse((k, i, v)));
        }
    }
    let mut out = Vec::new();
    while let Some(Reverse((key, src, val))) = heap.pop() {
        if let Some((k, v)) = iters[src].next() {
            heap.push(Reverse((k, src, v)));
        }
        while heap.peek().is_some_and(|Reverse((k, _, _))| *k == key) {
            if let Some(Reverse((_, s, _))) = heap.pop() {
                if let Some((k, v)) = iters[s].next() {
                    heap.push(Reverse((k, s, v)));
                }
            }
        }
        if !(drop_tombstones && val.is_none()) {
            out.push((key, val));
        }
    }
    out
}
