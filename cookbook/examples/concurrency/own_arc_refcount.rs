//! RECIPE 192 (concurrency): your own `Arc<T>` and `Weak<T>` from scratch: atomic refcounts, Release/Acquire on drop, CAS upgrade.
//! LEVEL: advanced
//! Use in: understanding std::sync::Arc, intrusive/shared ownership, any code that frees memory when "the last owner leaves".   Run: cargo run --release --example own_arc_refcount
//! (unsafe part: also try `cargo +nightly miri run --example own_arc_refcount`; Miri is not installed here, lower the loop counts first)
#![allow(dead_code)]
//!
//! DESIGN. One heap block `Inner<T> { strong, weak, data }`. `MyArc` and `MyWeak` are both a `NonNull<Inner<T>>`.
//!  * `strong` counts `MyArc`s. When it reaches 0 the VALUE `T` is dropped (in place, `data` is `ManuallyDrop`).
//!  * `weak` counts `MyWeak`s PLUS ONE implicit weak that all the strongs share together. So the allocation is freed
//!    when `weak` reaches 0, which can only happen after the last strong has gone and released the implicit weak. This
//!    is the std design; it makes "last strong drops value, then drops one weak" a single uniform code path.
//!
//! ORDERINGS (Rust Atomics and Locks, ch. 6 https://marabos.nl/atomics/building-arc.html):
//!  * `clone`: `fetch_add(Relaxed)`. Creating a new owner from an existing owner needs no ordering: we already hold a
//!    reference, so the count cannot hit zero under us, and we are not publishing any data. Guard against overflow
//!    (`mem::forget` in a loop could wrap to 0 and cause use-after-free): abort past `isize::MAX`.
//!  * `drop`: `fetch_sub(Release)`. Every thread that used the data (reads, or writes via get_mut) must have those accesses
//!    ordered BEFORE its decrement is visible. Release gives that.
//!  * The thread that sees the old value 1 (the last owner) then runs `fence(Acquire)` before dropping/freeing. The fence
//!    synchronises with the Release decrements of ALL other threads, so every earlier access by them happens-before our
//!    destructor. Without it, another thread's last read of `T` could be reordered after our free (a data race / UAF).
//!    The fence is used instead of making every decrement AcqRel because only the final decrementer needs Acquire.
//!  * `upgrade` (Weak to Arc): a CAS loop `n -> n+1` that refuses when `n == 0`. A plain `fetch_add` would resurrect an
//!    object whose value is already being dropped. Relaxed is enough: success means a strong exists, so data is alive.
//!  * `get_mut` needs uniqueness: strong == 1 AND weak == 1 (no other Weak could upgrade). Acquire loads pair with the
//!    Release decrements of owners that went away. `try_unwrap` is `strong: CAS 1 -> 0` then an Acquire fence.
//!  * `Send`/`Sync` need `T: Send + Sync`: sharing an Arc lets any thread get `&T` (needs Sync) and the last owner on any
//!    thread drops `T` (needs Send). An Arc<Cell<_>> must not be Send, otherwise it is a data race.
//! COMMON USES: this is how std::sync::Arc, the `triomphe` crate (no weak count) and tokio's task refcounts work;
//! Weak breaks parent/child cycles (trees with parent pointers, observer lists, caches).
//! Read: https://marabos.nl/atomics/building-arc.html , https://doc.rust-lang.org/nomicon/atomics.html , https://doc.rust-lang.org/std/sync/struct.Arc.html
use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering, fence};
use std::thread;

const MAX_REFCOUNT: usize = isize::MAX as usize;

struct Inner<T> {
    strong: AtomicUsize,
    weak: AtomicUsize, // number of MyWeak + 1 implicit weak held collectively by all strongs
    data: UnsafeCell<ManuallyDrop<T>>,
}

pub struct MyArc<T> {
    ptr: NonNull<Inner<T>>,
}
pub struct MyWeak<T> {
    ptr: NonNull<Inner<T>>,
}

