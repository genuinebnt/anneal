//! RECIPE 1048 (std-sync): `std::sync` and thread coordination by use case: Mutex, RwLock, Condvar, Once, OnceLock, LazyLock, Barrier, mpsc, Arc.
//! LEVEL: intermediate
//! Use in: shared server state, connection pools, lazy config, worker queues, graceful shutdown.   Run: cargo run --release --example std_sync_primitives_use_cases
//! COMMON USES: `Arc<Mutex<T>>` shared state; `RwLock` config snapshots; `OnceLock`/`LazyLock` globals; bounded `sync_channel` for backpressure; `Condvar` blocking queues and pools.
//! METHODS: Mutex::new, Mutex::lock, Mutex::try_lock, Mutex::is_poisoned, Mutex::clear_poison, Mutex::get_mut, Mutex::into_inner, PoisonError::into_inner,
//! RwLock::read, RwLock::write, RwLock::try_read, RwLock::try_write, RwLock::is_poisoned, RwLockWriteGuard::downgrade, Condvar::wait, Condvar::wait_while,
//! Condvar::wait_timeout_while, Condvar::notify_one, Condvar::notify_all, WaitTimeoutResult::timed_out, Once::call_once, Once::call_once_force,
//! Once::is_completed, OnceState::is_poisoned, OnceLock::get, OnceLock::set, OnceLock::get_or_init, OnceLock::wait, OnceLock::into_inner, LazyLock::new,
//! LazyLock::force, LazyLock::get, Barrier::new, Barrier::wait, BarrierWaitResult::is_leader, mpsc::channel, mpsc::sync_channel, Sender::send,
//! SyncSender::send, SyncSender::try_send, Receiver::recv, Receiver::try_recv, Receiver::recv_timeout, Receiver::iter, Receiver::try_iter,
//! Arc::new, Arc::clone, Arc::strong_count, Arc::downgrade, Weak::upgrade, Arc::make_mut, Arc::ptr_eq, Arc::try_unwrap, Arc::into_inner,
//! thread::scope, thread::Builder::name, thread::Builder::stack_size, thread::Builder::spawn, thread::Builder::spawn_scoped, thread::current,
//! thread::park, Thread::unpark, JoinHandle::join, thread::available_parallelism, thread_local!, LocalKey::with, LocalKey::try_with,
//! LocalKey::set, LocalKey::take, LocalKey::replace, LocalKey::get, LocalKey::with_borrow, LocalKey::with_borrow_mut
//! DOCS: https://doc.rust-lang.org/std/sync/index.html
//! DEFINED IN: std for `Mutex`, `RwLock`, `Condvar`, `Once`, `OnceLock`, `LazyLock`, `Barrier`, `mpsc`, `std::thread`, `thread_local!` (they need the OS).
//!   alloc: `Arc` and `Weak` (`alloc::sync`). core: atomics (`core::sync::atomic`), `OnceCell`/`LazyCell` (`core::cell`, single-threaded).
//! NOT COVERED (and why): `std::sync::nonpoison`, `ReentrantLock`, `mpmc`, `oneshot`, mapped guards, `Mutex::{get_cloned,set,replace}`, `Receiver::recv_deadline`,
//! `LocalKey::update`, `OnceLock::get_mut_or_init`, `SyncView` (formerly `Exclusive`) (all unstable in 1.98.1); async locks (see tokio examples); atomics (see std_atomics_ordering_use_cases).
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   mutate shared state                      -> `Arc<Mutex<T>>`; borrowed from the stack: `thread::scope` + `&Mutex<T>`
//!   many readers, rare writer                -> `RwLock<T>` (measure first; or swap an `Arc<T>` under a short lock)
//!   survive a panic in another thread        -> `lock().unwrap_or_else(PoisonError::into_inner)`; after repairing: `clear_poison()`
//!   do not block                             -> `try_lock` / `try_read` / `try_write` (`WouldBlock` or `Poisoned`)
//!   sleep until state changes                -> `condvar.wait_while(guard, |s| !ready(s))`
//!   same, with a deadline                    -> `wait_timeout_while` then check `timed_out()`
//!   run an initialiser once, no value        -> `Once::call_once`
//!   global set from runtime input            -> `static X: OnceLock<T>` + `get_or_init` / `set`
//!   global with a fixed initialiser          -> `static X: LazyLock<T> = LazyLock::new(..)`
//!   all workers reach a phase boundary       -> `Barrier`
//!   hand work to other threads               -> `mpsc::sync_channel(n)` (bounded, backpressure); `channel()` is unbounded; `0` is a rendezvous
//!   know when all producers are done         -> drop every `Sender`; `for x in rx` then ends
//!   copy-on-write shared snapshot            -> `Arc::make_mut`
//!   cache without keeping alive              -> `Weak` + `upgrade`
//!   borrow locals in threads                 -> `thread::scope`
//!   name threads (visible in panics)         -> `thread::Builder::new().name(..)`
//!   per-thread buffer / request id           -> `thread_local!` + `with_borrow_mut` / `set`
//!
//! PITFALLS: re-locking a Mutex you already hold deadlocks or panics (unspecified which); two reads of one RwLock on one thread can deadlock if a writer
//! queues between them; RwLock fairness is OS-dependent (writer or reader starvation are both permitted); `Condvar::wait` may wake spuriously.
//! TRY NEXT:
//!   - [S] In section 1 replace `lock_ordered` by "lock `from` first, then `to`". Do not run it with 8 threads without a watchdog: explain the interleaving that deadlocks.
//!   - [S] Predict, then assert: after a panic inside a `RwLock::read` guard, what does `is_poisoned()` return?
//!   - [M] Add `Pool::checkout_timeout` with `wait_timeout_while` and test the "pool exhausted" path without sleeping in the test itself.
//!   - [M] Turn the pipeline in section 6 into a fan-out of 4 workers sharing one `Mutex<Receiver>` and prove every job is processed exactly once.
//!   - [A] PROJECT: combine with `std_net_tcp_udp_ip_addr` and `std_time_instant_duration_systemtime`: a loopback server with a bounded worker pool, load shedding via `try_send`, and drain-on-shutdown.
//! Read: https://doc.rust-lang.org/std/sync/index.html ; https://doc.rust-lang.org/std/sync/struct.Condvar.html ; Mara Bos, Rust Atomics and Locks https://marabos.nl/atomics/
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvError, RecvTimeoutError, TryRecvError, TrySendError};
use std::sync::{
    Arc, Barrier, Condvar, LazyLock, Mutex, MutexGuard, Once, OnceLock, PoisonError, RwLock,
    RwLockWriteGuard, TryLockError,
};
use std::thread;
use std::time::Duration;

