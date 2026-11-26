//! RECIPE 1067 (std_tour): `std::thread` in practice: ownership across threads, scoped parallelism, panic handling, worker pools, thread locals, parking.
//! LEVEL: intermediate
//! Use in: parallelising CPU work without extra crates, background workers, per-thread caches.   Run: cargo run --release --example std_thread_scope_local_park
//! COMMON USES: split a slice across cores; a fixed worker pool for blocking jobs; per-thread scratch buffers and counters; supervising a thread that may panic.
//! METHODS: thread::spawn, thread::scope, Scope::spawn, thread::Builder::new, Builder::name, Builder::spawn, thread::sleep, thread::yield_now,
//! thread::park, thread::park_timeout, Thread::unpark, thread::current, Thread::id, Thread::name, thread::available_parallelism, thread::panicking,
//! JoinHandle::join, JoinHandle::thread, JoinHandle::is_finished, ScopedJoinHandle::join, ScopedJoinHandle::is_finished, thread_local!,
//! LocalKey::with, LocalKey::with_borrow, LocalKey::with_borrow_mut, LocalKey::get, LocalKey::set, std::panic::catch_unwind, std::panic::resume_unwind,
//! std::panic::panic_any, slice::chunks, slice::chunks_mut, slice::split_at_mut
//! DOCS: https://doc.rust-lang.org/std/thread/index.html
//! DEFINED IN: std: all of `std::thread`, `thread_local!`, `LocalKey`, `catch_unwind`, `resume_unwind`. alloc: `Arc`. core: `chunks_mut`, atomics.
//! NOT COVERED (and why): `Thread::into_raw`/`from_raw`, `ThreadId::as_u64`, `thread::sleep_until`, `add_spawn_hook`, `LocalKey::update` (unstable in 1.98.1);
//! thread priorities and CPU affinity (not in std); rayon (see `rayon_data_parallelism`). Mutex/channel/Condvar details are in `std_sync_primitives_use_cases`.
//! Course: none. Goes beyond `threads_channels` (first steps) and `structured_concurrency_cancellation`.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   run something in the background and get its result   -> `let h = thread::spawn(move || ..); h.join()`
//!   borrow local data from threads                       -> `thread::scope` (no Arc, no 'static)
//!   split `&mut [T]` between threads                     -> `chunks_mut` / `split_at_mut`: the borrow checker proves they are disjoint
//!   uneven work items                                    -> threads claim the next index from an `AtomicUsize`
//!   choose a thread count                                -> `available_parallelism().map_or(1, NonZero::get)`, capped by the work
//!   combine results                                      -> return a value from each thread and merge after `join` (no shared lock)
//!   keep a worker alive after a bad job                  -> `catch_unwind` around the job
//!   pass a child panic on to the caller                  -> `h.join().unwrap_or_else(|p| resume_unwind(p))`
//!   skip cleanup that is unsafe during unwinding         -> `thread::panicking()` inside `Drop`
//!   per-thread cache / counter                           -> `thread_local!`; publish totals explicitly, before the thread ends
//!   wait for one condition                               -> `park` in a loop + `unpark`; for anything richer use `Condvar` or a channel
//!   channels or shared state?                            -> moving work or ownership: channel. Many readers/writers of one structure: lock. Independent partial results: neither.
//!
//! PITFALLS: dropping a `JoinHandle` DETACHES the thread (it keeps running; when `main` returns the process ends and kills it);
//! `scope` waits for each thread's main function but TLS destructors may still be running when it returns (an explicit `join` waits for them);
//! `thread::sleep` as synchronisation is a race; `available_parallelism` can over- or under-count (cgroups, affinity, VMs).
//! TRY NEXT:
//!   - [S] In section 2 change `chunks(chunk)` to `chunks(chunk - 1)` and say what happens to the thread count; then break it with `chunk == 0`.
//!   - [S] In section 3, do not join the panicking scoped thread. Predict what `thread::scope` does and assert it with `catch_unwind`.
//!   - [M] Add `Pool::execute_with_result` returning a `mpsc::Receiver<T>`; a panicking job must make `recv` return `Err`.
//!   - [M] Replace the static split in `par_map` with the atomic-index scheme and verify output order is unchanged for skewed work.
//!   - [A] PROJECT: combine with `std_sync_primitives_use_cases` and `std_time_instant_duration_systemtime`: a pool with a bounded queue, per-job deadline and metrics per worker.
//! Read: https://doc.rust-lang.org/std/thread/fn.scope.html ; https://doc.rust-lang.org/std/thread/struct.LocalKey.html ; https://marabos.nl/atomics/basics.html
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

