//! RECIPE 601 (interview): Smart pointers, interior mutability, reference cycles, Send/Sync and shared-state concurrency.
//! LEVEL: advanced
//! Use in: interview preparation, reviewing Rc/Arc/Mutex designs, explaining E0277 "cannot be sent between threads".   Run: cargo run --release --example rust_smart_pointers_send_sync_qa
//! COMMON USES: tree/graph structures, shared caches and config, worker pools, plugin registries (`Arc<dyn Trait>`), copy-on-write state.
#![allow(dead_code)]
//! Read: https://www.techinterview.org/post/3233474457/rust-interview-questions-2025-ownership-borrowing-lifetimes-traits-async-await-error-handling-smart-pointers-concurrency/
//! Read: https://www.techinterview.org/post/3233460429/rust-interview-questions/ and https://codeforgeek.com/rust-interview-questions/
//! Read: https://github.com/devinterview-io/rust-interview-questions and https://interviewing.io/rust-interview-questions
//!
//! QUESTION BANK
//! Q1. What is `Box<T>` for? Recursive types, trait objects, size. [Easy] [asked: very common]
//! Q2. Rc vs Arc: what is the difference and the cost? [Easy] [asked: very common]
//! Q3. Cell vs RefCell vs Mutex: interior mutability compared. [Medium] [asked: very common]
//! Q4. Can Rust leak memory? Show an Rc reference cycle and the Weak fix. [Medium] [asked: very common]
//! Q5. How do you build a tree with parent pointers using `Rc<RefCell<T>>` and `Weak`? [Medium] [asked: common]
//! Q6. Why is `Rc<T>` not `Send`? Why does `Arc<T>` work across threads? [Medium] [asked: very common]
//! Q7. Define Send and Sync. What is the relation `&T: Send` iff `T: Sync`? How do you opt out? [Hard] [asked: very common]
//! Q8. What is mutex poisoning and how do you recover? [Medium] [asked: common]
//! Q9. Mutex vs Condvar vs channels: when do you use each? [Medium] [asked: common]
//! Q10. Why is "wrap everything in `Arc<Mutex<T>>`" a design smell? [Medium] [asked: very common]
//! Q11. RwLock vs Mutex and the writer-starvation caveat. [Hard] [asked: occasional]
//! Q12. What do `Arc::make_mut` and `Arc::try_unwrap` do? [Hard] [asked: occasional]
//! Q13. What is `Cow` and when do you use it? [Medium] [asked: common]
//! Q14. `Box<dyn Trait>` vs `Rc<dyn Trait>` vs `Arc<dyn Trait + Send + Sync>`: which when? [Medium] [asked: common]
//! Q15. Why does `thread::spawn` need `'static + Send`, and what are scoped threads? [Medium] [asked: very common]

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::mem::size_of;
use std::panic::{self, AssertUnwindSafe};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;
use std::time::Instant;

/// Compile-time probes. `W::<T>::SEND` is true only when `T: Send` (inherent const wins when its
/// bounds hold; otherwise the blanket trait const is used). Same for Sync. This lets us ASSERT
/// negative facts such as "Rc is not Send" without a compile failure.
struct W<T>(PhantomData<T>);
trait NotSend {
    const SEND: bool = false;
}
impl<T> NotSend for W<T> {}
impl<T: Send> W<T> {
    const SEND: bool = true;
}
trait NotSync {
    const SYNC: bool = false;
}
impl<T> NotSync for W<T> {}
impl<T: Sync> W<T> {
    const SYNC: bool = true;
}

/// Run `f` with the panic message silenced (keeps the demo output clean) and restore the hook.
fn quiet<R>(f: impl FnOnce() -> R) -> R {
    let old = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let r = f();
    panic::set_hook(old);
    r
}

// ---------------------------------------------------------------------------
// Q1. Box
// ---------------------------------------------------------------------------
/// ANSWER: `Box<T>` is a single-owner pointer to a heap allocation; it is one machine word for
/// sized T (two for `Box<dyn Trait>` / `Box<[T]>`). Uses: (1) recursive types, because a type
/// containing itself directly has infinite size (E0072) while a Box has a fixed size; (2) trait
/// objects (`Box<dyn Trait>`); (3) moving large values without memcpy-ing them; (4) transferring
/// ownership of unsized data (`Box<str>`). Dropping the Box frees the heap memory; deref `*b` moves out.
///
/// REALLY TESTING: do you know why a recursive enum needs indirection and what the cost is (one alloc).
/// WRONG: "Box is faster than stack" (it is slower: allocation + indirection) or "Box is Rc".
/// FOLLOW-UP: what is the size of `Option<Box<T>>`? Same as `Box<T>` (null niche).
#[derive(Debug, PartialEq)]
enum List {
    Cons(i32, Box<List>),
    Nil,
}
// FAILING (E0072): enum Bad { Cons(i32, Bad), Nil }  // recursive type has infinite size
// help: insert some indirection (e.g., a `Box`)
fn sum_list(l: &List) -> i32 {
    match l {
        List::Cons(v, rest) => v + sum_list(rest),
        List::Nil => 0,
    }
}
fn q1_box() {
    use List::{Cons, Nil};
    let l = Cons(1, Box::new(Cons(2, Box::new(Cons(3, Box::new(Nil))))));
    assert_eq!(sum_list(&l), 6);
    assert_eq!(size_of::<Box<List>>(), size_of::<usize>());
    assert_eq!(size_of::<Box<i32>>(), size_of::<usize>());
    assert_eq!(
        size_of::<Box<dyn std::fmt::Debug>>(),
        2 * size_of::<usize>()
    ); // data + vtable
    assert_eq!(size_of::<Box<[u8]>>(), 2 * size_of::<usize>()); // ptr + len
    assert_eq!(size_of::<Option<Box<List>>>(), size_of::<Box<List>>()); // niche
    let b = Box::new([0u8; 1024]);
    let moved = *b; // deref-move out of the Box
    assert_eq!(moved.len(), 1024);
    let shown: Box<dyn std::fmt::Debug> = Box::new(42);
    assert_eq!(format!("{shown:?}"), "42");
}

