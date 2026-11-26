//! RECIPE 193 (concurrency): build locks, a condvar, a bounded MPMC channel, a semaphore and a barrier from simpler pieces.
//! LEVEL: advanced
//! Use in: understanding what std/parking_lot/crossbeam do under the hood; choosing a lock for a latency-critical path.   Run: cargo run --release --example mutex_condvar_channel
#![allow(dead_code)]
//!
//! WHAT: (1) `TicketLock`, FIFO-fair spinlock: take a ticket with fetch_add, spin until `now_serving == ticket`.
//! (2) `TtasLock`, test-and-test-and-set spinlock: spin on a plain LOAD (cache-line stays shared, no coherence storm) and only
//!     then try the CAS; exponential backoff via `spin_loop` hints, escalating to `yield_now` when oversubscribed.
//! (3) `ParkMutex`, a futex-STYLE mutex: one `AtomicU32` state (0 unlocked, 1 locked, 2 locked+contended) so the uncontended
//!     path is one CAS and unlock is one swap with NO syscall unless state == 2. Waiters sleep with `thread::park` and sit in
//!     a queue. Real implementations (std on Linux, parking_lot, Linux pthread) wait on the state word itself with the futex
//!     syscall (the kernel keeps the queue and checks "value still == 2" atomically). We emulate that check by enqueueing
//!     first and then re-trying the swap, which closes the lost-wakeup window: either the unlocker sees our queue entry
//!     (we enqueued before our swap, it saw our `2`), or our swap sees `0` and we take the lock.
//! (4) `MyCondvar` on top of it: each waiter registers a (thread, flag) pair BEFORE releasing the mutex, so a notify issued
//!     after the release cannot be missed; the flag makes spurious `park` wakeups harmless.
//! (5) A bounded MPMC channel = `Mutex<VecDeque>` + two Condvars (not_full / not_empty) with close semantics.
//! (6) A counting `Semaphore` and a reusable generation-counting `Barrier`.
//! WHY: spin locks are for critical sections of a few instructions with threads <= cores; with more threads than cores a
//! preempted holder makes everyone spin, so production locks spin briefly and then sleep. Ticket locks give fairness at the
//! price of convoying (every waiter must be scheduled in turn). Always re-check the predicate in a loop around a condvar wait.
//! Each lock is tested for exact totals on a NON-atomic counter, so a broken lock shows up as a wrong sum.
//! COMMON USES: spinlocks in kernels (Linux uses ticket/queued spinlocks), parking_lot-style small mutexes, work queues and
//! pipelines (bounded channels give back-pressure), semaphores for connection/IO limits, barriers in phase-based simulations.
//! Read: https://marabos.nl/atomics/building-locks.html , https://doc.rust-lang.org/std/thread/fn.park.html , https://doc.rust-lang.org/std/sync/struct.Condvar.html
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

// ---------------- (1) ticket lock ----------------
pub struct TicketLock<T> {
    next: AtomicUsize,
    serving: AtomicUsize,
    data: UnsafeCell<T>,
}
// SAFETY: access to `data` is serialised by the lock; T moves between threads, hence T: Send.
unsafe impl<T: Send> Sync for TicketLock<T> {}
pub struct TicketGuard<'a, T> {
    lock: &'a TicketLock<T>,
    ticket: usize,
}
impl<T> TicketLock<T> {
    pub fn new(v: T) -> Self {
        Self {
            next: AtomicUsize::new(0),
            serving: AtomicUsize::new(0),
            data: UnsafeCell::new(v),
        }
    }
    pub fn lock(&self) -> TicketGuard<'_, T> {
        let ticket = self.next.fetch_add(1, Ordering::Relaxed);
        let mut spins = 0u32;
        while self.serving.load(Ordering::Acquire) != ticket {
            spins += 1;
            if spins < 200 {
                std::hint::spin_loop()
            } else {
                thread::yield_now()
            } // yield: holder may be descheduled
        }
        TicketGuard { lock: self, ticket }
    }
}
impl<T> Deref for TicketGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: we hold the lock (our ticket is being served), so access is exclusive.
        unsafe { &*self.lock.data.get() }
    }
}
impl<T> DerefMut for TicketGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: as above, and `&mut self` makes the borrow unique.
        unsafe { &mut *self.lock.data.get() }
    }
}
impl<T> Drop for TicketGuard<'_, T> {
    fn drop(&mut self) {
        // Release publishes our writes to the next ticket holder's Acquire load.
        self.lock.serving.store(self.ticket + 1, Ordering::Release);
    }
}

