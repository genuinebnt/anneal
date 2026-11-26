//! RECIPE 1049 (std-sync): atomic types by method and by use case, with the ordering each use needs and why.
//! LEVEL: advanced
//! Use in: metrics, stop flags, id generators, once-flags, small lock-free structures.   Run: cargo run --release --example std_atomics_ordering_use_cases
//! COMMON USES: request counters, high-water marks, cooperative cancellation, "first caller wins", state bit sets, handing a batch of nodes to a consumer without a lock.
//! METHODS: AtomicBool, AtomicUsize, AtomicU64, AtomicI64, AtomicU8, AtomicPtr, new, load, store, swap, compare_exchange, compare_exchange_weak,
//! fetch_add, fetch_sub, fetch_and, fetch_or, fetch_xor, fetch_nand, fetch_not, fetch_max, fetch_min, try_update, update, fetch_update,
//! get_mut, into_inner, as_ptr, from_ptr, from_mut, from_mut_slice, std::sync::atomic::fence, std::sync::atomic::compiler_fence, std::hint::spin_loop
//! DOCS: https://doc.rust-lang.org/std/sync/atomic/index.html
//! DEFINED IN: core: all of it (`core::sync::atomic`, `core::hint::spin_loop`); std re-exports. `AtomicU64` and friends exist only where the target
//!   supports that width (`cfg(target_has_atomic = "64")`). alloc: `Box`, used by the pointer examples.
//! NOT COVERED (and why): 128-bit and other nightly-only atomics; `AtomicPtr::fetch_ptr_add`/`fetch_byte_add` and the tag-bit ops (specialist; see the ptr docs);
//! safe memory reclamation (hazard pointers, epochs: see `epoch_hazard_pointer_reclamation`); ABA in a popping stack (see `lockfree_stack_aba`);
//! `futex`-style waiting (not in std). Litmus tests that show reordering live in `memory_ordering_litmus`; seqlocks in `seqlock_and_priority_inversion`.
//! WHAT THE ASSERTIONS PROVE: exact totals and invariants on THIS run and THIS machine. A passing test does NOT prove an ordering is strong enough:
//! the orderings below rest on the happens-before argument written next to them. Miri
//! (rustc 1.100.0-nightly (5a2be9f5f 2026-09-06), 2026-11-26) reported nothing on the default run, seeds 1-6 and Tree Borrows, with reduced loop counts;
//! that samples some schedules, it does not explore all of them. loom was not run.
//! PORTABILITY: `from_mut` / `from_mut_slice` exist only on targets where the atomic's alignment equals the integer's size.
//! VERSION NOTE: in the 1.98.1 source `fetch_update` is marked deprecated since 1.99.0 ("renamed to `try_update`"); `try_update` and `update` are 1.95.
//!
//! CHEAT SHEET (I want to ... -> operation and ordering)
//!   count events; read the total after joining   -> `fetch_add(1, Relaxed)`: atomicity is enough, `join` provides the ordering
//!   unique ids                                    -> `fetch_add(1, Relaxed)` (it WRAPS on overflow)
//!   high / low water mark                         -> `fetch_max` / `fetch_min`, Relaxed
//!   publish data, then flag it                    -> writer: write data; `store(true, Release)`. reader: `load(Acquire)`; then read data
//!   stop flag that carries no data                -> Relaxed is enough; use Release/Acquire if the reader reads what the setter wrote before
//!   "only one caller wins"                        -> `swap(true, AcqRel)` or `compare_exchange(false, true, AcqRel, Acquire)`
//!   lock / unlock                                 -> acquire with `Acquire`, release with `Release`
//!   arbitrary read-modify-write                   -> `try_update` (may refuse) / `update`; the closure can run more than once
//!   set / clear / toggle bits                     -> `fetch_or(mask)` / `fetch_and(!mask)` / `fetch_xor(mask)`
//!   two threads each write one flag then read the other -> `SeqCst` (Release/Acquire does NOT order a store against a later load of another location)
//!   unsure                                        -> use a `Mutex`. If it must be atomic, SeqCst is the easiest to reason about; weaken only with an argument
//!
//! FACTS: `load` panics with Release/AcqRel; `store` panics with Acquire/AcqRel; a compare_exchange FAILURE ordering cannot be Release/AcqRel.
//! `fetch_add`/`fetch_sub` wrap around, even in debug builds. `compare_exchange_weak` may fail spuriously: only use it in a retry loop.
//! TRY NEXT:
//!   - [S] In section 5 change the unlock to `Relaxed`. The test will probably still pass. Write down the interleaving the language now allows.
//!   - [S] Replace `compare_exchange_weak` in `SpinLock::lock` by `swap(true, Acquire)`. Is it still correct? What differs for cache traffic?
//!   - [M] Add `try_lock` returning `Option<Guard>` to `TicketLock`, without taking a ticket you cannot use.
//!   - [M] Extend `Stats` to a consistent snapshot of (count, sum) using one `AtomicU64` that packs both, and state the limits.
//!   - [A] PROJECT: run `SpinLock` and `Stack` under loom or Miri (nightly) with 2 threads and 2 operations each, and weaken orderings until a tool objects.
//! Read: Mara Bos, Rust Atomics and Locks https://marabos.nl/atomics/memory-ordering.html ; https://doc.rust-lang.org/nomicon/atomics.html ; https://doc.rust-lang.org/std/sync/atomic/fn.fence.html
#![allow(dead_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::panic::catch_unwind;
use std::ptr;
use std::sync::atomic::{
    AtomicBool, AtomicI64, AtomicPtr, AtomicU8, AtomicU64, AtomicUsize, Ordering, compiler_fence,
    fence,
};
use std::thread;

