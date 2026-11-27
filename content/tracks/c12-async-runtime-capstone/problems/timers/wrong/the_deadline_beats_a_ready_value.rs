use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::marker::PhantomData;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

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
// The current runtime (capstone step 3)
// ---------------------------------------------------------------------------------------------------------------------

thread_local! {
    /// The handles this thread has entered, oldest first.
    static CURRENT: RefCell<Vec<(u64, Handle)>> = const { RefCell::new(Vec::new()) };
    static NEXT_ENTRY: Cell<u64> = const { Cell::new(0) };
}

/// A way to spawn tasks onto a runtime, from anywhere.
#[derive(Clone)]
pub struct Handle {
    scheduler: Arc<dyn Schedule>,
    timers: Arc<Timers>,
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
        spawn_on(&self.scheduler, future)
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
// The single-threaded runtime (capstone steps 3 and 4)
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct RunQueue {
    tasks: VecDeque<Arc<Task>>,
    /// Set when the runtime is dropped: later tasks are thrown away.
    closed: bool,
}

#[derive(Default)]
struct LocalQueue {
    state: Mutex<RunQueue>,
    changed: Condvar,
    root_woken: AtomicBool,
}

impl LocalQueue {
    fn pop(&self) -> Option<Arc<Task>> {
        lock(&self.state).tasks.pop_front()
    }

    fn queued(&self) -> usize {
        lock(&self.state).tasks.len()
    }

    /// Wakes the sleeping loop so that it looks again.
    fn poke(&self) {
        // taking the lock means the sleeper is either before its check or already waiting
        drop(lock(&self.state));
        self.changed.notify_all();
    }
}

impl Schedule for LocalQueue {
    fn schedule(&self, task: Arc<Task>) {
        let mut state = lock(&self.state);
        if state.closed {
            drop(state);
            // outside the lock: destroying the future may spawn again
            drop(task);
            return;
        }
        state.tasks.push_back(task);
        drop(state);
        self.changed.notify_all();
    }
}

/// The waker of the future given to `block_on`.
struct RootWaker(Arc<LocalQueue>);

impl Wake for RootWaker {
    fn wake(self: Arc<Self>) {
        self.0.root_woken.store(true, Ordering::SeqCst);
        self.0.poke();
    }
}

/// Runs tasks on the thread that calls `block_on` or `run_until_stalled`.
pub struct LocalRuntime {
    queue: Arc<LocalQueue>,
    handle: Handle,
}

impl LocalRuntime {
    pub fn new() -> LocalRuntime {
        let queue = Arc::new(LocalQueue::default());
        let clock = ManualClock::default();
        lock(&clock.state.sleepers).push(Arc::downgrade(&queue));
        let handle = Handle {
            scheduler: queue.clone(),
            timers: Arc::new(Timers::new(clock)),
        };
        LocalRuntime { queue, handle }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    /// This runtime's clock.
    pub fn clock(&self) -> ManualClock {
        self.handle.timers.clock.clone()
    }

    /// Sleeps that are waiting for their time.
    pub fn pending_timers(&self) -> usize {
        lock(&self.handle.timers.entries).len()
    }

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle.spawn(future)
    }

    /// Runs tasks and fires due timers until neither is possible without time passing. Returns the number of runs.
    pub fn run_until_stalled(&self) -> usize {
        let _entered = self.handle.enter();
        let mut runs = 0;
        loop {
            let fired = self.handle.timers.fire_due();
            // one round of what is queued now, then the timers again
            let round = self.queue.queued();
            for task in std::iter::from_fn(|| self.queue.pop()).take(round) {
                task.run();
                runs += 1;
            }
            if fired == 0 && round == 0 {
                return runs;
            }
        }
    }

