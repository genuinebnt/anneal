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
    _edges: (),
}

impl WaitsForGraph {
    /// A graph with no edges.
    pub fn new() -> WaitsForGraph {
        todo!("4d-06: no edges")
    }

    /// `from` waits for `to`. Adding an edge twice is the same as once.
    pub fn add_edge(&mut self, from: TxnId, to: TxnId) {
        todo!("4d-06: remember the edge")
    }

    /// Removes the edge `from` → `to`, if present.
    pub fn remove_edge(&mut self, from: TxnId, to: TxnId) {
        todo!("4d-06: forget the edge")
    }

    /// Is there a cycle? If so, the victim: the youngest (largest id) transaction on the cycle found by searching depth first from the
    /// smallest transaction id, trying each transaction's neighbours in increasing id order.
    pub fn has_cycle(&self) -> Option<TxnId> {
        todo!("4d-06: a depth-first search from the smallest id; the victim is the largest id on the cycle")
    }

    /// Every edge, ordered by (from, to).
    pub fn edge_list(&self) -> Vec<(TxnId, TxnId)> {
        todo!("4d-06: the edges in order")
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
        let handle = thread::spawn(move || {
            let _ = (&flag, &manager, every);
            todo!("4d-06: look for deadlocks every so often until stopped")
        });
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
