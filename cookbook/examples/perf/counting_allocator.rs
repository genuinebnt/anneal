//! RECIPE 57 (perf): count heap allocations with a custom global allocator, then remove them.
//! LEVEL: advanced
//! Use in: stage 8, hot loops, finding hidden allocations (String/Vec/Box/clone/format!).   Run: cargo run --release --example counting_allocator
//! COMMON USES: finding hidden allocations in hot paths; asserting allocation counts in tests.
#![allow(dead_code)]
//!
//! Allocation is often the real cost in Rust code, and it is invisible in the source. A wrapper around the system
//! allocator that counts calls lets you ASSERT "this function allocates at most N times", which makes a perf test
//! deterministic (unlike timing). Tools: dhat https://docs.rs/dhat , heaptrack, cargo-flamegraph.
//! Tactics shown: `with_capacity`, reusing a buffer (`clear()` keeps capacity), `write!` instead of `format!`, borrowing
//! instead of `clone`.  Read: https://nnethercote.github.io/perf-book/heap-allocations.html
use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(l.size(), Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed); // a growing Vec reallocates: count it
        BYTES.fetch_add(new_size, Relaxed);
        unsafe { System.realloc(p, l, new_size) }
    }
}
#[global_allocator]
static GLOBAL: Counting = Counting;

/// Run `f`, return (allocation calls, bytes requested). Do not print inside `f`.
fn measure<R>(f: impl FnOnce() -> R) -> (usize, usize) {
    let (a0, b0) = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
    black_box(f());
    (ALLOCS.load(Relaxed) - a0, BYTES.load(Relaxed) - b0)
}

fn main() {
    let grow = measure(|| {
        let mut v = Vec::new();
        for i in 0..1000u64 {
            v.push(i);
        }
        v
    });
    let presized = measure(|| {
        let mut v = Vec::with_capacity(1000);
        for i in 0..1000u64 {
            v.push(i);
        }
        v
    });
    println!(
        "Vec push x1000:           {:>3} allocs {:>6} bytes",
        grow.0, grow.1
    );
    println!(
        "with_capacity(1000):      {:>3} allocs {:>6} bytes",
        presized.0, presized.1
    );
    assert_eq!(presized.0, 1);
    assert!(grow.0 > presized.0);

    let formatted = measure(|| {
        (0..100)
            .map(|i| format!("job-{i}"))
            .map(|s| s.len())
            .sum::<usize>()
    });
    let mut buf = String::with_capacity(32);
    let reused = measure(|| {
        let mut total = 0;
        for i in 0..100 {
            buf.clear(); // keeps the capacity: no new allocation
            write!(buf, "job-{i}").unwrap();
            total += buf.len();
        }
        total
    });
    println!("format! x100:             {:>3} allocs", formatted.0);
    println!("reused String + write! :  {:>3} allocs", reused.0);
    assert!(reused.0 < 5 && formatted.0 >= 100);

    let words = vec![
        String::from("alpha"),
        String::from("beta"),
        String::from("gamma"),
    ];
    let cloned = measure(|| words.iter().cloned().collect::<Vec<String>>());
    let borrowed = measure(|| words.iter().map(|s| s.as_str()).collect::<Vec<&str>>());
    println!("cloning 3 Strings:        {:>3} allocs", cloned.0);
    println!("collecting 3 &str:        {:>3} allocs", borrowed.0);
    assert!(borrowed.0 < cloned.0);

    let mut sorted = vec![3, 1, 2];
    let in_place = measure(|| sorted.sort_unstable());
    println!("sort_unstable in place:   {:>3} allocs", in_place.0);
}
