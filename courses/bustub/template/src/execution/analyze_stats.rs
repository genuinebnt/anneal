//! What `EXPLAIN ANALYZE` measures: for each node of a plan, how many rows it produced, in how many batches, how many times it was
//! initialised and how long its calls took, children included.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::{Arc, Mutex};


#[derive(Debug, Default)]
pub struct NodeStats {
    /// Tuples produced, over all calls of `next`.
    pub rows: AtomicUsize,
    /// Calls of `next` that produced a batch.
    pub batches: AtomicUsize,
    /// Calls of `init`: a nested loop join starts its inner side again for every outer tuple.
    pub loops: AtomicUsize,
    /// Time spent inside `init` and `next` of this node, children included, in nanoseconds.
    pub nanos: AtomicU64,
}

/// The measurements of one execution, by plan node (the node's identity, not its text).
#[derive(Debug, Default)]
pub struct AnalyzeStats {
    nodes: Mutex<HashMap<usize, Arc<NodeStats>>>,
}

impl AnalyzeStats {
    fn key<T>(plan: &Arc<T>) -> usize {
        Arc::as_ptr(plan) as *const () as usize
    }

    /// The counters of `plan` (made on first use).
    pub fn node<T>(&self, plan: &Arc<T>) -> Arc<NodeStats> {
        self.nodes.lock().unwrap().entry(Self::key(plan)).or_default().clone()
    }

    /// The counters of `plan`, if it was executed.
    pub fn get<T>(&self, plan: &Arc<T>) -> Option<Arc<NodeStats>> {
        self.nodes.lock().unwrap().get(&Self::key(plan)).cloned()
    }
}
