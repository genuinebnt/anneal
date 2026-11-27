use solution::*;

#[allow(dead_code, unused_imports)]
use std::future::Future;
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::Arc;
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Wake, Waker};

/// A waker that counts how often it was woken.
#[allow(dead_code)]
struct Count(AtomicUsize);
impl Wake for Count {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
fn counter() -> (Arc<Count>, Waker) {
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    (count, waker)
}

#[allow(dead_code)]
fn woken(count: &Arc<Count>) -> usize {
    count.0.load(Ordering::SeqCst)
}

#[allow(dead_code)]
fn poll<F: Future + ?Sized>(f: &mut Pin<Box<F>>, waker: &Waker) -> Poll<F::Output> {
    f.as_mut().poll(&mut Context::from_waker(waker))
}

#[allow(dead_code)]
fn now<F: Future>(f: F) -> Poll<F::Output> {
    poll(&mut Box::pin(f), Waker::noop())
}

#[allow(dead_code)]
fn block_on<F: Future>(f: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::park();
    }
}

#[allow(dead_code)]
fn within<T: Send + 'static>(seconds: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(seconds))
        .expect("the test hung or panicked: a wake-up was probably lost")
}

#[test]
fn a_cancelled_acquire_does_not_strand_the_next() {
    let sem = Semaphore::new(1);
    let held = sem.try_acquire().unwrap();
    let (ca, wa) = counter();
    let (cb, wb) = counter();
    let mut a = Box::pin(sem.acquire());
    let mut b = Box::pin(sem.acquire());
    assert!(poll(&mut a, &wa).is_pending());
    assert!(poll(&mut b, &wb).is_pending());
    drop(held);
    assert!(woken(&ca) + woken(&cb) >= 1, "nobody was told about the free permit");
    drop(a);
    assert!(woken(&cb) >= 1, "a permit is free, one acquire waits, and it was never woken");
    assert!(poll(&mut b, &wb).is_ready(), "the survivor must get the permit");
}

#[test]
fn two_permits_returned_serve_two_waiters() {
    let sem = Semaphore::new(2);
    let h1 = sem.try_acquire().unwrap();
    let h2 = sem.try_acquire().unwrap();
    let wakers: Vec<_> = (0..2).map(|_| counter()).collect();
    let mut a = Box::pin(sem.acquire());
    let mut b = Box::pin(sem.acquire());
    assert!(poll(&mut a, &wakers[0].1).is_pending());
    assert!(poll(&mut b, &wakers[1].1).is_pending());
    drop(h1);
    drop(h2);
    // poll whoever has been woken; two permits are free, so both must get through, one after the other or at once
    let mut held = Vec::new();
    let mut waiting = [Some(a.as_mut()), Some(b.as_mut())];
    for _ in 0..2 {
        for (slot, (count, waker)) in waiting.iter_mut().zip(&wakers) {
            if woken(count) > 0 {
                if let Some(f) = slot {
                    if let Poll::Ready(permit) = f.as_mut().poll(&mut Context::from_waker(waker)) {
                        held.push(permit);
                        *slot = None;
                    }
                }
            }
        }
    }
    check!("waiters that got a permit", held.len(), 2);
    check!("none free", sem.available(), 0);
}

#[test]
fn a_dropped_woken_wait_passes_the_signal_on() {
    let signal = Signal::new();
    let (ca, wa) = counter();
    let (cb, wb) = counter();
    let mut a = Box::pin(signal.wait());
    let mut b = Box::pin(signal.wait());
    assert!(poll(&mut a, &wa).is_pending());
    assert!(poll(&mut b, &wb).is_pending());
    signal.signal();
    assert!(woken(&ca) + woken(&cb) >= 1);
    drop(a);
    assert!(woken(&cb) >= 1, "the signal is unused and the remaining wait was never woken");
    check!("the survivor uses it", poll(&mut b, &wb), Poll::Ready(()));
}

#[test]
fn a_signal_left_by_a_cancelled_wait_serves_a_later_one() {
    let signal = Signal::new();
    let (_c, w) = counter();
    let mut a = Box::pin(signal.wait());
    assert!(poll(&mut a, &w).is_pending());
    signal.signal();
    drop(a);
    check!("a new wait still sees it", now(signal.wait()), Poll::Ready(()));
}

#[test]
fn one_signal_completes_exactly_one_of_three() {
    let signal = Signal::new();
    let ws: Vec<_> = (0..3).map(|_| counter()).collect();
    let mut waits: Vec<_> = (0..3).map(|_| Box::pin(signal.wait())).collect();
    for (f, (_, w)) in waits.iter_mut().zip(&ws) {
        assert!(poll(f, w).is_pending());
    }
    signal.signal();
    let done = waits.iter_mut().zip(&ws).filter_map(|(f, (c, w))| (woken(c) > 0).then(|| poll(f, w))).filter(Poll::is_ready).count();
    check!("exactly one wait finished", done, 1);
    check!("nothing is left over", now(signal.wait()), Poll::Pending);
}