// SAFETY: see module docs: &T may be reached from any thread (Sync) and T may be dropped on any thread (Send).
unsafe impl<T: Send + Sync> Send for MyArc<T> {}
// SAFETY: same reasoning; sharing &MyArc only allows &T and atomic count updates.
unsafe impl<T: Send + Sync> Sync for MyArc<T> {}
// SAFETY: a Weak can be upgraded on any thread into a MyArc, so it needs the same bounds.
unsafe impl<T: Send + Sync> Send for MyWeak<T> {}
// SAFETY: as above.
unsafe impl<T: Send + Sync> Sync for MyWeak<T> {}

impl<T> MyArc<T> {
    pub fn new(data: T) -> Self {
        let b = Box::new(Inner {
            strong: AtomicUsize::new(1),
            weak: AtomicUsize::new(1), // the implicit weak
            data: UnsafeCell::new(ManuallyDrop::new(data)),
        });
        // SAFETY: Box::into_raw never returns null.
        MyArc {
            ptr: unsafe { NonNull::new_unchecked(Box::into_raw(b)) },
        }
    }

    fn inner(&self) -> &Inner<T> {
        // SAFETY: while a MyArc exists, strong >= 1 so the allocation is live.
        unsafe { self.ptr.as_ref() }
    }

    pub fn strong_count(this: &Self) -> usize {
        this.inner().strong.load(Ordering::Relaxed)
    }
    /// Number of real Weak pointers (hides the implicit one).
    pub fn weak_count(this: &Self) -> usize {
        this.inner().weak.load(Ordering::Relaxed) - 1
    }

    /// `&mut T` only if we are the sole owner and no Weak exists (so nobody can upgrade concurrently).
    pub fn get_mut(this: &mut Self) -> Option<&mut T> {
        let i = this.inner();
        if i.weak.load(Ordering::Acquire) == 1 && i.strong.load(Ordering::Acquire) == 1 {
            // SAFETY: strong == 1 and we hold `&mut self`, so we are that one owner; weak == 1 means no Weak can create
            // another strong, and downgrade needs `&MyArc`, which `&mut` excludes. The Acquire loads pair with the
            // Release decrements of owners that left, so their accesses happen-before ours.
            Some(unsafe { &mut *(*this.inner().data.get()) })
        } else {
            None
        }
    }

    /// Take the value out if we are the only strong owner (Weak pointers may still exist; they just stop upgrading).
    pub fn try_unwrap(this: Self) -> Result<T, Self> {
        if this
            .inner()
            .strong
            .compare_exchange(1, 0, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return Err(this);
        }
        fence(Ordering::Acquire); // pairs with Release decrements of previous owners
        // SAFETY: we moved strong 1 -> 0, so we are the only owner and upgrades now fail; the value is read out once.
        let v = unsafe { ManuallyDrop::take(&mut *this.inner().data.get()) };
        let ptr = this.ptr;
        std::mem::forget(this); // do not run Drop (it would decrement strong again)
        drop(MyWeak { ptr }); // release the implicit weak; frees the block if no real Weak remains
        Ok(v)
    }

    pub fn downgrade(this: &Self) -> MyWeak<T> {
        let old = this.inner().weak.fetch_add(1, Ordering::Relaxed);
        if old > MAX_REFCOUNT {
            std::process::abort();
        }
        MyWeak { ptr: this.ptr }
    }

    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.ptr == b.ptr
    }
}

impl<T> Deref for MyArc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: strong >= 1 keeps the value alive and nobody gets &mut while shared (get_mut checks uniqueness).
        unsafe { &*self.inner().data.get() }
    }
}

impl<T> Clone for MyArc<T> {
    fn clone(&self) -> Self {
        if self.inner().strong.fetch_add(1, Ordering::Relaxed) > MAX_REFCOUNT {
            std::process::abort(); // overflow guard
        }
        MyArc { ptr: self.ptr }
    }
}

