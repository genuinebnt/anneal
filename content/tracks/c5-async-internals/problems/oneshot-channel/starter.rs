use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct Sender<T> {
    // TODO: shared state: the value, the receiver's Waker, and whether each side is gone
    _marker: std::marker::PhantomData<T>,
}

pub struct Receiver<T> {
    _marker: std::marker::PhantomData<T>,
}

/// The sender was dropped without sending.
#[derive(Debug, PartialEq, Eq)]
pub struct RecvError;

#[derive(Debug, PartialEq, Eq)]
pub enum TryRecvError {
    /// Nothing yet, the sender may still send.
    Empty,
    /// Nothing, and the sender is gone.
    Closed,
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    todo!()
}

impl<T> Sender<T> {
    /// Hands the value over and wakes the receiver. `Err(value)` if the receiver is already gone.
    pub fn send(self, value: T) -> Result<(), T> {
        todo!()
    }

    pub fn is_closed(&self) -> bool {
        todo!()
    }
}

// TODO: `Drop` for `Sender` (wake the receiver: it must learn the sender is gone) and for `Receiver`.

impl<T> Receiver<T> {
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        todo!()
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        todo!()
    }
}
