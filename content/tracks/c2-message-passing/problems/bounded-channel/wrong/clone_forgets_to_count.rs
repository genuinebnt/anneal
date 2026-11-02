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

/// A bounded multi-producer multi-consumer channel. Panics if `cap` is 0.
pub fn channel<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    assert!(cap > 0, "capacity must be at least 1");
    let chan = Arc::new(Chan {
        state: Mutex::new(State { queue: VecDeque::new(), cap, senders: 1, receivers: 1 }),
        not_empty: Condvar::new(),
        not_full: Condvar::new(),
    });
    (Sender { chan: chan.clone() }, Receiver { chan })
}

impl<T> Sender<T> {
    /// Waits for room. `Err` (with the value) once every receiver is gone, also when that happens while waiting.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut s = self.chan.state.lock().unwrap();
        loop {
            if s.receivers == 0 {
                return Err(SendError(value));
            }
            if s.queue.len() < s.cap {
                s.queue.push_back(value);
                self.chan.not_empty.notify_one();
                return Ok(());
            }
            s = self.chan.not_full.wait(s).unwrap();
        }
    }

    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        let mut s = self.chan.state.lock().unwrap();
        if s.receivers == 0 {
            return Err(TrySendError::Disconnected(value));
        }
        if s.queue.len() == s.cap {
            return Err(TrySendError::Full(value));
        }
        s.queue.push_back(value);
        self.chan.not_empty.notify_one();
        Ok(())
    }
}

impl<T> Receiver<T> {
    /// Waits for a value. `Err` once the queue is empty and every sender is gone.
    pub fn recv(&self) -> Result<T, RecvError> {
        let mut s = self.chan.state.lock().unwrap();
        loop {
            if let Some(v) = s.queue.pop_front() {
                self.chan.not_full.notify_one();
                return Ok(v);
            }
            if s.senders == 0 {
                return Err(RecvError);
            }
            s = self.chan.not_empty.wait(s).unwrap();
        }
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut s = self.chan.state.lock().unwrap();
        if let Some(v) = s.queue.pop_front() {
            self.chan.not_full.notify_one();
            return Ok(v);
        }
        if s.senders == 0 { Err(TryRecvError::Disconnected) } else { Err(TryRecvError::Empty) }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        let deadline = Instant::now() + timeout;
        let mut s = self.chan.state.lock().unwrap();
        loop {
            if let Some(v) = s.queue.pop_front() {
                self.chan.not_full.notify_one();
                return Ok(v);
            }
            if s.senders == 0 {
                return Err(RecvTimeoutError::Disconnected);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(RecvTimeoutError::Timeout);
            }
            s = self.chan.not_empty.wait_timeout(s, deadline - now).unwrap().0;
        }
    }
}

impl<T> Iterator for Receiver<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.recv().ok()
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Sender { chan: self.chan.clone() }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.chan.state.lock().unwrap().receivers += 1;
        Receiver { chan: self.chan.clone() }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut s = self.chan.state.lock().unwrap();
        s.senders -= 1;
        if s.senders == 0 {
            self.chan.not_empty.notify_all(); // blocked receivers must see the disconnect
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut s = self.chan.state.lock().unwrap();
        s.receivers -= 1;
        if s.receivers == 0 {
            self.chan.not_full.notify_all(); // blocked senders must see the disconnect
        }
    }
}
