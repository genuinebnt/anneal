use solution::*;

#[test]
fn get_mut_without_locking() {
    check!(r#"&mut access"#, { let mut l = SpinLock::new(1); *l.get_mut() = 8; l.into_inner() }, 8);
}

#[test]
fn try_lock_guard_gives_access() {
    check!(r#"try_lock succeeds and mutates"#, { let l = SpinLock::new(1); if let Some(mut g) = l.try_lock() { *g = 6; } let v = *l.lock(); v }, 6);
}

#[test]
fn two_locks_independent() {
    check!(r#"holding one does not block the other"#, { let a = SpinLock::new(1); let b = SpinLock::new(2); let _ga = a.lock(); let v = *b.lock(); v }, 2);
}

#[test]
fn guard_derefs_to_methods() {
    check!(r#"vec methods through the guard"#, { let l = SpinLock::new(Vec::new()); l.lock().push(3); l.lock().push(4); let n = l.lock().len(); let second = l.lock()[1]; (n, second) }, (2, 4));
}

#[test]
fn value_dropped_when_lock_dropped() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    struct D(Arc<AtomicUsize>);
    impl Drop for D {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let n = Arc::new(AtomicUsize::new(0));
    drop(SpinLock::new(D(n.clone())));
    check!("drop a lock holding a value", n.load(Ordering::SeqCst), 1);
}

#[test]
fn plain_counter_is_exact_across_threads() {
    let l = std::sync::Arc::new(SpinLock::new(0u64));
    let hs: Vec<_> = (0..8)
        .map(|_| {
            let l = l.clone();
            std::thread::spawn(move || {
                for _ in 0..50_000 {
                    *l.lock() += 1;
                }
            })
        })
        .collect();
    for h in hs {
        h.join().unwrap();
    }
    let total = *l.lock();
    check!("8 threads x 50000 increments of a plain u64", total, 400_000);
}

#[test]
fn long_critical_sections_stay_exclusive() {
    // each holder writes two fields in two steps; an observer that sees them differ broke mutual exclusion
    let l = std::sync::Arc::new(SpinLock::new((0u64, 0u64)));
    let hs: Vec<_> = (0..4)
        .map(|_| {
            let l = l.clone();
            std::thread::spawn(move || {
                let mut torn = 0;
                for _ in 0..20_000 {
                    let mut g = l.lock();
                    if g.0 != g.1 {
                        torn += 1;
                    }
                    g.0 += 1;
                    std::hint::spin_loop();
                    g.1 += 1;
                }
                torn
            })
        })
        .collect();
    let torn: u32 = hs.into_iter().map(|h| h.join().unwrap()).sum();
    let g = l.lock();
    check!("4 threads, two-step updates", (torn, g.0, g.1), (0, 80_000, 80_000));
}

#[test]
fn try_lock_contention_never_double_grants() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let l = Arc::new(SpinLock::new(()));
    let inside = Arc::new(AtomicUsize::new(0));
    let overlap = Arc::new(AtomicUsize::new(0));
    let hs: Vec<_> = (0..6)
        .map(|_| {
            let (l, inside, overlap) = (l.clone(), inside.clone(), overlap.clone());
            std::thread::spawn(move || {
                let mut got = 0u32;
                for _ in 0..30_000 {
                    if let Some(_g) = l.try_lock() {
                        if inside.fetch_add(1, Ordering::SeqCst) != 0 {
                            overlap.fetch_add(1, Ordering::SeqCst);
                        }
                        got += 1;
                        inside.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                got
            })
        })
        .collect();
    let got: u32 = hs.into_iter().map(|h| h.join().unwrap()).sum();
    check!("six threads try_lock in a loop: overlapping holders", overlap.load(Ordering::SeqCst), 0);
    assert!(got > 0, "nobody ever got the lock");
}

#[test]
fn works_with_non_sync_payload() {
    // Cell<u32> is Send but not Sync: SpinLock<Cell<u32>> must still be Sync (T: Send is the bound)
    fn needs_sync<T: Sync>() {}
    needs_sync::<SpinLock<std::cell::Cell<u32>>>();
    needs_sync::<SpinLock<Vec<String>>>();
}

/// `Is::<T>::SYNC` / `Is::<T>::SEND` are `true` only when `T` has the trait: the inherent const wins when its bound
/// holds, the trait's default otherwise. That lets a test check that a type is *not* `Sync` or `Send`.
#[allow(dead_code)]
struct Is<T: ?Sized>(std::marker::PhantomData<T>);
trait Fallback {
    const SYNC: bool = false;
    const SEND: bool = false;
}
impl<T: ?Sized> Fallback for Is<T> {}
#[allow(dead_code)]
impl<T: ?Sized + Sync> Is<T> {
    const SYNC: bool = true;
}
#[allow(dead_code)]
impl<T: ?Sized + Send> Is<T> {
    const SEND: bool = true;
}

#[test]
fn not_sync_or_send_for_the_wrong_types() {
    use std::cell::Cell;
    use std::rc::Rc;
    // An Rc inside the lock could be cloned under the lock and the clone kept, so it must stay on one thread.
    check!(
        "(SpinLock<Rc<u8>>: Sync, SpinLock<Rc<u8>>: Send, SpinLock<Cell<u32>>: Sync)",
        (Is::<SpinLock<Rc<u8>>>::SYNC, Is::<SpinLock<Rc<u8>>>::SEND, Is::<SpinLock<Cell<u32>>>::SYNC),
        (false, false, true)
    );
}

#[test]
fn guard_is_sync_only_for_sync_values() {
    use std::cell::Cell;
    // `&Guard` derefs to `&T`, so sharing a guard between threads shares `&T`: that needs `T: Sync`.
    check!(
        "(Guard<Cell<u32>>: Sync, Guard<u64>: Sync)",
        (Is::<Guard<'static, Cell<u32>>>::SYNC, Is::<Guard<'static, u64>>::SYNC),
        (false, true)
    );
}
