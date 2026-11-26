use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

struct Shared {
    /// The limit in the high half and the number in flight in the low half, so that one compare sees both.
    state: AtomicU64,
    peak: AtomicU32,
}

fn pack(limit: u32, in_flight: u32) -> u64 {
    (u64::from(limit) << 32) | u64::from(in_flight)
}

/// `(limit, in_flight)`
fn unpack(state: u64) -> (u32, u32) {
    ((state >> 32) as u32, state as u32)
}

/// Caps how many pieces of work are in progress at once. Clones share one gate.
#[derive(Clone)]
pub struct Gate {
    shared: Arc<Shared>,
}

/// Permission for one piece of work. Dropping it gives the place back.
pub struct Permit {
    shared: Arc<Shared>,
}

impl Gate {
    pub fn new(limit: u32) -> Self {
        Gate {
            shared: Arc::new(Shared {
                state: AtomicU64::new(pack(limit, 0)),
                peak: AtomicU32::new(0),
            }),
        }
    }

    pub fn try_acquire(&self) -> Option<Permit> {
        self.try_acquire_when(|_, _| true)
    }

    pub fn try_acquire_when(&self, mut allow: impl FnMut(u32, u32) -> bool) -> Option<Permit> {
        let state = &self.shared.state;
        let mut seen = state.load(Ordering::Acquire);
        loop {
            let (limit, in_flight) = unpack(seen);
            if in_flight >= limit || !allow(in_flight, limit) {
                return None;
            }
            let taken = pack(limit, in_flight + 1);
            state.store(taken, Ordering::Release);
            let _ = &mut seen;
            self.shared.peak.fetch_max(in_flight + 1, Ordering::Relaxed);
            return Some(Permit {
                shared: Arc::clone(&self.shared),
            });
        }
    }

    pub fn set_limit(&self, limit: u32) {
        let _ = self
            .shared
            .state
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                Some(pack(limit, unpack(state).1))
            });
    }

    pub fn limit(&self) -> u32 {
        unpack(self.shared.state.load(Ordering::Acquire)).0
    }

    pub fn in_flight(&self) -> u32 {
        unpack(self.shared.state.load(Ordering::Acquire)).1
    }

    pub fn peak(&self) -> u32 {
        self.shared.peak.load(Ordering::Relaxed)
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        // the low half is at least 1 while this permit exists, so nothing is borrowed from the limit
        self.shared.state.fetch_sub(1, Ordering::AcqRel);
    }
}