impl<T> Drop for MyArc<T> {
    fn drop(&mut self) {
        if self.inner().strong.fetch_sub(1, Ordering::Release) == 1 {
            fence(Ordering::Acquire);
            // SAFETY: we were the last strong; the fence makes all other owners' accesses happen-before this drop, and
            // upgrades fail because strong is 0, so nobody else can reach the value.
            unsafe { ManuallyDrop::drop(&mut *self.inner().data.get()) };
            // Release the implicit weak; this frees the allocation when no real Weak remains.
            drop(MyWeak { ptr: self.ptr });
        }
    }
}

impl<T> MyWeak<T> {
    fn inner(&self) -> &Inner<T> {
        // SAFETY: a MyWeak holds one weak count, which keeps the allocation (not the value) alive.
        unsafe { self.ptr.as_ref() }
    }
    pub fn upgrade(&self) -> Option<MyArc<T>> {
        let strong = &self.inner().strong;
        let mut n = strong.load(Ordering::Relaxed);
        loop {
            if n == 0 {
                return None; // value is gone or being dropped: never resurrect
            }
            assert!(n <= MAX_REFCOUNT);
            match strong.compare_exchange_weak(n, n + 1, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return Some(MyArc { ptr: self.ptr }),
                Err(cur) => n = cur,
            }
        }
    }
}

impl<T> Clone for MyWeak<T> {
    fn clone(&self) -> Self {
        if self.inner().weak.fetch_add(1, Ordering::Relaxed) > MAX_REFCOUNT {
            std::process::abort();
        }
        MyWeak { ptr: self.ptr }
    }
}

impl<T> Drop for MyWeak<T> {
    fn drop(&mut self) {
        if self.inner().weak.fetch_sub(1, Ordering::Release) == 1 {
            fence(Ordering::Acquire);
            // SAFETY: weak hit 0, so no MyArc (they hold the implicit weak) and no MyWeak exist; the block came from
            // Box::into_raw and is freed exactly once. `data` is ManuallyDrop so T is not dropped again.
            unsafe { drop(Box::from_raw(self.ptr.as_ptr())) };
        }
    }
}

// ---------- tests / demos ----------

use std::sync::Arc as StdArc;

