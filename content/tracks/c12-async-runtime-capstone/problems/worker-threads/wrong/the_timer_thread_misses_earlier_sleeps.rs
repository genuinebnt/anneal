use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::Future;
use std::marker::PhantomData;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, RwLock, Weak};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;
use std::time::{Duration, Instant};

use crossbeam::deque::{Injector, Steal, Stealer, Worker as TaskDeque};

/// A panic elsewhere must not poison the runtime.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------------------------------------------------
// Given: two points where a test can act, to force a schedule that real threads rarely produce. Keep this module, the
// two calls in `take_injected` and the one in `Gate::sleep`.
// ---------------------------------------------------------------------------------------------------------------------

pub mod probe {
    use std::cell::RefCell;
    use std::thread::LocalKey;

    type Hook = RefCell<Option<Box<dyn FnOnce()>>>;

    thread_local! {
        static BATCH_MOVE: Hook = const { RefCell::new(None) };
        static SLEEP: Hook = const { RefCell::new(None) };
    }

    fn run(hook: &'static LocalKey<Hook>) {
        let f = hook.try_with(|h| h.borrow_mut().take());
        if let Ok(Some(f)) = f {
            f();
        }
    }

    /// For tests: runs `f` once, in the middle of this thread's next move of a batch out of the shared queue.
    pub fn on_next_batch_move(f: impl FnOnce() + 'static) {
        BATCH_MOVE.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    /// For tests: runs `f` once, when this thread next goes to sleep in the gate.
    pub fn on_next_sleep(f: impl FnOnce() + 'static) {
        SLEEP.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    pub(crate) fn watching_batch_moves() -> bool {
        BATCH_MOVE.try_with(|h| h.borrow().is_some()) == Ok(true)
    }

    pub(crate) fn batch_moving() {
        run(&BATCH_MOVE);
    }

    pub(crate) fn going_to_sleep() {
        run(&SLEEP);
    }
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
// Tasks (capstone steps 1, 2 and 6)
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
        let task = Task::idle(future, scheduler);
        task.clone().wake();
        task
    }

    /// A task that has not been scheduled yet.
    fn idle(
        future: impl Future<Output = ()> + Send + 'static,
        scheduler: Arc<dyn Schedule>,
    ) -> Arc<Task> {
        Arc::new(Task {
            state: QueueState::new(),
            future: Mutex::new(Some(Box::pin(future))),
            aborted: AtomicBool::new(false),
            scheduler,
        })
    }

    /// Destroys the future now. Only for a shutdown: no other thread may be running tasks of this runtime.
    fn cancel(self: Arc<Self>) {
        self.aborted.store(true, Ordering::Release);
        // either it was already waiting for a run, or this makes it so
        self.state.request();
        if !self.is_complete() {
            self.run();
        }
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
    let (task, handle) = prepare(scheduler.clone(), future);
    task.wake();
    handle
}

/// A task that is not scheduled yet, and its handle.
fn prepare<F>(scheduler: Arc<dyn Schedule>, future: F) -> (Arc<Task>, JoinHandle<F::Output>)
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
    let task = Task::idle(body, scheduler);
    let handle = JoinHandle {
        cell,
        task: Arc::downgrade(&task),
    };
    (task, handle)
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
// The current runtime (capstone step 3, and the preparation part "The current one, on this thread")
// ---------------------------------------------------------------------------------------------------------------------

thread_local! {
    /// The handles this thread has entered, oldest first.
    static CURRENT: RefCell<Vec<(u64, Handle)>> = const { RefCell::new(Vec::new()) };
    static NEXT_ENTRY: Cell<u64> = const { Cell::new(0) };
}

/// A way to spawn tasks onto a runtime, from anywhere.
#[derive(Clone)]
pub struct Handle {
    shared: Arc<Shared>,
}

/// While this lives, its handle is the thread's current one.
pub struct Entered {
    id: u64,
    _this_thread: PhantomData<*const ()>,
}

impl Handle {
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.shared.spawn(future)
    }

    fn timers(&self) -> Arc<Timers> {
        self.shared.timers.clone()
    }

    /// The current runtime's handle. Panics outside a runtime.
    pub fn current() -> Handle {
        Handle::try_current().expect("called outside a runtime")
    }

    pub fn try_current() -> Option<Handle> {
        CURRENT.with(|c| c.borrow().last().map(|(_, handle)| handle.clone()))
    }

    pub fn enter(&self) -> Entered {
        let id = NEXT_ENTRY.with(|n| {
            n.set(n.get() + 1);
            n.get()
        });
        CURRENT.with(|c| c.borrow_mut().push((id, self.clone())));
        Entered {
            id,
            _this_thread: PhantomData,
        }
    }
}

impl Drop for Entered {
    fn drop(&mut self) {
        let removed = CURRENT.try_with(|c| {
            let mut entries = c.borrow_mut();
            let at = entries.iter().rposition(|(id, _)| *id == self.id);
            at.map(|at| entries.remove(at))
        });
        drop(removed);
    }
}

/// Spawns on the current runtime. Panics outside one.
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    Handle::current().spawn(future)
}

