//! RECIPE 181 (concurrency): a work-stealing thread pool on crossbeam-deque vs a single shared-queue pool
//! LEVEL: advanced
//! Use in: CPU-bound fork-join parallelism, understanding rayon/tokio schedulers.   Run: cargo run --release --example thread_pool_work_stealing
#![allow(dead_code)]
//!
//! WHAT: each worker owns a `crossbeam_deque::Worker` (LIFO for its own jobs = cache-hot, depth-first), a global
//! `Injector` takes jobs from outside threads, and idle workers `Stealer::steal` from the FRONT of other workers'
//! deques (oldest = biggest subtrees of a recursive computation). Jobs are `Box<dyn FnOnce() + Send>`.
//! WHY: a single `Mutex<VecDeque>` is one contention point every spawn/pop crosses; with many tiny recursive jobs the
//! lock dominates. In the stealing pool the common path (push/pop on your own deque) touches no shared state.
//! HOW:
//!  * a worker's deque lives in a thread-local, so a job that calls `spawn` pushes to its own deque (no lock);
//!  * `Batch::join` HELPS: while its jobs are pending it runs other jobs instead of blocking, so recursive
//!    fork-join cannot deadlock the pool even with fewer threads than the recursion depth;
//!  * idle workers sleep on a condvar using an epoch counter (spawn bumps the epoch, sleeper re-checks it under the
//!    lock: no lost wakeups; a 100 ms timed wait is only a safety net);
//!  * panics: every job runs under `catch_unwind`; a `Batch` records the message and `join` returns `Err(messages)`;
//!  * Drop sets `shutdown`, wakes everyone and joins; workers drain remaining jobs before exiting.
//! Both pools implement the same tiny `Exec` trait so the identical parallel fib / merge sort runs on each.
//!
//! COMMON USES: data-parallel CPU work (sorting, parsing, image tiles, parallel search), the executors under rayon
//! and tokio's multi-thread runtime (per-worker queues + stealing + a global injection queue), game-engine job
//! systems, build tools. For async I/O use tokio; for CPU-bound data parallelism use rayon, shown below.
//! Read: https://github.com/rayon-rs/rayon and https://docs.rs/crossbeam-deque and https://github.com/crossbeam-rs/crossbeam
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use rayon::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

type Job = Box<dyn FnOnce() + Send + 'static>;

trait Exec: Clone + Send + Sync + 'static {
    fn spawn_job(&self, job: Job);
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
    fn spawn_job(&self, job: Job) {
        let id = self.0.id();
        let mut job = Some(job);
        CTX.with(|c| {
            if let Some(ctx) = c.borrow().as_ref().filter(|x| x.pool_id == id) {
                ctx.local.push(job.take().unwrap()); // fast path: own deque, no lock
            }
        });
        if let Some(j) = job {
            self.0.injector.push(j);
        }
        self.0.wake();
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
        let workers: Vec<Worker<Job>> = (0..n).map(|_| Worker::new_lifo()).collect();
        let shared = Arc::new(WsShared {
            injector: Injector::new(),
            stealers: workers.iter().map(|w| w.stealer()).collect(),
            lock: Mutex::new(()),
            cv: Condvar::new(),
            epoch: AtomicU64::new(0),
            sleepers: AtomicUsize::new(0),
            shutdown: AtomicU64::new(0),
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
        let task = CTX.with(|c| find_task(c.borrow().as_ref().map(|x| &x.local), &sh));
        if let Some(job) = task {
            run_job(job);
            continue;
        }
        if sh.shutdown.load(SeqCst) != 0 {
            break; // shutdown is set before the last wake, and we only exit with nothing left to find
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
        self.handle.0.shutdown.store(1, SeqCst);
        self.handle.0.wake();
        self.handle.0.cv.notify_all();
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
#[derive(Clone)]
struct QHandle(Arc<QShared>);
impl Exec for QHandle {
    fn spawn_job(&self, job: Job) {
        self.0.q.lock().unwrap().push_back(job);
        self.0.cv.notify_one();
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
        self.handle.0.shutdown.store(1, SeqCst);
        drop(self.handle.0.q.lock().unwrap());
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
    fn spawn(&self, f: impl FnOnce() + Send + 'static) {
        self.st.pending.fetch_add(1, SeqCst);
        let st = self.st.clone();
        self.exec.spawn_job(Box::new(move || {
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
        });
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
        });
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

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed())
}

fn main() {
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
                inner.spawn(move || {
                    f2[i + 1].fetch_add(1, SeqCst);
                });
                inner.join().unwrap();
            }
        });
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
        });
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
        });
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
            pool.handle.spawn_job(Box::new(move || {
                d.fetch_add(1, SeqCst);
            }));
        }
    } // drop joins all workers
    assert_eq!(done.load(SeqCst), 10_000);
    println!("drop drained 10000 queued jobs");
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
