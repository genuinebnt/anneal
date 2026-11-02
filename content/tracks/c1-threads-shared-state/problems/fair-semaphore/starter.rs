use std::sync::{Condvar, Mutex};

struct State {
    available: usize,
    next_ticket: u64,
    serving: u64,
}

pub struct Semaphore {
    total: usize,
    state: Mutex<State>,
    turn: Condvar,
}

pub struct Permit<'a> {
    sem: &'a Semaphore,
    n: usize,
}

impl Semaphore {
    pub fn new(total: usize) -> Self {
        todo!()
    }

    /// Waits for its turn (strict arrival order) and for `n` permits. Panics unless `1 <= n <= total`.
    pub fn acquire(&self, n: usize) -> Permit<'_> {
        todo!()
    }

    /// Succeeds only if nobody is queued and `n` permits are free right now. Panics unless `1 <= n <= total`.
    pub fn try_acquire(&self, n: usize) -> Option<Permit<'_>> {
        todo!()
    }

    pub fn available(&self) -> usize {
        todo!()
    }

    /// Threads currently queued inside `acquire`.
    pub fn waiting(&self) -> usize {
        todo!()
    }
}

// TODO: `Drop` for `Permit`: give the permits back and wake the waiters.
