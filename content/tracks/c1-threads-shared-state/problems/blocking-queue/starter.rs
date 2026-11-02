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

pub struct BlockingQueue<T> {
    state: Mutex<State<T>>,
    not_empty: Condvar,
    not_full: Condvar,
}

impl<T> BlockingQueue<T> {
    /// Panics if `cap` is 0.
    pub fn new(cap: usize) -> Self {
        todo!()
    }

    /// Waits while full. Gives the value back if the queue is closed, even when it closes while we wait.
    pub fn push(&self, value: T) -> Result<(), T> {
        todo!()
    }

    /// Never waits: `Err(value)` when full or closed.
    pub fn try_push(&self, value: T) -> Result<(), T> {
        todo!()
    }

    /// Waits while empty. `None` once the queue is closed and drained.
    pub fn pop(&self) -> Option<T> {
        todo!()
    }

    pub fn try_pop(&self) -> Option<T> {
        todo!()
    }

    /// Waits at most `timeout` in total. `Err(Closed)` when closed and drained, `Err(Timeout)` otherwise.
    pub fn pop_timeout(&self, timeout: Duration) -> Result<T, PopError> {
        todo!()
    }

    /// No more pushes; queued items can still be popped. Wakes every waiting thread.
    pub fn close(&self) {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }
}