/// Panics raised on purpose by this file carry this marker; the hook stays quiet for them and loud for real failures.
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
        // std's own follow-on panics (a poisoned Once/LazyLock, a re-panicking scope) cannot carry our marker.
        if !msg.contains(EXPECTED)
            && !msg.contains("previously been poisoned")
            && !msg.contains("a scoped thread panicked")
        {
            default(info);
        }
    }));
}

fn panics<R>(f: impl FnOnce() -> R) -> bool {
    catch_unwind(AssertUnwindSafe(f)).is_err()
}

// ------------------------------------------------------------ 1. Mutex: shared state, scoped threads, lock ordering
struct Account {
    id: u32,
    balance: Mutex<i64>,
}

/// Deadlock avoidance: every thread acquires the two locks in the same global order (by id), whatever the transfer direction.
fn transfer(from: &Account, to: &Account, amount: i64) -> bool {
    assert_ne!(from.id, to.id, "same account would lock one mutex twice");
    let (first, second) = if from.id < to.id {
        (from, to)
    } else {
        (to, from)
    };
    let a: MutexGuard<i64> = first.balance.lock().unwrap();
    let b: MutexGuard<i64> = second.balance.lock().unwrap();
    let (mut src, mut dst) = if from.id < to.id { (a, b) } else { (b, a) };
    if *src < amount {
        return false;
    }
    *src -= amount;
    *dst += amount;
    true
}

