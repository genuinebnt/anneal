//! RECIPE 650 (interview): concurrency interview questions: blocking queue, rate limiter, LRU, RW lock, philosophers, barriers, pools, orderings, Send/Sync.
//! LEVEL: advanced
//! Use in: interview preparation for systems/backend roles, reviewing hand-rolled synchronisation, teaching Mutex+Condvar patterns.   Run: cargo run --release --example concurrency_interview_questions
//! COMMON USES: every primitive here is something production code re-implements or wraps: bounded channels, token buckets in API gateways, sharded caches, thread pools, singleflight in caches (Go's `singleflight`, Rust's `moka`), lock-order checkers (Linux lockdep).
//!
//! SPOILER: Q11 (lock-order graph) contains graph cycle detection (skip Q11 if you want to work that out yourself first).
#![allow(dead_code)]
//! Read: https://marabos.nl/atomics/ (Rust Atomics and Locks) and https://doc.rust-lang.org/nomicon/send-and-sync.html and https://doc.rust-lang.org/std/sync/struct.Condvar.html
//! Practice: the LeetCode "Concurrency" problem list (FooBar, ZeroEvenOdd, print in order); named only, the site refuses automated link checks.
//!
//! TRY NEXT:
//!   - [S] In Q1 replace the `while` around `Condvar::wait` with an `if`, add a second consumer, and predict which assertion fires; then restore it and explain why Q21's lost-wakeup demo is the same bug.
//!   - [S] In Q4 make readers preferred instead of writers (drop the `writers_waiting` check). Predict the order log of the starvation test, run it, and name the system symptom (a hot read path starves config reloads).
//!   - [M] Extend Q12's pool with a bounded queue (`sync_channel`) and a `try_execute` that returns `Err(Rejected)` when full; assert that the rejected count plus executed count equals submitted under 8 submitters.
//!   - [M] Port Q22's singleflight to a `tokio` version with `Shared<BoxFuture>`; what changes about cancellation if the leader task is dropped? Compare with `structured_concurrency_cancellation`.
//!   - [A] PROJECT: combine Q2 (token bucket), Q12 (pool) and Q1 (bounded queue) into an admission-controlled request server; add a deadlock check from Q11 on its lock order and test it with the model checker `model_checker_for_concurrent_code`.
//!
//! QUESTION BANK
//! Q1. Implement a thread-safe bounded blocking queue (Mutex + two Condvars). [Medium] [asked: very common]
//! Q2. Implement a thread-safe token-bucket rate limiter with an injectable clock. [Medium] [asked: very common]
//! Q3. Implement an LRU cache, then make it concurrent (one lock vs sharded). [Medium] [asked: very common]
//! Q4. Build a reader-writer lock and explain writer starvation and fairness. [Hard] [asked: common]
//! Q5. Dining philosophers: avoid deadlock. [Medium] [asked: very common]
//! Q6. Two threads print "foo" and "bar" alternately n times (FooBar). [Medium] [asked: very common]
//! Q7. Three threads print 0, odd, even to produce 0102030405... (ZeroEvenOdd). [Medium] [asked: common]
//! Q8. Implement a reusable barrier from Mutex + Condvar. [Medium] [asked: common]
//! Q9. Implement a counting semaphore from Mutex + Condvar. [Medium] [asked: common]
//! Q10. Lazy one-time initialisation: `OnceLock`, and why double-checked locking needs Acquire/Release. [Medium] [asked: very common]
//! Q11. Detect potential deadlocks from lock acquisition order (lockdep). [Hard] [asked: common]
//! Q12. Build a thread pool with graceful shutdown (and panicking jobs). [Medium] [asked: very common]
//! Q13. Parallel merge sort with scoped threads. [Medium] [asked: common]
//! Q14. Concurrent word count: shared map vs per-thread merge vs sharded. [Medium] [asked: common]
//! Q15. Which atomic ordering do you need for a counter? For publishing data? [Hard] [asked: very common]
//! Q16. What is the ABA problem? Reproduce it deterministically and fix it with a tag. [Hard] [asked: common]
//! Q17. `Arc<Mutex<T>>` vs channels vs atomics: same job three ways. [Medium] [asked: very common]
//! Q18. Async tasks vs OS threads: what actually differs? [Medium] [asked: very common]
//! Q19. How do you cancel a running thread? [Medium] [asked: very common]
//! Q20. Send and Sync: reason about which types are which, and why the compiler rejects the rest. [Hard] [asked: very common]
//! Q21. What are spurious wakeups and lost wakeups? [Medium] [asked: very common]
//! Q22. Cache stampede: implement singleflight (collapse concurrent identical loads). [Hard] [asked: common]

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::*};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
/// True if `f` panics (its message is not printed).
fn panics(f: impl FnOnce()) -> bool {
    quiet(|| std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err())
}
fn quiet<R>(f: impl FnOnce() -> R) -> R {
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = f();
    std::panic::set_hook(old);
    r
}

// ---------------------------------------------------------------------------
// Q1. bounded blocking queue
// ---------------------------------------------------------------------------
/// ANSWER: one Mutex guards `VecDeque` + `closed`; `not_full` makes producers wait, `not_empty` makes consumers wait.
/// ALWAYS wait in a `while` loop re-checking the predicate (spurious wakeups, and another thread may take the slot between the
/// notify and your wake-up). Notify AFTER changing state. `close()` lets consumers drain and then stop (no sentinel values needed).
/// Two condvars (not one) avoid waking the wrong kind of waiter; with one condvar you need `notify_all`.
///
/// REALLY TESTING: do you know the predicate loop, and can you shut it down cleanly?
/// WRONG: `if queue.is_empty() { wait }`, sleeping/polling, or holding the lock while doing the consumer's work.
/// FOLLOW-UP: how would you add `push_timeout`? `Condvar::wait_timeout_while`. How does this relate to `sync_channel(cap)`? It IS one.
struct BoundedQueue<T> {
    state: Mutex<(VecDeque<T>, bool)>, // (items, closed)
    not_empty: Condvar,
    not_full: Condvar,
    cap: usize,
    max_len_seen: AtomicUsize,
}
impl<T> BoundedQueue<T> {
    fn new(cap: usize) -> Self {
        assert!(cap > 0, "with no room every push would wait for ever");
        BoundedQueue {
            state: Mutex::new((VecDeque::new(), false)),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            cap,
            max_len_seen: AtomicUsize::new(0),
        }
    }
    fn push(&self, v: T) -> Result<(), T> {
        let mut g = self.state.lock().unwrap();
        while g.0.len() == self.cap && !g.1 {
            g = self.not_full.wait(g).unwrap();
        }
        if g.1 {
            return Err(v);
        }
        g.0.push_back(v);
        self.max_len_seen.fetch_max(g.0.len(), Relaxed);
        self.not_empty.notify_one();
        Ok(())
    }
    fn pop(&self) -> Option<T> {
        let mut g = self.state.lock().unwrap();
        while g.0.is_empty() && !g.1 {
            g = self.not_empty.wait(g).unwrap();
        }
        let v = g.0.pop_front();
        if v.is_some() {
            self.not_full.notify_one();
        }
        v
    }
    fn close(&self) {
        self.state.lock().unwrap().1 = true;
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }
}
fn q1_bounded_queue() {
    let q = Arc::new(BoundedQueue::new(8));
    let sum = Arc::new(AtomicU64::new(0));
    let producers: Vec<_> = (0..4u64)
        .map(|p| {
            let q = q.clone();
            thread::spawn(move || (0..5000u64).for_each(|i| q.push(p * 10_000 + i).unwrap()))
        })
        .collect();
    let consumers: Vec<_> = (0..4)
        .map(|_| {
            let (q, sum) = (q.clone(), sum.clone());
            thread::spawn(move || {
                let mut n = 0;
                while let Some(v) = q.pop() {
                    sum.fetch_add(v, Relaxed);
                    n += 1;
                }
                n
            })
        })
        .collect();
    producers.into_iter().for_each(|h| h.join().unwrap());
    q.close(); // all items are in; consumers drain then see None
    let popped: usize = consumers.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(popped, 20_000);
    let expect: u64 = (0..4u64)
        .map(|p| (0..5000u64).map(|i| p * 10_000 + i).sum::<u64>())
        .sum();
    assert_eq!(sum.load(Relaxed), expect, "lost or duplicated items");
    assert!(q.max_len_seen.load(Relaxed) <= 8, "capacity exceeded");
    assert_eq!(q.push(1), Err(1), "push after close is rejected");
    assert!(
        panics(|| drop(BoundedQueue::<u8>::new(0))),
        "capacity 0 is refused"
    );
}

