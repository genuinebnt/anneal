use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The top bit of the state: set once closed. The other bits count the units handed out.
const CLOSED: usize = !(usize::MAX >> 1);

pub struct Region {
    len: usize,
    state: AtomicUsize,
    closed: AtomicBool,
}

impl Region {
    pub fn new(len: usize) -> Self {
        assert!(len <= usize::MAX / 2, "the region is too large");
        Region {
            len,
            state: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
        }
    }

    pub fn claim(&self, n: usize) -> Option<Range<usize>> {
        self.claim_with(|_| n)
    }

    pub fn claim_with(&self, mut choose: impl FnMut(usize) -> usize) -> Option<Range<usize>> {
        let mut seen = self.state.load(Ordering::Acquire);
        loop {
            if self.closed.load(Ordering::Acquire) {
                return None;
            }
            let free = self.len - seen;
            let n = choose(free);
            if n > free {
                return None;
            }
            // Take the units only if nothing at all changed since `seen`: neither a claim nor `close`.
            match self.state.compare_exchange_weak(
                seen,
                seen + n,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(seen..seen + n),
                Err(now) => seen = now,
            }
        }
    }

    pub fn free(&self) -> usize {
        let state = self.state.load(Ordering::Acquire);
        if self.closed.load(Ordering::Acquire) {
            0
        } else {
            self.len - state
        }
    }

    pub fn close(&self) -> usize {
        self.closed.store(true, Ordering::Release);
        self.state.load(Ordering::Acquire)
    }
}
