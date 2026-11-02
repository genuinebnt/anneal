use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;

type Job = Box<dyn FnOnce() + Send + 'static>;

struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    panicked: AtomicUsize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskPanicked;

pub struct TaskHandle<R> {
    rx: mpsc::Receiver<R>,
}

impl<R> TaskHandle<R> {
    /// Waits for the task's result. `Err(TaskPanicked)` if the task panicked.
    pub fn join(self) -> Result<R, TaskPanicked> {
        todo!()
    }
}

pub struct ThreadPool {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl ThreadPool {
    /// Starts `workers` threads that take jobs in FIFO order. Panics if `workers` is 0.
    pub fn new(workers: usize) -> Self {
        todo!()
    }

    /// Queues a job. Never runs it on the calling thread.
    pub fn execute<F: FnOnce() + Send + 'static>(&self, f: F) {
        todo!()
    }

    /// Like `execute`, but the closure's result comes back through the handle.
    pub fn submit<F, R>(&self, f: F) -> TaskHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        todo!()
    }

    /// Stops accepting work, lets the workers finish every job already queued, joins them, and returns how many
    /// jobs panicked. A panicking job must not kill its worker.
    pub fn shutdown(self) -> usize {
        todo!()
    }
}

// TODO: dropping a pool must do the same as `shutdown` (no detached threads, no lost jobs).
