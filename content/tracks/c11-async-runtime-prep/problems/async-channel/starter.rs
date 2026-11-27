use std::marker::PhantomData;

pub struct Sender<T> {
    // TODO: the fields
    _todo: PhantomData<fn() -> T>,
}

pub struct Receiver<T> {
    // TODO: the fields
    _todo: PhantomData<fn() -> T>,
}

/// A bounded queue between tasks. Panics if `capacity` is 0.
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    todo!()
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        todo!()
    }
}

impl<T> Sender<T> {
    /// Waits for room. `Err(value)` once the receiver is gone.
    pub async fn send(&self, value: T) -> Result<(), T> {
        todo!()
    }

    /// Is the receiver gone?
    pub fn is_closed(&self) -> bool {
        todo!()
    }

    /// Values in the queue now.
    pub fn len(&self) -> usize {
        todo!()
    }
}

impl<T> Receiver<T> {
    /// The oldest value, or `None` once every sender is gone and the queue is empty.
    pub async fn recv(&mut self) -> Option<T> {
        todo!()
    }
}