const EXPECTED: &str = "expected-demo-panic";

fn install_quiet_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let p = info.payload();
        let msg = p
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| p.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("");
        if !msg.contains(EXPECTED) && !msg.contains("a scoped thread panicked") {
            default(info);
        }
    }));
}

fn workers_for(items: usize) -> usize {
    thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(items)
        .max(1)
}

// ------------------------------------------------------------ 1. spawn: 'static, move, results, handles
fn s01_spawn() {
    let data = vec![1u64, 2, 3];
    // `spawn` requires 'static: the thread may outlive this function. So MOVE the data in, and get it back through join.
    let h = thread::spawn(move || {
        let sum: u64 = data.iter().sum();
        (data, sum)
    });
    let (data, sum) = h.join().unwrap();
    assert_eq!((data.len(), sum), (3, 6));

    // sharing instead of moving: clone an Arc per thread
    let table = Arc::new(vec!["a", "b"]);
    let hs: Vec<_> = (0..2)
        .map(|i| {
            let t = Arc::clone(&table);
            thread::spawn(move || t[i].to_uppercase())
        })
        .collect();
    // results come back in JOIN order (here spawn order), not completion order
    assert_eq!(
        hs.into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>(),
        ["A", "B"]
    );

    // PITFALL: `(0..n).map(spawn).map(join)` is lazy: it spawns one, joins it, spawns the next: fully sequential.
    // Collect the handles first, as above.
    let running = AtomicUsize::new(0);
    let max_seen = AtomicUsize::new(0);
    thread::scope(|s| {
        (0..4)
            .map(|_| {
                s.spawn(|| {
                    max_seen
                        .fetch_max(running.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                    running.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .map(|h| h.join().unwrap())
            .for_each(drop);
    });
    assert_eq!(
        max_seen.load(Ordering::SeqCst),
        1,
        "the lazy chain never had two threads alive"
    );

    // identity
    let me = thread::current().id();
    let h = thread::Builder::new()
        .name("probe".into())
        .spawn(|| thread::current().id())
        .unwrap();
    assert_eq!(h.thread().name(), Some("probe")); // JoinHandle::thread: inspect or unpark it before joining
    let handle_id = h.thread().id();
    let inside = h.join().unwrap();
    assert!(inside == handle_id && inside != me);

    // is_finished: a non-blocking poll. Here the thread is held open by a channel, so both answers are deterministic.
    let (go, wait) = mpsc::channel::<()>();
    let h = thread::spawn(move || wait.recv().unwrap());
    assert!(!h.is_finished());
    go.send(()).unwrap();
    while !h.is_finished() {
        thread::yield_now(); // a hint to the scheduler, not a guarantee that anything else runs
    }
    h.join().unwrap();

    // dropping the handle detaches: the thread still runs to completion if the process lives long enough
    let (tx, rx) = mpsc::channel();
    drop(thread::spawn(move || tx.send(42).unwrap()));
    assert_eq!(rx.recv(), Ok(42));
    thread::sleep(Duration::from_millis(1));
    println!("1. spawn ok");
}

// ------------------------------------------------------------ 2. scope: data parallelism on borrowed slices
fn par_sum(xs: &[u64]) -> u64 {
    if xs.is_empty() {
        return 0; // chunks(0) would panic
    }
    let chunk = xs.len().div_ceil(workers_for(xs.len()));
    thread::scope(|s| {
        let hs: Vec<_> = xs
            .chunks(chunk)
            .map(|c| s.spawn(move || c.iter().sum::<u64>()))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).sum()
    })
}

fn par_apply<T: Send>(xs: &mut [T], threads: usize, f: impl Fn(&mut T) + Sync) {
    if xs.is_empty() {
        return;
    }
    let chunk = xs.len().div_ceil(threads.max(1));
    thread::scope(|s| {
        for c in xs.chunks_mut(chunk) {
            let f = &f; // share the closure by reference (hence F: Sync)
            s.spawn(move || c.iter_mut().for_each(f));
        }
    });
}

/// Order-preserving parallel map with dynamic scheduling: good when item costs differ widely.
fn par_map_dynamic<T: Sync, R: Send>(
    xs: &[T],
    threads: usize,
    f: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let mut tagged: Vec<(usize, R)> = thread::scope(|s| {
        let hs: Vec<_> = (0..threads.max(1))
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed); // atomicity alone makes each index unique
                        let Some(x) = xs.get(i) else { return mine };
                        mine.push((i, f(x)));
                    }
                })
            })
            .collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    tagged.sort_by_key(|(i, _)| *i);
    tagged.into_iter().map(|(_, r)| r).collect()
}

