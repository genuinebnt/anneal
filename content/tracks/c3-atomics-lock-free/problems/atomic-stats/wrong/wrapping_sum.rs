use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub count: u64,
    pub sum: u64,
    pub min: u64,
    pub max: u64,
}

/// Latency-style counters shared by many threads, with no lock.
pub struct Stats {
    count: AtomicU64,
    sum: AtomicU64,
    min: AtomicU64,
    max: AtomicU64,
}

impl Stats {
    pub fn new() -> Self {
        Stats { count: AtomicU64::new(0), sum: AtomicU64::new(0), min: AtomicU64::new(u64::MAX), max: AtomicU64::new(0) }
    }

    pub fn record(&self, v: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        // saturating add as a compare-exchange loop: `fetch_update` retries until nobody raced us
        self.sum.fetch_add(v, Ordering::Relaxed);
        self.min.fetch_min(v, Ordering::Relaxed);
        self.max.fetch_max(v, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Option<Snapshot> {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return None;
        }
        Some(Snapshot {
            count,
            sum: self.sum.load(Ordering::Relaxed),
            min: self.min.load(Ordering::Relaxed),
            max: self.max.load(Ordering::Relaxed),
        })
    }

    /// Returns what was recorded and starts again from empty. Only called when no `record` is running.
    pub fn take(&self) -> Option<Snapshot> {
        let count = self.count.swap(0, Ordering::Relaxed);
        let sum = self.sum.swap(0, Ordering::Relaxed);
        let min = self.min.swap(u64::MAX, Ordering::Relaxed);
        let max = self.max.swap(0, Ordering::Relaxed);
        if count == 0 {
            return None;
        }
        Some(Snapshot { count, sum, min, max })
    }
}
