use std::sync::Arc;

struct Shared {
    // what every clone and permit shares
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
        todo!()
    }

    /// A permit if fewer than `limit` are held, else `None`. Never waits.
    pub fn try_acquire(&self) -> Option<Permit> {
        todo!()
    }

    /// Like `try_acquire`, but `allow(in_flight, limit)` must also agree.
    pub fn try_acquire_when(&self, allow: impl FnMut(u32, u32) -> bool) -> Option<Permit> {
        todo!()
    }

    pub fn set_limit(&self, limit: u32) {
        todo!()
    }

    pub fn limit(&self) -> u32 {
        todo!()
    }

    pub fn in_flight(&self) -> u32 {
        todo!()
    }

    /// The most permits that have been held at once.
    pub fn peak(&self) -> u32 {
        todo!()
    }
}

// TODO: `Drop` for `Permit`.