fn s01_mutex() {
    let accounts: Vec<Account> = (0..5)
        .map(|id| Account {
            id,
            balance: Mutex::new(1_000),
        })
        .collect();
    let moved = AtomicUsize::new(0);
    thread::scope(|s| {
        // scope: the threads borrow `accounts` and `moved` directly, no Arc needed
        for t in 0..8usize {
            let (accounts, moved) = (&accounts, &moved);
            s.spawn(move || {
                for i in 0..2_000usize {
                    let from = (t + i) % 5;
                    let to = (from + 1 + (i % 4)) % 5;
                    if transfer(&accounts[from], &accounts[to], (i % 50) as i64) {
                        moved.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
    }); // all threads are joined here
    let total: i64 = accounts.iter().map(|a| *a.balance.lock().unwrap()).sum();
    assert_eq!(total, 5_000, "money is conserved");
    assert!(accounts.iter().all(|a| *a.balance.lock().unwrap() >= 0));
    assert!(moved.load(Ordering::Relaxed) > 0);

    // exclusive access needs no locking: `get_mut(&mut self)` and `into_inner(self)`
    static HITS: Mutex<Vec<u32>> = Mutex::new(Vec::new()); // const constructor: usable in statics without LazyLock
    HITS.lock().unwrap().push(1);
    assert_eq!(HITS.lock().unwrap().len(), 1);
    let mut m = Mutex::new(5);
    *m.get_mut().unwrap() += 1;
    assert_eq!(m.into_inner().unwrap(), 6);

    // try_lock: never blocks. Probed from another thread, so we never re-lock on the owning thread.
    let m = Mutex::new(0);
    let held = m.lock().unwrap();
    thread::scope(|s| {
        s.spawn(|| assert!(matches!(m.try_lock(), Err(TryLockError::WouldBlock))));
    });
    drop(held);
    assert!(m.try_lock().is_ok());

    // PITFALL: a temporary guard in a `match` scrutinee lives until the END of the match, not just until the arm starts.
    let queue = Mutex::new(vec![1, 2, 3]);
    match queue.lock().unwrap().last() {
        Some(_) => thread::scope(|s| {
            s.spawn(|| {
                assert!(
                    matches!(queue.try_lock(), Err(TryLockError::WouldBlock)),
                    "guard still held inside the arm"
                )
            });
        }),
        None => unreachable!(),
    }
    // Fix: copy the value out first; the temporary guard dies at the end of the `let`.
    let last = queue.lock().unwrap().last().copied();
    match last {
        Some(_) => assert!(queue.try_lock().is_ok()),
        None => unreachable!(),
    }
    println!("1. mutex ok");
}

// ------------------------------------------------------------ 2. Poisoning: what it means, how to recover
#[derive(Default)]
struct Ledger {
    entries: Vec<i64>,
    total: i64,
} // invariant: total == sum(entries)

fn s02_poison() {
    let ledger = Arc::new(Mutex::new(Ledger::default()));
    let l2 = Arc::clone(&ledger);
    let joined = thread::spawn(move || {
        let mut g = l2.lock().unwrap();
        g.entries.push(10);
        panic!("{EXPECTED}: crashed between the two halves of an update");
    })
    .join();
    assert!(joined.is_err());
    assert!(ledger.is_poisoned());
    assert!(
        ledger.lock().is_err(),
        "every later lock reports the poison"
    );
    assert!(matches!(ledger.try_lock(), Err(TryLockError::Poisoned(_))));

    // The data is still reachable; poisoning is a warning that an invariant MAY be broken. Here it is.
    {
        let mut g = ledger.lock().unwrap_or_else(PoisonError::into_inner);
        assert_ne!(
            g.total,
            g.entries.iter().sum::<i64>(),
            "the invariant really is broken"
        );
        g.total = g.entries.iter().sum(); // repair
    }
    assert!(ledger.is_poisoned(), "into_inner does not clear the flag");
    ledger.clear_poison(); // 1.77: declare the state good again
    assert!(!ledger.is_poisoned());
    assert_eq!(ledger.lock().unwrap().total, 10);

    // A panic while NOT holding the guard does not poison.
    let m = Mutex::new(0);
    assert!(panics(|| {
        drop(m.lock().unwrap());
        panic!("{EXPECTED}");
    }));
    assert!(!m.is_poisoned());

    // RwLock is poisoned only by a panic while locked for WRITING.
    let rw = RwLock::new(0);
    assert!(panics(|| {
        let _r = rw.read().unwrap();
        panic!("{EXPECTED} in reader");
    }));
    assert!(!rw.is_poisoned());
    assert!(panics(|| {
        let _w = rw.write().unwrap();
        panic!("{EXPECTED} in writer");
    }));
    assert!(rw.is_poisoned() && rw.read().is_err());
    rw.clear_poison();
    assert_eq!(*rw.read().unwrap(), 0);
    println!("2. poisoning ok");
}

// ------------------------------------------------------------ 3. RwLock: config snapshots
#[derive(Clone, Debug, PartialEq)]
struct Config {
    version: u32,
    max_conns: u32,
}

fn s03_rwlock() {
    let cfg = RwLock::new(Arc::new(Config {
        version: 1,
        max_conns: 10,
    }));
    // many readers at once
    {
        let a = cfg.read().unwrap();
        thread::scope(|s| {
            s.spawn(|| {
                let b = cfg
                    .try_read()
                    .expect("a second reader (other thread, no writer queued) gets in");
                assert_eq!(b.version, 1);
                assert!(
                    matches!(cfg.try_write(), Err(TryLockError::WouldBlock)),
                    "writers are excluded while a reader holds it"
                );
            });
        });
        assert_eq!(a.version, 1);
    }
    // Production pattern: hold the lock only long enough to clone the Arc; use the snapshot lock-free.
    let snapshot: Arc<Config> = Arc::clone(&cfg.read().unwrap());
    *cfg.write().unwrap() = Arc::new(Config {
        version: 2,
        max_conns: 20,
    });
    assert_eq!((snapshot.version, cfg.read().unwrap().version), (1, 2));

    // downgrade (1.92): turn a write guard into a read guard with no gap in which another writer could slip in.
    let mut w = cfg.write().unwrap();
    *w = Arc::new(Config {
        version: 3,
        max_conns: 30,
    });
    let r = RwLockWriteGuard::downgrade(w);
    thread::scope(|s| {
        s.spawn(|| assert!(matches!(cfg.try_write(), Err(TryLockError::WouldBlock))));
    });
    assert_eq!(r.version, 3);
    drop(r);

    // consistency under contention: a writer keeps two fields in step; readers must never see them differ.
    let pair = RwLock::new((0u64, 0u64));
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                for _ in 0..2_000 {
                    let g = pair.read().unwrap();
                    assert_eq!(g.0 * 2, g.1);
                }
            });
        }
        s.spawn(|| {
            for i in 1..=2_000 {
                let mut g = pair.write().unwrap();
                g.0 = i;
                g.1 = i * 2;
            }
        });
    });
    assert_eq!(*pair.read().unwrap(), (2_000, 4_000));
    println!("3. rwlock ok");
}

