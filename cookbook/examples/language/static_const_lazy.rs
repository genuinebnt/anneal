//! RECIPE 40 (language): const, static, OnceLock, LazyLock, thread_local, and global atomics.
//! LEVEL: simple
//! Use in: config loaded once, lookup tables, global counters/ids, caches.   Run: cargo run --example static_const_lazy
//! COMMON USES: global config, lookup tables built once, id generators, per-thread caches.
//!
//!   const X: T      inlined at every use, evaluated at compile time. Prefer for plain values.
//!   static X: T     ONE memory location with a fixed address for the whole program (`'static`).
//!   static mut      avoid: unsafe and almost always replaceable by the items below.
//!   OnceLock<T>     set exactly once at runtime (e.g. config parsed from args), then read anywhere.
//!   LazyLock<T>     compute on first use (e.g. a table built from a function), thread-safe.
//!   thread_local!   one value per thread.
//!   Atomic* statics lock-free global counters.
//! Docs: https://doc.rust-lang.org/std/sync/struct.OnceLock.html and https://doc.rust-lang.org/std/sync/struct.LazyLock.html
#![allow(dead_code)]
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, OnceLock};

const MAX_RETRIES: u32 = 5;
const fn kib(n: usize) -> usize {
    n * 1024 // a const fn can run at compile time
}
const BUF: usize = kib(64);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

static CONFIG: OnceLock<Config> = OnceLock::new();
#[derive(Debug)]
struct Config {
    workers: usize,
}
fn config() -> &'static Config {
    CONFIG
        .get()
        .expect("config must be initialised in main first")
}

static EXIT_NAMES: LazyLock<HashMap<i32, &'static str>> = LazyLock::new(|| {
    println!("  (building table once)");
    HashMap::from([(0, "ok"), (1, "user error"), (2, "usage")])
});

thread_local! {
    static CALLS: Cell<u32> = const { Cell::new(0) };
}

fn main() {
    println!("{MAX_RETRIES} {BUF} ids {} {}", next_id(), next_id());
    CONFIG.set(Config { workers: 4 }).unwrap();
    println!(
        "{:?} second set rejected: {}",
        config(),
        CONFIG.set(Config { workers: 9 }).is_err()
    );
    println!("first use:");
    println!("{:?}", EXIT_NAMES.get(&1));
    println!("second use (no rebuild): {:?}", EXIT_NAMES.get(&2));
    CALLS.with(|c| c.set(c.get() + 1));
    let other = std::thread::spawn(|| CALLS.with(|c| c.get()))
        .join()
        .unwrap();
    println!(
        "this thread: {}, other thread sees: {other}",
        CALLS.with(|c| c.get())
    );
}
