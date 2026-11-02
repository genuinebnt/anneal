use std::sync::{Condvar, Mutex};

struct State {
    available: usize,
    next_ticket: u64,
    serving: u64,
}

/// A counting semaphore that serves waiters strictly in arrival order, so a request for many permits is never
/// starved by a stream of small ones.
pub struct Semaphore {
    total: usize,
    state: Mutex<State>,
    turn: Condvar,
}

/// Returns its permits when dropped.
pub struct Permit<'a> {
    sem: &'a Semaphore,
    n: usize,
}

impl Semaphore {
    pub fn new(total: usize) -> Self {
        Semaphore { total, state: Mutex::new(State { available: total, next_ticket: 0, serving: 0 }), turn: Condvar::new() }
    }

    /// Waits for its turn and for `n` permits. Panics unless `1 <= n <= total`.
    pub fn acquire(&self, n: usize) -> Permit<'_> {
        assert!(n >= 1 && n <= self.total, "request must be between 1 and the total");
        let mut s = self.state.lock().unwrap();
        let ticket = s.next_ticket;
        s.next_ticket += 1;
        while s.serving != ticket || s.available < n {
            s = self.turn.wait(s).unwrap();
        }
        s.available -= n;
        s.serving += 1;
        drop(s);
        self.turn.notify_one();
        Permit { sem: self, n }
    }

    /// Succeeds only if nobody is queued and `n` permits are free right now.
    pub fn try_acquire(&self, n: usize) -> Option<Permit<'_>> {
        assert!(n >= 1 && n <= self.total, "request must be between 1 and the total");
        let mut s = self.state.lock().unwrap();
        if s.next_ticket != s.serving || s.available < n {
            return None;
        }
        s.available -= n;
        Some(Permit { sem: self, n })
    }

    pub fn available(&self) -> usize {
        self.state.lock().unwrap().available
    }

    /// Threads currently queued in `acquire`.
    pub fn waiting(&self) -> usize {
        let s = self.state.lock().unwrap();
        (s.next_ticket - s.serving) as usize
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.sem.state.lock().unwrap().available += self.n;
        self.sem.turn.notify_one();
    }
}
