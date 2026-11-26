//! RECIPE 110 (concurrency): memory-ordering litmus tests with real threads: message passing, store buffering, spinlock, fences.
//! LEVEL: advanced
//! Use in: lock-free code, understanding what `Ordering` buys you, reviewing `unsafe` synchronization.   Run: cargo run --release --example memory_ordering_litmus
//! (unsafe part: also try `cargo +nightly miri run --example memory_ordering_litmus`, reduce iterations first)
#![allow(dead_code)]
//!
//! A litmus test is a tiny multi-threaded program whose possible outcomes reveal the memory model.
//!   (a) MESSAGE PASSING: writer `data=42; flag.store(Release)`; reader `while !flag.load(Acquire){}; read data`. The Release
//!       store "publishes" everything before it; the Acquire load that sees it "imports" it. Stale data is forbidden.
//!   (b) STORE BUFFERING: T1 `x=1; r1=y`, T2 `y=1; r2=x`. Outcome r1==0&&r2==0 means each thread's store was still in its store
//!       buffer when the other loaded. Release/Acquire does NOT forbid it (it orders only against the SAME location's pairing);
//!       only SeqCst (single total order) does. The Relaxed count is hardware-dependent: printed, never asserted.
//!   (c) SPINLOCK: `compare_exchange(Acquire)` to lock, `store(Release)` to unlock: the critical sections form a happens-before chain,
//!       so a NON-atomic counter in an `UnsafeCell` is race free.
//!   (d) FENCE: `fence(SeqCst)` between the store and the load gives the same SB guarantee as SeqCst accesses.
//!
//!  Ordering   | guarantees                                           | x86-64 (TSO)            | ARMv8
//!  -----------|------------------------------------------------------|-------------------------|---------------------------
//!  Relaxed    | atomicity only, no ordering vs other locations       | plain mov               | plain ldr/str
//!  Acquire    | later ops cannot move before this load               | plain mov (free)        | ldar
//!  Release    | earlier ops cannot move after this store             | plain mov (free)        | stlr
//!  AcqRel     | both, for read-modify-write (e.g. swap, CAS)         | lock-prefixed op        | ldaxr/stlxr or LSE casal
//!  SeqCst     | Acq/Rel + one global total order of all SeqCst ops   | xchg/mfence on stores   | ldar/stlr (+ dmb in places)
//! x86 already orders everything except store->load, which is why SB shows up there with Relaxed and why bugs in
//! Release/Acquire code often hide on x86 and appear on ARM.
//! COMMON USES: mutexes/spinlocks, reference counts (Arc uses Release on decrement + Acquire fence), `Once`, SPSC queues (spsc_ring_buffer), seqlocks.
//! Loom (https://github.com/tokio-rs/loom) model-checks all legal interleavings and reorderings of such code; use it in CI for
//! lock-free structures, since running threads in a loop can never prove absence of a bug.
//! Read: https://marabos.nl/atomics/memory-ordering.html , https://doc.rust-lang.org/nomicon/atomics.html
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering, fence};
use std::thread;

/// Reusable sense-less spin barrier for exactly `n` threads (generation counter).
struct SpinBarrier {
    n: usize,
    count: AtomicUsize,
    gen_: AtomicUsize,
}
impl SpinBarrier {
    fn new(n: usize) -> Self {
        Self {
            n,
            count: AtomicUsize::new(0),
            gen_: AtomicUsize::new(0),
        }
    }
    fn wait(&self) {
        let g = self.gen_.load(Ordering::Acquire);
        if self.count.fetch_add(1, Ordering::AcqRel) + 1 == self.n {
            self.count.store(0, Ordering::Relaxed);
            self.gen_.fetch_add(1, Ordering::Release); // releases the waiters, publishing the reset
        } else {
            while self.gen_.load(Ordering::Acquire) == g {
                std::hint::spin_loop();
            }
        }
    }
}

