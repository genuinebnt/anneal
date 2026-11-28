//! RECIPE 504 (concurrency): how far apart must two busy counters be?
//! LEVEL: intermediate
//! Use in: placing the indices of a queue, per-thread counters.   Run: cargo run --release --example cache_line_distance
//!
//! WHAT: two threads each add to their OWN counter. Nothing is shared in the program, but cores move memory in
//! whole cache lines. Counters on one line can slow each other down ("false sharing").
//!  * The program times three distances: 8, 64 and 128 bytes. It checks the counts and the distances.
//!  * It does NOT check the times. They depend on the CPU, its load and where the threads run.
//!
//! COMMON USES: crossbeam's `CachePadded`, sharded counters, queue indices.
//! Read: https://docs.rs/crossbeam-utils/latest/crossbeam_utils/struct.CachePadded.html
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::thread;
use std::time::Instant;

/// 32 counters in a row, starting on a 128-byte boundary.
#[repr(align(128))]
struct Row([AtomicU64; 32]);

/// Two threads add `rounds` times each: one to slot 0, one to slot `other`. Returns the seconds taken.
fn race(row: &Row, other: usize, rounds: u64) -> f64 {
    let start = Instant::now();
    thread::scope(|s| {
        for slot in [0, other] {
            let counter = &row.0[slot];
            s.spawn(move || {
                for _ in 0..rounds {
                    // Relaxed is enough: the count is read only after both threads are joined
                    counter.fetch_add(1, Relaxed);
                }
            });
        }
    });
    start.elapsed().as_secs_f64()
}

fn main() {
    let rounds: u64 = if cfg!(miri) { 100 } else { 5_000_000 };
    for bytes in [8, 64, 128] {
        let other = bytes / size_of::<AtomicU64>();
        let mut times = Vec::new();
        for _ in 0..if cfg!(miri) { 1 } else { 9 } {
            let row = Row(std::array::from_fn(|_| AtomicU64::new(0)));
            let distance = (&raw const row.0[other]).addr() - (&raw const row.0[0]).addr();
            assert_eq!(distance, bytes);
            assert_eq!((&raw const row.0[0]).addr() % 128, 0);
            times.push(race(&row, other, rounds));
            assert_eq!(row.0[0].load(Relaxed), rounds);
            assert_eq!(row.0[other].load(Relaxed), rounds);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{bytes:>3} bytes apart: median {:.1} ms, fastest {:.1} ms, slowest {:.1} ms (not checked)",
            times[times.len() / 2] * 1e3,
            times[0] * 1e3,
            times[times.len() - 1] * 1e3
        );
    }
}
