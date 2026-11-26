//! loom: exhaustive model checking of concurrent code. `loom::model(|| ..)` runs the closure
//! over EVERY thread interleaving (and every permitted memory-ordering behaviour) of loom's
//! atomics/threads/cells, within a preemption bound. A passing model is a proof for that
//! scenario; a failing one prints the interleaving. Here loom types are used directly (in-process).
//! To model the REAL library types instead, build with `RUSTFLAGS="--cfg loom"` (see src/sync.rs
//! and the `#[cfg(loom)]` test at the bottom).
//!
//! Limitations: (1) state explosion, so keep models to 2-3 threads and a few operations; bound
//! with `Builder::preemption_bound` or `LOOM_MAX_PREEMPTIONS=3`; (2) only loom's own types are
//! modelled (no std Mutex/channels, no syscalls, no real time); (3) spin loops MUST call
//! `loom::thread::yield_now()`; (4) SeqCst fences and some exotic orderings are approximated;
//! (5) loom tests are slow, so real projects gate them behind `#[cfg(loom)]`.
use std::panic::{AssertUnwindSafe, catch_unwind};

use loom::cell::UnsafeCell;
use loom::sync::Arc;
use loom::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use loom::thread;

/// Runs a model; `true` if loom found a failing execution (panic / data race / causality violation).
fn loom_finds_bug(f: impl Fn() + Sync + Send + 'static) -> bool {
    catch_unwind(AssertUnwindSafe(|| loom::model(f))).is_err()
}

// ---- 1. spinlock: protects a NON-atomic cell, so ordering bugs are data races -------------------

struct SpinLock {
    locked: AtomicBool,
    data: UnsafeCell<usize>,
    lock_order: Ordering,
    unlock_order: Ordering,
}
// SAFETY: access to `data` is serialised by `locked` (if the orderings are right: that is what the test checks).
unsafe impl Sync for SpinLock {}

impl SpinLock {
    fn new(lock_order: Ordering, unlock_order: Ordering) -> Self {
        SpinLock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(0),
            lock_order,
            unlock_order,
        }
    }
    fn increment(&self) {
        while self
            .locked
            .compare_exchange(false, true, self.lock_order, Ordering::Relaxed)
            .is_err()
        {
            thread::yield_now(); // tells loom "this thread cannot progress until another runs"
        }
        // loom::cell::UnsafeCell tracks happens-before: an unsynchronised access panics.
        self.data.with_mut(|p| unsafe { *p += 1 });
        self.locked.store(false, self.unlock_order);
    }
}

fn spin_model(lock: Ordering, unlock: Ordering) {
    let l = Arc::new(SpinLock::new(lock, unlock));
    let l2 = l.clone();
    let t = thread::spawn(move || l2.increment());
    l.increment();
    t.join().unwrap();
    assert_eq!(l.data.with(|p| unsafe { *p }), 2);
}

#[test]
fn spinlock_acquire_release_is_correct() {
    // Acquire on lock, Release on unlock: the critical sections are ordered => exhaustive pass.
    loom::model(|| spin_model(Ordering::Acquire, Ordering::Release));
}

#[test]
fn spinlock_with_relaxed_orderings_is_caught() {
    // Same code, orderings weakened to Relaxed: nothing makes the first thread's write visible
    // to the second, so loom reports a causality violation. Asserting on that keeps CI green.
    assert!(
        loom_finds_bug(|| spin_model(Ordering::Relaxed, Ordering::Relaxed)),
        "loom should reject Relaxed spinlock"
    );
}

// ---- 2. flag handoff (publish data, then raise a flag) ------------------------------------------

struct Handoff {
    ready: AtomicBool,
    value: UnsafeCell<u32>,
}
unsafe impl Sync for Handoff {}

fn handoff_model(store: Ordering, load: Ordering) {
    let h = Arc::new(Handoff {
        ready: AtomicBool::new(false),
        value: UnsafeCell::new(0),
    });
    let p = h.clone();
    let producer = thread::spawn(move || {
        p.value.with_mut(|v| unsafe { *v = 42 });
        p.ready.store(true, store);
    });
    if h.ready.load(load) {
        assert_eq!(h.value.with(|v| unsafe { *v }), 42);
    }
    producer.join().unwrap();
}