/// Pending once: the task goes to the back of the queue.
pub fn yield_now() -> impl Future<Output = ()> + Send {
    let mut yielded = false;
    std::future::poll_fn(move |cx| {
        if yielded {
            return Poll::Ready(());
        }
        yielded = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    })
}

// ---------------------------------------------------------------------------------------------------------------------
// Time, on the real clock (capstone step 4, with a driver thread added in step 6)
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct TimerState {
    /// Earliest deadline first; equal deadlines in the order they started waiting.
    entries: BTreeMap<(Duration, u64), Waker>,
    stopped: bool,
}

/// The pending sleeps of one runtime. Deadlines are times since `start`.
struct Timers {
    start: Instant,
    state: Mutex<TimerState>,
    /// Signalled when a sleep is added and when the driver must stop.
    changed: Condvar,
    next_seq: AtomicU64,
}

impl Timers {
    fn new() -> Timers {
        Timers {
            start: Instant::now(),
            state: Mutex::default(),
            changed: Condvar::new(),
            next_seq: AtomicU64::new(0),
        }
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    /// The body of the timer thread: wakes sleeps as they come due, until `stop`.
    fn drive(&self) {
        let mut state = lock(&self.state);
        while !state.stopped {
            let now = self.now();
            let mut due = Vec::new();
            while let Some(entry) = state.entries.first_entry() {
                if entry.key().0 > now {
                    break;
                }
                due.push(entry.remove());
            }
            if !due.is_empty() {
                // outside the lock: waking runs the scheduler
                drop(state);
                due.into_iter().for_each(Waker::wake);
                state = lock(&self.state);
                continue;
            }
            state = match state.entries.first_key_value() {
                Some((&(deadline, _), _)) => {
                    // an earlier sleep added meanwhile signals `changed`
                    self.changed
                        .wait_timeout(state, deadline.saturating_sub(now))
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
                None => self
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner),
            };
        }
    }

    fn stop(&self) {
        lock(&self.state).stopped = true;
        self.changed.notify_all();
    }
}

/// Completes once the clock reaches its deadline.
pub struct Sleep {
    timers: Arc<Timers>,
    deadline: Duration,
    /// Set while this sleep has an entry in `timers`.
    seq: Option<u64>,
}

impl Sleep {
    fn new(timers: Arc<Timers>, deadline: Duration) -> Sleep {
        Sleep {
            timers,
            deadline,
            seq: None,
        }
    }

