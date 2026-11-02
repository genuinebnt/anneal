use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A single-threaded executor: `spawn` queues tasks, `run` polls them until all have finished.
/// Tasks are woken by their `Waker` (from any thread) and polled in the order they were woken.
#[derive(Clone)]
pub struct Executor {
    // TODO: shared state: a queue of woken tasks, a Condvar for sleeping, and a count of unfinished tasks
    _marker: (),
}

/// Awaiting a `JoinHandle` gives the task's output. Dropping it does not cancel the task.
pub struct JoinHandle<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T> JoinHandle<T> {
    /// The output, if the task has finished and nobody took it yet.
    pub fn take(&self) -> Option<T> {
        todo!()
    }

    pub fn is_finished(&self) -> bool {
        todo!()
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        todo!()
    }
}

/// A future that is pending exactly once and asks to be polled again at once: it sends the task to the back of the queue.
pub struct YieldNow(bool);

pub fn yield_now() -> YieldNow {
    todo!()
}

impl Future for YieldNow {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        todo!()
    }
}

impl Executor {
    pub fn new() -> Self {
        todo!()
    }

    /// Queues `fut` as a new task. `Send` because a waker (which holds the task) may be called from any thread.
    pub fn spawn<F>(&self, fut: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        todo!()
    }

    /// Runs tasks until every spawned task has finished (including ones spawned while running). Sleeps when nothing is
    /// ready but tasks are still waiting for a wake-up from elsewhere.
    pub fn run(&self) {
        todo!()
    }
}