// ---------------- (2) TTAS spinlock with exponential backoff ----------------
pub struct TtasLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}
// SAFETY: access to `data` is serialised by the lock; T: Send is enough.
unsafe impl<T: Send> Sync for TtasLock<T> {}
pub struct TtasGuard<'a, T>(&'a TtasLock<T>);
impl<T> TtasLock<T> {
    pub fn new(v: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(v),
        }
    }
    pub fn lock(&self) -> TtasGuard<'_, T> {
        let mut exp = 0u32;
        loop {
            while self.locked.load(Ordering::Relaxed) {
                // test: read-only spin; the line stays Shared in every core's cache
                if exp < 6 {
                    for _ in 0..(1u32 << exp) {
                        std::hint::spin_loop()
                    }
                    exp += 1;
                } else {
                    thread::yield_now();
                }
            }
            // test-and-set: only now try to write
            if self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return TtasGuard(self);
            }
            exp = (exp + 1).min(6); // lost the race: back off more
        }
    }
}
impl<T> Deref for TtasGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the guard proves we hold the lock.
        unsafe { &*self.0.data.get() }
    }
}
impl<T> DerefMut for TtasGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: lock held, `&mut self` is unique.
        unsafe { &mut *self.0.data.get() }
    }
}
impl<T> Drop for TtasGuard<'_, T> {
    fn drop(&mut self) {
        self.0.locked.store(false, Ordering::Release);
    }
}

// ---------------- (3) futex-style parking mutex ----------------
const UNLOCKED: u32 = 0;
const LOCKED: u32 = 1;
const CONTENDED: u32 = 2;

pub struct ParkMutex<T> {
    state: AtomicU32,
    waiters: Mutex<VecDeque<Thread>>,
    data: UnsafeCell<T>,
}
// SAFETY: access to `data` is serialised by `state`; T: Send is enough.
unsafe impl<T: Send> Sync for ParkMutex<T> {}
unsafe impl<T: Send> Send for ParkMutex<T> {}
pub struct ParkGuard<'a, T> {
    m: &'a ParkMutex<T>,
}
impl<T> ParkMutex<T> {
    pub fn new(v: T) -> Self {
        Self {
            state: AtomicU32::new(UNLOCKED),
            waiters: Mutex::new(VecDeque::new()),
            data: UnsafeCell::new(v),
        }
    }
    pub fn lock(&self) -> ParkGuard<'_, T> {
        if self
            .state
            .compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.lock_slow();
        }
        ParkGuard { m: self }
    }
    fn lock_slow(&self) {
        for _ in 0..100 {
            // brief spin first: critical sections are usually short
            if self
                .state
                .compare_exchange_weak(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
            std::hint::spin_loop();
        }
        let me = thread::current();
        loop {
            // Mark contended; if it was unlocked we now own it (state stays 2: an extra unpark later is harmless).
            if self.state.swap(CONTENDED, Ordering::Acquire) == UNLOCKED {
                return;
            }
            // Enqueue BEFORE the re-check, so an unlocker that saw our CONTENDED also sees our queue entry.
            self.waiters.lock().unwrap().push_back(me.clone());
            if self.state.swap(CONTENDED, Ordering::AcqRel) == UNLOCKED {
                self.waiters.lock().unwrap().retain(|t| t.id() != me.id()); // no stale entry may steal a future wakeup
                return;
            }
            thread::park(); // spurious wakeups are fine: we loop and retry
            self.waiters.lock().unwrap().retain(|t| t.id() != me.id());
        }
    }
}
impl<T> Deref for ParkGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the guard proves ownership of the lock.
        unsafe { &*self.m.data.get() }
    }
}
impl<T> DerefMut for ParkGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: lock held, `&mut self` is unique.
        unsafe { &mut *self.m.data.get() }
    }
}
impl<T> Drop for ParkGuard<'_, T> {
    fn drop(&mut self) {
        if self.m.state.swap(UNLOCKED, Ordering::AcqRel) == CONTENDED {
            // wake ONE waiter (it will race to re-lock; "barging" is allowed, as in most real mutexes)
            let t = self.m.waiters.lock().unwrap().pop_front();
            if let Some(t) = t {
                t.unpark();
            }
        }
    }
}