    fn unregister(&mut self) {
        if let Some(seq) = self.seq.take() {
            let old = lock(&self.timers.state)
                .entries
                .remove(&(self.deadline, seq));
            drop(old);
        }
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.timers.now() >= self.deadline {
            self.unregister();
            return Poll::Ready(());
        }
        let seq = match self.seq {
            Some(seq) => seq,
            None => self.timers.next_seq.fetch_add(1, Ordering::Relaxed),
        };
        self.seq = Some(seq);
        let old = lock(&self.timers.state)
            .entries
            .insert((self.deadline, seq), cx.waker().clone());
        drop(old);
        if lock(&self.timers.state).entries.len() == 1 {
            self.timers.changed.notify_all();
        }
        // the clock may have moved between the check and the entry
        if self.timers.now() >= self.deadline {
            self.unregister();
            return Poll::Ready(());
        }
        Poll::Pending
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        self.unregister();
    }
}

/// The current runtime's time.
pub fn now() -> Duration {
    Handle::current().timers().now()
}

pub fn sleep_until(deadline: Duration) -> Sleep {
    Sleep::new(Handle::current().timers(), deadline)
}

pub fn sleep(duration: Duration) -> Sleep {
    let timers = Handle::current().timers();
    let deadline = timers.now().saturating_add(duration);
    Sleep::new(timers, deadline)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elapsed;

/// `future`'s output, unless the time runs out first.
pub fn timeout<F: Future>(
    duration: Duration,
    future: F,
) -> impl Future<Output = Result<F::Output, Elapsed>> {
    // made now, so that the time counts from the call
    let mut deadline = sleep(duration);
    async move {
        // an `async` block drops what it holds as it finishes; a bare `poll_fn` future would keep it all
        let mut future = Box::pin(future);
        std::future::poll_fn(|cx| {
            if let Poll::Ready(value) = future.as_mut().poll(cx) {
                return Poll::Ready(Ok(value));
            }
            Pin::new(&mut deadline).poll(cx).map(|()| Err(Elapsed))
        })
        .await
    }
}

/// Ticks at its start and then once every period.
pub struct Interval {
    timers: Arc<Timers>,
    period: Duration,
    next: Duration,
}

pub fn interval(period: Duration) -> Interval {
    assert!(!period.is_zero(), "the period must not be zero");
    let timers = Handle::current().timers();
    let next = timers.now();
    Interval {
        timers,
        period,
        next,
    }
}

impl Interval {
    /// Waits for the next tick and returns the time it was due.
    pub async fn tick(&mut self) -> Duration {
        Sleep::new(self.timers.clone(), self.next).await;
        // only now, so that dropping a waiting `tick` loses nothing
        let due = self.next;
        self.next = self.next.saturating_add(self.period);
        due
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
            tick: self.tick,
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
                if let Some(task) = take_injected(&pool.injector, &context.local) {
                    return Some(task);
                }
            }
            if let Some(task) = context.local.pop() {
                return Some(task);
            }
            if let Some(task) = take_injected(&pool.injector, &context.local) {
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

/// Takes the oldest task of the shared queue, and may move more into `local`. While a batch is being moved, its tasks are
/// in neither queue, so other threads can't see them. A test uses `probe` to make that moment last.
fn take_injected(
    injector: &Injector<Arc<Task>>,
    local: &TaskDeque<Arc<Task>>,
) -> Option<Arc<Task>> {
    if !probe::watching_batch_moves() {
        return retrying(|| injector.steal_batch_and_pop(local));
    }
    let held = TaskDeque::new_fifo();
    let first = retrying(|| injector.steal_batch_and_pop(&held));
    if !held.is_empty() {
        probe::batch_moving();
        while let Some(task) = held.pop() {
            local.push(task);
        }
    }
    first
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

impl Pool {
    /// Empties every queue. Used at shutdown, so that no queued task keeps the runtime alive.
    fn clear(&self) {
        while retrying(|| self.injector.steal()).is_some() {}
        for other in &self.stealers {
            while retrying(|| other.steal()).is_some() {}
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Where idle workers sleep (preparation: "Go to sleep without missing work")
// ---------------------------------------------------------------------------------------------------------------------

struct Ticket(u64);

/// Counts notifications. A ticket sleeps only while the count has not moved since it was made.
#[derive(Default)]
struct Gate {
    notifications: Mutex<u64>,
    changed: Condvar,
}

impl Gate {
    fn prepare(&self) -> Ticket {
        Ticket(*lock(&self.notifications))
    }

    fn sleep(&self, ticket: Ticket) {
        let count = lock(&self.notifications);
        if *count == ticket.0 {
            // the lock is held until the wait begins, so no notification can come in between
            probe::going_to_sleep();
        }
        drop(self.changed.wait_while(count, |count| *count == ticket.0));
    }

    fn notify_one(&self) {
        let mut count = lock(&self.notifications);
        *count += 1;
        // signalled under the lock, so that only tickets older than this one can be asleep to receive it
        self.changed.notify_one();
        drop(count);
    }

    fn notify_all(&self) {
        *lock(&self.notifications) += 1;
        self.changed.notify_all();
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The multi-threaded runtime (capstone step 6)
// ---------------------------------------------------------------------------------------------------------------------

/// Every task that might still have a future, so that shutting down can destroy it.
#[derive(Default)]
struct Registry {
    tasks: Vec<Weak<Task>>,
    /// Finished tasks are cleared out when the list reaches this length.
    prune_at: usize,
}

/// What the workers, the handles and the tasks share.
struct Shared {
    pool: Arc<Pool>,
    gate: Gate,
    timers: Arc<Timers>,
    stopping: AtomicBool,
    /// Set once, at shutdown. Held for reading while a task is queued, so nothing is queued after the queues are drained.
    closed: RwLock<bool>,
    registry: Mutex<Registry>,
}

impl Shared {
    fn is_closed(&self) -> bool {
        *self.closed.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn spawn<F>(self: &Arc<Self>, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (task, handle) = prepare(self.clone(), future);
        {
            let mut registry = lock(&self.registry);
            // checked under the registry's lock: shutdown closes first, then takes the list
            if self.is_closed() {
                drop(registry);
                drop(task);
                return handle;
            }
            if registry.tasks.len() >= registry.prune_at {
                registry
                    .tasks
                    .retain(|t| t.upgrade().is_some_and(|t| !t.is_complete()));
                registry.prune_at = (2 * registry.tasks.len()).max(64);
            }
            registry.tasks.push(Arc::downgrade(&task));
        }
        task.wake();
        handle
    }
}

impl Schedule for Shared {
    fn schedule(&self, task: Arc<Task>) {
        let closed = self.closed.read().unwrap_or_else(PoisonError::into_inner);
        if *closed {
            drop(closed);
            drop(task);
            return;
        }
        self.pool.schedule(task);
        drop(closed);
        self.gate.notify_one();
    }
}

fn work(shared: Arc<Shared>, mut worker: LocalWorker) {
    let _current = Handle {
        shared: shared.clone(),
    }
    .enter();
    let mut worker = worker.enter();
    while !shared.stopping.load(Ordering::SeqCst) {
        if let Some(task) = worker.next_task() {
            // one wake-up may have stood for several tasks, and tasks being moved between queues can look absent to a
            // worker that is about to sleep. So whoever takes a task and still sees work wakes another worker
            if shared.pool.has_work() {
                shared.gate.notify_one();
            }
            task.run();
            continue;
        }
        let ticket = shared.gate.prepare();
        if !shared.pool.has_work() && !shared.stopping.load(Ordering::SeqCst) {
            shared.gate.sleep(ticket);
        }
    }
}

/// Wakes the thread that is inside `block_on`.
struct Unpark {
    thread: Thread,
    woken: AtomicBool,
}

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.woken.store(true, Ordering::Release);
        self.thread.unpark();
    }
}

/// A runtime with worker threads and a timer thread.
pub struct Runtime {
    shared: Arc<Shared>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Runtime {
    /// Starts `workers` worker threads. Panics if `workers` is 0.
    pub fn new(workers: usize) -> Runtime {
        let (pool, locals) = pool(workers);
        let shared = Arc::new(Shared {
            pool,
            gate: Gate::default(),
            timers: Arc::new(Timers::new()),
            stopping: AtomicBool::new(false),
            closed: RwLock::new(false),
            registry: Mutex::default(),
        });
        let mut threads: Vec<_> = locals
            .into_iter()
            .map(|local| {
                let shared = shared.clone();
                std::thread::spawn(move || work(shared, local))
            })
            .collect();
        let timers = shared.timers.clone();
        threads.push(std::thread::spawn(move || timers.drive()));
        Runtime { shared, threads }
    }

    pub fn handle(&self) -> Handle {
        Handle {
            shared: self.shared.clone(),
        }
    }

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.shared.spawn(future)
    }

    /// Drives `future` on the calling thread while the workers run the tasks.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        let _current = self.handle().enter();
        let parker = Arc::new(Unpark {
            thread: std::thread::current(),
            woken: AtomicBool::new(false),
        });
        let waker = Waker::from(parker.clone());
        let mut cx = Context::from_waker(&waker);
        let mut future = pin!(future);
        loop {
            if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
            // `park` may return for no reason, and the flag says whether this waker was used
            while !parker.woken.swap(false, Ordering::Acquire) {
                std::thread::park();
            }
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let shared = &self.shared;
        *shared.closed.write().unwrap_or_else(PoisonError::into_inner) = true;
        shared.stopping.store(true, Ordering::SeqCst);
        shared.gate.notify_all();
        shared.timers.stop();
        for thread in self.threads.drain(..) {
            // a worker only panics if the runtime itself is broken
            let _ = thread.join();
        }
        let tasks = std::mem::take(&mut lock(&shared.registry).tasks);
        let _current = self.handle().enter();
        for task in tasks.iter().filter_map(Weak::upgrade) {
            task.cancel();
        }
        // queued references keep tasks and the runtime pointing at each other
        shared.pool.clear();
    }
}

