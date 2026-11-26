//! RECIPE 85 (perf): false sharing: two threads, two separate counters, one cache line, 5-10x slower.
//! LEVEL: advanced
//! Use in: per-thread/per-worker counters, stats, queues with head/tail indices, any hot shared atomics.   Run: cargo run --release --example false_sharing
//! COMMON USES: per-thread counters/metrics, queue head/tail indices, any hot shared atomics.
#![allow(dead_code)]
//!
//! Cores keep their own copy of each cache line and must agree via a coherence protocol. If thread A writes variable X and
//! thread B writes a DIFFERENT variable Y that happens to sit in the SAME 64-byte line, every write invalidates the other core's
//! copy: the line ping-pongs although the threads share no data. Fix: pad/align hot per-thread data to its own line
//! (`#[repr(align(64))]`; crossbeam's `CachePadded` does this, using 128 on some CPUs where the prefetcher pairs lines).
//! Read: https://en.algorithmica.org/hpc/cpu-cache/sharing/ and https://docs.rs/crossbeam-utils/latest/crossbeam_utils/struct.CachePadded.html
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::thread;
use std::time::Instant;

#[repr(align(64))]
struct Padded(AtomicU64);

fn hammer(c: &AtomicU64, n: u64) {
    for _ in 0..n {
        c.fetch_add(1, Relaxed);
    }
}

fn main() {
    const N: u64 = 10_000_000;

    // adjacent in memory: both counters live in the same cache line
    let packed = [AtomicU64::new(0), AtomicU64::new(0)];
    let t = Instant::now();
    thread::scope(|s| {
        s.spawn(|| hammer(&packed[0], N));
        s.spawn(|| hammer(&packed[1], N));
    });
    let packed_time = t.elapsed();

    // each counter in its own line
    let padded = [Padded(AtomicU64::new(0)), Padded(AtomicU64::new(0))];
    let t = Instant::now();
    thread::scope(|s| {
        s.spawn(|| hammer(&padded[0].0, N));
        s.spawn(|| hammer(&padded[1].0, N));
    });
    let padded_time = t.elapsed();

    assert_eq!(packed[0].load(Relaxed) + packed[1].load(Relaxed), 2 * N);
    assert_eq!(padded[0].0.load(Relaxed) + padded[1].0.load(Relaxed), 2 * N);
    println!(
        "size_of: packed pair = {} bytes, padded pair = {} bytes",
        std::mem::size_of_val(&packed),
        std::mem::size_of_val(&padded)
    );
    println!("same line:      {packed_time:.2?}");
    println!("separate lines: {padded_time:.2?}");
    println!(
        "speed-up from padding: {:.1}x (needs >= 2 cores; ~1x on a single core)",
        packed_time.as_secs_f64() / padded_time.as_secs_f64()
    );
    println!(
        "trade-off: padding wastes memory, so apply it only to hot, per-thread-written data (counters, queue indices)."
    );
}