// ------------------------------------------------------------ 4. Condvar: a connection pool and a bounded blocking queue
struct Pool {
    idle: Mutex<Vec<u32>>,
    returned: Condvar,
}
impl Pool {
    fn new(n: u32) -> Self {
        Pool {
            idle: Mutex::new((0..n).collect()),
            returned: Condvar::new(),
        }
    }
    fn checkout(&self) -> u32 {
        // wait_while re-checks the predicate after every wake-up, so spurious wake-ups are harmless.
        let mut idle = self
            .returned
            .wait_while(self.idle.lock().unwrap(), |v| v.is_empty())
            .unwrap();
        idle.pop().expect("predicate guarantees non-empty")
    }
    fn checkout_timeout(&self, d: Duration) -> Option<u32> {
        // The timeout bounds the TOTAL wait, even across several wake-ups.
        let (mut idle, res) = self
            .returned
            .wait_timeout_while(self.idle.lock().unwrap(), d, |v| v.is_empty())
            .unwrap();
        if res.timed_out() { None } else { idle.pop() }
    }
    fn checkin(&self, c: u32) {
        self.idle.lock().unwrap().push(c);
        self.returned.notify_one(); // one item became available: waking one waiter is enough
    }
}

struct BlockingQueue<T> {
    state: Mutex<(VecDeque<T>, bool)>, // (items, closed)
    not_empty: Condvar,
    not_full: Condvar,
    cap: usize,
}
impl<T> BlockingQueue<T> {
    fn new(cap: usize) -> Self {
        BlockingQueue {
            state: Mutex::new((VecDeque::new(), false)),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            cap,
        }
    }
    fn push(&self, item: T) -> Result<(), T> {
        let mut g = self
            .not_full
            .wait_while(self.state.lock().unwrap(), |(q, closed)| {
                !*closed && q.len() == self.cap
            })
            .unwrap();
        if g.1 {
            return Err(item);
        }
        g.0.push_back(item);
        assert!(g.0.len() <= self.cap);
        self.not_empty.notify_one();
        Ok(())
    }
    fn pop(&self) -> Option<T> {
        // The hand-written form of wait_while: `while !condition { guard = cv.wait(guard) }`. Never use `if`.
        let mut g = self.state.lock().unwrap();
        while g.0.is_empty() && !g.1 {
            g = self.not_empty.wait(g).unwrap();
        }
        let item = g.0.pop_front(); // None only when closed AND drained
        self.not_full.notify_one();
        item
    }
    fn close(&self) {
        self.state.lock().unwrap().1 = true;
        // EVERY waiter must observe `closed`, so notify_all, on both condvars.
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }
}