    /// Drives `future` to completion, running tasks in between and sleeping when there is nothing to do.
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        let _entered = self.handle.enter();
        let waker = Waker::from(Arc::new(RootWaker(self.queue.clone())));
        let mut cx = Context::from_waker(&waker);
        let mut future = pin!(future);
        self.queue.root_woken.store(true, Ordering::SeqCst);
        loop {
            self.handle.timers.fire_due();
            if self.queue.root_woken.swap(false, Ordering::SeqCst) {
                if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                    return value;
                }
            }
            // one task at least, and stop as soon as the root has been woken
            let round = self.queue.queued();
            for task in std::iter::from_fn(|| self.queue.pop()).take(round) {
                task.run();
                if self.queue.root_woken.load(Ordering::SeqCst) {
                    break;
                }
            }
            // sleep until there is a task, the root has been woken, or a timer is due
            drop(
                self.queue
                    .changed
                    .wait_while(lock(&self.queue.state), |state| {
                        state.tasks.is_empty()
                            && !self.queue.root_woken.load(Ordering::SeqCst)
                            && !self.handle.timers.is_due()
                    }),
            );
        }
    }
}

impl Default for LocalRuntime {
    fn default() -> Self {
        LocalRuntime::new()
    }
}

impl Drop for LocalRuntime {
    fn drop(&mut self) {
        let queued = {
            let mut state = lock(&self.queue.state);
            state.closed = true;
            std::mem::take(&mut state.tasks)
        };
        // the destructors run as if inside the runtime, and whatever they spawn is thrown away too
        let _entered = self.handle.enter();
        drop(queued);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Time (capstone step 4)
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct ClockState {
    now: Mutex<Duration>,
    /// Runtimes that must wake up when time moves.
    sleepers: Mutex<Vec<Weak<LocalQueue>>>,
}

/// A clock that moves only when told to. Clones share it, and any thread may use one.
#[derive(Clone, Default)]
pub struct ManualClock {
    state: Arc<ClockState>,
}

impl ManualClock {
    /// Time since the clock was made.
    pub fn now(&self) -> Duration {
        *lock(&self.state.now)
    }

    pub fn advance(&self, by: Duration) {
        {
            let mut now = lock(&self.state.now);
            *now = now.saturating_add(by);
        }
        let sleepers = lock(&self.state.sleepers).clone();
        for queue in sleepers.iter().filter_map(Weak::upgrade) {
            queue.poke();
        }
    }
}

/// The pending sleeps of one runtime, earliest first; equal deadlines in the order they started waiting.
struct Timers {
    clock: ManualClock,
    entries: Mutex<BTreeMap<(Duration, u64), Waker>>,
    next_seq: AtomicU64,
}

impl Timers {
    fn new(clock: ManualClock) -> Timers {
        Timers {
            clock,
            entries: Mutex::new(BTreeMap::new()),
            next_seq: AtomicU64::new(0),
        }
    }

    fn is_due(&self) -> bool {
        let now = self.clock.now();
        lock(&self.entries)
            .first_key_value()
            .is_some_and(|((deadline, _), _)| *deadline <= now)
    }

    /// Wakes every sleep whose time has come, earliest first. Returns how many.
    fn fire_due(&self) -> usize {
        let now = self.clock.now();
        let mut due = Vec::new();
        {
            let mut entries = lock(&self.entries);
            while let Some(entry) = entries.first_entry() {
                if entry.key().0 > now {
                    break;
                }
                due.push(entry.remove());
            }
        }
        // outside the lock: waking runs the scheduler
        let fired = due.len();
        due.into_iter().for_each(Waker::wake);
        fired
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
            let old = lock(&self.timers.entries).remove(&(self.deadline, seq));
            drop(old);
        }
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.timers.clock.now() >= self.deadline {
            self.unregister();
            return Poll::Ready(());
        }
        let seq = match self.seq {
            Some(seq) => seq,
            None => self.timers.next_seq.fetch_add(1, Ordering::Relaxed),
        };
        self.seq = Some(seq);
        let old = lock(&self.timers.entries).insert((self.deadline, seq), cx.waker().clone());
        drop(old);
        // the clock may have moved between the check and the entry
        if self.timers.clock.now() >= self.deadline {
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
    Handle::current().timers.clock.now()
}

pub fn sleep_until(deadline: Duration) -> Sleep {
    Sleep::new(Handle::current().timers, deadline)
}

pub fn sleep(duration: Duration) -> Sleep {
    let timers = Handle::current().timers;
    let deadline = timers.clock.now().saturating_add(duration);
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
            if Pin::new(&mut deadline).poll(cx).is_ready() {
                return Poll::Ready(Err(Elapsed));
            }
            future.as_mut().poll(cx).map(Ok)
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
    let timers = Handle::current().timers;
    let next = timers.clock.now();
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

