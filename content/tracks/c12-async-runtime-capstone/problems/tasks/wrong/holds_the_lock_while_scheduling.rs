use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Wake, Waker};

/// A panic elsewhere must not poison the runtime.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------------------------------------------------
// When a task is queued (preparation: "Queue a job once, and lose no request")
// ---------------------------------------------------------------------------------------------------------------------

const IDLE: u8 = 0;
const QUEUED: u8 = 1;
const RUNNING: u8 = 2;
const RUNNING_AGAIN: u8 = 3;
const DONE: u8 = 4;

/// Idle, queued, running, running and asked for again, or done.
struct QueueState(AtomicU8);

impl QueueState {
    fn new() -> QueueState {
        QueueState(AtomicU8::new(IDLE))
    }

    fn step(&self, next: impl FnMut(u8) -> Option<u8>) -> Result<u8, u8> {
        self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, next)
    }

    /// `true`: the caller must queue the job now.
    fn request(&self) -> bool {
        self.step(|s| {
            Some(match s {
                IDLE => QUEUED,
                RUNNING => RUNNING_AGAIN,
                other => other,
            })
        }) == Ok(IDLE)
    }

    fn begin(&self) {
        let began = self.step(|s| (s == QUEUED).then_some(RUNNING));
        assert!(began.is_ok(), "the job is not queued");
    }

    /// `true`: queue the job again.
    fn end(&self) -> bool {
        let ended = self.step(|s| match s {
            RUNNING => Some(IDLE),
            RUNNING_AGAIN => Some(QUEUED),
            _ => None,
        });
        ended == Ok(RUNNING_AGAIN)
    }

    fn finish(&self) {
        let finished = self.step(|s| matches!(s, RUNNING | RUNNING_AGAIN).then_some(DONE));
        assert!(finished.is_ok(), "the job is not running");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Tasks (capstone step 1)
// ---------------------------------------------------------------------------------------------------------------------

pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Where tasks go when they have something to do.
pub trait Schedule: Send + Sync + 'static {
    fn schedule(&self, task: Arc<Task>);
}

/// A spawned future, shared between the scheduler and the wakers.
pub struct Task {
    state: QueueState,
    /// `None` once the task is complete.
    future: Mutex<Option<BoxFuture>>,
    scheduler: Arc<dyn Schedule>,
}

impl Task {
    /// Makes a task and hands it to `scheduler`.
    pub fn spawn(
        future: impl Future<Output = ()> + Send + 'static,
        scheduler: Arc<dyn Schedule>,
    ) -> Arc<Task> {
        let task = Arc::new(Task {
            state: QueueState::new(),
            future: Mutex::new(Some(Box::pin(future))),
            scheduler,
        });
        task.clone().wake();
        task
    }

    /// Polls the task once. Called by whoever took it from the scheduler.
    pub fn run(self: Arc<Self>) {
        self.state.begin();
        let waker = Waker::from(self.clone());
        let mut cx = Context::from_waker(&waker);
        let mut slot = lock(&self.future);
        let future = slot.as_mut().expect("a queued task has its future");
        if future.as_mut().poll(&mut cx).is_pending() {
            if self.state.end() {
                self.scheduler.schedule(self.clone());
            }
            drop(slot);
            return;
        }
        let done = slot.take();
        drop(slot);
        // the destructor may wake this task: that is recorded, then forgotten by `finish`
        drop(done);
        self.state.finish();
    }

    pub fn is_complete(&self) -> bool {
        self.state.0.load(Ordering::Acquire) == DONE
    }
}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        if self.state.request() {
            let scheduler = self.scheduler.clone();
            scheduler.schedule(self);
        }
    }
}

