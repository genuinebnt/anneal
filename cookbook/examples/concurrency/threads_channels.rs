//! RECIPE 50 (concurrency): OS threads: spawn/join, scoped threads, channels, and what `Send`/`Sync` mean.
//! LEVEL: intermediate
//! Use in: CPU-bound work, background workers, understanding what tokio builds on.   Run: cargo run --release --example threads_channels
//! COMMON USES: CPU-bound parallel work, background workers, understanding what async runtimes build on.
#![allow(dead_code)]
//!
//!  Send = a value can be MOVED to another thread.   Sync = `&T` can be SHARED between threads.
//!  `Rc` is neither (non-atomic counter); `Arc<Mutex<T>>` is both; raw `RefCell` is Send but not Sync.
//!  The compiler checks this for you: sharing the wrong thing across `thread::spawn` is a compile error.
//! Read: https://doc.rust-lang.org/book/ch16-00-concurrency.html and Rust Atomics and Locks https://marabos.nl/atomics/
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    // 1. spawn + join returns the closure's value; `move` gives the thread its own data
    let h = thread::spawn(move || (1..=10).sum::<u32>());
    println!("sum from thread = {}", h.join().unwrap());

    // 2. scoped threads may BORROW locals (no Arc, no 'static): they are guaranteed to finish before the scope ends
    let data = vec![1u64, 2, 3, 4, 5, 6, 7, 8];
    let total: u64 = thread::scope(|s| {
        let handles: Vec<_> = data
            .chunks(3)
            .map(|chunk| s.spawn(move || chunk.iter().sum::<u64>()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).sum() // join in order => deterministic
    });
    println!("parallel sum = {total}");

    // 3. channels: many producers (clone the Sender), one consumer; the loop ends when ALL senders are dropped
    let (tx, rx) = mpsc::channel::<(usize, String)>();
    for id in 0..3 {
        let tx = tx.clone();
        thread::spawn(move || {
            tx.send((id, format!("hello from {id}"))).unwrap();
        });
    }
    drop(tx); // forget this and `rx` below would wait forever
    let mut got: Vec<_> = rx.iter().collect();
    got.sort(); // arrival order is nondeterministic: sort before printing
    println!("{got:?}");

    // 4. shared mutable state: Arc (shared ownership) + Mutex (exclusive access)
    let counter = Arc::new(Mutex::new(0));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let c = Arc::clone(&counter);
            thread::spawn(move || {
                for _ in 0..1000 {
                    *c.lock().unwrap() += 1;
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    println!("counter = {}", *counter.lock().unwrap());

    // 5. a panicking thread does not kill the process: join returns Err
    std::panic::set_hook(Box::new(|_| {}));
    let r = thread::spawn(|| -> u32 { panic!("worker died") }).join();
    println!("worker panicked: {}", r.is_err());

    // Does not compile (uncomment): Rc is not Send
    // let rc = std::rc::Rc::new(1); thread::spawn(move || println!("{rc}"));
    // error[E0277]: `Rc<i32>` cannot be sent between threads safely
    println!(
        "cpus: {}",
        thread::available_parallelism()
            .map(|n| n.get() > 0)
            .unwrap_or(true)
    );
}
