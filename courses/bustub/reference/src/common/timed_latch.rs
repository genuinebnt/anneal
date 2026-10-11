//! An exclusive latch with a bounded wait.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub struct TimedLatch {
    // @begin 1g-c4
    held: Mutex<bool>,
    free: Condvar,
    //~ _latch: (),
    // @end
}

impl TimedLatch {
    /// A latch that is free.
    pub fn new() -> TimedLatch {
        // @begin 1g-c4
        TimedLatch { held: Mutex::new(false), free: Condvar::new() }
        //~ todo!("1g-c4: a free latch")
        // @end
    }

    /// Takes the latch if it is free, without waiting; returns whether it did.
    pub fn try_acquire(&self) -> bool {
        // @begin 1g-c4
        let mut held = self.held.lock().unwrap();
        if *held {
            false
        } else {
            *held = true;
            true
        }
        //~ todo!("1g-c4: take the latch if it is free")
        // @end
    }

    /// Waits at most `timeout` for the latch; true if it was acquired.
    pub fn acquire_timeout(&self, timeout: Duration) -> bool {
        // @begin 1g-c4
        let deadline = Instant::now() + timeout;
        let mut held = self.held.lock().unwrap();
        loop {
            if !*held {
                *held = true;
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            held = self.free.wait_timeout(held, deadline - now).unwrap().0;
        }
        //~ todo!("1g-c4: wait on the condition variable until the latch is free or the time is up; check the condition again after every wake-up")
        // @end
    }

    /// Frees the latch; false if it was not held.
    pub fn release(&self) -> bool {
        // @begin 1g-c4
        let mut held = self.held.lock().unwrap();
        if !*held {
            return false;
        }
        *held = false;
        self.free.notify_one();
        true
        //~ todo!("1g-c4: free it and wake one waiter")
        // @end
    }
}

impl Default for TimedLatch {
    fn default() -> Self {
        TimedLatch::new()
    }
}