// ---------------------------------------------------------------------------
// Q2. Rc vs Arc
// ---------------------------------------------------------------------------
/// ANSWER: Both are reference-counted shared ownership of immutable data. `Rc` updates its counters
/// with plain (non-atomic) increments: cheap, but not thread-safe, so the compiler makes it `!Send`
/// and `!Sync`. `Arc` uses atomic read-modify-write instructions: safe across threads, a bit slower
/// (contended cache line). `Rc::clone` copies a pointer and bumps a counter, never the data. Use
/// `Rc` single-threaded, `Arc` when the handle crosses threads. Both give only `&T` unless you add
/// interior mutability (`RefCell`/`Mutex`).
///
/// REALLY TESTING: whether you know clone is cheap and why two types exist.
/// WRONG: "Arc is always better/safer" (pay atomics for nothing) or "Rc::clone deep-copies".
/// FOLLOW-UP: `Rc::strong_count`, `Rc::ptr_eq`, `Rc::get_mut` (works only when unique).
fn q2_rc_vs_arc() {
    let a = Rc::new(String::from("shared"));
    assert_eq!(Rc::strong_count(&a), 1);
    let b = Rc::clone(&a);
    {
        let _c = a.clone();
        assert_eq!(Rc::strong_count(&a), 3);
    }
    assert_eq!(Rc::strong_count(&a), 2);
    assert!(Rc::ptr_eq(&a, &b));
    assert!(
        Rc::get_mut(&mut Rc::clone(&a)).is_none(),
        "shared: no unique access"
    );
    drop(b);
    assert_eq!(Rc::strong_count(&a), 1);

    let arc = Arc::new(vec![1, 2, 3]);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let arc = Arc::clone(&arc);
            thread::spawn(move || arc.iter().sum::<i32>())
        })
        .collect();
    let sums: Vec<i32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(sums, [6, 6, 6, 6]);
    assert_eq!(Arc::strong_count(&arc), 1, "all thread clones were dropped");
    assert_eq!(size_of::<Rc<i32>>(), size_of::<Arc<i32>>()); // same size, different counters
}

// ---------------------------------------------------------------------------
// Q3. Cell vs RefCell vs Mutex
// ---------------------------------------------------------------------------
/// ANSWER: Interior mutability = mutate through `&T`. `Cell<T>` moves/copies values in and out (no
/// references handed out; zero overhead; single thread). `RefCell<T>` hands out `Ref`/`RefMut`
/// guards and enforces the borrow rules at RUNTIME, panicking (or `try_borrow*` -> Err) on a
/// violation; single thread (`!Sync`). `Mutex<T>` is the thread-safe version: blocks other threads
/// instead of panicking, `Sync` when `T: Send`. Atomics are the lock-free option for integers.
///
/// REALLY TESTING: whether you know the checks move to runtime and what the failure mode is.
/// WRONG: "RefCell bypasses the borrow checker" (it enforces the same rules, later).
/// FOLLOW-UP: what happens on a double `borrow_mut` in a Mutex-like way? Panic vs deadlock.
fn q3_interior_mutability() {
    let c = Cell::new(1);
    c.set(c.get() + 1);
    assert_eq!(c.replace(10), 2);
    assert_eq!(c.get(), 10);

    let r = RefCell::new(vec![1]);
    r.borrow_mut().push(2);
    assert_eq!(r.borrow().len(), 2);
    {
        let _a = r.borrow();
        let _b = r.borrow(); // many shared: fine
        assert!(
            r.try_borrow_mut().is_err(),
            "writer refused while readers exist"
        );
    }
    // Runtime violation panics: both borrows are live at the same time.
    let caught = quiet(|| {
        panic::catch_unwind(AssertUnwindSafe(|| {
            let _w1 = r.borrow_mut();
            let _w2 = r.borrow_mut(); // panics: already borrowed (BorrowMutError)
        }))
    });
    assert!(caught.is_err());
    assert!(
        r.try_borrow_mut().is_ok(),
        "guards were dropped during unwinding"
    );

    let m = Mutex::new(0);
    *m.lock().unwrap() += 5;
    assert_eq!(*m.lock().unwrap(), 5);
    assert!(W::<Mutex<i32>>::SYNC && !W::<RefCell<i32>>::SYNC && !W::<Cell<i32>>::SYNC);
}

