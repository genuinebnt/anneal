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

#[test]
fn only_n_permits_exist() {
    let sem = Semaphore::new(2);
    let a = now(sem.acquire());
    let b = now(sem.acquire());
    assert!(a.is_ready() && b.is_ready(), "two permits should be free at once");
    check!("none left", sem.available(), 0);
    assert!(now(sem.acquire()).is_pending(), "a third permit appeared");
}

#[test]
fn dropping_a_permit_wakes_a_waiter() {
    let sem = Semaphore::new(1);
    let held = sem.try_acquire().expect("one permit");
    let (count, waker) = counter();
    let mut waiting = Box::pin(sem.acquire());
    assert!(poll(&mut waiting, &waker).is_pending());
    check!("not woken yet", woken(&count), 0);
    drop(held);
    assert!(woken(&count) >= 1, "the waiter was not woken");
    assert!(poll(&mut waiting, &waker).is_ready(), "the woken waiter should get the permit");
}

#[test]
fn a_signal_sent_early_is_remembered() {
    let signal = Signal::new();
    signal.signal();
    check!("ready at once", now(signal.wait()), Poll::Ready(()));
    check!("used up", now(signal.wait()), Poll::Pending);
}

#[test]
fn signals_merge_while_unused() {
    let signal = Signal::new();
    signal.signal();
    signal.signal();
    signal.signal();
    check!("one wait finishes", now(signal.wait()), Poll::Ready(()));
    check!("a second does not", now(signal.wait()), Poll::Pending);
}

#[test]
fn a_signal_wakes_a_waiting_task() {
    let signal = Signal::new();
    let (count, waker) = counter();
    let mut wait = Box::pin(signal.wait());
    assert!(poll(&mut wait, &waker).is_pending());
    signal.signal();
    assert!(woken(&count) >= 1, "the waiting task was not woken");
    check!("and ready", poll(&mut wait, &waker), Poll::Ready(()));
}

#[test]
fn try_acquire_never_waits() {
    let sem = Semaphore::new(1);
    let first = sem.try_acquire();
    assert!(first.is_some());
    assert!(sem.try_acquire().is_none());
    drop(first);
    check!("back", sem.available(), 1);
    assert!(sem.try_acquire().is_some());
}
