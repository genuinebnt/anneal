//! RECIPE 181 (concurrency): a work-stealing thread pool on crossbeam-deque vs a single shared-queue pool
//! LEVEL: advanced
//! Use in: CPU-bound fork-join parallelism, understanding rayon/tokio schedulers.   Run: cargo run --release --example thread_pool_work_stealing
#![allow(dead_code)]
//!
//! WHAT: each worker owns a `crossbeam_deque::Worker` (LIFO for its own jobs), a global `Injector` takes jobs from
//! outside threads, and idle workers `Stealer::steal` from the FRONT of other workers' deques (the oldest job, which
//! in a recursive split is usually a large one). Jobs are `Box<dyn FnOnce() + Send>`.
//! WHY: a single `Mutex<VecDeque>` is a lock every spawn and pop must take; with many tiny recursive jobs it can become
//! the bottleneck. In the stealing pool, pushing and popping your own deque takes no lock. Spawning is not free of
//! shared state: every spawn updates a shared epoch counter and reads the sleeper count, and a `Batch` spawn also
//! updates the batch's pending count.
//! HOW:
//!  * a worker's deque lives in a thread-local, so a job that calls `spawn_job` pushes to its own deque (no lock);
//!  * idle workers try the other deques in index order, always starting at worker 0. That is simple and correct but
//!    does not spread thieves out (rayon and tokio's multi-thread runtime start at a random worker);
//!  * `Batch::join` HELPS: while its jobs are pending it runs other jobs instead of blocking, so fork-join that waits
//!    through `join` does not deadlock the pool even with fewer threads than the recursion depth. The cost: helped jobs
//!    nest on the waiting thread's stack with no depth limit. This demo overflowed an 8 MiB main stack in some debug
//!    runs, so it now calls from a thread with a large stack. A job that blocks any other way still holds its thread;
//!  * idle workers sleep on a condvar using an epoch counter (spawn bumps the epoch, sleeper re-checks it under the
//!    lock: no lost wakeups; a 100 ms timed wait is only a safety net);
//!  * panics: every job runs under `catch_unwind` (unwinding panics only); a `Batch` records the message and `join`
//!    returns `Err(messages)`;
//!  * shutdown: once Drop starts, `spawn_job` from outside the pool returns `Err(Rejected(job))`. Jobs already
//!    accepted still run, including jobs they spawn from a worker. A worker notes whether shutdown has started BEFORE
//!    it searches, and exits only if it had and the search found nothing; Drop then joins every worker.
//!
//! Both pools implement the same tiny `Exec` trait so the identical parallel fib / merge sort runs on each.
//!
//! COMMON USES: data-parallel CPU work (sorting, parsing, image tiles, parallel search), game-engine job systems,
//! build tools. Rayon and tokio's multi-thread runtime also schedule by work stealing, with far more refined queues
//! and sleeping. For async I/O use tokio; for CPU-bound data parallelism use rayon, shown below.
//! Read: https://github.com/rayon-rs/rayon and https://docs.rs/crossbeam-deque and https://github.com/crossbeam-rs/crossbeam
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use rayon::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Barrier, Condvar, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

type Job = Box<dyn FnOnce() + Send + 'static>;

/// The pool is shutting down, so the job was not queued. It is handed back to the caller.
struct Rejected(Job);
impl std::fmt::Debug for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Rejected(job): the pool is shutting down")
    }
}

trait Exec: Clone + Send + Sync + 'static {
    /// Queues a job. `Err` means it was refused and will never run unless the caller runs it.
    fn spawn_job(&self, job: Job) -> Result<(), Rejected>;
    /// Run one pending job on the calling thread if any; used by `Batch::join` to help instead of block.
    fn help_one(&self) -> bool;
}

fn run_job(job: Job) {
    let _ = panic::catch_unwind(AssertUnwindSafe(job)); // isolation: never kill the worker
}

