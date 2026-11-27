use solution::*;

#[allow(dead_code, unused_imports)]
use std::collections::VecDeque;
#[allow(dead_code, unused_imports)]
use std::io;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Wake, Waker};

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
    (count.clone(), Waker::from(count))
}

#[allow(dead_code)]
fn woken(count: &Arc<Count>) -> usize {
    count.0.load(Ordering::SeqCst)
}

#[allow(dead_code)]
fn would_block<T>() -> io::Result<T> {
    Err(io::ErrorKind::WouldBlock.into())
}

/// Polls once; `Some(value)` when ready. Errors are shown by their kind.
#[allow(dead_code)]
fn attempt<T>(r: &Readiness, waker: &Waker, op: impl FnMut() -> io::Result<T>) -> Option<Result<T, io::ErrorKind>> {
    match poll_io(r, &mut Context::from_waker(waker), op) {
        Poll::Pending => None,
        Poll::Ready(result) => Some(result.map_err(|e| e.kind())),
    }
}

#[test]
fn success_is_ready_at_once() {
    let r = Readiness::new();
    check!("no ready() needed", attempt(&r, Waker::noop(), || Ok(5)), Some(Ok(5)));
}

#[test]
fn other_errors_come_out() {
    let r = Readiness::new();
    let got = attempt::<u8>(&r, Waker::noop(), || Err(io::ErrorKind::BrokenPipe.into()));
    check!("the error", got, Some(Err(io::ErrorKind::BrokenPipe)));
}

#[test]
fn nothing_yet_is_pending_and_later_news_wakes() {
    let r = Readiness::new();
    let (count, waker) = counter();
    check!("pending", attempt::<u8>(&r, &waker, would_block), None);
    check!("not woken yet", woken(&count), 0);
    r.ready();
    check!("woken", woken(&count), 1);
}

#[test]
fn one_wake_per_pending_poll() {
    let r = Readiness::new();
    let (count, waker) = counter();
    let _ = attempt::<u8>(&r, &waker, would_block);
    r.ready();
    r.ready();
    r.ready();
    check!("once", woken(&count), 1);
}

#[test]
fn interrupted_is_tried_again() {
    let r = Readiness::new();
    let mut calls = 0;
    let got = attempt(&r, Waker::noop(), || {
        calls += 1;
        if calls < 4 {
            Err(io::ErrorKind::Interrupted.into())
        } else {
            Ok(calls)
        }
    });
    check!("retried until it answered", got, Some(Ok(4)));
}

#[test]
fn news_during_the_attempt_is_not_lost() {
    let r = Readiness::new();
    let (count, waker) = counter();
    let mut calls = 0;
    let got = attempt(&r, &waker, || {
        calls += 1;
        if calls == 1 {
            // the data lands just after we found nothing
            r.ready();
            would_block()
        } else {
            Ok("data")
        }
    });
    match got {
        Some(v) => check!("tried again", v, Ok("data")),
        None => assert!(woken(&count) >= 1, "pending, but the news that came during the attempt woke nobody"),
    }
}
