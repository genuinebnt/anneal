use std::any::Any;
use std::cell::RefCell;
use std::future::Future;
use std::marker::PhantomData;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::task::{Context, Poll, Wake, Waker};

use crossbeam::deque::{Injector, Steal, Stealer, Worker as TaskDeque};

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
// How a future ended (preparation: "Say how a future ended, after it has cleaned up")
// ---------------------------------------------------------------------------------------------------------------------

enum Ending<T> {
    Finished(T),
    Panicked(String),
    Dropped,
}

/// Drives a future and calls `report` once, after the future is destroyed. No panic of the future escapes.
struct Watched<F: Future, R: FnOnce(Ending<F::Output>)> {
    running: Option<(Pin<Box<F>>, R)>,
}

// the future is boxed and the report function is never pinned
impl<F: Future, R: FnOnce(Ending<F::Output>)> Unpin for Watched<F, R> {}

fn watched<F: Future, R: FnOnce(Ending<F::Output>)>(future: F, report: R) -> Watched<F, R> {
    Watched {
        running: Some((Box::pin(future), report)),
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-string panic".to_string()
    }
}

fn destroy<F>(future: Pin<Box<F>>) -> Result<(), String> {
    catch_unwind(AssertUnwindSafe(move || drop(future))).map_err(panic_message)
}

impl<F: Future, R: FnOnce(Ending<F::Output>)> Future for Watched<F, R> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let (future, _) = self.running.as_mut().expect("polled after it finished");
        let result = match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx))) {
            Ok(Poll::Pending) => return Poll::Pending,
            Ok(Poll::Ready(value)) => Ok(value),
            Err(payload) => Err(panic_message(payload)),
        };
        let (future, report) = self.running.take().expect("checked above");
        report(match (result, destroy(future)) {
            (Err(first), _) => Ending::Panicked(first),
            (Ok(value), Ok(())) => Ending::Finished(value),
            (Ok(_), Err(from_drop)) => Ending::Panicked(from_drop),
        });
        Poll::Ready(())
    }
}