fn s04_condvar() {
    let pool = Pool::new(3);
    let in_use = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                for _ in 0..200 {
                    let c = pool.checkout();
                    let now = in_use.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    thread::yield_now();
                    in_use.fetch_sub(1, Ordering::SeqCst);
                    pool.checkin(c);
                }
            });
        }
    });
    assert!(
        peak.load(Ordering::SeqCst) <= 3,
        "never more checkouts than connections"
    );
    let mut ids = pool.idle.lock().unwrap().clone();
    ids.sort();
    assert_eq!(ids, [0, 1, 2]);

    // deadline path: nobody returns anything, so the predicate stays true and the wait must time out
    let empty = Pool::new(0);
    assert_eq!(empty.checkout_timeout(Duration::from_millis(5)), None);
    assert!(
        pool.checkout_timeout(Duration::from_millis(5)).is_some(),
        "predicate already false: returns without waiting"
    );

    let q = BlockingQueue::new(4);
    let sum = AtomicUsize::new(0);
    thread::scope(|s| {
        let consumers: Vec<_> = (0..3)
            .map(|_| {
                s.spawn(|| {
                    while let Some(x) = q.pop() {
                        sum.fetch_add(x, Ordering::Relaxed);
                    }
                })
            })
            .collect();
        let producers: Vec<_> = (0..2)
            .map(|p| {
                let q = &q;
                s.spawn(move || {
                    for i in 0..500 {
                        q.push(p * 500 + i).unwrap();
                    }
                })
            })
            .collect();
        producers.into_iter().for_each(|h| h.join().unwrap());
        q.close(); // consumers drain what is left, then see None
        consumers.into_iter().for_each(|h| h.join().unwrap());
    });
    assert_eq!(sum.load(Ordering::Relaxed), (0..1000).sum());
    assert_eq!(q.push(1), Err(1), "closed queue hands the item back");
    println!("4. condvar ok");
}

// ------------------------------------------------------------ 5. One-time initialisation: Once, OnceLock, LazyLock
static SQUARES_BUILDS: AtomicUsize = AtomicUsize::new(0);
static SQUARES: LazyLock<Vec<u64>> = LazyLock::new(|| {
    SQUARES_BUILDS.fetch_add(1, Ordering::SeqCst);
    (0..100).map(|i| i * i).collect()
});

fn database_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| "postgres://localhost/app".to_string())
}