// ---------------------------------------------------------------- work-stealing pool
struct WsShared {
    injector: Injector<Job>,
    stealers: Vec<Stealer<Job>>,
    lock: Mutex<()>,
    cv: Condvar,
    epoch: AtomicU64,
    sleepers: AtomicUsize,
    shutdown: AtomicU64,
    /// Outside callers hold this for reading while they push, so none can push after it is set.
    closed: RwLock<bool>,
    gate: Option<Arc<Gate>>,
}
/// Test hook: parks one worker, once, right after a search that found nothing.
struct Gate {
    armed: AtomicBool,
    reached: Barrier,
    resume: Barrier,
}
impl Gate {
    fn new() -> Arc<Self> {
        Arc::new(Gate {
            armed: AtomicBool::new(true),
            reached: Barrier::new(2),
            resume: Barrier::new(2),
        })
    }
    fn pause_once(&self) {
        if self.armed.swap(false, SeqCst) {
            self.reached.wait();
            self.resume.wait();
        }
    }
}
struct WorkerCtx {
    pool_id: usize,
    local: Worker<Job>,
}
thread_local! { static CTX: RefCell<Option<WorkerCtx>> = const { RefCell::new(None) }; }

#[derive(Clone)]
struct WsHandle(Arc<WsShared>);

impl WsShared {
    fn id(self: &Arc<Self>) -> usize {
        Arc::as_ptr(self) as usize
    }
    fn wake(&self) {
        self.epoch.fetch_add(1, SeqCst);
        if self.sleepers.load(SeqCst) > 0 {
            drop(self.lock.lock().unwrap()); // order against the sleeper's check-then-wait
            self.cv.notify_one();
        }
    }
}

fn find_task(local: Option<&Worker<Job>>, sh: &WsShared) -> Option<Job> {
    if let Some(j) = local.and_then(|l| l.pop()) {
        return Some(j);
    }
    loop {
        let s = match local {
            Some(l) => sh.injector.steal_batch_and_pop(l),
            None => sh.injector.steal(),
        }
        .or_else(|| sh.stealers.iter().map(|s| s.steal()).collect());
        match s {
            Steal::Success(j) => return Some(j),
            Steal::Empty => return None,
            Steal::Retry => continue,
        }
    }
}

impl Exec for WsHandle {
    fn spawn_job(&self, job: Job) -> Result<(), Rejected> {
        let id = self.0.id();
        let mut job = Some(job);
        CTX.with(|c| {
            if let Some(ctx) = c.borrow().as_ref().filter(|x| x.pool_id == id) {
                ctx.local.push(job.take().unwrap()); // fast path: own deque, no lock
            }
        });
        if let Some(j) = job {
            let closed = self.0.closed.read().unwrap(); // held across the push
            if *closed {
                return Err(Rejected(j));
            }
            self.0.injector.push(j);
        }
        self.0.wake();
        Ok(())
    }
    fn help_one(&self) -> bool {
        let id = self.0.id();
        let task = CTX.with(|c| {
            let b = c.borrow();
            let local = b.as_ref().filter(|x| x.pool_id == id).map(|x| &x.local);
            find_task(local, &self.0)
        }); // borrow released before running the job (jobs re-enter CTX)
        task.map(run_job).is_some()
    }
}

