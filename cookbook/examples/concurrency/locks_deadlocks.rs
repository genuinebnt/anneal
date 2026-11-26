//! RECIPE 51 (concurrency): Mutex, RwLock, poisoning, lock ordering, try_lock, and Condvar.
//! LEVEL: advanced
//! Use in: shared caches, schedulers with shared state, anything where two threads touch the same data.   Run: cargo run --release --example locks_deadlocks
//! COMMON USES: shared caches and accounts: lock ordering, poisoning policy, Condvar signalling.
#![allow(dead_code)]
//!
//! Rules that prevent most lock bugs:
//!  1. Keep critical sections SMALL; never do IO/sleep/await while holding a guard.
//!  2. If code ever needs two locks, always take them in the SAME global order (or you can deadlock).
//!  3. Prefer message passing or one owner task (shared_state) when the logic gets complicated.
//!  4. `RwLock` helps only when reads vastly outnumber writes and critical sections are not tiny.
//! A std `Mutex` is "poisoned" if a thread panicked while holding it; decide whether to recover.
//! Faster/fairer locks: https://docs.rs/parking_lot . Read: https://doc.rust-lang.org/std/sync/struct.Mutex.html
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::Duration;

struct Account {
    id: u32,
    balance: Mutex<i64>,
}

// Deadlock-free transfer: ALWAYS lock the lower id first, whatever the direction.
fn transfer(from: &Account, to: &Account, amount: i64) {
    let (first, second) = if from.id < to.id {
        (from, to)
    } else {
        (to, from)
    };
    let mut g1 = first.balance.lock().unwrap();
    let mut g2 = second.balance.lock().unwrap();
    let (src, dst) = if from.id < to.id {
        (&mut *g1, &mut *g2)
    } else {
        (&mut *g2, &mut *g1)
    };
    *src -= amount;
    *dst += amount;
}

fn main() {
    // RwLock: many readers OR one writer
    let cfg = Arc::new(RwLock::new(String::from("v1")));
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let cfg = cfg.clone();
            thread::spawn(move || cfg.read().unwrap().len())
        })
        .collect();
    println!(
        "reader lens: {:?}",
        readers
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    );
    *cfg.write().unwrap() = "v2".into();
    println!("{}", cfg.read().unwrap());

    // opposite-direction transfers at the same time, no deadlock thanks to ordering
    let a = Arc::new(Account {
        id: 1,
        balance: Mutex::new(1000),
    });
    let b = Arc::new(Account {
        id: 2,
        balance: Mutex::new(1000),
    });
    let t1 = {
        let (a, b) = (a.clone(), b.clone());
        thread::spawn(move || (0..5000).for_each(|_| transfer(&a, &b, 1)))
    };
    let t2 = {
        let (a, b) = (a.clone(), b.clone());
        thread::spawn(move || (0..5000).for_each(|_| transfer(&b, &a, 1)))
    };
    t1.join().unwrap();
    t2.join().unwrap();
    println!(
        "total preserved: {}",
        *a.balance.lock().unwrap() + *b.balance.lock().unwrap()
    );

    // try_lock: do not wait; useful for "skip if busy" work
    let m = Mutex::new(0);
    let g = m.lock().unwrap();
    println!("try_lock while held: {}", m.try_lock().is_err());
    drop(g);

    // poisoning: a panic while holding the guard poisons the mutex; recover the data if it is still valid
    std::panic::set_hook(Box::new(|_| {}));
    let shared = Arc::new(Mutex::new(vec![1, 2, 3]));
    let s2 = shared.clone();
    let _ = thread::spawn(move || {
        let _g = s2.lock().unwrap();
        panic!("crashed while holding the lock");
    })
    .join();
    println!("poisoned: {}", shared.is_poisoned());
    let data = shared.lock().unwrap_or_else(|e| e.into_inner()); // take the guard anyway
    println!("recovered {:?}", *data);
    drop(data);

    // Condvar: sleep until another thread signals a condition (no busy loop)
    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    let p2 = pair.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(30));
        *p2.0.lock().unwrap() = true;
        p2.1.notify_one();
    });
    let (lock, cv) = &*pair;
    let ready = cv
        .wait_while(lock.lock().unwrap(), |ready| !*ready)
        .unwrap(); // loops for spurious wakeups
    println!("condition met: {}", *ready);
    drop(ready);
    worker.join().unwrap();
}
