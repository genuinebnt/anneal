//! RECIPE 37 (language): `Drop` and RAII: cleanup that cannot be forgotten.
//! LEVEL: simple
//! Use in: releasing leases/locks/permits, temp files, "task finished" markers, metrics timers.   Run: cargo run --example drop_raii
//! COMMON USES: locks, file handles, temp files, metrics timers, transaction guards: cleanup that cannot be forgotten.
//!
//! RAII: acquiring a resource creates a guard value; when the guard goes out of scope (return, `?`, even panic),
//! `Drop::drop` runs and releases it. `MutexGuard`, `File`, tokio `OwnedSemaphorePermit` all work this way.
//! Drop order: locals drop in REVERSE declaration order; struct fields drop in declaration order.
//! Read: https://doc.rust-lang.org/book/ch15-03-drop.html and https://doc.rust-lang.org/std/ops/trait.Drop.html
#![allow(dead_code)]
use std::cell::RefCell;
use std::rc::Rc;

struct Noisy(&'static str);
impl Drop for Noisy {
    fn drop(&mut self) {
        println!("  drop {}", self.0);
    }
}

// A lease that returns itself to a pool when dropped.
struct Lease {
    id: u32,
    pool: Rc<RefCell<Vec<u32>>>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.pool.borrow_mut().push(self.id); // runs on every exit path
    }
}

// Scope guard: run a closure at scope end (like Go's `defer`).
struct Defer<F: FnMut()>(F);
impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

fn risky(pool: &Rc<RefCell<Vec<u32>>>, fail: bool) -> Result<(), &'static str> {
    let _lease = Lease {
        id: 7,
        pool: pool.clone(),
    };
    if fail {
        return Err("failed early"); // lease is STILL returned to the pool
    }
    Ok(())
}

fn main() {
    println!("declaration order a, b, c:");
    {
        let _a = Noisy("a");
        let _b = Noisy("b");
        let _c = Noisy("c");
    } // drops c, b, a
    println!("early drop:");
    let x = Noisy("x");
    drop(x); // std::mem::drop just takes ownership and lets it fall out of scope
    println!("after early drop");

    let pool = Rc::new(RefCell::new(vec![]));
    let _ = risky(&pool, true);
    let _ = risky(&pool, false);
    println!("leases returned: {:?}", pool.borrow());

    {
        let _cleanup = Defer(|| println!("  cleanup ran"));
        println!("  doing work");
    }

    // Drop runs during a panic unwind too.
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(|| {
        let _g = Noisy("guard-during-panic");
        panic!("boom");
    });
    println!("panic caught: {}", r.is_err());
    // `mem::forget(x)` skips Drop (leaks); needed almost never.
}