struct WsPool {
    handle: WsHandle,
    threads: Vec<thread::JoinHandle<()>>,
}
impl WsPool {
    fn new(n: usize) -> Self {
        Self::with_gate(n, None)
    }
    fn with_gate(n: usize, gate: Option<Arc<Gate>>) -> Self {
        let workers: Vec<Worker<Job>> = (0..n).map(|_| Worker::new_lifo()).collect();
        let shared = Arc::new(WsShared {
            injector: Injector::new(),
            stealers: workers.iter().map(|w| w.stealer()).collect(),
            lock: Mutex::new(()),
            cv: Condvar::new(),
            epoch: AtomicU64::new(0),
            sleepers: AtomicUsize::new(0),
            shutdown: AtomicU64::new(0),
            closed: RwLock::new(false),
            gate,
        });
        let threads = workers
            .into_iter()
            .map(|local| {
                let sh = shared.clone();
                thread::Builder::new()
                    .stack_size(16 << 20)
                    .spawn(move || ws_worker(sh, local))
                    .unwrap()
            })
            .collect();
        WsPool {
            handle: WsHandle(shared),
            threads,
        }
    }
    /// Starts shutting down without waiting for the workers; `drop` then joins them.
    fn begin_shutdown(&self) {
        *self.handle.0.closed.write().unwrap() = true; // waits for outside pushes in progress
        self.handle.0.shutdown.store(1, SeqCst); // set after the last outside push
        self.handle.0.wake();
        self.handle.0.cv.notify_all();
    }
}
fn ws_worker(sh: Arc<WsShared>, local: Worker<Job>) {
    CTX.with(|c| {
        *c.borrow_mut() = Some(WorkerCtx {
            pool_id: sh.id(),
            local,
        })
    });
    loop {
        let e = sh.epoch.load(SeqCst);
        // read BEFORE searching: if this is true, every outside job is already queued where the search can see it
        let stopping = sh.shutdown.load(SeqCst) != 0;
        let task = CTX.with(|c| find_task(c.borrow().as_ref().map(|x| &x.local), &sh));
        if let Some(job) = task {
            run_job(job);
            continue;
        }
        if let Some(gate) = &sh.gate {
            gate.pause_once();
        }
        if stopping {
            break; // shutdown had started before a search that found nothing
        }
        let g = sh.lock.lock().unwrap();
        sh.sleepers.fetch_add(1, SeqCst);
        let g = if sh.epoch.load(SeqCst) == e && sh.shutdown.load(SeqCst) == 0 {
            sh.cv.wait_timeout(g, Duration::from_millis(100)).unwrap().0
        } else {
            g
        };
        sh.sleepers.fetch_sub(1, SeqCst);
        drop(g);
    }
}
impl Drop for WsPool {
    fn drop(&mut self) {
        self.begin_shutdown();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

// ---------------------------------------------------------------- baseline: one shared locked queue
struct QShared {
    q: Mutex<VecDeque<Job>>,
    cv: Condvar,
    shutdown: AtomicU64,
}
thread_local! { static Q_WORKER_OF: Cell<usize> = const { Cell::new(0) }; }
#[derive(Clone)]
struct QHandle(Arc<QShared>);
impl Exec for QHandle {
    fn spawn_job(&self, job: Job) -> Result<(), Rejected> {
        let mut q = self.0.q.lock().unwrap();
        let own_worker = Q_WORKER_OF.get() == Arc::as_ptr(&self.0) as usize;
        if self.0.shutdown.load(SeqCst) != 0 && !own_worker {
            return Err(Rejected(job));
        }
        q.push_back(job);
        drop(q);
        self.0.cv.notify_one();
        Ok(())
    }
    fn help_one(&self) -> bool {
        let j = self.0.q.lock().unwrap().pop_back(); // newest first, same locality as the LIFO deques
        j.map(run_job).is_some()
    }
}
struct QPool {
    handle: QHandle,
    threads: Vec<thread::JoinHandle<()>>,
}
impl QPool {
    fn new(n: usize) -> Self {
        let sh = Arc::new(QShared {
            q: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            shutdown: AtomicU64::new(0),
        });
        let threads = (0..n)
            .map(|_| {
                let sh = sh.clone();
                thread::Builder::new()
                    .stack_size(16 << 20)
                    .spawn(move || {
                        Q_WORKER_OF.set(Arc::as_ptr(&sh) as usize);
                        let mut g = sh.q.lock().unwrap();
                        loop {
                            if let Some(j) = g.pop_back() {
                                drop(g);
                                run_job(j);
                                g = sh.q.lock().unwrap();
                            } else if sh.shutdown.load(SeqCst) != 0 {
                                break;
                            } else {
                                g = sh.cv.wait_timeout(g, Duration::from_millis(100)).unwrap().0;
                            }
                        }
                    })
                    .unwrap()
            })
            .collect();
        QPool {
            handle: QHandle(sh),
            threads,
        }
    }
}
impl Drop for QPool {
    fn drop(&mut self) {
        {
            let _q = self.handle.0.q.lock().unwrap(); // spawners check the flag under this lock
            self.handle.0.shutdown.store(1, SeqCst);
        }
        self.handle.0.cv.notify_all();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

// ---------------------------------------------------------------- batch = scope-like join
struct BatchState {
    pending: AtomicUsize,
    panics: Mutex<Vec<String>>,
}
struct Batch<E: Exec> {
    exec: E,
    st: Arc<BatchState>,
}
impl<E: Exec> Batch<E> {
    fn new(exec: &E) -> Self {
        Batch {
            exec: exec.clone(),
            st: Arc::new(BatchState {
                pending: AtomicUsize::new(0),
                panics: Mutex::new(Vec::new()),
            }),
        }
    }
    fn spawn(&self, f: impl FnOnce() + Send + 'static) -> Result<(), Rejected> {
        self.st.pending.fetch_add(1, SeqCst);
        let st = self.st.clone();
        let queued = self.exec.spawn_job(Box::new(move || {
            if let Err(p) = panic::catch_unwind(AssertUnwindSafe(f)) {
                let msg = p
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| p.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "non-string panic".into());
                st.panics.lock().unwrap().push(msg);
            }
            st.pending.fetch_sub(1, SeqCst); // last: join may return as soon as this hits zero
        }));
        if queued.is_err() {
            self.st.pending.fetch_sub(1, SeqCst); // a refused job must not be waited for
        }
        queued
    }
    /// Wait for every job spawned on this batch; helps run other jobs meanwhile. Err = panic messages.
    fn join(self) -> Result<(), Vec<String>> {
        let mut idle = 0u32;
        while self.st.pending.load(SeqCst) > 0 {
            if self.exec.help_one() {
                idle = 0;
            } else {
                idle += 1;
                if idle < 100 {
                    thread::yield_now()
                } else {
                    thread::sleep(Duration::from_micros(50))
                }
            }
        }
        let p = std::mem::take(&mut *self.st.panics.lock().unwrap());
        if p.is_empty() { Ok(()) } else { Err(p) }
    }
}

// ---------------------------------------------------------------- fork-join workloads
static JOBS_RUN: AtomicUsize = AtomicUsize::new(0);

fn fib_seq(n: u32) -> u64 {
    if n < 2 {
        n as u64
    } else {
        fib_seq(n - 1) + fib_seq(n - 2)
    }
}
fn fib_par<E: Exec>(ex: &E, n: u32, cutoff: u32) -> u64 {
    if n < cutoff {
        return fib_seq(n);
    }
    let slot = Arc::new(AtomicU64::new(0));
    let b = Batch::new(ex);
    {
        let (ex, slot) = (ex.clone(), slot.clone());
        b.spawn(move || {
            JOBS_RUN.fetch_add(1, SeqCst);
            slot.store(fib_par(&ex, n - 1, cutoff), SeqCst);
        })
        .expect("pool is open");
    }
    let right = fib_par(ex, n - 2, cutoff);
    b.join().expect("fib child");
    slot.load(SeqCst) + right
}
fn fib_jobs(n: u32, cutoff: u32) -> usize {
    if n < cutoff {
        0
    } else {
        1 + fib_jobs(n - 1, cutoff) + fib_jobs(n - 2, cutoff)
    }
}

fn merge(a: Vec<u32>, b: Vec<u32>) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] <= b[j] {
            out.push(a[i]);
            i += 1
        } else {
            out.push(b[j]);
            j += 1
        }
    }
    out.extend_from_slice(&a[i..]);
    out.extend_from_slice(&b[j..]);
    out
}
fn sort_par<E: Exec>(ex: &E, mut v: Vec<u32>, cutoff: usize) -> Vec<u32> {
    if v.len() <= cutoff {
        v.sort_unstable();
        return v;
    }
    let right = v.split_off(v.len() / 2);
    let slot: Arc<Mutex<Option<Vec<u32>>>> = Arc::new(Mutex::new(None));
    let b = Batch::new(ex);
    {
        let (ex, slot) = (ex.clone(), slot.clone());
        b.spawn(move || {
            JOBS_RUN.fetch_add(1, SeqCst);
            *slot.lock().unwrap() = Some(sort_par(&ex, right, cutoff));
        })
        .expect("pool is open");
    }
    let left = sort_par(ex, v, cutoff);
    b.join().expect("sort child");
    let r = slot.lock().unwrap().take().unwrap();
    merge(left, r)
}
fn sort_jobs(n: usize, cutoff: usize) -> usize {
    if n <= cutoff {
        0
    } else {
        1 + sort_jobs(n / 2, cutoff) + sort_jobs(n - n / 2, cutoff)
    }
}