use Ordering::{AcqRel, Acquire, Relaxed, Release, SeqCst};

const THREADS: usize = 8;
const ROUNDS: usize = if cfg!(miri) { 50 } else { 20_000 };
/// Repetition counts are divided by this under Miri, which interprets the program and is far slower.
const SCALE: usize = if cfg!(miri) { 50 } else { 1 };

fn spawn_n(n: usize, f: impl Fn(usize) + Sync) {
    thread::scope(|s| {
        for t in 0..n {
            let f = &f;
            s.spawn(move || f(t));
        }
    });
}

/// Spin briefly, then give up the time slice: pure spinning wastes whole quanta when there are more threads than cores.
fn backoff(tries: &mut u32) {
    *tries += 1;
    if *tries < 64 {
        std::hint::spin_loop();
    } else {
        thread::yield_now();
    }
}

// ------------------------------------------------------------ 1. The method tour, single threaded
fn s01_methods() {
    let a = AtomicI64::new(10);
    // every fetch_* returns the PREVIOUS value
    assert_eq!(a.fetch_add(5, Relaxed), 10);
    assert_eq!(a.fetch_sub(20, Relaxed), 15);
    assert_eq!(a.load(Relaxed), -5);
    assert_eq!(a.swap(100, Relaxed), -5);
    a.store(7, Relaxed);
    assert_eq!(
        (
            a.fetch_max(3, Relaxed),
            a.fetch_max(9, Relaxed),
            a.fetch_min(-1, Relaxed),
            a.load(Relaxed)
        ),
        (7, 7, 9, -1)
    );

    let bits = AtomicU8::new(0b1100);
    assert_eq!(bits.fetch_and(0b1010, Relaxed), 0b1100); // now 1000
    assert_eq!(bits.fetch_or(0b0001, Relaxed), 0b1000); // now 1001
    assert_eq!(bits.fetch_xor(0b1111, Relaxed), 0b1001); // now 0110
    assert_eq!(bits.fetch_nand(0b0100, Relaxed), 0b0110); // now !(0110 & 0100)
    assert_eq!(bits.load(Relaxed), !0b0100);
    let flag = AtomicBool::new(false);
    assert!(!flag.fetch_not(Relaxed)); // 1.81
    assert!(
        flag.fetch_and(true, Relaxed)
            && flag.fetch_xor(true, Relaxed)
            && !flag.fetch_or(true, Relaxed)
    );

    // compare_exchange: Ok(previous) on success, Err(actual) on failure
    assert_eq!(a.compare_exchange(-1, 50, AcqRel, Acquire), Ok(-1));
    assert_eq!(a.compare_exchange(-1, 60, AcqRel, Acquire), Err(50));

    // wrapping, with no panic even in debug builds
    let w = AtomicU8::new(255);
    assert_eq!((w.fetch_add(1, Relaxed), w.load(Relaxed)), (255, 0));
    assert_eq!((w.fetch_sub(1, Relaxed), w.load(Relaxed)), (0, 255));

    // try_update: Ok(previous) if the closure returned Some and the value was stored; Err(current) if it returned None
    let c = AtomicUsize::new(2);
    assert_eq!(c.try_update(AcqRel, Acquire, |x| x.checked_sub(2)), Ok(2));
    assert_eq!(c.try_update(AcqRel, Acquire, |x| x.checked_sub(1)), Err(0));
    assert_eq!(
        (c.update(AcqRel, Acquire, |x| x * 2 + 1), c.load(Relaxed)),
        (0, 1)
    );
    #[allow(deprecated)] // the older name of try_update: needed only for toolchains before 1.95
    {
        assert_eq!(c.fetch_update(AcqRel, Acquire, |x| Some(x + 1)), Ok(1));
    }

    // with exclusive access no atomic operation is needed
    let mut a = AtomicU64::new(1);
    *a.get_mut() += 1;
    assert_eq!(a.into_inner(), 2);
    let mut plain = [1usize, 2, 3];
    {
        let atomics = AtomicUsize::from_mut_slice(&mut plain); // 1.98: share a plain buffer atomically for a while
        spawn_n(4, |_| {
            atomics.iter().for_each(|x| {
                let _ = x.fetch_add(10, Relaxed);
            })
        });
    }
    assert_eq!(plain, [41, 42, 43]);
    let mut one = 5u64;
    AtomicU64::from_mut(&mut one).store(6, Relaxed);
    assert_eq!(one, 6);

    let raw = AtomicUsize::new(3);
    let p: *mut usize = raw.as_ptr();
    // SAFETY: `p` comes from a live AtomicUsize (valid, aligned for AtomicUsize) and every access through the returned
    // reference is atomic; no non-atomic access happens while it exists.
    let view = unsafe { AtomicUsize::from_ptr(p) };
    view.store(4, Relaxed);
    assert_eq!(raw.load(Relaxed), 4);

    // invalid orderings are runtime panics (rustc also rejects them when written as constants)
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let hide = std::hint::black_box::<Ordering>;
    assert!(catch_unwind(move || AtomicUsize::new(0).load(hide(Release))).is_err());
    assert!(catch_unwind(move || AtomicUsize::new(0).store(1, hide(Acquire))).is_err());
    assert!(
        catch_unwind(move || AtomicUsize::new(0).compare_exchange(0, 1, SeqCst, hide(Release)))
            .is_err()
    );
    std::panic::set_hook(old);
    println!("1. method tour ok");
}