// ---------------- (4) condvar on top of ParkMutex ----------------
#[derive(Default)]
pub struct MyCondvar {
    waiters: Mutex<VecDeque<(Thread, Arc<AtomicBool>)>>,
}
impl MyCondvar {
    pub fn new() -> Self {
        Self::default()
    }
    /// Atomically (w.r.t. notify) release the mutex, sleep until notified, re-lock. May wake spuriously? No: the flag
    /// makes it exact, but callers must still loop on their predicate (another thread may consume the condition first).
    pub fn wait<'a, T>(&self, guard: ParkGuard<'a, T>) -> ParkGuard<'a, T> {
        let m = guard.m;
        let flag = Arc::new(AtomicBool::new(false));
        self.waiters
            .lock()
            .unwrap()
            .push_back((thread::current(), flag.clone())); // register BEFORE unlocking
        drop(guard);
        while !flag.load(Ordering::Acquire) {
            thread::park();
        }
        m.lock()
    }
    pub fn notify_one(&self) {
        let w = self.waiters.lock().unwrap().pop_front();
        if let Some((t, f)) = w {
            f.store(true, Ordering::Release);
            t.unpark();
        }
    }
    pub fn notify_all(&self) {
        let all: Vec<_> = self.waiters.lock().unwrap().drain(..).collect();
        for (t, f) in all {
            f.store(true, Ordering::Release);
            t.unpark();
        }
    }
}

/// Bounded MPMC queue built on ParkMutex + MyCondvar (same logic as the std-based channel below).
struct ParkQueue {
    st: ParkMutex<(VecDeque<u64>, bool)>, // (items, closed)
    not_empty: MyCondvar,
    not_full: MyCondvar,
    cap: usize,
}
impl ParkQueue {
    fn send(&self, v: u64) {
        let mut g = self.st.lock();
        while g.0.len() >= self.cap {
            g = self.not_full.wait(g);
        }
        g.0.push_back(v);
        drop(g);
        self.not_empty.notify_one();
    }
    fn recv(&self) -> Option<u64> {
        let mut g = self.st.lock();
        loop {
            if let Some(v) = g.0.pop_front() {
                drop(g);
                self.not_full.notify_one();
                return Some(v);
            }
            if g.1 {
                return None;
            }
            g = self.not_empty.wait(g);
        }
    }
    fn close(&self) {
        self.st.lock().1 = true;
        self.not_empty.notify_all();
    }
}

// ---------------- (5) bounded MPMC channel ----------------
struct St<T> {
    q: VecDeque<T>,
    closed: bool,
}
pub struct Channel<T> {
    st: Mutex<St<T>>,
    not_full: Condvar,
    not_empty: Condvar,
    cap: usize,
}
impl<T> Channel<T> {
    pub fn new(cap: usize) -> Self {
        assert!(cap > 0);
        Self {
            st: Mutex::new(St {
                q: VecDeque::new(),
                closed: false,
            }),
            not_full: Condvar::new(),
            not_empty: Condvar::new(),
            cap,
        }
    }
    /// Blocks while full. Err(v) gives the value back if the channel is (or becomes) closed.
    pub fn send(&self, v: T) -> Result<(), T> {
        let mut s = self.st.lock().unwrap();
        loop {
            if s.closed {
                return Err(v);
            }
            if s.q.len() < self.cap {
                s.q.push_back(v);
                drop(s);
                self.not_empty.notify_one();
                return Ok(());
            }
            s = self.not_full.wait(s).unwrap();
        }
    }
    /// Blocks while empty. None only after close AND drain.
    pub fn recv(&self) -> Option<T> {
        let mut s = self.st.lock().unwrap();
        loop {
            if let Some(v) = s.q.pop_front() {
                drop(s);
                self.not_full.notify_one();
                return Some(v);
            }
            if s.closed {
                return None;
            }
            s = self.not_empty.wait(s).unwrap();
        }
    }
    pub fn close(&self) {
        self.st.lock().unwrap().closed = true;
        self.not_full.notify_all();
        self.not_empty.notify_all();
    }
    pub fn len(&self) -> usize {
        self.st.lock().unwrap().q.len()
    }
}

