use std::sync::atomic::{AtomicU8, Ordering};

/// Where one job is: waiting for work, in the queue, running, or done for ever.
pub struct QueueState {
    state: AtomicU8,
}

impl QueueState {
    pub fn new() -> QueueState {
        todo!()
    }

    /// New work for this job. `true`: the caller must put it in the queue.
    pub fn request(&self) -> bool {
        todo!()
    }

    /// A worker starts running the queued job.
    pub fn begin(&self) {
        todo!()
    }

    /// The run is over. `true`: put the job back in the queue.
    pub fn end(&self) -> bool {
        todo!()
    }

    /// The run is over and the job will never run again.
    pub fn finish(&self) {
        todo!()
    }
}
