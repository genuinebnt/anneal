//! Finding deadlocks: a graph of who waits for whom, and a thread that looks for cycles in it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::concurrency::lock_manager::LockManager;
use crate::concurrency::transaction::TxnId;

/// "`a` waits for `b`" edges. A cycle is a deadlock: nobody on it can ever go on.
pub struct WaitsForGraph {
    // @begin 4d-06
    edges: BTreeMap<TxnId, BTreeSet<TxnId>>,
    //~ _edges: (),
    // @end
}

impl WaitsForGraph {
    /// A graph with no edges.
    pub fn new() -> WaitsForGraph {
        // @begin 4d-06
        WaitsForGraph { edges: BTreeMap::new() }
        //~ todo!("4d-06: no edges")
        // @end
    }

    /// `from` waits for `to`. Adding an edge twice is the same as once.
    pub fn add_edge(&mut self, from: TxnId, to: TxnId) {
        // @begin 4d-06
        self.edges.entry(from).or_default().insert(to);
        //~ todo!("4d-06: remember the edge")
        // @end
    }

    /// Removes the edge `from` → `to`, if present.
    pub fn remove_edge(&mut self, from: TxnId, to: TxnId) {
        // @begin 4d-06
        if let Some(set) = self.edges.get_mut(&from) {
            set.remove(&to);
            if set.is_empty() {
                self.edges.remove(&from);
            }
        }
        //~ todo!("4d-06: forget the edge")
        // @end
    }

    /// Is there a cycle? If so, the victim: the youngest (largest id) transaction on the cycle found by searching depth first from the
    /// smallest transaction id, trying each transaction's neighbours in increasing id order.
    pub fn has_cycle(&self) -> Option<TxnId> {
        // @begin 4d-06
        fn visit(g: &WaitsForGraph, node: TxnId, path: &mut Vec<TxnId>, done: &mut BTreeSet<TxnId>) -> Option<TxnId> {
            if let Some(at) = path.iter().position(|&n| n == node) {
                return path[at..].iter().copied().max();
            }
            if done.contains(&node) {
                return None;
            }
            path.push(node);
            if let Some(next) = g.edges.get(&node) {
                for &n in next {
                    if let Some(v) = visit(g, n, path, done) {
                        return Some(v);
                    }
                }
            }
            path.pop();
            done.insert(node);
            None
        }
        let mut done = BTreeSet::new();
        for &start in self.edges.keys() {
            if let Some(v) = visit(self, start, &mut Vec::new(), &mut done) {
                return Some(v);
            }
        }
        None
        //~ todo!("4d-06: a depth-first search from the smallest id; the victim is the largest id on the cycle")
        // @end
    }

    /// Every edge, ordered by (from, to).
    pub fn edge_list(&self) -> Vec<(TxnId, TxnId)> {
        // @begin 4d-06
        self.edges.iter().flat_map(|(&a, set)| set.iter().map(move |&b| (a, b))).collect()
        //~ todo!("4d-06: the edges in order")
        // @end
    }
}

impl Default for WaitsForGraph {
    fn default() -> Self {
        WaitsForGraph::new()
    }
}

/// A thread that runs [`LockManager::detect_deadlocks`] every `every` until the detector is dropped.
pub struct DeadlockDetector {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl DeadlockDetector {
    /// Starts a thread that runs deadlock detection on `manager` every `every`, until the returned detector is dropped.
    pub fn start(manager: Arc<LockManager>, every: Duration) -> DeadlockDetector {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        // @begin 4d-06
        let handle = thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                manager.detect_deadlocks();
                thread::sleep(every);
            }
        });
        //~ let handle = thread::spawn(move || {
        //~     let _ = (&flag, &manager, every);
        //~     todo!("4d-06: look for deadlocks every so often until stopped")
        //~ });
        // @end
        DeadlockDetector { stop, handle: Some(handle) }
    }
}

impl Drop for DeadlockDetector {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
