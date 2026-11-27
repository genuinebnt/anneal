use solution::*;

#[allow(dead_code, unused_imports)]
use std::future::Future;
#[allow(dead_code, unused_imports)]
use std::panic::{catch_unwind, AssertUnwindSafe};
#[allow(dead_code, unused_imports)]
use std::pin::Pin;
#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(dead_code, unused_imports)]
use std::task::{Context, Poll, Waker};

#[allow(dead_code)]
fn step<F: Future + ?Sized>(f: &mut Pin<Box<F>>) -> Poll<F::Output> {
    f.as_mut().poll(&mut Context::from_waker(Waker::noop()))
}

/// What the report function saw: the endings, each with "was the future already destroyed?".
#[allow(dead_code)]
type Seen = Arc<Mutex<Vec<(Ending<u32>, bool)>>>;

/// Sets the flag when dropped: it stands for the future's files and locks.
#[allow(dead_code)]
struct Held(Arc<AtomicBool>);
impl Drop for Held {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[allow(dead_code)]
fn setup() -> (Seen, Arc<AtomicBool>, Held, Box<dyn FnOnce(Ending<u32>) + Send>) {
    let seen = Seen::default();
    let gone = Arc::new(AtomicBool::new(false));
    let (s, g) = (seen.clone(), gone.clone());
    let report = Box::new(move |ending| s.lock().unwrap().push((ending, g.load(Ordering::SeqCst))));
    (seen, gone.clone(), Held(gone), report)
}

#[allow(dead_code)]
fn seen_of(seen: &Seen) -> Vec<(Ending<u32>, bool)> {
    seen.lock().unwrap().clone()
}

#[test]
fn a_finished_future_is_reported_after_it_is_destroyed() {
    let (seen, _gone, held, report) = setup();
    let mut task = Box::pin(watched(
        async move {
            let _held = held;
            7
        },
        report,
    ));
    check!("ready", step(&mut task), Poll::Ready(()));
    check!("one report, and the future was already gone", seen_of(&seen), vec![(Ending::Finished(7), true)]);
}

#[test]
fn pending_reports_nothing() {
    let (seen, gone, held, report) = setup();
    let mut task = Box::pin(watched(
        async move {
            let _held = held;
            std::future::pending::<u32>().await
        },
        report,
    ));
    check!("pending", step(&mut task), Poll::Pending);
    check!("nothing reported", seen_of(&seen), Vec::new());
    check!("the future is alive", gone.load(Ordering::SeqCst), false);
}

#[test]
fn dropping_it_early_reports_dropped_after_cleanup() {
    let (seen, _gone, held, report) = setup();
    let mut task = Box::pin(watched(
        async move {
            let _held = held;
            std::future::pending::<u32>().await
        },
        report,
    ));
    let _ = step(&mut task);
    drop(task);
    check!("dropped, after the cleanup", seen_of(&seen), vec![(Ending::Dropped, true)]);
}

#[test]
fn a_panic_is_caught_and_reported() {
    let (seen, _gone, held, report) = setup();
    let mut task = Box::pin(watched(
        async move {
            let _held = held;
            if true {
                panic!("boom {}", 42);
            }
            0
        },
        report,
    ));
    check!("poll returns", step(&mut task), Poll::Ready(()));
    check!("the message", seen_of(&seen), vec![(Ending::Panicked("boom 42".into()), true)]);
}

#[test]
fn never_polled_is_dropped() {
    let (seen, _gone, held, report) = setup();
    let task = watched(
        async move {
            let _held = held;
            1
        },
        report,
    );
    drop(task);
    check!("never polled", seen_of(&seen), vec![(Ending::Dropped, true)]);
}

#[test]
fn nothing_is_reported_twice() {
    let (seen, _gone, held, report) = setup();
    let mut task = Box::pin(watched(
        async move {
            let _held = held;
            3
        },
        report,
    ));
    let _ = step(&mut task);
    drop(task);
    check!("only the one report", seen_of(&seen).len(), 1);
}