// ---------------------------------------------------------------------------
// Q4. reference cycles
// ---------------------------------------------------------------------------
/// ANSWER: Yes: leaking is SAFE in Rust (`mem::forget`, `Rc` cycles). If A holds an `Rc` to B and B
/// holds an `Rc` to A, both strong counts stay >= 1 after the last outside handle is dropped, so
/// neither destructor runs. The fix is to make the back-edge a `Weak<T>`: it does not keep the
/// value alive (`weak_count`, not `strong_count`) and `upgrade()` returns `None` once the value is gone.
///
/// REALLY TESTING: do you know memory safety != no leaks, and the Weak idiom (parent pointers, observers, caches).
/// WRONG: "Rust guarantees no memory leaks".
/// FOLLOW-UP: how do you detect cycles? Check strong counts in tests, `Rc::strong_count`, valgrind/leak sanitizers.
struct Cyclic {
    id: u32,
    other: RefCell<Option<Rc<Cyclic>>>,
    drops: Rc<Cell<u32>>,
}
impl Drop for Cyclic {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
struct Fixed {
    other: RefCell<Option<Weak<Fixed>>>,
    drops: Rc<Cell<u32>>,
}
impl Drop for Fixed {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn q4_cycles_and_weak() {
    // 1. Leaking cycle.
    let drops = Rc::new(Cell::new(0));
    {
        let a = Rc::new(Cyclic {
            id: 1,
            other: RefCell::new(None),
            drops: drops.clone(),
        });
        let b = Rc::new(Cyclic {
            id: 2,
            other: RefCell::new(None),
            drops: drops.clone(),
        });
        *a.other.borrow_mut() = Some(b.clone());
        *b.other.borrow_mut() = Some(a.clone());
        assert_eq!(Rc::strong_count(&a), 2); // `a` + b.other
        assert_eq!(Rc::strong_count(&b), 2);
    } // a and b go out of scope: counts fall to 1, never 0
    assert_eq!(drops.get(), 0, "cycle leaked: Drop never ran");

    // 2. Fixed with Weak on the back edge.
    let drops2 = Rc::new(Cell::new(0));
    {
        let a = Rc::new(Fixed {
            other: RefCell::new(None),
            drops: drops2.clone(),
        });
        let b = Rc::new(Fixed {
            other: RefCell::new(None),
            drops: drops2.clone(),
        });
        *a.other.borrow_mut() = Some(Rc::downgrade(&b)); // weak edge a -> b
        *b.other.borrow_mut() = Some(Rc::downgrade(&a)); // weak edge b -> a
        assert_eq!((Rc::strong_count(&a), Rc::weak_count(&a)), (1, 1));
        assert!(a.other.borrow().as_ref().unwrap().upgrade().is_some());
    }
    assert_eq!(drops2.get(), 2, "weak edges do not keep values alive");

    let weak: Weak<i32>;
    {
        let strong = Rc::new(7);
        weak = Rc::downgrade(&strong);
        assert_eq!(weak.upgrade().map(|r| *r), Some(7));
    }
    assert!(weak.upgrade().is_none());