/// Counts its own drops through a shared std Arc counter.
struct Tracked {
    drops: StdArc<AtomicUsize>,
    payload: Vec<u64>,
}
impl Drop for Tracked {
    fn drop(&mut self) {
        self.payload.clear();
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn tracked(drops: &StdArc<AtomicUsize>) -> MyArc<Tracked> {
    MyArc::new(Tracked {
        drops: drops.clone(),
        payload: (0..64).collect(),
    })
}

/// Cycle demo node: `strong_link` may create cycles, `weak_link` may not leak.
struct Node {
    drops: StdArc<AtomicUsize>,
    strong_link: Mutex<Option<MyArc<Node>>>,
    weak_link: Mutex<Option<MyWeak<Node>>>,
}
impl Drop for Node {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn node(d: &StdArc<AtomicUsize>) -> MyArc<Node> {
    MyArc::new(Node {
        drops: d.clone(),
        strong_link: Mutex::new(None),
        weak_link: Mutex::new(None),
    })
}

fn main() {
    // 1. basics
    let drops = StdArc::new(AtomicUsize::new(0));
    let mut a = tracked(&drops);
    assert!(MyArc::get_mut(&mut a).is_some());
    let b = a.clone();
    assert_eq!(MyArc::strong_count(&a), 2);
    assert!(MyArc::get_mut(&mut a).is_none(), "shared: no &mut");
    let w = MyArc::downgrade(&a);
    assert_eq!(MyArc::weak_count(&a), 1);
    drop(b);
    assert!(
        MyArc::get_mut(&mut a).is_none(),
        "a Weak exists: could upgrade"
    );
    drop(w);
    MyArc::get_mut(&mut a).unwrap().payload.push(7);
    assert_eq!(a.payload.len(), 65);
    let a2 = a.clone();
    let Err(a) = MyArc::try_unwrap(a) else {
        panic!("not unique")
    };
    drop(a2);
    let t = MyArc::try_unwrap(a).ok().unwrap();
    assert_eq!(
        drops.load(Ordering::SeqCst),
        0,
        "moved out, not dropped yet"
    );
    drop(t);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    println!("basics ok");

    // 2. 8 threads x 100k clone/drop pairs: T dropped exactly once
    let drops = StdArc::new(AtomicUsize::new(0));
    let shared = tracked(&drops);
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let local = shared.clone();
            thread::spawn(move || {
                for _ in 0..100_000 {
                    let c = local.clone();
                    assert_eq!(c.payload.len(), 64); // read the data: must still be alive
                    drop(c);
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(MyArc::strong_count(&shared), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(shared);
    assert_eq!(drops.load(Ordering::SeqCst), 1, "dropped exactly once");
    println!("clone/drop stress ok: exactly one drop");

    // 3. weak upgrade races: upgraders hammer while the owners vanish
    let mut total_upgrades = 0usize;
    for round in 0..200 {
        let drops = StdArc::new(AtomicUsize::new(0));
        let strong = tracked(&drops);
        let weak = MyArc::downgrade(&strong);
        let hs: Vec<_> = (0..4)
            .map(|_| {
                let w = weak.clone();
                thread::spawn(move || {
                    let mut ok = 0usize;
                    for _ in 0..2_000 {
                        match w.upgrade() {
                            Some(s) => {
                                // if upgrade succeeded the value must be intact (not freed, not dropped)
                                assert_eq!(s.payload.iter().sum::<u64>(), 63 * 64 / 2);
                                ok += 1;
                            }
                            None => break, // once dead, always dead
                        }
                    }
                    assert!(w.upgrade().is_none() || ok > 0);
                    ok
                })
            })
            .collect();
        if round % 2 == 0 {
            thread::yield_now();
        }
        drop(strong); // last owner may leave while upgraders hold temporary strongs
        total_upgrades += hs.into_iter().map(|h| h.join().unwrap()).sum::<usize>();
        assert!(weak.upgrade().is_none(), "never resurrected");
        assert_eq!(
            drops.load(Ordering::SeqCst),
            1,
            "exactly one drop in round {round}"
        );
    }
    println!("weak upgrade races ok ({total_upgrades} successful upgrades, never resurrected)");

    // 4. cycle leak and the Weak fix
    let d = StdArc::new(AtomicUsize::new(0));
    {
        let (x, y) = (node(&d), node(&d));
        *x.strong_link.lock().unwrap() = Some(y.clone());
        *y.strong_link.lock().unwrap() = Some(x.clone()); // x <-> y strong cycle
    }
    assert_eq!(
        d.load(Ordering::SeqCst),
        0,
        "cycle: both nodes leaked (count never reaches 0)"
    );
    println!("strong cycle leaked 2 nodes (as expected; Miri/valgrind would report it)");
    let d = StdArc::new(AtomicUsize::new(0));
    {
        let (x, y) = (node(&d), node(&d));
        *x.strong_link.lock().unwrap() = Some(y.clone()); // parent -> child strong
        *y.weak_link.lock().unwrap() = Some(MyArc::downgrade(&x)); // child -> parent weak
        let up = y.weak_link.lock().unwrap().as_ref().unwrap().upgrade();
        assert!(up.is_some());
    }
    assert_eq!(
        d.load(Ordering::SeqCst),
        2,
        "with Weak back-pointer both nodes are freed"
    );
    println!("weak back-pointer: both nodes dropped, no leak");

    // 5. Send + Sync bound check at compile time
    fn is_send_sync<X: Send + Sync>() {}
    is_send_sync::<MyArc<u64>>();
    is_send_sync::<MyWeak<String>>();
    // is_send_sync::<MyArc<std::cell::Cell<u8>>>(); // would NOT compile: Cell is not Sync
    println!("ALL OK");
}