// ---------------- (6) semaphore and barrier ----------------
pub struct Semaphore {
    permits: Mutex<usize>,
    cv: Condvar,
}
impl Semaphore {
    pub fn new(n: usize) -> Self {
        Self {
            permits: Mutex::new(n),
            cv: Condvar::new(),
        }
    }
    pub fn acquire(&self) {
        let mut p = self.permits.lock().unwrap();
        while *p == 0 {
            p = self.cv.wait(p).unwrap();
        }
        *p -= 1;
    }
    pub fn release(&self) {
        *self.permits.lock().unwrap() += 1;
        self.cv.notify_one();
    }
}
/// Reusable barrier: the generation counter stops a fast thread from racing into the NEXT round and being counted twice.
pub struct Barrier {
    n: usize,
    st: Mutex<(usize, usize)>, // (arrived, generation)
    cv: Condvar,
}
impl Barrier {
    pub fn new(n: usize) -> Self {
        Self {
            n,
            st: Mutex::new((0, 0)),
            cv: Condvar::new(),
        }
    }
    /// Returns true for exactly one thread per round (the last to arrive).
    pub fn wait(&self) -> bool {
        let mut s = self.st.lock().unwrap();
        s.0 += 1;
        if s.0 == self.n {
            s.0 = 0;
            s.1 += 1;
            self.cv.notify_all();
            return true;
        }
        let gen_ = s.1;
        while s.1 == gen_ {
            s = self.cv.wait(s).unwrap();
        }
        false
    }
}

// ---------------- harness ----------------
trait Lk: Sync + Send + 'static {
    const NAME: &'static str;
    fn make() -> Self;
    fn incr(&self);
    fn get(&self) -> u64;
}
impl Lk for TicketLock<u64> {
    const NAME: &'static str = "ticket";
    fn make() -> Self {
        TicketLock::new(0)
    }
    fn incr(&self) {
        *self.lock() += 1
    }
    fn get(&self) -> u64 {
        *self.lock()
    }
}
impl Lk for TtasLock<u64> {
    const NAME: &'static str = "ttas+backoff";
    fn make() -> Self {
        TtasLock::new(0)
    }
    fn incr(&self) {
        *self.lock() += 1
    }
    fn get(&self) -> u64 {
        *self.lock()
    }
}
impl Lk for ParkMutex<u64> {
    const NAME: &'static str = "park-mutex";
    fn make() -> Self {
        ParkMutex::new(0)
    }
    fn incr(&self) {
        *self.lock() += 1
    }
    fn get(&self) -> u64 {
        *self.lock()
    }
}
impl Lk for Mutex<u64> {
    const NAME: &'static str = "std Mutex";
    fn make() -> Self {
        Mutex::new(0)
    }
    fn incr(&self) {
        *self.lock().unwrap() += 1
    }
    fn get(&self) -> u64 {
        *self.lock().unwrap()
    }
}