impl<F: Future, R: FnOnce(Ending<F::Output>)> Drop for Watched<F, R> {
    fn drop(&mut self) {
        if let Some((future, report)) = self.running.take() {
            report(match destroy(future) {
                Ok(()) => Ending::Dropped,
                Err(from_drop) => Ending::Panicked(from_drop),
            });
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Tasks (capstone steps 1 and 2)
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
    aborted: AtomicBool,
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
            aborted: AtomicBool::new(false),
            scheduler,
        });
        task.clone().wake();
        task
    }

    /// Polls the task once. Called by whoever took it from the scheduler.
    pub fn run(self: Arc<Self>) {
        self.state.begin();
        if self.aborted.load(Ordering::Acquire) {
            self.complete();
            return;
        }
        let waker = Waker::from(self.clone());
        let mut cx = Context::from_waker(&waker);
        let mut slot = lock(&self.future);
        let future = slot.as_mut().expect("a queued task has its future");
        if future.as_mut().poll(&mut cx).is_pending() {
            // unlock first: `schedule` may run the task again at once
            drop(slot);
            if self.state.end() {
                self.scheduler.schedule(self.clone());
            }
            return;
        }
        drop(slot);
        self.complete();
    }

    /// Destroys the future and retires the task. Only called by `run`.
    fn complete(&self) {
        let done = lock(&self.future).take();
        // the destructor may wake this task: that is recorded, then forgotten by `finish`
        drop(done);
        self.state.finish();
    }

    /// Asks for the future to be dropped instead of polled, at the task's next run.
    fn abort(self: Arc<Self>) {
        self.aborted.store(true, Ordering::Release);
        self.wake();
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

// ---------------------------------------------------------------------------------------------------------------------
// Join handles (capstone step 2)
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinError {
    Panicked(String),
    Cancelled,
}

struct JoinState<T> {
    result: Option<Result<T, JoinError>>,
    finished: bool,
    waker: Option<Waker>,
}

type JoinCell<T> = Mutex<JoinState<T>>;

fn complete<T>(cell: &JoinCell<T>, result: Result<T, JoinError>) {
    let waker = {
        let mut state = lock(cell);
        state.result = Some(result);
        state.finished = true;
        state.waker.take()
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// The other end of a spawned task: awaits its result, or cancels it.
pub struct JoinHandle<T> {
    cell: Arc<JoinCell<T>>,
    /// Weak, so that a task nobody can wake any more is freed, and its handle learns of it.
    task: Weak<Task>,
}

/// Spawns `future` as a task on `scheduler`.
pub fn spawn_on<F>(scheduler: &Arc<dyn Schedule>, future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let cell = Arc::new(Mutex::new(JoinState {
        result: None,
        finished: false,
        waker: None,
    }));
    let report = cell.clone();
    let body = watched(future, move |ending| {
        complete(
            &report,
            match ending {
                Ending::Finished(value) => Ok(value),
                Ending::Panicked(message) => Err(JoinError::Panicked(message)),
                Ending::Dropped => Err(JoinError::Cancelled),
            },
        )
    });
    let task = Task::spawn(body, scheduler.clone());
    JoinHandle {
        cell,
        task: Arc::downgrade(&task),
    }
}

impl<T> JoinHandle<T> {
    /// The task's next run drops its future instead of polling it.
    pub fn abort(&self) {
        if let Some(task) = self.task.upgrade() {
            task.abort();
        }
    }

    /// Has the task ended, with its future destroyed?
    pub fn is_finished(&self) -> bool {
        lock(&self.cell).finished
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = lock(&self.cell);
        if state.finished {
            let result = state.result.take();
            return Poll::Ready(result.expect("a join handle polled after it finished"));
        }
        let old = state.waker.replace(cx.waker().clone());
        drop(state);
        drop(old);
        Poll::Pending
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Where worker threads find tasks (capstone step 5)
// ---------------------------------------------------------------------------------------------------------------------

/// Every `GLOBAL_EVERY`th look goes to the shared queue first.
const GLOBAL_EVERY: u64 = 61;

/// A worker that this thread has entered.
struct WorkerContext {
    pool: *const Pool,
    index: usize,
    local: TaskDeque<Arc<Task>>,
    tick: u64,
}

thread_local! {
    /// The workers this thread has entered, oldest first.
    static ENTERED: RefCell<Vec<WorkerContext>> = const { RefCell::new(Vec::new()) };
}

/// The part of the queues that every thread may touch.
pub struct Pool {
    injector: Injector<Arc<Task>>,
    stealers: Vec<Stealer<Arc<Task>>>,
    steals: Vec<AtomicU64>,
}

/// One worker's own queue. It can move to another thread.
pub struct LocalWorker {
    pool: Arc<Pool>,
    index: usize,
    /// `None` while entered: the queue is then in the thread's context.
    local: Option<TaskDeque<Arc<Task>>>,
    tick: u64,
}

/// A worker that is entered on this thread.
pub struct EnteredWorker<'a> {
    worker: &'a mut LocalWorker,
    _this_thread: PhantomData<*const ()>,
}

/// Makes the queues for `workers` workers. Panics if `workers` is 0.
pub fn pool(workers: usize) -> (Arc<Pool>, Vec<LocalWorker>) {
    assert!(workers > 0, "at least one worker");
    let locals: Vec<TaskDeque<Arc<Task>>> = (0..workers).map(|_| TaskDeque::new_fifo()).collect();
    let pool = Arc::new(Pool {
        injector: Injector::new(),
        stealers: locals.iter().map(TaskDeque::stealer).collect(),
        steals: (0..workers).map(|_| AtomicU64::new(0)).collect(),
    });
    let handles = locals
        .into_iter()
        .enumerate()
        .map(|(index, local)| LocalWorker {
            pool: pool.clone(),
            index,
            local: Some(local),
            tick: 0,
        })
        .collect();
    (pool, handles)
}

impl Pool {
    /// Is any task waiting in any queue?
    pub fn has_work(&self) -> bool {
        !self.injector.is_empty() || self.stealers.iter().any(|s| !s.is_empty())
    }

    /// Looks of worker `index` that took a task from another worker's queue.
    pub fn steals(&self, index: usize) -> u64 {
        self.steals[index].load(Ordering::Relaxed)
    }
}

impl Schedule for Pool {
    fn schedule(&self, task: Arc<Task>) {
        // own queue if the innermost entered worker is one of ours
        let left = ENTERED.try_with(|entered| match entered.borrow().last() {
            Some(context) if std::ptr::eq(context.pool, self) => {
                context.local.push(task);
                None
            }
            _ => Some(task),
        });
        if let Ok(Some(task)) = left {
            self.injector.push(task);
        }
    }
}

impl LocalWorker {
    pub fn index(&self) -> usize {
        self.index
    }

    /// Until the guard is dropped, tasks this thread schedules on the pool go to this worker's queue.
    pub fn enter(&mut self) -> EnteredWorker<'_> {
        let context = WorkerContext {
            pool: Arc::as_ptr(&self.pool),
            index: self.index,
            local: self.local.take().expect("a worker is entered once at a time"),
            tick: 0,
        };
        ENTERED.with(|entered| entered.borrow_mut().push(context));
        EnteredWorker {
            worker: self,
            _this_thread: PhantomData,
        }
    }
}

impl EnteredWorker<'_> {
    fn with_context<R>(&self, f: impl FnOnce(&mut WorkerContext) -> R) -> R {
        let (pool, index) = (Arc::as_ptr(&self.worker.pool), self.worker.index);
        ENTERED.with(|entered| {
            let mut entered = entered.borrow_mut();
            let context = entered
                .iter_mut()
                .find(|c| std::ptr::eq(c.pool, pool) && c.index == index);
            f(context.expect("this worker is entered"))
        })
    }

    /// The next task for this worker, or `None` if there is none to be had.
    pub fn next_task(&mut self) -> Option<Arc<Task>> {
        let pool = self.worker.pool.clone();
        self.with_context(|context| {
            context.tick += 1;
            if context.tick % GLOBAL_EVERY == 0 {
                // otherwise a worker with a busy queue of its own would never look outside
                if let Some(task) = retrying(|| pool.injector.steal_batch_and_pop(&context.local)) {
                    return Some(task);
                }
            }
            if let Some(task) = context.local.pop() {
                return Some(task);
            }
            if let Some(task) = retrying(|| pool.injector.steal_batch_and_pop(&context.local)) {
                return Some(task);
            }
            let others = pool.stealers.iter().enumerate();
            let stolen = others
                .filter(|(i, _)| *i != context.index)
                .find_map(|(_, other)| retrying(|| other.steal_batch_and_pop(&context.local)));
            if stolen.is_some() {
                pool.steals[context.index].fetch_add(1, Ordering::Relaxed);
            }
            stolen
        })
    }

    /// Does this worker's own queue hold tasks?
    pub fn has_local_tasks(&self) -> bool {
        self.with_context(|context| !context.local.is_empty())
    }
}

/// `Retry` means "ask again", not "empty".
fn retrying<T>(mut attempt: impl FnMut() -> Steal<T>) -> Option<T> {
    loop {
        match attempt() {
            Steal::Success(value) => return Some(value),
            Steal::Empty => return None,
            Steal::Retry => {}
        }
    }
}

impl Drop for EnteredWorker<'_> {
    fn drop(&mut self) {
        let (pool, index) = (Arc::as_ptr(&self.worker.pool), self.worker.index);
        let context = ENTERED.try_with(|entered| {
            let mut entered = entered.borrow_mut();
            let at = entered
                .iter()
                .rposition(|c| std::ptr::eq(c.pool, pool) && c.index == index);
            at.map(|at| entered.remove(at))
        });
        if let Ok(Some(context)) = context {
            self.worker.tick = context.tick;
            self.worker.local = Some(context.local);
        }
    }
}