fn s05_once() {
    // Once: a side effect that must run exactly once, with every caller waiting until it has finished.
    static INIT: Once = Once::new();
    let runs = AtomicUsize::new(0);
    thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                INIT.call_once(|| {
                    runs.fetch_add(1, Ordering::SeqCst);
                });
                assert_eq!(
                    runs.load(Ordering::SeqCst),
                    1,
                    "after call_once returns, the effect is visible"
                );
            });
        }
    });
    assert!(INIT.is_completed());

    // OnceLock: racing initialisers, exactly one closure runs and everyone gets the same value.
    let cell: OnceLock<usize> = OnceLock::new();
    let calls = AtomicUsize::new(0);
    thread::scope(|s| {
        let waiter = s.spawn(|| *cell.wait()); // 1.86: block until someone initialises it
        for i in 0..8 {
            let (cell, calls) = (&cell, &calls);
            s.spawn(move || {
                cell.get_or_init(|| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    i
                });
            });
        }
        assert!(waiter.join().unwrap() < 8);
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        cell.set(99),
        Err(99),
        "set on a full cell returns your value"
    );
    assert!(cell.into_inner().unwrap() < 8);
    assert!(std::ptr::eq(database_url(), database_url()));

    // LazyLock: initialiser fixed at the declaration; runs on first deref.
    assert!(
        LazyLock::get(&SQUARES).is_none(),
        "1.94: peek without forcing"
    );
    assert_eq!(SQUARES_BUILDS.load(Ordering::SeqCst), 0);
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| assert_eq!(SQUARES[9], 81));
        }
    });
    assert_eq!(LazyLock::force(&SQUARES).len(), 100);
    assert_eq!(SQUARES_BUILDS.load(Ordering::SeqCst), 1);

    // The three behave DIFFERENTLY when the initialiser panics:
    let c: OnceLock<u8> = OnceLock::new();
    assert!(panics(|| *c.get_or_init(|| panic!("{EXPECTED}"))));
    assert_eq!(c.get(), None);
    assert_eq!(
        *c.get_or_init(|| 7),
        7,
        "OnceLock: stays empty, retry works"
    );

    let o = Once::new();
    assert!(panics(|| o.call_once(|| panic!("{EXPECTED}"))));
    assert!(
        panics(|| o.call_once(|| ())),
        "Once: poisoned, call_once now panics"
    );
    o.call_once_force(|state| assert!(state.is_poisoned()));
    assert!(o.is_completed(), "Once: call_once_force recovers");

    let l: LazyLock<u8> = LazyLock::new(|| panic!("{EXPECTED}"));
    assert!(panics(|| *l));
    assert!(panics(|| *l), "LazyLock: poisoned for good");
    // Consequence: a fallible init (open a file, read env) belongs in OnceLock::get_or_init or an explicit startup step.
    println!("5. once / oncelock / lazylock ok");
}

// ------------------------------------------------------------ 6. mpsc channels
fn s06_channels() {
    // unbounded: send never blocks; cloned senders give multi-producer
    let (tx, rx) = mpsc::channel::<(u32, u32)>();
    thread::scope(|s| {
        for p in 0..4 {
            let tx = tx.clone();
            s.spawn(move || (0..100).for_each(|i| tx.send((p, i)).unwrap()));
        }
        drop(tx); // the original must go too, or the loop below never ends
        let mut next = [0u32; 4];
        let mut n = 0;
        for (p, i) in &rx {
            assert_eq!(
                i, next[p as usize],
                "FIFO per producer; interleaving across producers is arbitrary"
            );
            next[p as usize] += 1;
            n += 1;
        }
        assert_eq!(n, 400);
    });
    assert_eq!(rx.recv(), Err(RecvError));

    // the four receive flavours and the two ways to fail
    let (tx, rx) = mpsc::channel::<u8>();
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(
        rx.recv_timeout(Duration::from_millis(2)),
        Err(RecvTimeoutError::Timeout)
    );
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    tx.send(3).unwrap();
    assert_eq!(rx.try_iter().take(1).collect::<Vec<_>>(), [1]); // try_iter: drain what is there, never block
    drop(tx);
    // buffered messages survive disconnection; Disconnected is reported only once the buffer is empty
    assert_eq!(rx.try_recv(), Ok(2));
    assert_eq!(rx.recv_timeout(Duration::from_millis(2)), Ok(3));
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    assert_eq!(
        rx.recv_timeout(Duration::from_millis(2)),
        Err(RecvTimeoutError::Disconnected)
    );

    // send to a dropped receiver gives the value back
    let (tx, rx) = mpsc::channel();
    drop(rx);
    assert_eq!(tx.send(String::from("job")).unwrap_err().0, "job");

    // bounded: try_send is the load-shedding primitive
    let (tx, rx) = mpsc::sync_channel(2);
    assert_eq!(tx.try_send(1), Ok(()));
    assert_eq!(tx.try_send(2), Ok(()));
    assert_eq!(tx.try_send(3), Err(TrySendError::Full(3)));
    assert_eq!(rx.recv(), Ok(1));
    assert_eq!(tx.try_send(3), Ok(()));
    drop(rx);
    assert_eq!(tx.try_send(4), Err(TrySendError::Disconnected(4)));

    // capacity 0: rendezvous. No buffer at all: send returns only once a receiver has taken the value.
    let (tx, rx) = mpsc::sync_channel::<u8>(0);
    assert_eq!(
        tx.try_send(1),
        Err(TrySendError::Full(1)),
        "nobody is waiting in recv"
    );
    let delivered = AtomicBool::new(false);
    thread::scope(|s| {
        s.spawn(|| {
            tx.send(5).unwrap();
            delivered.store(true, Ordering::SeqCst);
        });
        for _ in 0..100 {
            thread::yield_now();
        }
        assert!(
            !delivered.load(Ordering::SeqCst),
            "send cannot have completed before any recv"
        );
        assert_eq!(rx.recv(), Ok(5));
    });
    assert!(delivered.load(Ordering::SeqCst));

    // pipeline with backpressure; shutdown propagates stage by stage as senders drop
    let (raw_tx, raw_rx) = mpsc::sync_channel::<String>(2);
    let (num_tx, num_rx) = mpsc::sync_channel::<u64>(2);
    let producer =
        thread::spawn(move || (1..=200).for_each(|i| raw_tx.send(i.to_string()).unwrap()));
    let parser = thread::spawn(move || {
        raw_rx
            .iter()
            .for_each(|s| num_tx.send(s.parse().unwrap()).unwrap())
    });
    let total: u64 = num_rx.iter().sum();
    producer.join().unwrap();
    parser.join().unwrap();
    assert_eq!(total, 20_100);
    println!("6. channels ok");
}