#[test]
fn handoff_release_acquire_passes() {
    loom::model(|| handoff_model(Ordering::Release, Ordering::Acquire));
}

#[test]
fn handoff_relaxed_fails() {
    assert!(loom_finds_bug(|| handoff_model(
        Ordering::Relaxed,
        Ordering::Relaxed
    )));
    // Half-fixing it is still wrong: BOTH sides must participate.
    assert!(loom_finds_bug(|| handoff_model(
        Ordering::Release,
        Ordering::Relaxed
    )));
}

// ---- 3. atomicity, not ordering: lost updates ---------------------------------------------------

#[test]
fn relaxed_fetch_add_never_loses_updates() {
    // Relaxed is enough when only the RMW's atomicity matters (this is what IdGen relies on).
    loom::model(|| {
        let c = Arc::new(AtomicUsize::new(0));
        let hs: Vec<_> = (0..2)
            .map(|_| {
                let c = c.clone();
                thread::spawn(move || {
                    c.fetch_add(1, Ordering::Relaxed);
                })
            })
            .collect();
        hs.into_iter().for_each(|h| h.join().unwrap());
        assert_eq!(c.load(Ordering::Relaxed), 2);
    });
}

#[test]
fn load_then_store_loses_updates() {
    // Non-atomic read-modify-write: stronger orderings (even SeqCst) cannot save it. loom finds
    // the interleaving where both threads read 0.
    assert!(loom_finds_bug(|| {
        let c = Arc::new(AtomicUsize::new(0));
        let c2 = c.clone();
        let t = thread::spawn(move || {
            let v = c2.load(Ordering::SeqCst);
            c2.store(v + 1, Ordering::SeqCst);
        });
        let v = c.load(Ordering::SeqCst);
        c.store(v + 1, Ordering::SeqCst);
        t.join().unwrap();
        assert_eq!(c.load(Ordering::SeqCst), 2);
    }));
}

#[test]
fn bounded_model_with_preemption_bound() {
    // Builder is the programmatic twin of LOOM_MAX_PREEMPTIONS: explore only schedules with at
    // most N forced context switches (most real bugs need <= 2-3). Great for 3+ threads.
    let mut b = loom::model::Builder::new();
    b.preemption_bound = Some(2);
    b.check(|| {
        let c = Arc::new(AtomicUsize::new(0));
        let hs: Vec<_> = (0..3)
            .map(|_| {
                let c = c.clone();
                thread::spawn(move || {
                    let mut cur = c.load(Ordering::Relaxed);
                    // a CAS loop instead of load+store: correct under every interleaving
                    while let Err(seen) =
                        c.compare_exchange(cur, cur + 1, Ordering::AcqRel, Ordering::Relaxed)
                    {
                        cur = seen;
                    }
                })
            })
            .collect();
        hs.into_iter().for_each(|h| h.join().unwrap());
        assert_eq!(c.load(Ordering::Acquire), 3);
    });
}

// ---- 4. the REAL library types under loom (only with RUSTFLAGS="--cfg loom") ----------------------
// `cargo test` does not compile this; `RUSTFLAGS="--cfg loom" cargo test -p cb-testing-tools --test loom_model`
// does, because src/sync.rs then re-exports loom's atomics to the library.
#[cfg(loom)]
#[test]
fn library_bounded_counter_never_overshoots() {
    use cb_testing_tools::counter::{Bounded, IdGen};
    loom::model(|| {
        let b = Arc::new(Bounded::new(2));
        let hs: Vec<_> = (0..3)
            .map(|_| {
                let b = b.clone();
                thread::spawn(move || b.try_take())
            })
            .collect();
        let granted = hs
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|g| *g)
            .count();
        assert_eq!(granted, 2);
        assert_eq!(b.used(), 2);
        let _ = IdGen::new().next_id();
    });
}
