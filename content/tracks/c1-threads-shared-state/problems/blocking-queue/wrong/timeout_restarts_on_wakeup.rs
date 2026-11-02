use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PopError {
    Timeout,
    Closed,
}

struct State<T> {
    items: VecDeque<T>,
    cap: usize,
    closed: bool,
}

/// A bounded FIFO shared by threads: producers wait while it is full, consumers wait while it is empty.
pub struct BlockingQueue<T> {
    state: Mutex<State<T>>,
    not_empty: Condvar,
    not_full: Condvar,
}

impl<T> BlockingQueue<T> {
    /// Panics if `cap` is 0.
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0, "capacity must be at least 1");
        BlockingQueue { state: Mutex::new(State { items: VecDeque::new(), cap, closed: false }), not_empty: Condvar::new(), not_full: Condvar::new() }
    }

    /// Waits for room. Gives the value back if the queue is (or becomes) closed.
    pub fn push(&self, value: T) -> Result<(), T> {
        let mut s = self.state.lock().unwrap();
        while s.items.len() == s.cap && !s.closed {
            s = self.not_full.wait(s).unwrap();
        }
        if s.closed {
            return Err(value);
        }
        s.items.push_back(value);
        self.not_empty.notify_one();
        Ok(())
    }

    /// Never waits: `Err` when full or closed.
    pub fn try_push(&self, value: T) -> Result<(), T> {
        let mut s = self.state.lock().unwrap();
        if s.closed || s.items.len() == s.cap {
            return Err(value);
        }
        s.items.push_back(value);
        self.not_empty.notify_one();
        Ok(())
    }

    /// Waits for an item. `None` once the queue is closed AND drained.
    pub fn pop(&self) -> Option<T> {
        let mut s = self.state.lock().unwrap();
        while s.items.is_empty() && !s.closed {
            s = self.not_empty.wait(s).unwrap();
        }
        let item = s.items.pop_front();
        if item.is_some() {
            self.not_full.notify_one();
        }
        item
    }

    pub fn try_pop(&self) -> Option<T> {
        let mut s = self.state.lock().unwrap();
        let item = s.items.pop_front();
        if item.is_some() {
            self.not_full.notify_one();
        }
        item
    }

    /// Like `pop`, but gives up after `timeout` in total (spurious wake-ups must not shorten or extend it).
    pub fn pop_timeout(&self, timeout: Duration) -> Result<T, PopError> {
        let deadline = Instant::now() + timeout;
        let mut s = self.state.lock().unwrap();
        loop {
            if let Some(item) = s.items.pop_front() {
                self.not_full.notify_one();
                return Ok(item);
            }
            if s.closed {
                return Err(PopError::Closed);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(PopError::Timeout);
            }
            s = self.not_empty.wait_timeout(s, timeout).unwrap().0;
        }
    }

    /// No more pushes; items already queued can still be popped. Wakes every waiter.
    pub fn close(&self) {
        self.state.lock().unwrap().closed = true;
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }

    pub fn len(&self) -> usize {
        self.state.lock().unwrap().items.len()
    }
}
