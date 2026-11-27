use solution::*;

#[allow(dead_code, unused_imports)]
use std::future::{poll_fn, Future};
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
type Log = Arc<Mutex<Vec<String>>>;

#[allow(dead_code)]
fn note(log: &Log, line: impl Into<String>) {
    log.lock().unwrap().push(line.into());
}

#[allow(dead_code)]
fn lines(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

/// Turns a hang into a failure.
#[allow(dead_code)]
fn within<T: Send + 'static>(seconds: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(seconds))
        .expect("the test hung or panicked")
}

/// A one-shot signal that any thread can set.
#[allow(dead_code)]
#[derive(Default)]
struct Latch {
    set: AtomicBool,
    waker: Mutex<Option<Waker>>,
}
#[allow(dead_code)]
impl Latch {
    fn open(&self) {
        self.set.store(true, Ordering::SeqCst);
        let waker = self.waker.lock().unwrap().take();
        if let Some(w) = waker {
            w.wake();
        }
    }
    fn poll(&self, cx: &mut Context<'_>) -> Poll<()> {
        *self.waker.lock().unwrap() = Some(cx.waker().clone());
        if self.set.load(Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[allow(dead_code)]
async fn opened(latch: Arc<Latch>) {
    poll_fn(|cx| latch.poll(cx)).await
}

#[allow(dead_code)]
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
#[allow(dead_code, unused_imports)]
use std::time::Duration;

#[allow(dead_code)]
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Polls once with a waker that does nothing.
#[allow(dead_code)]
fn peek<F: Future + Unpin>(f: &mut F) -> Poll<F::Output> {
    Pin::new(f).poll(&mut Context::from_waker(Waker::noop()))
}

/// Spawns a sleeper per delay; each writes down its id when it wakes.
#[allow(dead_code)]
fn sleepers(rt: &LocalRuntime, delays: &[u64]) -> Log {
    let log = Log::default();
    let _inside = rt.handle().enter();
    for (id, &delay) in delays.iter().enumerate() {
        let log = log.clone();
        let wait = sleep(ms(delay));
        spawn(async move {
            wait.await;
            note(&log, id.to_string());
        });
    }
    log
}

#[test]
fn a_thousand_timers_in_a_permuted_order() {
    let rt = LocalRuntime::new();
    let delays: Vec<u64> = (0..1000u64).map(|i| (i * 7919) % 1000 + 1).collect();
    let log = sleepers(&rt, &delays);
    rt.run_until_stalled();
    let woken = || lines(&log).iter().map(|id| delays[id.parse::<usize>().unwrap()]).collect::<Vec<u64>>();
    rt.clock().advance(ms(499));
    rt.run_until_stalled();
    check!("exactly the due ones, in order", woken(), (1..=499).collect::<Vec<_>>());
    rt.clock().advance(ms(1));
    rt.run_until_stalled();
    check!("one more", woken().len(), 500);
    rt.clock().advance(ms(500));
    rt.run_until_stalled();
    check!("all in deadline order", woken(), (1..=1000).collect::<Vec<_>>());
    check!("none left", rt.pending_timers(), 0);
}

#[test]
fn equal_deadlines_wake_in_the_order_they_started_waiting() {
    let rt = LocalRuntime::new();
    let log = sleepers(&rt, &[5; 20]);
    rt.run_until_stalled();
    rt.clock().advance(ms(5));
    rt.run_until_stalled();
    check!("first come, first woken", lines(&log), (0..20).map(|i| i.to_string()).collect::<Vec<_>>());
}

#[test]
fn ties_follow_the_first_pending_poll_not_creation() {
    let rt = LocalRuntime::new();
    let log = Log::default();
    let _inside = rt.handle().enter();
    let (a, b) = (sleep(ms(5)), sleep(ms(5)));
    for (name, wait) in [("b", b), ("a", a)] {
        let log = log.clone();
        spawn(async move {
            wait.await;
            note(&log, name);
        });
    }
    rt.run_until_stalled();
    rt.clock().advance(ms(5));
    rt.run_until_stalled();
    check!("b waited first", lines(&log).concat(), "ba".to_string());
}

#[test]
fn the_deadline_is_fixed_when_sleep_is_called() {
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    let mut early = sleep(ms(10));
    let mut limited = Box::pin(timeout(ms(10), std::future::pending::<u8>()));
    rt.clock().advance(ms(10));
    check!("sleep polled late is ready", peek(&mut early), Poll::Ready(()));
    check!("timeout polled late has expired", peek(&mut limited), Poll::Ready(Err(Elapsed)));
}

#[test]
fn zero_and_past_deadlines_are_ready_at_once() {
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    rt.clock().advance(ms(100));
    check!("zero", peek(&mut sleep(ms(0))), Poll::Ready(()));
    check!("past", peek(&mut sleep_until(ms(40))), Poll::Ready(()));
    check!("now", (peek(&mut sleep_until(ms(100))), now()), (Poll::Ready(()), ms(100)));
    check!("no entries", rt.pending_timers(), 0);
}

#[test]
fn a_dropped_sleep_is_removed_at_once() {
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    let mut waits: Vec<Sleep> = (0..100).map(|i| sleep(ms(1000 + i))).collect();
    check!("not polled, not registered", rt.pending_timers(), 0);
    for w in &mut waits {
        assert!(peek(w).is_pending());
        assert!(peek(w).is_pending());
    }
    check!("each once", rt.pending_timers(), 100);
    waits.truncate(10);
    check!("ninety removed", rt.pending_timers(), 10);
    drop(waits);
    check!("all removed", rt.pending_timers(), 0);
}

#[test]
fn many_timeouts_that_finish_early_leave_nothing_behind() {
    let rt = LocalRuntime::new();
    let left = rt.block_on(async {
        for i in 0..10_000u32 {
            let got = timeout(Duration::from_secs(3600), async move {
                if i % 2 == 0 {
                    yield_now().await;
                }
                i
            })
            .await;
            assert_eq!(got, Ok(i));
        }
        rt.pending_timers()
    });
    check!("timer entries left", left, 0);
}

#[test]
fn a_dropped_sleep_never_wakes() {
    struct Count(AtomicUsize);
    impl std::task::Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let waker = Waker::from(count.clone());
    let mut wait = sleep(ms(5));
    assert!(Pin::new(&mut wait).poll(&mut Context::from_waker(&waker)).is_pending());
    drop(wait);
    rt.clock().advance(ms(10));
    rt.run_until_stalled();
    check!("never woken", count.0.load(Ordering::SeqCst), 0);
}

#[test]
fn the_latest_waker_of_a_sleep_is_woken() {
    struct Count(AtomicUsize);
    impl std::task::Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    let old = Arc::new(Count(AtomicUsize::new(0)));
    let new = Arc::new(Count(AtomicUsize::new(0)));
    let mut wait = sleep(ms(5));
    for c in [&old, &new] {
        let waker = Waker::from(c.clone());
        assert!(Pin::new(&mut wait).poll(&mut Context::from_waker(&waker)).is_pending());
    }
    rt.clock().advance(ms(5));
    rt.run_until_stalled();
    check!("(old, new)", (old.0.load(Ordering::SeqCst), new.0.load(Ordering::SeqCst)), (0, 1));
}

#[test]
fn a_value_ready_at_the_deadline_wins() {
    let rt = LocalRuntime::new();
    let got = rt.block_on(async {
        let wait = sleep(ms(20));
        let limited = timeout(ms(20), async move {
            wait.await;
            "value"
        });
        let mover = rt.clock();
        spawn(async move { mover.advance(ms(20)) });
        limited.await
    });
    check!("both are due at 20", got, Ok("value"));
}

#[test]
fn timeout_destroys_the_future_before_it_returns() {
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    for expires in [true, false] {
        let gone = Arc::new(AtomicBool::new(false));
        let guard = DropFlag(gone.clone());
        let mut limited = Box::pin(timeout(ms(10), async move {
            let _guard = guard;
            if !expires {
                return;
            }
            std::future::pending::<()>().await
        }));
        if expires {
            assert!(peek(&mut limited).is_pending());
            rt.clock().advance(ms(10));
        }
        let got = peek(&mut limited);
        check!(format!("expires {expires}: result"), got, Poll::Ready(if expires { Err(Elapsed) } else { Ok(()) }));
        // `limited` is still alive here
        check!(format!("expires {expires}: inner future gone, no timer left"), (gone.load(Ordering::SeqCst), rt.pending_timers()), (true, 0));
    }
}

#[test]
fn timeout_removes_its_timer_when_the_value_comes() {
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    let latch = Arc::new(Latch::default());
    let mut limited = Box::pin(timeout(ms(1000), opened(latch.clone())));
    assert!(peek(&mut limited).is_pending());
    check!("one timer", rt.pending_timers(), 1);
    latch.open();
    check!("value", peek(&mut limited), Poll::Ready(Ok(())));
    check!("no timer", rt.pending_timers(), 0);
}

#[test]
fn a_cancelled_tick_is_not_lost() {
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    let mut every = interval(ms(10));
    {
        let mut first = Box::pin(every.tick());
        check!("tick 0", peek(&mut first), Poll::Ready(ms(0)));
    }
    {
        let mut second = Box::pin(every.tick());
        check!("not yet", peek(&mut second), Poll::Pending);
    }
    rt.clock().advance(ms(10));
    let mut again = Box::pin(every.tick());
    check!("the same tick", peek(&mut again), Poll::Ready(ms(10)));
}

#[test]
fn interval_counts_from_its_creation() {
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    rt.clock().advance(ms(7));
    let mut every = interval(ms(10));
    rt.clock().advance(ms(100));
    let due: Vec<Duration> = (0..11).map(|_| match peek(&mut Box::pin(every.tick())) {
        Poll::Ready(d) => d,
        Poll::Pending => panic!("a late tick was skipped"),
    }).collect();
    check!("7, 17, ... 107", due, (0..11).map(|k| ms(7 + 10 * k)).collect::<Vec<_>>());
    check!("the next is in the future", peek(&mut Box::pin(every.tick())), Poll::Pending);
}

#[test]
#[should_panic]
fn a_zero_period_panics() {
    let rt = LocalRuntime::new();
    let _inside = rt.handle().enter();
    let _ = interval(ms(0));
}

#[test]
fn time_functions_need_a_runtime() {
    check!("sleep", catch_unwind(|| drop(sleep(ms(1)))).is_err(), true);
    check!("now", catch_unwind(now).is_err(), true);
}

#[test]
fn busy_tasks_do_not_stop_timers() {
    for use_block_on in [false, true] {
        within(8, move || {
            let rt = LocalRuntime::new();
            let done = Arc::new(Latch::default());
            let d = done.clone();
            rt.spawn(async move {
                sleep(ms(50)).await;
                d.open();
            });
            let (clock, d) = (rt.clock(), done.clone());
            let starved = Arc::new(AtomicBool::new(false));
            let s = starved.clone();
            rt.spawn(async move {
                let mut spins = 0;
                while !d.set.load(Ordering::SeqCst) {
                    spins += 1;
                    if spins == 10_000 {
                        s.store(true, Ordering::SeqCst);
                        break;
                    }
                    clock.advance(ms(1));
                    yield_now().await;
                }
            });
            if use_block_on {
                // the root waits only for the latch, so it is not woken while the busy task spins
                rt.block_on(opened(done.clone()));
            } else {
                rt.run_until_stalled();
            }
            assert!(done.set.load(Ordering::SeqCst));
            assert!(!starved.load(Ordering::SeqCst), "block_on: {use_block_on}: a due timer waited behind a busy task");
        });
    }
}

#[test]
fn many_clock_moves_from_another_thread_are_never_missed() {
    within(12, || {
        let rt = LocalRuntime::new();
        let clock = rt.clock();
        let stop = Arc::new(AtomicBool::new(false));
        let step = Arc::new(AtomicUsize::new(0));
        let (s, st) = (stop.clone(), step.clone());
        let mover = std::thread::spawn(move || {
            let mut moved = 0;
            while !s.load(Ordering::SeqCst) {
                // one move for each sleep the runtime has started, so a missed move stops everything
                if st.load(Ordering::SeqCst) > moved {
                    moved += 1;
                    clock.advance(ms(1));
                } else {
                    std::thread::yield_now();
                }
            }
        });
        rt.block_on(async {
            for _ in 0..3000 {
                let wait = sleep(ms(1));
                step.fetch_add(1, Ordering::SeqCst);
                wait.await;
            }
        });
        stop.store(true, Ordering::SeqCst);
        mover.join().unwrap();
    });
}

#[test]
fn random_sleeps_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(120_401);
    for round in 0..200 {
        let rt = LocalRuntime::new();
        let delays: Vec<u64> = (0..rng.below(12)).map(|_| rng.below(20) as u64).collect();
        let log = sleepers(&rt, &delays);
        rt.run_until_stalled();
        let mut time = 0;
        let mut want: Vec<(u64, usize)> = delays.iter().enumerate().filter(|(_, &d)| d == 0).map(|(i, &d)| (d, i)).collect();
        let mut rest: Vec<(u64, usize)> = delays.iter().enumerate().filter(|(_, &d)| d > 0).map(|(i, &d)| (d, i)).collect();
        rest.sort();
        for _ in 0..6 {
            let by = rng.below(8) as u64;
            time += by;
            rt.clock().advance(ms(by));
            rt.run_until_stalled();
            let due = rest.iter().take_while(|(d, _)| *d <= time).count();
            want.extend(rest.drain(..due));
            let ids: Vec<String> = want.iter().map(|(_, i)| i.to_string()).collect();
            check!(format!("round {round}, delays {delays:?}, time {time}"), lines(&log), ids);
            check!(format!("round {round}, time {time}: pending"), rt.pending_timers(), rest.len());
        }
    }
}

#[test]
fn step_three_rules_still_hold() {
    let rt = LocalRuntime::new();
    let log = Log::default();
    for id in ["a", "b", "c"] {
        let log = log.clone();
        rt.spawn(async move { note(&log, id) });
    }
    let mut left = 3;
    rt.block_on(poll_fn(|cx| {
        note(&log, "R");
        if left == 0 {
            return Poll::Ready(());
        }
        left -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }));
    check!("root and tasks alternate", lines(&log).concat(), "RaRbRcR".to_string());
}