// (a) message passing
fn message_passing(iters: usize) {
    let data = AtomicU32::new(0);
    let flag = AtomicBool::new(false);
    let bar = SpinBarrier::new(2);
    let stale = AtomicUsize::new(0);
    thread::scope(|s| {
        s.spawn(|| {
            for i in 1..=iters as u32 {
                bar.wait(); // start of round
                data.store(i, Ordering::Relaxed);
                flag.store(true, Ordering::Release);
                bar.wait(); // end of round
                flag.store(false, Ordering::Relaxed);
                bar.wait(); // reset visible to all
            }
        });
        s.spawn(|| {
            for i in 1..=iters as u32 {
                bar.wait();
                while !flag.load(Ordering::Acquire) {
                    std::hint::spin_loop();
                }
                if data.load(Ordering::Relaxed) != i {
                    stale.fetch_add(1, Ordering::Relaxed);
                }
                bar.wait();
                bar.wait();
            }
        });
    });
    assert_eq!(
        stale.load(Ordering::Relaxed),
        0,
        "Release/Acquire must never expose stale data"
    );
}

// (b)/(d) store buffering, parametrised by orderings
fn store_buffering(iters: usize, store: Ordering, load: Ordering, use_fence: bool) -> usize {
    let (x, y) = (AtomicU32::new(0), AtomicU32::new(0));
    let (r1, r2) = (AtomicU32::new(0), AtomicU32::new(0));
    let bar = SpinBarrier::new(2);
    let both_zero = AtomicUsize::new(0);
    thread::scope(|s| {
        s.spawn(|| {
            for _ in 0..iters {
                bar.wait();
                x.store(1, store);
                if use_fence {
                    fence(Ordering::SeqCst);
                }
                r1.store(y.load(load), Ordering::Relaxed);
                bar.wait();
            }
        });
        s.spawn(|| {
            for _ in 0..iters {
                bar.wait();
                y.store(1, store);
                if use_fence {
                    fence(Ordering::SeqCst);
                }
                r2.store(x.load(load), Ordering::Relaxed);
                bar.wait();
                // after the 2nd barrier both results are visible; thread 2 checks and resets
                if r1.load(Ordering::Relaxed) == 0 && r2.load(Ordering::Relaxed) == 0 {
                    both_zero.fetch_add(1, Ordering::Relaxed);
                }
                x.store(0, Ordering::Relaxed);
                y.store(0, Ordering::Relaxed);
                // the next round's first barrier publishes these resets to thread 1
            }
        });
    });
    both_zero.load(Ordering::Relaxed)
}

// (c) spinlock
struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}
// SAFETY: the lock gives exclusive access to `value`, so sharing `&SpinLock<T>` across threads is sound when T: Send.
unsafe impl<T: Send> Sync for SpinLock<T> {}
impl<T> SpinLock<T> {
    fn new(v: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(v),
        }
    }
    fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        // SAFETY: we hold the lock, so no other thread accesses `value`; Acquire pairs with the previous holder's Release.
        let r = f(unsafe { &mut *self.value.get() });
        self.locked.store(false, Ordering::Release); // publishes our writes to the next holder
        r
    }
}

fn main() {
    message_passing(100_000);
    println!("(a) message passing: 100000 rounds, no stale reads");

    let n = 100_000;
    let relaxed = store_buffering(n, Ordering::Relaxed, Ordering::Relaxed, false);
    println!(
        "(b) store buffering r1==0&&r2==0 out of {n}: Relaxed = {relaxed} (machine dependent)"
    );
    // Release store / Acquire load are the strongest legal pair for plain stores/loads; the outcome is still allowed.
    let relacq = store_buffering(n, Ordering::Release, Ordering::Acquire, false);
    println!("    Release/Acquire = {relacq} (allowed to be non-zero)");
    let seqcst = store_buffering(n, Ordering::SeqCst, Ordering::SeqCst, false);
    println!("    SeqCst          = {seqcst}");
    assert_eq!(seqcst, 0, "SeqCst forbids the both-zero outcome");
    let fenced = store_buffering(n, Ordering::Relaxed, Ordering::Relaxed, true);
    println!("(d) Relaxed + fence(SeqCst) = {fenced}");
    assert_eq!(
        fenced, 0,
        "fence(SeqCst) between store and load forbids it too"
    );

    let lock = SpinLock::new(0u64);
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                for _ in 0..50_000 {
                    lock.with(|c| *c += 1);
                }
            });
        }
    });
    let total = lock.with(|c| *c);
    assert_eq!(total, 200_000);
    println!("(c) spinlock protected non-atomic counter = {total}");
}