// ---------------------------------------------------------------- shutdown self-checks

/// A job queued after a worker's empty search, but before it decides whether to exit, must still run.
fn check_job_queued_in_the_gap_runs() {
    let gate = Gate::new();
    let pool = WsPool::with_gate(1, Some(gate.clone()));
    gate.reached.wait(); // the only worker has searched, found nothing, and is parked
    let ran = Arc::new(AtomicUsize::new(0));
    let r = ran.clone();
    pool.handle
        .spawn_job(Box::new(move || {
            r.fetch_add(1, SeqCst);
        }))
        .expect("pool is open");
    pool.begin_shutdown();
    gate.resume.wait();
    drop(pool);
    assert_eq!(ran.load(SeqCst), 1, "a job queued before shutdown was lost");
}

/// A handle kept after the pool is gone is told so, and gets its job back.
fn check_spawn_after_drop_is_refused<E: Exec>(pool: impl Sized, handle: E) {
    drop(pool);
    let ran = Arc::new(AtomicUsize::new(0));
    let r = ran.clone();
    let refused = handle.spawn_job(Box::new(move || {
        r.fetch_add(1, SeqCst);
    }));
    let Err(Rejected(job)) = refused else {
        panic!("a job was accepted by a pool that has no workers");
    };
    assert_eq!(ran.load(SeqCst), 0);
    job(); // the caller still owns it
    assert_eq!(ran.load(SeqCst), 1);
    let batch = Batch::new(&handle);
    assert!(batch.spawn(|| {}).is_err());
    batch.join().expect("a refused job is not waited for");
}

