//! Publication: write ordinary (non-atomic) data once, then let one flag hand it over.

use crate::sync::{Arc, AtomicBool, Ordering, UnsafeCell};

struct Slot {
    value: UnsafeCell<u64>,
    ready: AtomicBool,
}

// SAFETY: `value` is written once, by the only `Publisher`, before `ready` becomes
// true. It is read only after a thread has seen `ready == true` with `Acquire`.
// The Release store and that Acquire load make the write happen before every read.
unsafe impl Sync for Slot {}

/// The writing half. `publish` takes `self`, so there is at most one write.
pub struct Publisher(Arc<Slot>);

/// The reading half.
pub struct Reader(Arc<Slot>);

pub fn slot() -> (Publisher, Reader) {
    let slot = Arc::new(Slot {
        value: UnsafeCell::new(0),
        ready: AtomicBool::new(false),
    });
    (Publisher(slot.clone()), Reader(slot))
}

impl Publisher {
    pub fn publish(self, value: u64) {
        // SAFETY: this is the only write, and no reader touches `value` until it
        // has seen the store below.
        self.0.value.with_mut(|p| unsafe { *p = value });
        self.0.ready.store(true, Ordering::Release);
    }
}

impl Reader {
    /// `None` until the value has been published. Never blocks.
    pub fn try_read(&self) -> Option<u64> {
        if !self.0.ready.load(Ordering::Acquire) {
            return None;
        }
        // SAFETY: the Acquire load saw the Release store, so the write to `value`
        // happened before this read, and nothing writes it again.
        Some(self.0.value.with(|p| unsafe { *p }))
    }
}