// ------------------------------------------------------------ 7. Barrier: lock-step phases
fn s07_barrier() {
    const N: usize = 6;
    const PHASES: usize = 20;
    let barrier = Barrier::new(N);
    let arrived: Vec<AtomicUsize> = (0..PHASES).map(|_| AtomicUsize::new(0)).collect();
    let leaders: Vec<AtomicUsize> = (0..PHASES).map(|_| AtomicUsize::new(0)).collect();
    thread::scope(|s| {
        for _ in 0..N {
            s.spawn(|| {
                for p in 0..PHASES {
                    arrived[p].fetch_add(1, Ordering::Relaxed); // Relaxed is enough: the barrier synchronises
                    if barrier.wait().is_leader() {
                        leaders[p].fetch_add(1, Ordering::Relaxed);
                    }
                    assert_eq!(
                        arrived[p].load(Ordering::Relaxed),
                        N,
                        "nobody passes until all {N} arrived"
                    );
                }
            });
        }
    });
    assert!(
        leaders.iter().all(|l| l.load(Ordering::Relaxed) == 1),
        "exactly one leader per generation"
    );
    println!("7. barrier ok");
}

// ------------------------------------------------------------ 8. Arc and Weak
fn s08_arc() {
    let a = Arc::new(vec![1, 2, 3]);
    let b = Arc::clone(&a); // bumps a count; does not copy the Vec
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(Arc::strong_count(&a), 2);

    // make_mut: copy-on-write. It clones only when shared.
    let mut c = Arc::clone(&a);
    Arc::make_mut(&mut c).push(4);
    assert!(!Arc::ptr_eq(&a, &c));
    assert_eq!((a.len(), c.len()), (3, 4));
    let before = Arc::as_ptr(&c);
    Arc::make_mut(&mut c).push(5);
    assert_eq!(
        before,
        Arc::as_ptr(&c),
        "unique owner: mutated in place, same allocation"
    );

    // Weak does not keep the value alive; use it for caches and to break cycles
    let w = Arc::downgrade(&c);
    assert_eq!(w.upgrade().map(|v| v.len()), Some(5));
    drop(c);
    assert!(w.upgrade().is_none());

    // take the value back out: try_unwrap returns the Arc on failure; into_inner drops your handle on failure
    let a = Arc::try_unwrap(a).unwrap_err();
    drop(b);
    assert_eq!(Arc::try_unwrap(a).unwrap(), [1, 2, 3]);

    // into_inner: when every clone calls it, exactly one thread gets Some (try_unwrap(..).ok() can lose the value under a race)
    let shared = Arc::new(String::from("result"));
    let handles: Vec<_> = (0..8)
        .map(|_| Arc::clone(&shared))
        .map(|h| thread::spawn(move || Arc::into_inner(h)))
        .collect();
    let main_got = Arc::into_inner(shared);
    let winners = handles
        .into_iter()
        .filter_map(|h| h.join().unwrap())
        .count()
        + usize::from(main_got.is_some());
    assert_eq!(winners, 1);
    println!("8. arc ok");
}

