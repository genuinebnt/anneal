use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub struct SendError<T>(pub T);

#[derive(Debug, PartialEq, Eq)]
pub enum TrySendError<T> {
    Full(T),
    Disconnected(T),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct RecvError;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TryRecvError {
    Empty,
    Disconnected,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RecvTimeoutError {
    Timeout,
    Disconnected,
}

struct State<T> {
    queue: VecDeque<T>,
    cap: usize,
    senders: usize,
    receivers: usize,
}

struct Chan<T> {
    state: Mutex<State<T>>,
    not_empty: Condvar,
    not_full: Condvar,
}

pub struct Sender<T> {
    chan: Arc<Chan<T>>,
}

pub struct Receiver<T> {
    chan: Arc<Chan<T>>,
}

/// A bounded multi-producer multi-consumer channel (both ends are `Clone`). Panics if `cap` is 0.
pub fn channel<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    todo!()
}

impl<T> Sender<T> {
    /// Waits while the channel is full. `Err(SendError(value))` once every receiver is gone (also if that happens while waiting).
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        todo!()
    }

    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        todo!()
    }
}

impl<T> Receiver<T> {
    /// Waits while empty. `Err` once it is empty and every sender is gone; values sent before that are still delivered.
    pub fn recv(&self) -> Result<T, RecvError> {
        todo!()
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        todo!()
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        todo!()
    }
}

/// Receiving until disconnected: `for v in rx { .. }`.
impl<T> Iterator for Receiver<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        todo!()
    }
}

// TODO: `Clone` for both ends and `Drop` for both. The counts decide when the other side sees "disconnected",
// and a blocked thread has to be woken when the count reaches zero.
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}