// ------------------------------------------------------------ 2. Counters and statistics: Relaxed
#[derive(Default)]
struct Stats {
    count: AtomicU64,
    sum: AtomicU64,
    max: AtomicU64,
    min_inv: AtomicU64, // stores !min so that Default (0) means "no sample yet"
}
impl Stats {
    fn record(&self, v: u64) {
        self.count.fetch_add(1, Relaxed);
        self.sum.fetch_add(v, Relaxed);
        self.max.fetch_max(v, Relaxed);
        self.min_inv.fetch_max(!v, Relaxed);
    }
}

fn s02_counters() {
    let s = Stats::default();
    let next_id = AtomicU64::new(0);
    let id_sum = AtomicU64::new(0);
    spawn_n(THREADS, |t| {
        for i in 0..ROUNDS {
            s.record((t * ROUNDS + i) as u64);
            id_sum.fetch_add(next_id.fetch_add(1, Relaxed), Relaxed);
        }
    });
    // exact, although Relaxed: each RMW is indivisible, and the scope's join makes all writes visible here
    let n = (THREADS * ROUNDS) as u64;
    assert_eq!(s.count.load(Relaxed), n);
    assert_eq!(s.sum.load(Relaxed), n * (n - 1) / 2);
    assert_eq!((s.max.load(Relaxed), !s.min_inv.load(Relaxed)), (n - 1, 0));
    assert_eq!(
        (next_id.load(Relaxed), id_sum.load(Relaxed)),
        (n, n * (n - 1) / 2),
        "ids 0..n each handed out exactly once"
    );
    // CAVEAT: while writers run, a reader may see a new `count` with an old `sum`. Separate atomics are never one snapshot.

    // LOST UPDATE: load-then-store is two operations; fetch_add is one. This interleaving is written out by hand.
    let c = AtomicU64::new(0);
    let (a_read, b_read) = (c.load(SeqCst), c.load(SeqCst));
    c.store(a_read + 1, SeqCst);
    c.store(b_read + 1, SeqCst);
    assert_eq!(
        c.load(SeqCst),
        1,
        "two increments, one lost, even with SeqCst everywhere"
    );
    println!("2. counters / stats ok");
}

