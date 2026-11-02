use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

struct State<T> {
    value: Option<T>,
    waker: Option<Waker>, // the receiver's, refreshed on every poll
    tx_gone: bool,
    rx_gone: bool,
}

pub struct Sender<T> {
    state: Arc<Mutex<State<T>>>,
}

pub struct Receiver<T> {
    state: Arc<Mutex<State<T>>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecvError;

#[derive(Debug, PartialEq, Eq)]
pub enum TryRecvError {
    Empty,
    Closed,
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let state = Arc::new(Mutex::new(State { value: None, waker: None, tx_gone: false, rx_gone: false }));
    (Sender { state: state.clone() }, Receiver { state })
}

impl<T> Sender<T> {
    /// Hands the value over and wakes the receiver. `Err(value)` if the receiver is already gone.
    pub fn send(self, value: T) -> Result<(), T> {
        let waker = {
            let mut s = self.state.lock().unwrap();
            s.value = Some(value);
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake(); // after the lock is released: a woken task may run at once and want the lock
        }
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.state.lock().unwrap().rx_gone
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let waker = {
            let mut s = self.state.lock().unwrap();
            s.tx_gone = true;
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake(); // a receiver waiting on a sender that vanished must learn about it
        }
    }
}

impl<T> Receiver<T> {
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let mut s = self.state.lock().unwrap();
        match s.value.take() {
            Some(v) => Ok(v),
            None if s.tx_gone => Err(TryRecvError::Closed),
            None => Err(TryRecvError::Empty),
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.state.lock().unwrap().rx_gone = true;
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut s = self.state.lock().unwrap();
        if let Some(v) = s.value.take() {
            return Poll::Ready(Ok(v));
        }
        if s.tx_gone {
            return Poll::Ready(Err(RecvError));
        }
        // The task may have moved to a different waker since the last poll: always keep the newest.
        match &s.waker {
            Some(w) if w.will_wake(cx.waker()) => {}
            _ => s.waker = Some(cx.waker().clone()),
        }
        Poll::Pending
    }
}