fn s02_scope() {
    for n in [0usize, 1, 2, 7, 1_000, 10_001] {
        let xs: Vec<u64> = (0..n as u64).collect();
        assert_eq!(par_sum(&xs), xs.iter().sum::<u64>(), "n={n}");
        for threads in [1, 3, 8, 64] {
            let mut ys = xs.clone();
            par_apply(&mut ys, threads, |y| *y = *y * 3 + 1);
            let want: Vec<u64> = xs.iter().map(|x| x * 3 + 1).collect();
            assert_eq!(ys, want, "apply n={n} t={threads}");
            assert_eq!(
                par_map_dynamic(&xs, threads, |x| x * 3 + 1),
                want,
                "dynamic n={n} t={threads}"
            );
        }
    }
    // two disjoint halves of one buffer
    let mut buf = [0u8; 10];
    let (a, b) = buf.split_at_mut(5);
    thread::scope(|s| {
        s.spawn(|| a.fill(1));
        s.spawn(|| b.fill(2));
    });
    assert_eq!(buf, [1, 1, 1, 1, 1, 2, 2, 2, 2, 2]);

    // scoped threads can spawn siblings on the same scope
    let count = AtomicUsize::new(0);
    thread::scope(|s| {
        s.spawn(|| {
            count.fetch_add(1, Ordering::Relaxed);
            s.spawn(|| count.fetch_add(1, Ordering::Relaxed));
        });
    });
    assert_eq!(count.load(Ordering::Relaxed), 2);
    let h_done = thread::scope(|s| {
        let h = s.spawn(|| 5);
        while !h.is_finished() {
            thread::yield_now();
        }
        h.join().unwrap()
    });
    assert_eq!(h_done, 5);
    println!("2. scoped parallelism ok");
}

// ------------------------------------------------------------ 3. Panics across threads
struct Guard<'a> {
    rolled_back: &'a AtomicBool,
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.rolled_back.store(true, Ordering::SeqCst); // like a transaction guard
        }
    }
}

#[derive(Debug)]
struct JobFailed {
    code: u8,
}