// ------------------------------------------------------------ 9. Threads: Builder, join errors, park, thread locals
thread_local! {
    static REQUEST_ID: Cell<u64> = const { Cell::new(0) }; // `const` block: no lazy-init check on access
    static SCRATCH: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(64));
}

fn s09_threads() {
    // named threads: the name appears in panic messages and debuggers
    let h = thread::Builder::new()
        .name("ingest-0".into())
        .stack_size(256 * 1024)
        .spawn(|| thread::current().name().map(str::to_owned))
        .expect("Builder::spawn returns io::Result instead of panicking like thread::spawn");
    assert_eq!(h.join().unwrap().as_deref(), Some("ingest-0"));
    let local = 5;
    thread::scope(|s| {
        let h = thread::Builder::new()
            .name("scoped".into())
            .spawn_scoped(s, || local * 2)
            .unwrap();
        assert_eq!(h.join().unwrap(), 10);
    });

    // join returns the panic payload. A literal message is &'static str, a formatted one is String.
    let e = thread::spawn(|| std::panic::panic_any("expected-demo-panic literal"))
        .join()
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<&str>(),
        Some(&"expected-demo-panic literal")
    );
    let code = std::hint::black_box(42);
    let e = thread::spawn(move || panic!("{EXPECTED} code {code}"))
        .join()
        .unwrap_err();
    assert_eq!(
        e.downcast_ref::<String>().map(String::as_str),
        Some("expected-demo-panic code 42")
    );

    // thread::scope re-panics if a thread you did not join panicked; join it yourself to handle the error.
    assert!(panics(|| thread::scope(|s| {
        s.spawn(|| panic!("{EXPECTED}"));
    })));
    thread::scope(|s| assert!(s.spawn(|| panic!("{EXPECTED}")).join().is_err()));

    // park/unpark: always loop on a condition, because park may return spuriously. unpark before park is remembered (one token).
    thread::current().unpark();
    thread::park(); // consumes the token and returns at once
    let ready = AtomicBool::new(false);
    thread::scope(|s| {
        let sleeper = s.spawn(|| {
            while !ready.load(Ordering::Acquire) {
                thread::park();
            }
            true
        });
        ready.store(true, Ordering::Release); // set the condition FIRST
        sleeper.thread().unpark(); // then wake
        assert!(sleeper.join().unwrap());
    });

    // thread_local: each thread has its own copy, initialised on that thread
    REQUEST_ID.set(7); // set / get / take / replace on LocalKey<Cell<T>> (1.73)
    assert_eq!(REQUEST_ID.get(), 7);
    thread::scope(|s| {
        s.spawn(|| {
            assert_eq!(REQUEST_ID.get(), 0, "a new thread sees the initial value");
            REQUEST_ID.set(100);
        });
    });
    assert_eq!(REQUEST_ID.replace(8), 7);
    assert_eq!(REQUEST_ID.take(), 8);
    assert_eq!(REQUEST_ID.get(), 0);
    SCRATCH.with_borrow_mut(|b| b.extend_from_slice(b"abc")); // LocalKey<RefCell<T>>
    assert_eq!(SCRATCH.with_borrow(|b| b.len()), 3);
    assert_eq!(SCRATCH.with(|c| c.borrow().len()), 3); // the general form
    assert_eq!(SCRATCH.try_with(|c| c.borrow().len()), Ok(3)); // Err instead of panic during/after TLS destruction

    // available_parallelism is a hint (it respects affinity masks and some container quotas); it can fail, so give a fallback
    let n = thread::available_parallelism().map_or(1, |n| n.get());
    assert!(n >= 1);
    println!("9. threads and thread locals ok");
}

fn main() {
    install_quiet_hook();
    s01_mutex();
    s02_poison();
    s03_rwlock();
    s04_condvar();
    s05_once();
    s06_channels();
    s07_barrier();
    s08_arc();
    s09_threads();
    println!("all sync sections passed");
}
