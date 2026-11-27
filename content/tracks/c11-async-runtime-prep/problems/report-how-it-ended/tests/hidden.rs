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

/// Panics when dropped.
struct Bomb(&'static str);
impl Drop for Bomb {
    fn drop(&mut self) {
        panic!("{}", self.0);
    }
}

/// A future with a destructor that panics. `ready` decides what its poll does.
struct Explosive {
    ready: Option<u32>,
    panic_in_poll: bool,
    _bomb: Bomb,
}
impl Future for Explosive {
    type Output = u32;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<u32> {
        if self.panic_in_poll {
            panic!("from poll");
        }
        match self.ready {
            Some(v) => Poll::Ready(v),
            None => Poll::Pending,
        }
    }
}

fn explosive(ready: Option<u32>, panic_in_poll: bool) -> Explosive {
    Explosive { ready, panic_in_poll, _bomb: Bomb("from drop") }
}

#[test]
fn a_destructor_panic_after_ready_replaces_the_value() {
    let (seen, _, _held, report) = setup();
    let mut task = Box::pin(watched(explosive(Some(5), false), report));
    check!("nothing escapes", catch_unwind(AssertUnwindSafe(|| step(&mut task))).ok(), Some(Poll::Ready(())));
    check!("the ending", seen_of(&seen).into_iter().map(|e| e.0).collect::<Vec<_>>(), vec![Ending::Panicked("from drop".into())]);
}

#[test]
fn a_destructor_panic_during_an_early_drop_does_not_escape() {
    let (seen, _, _held, report) = setup();
    let mut task = Box::pin(watched(explosive(None, false), report));
    let _ = step(&mut task);
    let escaped = catch_unwind(AssertUnwindSafe(move || drop(task))).is_err();
    check!("nothing escapes the drop", escaped, false);
    check!("the ending", seen_of(&seen).into_iter().map(|e| e.0).collect::<Vec<_>>(), vec![Ending::Panicked("from drop".into())]);
}

#[test]
fn the_first_panic_wins() {
    let (seen, _, _held, report) = setup();
    let mut task = Box::pin(watched(explosive(None, true), report));
    check!("nothing escapes", catch_unwind(AssertUnwindSafe(|| step(&mut task))).ok(), Some(Poll::Ready(())));
    check!("the message from poll", seen_of(&seen).into_iter().map(|e| e.0).collect::<Vec<_>>(), vec![Ending::Panicked("from poll".into())]);
}

#[test]
fn dropped_while_the_thread_is_unwinding() {
    let (seen, _, _held, report) = setup();
    let outer = catch_unwind(AssertUnwindSafe(move || {
        let mut task = Box::pin(watched(explosive(None, false), report));
        let _ = step(&mut task);
        panic!("outer");
    }));
    let payload = outer.expect_err("the outer panic continues");
    check!("and it is still the outer one", payload.downcast_ref::<&str>().copied(), Some("outer"));
    check!("reported once", seen_of(&seen).len(), 1);
}

#[test]
fn message_kinds() {
    fn ending_of(f: impl FnOnce() + Send + 'static) -> Vec<Ending<u32>> {
        let (seen, _, _held, report) = setup();
        let mut f = Some(f);
        let mut task = Box::pin(watched(
            std::future::poll_fn(move |_| -> Poll<u32> {
                (f.take().expect("polled once"))();
                unreachable!()
            }),
            report,
        ));
        let _ = step(&mut task);
        seen_of(&seen).into_iter().map(|e| e.0).collect()
    }
    check!("a literal", ending_of(|| panic!("plain")), vec![Ending::Panicked("plain".into())]);
    check!("formatted", ending_of(|| panic!("n = {}", 3)), vec![Ending::Panicked("n = 3".into())]);
    check!("a String payload", ending_of(|| std::panic::panic_any(String::from("owned"))), vec![Ending::Panicked("owned".into())]);
    check!("something else", ending_of(|| std::panic::panic_any(17u8)), vec![Ending::Panicked("non-string panic".into())]);
}

#[test]
fn the_context_is_passed_on() {
    use std::task::Wake;
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    let mut task = Box::pin(watched(
        std::future::poll_fn(|cx| {
            cx.waker().wake_by_ref();
            Poll::<u32>::Pending
        }),
        |_| {},
    ));
    let _ = task.as_mut().poll(&mut Context::from_waker(&waker));
    check!("the caller's waker", count.0.load(Ordering::SeqCst), 1);
}

#[test]
fn polling_after_ready_panics_without_a_second_report() {
    let (seen, _, _held, report) = setup();
    let mut task = Box::pin(watched(async { 1 }, report));
    let _ = step(&mut task);
    check!("panics", catch_unwind(AssertUnwindSafe(|| step(&mut task))).is_err(), true);
    drop(task);
    check!("one report", seen_of(&seen).len(), 1);
}

#[test]
fn a_panicked_future_is_destroyed_before_the_report() {
    let (seen, _, held, report) = setup();
    struct PanicThenHold(Held);
    impl Future for PanicThenHold {
        type Output = u32;
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<u32> {
            panic!("x")
        }
    }
    let mut task = Box::pin(watched(PanicThenHold(held), report));
    let _ = step(&mut task);
    check!("gone at report time", seen_of(&seen), vec![(Ending::Panicked("x".into()), true)]);
}

#[test]
fn it_can_cross_threads() {
    fn is_send<T: Send>(_: &T) {}
    let task = watched(async { 1u8 }, |_| {});
    is_send(&task);
}

#[test]
fn random_endings() {
    let mut rng = anneal_prelude::Rng::new(110_501);
    for round in 0..300 {
        let waits = rng.below(4);
        let polls = rng.below(6);
        let fails = rng.bool();
        let (seen, gone, held, report) = setup();
        let mut left = waits;
        let mut task = Box::pin(watched(
            async move {
                let _held = held;
                std::future::poll_fn(move |_| {
                    if left == 0 {
                        Poll::Ready(())
                    } else {
                        left -= 1;
                        Poll::Pending
                    }
                })
                .await;
                if fails {
                    panic!("round");
                }
                round
            },
            report,
        ));
        let mut finished = false;
        for _ in 0..polls {
            if !finished {
                finished = step(&mut task).is_ready();
            }
        }
        check!(format!("waits {waits}, polls {polls}"), finished, polls > waits);
        drop(task);
        let want = if !finished {
            Ending::Dropped
        } else if fails {
            Ending::Panicked("round".to_string())
        } else {
            Ending::Finished(round)
        };
        check!(format!("waits {waits}, polls {polls}, fails {fails}"), seen_of(&seen), vec![(want, true)]);
        assert!(gone.load(Ordering::SeqCst));
    }
}