fn panic_message(p: &(dyn std::any::Any + Send)) -> &str {
    p.downcast_ref::<&str>()
        .copied()
        .or_else(|| p.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string payload>")
}

fn s03_panics() {
    // a panic ends only its own thread; the parent learns of it from join
    let r = thread::spawn(|| -> u32 { panic!("{EXPECTED}: boom") }).join();
    assert_eq!(
        panic_message(r.unwrap_err().as_ref()),
        "expected-demo-panic: boom"
    );
    // typed payloads with panic_any (the hook above cannot read them, so silence it for this one call)
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let p = thread::spawn(|| std::panic::panic_any(JobFailed { code: 3 }))
        .join()
        .unwrap_err();
    std::panic::set_hook(old);
    assert_eq!(p.downcast::<JobFailed>().unwrap().code, 3);

    // Drop runs during unwinding; thread::panicking tells the two exits apart
    let flag = AtomicBool::new(false);
    thread::scope(|s| s.spawn(|| drop(Guard { rolled_back: &flag })).join()).unwrap();
    assert!(!flag.load(Ordering::SeqCst));
    assert!(
        thread::scope(|s| s
            .spawn(|| {
                let _g = Guard { rolled_back: &flag };
                panic!("{EXPECTED}")
            })
            .join())
        .is_err()
    );
    assert!(flag.load(Ordering::SeqCst));

    // resume_unwind re-raises the ORIGINAL payload in the caller without running the panic hook again
    let caught = catch_unwind(|| {
        thread::spawn(|| panic!("{EXPECTED}: original"))
            .join()
            .unwrap_or_else(|p| resume_unwind(p))
    })
    .map(|()| ())
    .unwrap_err();
    assert_eq!(
        panic_message(caught.as_ref()),
        "expected-demo-panic: original"
    );

    // handle each worker's failure instead of letting the scope re-panic
    let results: Vec<Result<u32, String>> = thread::scope(|s| {
        let hs: Vec<_> = (0..4u32)
            .map(|i| {
                s.spawn(move || {
                    if i == 2 {
                        panic!("{EXPECTED}: worker {i}")
                    } else {
                        i * 10
                    }
                })
            })
            .collect();
        hs.into_iter()
            .map(|h| h.join().map_err(|p| panic_message(p.as_ref()).to_string()))
            .collect()
    });
    assert_eq!(results[2], Err("expected-demo-panic: worker 2".to_string()));
    assert_eq!(
        results.iter().filter_map(|r| r.as_ref().ok()).sum::<u32>(),
        40
    );
    // NOTE: catch_unwind and join(Err) do nothing under `panic = "abort"`: the process dies at the panic.
    println!("3. panic handling ok");
}

// ------------------------------------------------------------ 4. A fixed worker pool
type Job = Box<dyn FnOnce() + Send + 'static>;

struct Pool {
    tx: Option<mpsc::Sender<Job>>,
    workers: Vec<thread::JoinHandle<()>>,
    panicked: Arc<AtomicUsize>,
}
impl Pool {
    fn new(n: usize) -> Pool {
        assert!(n > 0);
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx)); // std's Receiver is single-consumer: share it behind a Mutex
        let panicked = Arc::new(AtomicUsize::new(0));
        let workers = (0..n)
            .map(|i| {
                let (rx, panicked) = (Arc::clone(&rx), Arc::clone(&panicked));
                thread::Builder::new()
                    .name(format!("pool-{i}"))
                    .spawn(move || {
                        loop {
                            // The guard is a temporary of THIS statement, so the lock is released before the job runs.
                            // `while let Ok(job) = rx.lock().unwrap().recv()` would hold it for the whole body: one job at a time.
                            let msg = rx.lock().unwrap().recv();
                            let Ok(job) = msg else { break }; // all senders gone and queue drained
                            // AssertUnwindSafe: the job is consumed and nothing it half-modified is reused by the worker
                            if catch_unwind(AssertUnwindSafe(job)).is_err() {
                                panicked.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    })
                    .expect("spawn worker")
            })
            .collect();
        Pool {
            tx: Some(tx),
            workers,
            panicked,
        }
    }
    fn execute(&self, job: impl FnOnce() + Send + 'static) {
        self.tx
            .as_ref()
            .unwrap()
            .send(Box::new(job))
            .expect("workers alive");
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        drop(self.tx.take()); // close the queue FIRST, or the joins below wait forever
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

fn s04_pool() {
    let done = Arc::new(AtomicU64::new(0));
    let names = Arc::new(Mutex::new(HashSet::new()));
    let panicked;
    {
        let pool = Pool::new(3);
        panicked = Arc::clone(&pool.panicked);
        for i in 1..=300u64 {
            let (done, names) = (Arc::clone(&done), Arc::clone(&names));
            pool.execute(move || {
                if i % 100 == 0 {
                    panic!("{EXPECTED}: bad job {i}");
                }
                names
                    .lock()
                    .unwrap()
                    .insert(thread::current().name().unwrap().to_string());
                done.fetch_add(i, Ordering::SeqCst);
            });
        }
    } // Drop: drains the queue, then joins
    assert_eq!(
        done.load(Ordering::SeqCst),
        (1..=300).filter(|i| i % 100 != 0).sum::<u64>()
    );
    assert_eq!(
        panicked.load(Ordering::SeqCst),
        3,
        "bad jobs were contained; the workers survived"
    );
    assert!(names.lock().unwrap().iter().all(|n| n.starts_with("pool-")));

    // prove jobs really overlap: two jobs that each wait for the other would deadlock a pool that ran one at a time
    let pool = Pool::new(2);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let (tx, rx) = mpsc::channel();
    for _ in 0..2 {
        let (b, tx) = (Arc::clone(&barrier), tx.clone());
        pool.execute(move || {
            b.wait();
            tx.send(()).unwrap();
        });
    }
    for _ in 0..2 {
        rx.recv_timeout(Duration::from_secs(30))
            .expect("both jobs must be running at once");
    }
    println!("4. worker pool ok");
}

// ------------------------------------------------------------ 5. Thread locals
thread_local! {
    static HITS: Cell<u64> = const { Cell::new(0) };
    static BUF: RefCell<String> = const { RefCell::new(String::new()) };
    static FLUSH: RefCell<Option<FlushOnExit>> = const { RefCell::new(None) };
}
static GLOBAL_HITS: AtomicU64 = AtomicU64::new(0);

struct FlushOnExit(Arc<AtomicBool>);
impl Drop for FlushOnExit {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Reuses one allocation per thread. The closure gets a borrow that cannot escape.
fn with_formatted<R>(id: u32, f: impl FnOnce(&str) -> R) -> R {
    BUF.with_borrow_mut(|b| {
        use std::fmt::Write;
        b.clear();
        write!(b, "req-{id:06}").unwrap();
        f(b) // PITFALL: calling with_formatted again inside `f` panics ("already borrowed")
    })
}

fn s05_thread_local() {
    assert_eq!(with_formatted(42, str::to_owned), "req-000042");
    let cap = BUF.with_borrow(String::capacity);
    with_formatted(7, |s| assert_eq!(s, "req-000007"));
    assert_eq!(BUF.with_borrow(String::capacity), cap, "buffer reused");
    assert!(
        catch_unwind(|| {
            let old = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let r = catch_unwind(|| with_formatted(1, |_| with_formatted(2, |_| ())));
            std::panic::set_hook(old);
            r.is_err()
        })
        .unwrap()
    );

    // sharded counter: count locally without contention, then publish once, explicitly, before the thread ends
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                for _ in 0..10_000 {
                    HITS.set(HITS.get() + 1);
                }
                GLOBAL_HITS.fetch_add(HITS.get(), Ordering::Relaxed);
            });
        }
    });
    assert_eq!(GLOBAL_HITS.load(Ordering::Relaxed), 40_000);
    assert_eq!(HITS.get(), 0, "the main thread's copy was never touched");

    // TLS destructors run as a spawned thread exits, and an explicit join waits for them. (Do not rely on this at the end of
    // `scope` without joining, and never for the main thread: its destructors may not run.)
    let dropped = Arc::new(AtomicBool::new(false));
    let d = Arc::clone(&dropped);
    thread::spawn(move || FLUSH.set(Some(FlushOnExit(d))))
        .join()
        .unwrap();
    assert!(dropped.load(Ordering::SeqCst));
    println!("5. thread locals ok");
}