// ---------------------------------------------------------------------------
// Q2. token bucket
// ---------------------------------------------------------------------------
/// ANSWER: state = (tokens, last_refill_time). On each call: refill `elapsed * rate` capped at `burst`, then take `n` if available.
/// The whole read-refill-take sequence must be ONE critical section (or one CAS loop on a packed word), otherwise two threads both
/// see "1 token left". Inject the clock (`now_ns` parameter) so tests are deterministic. Integer milli-tokens avoid float drift,
/// but each division leaves a remainder: carry it into the next refill, or frequent callers get less than the configured rate.
/// Variants: leaky bucket (queue + fixed drain), sliding window log/counter, GCRA (one timestamp per client, see the rate-limit track in the roadmap).
///
/// REALLY TESTING: atomicity of check-then-act and testability (time injection).
/// WRONG: `if tokens > 0 { tokens -= 1 }` on separate atomics; calling `Instant::now()` inside (untestable).
/// FOLLOW-UP: per-user limiters? A sharded map of buckets with eviction. Distributed? Redis Lua / GCRA.
struct TokenBucket {
    state: Mutex<(u64, u64, u64)>, // (milli-tokens, last refill ns, remainder in billionths of a milli-token)
    burst_milli: u64,
    rate_milli_per_s: u64,
}
impl TokenBucket {
    fn new(burst: u64, rate_per_s: u64) -> Self {
        TokenBucket {
            state: Mutex::new((burst.saturating_mul(1000), 0, 0)),
            burst_milli: burst.saturating_mul(1000),
            rate_milli_per_s: rate_per_s.saturating_mul(1000),
        }
    }
    fn try_acquire(&self, now_ns: u64, n: u64) -> bool {
        let mut g = self.state.lock().unwrap();
        let elapsed = now_ns.saturating_sub(g.1);
        let earned = elapsed as u128 * self.rate_milli_per_s as u128 + g.2 as u128;
        let total = g.0 as u128 + earned / 1_000_000_000;
        if total >= self.burst_milli as u128 {
            (g.0, g.2) = (self.burst_milli, 0); // a full bucket saves nothing, not even a fraction
        } else {
            (g.0, g.2) = (total as u64, (earned % 1_000_000_000) as u64);
        }
        g.1 = g.1.max(now_ns); // never back: an earlier time must not be credited again
        // `n` may come from a request: a cost too large to express is refused, not wrapped round to a small one
        match n.checked_mul(1000) {
            Some(cost) if g.0 >= cost => {
                g.0 -= cost;
                true
            }
            _ => false,
        }
    }
}
fn q2_token_bucket() {
    let b = TokenBucket::new(10, 100); // burst 10, 100 tokens/s
    assert_eq!(
        (0..11).filter(|_| b.try_acquire(0, 1)).count(),
        10,
        "burst then deny"
    );
    assert_eq!(
        (0..9).filter(|_| b.try_acquire(50_000_000, 1)).count(),
        5,
        "50 ms at 100/s refills 5"
    );
    assert!(!b.try_acquire(50_000_000, 1));
    assert_eq!(
        (0..99).filter(|_| b.try_acquire(10_000_000_000, 1)).count(),
        10,
        "refill is capped at the burst"
    );
    // Polling every 19,999 ns earns 1.9999 milli-tokens a time. After exactly 10 s at 100/s, 1000 tokens are due.
    let b = TokenBucket::new(1000, 100);
    assert!(b.try_acquire(0, 1000));
    let end = 10_000_000_000u64;
    for t in (19_999..end).step_by(19_999).chain([end]) {
        assert!(b.try_acquire(t, 0)); // takes nothing: only refills
    }
    assert!(
        b.try_acquire(end, 999) && b.try_acquire(end, 1) && !b.try_acquire(end, 1),
        "refill remainders were lost"
    );
    // time spent full earns nothing, and a clock that goes back earns nothing twice (100/s: one token per 10 ms)
    let b = TokenBucket::new(1, 100);
    assert!(b.try_acquire(1_000_000_000, 1));
    assert!(!b.try_acquire(5_000_000, 1));
    assert!(!b.try_acquire(1_009_999_999, 1));
    assert!(b.try_acquire(1_010_000_000, 1));
    // a cost whose milli-token count overflows u64 (this one would wrap to 384) is refused and takes nothing
    let b = TokenBucket::new(10, 1);
    assert!(!b.try_acquire(0, u64::MAX / 1000 + 1) && !b.try_acquire(0, u64::MAX));
    assert!(b.try_acquire(0, 10));
    // 8 threads at the same instant: exactly `burst` admissions, never more
    let b = Arc::new(TokenBucket::new(25, 10));
    let admitted = Arc::new(AtomicUsize::new(0));
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let (b, a) = (b.clone(), admitted.clone());
            thread::spawn(move || {
                for _ in 0..100 {
                    if b.try_acquire(1_000, 1) {
                        a.fetch_add(1, Relaxed);
                    }
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(admitted.load(Relaxed), 25);
}

// ---------------------------------------------------------------------------
// Q3. LRU cache and its concurrent versions
// ---------------------------------------------------------------------------
/// ANSWER: LRU needs O(1) lookup AND O(1) "move to most recent / evict oldest". Classic: HashMap + doubly linked list. In Rust, intrusive
/// pointers fight the borrow checker, so use an index-linked list in a Vec, or (as here) a monotonically increasing TICK per entry plus a
/// `BTreeMap<tick, key>` (O(log n)). Concurrency: `get` MUTATES (recency), so even reads need `&mut`/a Mutex; a RwLock does not help.
/// The fix is SHARDING: N independent LRUs chosen by key hash, each with its own lock (capacity per shard, so global LRU order is approximate).
///
/// REALLY TESTING: can you see that a read path mutates, and do you reach for sharding instead of a global lock or lock-free heroics?
/// WRONG: RwLock<HashMap> with `get` updating a shared linked list without a write lock; evicting while iterating.
/// FOLLOW-UP: TinyLFU/W-TinyLFU admission (see the cookbook `caching` recipes), TTL, `moka`/`quick_cache` as production choices.
struct Lru<K, V> {
    cap: usize,
    tick: u64,
    map: HashMap<K, (V, u64)>,
    order: BTreeMap<u64, K>,
}
impl<K: Hash + Eq + Clone, V: Clone> Lru<K, V> {
    fn new(cap: usize) -> Self {
        Lru {
            cap,
            tick: 0,
            map: HashMap::new(),
            order: BTreeMap::new(),
        }
    }
    fn touch(&mut self, k: &K) {
        self.tick += 1;
        if let Some((_, t)) = self.map.get_mut(k) {
            self.order.remove(t);
            *t = self.tick;
            self.order.insert(self.tick, k.clone());
        }
    }
    fn get(&mut self, k: &K) -> Option<V> {
        let v = self.map.get(k)?.0.clone();
        self.touch(k);
        Some(v)
    }
    fn put(&mut self, k: K, v: V) {
        if self.map.contains_key(&k) {
            self.map.get_mut(&k).unwrap().0 = v;
            self.touch(&k);
            return;
        }
        if self.cap == 0 {
            return; // holds nothing: there is no entry to evict
        }
        if self.map.len() == self.cap {
            let (&oldest, _) = self.order.iter().next().unwrap();
            let victim = self.order.remove(&oldest).unwrap();
            self.map.remove(&victim);
        }
        self.tick += 1;
        self.order.insert(self.tick, k.clone());
        self.map.insert(k, (v, self.tick));
    }
    fn len(&self) -> usize {
        self.map.len()
    }
}
struct ShardedLru<V> {
    shards: Vec<Mutex<Lru<u64, V>>>,
}
impl<V: Clone> ShardedLru<V> {
    fn new(shards: usize, cap_each: usize) -> Self {
        ShardedLru {
            shards: (0..shards)
                .map(|_| Mutex::new(Lru::new(cap_each)))
                .collect(),
        }
    }
    fn shard(&self, k: u64) -> MutexGuard<'_, Lru<u64, V>> {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        k.hash(&mut h);
        self.shards[h.finish() as usize % self.shards.len()]
            .lock()
            .unwrap()
    }
}
fn q3_lru() {
    // differential test against an obviously-correct O(n) model
    let mut lru = Lru::new(5);
    let mut model: Vec<(u64, u64)> = Vec::new(); // front = least recent
    let mut r = Rng(7);
    for _ in 0..20_000 {
        let k = r.below(9);
        if r.below(2) == 0 {
            let v = r.next();
            lru.put(k, v);
            model.retain(|e| e.0 != k);
            model.push((k, v));
            if model.len() > 5 {
                model.remove(0);
            }
        } else {
            let want = model.iter().position(|e| e.0 == k).map(|i| {
                let e = model.remove(i);
                model.push(e);
                e.1
            });
            assert_eq!(lru.get(&k), want);
        }
        assert_eq!(lru.len(), model.len());
    }
    let mut empty = Lru::new(0);
    empty.put(1u64, 1u64);
    assert_eq!(
        (empty.get(&1), empty.len()),
        (None, 0),
        "capacity 0 holds nothing"
    );
    let sh = Arc::new(ShardedLru::new(8, 64));
    let hs: Vec<_> = (0..8u64)
        .map(|t| {
            let sh = sh.clone();
            thread::spawn(move || {
                let mut r = Rng(t + 1);
                for _ in 0..20_000 {
                    let k = r.below(2000);
                    if r.below(3) == 0 {
                        sh.shard(k).put(k, k * 2);
                    } else if let Some(v) = sh.shard(k).get(&k) {
                        assert_eq!(v, k * 2, "value does not belong to its key");
                    }
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert!(
        sh.shards.iter().all(|s| s.lock().unwrap().len() <= 64),
        "a shard exceeded its capacity"
    );
}

// ---------------------------------------------------------------------------
// Q4. reader-writer lock, fairness
// ---------------------------------------------------------------------------
/// ANSWER: state `(readers, writer_active, writers_waiting)` under a Mutex + Condvar. READER-PREFERRING: readers enter whenever no writer
/// is ACTIVE: a steady stream of readers starves writers forever. WRITER-PREFERRING (below): new readers also wait while a writer is
/// QUEUED, so writers get through, at the price of reader latency spikes and a reader-lock-recursion deadlock (a reader that re-enters
/// while a writer waits blocks on itself). FAIR (FIFO) locks use tickets. `std::sync::RwLock` leaves the policy to the OS (documented as unspecified).
///
/// REALLY TESTING: starvation reasoning, not syntax.
/// WRONG: "RwLock is always faster than Mutex" (it is slower under short critical sections and writer-heavy loads).
/// FOLLOW-UP: `parking_lot::RwLock` is eventually fair; seqlocks avoid reader locks entirely (see `seqlock_and_priority_inversion`).
struct WpRwLock {
    st: Mutex<(usize, bool, usize)>, // readers, writer_active, writers_waiting
    cv: Condvar,
}
impl WpRwLock {
    fn new() -> Self {
        WpRwLock {
            st: Mutex::new((0, false, 0)),
            cv: Condvar::new(),
        }
    }
    fn read_lock(&self) {
        let mut g = self.st.lock().unwrap();
        while g.1 || g.2 > 0 {
            g = self.cv.wait(g).unwrap();
        }
        g.0 += 1;
    }
    fn read_unlock(&self) {
        let mut g = self.st.lock().unwrap();
        g.0 -= 1;
        if g.0 == 0 {
            self.cv.notify_all();
        }
    }
    fn write_lock(&self) {
        let mut g = self.st.lock().unwrap();
        g.2 += 1;
        while g.1 || g.0 > 0 {
            g = self.cv.wait(g).unwrap();
        }
        g.2 -= 1;
        g.1 = true;
    }
    fn write_unlock(&self) {
        self.st.lock().unwrap().1 = false;
        self.cv.notify_all();
    }
    fn writers_waiting(&self) -> usize {
        self.st.lock().unwrap().2
    }
}
fn q4_rwlock() {
    // deterministic starvation test: a queued writer must block NEW readers
    let l = Arc::new(WpRwLock::new());
    let log = Arc::new(Mutex::new(Vec::new()));
    l.read_lock(); // reader A holds
    let w = {
        let (l, log) = (l.clone(), log.clone());
        thread::spawn(move || {
            l.write_lock();
            log.lock().unwrap().push("writer");
            l.write_unlock();
        })
    };
    while l.writers_waiting() == 0 {
        thread::yield_now(); // wait until the writer is queued
    }
    let b = {
        let (l, log) = (l.clone(), log.clone());
        thread::spawn(move || {
            l.read_lock();
            log.lock().unwrap().push("reader B");
            l.read_unlock();
        })
    };
    thread::sleep(Duration::from_millis(30));
    assert!(
        log.lock().unwrap().is_empty(),
        "reader B must not overtake the queued writer"
    );
    l.read_unlock(); // A leaves: writer goes first, then B
    w.join().unwrap();
    b.join().unwrap();
    assert_eq!(*log.lock().unwrap(), ["writer", "reader B"]);
    // exclusion invariant under load: writers keep two counters equal only between writes
    let (a, bb) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    let hs: Vec<_> = (0..6)
        .map(|i| {
            let (l, a, b) = (l.clone(), a.clone(), bb.clone());
            thread::spawn(move || {
                for _ in 0..3000 {
                    if i < 2 {
                        l.write_lock();
                        a.fetch_add(1, Relaxed);
                        thread::yield_now();
                        b.fetch_add(1, Relaxed);
                        l.write_unlock();
                    } else {
                        l.read_lock();
                        assert_eq!(
                            a.load(Relaxed),
                            b.load(Relaxed),
                            "reader observed a writer mid-update"
                        );
                        l.read_unlock();
                    }
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(a.load(Relaxed), 6000);
}

// ---------------------------------------------------------------------------
// Q5. dining philosophers
// ---------------------------------------------------------------------------
/// ANSWER: deadlock needs a CYCLE of waits (everyone holds the left fork, waits for the right). Break any Coffman condition. The cleanest:
/// impose a GLOBAL ORDER on forks and always take the lower-numbered one first (breaks circular wait). Others: a waiter semaphore allowing at
/// most N-1 diners, `try_lock` + back-off (livelock risk), or one philosopher left-handed (same as ordering).
///
/// REALLY TESTING: do you name the 4 Coffman conditions and use lock ordering rather than timeouts?
/// WRONG: "just add a sleep", or a global lock around eating (correct but no concurrency).
/// FOLLOW-UP: how do you prove starvation-freedom? You need fairness (ticket lock); ordering alone gives deadlock-freedom only.
fn q5_philosophers() {
    const N: usize = 5;
    let forks: Arc<Vec<Mutex<()>>> = Arc::new((0..N).map(|_| Mutex::new(())).collect());
    let eating: Arc<Vec<AtomicBool>> = Arc::new((0..N).map(|_| AtomicBool::new(false)).collect());
    let meals = Arc::new((0..N).map(|_| AtomicUsize::new(0)).collect::<Vec<_>>());
    let hs: Vec<_> = (0..N)
        .map(|i| {
            let (forks, eating, meals) = (forks.clone(), eating.clone(), meals.clone());
            thread::spawn(move || {
                let (l, r) = (i, (i + 1) % N);
                let (first, second) = (l.min(r), l.max(r)); // global order: the ONLY line that matters
                for _ in 0..2000 {
                    let _a = forks[first].lock().unwrap();
                    let _b = forks[second].lock().unwrap();
                    eating[i].store(true, SeqCst);
                    assert!(
                        !eating[l.checked_sub(1).unwrap_or(N - 1)].load(SeqCst)
                            && !eating[r].load(SeqCst),
                        "two neighbours eating at once"
                    );
                    meals[i].fetch_add(1, Relaxed);
                    eating[i].store(false, SeqCst);
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap()); // finishing at all proves no deadlock
    assert!(meals.iter().all(|m| m.load(Relaxed) == 2000));
}

// ---------------------------------------------------------------------------
// Q6. FooBar
// ---------------------------------------------------------------------------
/// ANSWER: a shared TURN variable under a Mutex + Condvar. Each thread waits for its turn (`wait_while`), does its print, flips the turn, `notify_all`
/// (two waiters on one condvar, so `notify_one` could wake the wrong one). Two semaphores/channels work too: each thread owns a "go" token.
///
/// REALLY TESTING: coordination of ordering with a predicate, not sleeping.
/// WRONG: `thread::sleep` to order output; busy-spinning on an unsynchronised bool.
/// FOLLOW-UP: generalise to N threads in a ring (turn = i mod N), as in Q7.
fn q6_foobar() {
    let n = 200;
    let st = Arc::new((Mutex::new(true), Condvar::new())); // true = foo's turn
    let out = Arc::new(Mutex::new(String::new()));
    let mk = |is_foo: bool, word: &'static str| {
        let (st, out) = (st.clone(), out.clone());
        thread::spawn(move || {
            for _ in 0..n {
                let g =
                    st.1.wait_while(st.0.lock().unwrap(), |turn| *turn != is_foo)
                        .unwrap();
                out.lock().unwrap().push_str(word);
                let mut g = g;
                *g = !is_foo;
                st.1.notify_all();
            }
        })
    };
    let (a, b) = (mk(true, "foo"), mk(false, "bar"));
    a.join().unwrap();
    b.join().unwrap();
    assert_eq!(*out.lock().unwrap(), "foobar".repeat(n));
}

// ---------------------------------------------------------------------------
// Q7. ZeroEvenOdd
// ---------------------------------------------------------------------------
/// ANSWER: a state machine `turn in {Zero, Odd, Even}` plus a counter. Zero prints 0 and chooses the next turn from the parity of the next
/// number; odd/even print their number and hand the turn back to Zero. `notify_all` because three threads wait on one condvar.
///
/// REALLY TESTING: generalising Q6 to a state machine and not deadlocking when `n` is odd or even (each worker must stop after ITS last number).
/// WRONG: workers looping `while i <= n` without a predicate on turn; zero thread that never exits.
/// FOLLOW-UP: rewrite with three channels: each thread blocks on `recv` and sends a token to the next.
#[derive(PartialEq, Clone, Copy)]
enum Turn {
    Zero,
    Odd,
    Even,
}
fn zero_even_odd(n: u32) -> String {
    let st = Arc::new((Mutex::new((Turn::Zero, 1u32)), Condvar::new()));
    let out = Arc::new(Mutex::new(String::new()));
    let zero = {
        let (st, out) = (st.clone(), out.clone());
        thread::spawn(move || {
            for _ in 1..=n {
                let mut g =
                    st.1.wait_while(st.0.lock().unwrap(), |s| s.0 != Turn::Zero)
                        .unwrap();
                out.lock().unwrap().push('0');
                g.0 = if g.1 % 2 == 1 { Turn::Odd } else { Turn::Even };
                st.1.notify_all();
            }
        })
    };
    let worker = |me: Turn, start: u32| {
        let (st, out) = (st.clone(), out.clone());
        thread::spawn(move || {
            for k in (start..=n).step_by(2) {
                let mut g =
                    st.1.wait_while(st.0.lock().unwrap(), |s| s.0 != me)
                        .unwrap();
                out.lock().unwrap().push_str(&k.to_string());
                g.1 += 1;
                g.0 = Turn::Zero;
                st.1.notify_all();
            }
        })
    };
    let (o, e) = (worker(Turn::Odd, 1), worker(Turn::Even, 2));
    for h in [zero, o, e] {
        h.join().unwrap();
    }
    let s = out.lock().unwrap().clone();
    s
}
fn q7_zero_even_odd() {
    assert_eq!(zero_even_odd(5), "0102030405");
    assert_eq!(zero_even_odd(6), "010203040506");
    assert_eq!(zero_even_odd(1), "01");
    let expect: String = (1..=40).map(|i| format!("0{i}")).collect();
    assert_eq!(zero_even_odd(40), expect);
}

// ---------------------------------------------------------------------------
// Q8. reusable barrier
// ---------------------------------------------------------------------------
/// ANSWER: count arrivals; the LAST arrival resets the count, bumps a GENERATION number and `notify_all`s. Waiters wait until the generation
/// changes, not until `count == n`: the last arrival resets the count, and a fast thread can start the next round before a slow one wakes,
/// so the slow one may never see the value it waits for. That is the reuse bug. `std::sync::Barrier` works this way, and its `wait` result
/// has `is_leader()`.
///
/// REALLY TESTING: the generation counter (reusability) and predicate-loop discipline.
/// WRONG: a one-shot barrier built on a bare counter and `wait` without a predicate.
/// FOLLOW-UP: tree/sense-reversing barriers for many cores; phasers.
struct Barrier {
    st: Mutex<(usize, u64)>,
    cv: Condvar,
    n: usize,
}
impl Barrier {
    fn new(n: usize) -> Self {
        assert!(n > 0, "a barrier for nobody would never open");
        Barrier {
            st: Mutex::new((0, 0)),
            cv: Condvar::new(),
            n,
        }
    }
    fn wait(&self) -> bool {
        let mut g = self.st.lock().unwrap();
        let gen_ = g.1;
        g.0 += 1;
        if g.0 == self.n {
            g.0 = 0;
            g.1 += 1;
            self.cv.notify_all();
            true
        } else {
            let _g = self.cv.wait_while(g, |s| s.1 == gen_).unwrap();
            false
        }
    }
}
fn q8_barrier() {
    const T: usize = 6;
    const PHASES: usize = 200;
    let b = Arc::new(Barrier::new(T));
    let progress: Arc<Vec<AtomicUsize>> = Arc::new((0..T).map(|_| AtomicUsize::new(0)).collect());
    let leaders = Arc::new(AtomicUsize::new(0));
    let hs: Vec<_> = (0..T)
        .map(|i| {
            let (b, p, leaders) = (b.clone(), progress.clone(), leaders.clone());
            thread::spawn(move || {
                for phase in 1..=PHASES {
                    p[i].store(phase, SeqCst);
                    if b.wait() {
                        leaders.fetch_add(1, Relaxed);
                    }
                    // after the barrier EVERY thread has reached this phase, and none can start the next one yet
                    for j in 0..T {
                        let v = p[j].load(SeqCst);
                        assert!(
                            v == phase,
                            "thread {j} at phase {v} while we are past phase {phase}"
                        );
                    }
                    b.wait(); // second barrier so nobody starts the next phase's store early (keeps the check above exact)
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(
        leaders.load(Relaxed),
        PHASES,
        "exactly one leader per round"
    );
    assert!(
        panics(|| drop(Barrier::new(0))),
        "a barrier for 0 threads is refused"
    );
}

// ---------------------------------------------------------------------------
// Q9. semaphore
// ---------------------------------------------------------------------------
/// ANSWER: a permit count under a Mutex; `acquire` waits while count == 0 then decrements, `release` increments and `notify_one`.
/// Return an RAII guard so a panic or early return cannot leak a permit. A semaphore with 1 permit is NOT a mutex: any thread may release it
/// (no ownership), which is both its power (signalling) and its danger.
///
/// REALLY TESTING: building a primitive from a lower one and RAII.
/// WRONG: busy-wait on an AtomicUsize with `yield_now`; releasing without a guard.
/// FOLLOW-UP: fairness (FIFO waiters), `tokio::sync::Semaphore` for async limits (see `joinset_semaphore`).
struct Semaphore {
    permits: Mutex<usize>,
    cv: Condvar,
}
struct Permit<'a>(&'a Semaphore);
impl Semaphore {
    fn new(n: usize) -> Self {
        Semaphore {
            permits: Mutex::new(n),
            cv: Condvar::new(),
        }
    }
    fn acquire(&self) -> Permit<'_> {
        let mut g = self
            .cv
            .wait_while(self.permits.lock().unwrap(), |p| *p == 0)
            .unwrap();
        *g -= 1;
        Permit(self)
    }
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self.0.permits.lock().unwrap() += 1;
        self.0.cv.notify_one();
    }
}
fn q9_semaphore() {
    let sem = Arc::new(Semaphore::new(3));
    let (cur, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let hs: Vec<_> = (0..12)
        .map(|_| {
            let (sem, cur, peak) = (sem.clone(), cur.clone(), peak.clone());
            thread::spawn(move || {
                for _ in 0..200 {
                    let _p = sem.acquire();
                    let now = cur.fetch_add(1, SeqCst) + 1;
                    peak.fetch_max(now, SeqCst);
                    thread::yield_now();
                    cur.fetch_sub(1, SeqCst);
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert!(peak.load(SeqCst) <= 3, "more than 3 threads inside");
    assert_eq!(*sem.permits.lock().unwrap(), 3, "all permits returned");
}

// ---------------------------------------------------------------------------
// Q10. lazy init
// ---------------------------------------------------------------------------
/// ANSWER: in Rust use `OnceLock`/`LazyLock` (the closure runs exactly once; other callers block until it finishes, then all see the value).
/// Double-checked locking by hand: fast path `if ready.load(Acquire)`; slow path takes a lock, re-checks, initialises, then
/// `ready.store(true, Release)`. The Release/Acquire pair is what makes the initialised DATA visible to a thread that only saw the flag. With
/// Relaxed the reader could see `ready == true` but stale data (on weak-memory CPUs; the compiler may also reorder).
///
/// REALLY TESTING: why plain `static mut` + bool flag is a data race, and what Acquire/Release buy.
/// WRONG: `if !INIT { INIT = true; init() }`; a lazy global without synchronisation.
/// FOLLOW-UP: what if init panics? `OnceLock` stays uninitialised and the next caller retries (`Once` poisons).
struct ManualLazy {
    ready: AtomicBool,
    data: AtomicU64,
    lock: Mutex<()>,
    inits: AtomicUsize,
}
impl ManualLazy {
    fn get(&self) -> u64 {
        if self.ready.load(Acquire) {
            return self.data.load(Relaxed); // ordered after the flag by Acquire
        }
        let _g = self.lock.lock().unwrap();
        if !self.ready.load(Relaxed) {
            self.inits.fetch_add(1, Relaxed);
            self.data.store(42, Relaxed);
            self.ready.store(true, Release); // publishes `data`
        }
        self.data.load(Relaxed)
    }
}
fn q10_lazy() {
    static CELL: OnceLock<u64> = OnceLock::new();
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    let hs: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(|| {
                *CELL.get_or_init(|| {
                    CALLS.fetch_add(1, Relaxed);
                    thread::sleep(Duration::from_millis(5));
                    7
                })
            })
        })
        .collect();
    assert!(hs.into_iter().all(|h| h.join().unwrap() == 7));
    assert_eq!(CALLS.load(Relaxed), 1);
    let l = Arc::new(ManualLazy {
        ready: AtomicBool::new(false),
        data: AtomicU64::new(0),
        lock: Mutex::new(()),
        inits: AtomicUsize::new(0),
    });
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let l = l.clone();
            thread::spawn(move || (0..1000).all(|_| l.get() == 42))
        })
        .collect();
    assert!(hs.into_iter().all(|h| h.join().unwrap()));
    assert_eq!(l.inits.load(Relaxed), 1);
}

// ---------------------------------------------------------------------------
// Q11. lock-order graph (lockdep)
// ---------------------------------------------------------------------------
/// ANSWER: deadlock potential = a CYCLE in the "lock A was held while acquiring lock B" graph, even if the deadlock never happened in this run.
/// Record an edge `h -> L` for every lock `h` the thread holds when it takes `L`; if adding the edge closes a cycle, report both orders.
/// This is the idea behind Linux lockdep. Prevention: a global lock hierarchy. Detection of an ACTUAL deadlock uses the wait-for graph
/// instead (thread -> lock it waits for -> thread that holds it), as `parking_lot`'s optional deadlock detector does.
///
/// REALLY TESTING: graph thinking; potential vs actual deadlock.
/// WRONG: relying on timeouts in production to "detect" deadlocks; reasoning only about two locks.
/// FOLLOW-UP: reader locks, trylock edges (do not create waits), lock classes instead of instances.
#[derive(Default)]
struct LockOrder {
    edges: HashMap<usize, HashSet<usize>>,
    held: HashMap<usize, Vec<usize>>, // thread -> locks held
}
impl LockOrder {
    fn reaches(&self, from: usize, to: usize) -> bool {
        let (mut stack, mut seen) = (vec![from], HashSet::new());
        while let Some(n) = stack.pop() {
            if n == to {
                return true;
            }
            if seen.insert(n) {
                stack.extend(self.edges.get(&n).into_iter().flatten().copied());
            }
        }
        false
    }
    /// Returns Err((held, wanted)) if taking `lock` closes a cycle.
    fn acquire(&mut self, th: usize, lock: usize) -> Result<(), (usize, usize)> {
        let held = self.held.entry(th).or_default().clone();
        for &h in &held {
            if self.reaches(lock, h) {
                return Err((h, lock)); // some thread took `lock` before `h` earlier: opposite order
            }
        }
        for &h in &held {
            self.edges.entry(h).or_default().insert(lock);
        }
        self.held.get_mut(&th).unwrap().push(lock);
        Ok(())
    }
    fn release(&mut self, th: usize, lock: usize) {
        self.held.get_mut(&th).unwrap().retain(|&l| l != lock);
    }
}
fn has_cycle(edges: &HashSet<(usize, usize)>, n: usize) -> bool {
    // Kahn: a graph is acyclic iff all nodes can be removed in indegree order
    let mut indeg = vec![0; n];
    edges.iter().for_each(|&(_, b)| indeg[b] += 1);
    let mut q: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut removed = 0;
    while let Some(x) = q.pop() {
        removed += 1;
        for &(a, b) in edges {
            if a == x {
                indeg[b] -= 1;
                if indeg[b] == 0 {
                    q.push(b);
                }
            }
        }
    }
    removed != n
}
fn q11_lock_order() {
    let mut g = LockOrder::default();
    assert!(g.acquire(1, 10).is_ok() && g.acquire(1, 20).is_ok()); // T1: A then B
    g.release(1, 20);
    g.release(1, 10);
    assert!(g.acquire(2, 20).is_ok());
    assert_eq!(
        g.acquire(2, 10),
        Err((20, 10)),
        "T2 takes B then A: potential deadlock reported WITHOUT it happening"
    );
    // randomised: the incremental detector fires iff the union of all threads' ordering edges has a cycle
    let mut r = Rng(99);
    let mut outcomes = [0u32; 2]; // [clean histories, rejected histories]: both must occur or the comparison is vacuous
    for _ in 0..500 {
        let mut g = LockOrder::default();
        let mut edges = HashSet::new();
        let mut detected = false;
        'outer: for th in 0..4 {
            let seq: Vec<usize> = {
                let mut v: Vec<usize> = (0..5).collect();
                for i in (1..5).rev() {
                    v.swap(i, r.below(i as u64 + 1) as usize);
                }
                v.truncate(2 + r.below(3) as usize);
                v
            };
            for (i, &l) in seq.iter().enumerate() {
                // the oracle graph holds every ATTEMPTED ordering edge: record them before asking, because the
                // rejected acquisition is exactly the one whose edges close the cycle
                assert!(
                    !has_cycle(&edges, 5),
                    "the detector accepted an acquisition that closed a cycle"
                );
                (0..i).for_each(|j| {
                    edges.insert((seq[j], l));
                });
                if g.acquire(th, l).is_err() {
                    detected = true;
                    break 'outer;
                }
            }
        }
        assert_eq!(
            detected,
            has_cycle(&edges, 5),
            "incremental detector disagrees with the full-graph check"
        );
        outcomes[usize::from(detected)] += 1;
    }
    assert!(
        outcomes.iter().all(|&n| n > 20),
        "random histories must exercise both verdicts: {outcomes:?}"
    );
}

// ---------------------------------------------------------------------------
// Q12. thread pool with graceful shutdown
// ---------------------------------------------------------------------------
/// ANSWER: N workers share a `Receiver` behind `Arc<Mutex<_>>` (std mpsc is single-consumer). `execute` sends a boxed `FnOnce + Send`. GRACEFUL
/// shutdown = drop the Sender (workers see `Err` from `recv` once the queue is DRAINED) then join every worker; jobs already queued still run.
/// Wrap each job in `catch_unwind` so one panic does not kill a worker (or the pool silently loses capacity). Taking `self` by value in
/// `shutdown` makes "execute after shutdown" a compile error.
///
/// REALLY TESTING: shutdown protocol, panic isolation, why the receiver is shared, and not holding the receiver lock while running the job.
/// WRONG: `loop { if let Ok(job) = rx.lock().unwrap().recv() { job() } }` (the temporary guard lives through `job()` and serialises all workers).
/// FOLLOW-UP: bounded queue and rejection policy, work stealing (see `thread_pool_work_stealing`), `rayon::ThreadPool`.
type Job = Box<dyn FnOnce() + Send + 'static>;
struct Pool {
    tx: Option<mpsc::Sender<Job>>,
    workers: Vec<thread::JoinHandle<()>>,
}
impl Pool {
    fn new(n: usize) -> Self {
        assert!(n > 0, "a pool without workers would run nothing");
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let workers = (0..n)
            .map(|_| {
                let rx = rx.clone();
                thread::spawn(move || {
                    loop {
                        let job = rx.lock().unwrap().recv(); // guard dropped at the end of this statement
                        match job {
                            Ok(job) => {
                                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                            }
                            Err(_) => break, // sender dropped AND queue empty
                        }
                    }
                })
            })
            .collect();
        Pool {
            tx: Some(tx),
            workers,
        }
    }
    fn execute(&self, f: impl FnOnce() + Send + 'static) {
        self.tx.as_ref().unwrap().send(Box::new(f)).unwrap();
    }
    fn shutdown(mut self) {
        drop(self.tx.take());
        for w in self.workers.drain(..) {
            w.join().unwrap();
        }
    }
}
fn q12_pool() {
    let done = Arc::new(AtomicUsize::new(0));
    let pool = Pool::new(4);
    quiet(|| {
        for i in 0..1000 {
            let done = done.clone();
            pool.execute(move || {
                if i % 100 == 7 {
                    panic!("job {i} fails on purpose");
                }
                done.fetch_add(1, Relaxed);
            });
        }
        pool.shutdown(); // returns only after ALL queued jobs ran
    });
    assert_eq!(
        done.load(Relaxed),
        1000 - 10,
        "every non-panicking job ran before shutdown returned"
    );
    assert!(panics(|| drop(Pool::new(0))), "0 workers is refused");
}

// ---------------------------------------------------------------------------
// Q13. parallel merge sort
// ---------------------------------------------------------------------------
/// ANSWER: split in halves, sort each in its own scoped thread (`split_at_mut` gives two disjoint `&mut` borrows, no unsafe), merge through a
/// temp buffer. Stop spawning below a THRESHOLD or past depth log2(cores): thread creation costs ~10-50 us, more than sorting 1000 items.
/// The merge is sequential (O(n)) so speed-up is bounded (Amdahl); a parallel merge splits by binary search. Rayon's `par_sort` is the real answer.
///
/// REALLY TESTING: scoped threads and disjoint borrows (the Rust-specific part), granularity control.
/// WRONG: spawning a thread per element/recursion level without a cutoff; `Arc<Mutex<Vec>>` around the whole slice.
/// FOLLOW-UP: stable? yes if the merge takes from the left run on ties. Memory: one temp buffer per merge vs one shared.
fn psort(v: &mut [i64], depth: u32) {
    if v.len() <= 1024 || depth == 0 {
        v.sort();
        return;
    }
    let mid = v.len() / 2;
    {
        let (l, r) = v.split_at_mut(mid);
        thread::scope(|s| {
            s.spawn(|| psort(l, depth - 1));
            psort(r, depth - 1);
        });
    }
    let mut tmp = Vec::with_capacity(v.len());
    let (mut i, mut j) = (0, mid);
    while i < mid && j < v.len() {
        if v[j] < v[i] {
            tmp.push(v[j]);
            j += 1;
        } else {
            tmp.push(v[i]); // ties from the left run first => stable
            i += 1;
        }
    }
    tmp.extend_from_slice(&v[i..mid]);
    tmp.extend_from_slice(&v[j..]);
    v.copy_from_slice(&tmp);
}
fn q13_merge_sort() {
    let mut r = Rng(5);
    for n in [0usize, 1, 2, 3, 1023, 1025, 5000, 300_000] {
        let mut a: Vec<i64> = (0..n).map(|_| (r.next() % 1000) as i64 - 500).collect(); // many duplicates
        let mut want = a.clone();
        want.sort();
        psort(&mut a, 3);
        assert_eq!(a, want, "n={n}");
    }
    let mut sorted: Vec<i64> = (0..10_000).collect();
    psort(&mut sorted, 3);
    assert!(sorted.windows(2).all(|w| w[0] <= w[1]));
    let mut rev: Vec<i64> = (0..10_000).rev().collect();
    psort(&mut rev, 3);
    assert!(rev.windows(2).all(|w| w[0] <= w[1]));
}

// ---------------------------------------------------------------------------
// Q14. concurrent word count
// ---------------------------------------------------------------------------
/// ANSWER: three designs. (1) one `Mutex<HashMap>` updated per word: correct, serialises everything. (2) MAP-REDUCE: each thread counts its own chunk
/// into a private map (no sharing), then merge the maps: usually fastest. (3) sharded locks by word hash: a middle path when results must be
/// visible while counting. Chunk boundaries must fall on word boundaries (split on whitespace, never mid-word).
///
/// REALLY TESTING: do you minimise shared mutable state before reaching for a clever concurrent map?
/// WRONG: `Arc<Mutex<HashMap>>` locked for the entire chunk; splitting the text at arbitrary byte offsets.
/// FOLLOW-UP: skewed words (hot key) in design 3; Unicode word boundaries; streaming input; rayon `fold`/`reduce`.
fn count_seq(words: &[&str]) -> HashMap<String, u64> {
    let mut m = HashMap::new();
    for w in words {
        *m.entry(w.to_string()).or_insert(0) += 1;
    }
    m
}
fn q14_word_count() {
    let vocab = [
        "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog", "rust", "thread", "lock",
        "queue",
    ];
    let mut r = Rng(3);
    let text: String = (0..60_000)
        .map(|_| vocab[(r.below(12) * r.below(12) / 11) as usize])
        .collect::<Vec<_>>()
        .join(" ");
    let words: Vec<&str> = text.split_whitespace().collect();
    let want = count_seq(&words);
    let chunks: Vec<&[&str]> = words.chunks(words.len() / 8 + 1).collect();
    // (1) global mutex
    let global = Mutex::new(HashMap::<String, u64>::new());
    thread::scope(|s| {
        for c in &chunks {
            let global = &global;
            s.spawn(move || {
                for w in *c {
                    *global.lock().unwrap().entry(w.to_string()).or_insert(0) += 1;
                }
            });
        }
    });
    assert_eq!(global.into_inner().unwrap(), want);
    // (2) per-thread maps merged
    let parts: Vec<HashMap<String, u64>> = thread::scope(|s| {
        let hs: Vec<_> = chunks
            .iter()
            .map(|c| s.spawn(move || count_seq(c)))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut merged: HashMap<String, u64> = HashMap::new();
    for p in parts {
        for (k, v) in p {
            *merged.entry(k).or_insert(0) += v;
        }
    }
    assert_eq!(merged, want);
    // (3) sharded by word hash
    let shards: Vec<Mutex<HashMap<String, u64>>> =
        (0..8).map(|_| Mutex::new(HashMap::new())).collect();
    thread::scope(|s| {
        for c in &chunks {
            let shards = &shards;
            s.spawn(move || {
                for w in *c {
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    w.hash(&mut h);
                    *shards[h.finish() as usize % 8]
                        .lock()
                        .unwrap()
                        .entry(w.to_string())
                        .or_insert(0) += 1;
                }
            });
        }
    });
    let mut sharded = HashMap::new();
    for s in shards {
        sharded.extend(s.into_inner().unwrap());
    }
    assert_eq!(sharded, want);
    assert_eq!(want.values().sum::<u64>(), words.len() as u64);
}

// ---------------------------------------------------------------------------
// Q15. atomic orderings
// ---------------------------------------------------------------------------
/// ANSWER: Ordering says what OTHER memory operations become visible, not whether the atomic itself is atomic (every ordering is atomic).
/// - A pure statistics counter: `Relaxed`. Nobody uses the count to infer anything about other data.
/// - PUBLISHING data via a flag: writer `store(Release)` after writing the data, reader `load(Acquire)` before reading it. The pair creates
///   a happens-before edge. - Several flags that must be seen in a single global order (Dekker/Peterson, store-buffering): `SeqCst`.
/// - Locks: lock = Acquire, unlock = Release. - `fetch_add` is a single RMW: no lost updates at ANY ordering; `load` then `store` loses them.
/// x86 hides most mistakes (it is strong); ARM exposes them. Test with miri/loom, not by running on your laptop (see `memory_ordering_litmus`).
///
/// REALLY TESTING: whether you cargo-cult SeqCst everywhere or can justify the weakest sufficient ordering.
/// WRONG: "Relaxed is racy" (it is race-free, just unordered with other locations); "SeqCst makes the other thread see it faster" (no, ordering is not latency).
/// FOLLOW-UP: what does `fence(Acquire)` do? Orders prior relaxed loads before later operations (the seqlock reader uses it).
fn q15_orderings() {
    let c = Arc::new(AtomicU64::new(0));
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let c = c.clone();
            thread::spawn(move || {
                (0..100_000).for_each(|_| {
                    c.fetch_add(1, Relaxed);
                })
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(
        c.load(Relaxed),
        800_000,
        "fetch_add never loses updates, even Relaxed"
    );
    // load-then-store loses updates (the classic bug); we only assert the upper bound since the loss is probabilistic
    let c = Arc::new(AtomicU64::new(0));
    let hs: Vec<_> = (0..4)
        .map(|_| {
            let c = c.clone();
            thread::spawn(move || {
                for _ in 0..100_000 {
                    let v = c.load(Relaxed);
                    c.store(v + 1, Relaxed);
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert!(c.load(Relaxed) <= 400_000);
    println!(
        "   (load+store counter ended at {} of 400000)",
        c.load(Relaxed)
    );
    // message passing: Release/Acquire publication, repeated many times
    for round in 0..2000u64 {
        let (data, flag) = (
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(false)),
        );
        let reader = {
            let (data, flag) = (data.clone(), flag.clone());
            thread::spawn(move || {
                while !flag.load(Acquire) {
                    std::hint::spin_loop();
                }
                data.load(Relaxed)
            })
        };
        data.store(round + 1, Relaxed);
        flag.store(true, Release);
        assert_eq!(
            reader.join().unwrap(),
            round + 1,
            "data not published by Release/Acquire"
        );
    }
}

// ---------------------------------------------------------------------------
// Q16. ABA
// ---------------------------------------------------------------------------
/// ANSWER: a CAS checks that a location still holds the value I read earlier, not that NOTHING changed. If the value goes A -> B -> A in
/// between (typically a node freed and re-allocated at the same address/index) the CAS succeeds on stale assumptions. In a Treiber stack: T1
/// reads head=A, next=B and stalls; T2 pops A, pops B, pushes A back (now A.next=C); T1's CAS(head, A, B) succeeds and head=B, a node no longer
/// in the stack. Fixes: a TAG/version in the same word (double-width CAS or packed index+tag), hazard pointers or epochs (never reuse memory
/// while someone may hold it), garbage collection, or avoiding CAS-on-pointer designs. See `lockfree_stack_aba` and, mechanically proved, `model_checker_for_concurrent_code`.
///
/// REALLY TESTING: can you reproduce it by hand (a precise interleaving) and name the fixes?
/// WRONG: "ABA only matters with malloc" (any reuse of an index/slot has it); "the tag solves it forever" (it wraps after 2^bits CASes).
/// FOLLOW-UP: why does a garbage-collected language avoid it? The A node cannot be reused while T1 still references it.
struct ArenaStack {
    next: Vec<usize>,   // next[i] = index below node i (0 = none); nodes are 1..
    head: (usize, u32), // (node, tag)
}
impl ArenaStack {
    fn cas(&mut self, expect: (usize, u32), new_node: usize, tagged: bool) -> bool {
        let cur = if tagged { self.head } else { (self.head.0, 0) };
        let exp = if tagged { expect } else { (expect.0, 0) };
        if cur == exp {
            self.head = (new_node, self.head.1 + 1);
            true
        } else {
            false
        }
    }
}
fn aba_scenario(tagged: bool) -> ArenaStack {
    let mut s = ArenaStack {
        next: vec![0, 2, 3, 0],
        head: (1, 0),
    }; // A=1 -> B=2 -> C=3
    // T1 reads head and next, then is preempted
    let (t1_head, t1_next) = (s.head, s.next[s.head.0]);
    // T2 runs to completion: pop A, pop B, push A back on top of C
    let (a, b) = (s.head, s.head.0);
    let nb = s.next[b];
    assert!(s.cas(a, nb, tagged)); // pop A: head = B
    let b_head = s.head;
    let nb2 = s.next[b_head.0];
    assert!(s.cas(b_head, nb2, tagged)); // pop B: head = C
    s.next[1] = s.head.0; // A.next = C
    let c_head = s.head;
    assert!(s.cas(c_head, 1, tagged)); // push A: head = A (same node id as T1 saw)
    // T1 resumes
    let ok = s.cas(t1_head, t1_next, tagged);
    assert_eq!(
        ok, !tagged,
        "untagged CAS succeeds wrongly; tagged CAS must fail"
    );
    s
}
fn q16_aba() {
    let bad = aba_scenario(false);
    assert_eq!(
        bad.head.0, 2,
        "head now points at B, which was popped: corruption"
    );
    let good = aba_scenario(true);
    assert_eq!(
        good.head.0, 1,
        "tagged CAS failed as it should; head is still A"
    );
}

// ---------------------------------------------------------------------------
// Q17. Mutex vs channel vs atomic
// ---------------------------------------------------------------------------
/// ANSWER: same job (sum of squares 1..=N over 8 threads) three ways. `Arc<Mutex<u64>>`: simplest mental model, fine when the lock is held
/// briefly and rarely; contention grows with per-item locking. CHANNEL: threads send partial results, one owner accumulates: no shared mutable
/// state, natural for pipelines and back-pressure. ATOMIC: one RMW instruction, best for single counters/flags, useless for multi-word invariants.
/// Best of all: reduce LOCALLY (each thread sums its slice) and combine once. Rule of thumb: share memory with a lock for data, communicate
/// with channels for work/events, use atomics for flags and counters.
///
/// REALLY TESTING: judgement, and the observation that batching beats any primitive.
/// WRONG: "channels are always slower/faster"; locking per item in a hot loop.
/// FOLLOW-UP: when do channels deadlock? Unbounded growth, bounded cycles (A sends to B while B sends to A, both full).
fn q17_three_ways() {
    const N: u64 = 400_000;
    let expect: u64 = (1..=N).map(|i| i * i % 1000).sum();
    let slices: Vec<(u64, u64)> = (0..8).map(|t| (1 + t * N / 8, (t + 1) * N / 8)).collect();
    let t0 = Instant::now();
    let m = Arc::new(Mutex::new(0u64));
    thread::scope(|s| {
        for &(a, b) in &slices {
            let m = m.clone();
            s.spawn(move || (a..=b).for_each(|i| *m.lock().unwrap() += i * i % 1000)); // lock per item
        }
    });
    let t_mutex = t0.elapsed();
    let t0 = Instant::now();
    let (tx, rx) = mpsc::channel();
    thread::scope(|s| {
        for &(a, b) in &slices {
            let tx = tx.clone();
            s.spawn(move || tx.send((a..=b).map(|i| i * i % 1000).sum::<u64>()).unwrap()); // one message per thread
        }
    });
    drop(tx);
    let via_channel: u64 = rx.iter().sum();
    let t_chan = t0.elapsed();
    let t0 = Instant::now();
    let at = AtomicU64::new(0);
    thread::scope(|s| {
        for &(a, b) in &slices {
            let at = &at;
            s.spawn(move || {
                (a..=b).for_each(|i| {
                    at.fetch_add(i * i % 1000, Relaxed); // atomic per item
                })
            });
        }
    });
    let t_atomic = t0.elapsed();
    assert_eq!(*m.lock().unwrap(), expect);
    assert_eq!(via_channel, expect);
    assert_eq!(at.load(Relaxed), expect);
    println!(
        "   mutex per item {t_mutex:?} | channel of partials {t_chan:?} | atomic per item {t_atomic:?} (printed only)"
    );
}

// ---------------------------------------------------------------------------
// Q18. async tasks vs OS threads
// ---------------------------------------------------------------------------
/// ANSWER: an OS thread = a kernel-scheduled stack (virtual 2 MiB default in Rust, ~10-20 us to create, context switches in the kernel).
/// An async task = a heap-allocated state machine (hundreds of bytes) multiplexed by an executor over a FEW worker threads; waiting costs no thread.
/// So: 100k concurrent idle connections are fine as tasks, impossible as threads. But a task that BLOCKS (CPU loop, `thread::sleep`, blocking
/// IO) freezes its worker and every task queued behind it. CPU-bound work wants threads/rayon (`spawn_blocking` from async).
///
/// REALLY TESTING: the cost model, and the blocking-the-executor trap.
/// WRONG: "async is faster" (it is about scalability of waiting, not CPU speed); calling blocking APIs in `async fn`.
/// FOLLOW-UP: how many threads does tokio use by default? One worker per core (+ a blocking pool up to 512).
fn q18_async_vs_threads() {
    use tokio::runtime::Builder;
    let rt = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_time()
        .build()
        .unwrap();
    let ids = Arc::new(Mutex::new(HashSet::new()));
    rt.block_on(async {
        let hs: Vec<_> = (0..5000)
            .map(|_| {
                let ids = ids.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    ids.lock().unwrap().insert(thread::current().id());
                })
            })
            .collect();
        for h in hs {
            h.await.unwrap();
        }
    });
    assert!(
        ids.lock().unwrap().len() <= 2,
        "5000 tasks ran on at most 2 worker threads"
    );
    let tids = Arc::new(Mutex::new(HashSet::new()));
    let hs: Vec<_> = (0..200)
        .map(|_| {
            let tids = tids.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(1));
                tids.lock().unwrap().insert(thread::current().id());
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(
        tids.lock().unwrap().len(),
        200,
        "200 tasks as threads = 200 OS threads"
    );
    // blocking inside async stalls the whole (single-threaded) executor
    let rt = Builder::new_current_thread().enable_time().build().unwrap();
    let elapsed = rt.block_on(async {
        let t0 = Instant::now();
        let blocker = tokio::spawn(async {
            thread::sleep(Duration::from_millis(40)); // BAD: blocks the only worker
        });
        let quick = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(1)).await;
            Instant::now()
        });
        let done = quick.await.unwrap();
        blocker.await.unwrap();
        done.duration_since(t0)
    });
    assert!(
        elapsed >= Duration::from_millis(35),
        "a 1 ms timer finished only after the blocking task released the worker ({elapsed:?})"
    );
}

// ---------------------------------------------------------------------------
// Q19. cancellation
// ---------------------------------------------------------------------------
/// ANSWER: Rust cannot (safely) kill a thread: no `pthread_cancel`, it would skip destructors and leave locks held. Cancellation is
/// COOPERATIVE: share a token (`AtomicBool` for polling loops; a `Mutex<bool>` + `Condvar` so SLEEPING workers wake immediately), check it at
/// loop heads/between chunks, and make blocking calls interruptible (`recv_timeout`, `wait_timeout`, closing the channel, shutting down the socket).
/// Make cancellation structured: a guard that cancels on Drop so a scope exit stops its children (it asks; join to know they stopped). Async is
/// similar: a dropped future stops at the `.await` where it is suspended, so state must be consistent at every await point (see
/// `structured_concurrency_cancellation`).
///
/// REALLY TESTING: do you know there is no kill, and how to make blocked threads interruptible?
/// WRONG: `AtomicBool` flag + `thread::sleep(10s)` (the flag is not seen for 10 s).
/// FOLLOW-UP: what if the worker is stuck in a blocking syscall? Signal it, or close the fd from another thread.
#[derive(Clone)]
struct CancelToken(Arc<(Mutex<bool>, Condvar)>);
impl CancelToken {
    fn new() -> Self {
        CancelToken(Arc::new((Mutex::new(false), Condvar::new())))
    }
    fn cancel(&self) {
        *self.0.0.lock().unwrap() = true;
        self.0.1.notify_all();
    }
    fn is_cancelled(&self) -> bool {
        *self.0.0.lock().unwrap()
    }
    /// Sleep up to `d`, returning early (true) if cancelled.
    fn sleep(&self, d: Duration) -> bool {
        let (g, _) = self
            .0
            .1
            .wait_timeout_while(self.0.0.lock().unwrap(), d, |c| !*c)
            .unwrap();
        *g
    }
}
struct CancelOnDrop(CancelToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
fn q19_cancellation() {
    let tok = CancelToken::new();
    let t = {
        let tok = tok.clone();
        thread::spawn(move || {
            let mut chunks = 0;
            while !tok.sleep(Duration::from_secs(10)) {
                chunks += 1; // would run every 10 s; we cancel long before
            }
            chunks
        })
    };
    let t0 = Instant::now();
    thread::sleep(Duration::from_millis(20));
    tok.cancel();
    assert_eq!(t.join().unwrap(), 0);
    assert!(
        t0.elapsed() < Duration::from_secs(2),
        "a condvar-based sleep wakes at once; an AtomicBool + sleep(10s) would not"
    );
    // scope-exit cancels children
    let tok = CancelToken::new();
    let (running_tx, running_rx) = mpsc::channel();
    let h = {
        let tok = tok.clone();
        thread::spawn(move || {
            let mut n = 0u64;
            while !tok.is_cancelled() {
                n += 1;
                if n == 1 {
                    running_tx.send(()).unwrap();
                }
                thread::yield_now();
            }
            n
        })
    };
    {
        let _guard = CancelOnDrop(tok.clone());
        running_rx.recv().unwrap(); // the child has done some work, however late it was scheduled
    } // guard dropped here
    assert!(h.join().unwrap() > 0 && tok.is_cancelled());
}

// ---------------------------------------------------------------------------
// Q20. Send / Sync
// ---------------------------------------------------------------------------
/// ANSWER: `T: Send` = ownership of a T may move to another thread. `T: Sync` = `&T` may be shared across threads (`T: Sync <=> &T: Send`).
/// Both are AUTO traits derived from fields; raw pointers, `Rc` and `Cell`-family types opt out; `unsafe impl` is a promise YOU audit.
/// Table (verified below): `Rc<T>` neither; `Cell/RefCell<T>`: Send (if T) but NOT Sync (unsynchronised interior mutability);
/// `Mutex<T>`: Send+Sync if `T: Send` (the lock restores safety); `MutexGuard`: NOT Send (some OS locks must be released by the locking thread)
/// but Sync; `Arc<T>`: Send+Sync only if `T: Send + Sync` (so `Arc<RefCell<T>>` is NOT Send: the classic error); `*const T`: neither.
/// The compiler errors you see are E0277: "`Rc<..>` cannot be sent between threads safely" when the closure given to `thread::spawn` captures such a type:
///
///     // does NOT compile:
///     let rc = std::rc::Rc::new(1);
///     std::thread::spawn(move || println!("{rc}"));   // E0277: Rc<i32> is not Send
///     // fix: Arc, or move plain data in
///     // does NOT compile:
///     let c = std::sync::Arc::new(std::cell::RefCell::new(1));
///     std::thread::spawn(move || *c.borrow_mut() += 1); // E0277: RefCell<i32> is not Sync, so Arc<RefCell<..>> is not Send
///     // fix: Arc<Mutex<..>>
///
/// REALLY TESTING: derive the answers from the two rules instead of memorising.
/// WRONG: "Mutex makes anything Send" (`Mutex<Rc<T>>` is still not Send); "Sync means thread-safe to mutate".
/// FOLLOW-UP: write a wrapper around a raw pointer and justify `unsafe impl Send`: show the invariant (unique ownership) in a SAFETY comment.
struct P<T: ?Sized>(PhantomData<T>);
trait Fallback {
    fn send(&self) -> bool {
        false
    }
    fn sync(&self) -> bool {
        false
    }
}
impl<T: ?Sized> Fallback for P<T> {}
impl<T: ?Sized + Send> P<T> {
    fn send(&self) -> bool {
        true
    }
}
impl<T: ?Sized + Sync> P<T> {
    fn sync(&self) -> bool {
        true
    }
}
macro_rules! is_send {
    ($t:ty) => {
        P::<$t>(PhantomData).send()
    };
}
macro_rules! is_sync {
    ($t:ty) => {
        P::<$t>(PhantomData).sync()
    };
}
struct RawBox(*mut u8);
// SAFETY (demonstration): RawBox uniquely owns its pointee and exposes no aliasing, so moving it between threads is sound.
unsafe impl Send for RawBox {}
fn q20_send_sync() {
    assert!(is_send!(i32) && is_sync!(i32));
    assert!(!is_send!(Rc<i32>) && !is_sync!(Rc<i32>));
    assert!(is_send!(Cell<i32>) && !is_sync!(Cell<i32>));
    assert!(is_send!(RefCell<i32>) && !is_sync!(RefCell<i32>));
    assert!(
        is_send!(Mutex<Cell<i32>>) && is_sync!(Mutex<Cell<i32>>),
        "Mutex<T> is Sync if T: Send"
    );
    assert!(
        !is_send!(Mutex<Rc<i32>>),
        "a Mutex does not launder a non-Send payload"
    );
    assert!(is_send!(Arc<Mutex<i32>>) && is_sync!(Arc<Mutex<i32>>));
    assert!(
        !is_send!(Arc<RefCell<i32>>),
        "Arc<T> is Send only if T: Send + Sync"
    );
    assert!(!is_send!(MutexGuard<'static, i32>) && is_sync!(MutexGuard<'static, i32>));
    assert!(!is_send!(*const u8) && !is_sync!(*const u8));
    assert!(
        is_send!(RawBox) && !is_sync!(RawBox),
        "unsafe impl Send only; Sync stays off"
    );
    assert!(!is_send!(&Cell<i32>), "&T is Send iff T: Sync");
    assert!(is_send!(&Mutex<i32>));
    assert!(is_send!(AtomicU64) && is_sync!(AtomicU64));
    assert!(
        is_send!(dyn Fn() + Send) && !is_send!(dyn Fn()),
        "trait objects carry Send explicitly"
    );
}

// ---------------------------------------------------------------------------
// Q21. spurious and lost wakeups
// ---------------------------------------------------------------------------
/// ANSWER: SPURIOUS wakeup: `Condvar::wait` may return without anyone notifying (allowed by POSIX/Windows, and implementations need it).
/// LOST wakeup: a `notify` sent when nobody is waiting is FORGOTTEN (condvars have no memory, unlike semaphores/channels). The cure for both is
/// the same: the PREDICATE lives in the mutex-protected state, you check it before waiting and re-check after every wake-up
/// (`while !pred { wait }` or `wait_while`). Then "notify before wait" is harmless because the waiter sees the predicate already true.
///
/// REALLY TESTING: the predicate pattern; why the check and the wait must be under the same mutex (atomic release-and-sleep).
/// WRONG: bare `cv.wait(g)` with no predicate; checking the flag, unlocking, then waiting (race window = lost wakeup).
/// FOLLOW-UP: notify_one vs notify_all, and thundering herd.
fn q21_wakeups() {
    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    // notify BEFORE anyone waits: the signal is lost
    pair.1.notify_one();
    *pair.0.lock().unwrap() = true;
    let g = pair.0.lock().unwrap();
    // A wait that does not look at the state sleeps although it is already `true`. (`|_| true` ignores the state and, unlike one
    // `wait_timeout` call, goes back to sleep after a spurious wakeup, so the timeout is the only way out.)
    let (_g, res) = pair
        .1
        .wait_timeout_while(g, Duration::from_millis(30), |_| true)
        .unwrap();
    assert!(
        res.timed_out(),
        "a bare wait after a missed notify sleeps until the timeout (forever without one)"
    );
    drop(_g);
    // the predicate form returns immediately because it checks the state first
    let mut predicate_calls = 0;
    let g = pair
        .1
        .wait_while(pair.0.lock().unwrap(), |ready| {
            predicate_calls += 1;
            !*ready
        })
        .unwrap();
    drop(g);
    assert_eq!(
        predicate_calls, 1,
        "one check and no sleep: every wake-up would have re-run the predicate"
    );
    // stress: many waiters, notifications racing with waits, none may be lost
    let st = Arc::new((Mutex::new(0u32), Condvar::new()));
    let hs: Vec<_> = (1..=8u32)
        .map(|k| {
            let st = st.clone();
            thread::spawn(move || {
                let g = st.1.wait_while(st.0.lock().unwrap(), |n| *n < k).unwrap();
                assert!(*g >= k);
            })
        })
        .collect();
    for _ in 0..8 {
        *st.0.lock().unwrap() += 1;
        st.1.notify_all();
        thread::yield_now();
    }
    hs.into_iter().for_each(|h| h.join().unwrap());
}

// ---------------------------------------------------------------------------
// Q22. singleflight
// ---------------------------------------------------------------------------
/// ANSWER: when a hot cache key expires, N concurrent requests all miss and hit the database at once (cache STAMPEDE / thundering herd).
/// Singleflight: the first caller for a key becomes the LEADER and runs the loader OUTSIDE the map lock; later callers for the same key register as
/// followers and wait on the leader's slot; the leader publishes the value, removes the in-flight entry (so a later call reloads) and wakes everyone.
/// Add negative caching, jittered TTLs, or "serve stale while revalidating" for the rest of the stampede toolbox. Go's `x/sync/singleflight` is this.
///
/// REALLY TESTING: lock scope (never hold the map lock during the slow load), cleanup of the in-flight entry, error propagation.
/// WRONG: holding one global lock during the load (serialises unrelated keys); forgetting to remove the entry (the value never refreshes).
/// FOLLOW-UP: what if the leader panics? Followers must not wait forever. Here a drop guard marks the flight as abandoned and removes the
/// entry; each waiting follower then tries again with its own loader (one of them leads). Returning the error to all followers is the
/// other choice. A loader that never returns still blocks them all: real code adds a timeout.
enum Flight<V> {
    Loading,
    Done(V),
    Abandoned, // the leader panicked
}
type Slot<V> = Arc<(Mutex<Flight<V>>, Condvar)>;
/// Held by the leader. Dropping it ends the flight, whether the load returned or panicked.
struct Leading<'a, V> {
    group: &'a SingleFlight<V>,
    key: &'a str,
    slot: Slot<V>,
}
impl<V> Drop for Leading<'_, V> {
    fn drop(&mut self) {
        let mut state = self.slot.0.lock().unwrap();
        if matches!(*state, Flight::Loading) {
            *state = Flight::Abandoned;
        }
        drop(state);
        self.group.inflight.lock().unwrap().remove(self.key); // later calls start a fresh load
        self.slot.1.notify_all();
    }
}
struct SingleFlight<V> {
    inflight: Mutex<HashMap<String, Slot<V>>>,
    followers: AtomicUsize,
}
impl<V: Clone> SingleFlight<V> {
    fn new() -> Self {
        SingleFlight {
            inflight: Mutex::new(HashMap::new()),
            followers: AtomicUsize::new(0),
        }
    }
    fn call(&self, key: &str, load: impl FnOnce() -> V) -> V {
        loop {
            let (slot, leader) = {
                let mut m = self.inflight.lock().unwrap();
                match m.get(key) {
                    Some(s) => (s.clone(), false),
                    None => {
                        let s: Slot<V> = Arc::new((Mutex::new(Flight::Loading), Condvar::new()));
                        m.insert(key.to_string(), s.clone());
                        (s, true)
                    }
                }
            };
            if leader {
                let flight = Leading {
                    group: self,
                    key,
                    slot,
                };
                let v = load(); // slow work with NO lock held
                *flight.slot.0.lock().unwrap() = Flight::Done(v.clone());
                return v; // dropping `flight` removes the entry and wakes the followers
            }
            self.followers.fetch_add(1, SeqCst);
            let state = slot
                .1
                .wait_while(slot.0.lock().unwrap(), |s| matches!(s, Flight::Loading))
                .unwrap();
            if let Flight::Done(v) = &*state {
                return v.clone();
            }
        }
    }
}
fn q22_singleflight() {
    let sf = Arc::new(SingleFlight::<u64>::new());
    let loads = Arc::new(AtomicUsize::new(0));
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let (sf, loads) = (sf.clone(), loads.clone());
            thread::spawn(move || {
                sf.call("hot", || {
                    loads.fetch_add(1, SeqCst);
                    while sf.followers.load(SeqCst) < 7 {
                        thread::yield_now(); // keep the load "slow" until all 7 others have joined: deterministic collapse
                    }
                    99
                })
            })
        })
        .collect();
    assert!(hs.into_iter().all(|h| h.join().unwrap() == 99));
    assert_eq!(
        loads.load(SeqCst),
        1,
        "8 concurrent requests collapsed into ONE load"
    );
    assert_eq!(
        sf.call("hot", || 100),
        100,
        "after completion a new call loads again (no caching here)"
    );
    assert!(sf.inflight.lock().unwrap().is_empty());
    // a leader that panics while a follower waits: the follower must finish with its own load, not hang
    let sf = Arc::new(SingleFlight::<u64>::new());
    let (leading_tx, leading_rx) = mpsc::channel();
    let leader = {
        let sf = sf.clone();
        thread::spawn(move || {
            sf.call("bad", || {
                leading_tx.send(()).unwrap();
                while sf.followers.load(SeqCst) < 1 {
                    thread::yield_now();
                }
                std::panic::resume_unwind(Box::new("the load failed")) // a panic without the printed message
            })
        })
    };
    leading_rx.recv().unwrap();
    let (got_tx, got_rx) = mpsc::channel();
    let follower = {
        let sf = sf.clone();
        thread::spawn(move || got_tx.send(sf.call("bad", || 5)).unwrap())
    };
    assert!(leader.join().is_err());
    // the time limit only turns a hang into a failure
    assert_eq!(
        got_rx.recv_timeout(Duration::from_secs(20)),
        Ok(5),
        "the follower is stuck behind a leader that is gone"
    );
    follower.join().unwrap();
    assert!(sf.inflight.lock().unwrap().is_empty());
}

fn main() {
    let questions: [(&str, fn()); 22] = [
        ("Q1  bounded blocking queue", q1_bounded_queue),
        ("Q2  token bucket", q2_token_bucket),
        ("Q3  LRU + sharded", q3_lru),
        ("Q4  writer-preferring RwLock", q4_rwlock),
        ("Q5  dining philosophers", q5_philosophers),
        ("Q6  FooBar", q6_foobar),
        ("Q7  ZeroEvenOdd", q7_zero_even_odd),
        ("Q8  barrier", q8_barrier),
        ("Q9  semaphore", q9_semaphore),
        ("Q10 lazy init", q10_lazy),
        ("Q11 lock-order graph", q11_lock_order),
        ("Q12 thread pool + shutdown", q12_pool),
        ("Q13 parallel merge sort", q13_merge_sort),
        ("Q14 word count", q14_word_count),
        ("Q15 atomic orderings", q15_orderings),
        ("Q16 ABA", q16_aba),
        ("Q17 mutex vs channel vs atomic", q17_three_ways),
        ("Q18 async vs threads", q18_async_vs_threads),
        ("Q19 cancellation", q19_cancellation),
        ("Q20 Send/Sync", q20_send_sync),
        ("Q21 spurious/lost wakeups", q21_wakeups),
        ("Q22 singleflight", q22_singleflight),
    ];
    for (name, f) in questions {
        f();
        println!("{name}: ok");
    }
    println!("all {} concurrency questions verified", questions.len());
}
