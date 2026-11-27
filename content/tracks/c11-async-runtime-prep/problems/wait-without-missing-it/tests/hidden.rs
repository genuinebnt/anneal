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
fn the_latest_waker_is_the_one_woken() {
    let r = Readiness::new();
    let (old, w1) = counter();
    let (new, w2) = counter();
    let _ = attempt::<u8>(&r, &w1, would_block);
    let _ = attempt::<u8>(&r, &w2, would_block);
    r.ready();
    check!("(old, new)", (woken(&old), woken(&new)), (0, 1));
}

#[test]
fn without_news_it_does_not_spin() {
    let r = Readiness::new();
    r.ready();
    r.ready();
    let mut calls = 0;
    let got = attempt::<u8>(&r, Waker::noop(), || {
        calls += 1;
        assert!(calls <= 100, "spinning");
        would_block()
    });
    check!("pending", got, None);
    assert!((1..=3).contains(&calls), "op was called {calls} times although nothing new happened");
}

#[test]
fn news_before_the_first_poll_does_not_replace_the_attempt() {
    let r = Readiness::new();
    r.ready();
    let mut calls = 0;
    let got = attempt(&r, Waker::noop(), || {
        calls += 1;
        Ok(calls)
    });
    check!("op ran and its answer came back", got, Some(Ok(1)));
}

#[test]
fn news_during_a_later_attempt_too() {
    let r = Readiness::new();
    let (count, waker) = counter();
    check!("first", attempt::<u8>(&r, &waker, would_block), None);
    r.ready();
    let seen = woken(&count);
    let mut calls = 0;
    let got = attempt(&r, &waker, || {
        calls += 1;
        match calls {
            1 => {
                r.ready();
                would_block()
            }
            _ => Ok(9),
        }
    });
    if got.is_none() {
        assert!(woken(&count) > seen, "news during the second poll's attempt was lost");
    }
}

#[test]
fn news_during_an_interrupted_attempt() {
    let r = Readiness::new();
    let (count, waker) = counter();
    let mut calls = 0;
    let got = attempt(&r, &waker, || {
        calls += 1;
        match calls {
            1 => {
                r.ready();
                Err(io::ErrorKind::Interrupted.into())
            }
            2 => {
                r.ready();
                would_block()
            }
            _ => Ok(1),
        }
    });
    if got.is_none() {
        assert!(woken(&count) >= 1);
    }
}

#[test]
fn a_waker_may_call_back_into_ready() {
    struct Again(Arc<Readiness>);
    impl Wake for Again {
        fn wake(self: Arc<Self>) {
            self.0.ready();
        }
    }
    within(5, || {
        let r = Arc::new(Readiness::new());
        let waker = Waker::from(Arc::new(Again(r.clone())));
        let _ = attempt::<u8>(&r, &waker, would_block);
        r.ready(); // deadlocks if wake() runs under a lock
    });
}

#[test]
fn it_is_shared_between_threads() {
    fn both<T: Send + Sync>() {}
    both::<Readiness>();
}

#[test]
fn two_directions_are_independent() {
    let (read, write) = (Readiness::new(), Readiness::new());
    let (rc, rw) = counter();
    let (wc, ww) = counter();
    let _ = attempt::<u8>(&read, &rw, would_block);
    let _ = attempt::<u8>(&write, &ww, would_block);
    write.ready();
    check!("only the writer", (woken(&rc), woken(&wc)), (0, 1));
}

#[test]
fn random_scripts_never_lose_news() {
    // each call of `op` takes the next step of a script: how much news arrives during the call, and what it answers
    let mut rng = anneal_prelude::Rng::new(110_801);
    for round in 0..2000 {
        let r = Readiness::new();
        let (count, waker) = counter();
        let mut queued = 0u32;
        let mut history = Vec::new();
        for _ in 0..6 {
            let before_wakes = woken(&count);
            let mut news_in_poll = false;
            let mut arrivals: VecDeque<bool> = (0..rng.below(3)).map(|_| rng.bool()).collect();
            let mut calls = 0;
            let got = attempt(&r, &waker, || {
                calls += 1;
                assert!(calls < 50, "spinning: {history:?}");
                let answer = if queued > 0 {
                    queued -= 1;
                    Ok(())
                } else {
                    would_block()
                };
                // news that lands during the call, after the answer was decided
                if arrivals.pop_front() == Some(true) {
                    queued += 1;
                    news_in_poll = true;
                    r.ready();
                }
                answer
            });
            history.push(format!("poll -> {got:?}, news during it: {news_in_poll}"));
            if got.is_none() {
                if queued > 0 {
                    assert!(woken(&count) > before_wakes, "round {round}: data waits, the task is pending and was not woken: {history:?}");
                }
                // some news between polls
                if rng.bool() {
                    let seen = woken(&count);
                    queued += 1;
                    r.ready();
                    history.push("news".into());
                    if seen == before_wakes {
                        check!(format!("round {round}: news after a pending poll wakes it, {history:?}"), woken(&count), seen + 1);
                    }
                }
            }
        }
    }
}

/// The source sends the next item only after the previous one was taken, so one lost wake-up stops everything.
#[test]
fn a_source_on_another_thread() {
    within(12, || {
        let readiness = Arc::new(Readiness::new());
        let items = Arc::new(Mutex::new(VecDeque::new()));
        let taken = Arc::new(AtomicUsize::new(0));
        let (r, q, t) = (readiness.clone(), items.clone(), taken.clone());
        let source = std::thread::spawn(move || {
            for i in 0..30_000 {
                q.lock().unwrap().push_back(i);
                r.ready();
                while t.load(Ordering::SeqCst) <= i {
                    std::thread::yield_now();
                }
            }
        });
        struct Unpark(std::thread::Thread);
        impl Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        for i in 0..30_000 {
            let got = loop {
                let attempt = poll_io(&readiness, &mut cx, || match items.lock().unwrap().pop_front() {
                    Some(v) => Ok(v),
                    None => would_block(),
                });
                match attempt {
                    Poll::Ready(v) => break v.unwrap(),
                    Poll::Pending => std::thread::park(),
                }
            };
            assert_eq!(got, i);
            taken.fetch_add(1, Ordering::SeqCst);
        }
        source.join().unwrap();
    });
}