// ------------------------------------------------------------ 6. park / unpark: a one-shot slot built by hand
/// Teaching version of a one-value handoff. In production use `mpsc::sync_channel(1)` or `OnceLock`.
fn handoff<T: Send>(produce: impl FnOnce() -> T + Send) -> T {
    let slot: Mutex<Option<T>> = Mutex::new(None);
    let consumer = thread::current();
    thread::scope(|s| {
        s.spawn(|| {
            let v = produce();
            *slot.lock().unwrap() = Some(v); // 1. publish
            consumer.unpark(); // 2. wake
        });
        loop {
            // check, then park. An unpark that lands between the check and the park leaves a token, so no wake-up is lost.
            if let Some(v) = slot.lock().unwrap().take() {
                return v;
            }
            thread::park(); // may also return spuriously: hence the loop
        }
    })
}

fn s06_park() {
    for i in 0..500 {
        assert_eq!(handoff(move || i * 2), i * 2);
    }
    // tokens do not accumulate: many unparks still release only ONE park without blocking
    let me = thread::current();
    me.unpark();
    me.unpark();
    me.unpark();
    thread::park(); // consumes the single token
    thread::park_timeout(Duration::from_millis(1)); // no token left; returns by timeout (or spuriously)
    println!("6. park / unpark ok");
}

// ------------------------------------------------------------ 7. Word count: three designs, one answer
fn count_words(text: &str, out: &mut BTreeMap<String, usize>) {
    for w in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        *out.entry(w.to_ascii_lowercase()).or_default() += 1;
    }
}