// ------------------------------------------------------------ 3. Gauges with a limit, and bit sets
/// Admit at most `limit` holders. The check and the increment must be ONE operation.
struct Gate {
    inside: AtomicUsize,
    limit: usize,
}
struct Permit<'a>(&'a Gate);
impl Gate {
    fn enter(&self) -> Option<Permit<'_>> {
        self.inside
            .try_update(Acquire, Relaxed, |n| (n < self.limit).then(|| n + 1))
            .ok()?;
        Some(Permit(self))
    }
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.inside.fetch_sub(1, Release); // pairs with the Acquire in enter
    }
}

const READY: u8 = 1;
const DRAINING: u8 = 2;
const FAILED: u8 = 4;

fn s03_gate_bits() {
    let gate = Gate {
        inside: AtomicUsize::new(0),
        limit: 3,
    };
    let held = AtomicUsize::new(0);
    let admitted = AtomicUsize::new(0);
    spawn_n(THREADS, |_| {
        for _ in 0..2_000 / SCALE {
            if let Some(_p) = gate.enter() {
                assert!(held.fetch_add(1, SeqCst) < 3, "limit exceeded");
                admitted.fetch_add(1, Relaxed);
                thread::yield_now();
                held.fetch_sub(1, SeqCst);
            }
        }
    });
    assert_eq!(gate.inside.load(Relaxed), 0);
    assert!(admitted.load(Relaxed) > 0);
    let (a, b, c) = (gate.enter(), gate.enter(), gate.enter());
    assert!(a.is_some() && b.is_some() && c.is_some() && gate.enter().is_none());
    drop(a);
    assert!(gate.enter().is_some());

    let state = AtomicU8::new(0);
    state.fetch_or(READY, AcqRel);
    // "start draining" must succeed for exactly one caller: fetch_or reports whether the bit was already set
    let winners = AtomicUsize::new(0);
    spawn_n(THREADS, |_| {
        if state.fetch_or(DRAINING, AcqRel) & DRAINING == 0 {
            winners.fetch_add(1, Relaxed);
        }
    });
    assert_eq!(winners.load(Relaxed), 1);
    assert_eq!(state.fetch_and(!READY, AcqRel), READY | DRAINING);
    assert_eq!(state.load(Acquire), DRAINING);
    println!("3. gate / bit flags ok");
}