/// Refusing outsiders must not break the drain: a queued job may still split while the pool shuts down.
fn check_job_can_spawn_during_shutdown() {
    let gate = Gate::new();
    let pool = WsPool::with_gate(1, Some(gate.clone()));
    gate.reached.wait();
    let ran = Arc::new(AtomicUsize::new(0));
    let (r, inside) = (ran.clone(), pool.handle.clone());
    pool.handle
        .spawn_job(Box::new(move || {
            inside
                .spawn_job(Box::new(move || {
                    r.fetch_add(1, SeqCst);
                }))
                .expect("a worker of this pool may still spawn");
        }))
        .expect("pool is open");
    pool.begin_shutdown();
    gate.resume.wait();
    drop(pool);
    assert_eq!(ran.load(SeqCst), 1, "the child job was lost");
}

/// Outside threads keep spawning while the pool is dropped. The interleaving varies; the invariant may not:
/// every job that was accepted has run by the time `drop` returns.
fn check_every_accepted_job_runs() {
    for _ in 0..20 {
        let ran = Arc::new(AtomicUsize::new(0));
        let pool = WsPool::new(2);
        let start = Arc::new(Barrier::new(4));
        let spawners: Vec<_> = (0..3)
            .map(|_| {
                let (h, ran, start) = (pool.handle.clone(), ran.clone(), start.clone());
                thread::spawn(move || {
                    start.wait();
                    let mut accepted = 0usize;
                    while accepted < 2_000 {
                        let r = ran.clone();
                        let job: Job = Box::new(move || {
                            r.fetch_add(1, SeqCst);
                        });
                        if h.spawn_job(job).is_err() {
                            break;
                        }
                        accepted += 1;
                    }
                    accepted
                })
            })
            .collect();
        start.wait();
        drop(pool);
        let ran_when_drop_returned = ran.load(SeqCst);
        let accepted: usize = spawners.into_iter().map(|t| t.join().unwrap()).sum();
        assert_eq!(
            ran_when_drop_returned, accepted,
            "accepted jobs and jobs run differ"
        );
    }
}

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed())
}

