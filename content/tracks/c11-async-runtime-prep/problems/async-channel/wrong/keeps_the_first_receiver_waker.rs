use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

struct State<T> {
    queue: VecDeque<T>,
    senders: usize,
    closed: bool,
    /// The receiver waits in at most one place.
    receiver: Option<Waker>,
    /// Waiting sends. All of them are woken whenever room appears.
    waiting_sends: Vec<Waker>,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    capacity: usize,
}

impl<T> Shared<T> {
    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
}

/// A bounded queue between tasks. Panics if `capacity` is 0.
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    assert!(capacity > 0, "capacity must be at least 1");
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            queue: VecDeque::new(),
            senders: 1,
            closed: false,
            receiver: None,
            waiting_sends: Vec::new(),
        }),
        capacity,
    });
    (
        Sender {
            shared: shared.clone(),
        },
        Receiver { shared },
    )
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared.lock().senders += 1;
        Sender {
            shared: self.shared.clone(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let waker = {
            let mut state = self.shared.lock();
            state.senders -= 1;
            if state.senders == 0 {
                state.receiver.take()
            } else {
                None
            }
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Sender<T> {
    /// Waits for room. `Err(value)` once the receiver is gone.
    pub async fn send(&self, value: T) -> Result<(), T> {
        // the value stays in this future until the poll that queues it, so dropping the future queues nothing
        let mut value = Some(value);
        poll_fn(|cx| self.poll_send(cx, &mut value)).await
    }

    fn poll_send(&self, cx: &mut Context<'_>, value: &mut Option<T>) -> Poll<Result<(), T>> {
        let mut state = self.shared.lock();
        let full = state.queue.len() >= self.shared.capacity;
        if !state.closed && full {
            if !state.waiting_sends.iter().any(|w| w.will_wake(cx.waker())) {
                state.waiting_sends.push(cx.waker().clone());
            }
            return Poll::Pending;
        }
        let value = value.take().expect("send polled after it finished");
        if state.closed {
            return Poll::Ready(Err(value));
        }
        state.queue.push_back(value);
        let receiver = state.receiver.take();
        drop(state);
        if let Some(waker) = receiver {
            waker.wake();
        }
        Poll::Ready(Ok(()))
    }

    /// Is the receiver gone?
    pub fn is_closed(&self) -> bool {
        self.shared.lock().closed
    }

    /// Values in the queue now.
    pub fn len(&self) -> usize {
        self.shared.lock().queue.len()
    }
}

impl<T> Receiver<T> {
    /// The oldest value, or `None` once every sender is gone and the queue is empty.
    pub async fn recv(&mut self) -> Option<T> {
        // a value leaves the queue only in the poll that returns it
        poll_fn(|cx| self.poll_recv(cx)).await
    }

    fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let mut state = self.shared.lock();
        if let Some(value) = state.queue.pop_front() {
            // wake them all: if we picked one and it was cancelled, the free place could be forgotten
            let waiting = std::mem::take(&mut state.waiting_sends);
            drop(state);
            waiting.into_iter().for_each(Waker::wake);
            return Poll::Ready(Some(value));
        }
        if state.senders == 0 {
            return Poll::Ready(None);
        }
        if state.receiver.is_none() {
            state.receiver = Some(cx.waker().clone());
        }
        Poll::Pending
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let (queued, waiting, own) = {
            let mut state = self.shared.lock();
            state.closed = true;
            (
                std::mem::take(&mut state.queue),
                std::mem::take(&mut state.waiting_sends),
                state.receiver.take(),
            )
        };
        waiting.into_iter().for_each(Waker::wake);
        drop(own);
        // outside the lock: a value's destructor may use this channel
        drop(queued);
    }
}