// ------------------------------------------------------------ 4. Flags: publish, stop, once, and the SeqCst case
/// A cell written once and then read by many. `state`: 0 empty, 1 being written, 2 ready.
struct Publish<T> {
    state: AtomicU8,
    value: UnsafeCell<Option<T>>,
}
// SAFETY: `value` is written only by the single thread that won the 0 -> 1 transition, and read only after observing 2
// with Acquire, which happens-after the Release store that followed the write. T: Send + Sync covers moving the value
// in from one thread and sharing &T with others.
unsafe impl<T: Send + Sync> Sync for Publish<T> {}
impl<T> Publish<T> {
    const fn new() -> Self {
        Publish {
            state: AtomicU8::new(0),
            value: UnsafeCell::new(None),
        }
    }
    fn set(&self, v: T) -> Result<(), T> {
        if self.state.compare_exchange(0, 1, Relaxed, Relaxed).is_err() {
            return Err(v); // Relaxed: the loser reads nothing that depends on the winner
        }
        // SAFETY: winning the CAS gives this thread exclusive access; no reader touches `value` before state == 2.
        unsafe { *self.value.get() = Some(v) };
        self.state.store(2, Release); // publishes the write above
        Ok(())
    }
    fn get(&self) -> Option<&T> {
        if self.state.load(Acquire) != 2 {
            return None;
        }
        // SAFETY: state == 2 was read with Acquire, so the write happened-before; `value` is never written again.
        unsafe { (*self.value.get()).as_ref() }
    }
}

fn s04_flags() {
    for round in 0..300 / SCALE {
        let cell: Publish<Vec<usize>> = Publish::new();
        let wins = AtomicUsize::new(0);
        spawn_n(4, |t| {
            if t < 2 {
                if cell.set(vec![round; 32]).is_ok() {
                    wins.fetch_add(1, Relaxed);
                }
            } else {
                let mut tries = 0;
                let v = loop {
                    if let Some(v) = cell.get() {
                        break v;
                    }
                    backoff(&mut tries);
                };
                assert!(
                    v.len() == 32 && v.iter().all(|&x| x == round),
                    "never a half-written value"
                );
            }
        });
        assert_eq!(wins.load(Relaxed), 1);
    }
    // In real code this type already exists: std::sync::OnceLock.

    // stop flag: workers poll it. It publishes nothing else, so Relaxed suffices.
    let stop = AtomicBool::new(false);
    let progress = AtomicUsize::new(0);
    thread::scope(|s| {
        let h = s.spawn(|| {
            let mut n = 0u64;
            while !stop.load(Relaxed) {
                n += 1;
                progress.store(n as usize, Relaxed);
                thread::yield_now();
            }
            n
        });
        while progress.load(Relaxed) < 10 {
            thread::yield_now();
        }
        stop.store(true, Relaxed);
        assert!(h.join().unwrap() >= 10);
    });

    // The SeqCst case (store buffering): each side raises its flag, then looks at the other's. With SeqCst on all four
    // accesses at least one side MUST see the other. With Release/Acquire both may read false.
    for _ in 0..500 / SCALE {
        let (x, y) = (AtomicBool::new(false), AtomicBool::new(false));
        let (r1, r2) = thread::scope(|s| {
            let a = s.spawn(|| {
                x.store(true, SeqCst);
                y.load(SeqCst)
            });
            let b = s.spawn(|| {
                y.store(true, SeqCst);
                x.load(SeqCst)
            });
            (a.join().unwrap(), b.join().unwrap())
        });
        assert!(r1 || r2, "forbidden outcome under SeqCst");
    }
    println!("4. publish / stop / seqcst ok");
}

// ------------------------------------------------------------ 5. Locks built from atomics (teaching code)
/// Use `std::sync::Mutex` in real code: it sleeps instead of burning CPU and handles poisoning.
struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}
// SAFETY: the `locked` protocol gives at most one thread access to `data` at a time, so sharing the lock only ever
// moves exclusive access between threads, which needs T: Send.
unsafe impl<T: Send> Sync for SpinLock<T> {}
struct SpinGuard<'a, T> {
    lock: &'a SpinLock<T>,
}
// SAFETY: sharing &SpinGuard gives out &T, so it must require T: Sync (the auto impl would only follow &SpinLock<T>).
unsafe impl<T: Sync> Sync for SpinGuard<'_, T> {}
impl<T> SpinLock<T> {
    const fn new(v: T) -> Self {
        SpinLock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(v),
        }
    }
    fn lock(&self) -> SpinGuard<'_, T> {
        let mut tries = 0;
        // Acquire on success pairs with the Release in drop: we see everything the previous holder wrote.
        while self
            .locked
            .compare_exchange_weak(false, true, Acquire, Relaxed)
            .is_err()
        {
            // test-and-test-and-set: wait on a plain load, which does not keep stealing the cache line
            while self.locked.load(Relaxed) {
                backoff(&mut tries);
            }
        }
        SpinGuard { lock: self }
    }
}
impl<T> Deref for SpinGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: a guard exists only while this thread holds the lock, and the borrow cannot outlive the guard.
        unsafe { &*self.lock.data.get() }
    }
}
impl<T> DerefMut for SpinGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: as above; &mut self ensures this is the only reference obtained through the guard.
        unsafe { &mut *self.lock.data.get() }
    }
}
impl<T> Drop for SpinGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Release);
    }
}

