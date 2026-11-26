#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Totals {
    pub accepted: u64,
    pub refused: u64,
    pub handled: u64,
    pub panicked: u64,
}

pub struct Service<T> {
    // TODO
    unused: std::marker::PhantomData<fn(T)>,
}

impl<T: Send + 'static> Service<T> {
    /// Panics if `workers` or `room` is 0.
    pub fn start(
        workers: usize,
        room: usize,
        handle: impl Fn(T) + Send + Sync + 'static,
    ) -> Service<T> {
        todo!()
    }

    /// Never blocks. `Err(item)` when `room` items are already waiting.
    pub fn offer(&self, item: T) -> Result<(), T> {
        todo!()
    }

    /// Accepted items that no worker has taken yet.
    pub fn waiting(&self) -> usize {
        todo!()
    }

    /// Handles everything accepted, waits for the workers and returns the totals. Dropping does the same.
    pub fn close(self) -> Totals {
        todo!()
    }
}
