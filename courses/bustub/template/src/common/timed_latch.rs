//! An exclusive latch with a bounded wait.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub struct TimedLatch {
    _latch: (),
}

impl TimedLatch {
    /// A latch that is free.
    pub fn new() -> TimedLatch {
        todo!("1g-c4: a free latch")
    }

    /// Takes the latch if it is free, without waiting; returns whether it did.
    pub fn try_acquire(&self) -> bool {
        todo!("1g-c4: take the latch if it is free")
    }

    /// Waits at most `timeout` for the latch; true if it was acquired.
    pub fn acquire_timeout(&self, timeout: Duration) -> bool {
        todo!("1g-c4: wait on the condition variable until the latch is free or the time is up; check the condition again after every wake-up")
    }

    /// Frees the latch; false if it was not held.
    pub fn release(&self) -> bool {
        todo!("1g-c4: free it and wake one waiter")
    }
}

impl Default for TimedLatch {
    fn default() -> Self {
        TimedLatch::new()
    }
}