#[test]
fn the_latest_waker_is_used() {
    let sem = Semaphore::new(1);
    let held = sem.try_acquire().unwrap();
    let (_old, w_old) = counter();
    let (new, w_new) = counter();
    let mut a = Box::pin(sem.acquire());
    assert!(poll(&mut a, &w_old).is_pending());
    assert!(poll(&mut a, &w_new).is_pending());
    drop(held);
    assert!(woken(&new) >= 1, "semaphore: the newest waker was not woken");
    let signal = Signal::new();
    let (_old, w_old) = counter();
    let (new, w_new) = counter();
    let mut b = Box::pin(signal.wait());
    assert!(poll(&mut b, &w_old).is_pending());
    assert!(poll(&mut b, &w_new).is_pending());
    signal.signal();
    assert!(woken(&new) >= 1, "signal: the newest waker was not woken");
}

#[test]
fn repeated_polls_do_not_pile_up_wakers() {
    let sem = Semaphore::new(1);
    let _held = sem.try_acquire().unwrap();
    let (count, w) = counter();
    let mut a = Box::pin(sem.acquire());
    for _ in 0..1000 {
        assert!(poll(&mut a, &w).is_pending());
    }
    assert!(Arc::strong_count(&count) <= 4, "semaphore keeps {} references", Arc::strong_count(&count));
    let signal = Signal::new();
    let (count, w) = counter();
    let mut b = Box::pin(signal.wait());
    for _ in 0..1000 {
        assert!(poll(&mut b, &w).is_pending());
    }
    assert!(Arc::strong_count(&count) <= 4, "signal keeps {} references", Arc::strong_count(&count));
}

#[test]
fn types_and_futures_are_thread_safe() {
    fn both<T: Send + Sync>() {}
    fn send<T: Send>(_: T) {}
    both::<Semaphore>();
    both::<Signal>();
    let sem = Semaphore::new(1);
    send(sem.acquire());
    let signal = Signal::new();
    send(signal.wait());
}

#[test]
fn random_acquire_release_and_cancel() {
    let mut rng = anneal_prelude::Rng::new(110_301);
    for round in 0..400 {
        let n = 1 + rng.below(3);
        let sem = Semaphore::new(n);
        let mut held = Vec::new();
        type Waiting<'a> = (Pin<Box<dyn Future<Output = Permit<'a>> + 'a>>, Arc<Count>, Waker);
        let mut waiting: Vec<Waiting<'_>> = Vec::new();
        let mut history = Vec::new();
        for _ in 0..40 {
            match rng.below(4) {
                0 => {
                    let (count, waker) = counter();
                    let mut f: Pin<Box<dyn Future<Output = Permit<'_>> + '_>> = Box::pin(sem.acquire());
                    history.push("acquire");
                    match poll(&mut f, &waker) {
                        Poll::Ready(p) => held.push(p),
                        Poll::Pending => {
                            if waiting.is_empty() {
                                check!(format!("round {round} {history:?}: waited with a free permit"), held.len(), n);
                            }
                            waiting.push((f, count, waker));
                        }
                    }
                }
                1 if !held.is_empty() => {
                    history.push("release");
                    held.remove(rng.below(held.len()));
                }
                2 if !waiting.is_empty() => {
                    history.push("cancel");
                    waiting.remove(rng.below(waiting.len()));
                }
                _ => {}
            }
            // the progress rule: poll whoever was woken until nobody else can make progress
            while held.len() < n && !waiting.is_empty() {
                let mut progressed = false;
                let mut i = 0;
                while i < waiting.len() && !progressed {
                    if woken(&waiting[i].1) > 0 {
                        waiting[i].1 .0.store(0, Ordering::SeqCst);
                        let (f, _, w) = &mut waiting[i];
                        if let Poll::Ready(p) = poll(f, w) {
                            held.push(p);
                            waiting.remove(i);
                            progressed = true;
                            continue;
                        }
                    }
                    i += 1;
                }
                assert!(progressed, "round {round} {history:?}: {} of {n} permits held, {} waiting, and nobody woken can proceed", held.len(), waiting.len());
            }
            assert!(held.len() <= n, "round {round} {history:?}: too many permits");
            check!(format!("round {round} {history:?}: available"), sem.available(), n - held.len());
        }
    }
}

#[test]
fn threads_never_exceed_the_limit() {
    let peak = within(12, || {
        let sem = Arc::new(Semaphore::new(3));
        let inside = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (sem, inside, peak) = (sem.clone(), inside.clone(), peak.clone());
                std::thread::spawn(move || {
                    for _ in 0..2000 {
                        let permit = block_on(sem.acquire());
                        let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        std::thread::yield_now();
                        inside.fetch_sub(1, Ordering::SeqCst);
                        drop(permit);
                    }
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        peak.load(Ordering::SeqCst)
    });
    assert!(peak <= 3, "{peak} holders at once with 3 permits");
}

#[test]
fn signals_between_threads_are_not_lost() {
    within(12, || {
        let ping = Arc::new(Signal::new());
        let pong = Arc::new(Signal::new());
        let (a, b) = (ping.clone(), pong.clone());
        let other = std::thread::spawn(move || {
            for _ in 0..5000 {
                block_on(a.wait());
                b.signal();
            }
        });
        for _ in 0..5000 {
            ping.signal();
            block_on(pong.wait());
        }
        other.join().unwrap();
    });
}
