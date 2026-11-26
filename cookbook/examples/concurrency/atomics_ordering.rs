//! RECIPE 16 (concurrency): atomics, and what `Ordering` actually buys you.
//! LEVEL: advanced
//! Use in: counters, flags, lock-free structures, concurrency invariants.   Run: cargo run --example atomics_ordering
//! COMMON USES: counters, flags, lock-free structures, publishing config to other threads without a mutex.
//!
//! Pattern 1: a shared counter needs atomicity, not ordering: `Relaxed` is enough.
//! Pattern 2: to PUBLISH data, write it, then set a flag with `Release`; the reader loads the flag with
//! `Acquire`. That pairing guarantees the reader sees the data written before the flag.
//! Book (free): https://marabos.nl/atomics/
#![allow(dead_code)]
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;

static DATA: AtomicU64 = AtomicU64::new(0);
static READY: AtomicBool = AtomicBool::new(false);

fn main() {
    let counter = AtomicUsize::new(0);
    thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                for _ in 0..1_000 {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });
    println!(
        "counter = {} (always 4000)",
        counter.load(Ordering::Relaxed)
    );
    assert_eq!(counter.load(Ordering::Relaxed), 4000);

    thread::scope(|s| {
        s.spawn(|| {
            DATA.store(42, Ordering::Relaxed);
            READY.store(true, Ordering::Release); // publish
        });
        s.spawn(|| {
            while !READY.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            assert_eq!(DATA.load(Ordering::Relaxed), 42); // guaranteed by Release/Acquire
            println!("reader saw the published data");
        });
    });
}