/// Run `threads` x (total/threads) increments; assert the exact total; return elapsed.
fn hammer<L: Lk>(threads: usize, total: u64) -> Duration {
    let l = Arc::new(L::make());
    let per = total / threads as u64;
    let t0 = Instant::now();
    let hs: Vec<_> = (0..threads)
        .map(|_| {
            let l = l.clone();
            thread::spawn(move || (0..per).for_each(|_| l.incr()))
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    let el = t0.elapsed();
    assert_eq!(l.get(), per * threads as u64, "{}: lost updates", L::NAME);
    el
}

/// Fairness: how many acquisitions did each of `threads` threads get in a fixed 100 ms window?
fn fairness<L: Lk>(threads: usize) -> Vec<u64> {
    let l = Arc::new(L::make());
    let stop = Arc::new(AtomicBool::new(false));
    let hs: Vec<_> = (0..threads)
        .map(|_| {
            let (l, stop) = (l.clone(), stop.clone());
            thread::spawn(move || {
                let mut n = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    l.incr();
                    n += 1;
                }
                n
            })
        })
        .collect();
    thread::sleep(Duration::from_millis(100));
    stop.store(true, Ordering::Relaxed);
    let v: Vec<u64> = hs.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(l.get(), v.iter().sum::<u64>());
    v
}

fn main() {
    // exact totals for every lock, non-atomic counter inside
    hammer::<TicketLock<u64>>(4, 200_000);
    hammer::<TtasLock<u64>>(4, 200_000);
    hammer::<ParkMutex<u64>>(8, 400_000);
    hammer::<Mutex<u64>>(4, 200_000);
    println!("locks: exact totals ok");

    // channel: 4 producers x 4 consumers x 200k items, capacity 16, no lost wakeups
    const P: u64 = 4;
    const PER: u64 = 50_000;
    let ch = Arc::new(Channel::<u64>::new(16));
    let max_len = Arc::new(AtomicUsize::new(0));
    let prods: Vec<_> = (0..P)
        .map(|p| {
            let (ch, ml) = (ch.clone(), max_len.clone());
            thread::spawn(move || {
                for i in 0..PER {
                    ch.send(p * PER + i + 1).unwrap();
                    ml.fetch_max(ch.len(), Ordering::Relaxed);
                }
            })
        })
        .collect();
    let cons: Vec<_> = (0..4)
        .map(|_| {
            let ch = ch.clone();
            thread::spawn(move || {
                let (mut n, mut sum) = (0u64, 0u64);
                while let Some(v) = ch.recv() {
                    n += 1;
                    sum += v;
                }
                (n, sum)
            })
        })
        .collect();
    prods.into_iter().for_each(|h| h.join().unwrap());
    ch.close();
    let (n, sum) = cons
        .into_iter()
        .map(|h| h.join().unwrap())
        .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let total = P * PER;
    assert_eq!(n, total);
    assert_eq!(
        sum,
        total * (total + 1) / 2,
        "every item delivered exactly once"
    );
    assert!(max_len.load(Ordering::Relaxed) <= 16, "bound respected");
    println!("channel 4x4 moved {n} items, bound respected");

    // close/drain semantics
    let ch = Arc::new(Channel::<u32>::new(1));
    ch.send(1).unwrap();
    let blocked = {
        let ch = ch.clone();
        thread::spawn(move || ch.send(2)) // full: blocks until close wakes it
    };
    thread::sleep(Duration::from_millis(30));
    ch.close();
    assert_eq!(
        blocked.join().unwrap(),
        Err(2),
        "blocked send returns its value on close"
    );
    assert_eq!(ch.send(3), Err(3));
    assert_eq!(ch.recv(), Some(1), "drain after close");
    assert_eq!(ch.recv(), None);
    assert_eq!(ch.recv(), None);
    let ch2 = Arc::new(Channel::<u32>::new(2));
    let waiter = {
        let ch2 = ch2.clone();
        thread::spawn(move || ch2.recv()) // empty: blocks until close
    };
    thread::sleep(Duration::from_millis(30));
    ch2.close();
    assert_eq!(waiter.join().unwrap(), None);
    println!("close/drain semantics ok");

    // custom ParkMutex + MyCondvar queue: 4x4, 100k items, tiny capacity to force blocking both ways
    let q = Arc::new(ParkQueue {
        st: ParkMutex::new((VecDeque::new(), false)),
        not_empty: MyCondvar::new(),
        not_full: MyCondvar::new(),
        cap: 4,
    });
    let prods: Vec<_> = (0..4u64)
        .map(|p| {
            let q = q.clone();
            thread::spawn(move || (0..25_000u64).for_each(|i| q.send(p * 25_000 + i + 1)))
        })
        .collect();
    let cons: Vec<_> = (0..4)
        .map(|_| {
            let q = q.clone();
            thread::spawn(move || {
                let mut sum = 0u64;
                let mut n = 0u64;
                while let Some(v) = q.recv() {
                    sum += v;
                    n += 1;
                }
                (n, sum)
            })
        })
        .collect();
    prods.into_iter().for_each(|h| h.join().unwrap());
    q.close();
    let (n, sum) = cons
        .into_iter()
        .map(|h| h.join().unwrap())
        .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    assert_eq!((n, sum), (100_000, 100_000u64 * 100_001 / 2));
    println!("ParkMutex+MyCondvar queue: no lost wakeups ({n} items)");

    // semaphore: never more than 3 holders at once
    let sem = Arc::new(Semaphore::new(3));
    let (cur, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let (sem, cur, peak) = (sem.clone(), cur.clone(), peak.clone());
            thread::spawn(move || {
                for _ in 0..2_000 {
                    sem.acquire();
                    let c = cur.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(c, Ordering::SeqCst);
                    std::hint::spin_loop();
                    cur.fetch_sub(1, Ordering::SeqCst);
                    sem.release();
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert!(peak.load(Ordering::SeqCst) <= 3 && cur.load(Ordering::SeqCst) == 0);
    println!(
        "semaphore: peak holders {} (limit 3)",
        peak.load(Ordering::SeqCst)
    );

    // reusable barrier: after each round every thread must see the full count of that round
    const T: usize = 6;
    const R: usize = 500;
    let bar = Arc::new(Barrier::new(T));
    let counter = Arc::new(AtomicUsize::new(0));
    let leaders = Arc::new(AtomicUsize::new(0));
    let hs: Vec<_> = (0..T)
        .map(|_| {
            let (bar, counter, leaders) = (bar.clone(), counter.clone(), leaders.clone());
            thread::spawn(move || {
                for r in 0..R {
                    counter.fetch_add(1, Ordering::SeqCst);
                    if bar.wait() {
                        leaders.fetch_add(1, Ordering::SeqCst);
                    }
                    assert_eq!(counter.load(Ordering::SeqCst), T * (r + 1));
                    bar.wait(); // second barrier so nobody increments for round r+1 before everyone has checked round r
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(leaders.load(Ordering::SeqCst), R, "one leader per round");
    println!("barrier: {R} rounds x {T} threads ok");

    // fairness observation (printed only)
    for (name, v) in [
        ("ticket", fairness::<TicketLock<u64>>(4)),
        ("ttas", fairness::<TtasLock<u64>>(4)),
    ] {
        let (mn, mx) = (*v.iter().min().unwrap(), *v.iter().max().unwrap());
        println!(
            "fairness {name:<7} per-thread acquisitions {v:?}  max/min = {:.2}",
            mx as f64 / mn.max(1) as f64
        );
    }

    // throughput (printed only): Mops/s for 400k total increments
    println!("throughput (M lock+incr/s, higher is better):");
    println!("{:<14}{:>9}{:>9}{:>9}{:>9}", "lock", "1t", "2t", "4t", "8t");
    fn row<L: Lk>() {
        let mut line = format!("{:<14}", L::NAME);
        for t in [1usize, 2, 4, 8] {
            let el = hammer::<L>(t, 400_000);
            line += &format!("{:>9.1}", 0.4 / el.as_secs_f64());
        }
        println!("{line}");
    }
    row::<Mutex<u64>>();
    row::<ParkMutex<u64>>();
    row::<TtasLock<u64>>();
    row::<TicketLock<u64>>();
    println!("ALL OK");
}