    // mem::forget leaks on purpose and is safe.
    let d3 = Rc::new(Cell::new(0));
    std::mem::forget(Fixed {
        other: RefCell::new(None),
        drops: d3.clone(),
    });
    assert_eq!(d3.get(), 0);
}

// ---------------------------------------------------------------------------
// Q5. tree with parent pointers
// ---------------------------------------------------------------------------
/// ANSWER: Parents OWN children (`children: RefCell<Vec<Rc<TreeNode>>>`), children only OBSERVE the
/// parent (`parent: RefCell<Weak<TreeNode>>`). Ownership therefore flows one way (a DAG), so no
/// cycle and the whole tree drops when the root handle goes away. `RefCell` is needed because the
/// links are mutated through shared `Rc` handles. Alternatives: arena + indices (no refcount at all).
///
/// REALLY TESTING: practical use of Rc+RefCell+Weak together, and why the parent edge is weak.
/// WRONG: "both directions strong, it works" (leaks).
/// FOLLOW-UP: what is the borrow-panic risk with RefCell here? Holding a `borrow_mut` while recursing.
struct TreeNode {
    name: &'static str,
    parent: RefCell<Weak<TreeNode>>,
    children: RefCell<Vec<Rc<TreeNode>>>,
    drops: Rc<Cell<u32>>,
}
impl Drop for TreeNode {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn add_child(parent: &Rc<TreeNode>, name: &'static str, drops: &Rc<Cell<u32>>) -> Rc<TreeNode> {
    let child = Rc::new(TreeNode {
        name,
        parent: RefCell::new(Rc::downgrade(parent)),
        children: RefCell::new(vec![]),
        drops: drops.clone(),
    });
    parent.children.borrow_mut().push(child.clone());
    child
}
fn path_to_root(n: &Rc<TreeNode>) -> Vec<&'static str> {
    let mut out = vec![n.name];
    let mut cur = n.parent.borrow().upgrade();
    while let Some(p) = cur {
        out.push(p.name);
        cur = p.parent.borrow().upgrade();
    }
    out
}
fn q5_tree_with_weak_parent() {
    let drops = Rc::new(Cell::new(0));
    let root = Rc::new(TreeNode {
        name: "root",
        parent: RefCell::new(Weak::new()),
        children: RefCell::new(vec![]),
        drops: drops.clone(),
    });
    let mid = add_child(&root, "mid", &drops);
    let leaf = add_child(&mid, "leaf", &drops);
    assert_eq!(path_to_root(&leaf), ["leaf", "mid", "root"]);
    assert_eq!((Rc::strong_count(&root), Rc::weak_count(&root)), (1, 1));
    assert_eq!((Rc::strong_count(&mid), Rc::weak_count(&mid)), (2, 1)); // root.children + `mid`
    drop(leaf);
    drop(mid);
    assert_eq!(drops.get(), 0, "still owned by the tree");
    drop(root);
    assert_eq!(drops.get(), 3, "dropping the root frees the whole tree");
}

// ---------------------------------------------------------------------------
// Q6. why Rc is not Send
// ---------------------------------------------------------------------------
/// ANSWER: `Rc`'s counter is a plain `usize`. If two threads each held a clone and cloned/dropped
/// concurrently, increments could be lost (non-atomic read-modify-write is a data race), so the
/// count could reach zero too early (use-after-free) or never (leak). `Send` means "ownership may be
/// transferred to another thread"; because `Rc` handles share one counter, moving one handle to
/// another thread would be a race, so `Rc: !Send` (and `!Sync`). `Arc` uses atomic counters, so
/// `Arc<T>: Send + Sync` IF `T: Send + Sync`. The compiler error is E0277:
/// "`Rc<i32>` cannot be sent between threads safely".
///
/// REALLY TESTING: do you understand the *reason*, not just the error message.
/// WRONG: "Rc is not Send because it is not thread-safe" (circular) or "Rc is slow".
/// FOLLOW-UP: is `Arc<RefCell<T>>` Send? No: `RefCell` is `!Sync`, so Arc<RefCell<T>> is `!Send`.
fn q6_rc_not_send() {
    // FAILING (E0277):
    //   let rc = Rc::new(5);
    //   thread::spawn(move || println!("{rc}"));
    //   // error[E0277]: `Rc<i32>` cannot be sent between threads safely
    //   // = help: the trait `Send` is not implemented for `Rc<i32>`
    //   // note: required because it's used within this closure ... required by `thread::spawn`
    assert!(!W::<Rc<i32>>::SEND && !W::<Rc<i32>>::SYNC);
    assert!(W::<Arc<i32>>::SEND && W::<Arc<i32>>::SYNC);
    assert!(
        !W::<Arc<RefCell<i32>>>::SEND,
        "Arc<T> is Send only if T: Send + Sync"
    );
    assert!(
        W::<Arc<Mutex<RefCell<i32>>>>::SEND,
        "Mutex adds Sync for Send payloads"
    );
    // FIXED: Arc, shared counter correct under contention.
    let counter = Arc::new(AtomicU64::new(0));
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let c = Arc::clone(&counter);
            thread::spawn(move || {
                for _ in 0..1000 {
                    let _tmp = Arc::clone(&c); // atomic clone/drop of the Arc itself
                    c.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    assert_eq!(counter.load(Ordering::SeqCst), 8000);
    assert_eq!(
        Arc::strong_count(&counter),
        1,
        "no lost updates to the refcount"
    );
}

// ---------------------------------------------------------------------------
// Q7. Send and Sync
// ---------------------------------------------------------------------------
/// ANSWER: `T: Send` = a value of T may be MOVED to another thread. `T: Sync` = `&T` may be SHARED
/// between threads; formally `T: Sync` iff `&T: Send`. Both are auto traits (derived structurally
/// from fields) and `unsafe` to implement manually. Raw pointers are neither; `Cell/RefCell` are
/// Send but !Sync; `Rc` is neither; `MutexGuard` is !Send (a lock must be released by the thread
/// that took it, on some platforms); `Mutex<T>: Sync` when `T: Send`. To opt a type out add a
/// field like `PhantomData<*const ()>` (raw pointer is !Send + !Sync). To opt in, `unsafe impl Send`
/// with a SAFETY argument.
///
/// REALLY TESTING: the precise definitions and the `&T: Send` relation.
/// WRONG: "Sync means thread-safe" (too vague) / "Send means it can be shared".
/// FOLLOW-UP: why is `Cell<i32>: Send` but not `Sync`? Moving it is fine; sharing `&Cell` lets two threads
/// call `set` concurrently (race).
struct NotThreadSafe {
    v: i32,
    _opt_out: PhantomData<*const ()>,
}
struct RawHandle(*mut u8);
// SAFETY (demonstration only): the pointer is never dereferenced and is owned uniquely.
unsafe impl Send for RawHandle {}
fn q7_send_sync() {
    assert!(W::<i32>::SEND && W::<i32>::SYNC);
    assert!(W::<Cell<i32>>::SEND && !W::<Cell<i32>>::SYNC);
    assert!(W::<RefCell<i32>>::SEND && !W::<RefCell<i32>>::SYNC);
    assert!(!W::<*const i32>::SEND && !W::<*const i32>::SYNC);
    assert!(
        !W::<std::sync::MutexGuard<'static, i32>>::SEND,
        "guards stay on their thread"
    );
    assert!(
        W::<Mutex<Cell<i32>>>::SYNC,
        "Mutex<T>: Sync when T: Send, even if T is !Sync"
    );
    assert!(!W::<NotThreadSafe>::SEND && !W::<NotThreadSafe>::SYNC);
    assert!(W::<RawHandle>::SEND && !W::<RawHandle>::SYNC);
    // The relation: &T: Send  <=>  T: Sync
    assert_eq!(W::<&i32>::SEND, W::<i32>::SYNC);
    assert_eq!(W::<&Cell<i32>>::SEND, W::<Cell<i32>>::SYNC);
    assert_eq!(W::<&RefCell<i32>>::SEND, false);
    assert!(W::<&mut Cell<i32>>::SEND, "&mut T: Send iff T: Send");
    let _ = NotThreadSafe {
        v: 1,
        _opt_out: PhantomData,
    };
}

// ---------------------------------------------------------------------------
// Q8. poisoning
// ---------------------------------------------------------------------------
/// ANSWER: If a thread panics while holding a `MutexGuard`, the mutex is marked POISONED: later
/// `lock()` returns `Err(PoisonError)` because the protected data may be half-updated. The error still
/// carries the guard: `into_inner()` (on the error) recovers it so you can repair or accept the data;
/// `clear_poison()` resets the flag. `lock().unwrap()` therefore turns one panic into a cascade of
/// panics in every other thread (often intended: fail fast; but be deliberate). `parking_lot::Mutex`
/// does not poison. `RwLock` poisons only on a panicking WRITER.
///
/// REALLY TESTING: failure-handling thinking in concurrent code.
/// WRONG: "poisoning means deadlock" or "the lock stays locked" (it is released during unwind).
/// FOLLOW-UP: is `into_inner()` on the Mutex itself the same? That one consumes the mutex.
fn q8_poisoning() {
    let m = Arc::new(Mutex::new(vec![1, 2, 3]));
    let m2 = Arc::clone(&m);
    let joined = quiet(|| {
        thread::spawn(move || {
            let mut g = m2.lock().unwrap();
            g.push(4); // half-finished update
            panic!("boom while locked");
        })
        .join()
    });
    assert!(joined.is_err(), "the thread panicked");
    assert!(m.is_poisoned());
    match m.lock() {
        Ok(_) => panic!("must be poisoned"),
        Err(poisoned) => {
            let mut guard = poisoned.into_inner(); // recover the data anyway
            assert_eq!(*guard, [1, 2, 3, 4]);
            guard.push(5);
        }
    }
    assert!(m.is_poisoned(), "flag stays set until cleared");
    m.clear_poison();
    assert!(!m.is_poisoned());
    assert_eq!(m.lock().unwrap().len(), 5);
    // Ignoring poison in one line:
    let v = m.lock().unwrap_or_else(|e| e.into_inner()).len();
    assert_eq!(v, 5);
}

// ---------------------------------------------------------------------------
// Q9. Mutex vs Condvar vs channels
// ---------------------------------------------------------------------------
/// ANSWER: `Mutex` protects shared data (mutual exclusion). `Condvar` lets a thread SLEEP until a
/// condition over that data holds (it is always paired with a Mutex and re-checked in a loop /
/// `wait_while` because of spurious wakeups). Channels (`mpsc`, crossbeam) transfer OWNERSHIP of
/// messages between threads: no shared state, natural for pipelines and worker pools ("share memory
/// by communicating"). Rule of thumb: stream of work => channel; small shared state/counters =>
/// Mutex/atomic; wait for a state change => Condvar (or a channel).
///
/// REALLY TESTING: choosing the right primitive, spurious wakeup awareness.
/// WRONG: "Condvar alone" (it holds no state), "if instead of while around wait".
/// FOLLOW-UP: what does dropping all `Sender`s do? `recv` returns Err: the natural shutdown signal.
fn q9_mutex_condvar_channel() {
    // Condvar: wait until a flag is set.
    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    let p2 = Arc::clone(&pair);
    let t = thread::spawn(move || {
        let (lock, cv) = &*p2;
        *lock.lock().unwrap() = true;
        cv.notify_one();
    });
    {
        let (lock, cv) = &*pair;
        let guard = cv
            .wait_while(lock.lock().unwrap(), |ready| !*ready)
            .unwrap();
        assert!(*guard);
    }
    t.join().unwrap();

    // Channel: ownership moves, shutdown by dropping the sender.
    let (tx, rx) = mpsc::channel::<String>();
    let workers: Vec<_> = (0..3)
        .map(|i| {
            let tx = tx.clone();
            thread::spawn(move || tx.send(format!("job{i}")).unwrap())
        })
        .collect();
    drop(tx); // the original sender; clones end with their threads
    for w in workers {
        w.join().unwrap();
    }
    let mut got: Vec<String> = rx.iter().collect(); // ends when every Sender is dropped
    got.sort();
    assert_eq!(got, ["job0", "job1", "job2"]);
}

// ---------------------------------------------------------------------------
// Q10. Arc<Mutex<T>> everywhere
// ---------------------------------------------------------------------------
/// ANSWER: It is not WRONG, it is a smell when it is the default. Costs: lock contention serialises
/// threads; lock ordering => deadlocks; coarse locks hide the data flow; poisoning/unwrap noise;
/// every access pays for synchronisation. Better: give each piece of data ONE owner (a thread/task)
/// and communicate by messages; partition data so threads work on disjoint chunks and merge results
/// (fold/reduce, rayon); use atomics for counters; keep lock scopes tiny and never hold them across
/// slow work or `.await`; prefer immutable `Arc<T>` snapshots with `make_mut`/swap for read-mostly data.
///
/// REALLY TESTING: architectural judgement, not syntax.
/// WRONG: "Mutex is slow so avoid it" (uncontended Mutex is cheap; contention is the issue).
/// FOLLOW-UP: show a measurement. (The timings below are printed, not asserted: they vary by machine.)
fn q10_arc_mutex_smell() {
    const THREADS: usize = 4;
    const PER: u64 = 200_000;

    let t0 = Instant::now();
    let shared = Arc::new(Mutex::new(0u64));
    let hs: Vec<_> = (0..THREADS)
        .map(|_| {
            let s = Arc::clone(&shared);
            thread::spawn(move || {
                for _ in 0..PER {
                    *s.lock().unwrap() += 1; // lock per increment: heavy contention
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    let mutex_t = t0.elapsed();
    assert_eq!(*shared.lock().unwrap(), THREADS as u64 * PER);

    let t1 = Instant::now();
    let atomic = Arc::new(AtomicU64::new(0));
    let hs: Vec<_> = (0..THREADS)
        .map(|_| {
            let a = Arc::clone(&atomic);
            thread::spawn(move || {
                for _ in 0..PER {
                    a.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    let atomic_t = t1.elapsed();
    assert_eq!(atomic.load(Ordering::SeqCst), THREADS as u64 * PER);

    // Ownership-based: each thread keeps a private total and returns it (no sharing during the work).
    let t2 = Instant::now();
    let hs: Vec<_> = (0..THREADS)
        .map(|_| {
            thread::spawn(move || {
                let mut local = 0u64;
                for _ in 0..PER {
                    local += 1;
                }
                local
            })
        })
        .collect();
    let total: u64 = hs.into_iter().map(|h| h.join().unwrap()).sum();
    let local_t = t2.elapsed();
    assert_eq!(total, THREADS as u64 * PER);
    println!(
        "  [info] contention ({THREADS} threads x {PER}): Mutex per op {mutex_t:?} | atomic {atomic_t:?} | thread-local + merge {local_t:?}"
    );
}

// ---------------------------------------------------------------------------
// Q11. RwLock
// ---------------------------------------------------------------------------
/// ANSWER: `RwLock` allows many concurrent readers OR one writer. Wins when reads dominate and the
/// critical section is long enough to amortise the heavier lock. Caveats: the fairness policy is
/// platform-defined, so a steady stream of readers can STARVE a writer (or, on writer-preferring
/// implementations, a pending writer blocks new readers, and recursive read-locking from one thread can
/// deadlock). A plain Mutex is often faster for short sections. Never upgrade a read guard to a
/// write by locking again (self-deadlock); drop the read guard first.
///
/// REALLY TESTING: nuance beyond "RwLock is a faster Mutex".
/// WRONG: "RwLock is always better for read-heavy loads".
/// FOLLOW-UP: alternatives: `arc-swap`/`Arc` snapshot swapping, sharded locks.
fn q11_rwlock() {
    let l = RwLock::new(5);
    {
        let r1 = l.read().unwrap();
        let r2 = l.read().unwrap(); // many readers
        assert_eq!(*r1 + *r2, 10);
        assert!(
            l.try_write().is_err(),
            "writer must wait for all readers to leave"
        );
    }
    *l.write().unwrap() += 1;
    {
        let _w = l.write().unwrap();
        assert!(l.try_read().is_err(), "readers wait for the writer");
    }
    assert_eq!(*l.read().unwrap(), 6);
    assert!(
        W::<RwLock<Cell<i32>>>::SYNC == false,
        "RwLock<T>: Sync needs T: Send + Sync (readers share &T)"
    );
}

// ---------------------------------------------------------------------------
// Q12. Arc::make_mut / try_unwrap
// ---------------------------------------------------------------------------
/// ANSWER: `Arc::make_mut(&mut arc)` gives `&mut T`: if the Arc is unique it mutates in place; if
/// other strong handles exist it CLONES the data first, so the others keep the old snapshot
/// (clone-on-write). `Arc::try_unwrap(arc)` returns `Ok(T)` only if this is the last strong handle
/// (else gives the Arc back as `Err`); `Arc::into_inner` is the race-free variant when several
/// handles are dropped concurrently. `Arc::get_mut` is the non-cloning check.
///
/// REALLY TESTING: familiarity with copy-on-write and persistent-snapshot designs.
/// WRONG: "make_mut mutates the shared data" (it never mutates shared data).
/// FOLLOW-UP: which of Rc/Arc offers the same? Both (`Rc::make_mut`).
fn q12_make_mut_try_unwrap() {
    let mut a = Arc::new(vec![1, 2, 3]);
    let ptr_before = Arc::as_ptr(&a);
    Arc::make_mut(&mut a).push(4); // unique: in place
    assert_eq!(Arc::as_ptr(&a), ptr_before);

    let snapshot = Arc::clone(&a);
    Arc::make_mut(&mut a).push(5); // shared: clone first
    assert_ne!(Arc::as_ptr(&a), Arc::as_ptr(&snapshot));
    assert_eq!(*snapshot, [1, 2, 3, 4], "old readers keep their snapshot");
    assert_eq!(*a, [1, 2, 3, 4, 5]);

    let shared = Arc::clone(&a);
    let back = Arc::try_unwrap(a).expect_err("another handle exists");
    drop(shared);
    let v = Arc::try_unwrap(back).expect("now unique");
    assert_eq!(v.len(), 5);
    assert!(Arc::get_mut(&mut Arc::new(1)).is_some());
}

// ---------------------------------------------------------------------------
// Q13. Cow
// ---------------------------------------------------------------------------
/// ANSWER: `Cow<'a, B>` is "clone on write": an enum `Borrowed(&'a B) | Owned(B::Owned)`. A function
/// that usually returns its input unchanged but sometimes needs to modify it can return `Cow<str>`
/// and allocate only on the modifying path. `to_mut()` clones once on first mutation; `into_owned()`
/// forces ownership. Typical: escaping, normalising, path/lossy UTF-8 (`String::from_utf8_lossy`).
///
/// REALLY TESTING: allocation awareness.
/// WRONG: "Cow is a smart pointer like Rc" (it is an enum, not shared ownership).
/// FOLLOW-UP: Cow in struct fields to accept both `&'static str` and `String` (`Cow<'static, str>`).
fn normalise(s: &str) -> Cow<'_, str> {
    if s.contains(' ') {
        Cow::Owned(s.replace(' ', "_"))
    } else {
        Cow::Borrowed(s)
    }
}
fn q13_cow() {
    let clean = normalise("already_clean");
    let dirty = normalise("needs fixing");
    assert!(matches!(clean, Cow::Borrowed(_)), "no allocation happened");
    assert!(matches!(dirty, Cow::Owned(_)));
    assert_eq!(dirty, "needs_fixing");
    let mut c: Cow<str> = Cow::Borrowed("abc");
    c.to_mut().push('d'); // first write clones
    assert!(matches!(c, Cow::Owned(_)));
    assert_eq!(c.into_owned(), "abcd");
    assert!(matches!(String::from_utf8_lossy(b"ok"), Cow::Borrowed(_)));
    assert!(matches!(
        String::from_utf8_lossy(&[0xff, b'a']),
        Cow::Owned(_)
    ));
}

// ---------------------------------------------------------------------------
// Q14. Box<dyn> vs Rc<dyn> vs Arc<dyn + Send + Sync>
// ---------------------------------------------------------------------------
/// ANSWER: All three are fat pointers (data ptr + vtable ptr) to a type-erased value. `Box<dyn T>`:
/// unique ownership, single place (heterogeneous Vec, plugin owned by one manager). `Rc<dyn T>`:
/// shared ownership in ONE thread. `Arc<dyn T + Send + Sync>`: shared across threads; the `Send + Sync`
/// bounds must be written (the `dyn Trait` alone says nothing about thread safety), otherwise
/// `thread::spawn` rejects it (E0277). `Arc<dyn Trait>` is built by unsizing coercion from `Arc<Concrete>`.
///
/// REALLY TESTING: type erasure plus thread-safety bounds on trait objects.
/// WRONG: "dyn Trait is automatically Send".
/// FOLLOW-UP: `dyn Trait` is `'static` by default inside Box/Arc (`Box<dyn Trait + 'static>`).
trait Greeter {
    fn greet(&self) -> String;
}
struct En;
struct Fr(String);
impl Greeter for En {
    fn greet(&self) -> String {
        "hello".into()
    }
}
impl Greeter for Fr {
    fn greet(&self) -> String {
        format!("bonjour {}", self.0)
    }
}
fn q14_trait_object_pointers() {
    let boxed: Vec<Box<dyn Greeter>> = vec![Box::new(En), Box::new(Fr("a".into()))];
    assert_eq!(
        boxed.iter().map(|g| g.greet()).collect::<Vec<_>>(),
        ["hello", "bonjour a"]
    );

    let rc: Rc<dyn Greeter> = Rc::new(En);
    let rc2 = rc.clone();
    assert_eq!((rc.greet(), Rc::strong_count(&rc2)), ("hello".into(), 2));

    let arc: Arc<dyn Greeter + Send + Sync> = Arc::new(Fr("b".into()));
    let a2 = Arc::clone(&arc);
    let out = thread::spawn(move || a2.greet()).join().unwrap();
    assert_eq!(out, "bonjour b");
    // FAILING (E0277): let plain: Arc<dyn Greeter> = Arc::new(En);
    //                  thread::spawn(move || plain.greet());   // `dyn Greeter` cannot be shared between threads safely
    assert!(!W::<Arc<dyn Greeter>>::SEND && W::<Arc<dyn Greeter + Send + Sync>>::SEND);
    assert_eq!(
        size_of::<Arc<dyn Greeter + Send + Sync>>(),
        2 * size_of::<usize>()
    );
    assert_eq!(size_of::<Box<dyn Greeter>>(), 2 * size_of::<usize>());

    let mut registry: HashMap<&str, Arc<dyn Greeter + Send + Sync>> = HashMap::new();
    registry.insert("en", Arc::new(En));
    assert_eq!(registry["en"].greet(), "hello");
}

// ---------------------------------------------------------------------------
// Q15. thread::spawn bounds, scoped threads
// ---------------------------------------------------------------------------
/// ANSWER: A spawned thread can outlive the function that created it, so `F: Send + 'static` and
/// the return type `T: Send + 'static`: the closure must own everything it uses (`move`, Arc clones)
/// and everything must be transferable. Borrowing a local yields E0373 ("closure may outlive the
/// current function"). `thread::scope` guarantees all spawned threads are joined before the scope
/// returns, so closures may BORROW locals (even `&mut` to disjoint data); they still need `Send`.
/// `'static` here means "owns its data", not "lives forever".
///
/// REALLY TESTING: connects lifetimes with threads.
/// WRONG: "spawn needs 'static because threads run forever".
/// FOLLOW-UP: what if a scoped thread panics? `scope` panics after joining all of them.
fn q15_spawn_bounds_and_scope() {
    // FAILING (E0373):
    //   let v = vec![1, 2, 3];
    //   let h = thread::spawn(|| println!("{:?}", v));
    //   // error[E0373]: closure may outlive the current function, but it borrows `v`
    //   // help: to force the closure to take ownership of `v`, use the `move` keyword
    let v = vec![1, 2, 3];
    let h = thread::spawn(move || v.len()); // FIXED: move ownership in
    assert_eq!(h.join().unwrap(), 3);

    // Scoped: borrow, no Arc, no clone, no 'static.
    let mut left = vec![1, 2, 3, 4];
    let right = vec![10, 20];
    let counter = Mutex::new(0);
    let (a, b) = left.split_at_mut(2);
    thread::scope(|s| {
        s.spawn(|| {
            a[0] += right[0]; // shared borrow of `right`
            *counter.lock().unwrap() += 1;
        });
        s.spawn(|| {
            b[0] += right[1]; // disjoint &mut
            *counter.lock().unwrap() += 1;
        });
    });
    assert_eq!(left, [11, 2, 23, 4]);
    assert_eq!(counter.into_inner().unwrap(), 2);
    // Moving a non-Send value in is still rejected, even in a scope:
    assert!(!W::<Rc<i32>>::SEND);
}

fn main() {
    q1_box();
    println!("Q1: ok");
    q2_rc_vs_arc();
    println!("Q2: ok");
    q3_interior_mutability();
    println!("Q3: ok");
    q4_cycles_and_weak();
    println!("Q4: ok");
    q5_tree_with_weak_parent();
    println!("Q5: ok");
    q6_rc_not_send();
    println!("Q6: ok");
    q7_send_sync();
    println!("Q7: ok");
    q8_poisoning();
    println!("Q8: ok");
    q9_mutex_condvar_channel();
    println!("Q9: ok");
    q10_arc_mutex_smell();
    println!("Q10: ok");
    q11_rwlock();
    println!("Q11: ok");
    q12_make_mut_try_unwrap();
    println!("Q12: ok");
    q13_cow();
    println!("Q13: ok");
    q14_trait_object_pointers();
    println!("Q14: ok");
    q15_spawn_bounds_and_scope();
    println!("Q15: ok");
}