/// FIFO fairness: take a ticket, wait until it is served.
struct TicketLock {
    next: AtomicUsize,
    serving: AtomicUsize,
}
impl TicketLock {
    fn with<R>(&self, f: impl FnOnce(usize) -> R) -> R {
        let ticket = self.next.fetch_add(1, Relaxed);
        let mut tries = 0;
        while self.serving.load(Acquire) != ticket {
            backoff(&mut tries);
        }
        struct Served<'a>(&'a AtomicUsize);
        impl Drop for Served<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, Release); // also runs if `f` panics, so the queue does not stall
            }
        }
        let _served = Served(&self.serving);
        f(ticket)
    }
}

fn s05_locks() {
    let lock = SpinLock::new((0u64, Vec::new()));
    spawn_n(THREADS, |t| {
        for i in 0..2_000 / SCALE {
            let mut g = lock.lock();
            g.0 += 1; // plain, non-atomic updates: correct only because of the lock's Acquire/Release
            if i % (100 / SCALE) == 0 {
                g.1.push(t);
            }
        }
    });
    let g = lock.lock();
    assert_eq!(
        (g.0, g.1.len()),
        ((THREADS * (2_000 / SCALE)) as u64, THREADS * 20)
    );
    drop(g);

    let ticket = TicketLock {
        next: AtomicUsize::new(0),
        serving: AtomicUsize::new(0),
    };
    let order = SpinLock::new(Vec::new());
    spawn_n(4, |_| {
        for _ in 0..200 / SCALE {
            ticket.with(|n| order.lock().push(n));
        }
    });
    assert_eq!(
        *order.lock(),
        (0..4 * (200 / SCALE)).collect::<Vec<_>>(),
        "served strictly in ticket order"
    );
    println!("5. spinlock / ticket lock ok");
}