/// The thread that calls `join` helps, so pending jobs pile up on its stack, as deep as chance allows. In 240 debug
/// runs the caller used up to 18 MiB, more than a main thread usually has. This is generous headroom, not a bound.
const CALLER_STACK: usize = 256 << 20;

fn main() {
    let demo = thread::Builder::new().stack_size(CALLER_STACK).spawn(demo);
    if let Err(panic) = demo.expect("spawn the demo thread").join() {
        panic::resume_unwind(panic);
    }
}

fn demo() {
    let n = thread::available_parallelism()
        .map_or(4, |p| p.get())
        .clamp(2, 8);
    let ws = WsPool::new(n);
    let qp = QPool::new(n);
    println!("threads per pool: {n}");

    // 1. every job runs exactly once (per-job flags), from outside AND from inside workers
    const N: usize = 50_000;
    let flags: Arc<Vec<AtomicU8>> = Arc::new((0..N).map(|_| AtomicU8::new(0)).collect());
    let b = Batch::new(&ws.handle);
    for i in 0..N {
        let (f, h) = (flags.clone(), ws.handle.clone());
        b.spawn(move || {
            f[i].fetch_add(1, SeqCst);
            if i % 100 == 0 {
                // nested spawn from a worker goes to its local deque
                let inner = Batch::new(&h);
                let f2 = f.clone();
                inner
                    .spawn(move || {
                        f2[i + 1].fetch_add(1, SeqCst);
                    })
                    .expect("pool is open");
                inner.join().unwrap();
            }
        })
        .expect("pool is open");
    }
    b.join().unwrap();
    let total: usize = flags.iter().map(|f| f.load(SeqCst) as usize).sum();
    assert_eq!(total, N + N / 100, "job count");
    assert!(
        flags
            .iter()
            .enumerate()
            .all(|(i, f)| f.load(SeqCst) == if i % 100 == 1 { 2 } else { 1 }),
        "some job ran 0 or 2+ times"
    );
    println!("exactly-once: {N} jobs + {} nested all ran once", N / 100);

    // 2. panic isolation: the panic surfaces on join, workers survive
    let prev = panic::take_hook();
    panic::set_hook(Box::new(|_| {})); // keep the demo output clean
    let b = Batch::new(&ws.handle);
    for i in 0..20 {
        b.spawn(move || {
            if i == 7 {
                panic!("boom {i}")
            }
        })
        .expect("pool is open");
    }
    let err = b.join().unwrap_err();
    panic::set_hook(prev);
    assert_eq!(err, vec!["boom 7".to_string()]);
    let after = Arc::new(AtomicUsize::new(0));
    let b = Batch::new(&ws.handle);
    for _ in 0..1000 {
        let a = after.clone();
        b.spawn(move || {
            a.fetch_add(1, SeqCst);
        })
        .expect("pool is open");
    }
    b.join().unwrap();
    assert_eq!(after.load(SeqCst), 1000);
    println!("panic isolation: join reported {err:?}; pool still healthy");

    // 3. fork-join comparison
    let (fn_, cutoff) = (34u32, 22u32);
    let (expect, t_seq) = time(|| fib_seq(fn_));
    JOBS_RUN.store(0, SeqCst);
    let (r1, t_ws) = time(|| fib_par(&ws.handle, fn_, cutoff));
    assert_eq!(r1, expect);
    assert_eq!(
        JOBS_RUN.load(SeqCst),
        fib_jobs(fn_, cutoff),
        "fib: every job ran exactly once"
    );
    JOBS_RUN.store(0, SeqCst);
    let (r2, t_q) = time(|| fib_par(&qp.handle, fn_, cutoff));
    assert_eq!(r2, expect);
    assert_eq!(JOBS_RUN.load(SeqCst), fib_jobs(fn_, cutoff));
    let (r3, t_rayon) = time(|| fib_rayon(fn_, cutoff));
    assert_eq!(r3, expect);
    println!(
        "fib({fn_}) cutoff {cutoff}: seq {t_seq:?} | work-stealing {t_ws:?} ({:.2}x) | shared-queue {t_q:?} ({:.2}x) | rayon join {t_rayon:?}",
        t_seq.as_secs_f64() / t_ws.as_secs_f64(),
        t_seq.as_secs_f64() / t_q.as_secs_f64()
    );

    // tiny cutoff = many tiny jobs: where the single lock hurts most
    let (small, c2) = (27u32, 8u32);
    let ((a, t_ws2), (b2, t_q2)) = (
        time(|| fib_par(&ws.handle, small, c2)),
        time(|| fib_par(&qp.handle, small, c2)),
    );
    assert_eq!((a, b2), (fib_seq(small), fib_seq(small)));
    println!(
        "fib({small}) cutoff {c2} ({} jobs): work-stealing {t_ws2:?} vs shared-queue {t_q2:?} (informational)",
        fib_jobs(small, c2)
    );

    let mut x = 0x2545F4914F6CDD1Du64;
    let data: Vec<u32> = (0..1_000_000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 16) as u32
        })
        .collect();
    let mut want = data.clone();
    want.sort_unstable();
    let sc = 16_384;
    JOBS_RUN.store(0, SeqCst);
    let (s1, t1) = time(|| sort_par(&ws.handle, data.clone(), sc));
    assert_eq!(s1, want);
    assert_eq!(JOBS_RUN.load(SeqCst), sort_jobs(data.len(), sc));
    let (s2, t2) = time(|| sort_par(&qp.handle, data.clone(), sc));
    assert_eq!(s2, want);
    let (s3, t3) = time(|| {
        let mut d = data.clone();
        d.par_sort_unstable();
        d
    });
    assert_eq!(s3, want);
    println!(
        "merge sort 1M u32: work-stealing {t1:?} | shared-queue {t2:?} | rayon par_sort_unstable {t3:?}"
    );

    // 4. rayon equivalent of a whole batch of parallel work: one line
    let sum_sq: u64 = (0..1_000_000u64).into_par_iter().map(|v| v * v % 7).sum();
    assert_eq!(sum_sq, (0..1_000_000u64).map(|v| v * v % 7).sum::<u64>());
    println!(
        "rayon one-liner: (0..1_000_000u64).into_par_iter().map(|v| v * v % 7).sum() = {sum_sq}"
    );

    // 5. graceful shutdown on Drop: queued jobs are drained, not lost
    let done = Arc::new(AtomicUsize::new(0));
    {
        let pool = WsPool::new(3);
        for _ in 0..10_000 {
            let d = done.clone();
            pool.handle
                .spawn_job(Box::new(move || {
                    d.fetch_add(1, SeqCst);
                }))
                .expect("pool is open");
        }
    } // drop joins all workers
    assert_eq!(done.load(SeqCst), 10_000);
    println!("drop drained 10000 queued jobs");

    // 6. the edges of shutdown
    check_job_queued_in_the_gap_runs();
    check_job_can_spawn_during_shutdown();
    check_every_accepted_job_runs();
    let (a, b) = (WsPool::new(2), QPool::new(2));
    let (ha, hb) = (a.handle.clone(), b.handle.clone());
    check_spawn_after_drop_is_refused(a, ha);
    check_spawn_after_drop_is_refused(b, hb);
    println!("shutdown: accepted jobs all ran; late outside spawns were refused");
    drop(qp);
    drop(ws);
    println!("thread_pool_work_stealing: all assertions passed");
}

fn fib_rayon(n: u32, cutoff: u32) -> u64 {
    if n < cutoff {
        return fib_seq(n);
    }
    let (a, b) = rayon::join(|| fib_rayon(n - 1, cutoff), || fib_rayon(n - 2, cutoff));
    a + b
}
