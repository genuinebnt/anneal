use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub count: u64,
    pub sum: u64,
    pub min: u64,
    pub max: u64,
}

pub struct Stats {
    // TODO: the fields
}

impl Stats {
    pub fn new() -> Self {
        todo!()
    }

    pub fn record(&self, v: u64) {
        todo!()
    }

    pub fn snapshot(&self) -> Option<Snapshot> {
        todo!()
    }

    pub fn take(&self) -> Option<Snapshot> {
        todo!()
    }
}