fn wc_merge(docs: &[String], threads: usize) -> BTreeMap<String, usize> {
    // (a) no sharing: each thread builds its own map; merge at the end
    let chunk = docs.len().div_ceil(threads).max(1);
    thread::scope(|s| {
        let hs: Vec<_> = docs
            .chunks(chunk)
            .map(|c| {
                s.spawn(move || {
                    let mut m = BTreeMap::new();
                    c.iter().for_each(|d| count_words(d, &mut m));
                    m
                })
            })
            .collect();
        let mut total = BTreeMap::new();
        for h in hs {
            for (k, v) in h.join().unwrap() {
                *total.entry(k).or_default() += v;
            }
        }
        total
    })
}

fn wc_locked(docs: &[String], threads: usize) -> BTreeMap<String, usize> {
    // (b) shared map: compute OUTSIDE the lock, merge inside it
    let total = Mutex::new(BTreeMap::new());
    thread::scope(|s| {
        for c in docs.chunks(docs.len().div_ceil(threads).max(1)) {
            let total = &total;
            s.spawn(move || {
                for d in c {
                    let mut local = BTreeMap::new();
                    count_words(d, &mut local);
                    let mut g = total.lock().unwrap();
                    for (k, v) in local {
                        *g.entry(k).or_insert(0) += v;
                    }
                }
            });
        }
    });
    total.into_inner().unwrap()
}

/// (c) pipeline: reader -> counters -> aggregator, with bounded channels so a fast reader cannot buffer everything.
/// Returns (counts, the largest number of documents that were ever in flight).
fn wc_pipeline(docs: &[String], threads: usize, cap: usize) -> (BTreeMap<String, usize>, usize) {
    let in_flight = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let (doc_tx, doc_rx) = mpsc::sync_channel::<&str>(cap);
    let (map_tx, map_rx) = mpsc::sync_channel::<BTreeMap<String, usize>>(cap);
    let doc_rx = Mutex::new(doc_rx);
    let counts = thread::scope(|s| {
        s.spawn(|| {
            let tx = doc_tx; // move the sender in so it drops when the reader finishes
            for d in docs {
                // count BEFORE sending: counting afterwards would let a fast worker decrement first and underflow
                peak.fetch_max(
                    in_flight.fetch_add(1, Ordering::SeqCst) + 1,
                    Ordering::SeqCst,
                );
                tx.send(d).unwrap(); // blocks while the queue is full: backpressure
            }
        });
        for _ in 0..threads {
            let (doc_rx, map_tx, in_flight) = (&doc_rx, map_tx.clone(), &in_flight);
            s.spawn(move || {
                loop {
                    let msg = doc_rx.lock().unwrap().recv();
                    let Ok(d) = msg else { break };
                    let mut m = BTreeMap::new();
                    count_words(d, &mut m);
                    map_tx.send(m).unwrap();
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                }
            });
        }
        drop(map_tx); // keep only the workers' clones, so the loop below ends
        let mut total = BTreeMap::new();
        for m in map_rx {
            for (k, v) in m {
                *total.entry(k).or_insert(0) += v;
            }
        }
        total
    });
    (counts, peak.load(Ordering::SeqCst))
}

fn s07_word_count() {
    let words = ["alpha", "Beta", "gamma", "delta", "x"];
    let docs: Vec<String> = (0..400)
        .map(|i| {
            (0..30)
                .map(|j| words[(i * 7 + j * j) % words.len()])
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect();
    let mut oracle = BTreeMap::new();
    docs.iter().for_each(|d| count_words(d, &mut oracle));
    assert_eq!(oracle.values().sum::<usize>(), 400 * 30);
    for threads in [1, 2, 8] {
        assert_eq!(wc_merge(&docs, threads), oracle);
        assert_eq!(wc_locked(&docs, threads), oracle);
        let (got, peak) = wc_pipeline(&docs, threads, 4);
        assert_eq!(got, oracle);
        // at most: 4 queued, 1 held by each worker, plus the 1 the reader is blocked trying to send
        assert!(
            peak <= 4 + threads + 1,
            "peak {peak} with {threads} workers"
        );
    }
    println!("7. word count: merge / lock / pipeline agree ok");
}

fn main() {
    install_quiet_hook();
    s01_spawn();
    s02_scope();
    s03_panics();
    s04_pool();
    s05_thread_local();
    s06_park();
    s07_word_count();
    println!("all thread sections passed");
}