// ------------------------------------------------------------ 6. AtomicPtr: push-only stack, drained all at once
struct Node<T> {
    value: T,
    next: *mut Node<T>,
}
/// Many producers push; `take_all` detaches the whole list. There is deliberately NO single-node pop: popping needs
/// safe memory reclamation (and defeating ABA), which is the hard part of lock-free structures.
struct Stack<T> {
    head: AtomicPtr<Node<T>>,
    // AtomicPtr is Send + Sync for ANY T, which alone would let a Stack<Rc<_>> cross threads. This marker removes both
    // auto traits, so that the explicit impls below can add them back with the right bound.
    _owns: PhantomData<*mut Node<T>>,
}
// SAFETY: the stack owns its values. Moving the stack, or sharing &Stack (push moves a T in, take_all moves Ts out, and
// no &T is ever handed out), only transfers ownership of T between threads, which is exactly what T: Send permits.
unsafe impl<T: Send> Send for Stack<T> {}
// SAFETY: as above.
unsafe impl<T: Send> Sync for Stack<T> {}
impl<T> Stack<T> {
    const fn new() -> Self {
        Stack {
            head: AtomicPtr::new(ptr::null_mut()),
            _owns: PhantomData,
        }
    }
    fn push(&self, value: T) {
        let node = Box::into_raw(Box::new(Node {
            value,
            next: ptr::null_mut(),
        }));
        let mut head = self.head.load(Relaxed);
        loop {
            // SAFETY: `node` is not yet published, so this thread has exclusive access to it.
            unsafe { (*node).next = head };
            // Release publishes the node's contents to whoever takes the list with Acquire.
            match self
                .head
                .compare_exchange_weak(head, node, Release, Relaxed)
            {
                Ok(_) => return,
                Err(actual) => head = actual, // the failure value saves a reload
            }
        }
    }
    fn take_all(&self) -> Vec<T> {
        let mut p = self.head.swap(ptr::null_mut(), Acquire);
        let mut out = Vec::new();
        while !p.is_null() {
            // SAFETY: the swap made this thread the sole owner of the detached chain. Each pointer came from
            // Box::into_raw in `push`, is visited once, and pushers never dereference a node after publishing it.
            let node = unsafe { Box::from_raw(p) };
            p = node.next;
            out.push(node.value);
        }
        out
    }
}
impl<T> Drop for Stack<T> {
    fn drop(&mut self) {
        drop(self.take_all());
    }
}
fn s06_atomic_ptr() {
    let stack: Stack<(usize, usize)> = Stack::new();
    let received = SpinLock::new(Vec::new());
    let done = AtomicUsize::new(0);
    thread::scope(|s| {
        for t in 0..4 {
            let (stack, done) = (&stack, &done);
            s.spawn(move || {
                (0..5_000 / SCALE).for_each(|i| stack.push((t, i)));
                done.fetch_add(1, Release);
            });
        }
        s.spawn(|| {
            loop {
                let finished = done.load(Acquire) == 4; // read BEFORE draining, or the last pushes could be missed
                let mut batch = stack.take_all();
                batch.reverse(); // LIFO chain -> push order
                received.lock().extend(batch);
                if finished {
                    break;
                }
                thread::yield_now();
            }
        });
    });
    let got = received.lock();
    assert_eq!(got.len(), 4 * (5_000 / SCALE));
    let mut next = [0usize; 4];
    for &(t, i) in got.iter() {
        assert_eq!(i, next[t], "per-producer order is preserved across batches");
        next[t] += 1;
    }
    drop(got);
    // un-taken nodes are freed by Drop
    let token = std::sync::Arc::new(());
    {
        let s = Stack::new();
        (0..10).for_each(|_| s.push(std::sync::Arc::clone(&token)));
        assert_eq!(std::sync::Arc::strong_count(&token), 11);
    }
    assert_eq!(std::sync::Arc::strong_count(&token), 1);
    println!("6. atomicptr stack ok");
}

// ------------------------------------------------------------ 7. Fences
fn s07_fences() {
    // One Release fence can cover several Relaxed stores; the reader pairs it with an Acquire fence after seeing the flag.
    for _ in 0..500 / SCALE {
        let data = [AtomicU64::new(0), AtomicU64::new(0)];
        let ready = AtomicBool::new(false);
        thread::scope(|s| {
            s.spawn(|| {
                data[0].store(1, Relaxed);
                data[1].store(2, Relaxed);
                fence(Release);
                ready.store(true, Relaxed);
            });
            s.spawn(|| {
                let mut tries = 0;
                while !ready.load(Relaxed) {
                    backoff(&mut tries);
                }
                fence(Acquire);
                assert_eq!((data[0].load(Relaxed), data[1].load(Relaxed)), (1, 2));
            });
        });
    }
    // compiler_fence restrains only the COMPILER (same-thread cases such as signal handlers). It emits no CPU barrier
    // and does not synchronise threads.
    compiler_fence(SeqCst);
    let old = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    assert!(
        catch_unwind(|| fence(std::hint::black_box(Relaxed))).is_err(),
        "fence(Relaxed) panics"
    );
    std::panic::set_hook(old);
    println!("7. fences ok");
}

fn main() {
    s01_methods();
    s02_counters();
    s03_gate_bits();
    s04_flags();
    s05_locks();
    s06_atomic_ptr();
    s07_fences();
    println!("all atomics sections passed");
}
