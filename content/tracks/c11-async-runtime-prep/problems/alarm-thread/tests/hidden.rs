use solution::*;

#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::mpsc::{channel, Receiver, Sender};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(dead_code, unused_imports)]
use std::time::{Duration, Instant};

/// A generous upper limit: it only turns a hang into a failure.
#[allow(dead_code)]
const PATIENCE: Duration = Duration::from_secs(5);

#[allow(dead_code)]
fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[allow(dead_code)]
fn got<T>(rx: &Receiver<T>) -> T {
    rx.recv_timeout(PATIENCE).expect("an alarm that should have run did not")
}

#[test]
fn equal_times_run_in_the_order_set() {
    let clock = AlarmClock::start();
    let (tx, rx) = channel();
    let when = Instant::now() + ms(200);
    for i in 0..50 {
        let tx = tx.clone();
        clock.alarms().at(when, move || tx.send(i).unwrap());
    }
    check!("in the order they were set", (0..50).map(|_| got(&rx)).collect::<Vec<_>>(), (0..50).collect::<Vec<_>>());
}

#[test]
fn a_past_time_is_due_at_once() {
    let clock = AlarmClock::start();
    let (tx, rx) = channel();
    let past = Instant::now();
    std::thread::sleep(ms(2));
    clock.alarms().at(past, move || tx.send(()).unwrap());
    got(&rx);
}

#[test]
fn cancel_drops_the_closure_at_once() {
    let clock = AlarmClock::start();
    let captured = Arc::new(());
    let keep = captured.clone();
    let id = clock.alarms().after(Duration::from_secs(3600), move || drop(keep));
    check!("captured", Arc::strong_count(&captured), 2);
    check!("cancel", clock.alarms().cancel(id), true);
    check!("released by the time cancel returns", Arc::strong_count(&captured), 1);
}

#[test]
fn cancel_after_it_ran_is_false() {
    let clock = AlarmClock::start();
    let alarms = clock.alarms();
    let (tx, rx) = channel();
    let id = alarms.after(ms(1), move || tx.send(()).unwrap());
    got(&rx);
    check!("too late", alarms.cancel(id), false);
    check!("nothing pending", alarms.pending(), 0);
}

#[test]
fn cancel_while_it_runs_is_false() {
    let clock = AlarmClock::start();
    let alarms = clock.alarms();
    let (started_tx, started) = channel();
    let (go, wait) = channel::<()>();
    let id = alarms.after(ms(1), move || {
        started_tx.send(()).unwrap();
        wait.recv_timeout(PATIENCE).unwrap();
    });
    got(&started);
    check!("it has started", alarms.cancel(id), false);
    check!("a running alarm is not pending", alarms.pending(), 0);
    go.send(()).unwrap();
}

/// Calls back into the clock from its destructor.
struct Calls(Alarms, Sender<usize>);
impl Drop for Calls {
    fn drop(&mut self) {
        let n = self.0.pending();
        let _ = self.1.send(n);
    }
}

#[test]
fn closures_and_destructors_may_call_back() {
    let (tx, rx) = channel();
    let worker = std::thread::spawn(move || {
        let clock = AlarmClock::start();
        let alarms = clock.alarms();
        let a2 = alarms.clone();
        let (done, ran) = channel();
        // an alarm that sets another alarm
        alarms.after(ms(1), move || {
            let n = a2.pending();
            let done = done.clone();
            a2.after(ms(1), move || done.send(n).unwrap());
        });
        let first = got(&ran);
        let (dtx, dropped) = channel();
        // dropped by cancel
        let probe = Calls(alarms.clone(), dtx.clone());
        let id = alarms.after(Duration::from_secs(3600), move || drop(probe));
        alarms.cancel(id);
        let by_cancel = got(&dropped);
        // dropped by stop
        let probe = Calls(alarms.clone(), dtx);
        alarms.after(Duration::from_secs(3600), move || drop(probe));
        drop(clock);
        let by_stop = got(&dropped);
        tx.send((first, by_cancel, by_stop)).unwrap();
    });
    let seen = rx.recv_timeout(Duration::from_secs(10)).expect("deadlock: a lock was held while running or dropping a closure");
    check!("pending() seen from inside", seen, (0, 0, 0));
    worker.join().unwrap();
}

#[test]
fn a_panicking_alarm_does_not_stop_the_clock() {
    let clock = AlarmClock::start();
    let (tx, rx) = channel();
    clock.alarms().after(ms(1), || panic!("alarm"));
    clock.alarms().after(ms(15), move || tx.send(()).unwrap());
    got(&rx);
}

#[test]
fn drop_waits_for_the_running_alarm_and_drops_the_rest() {
    let clock = AlarmClock::start();
    let alarms = clock.alarms();
    let finished = Arc::new(AtomicBool::new(false));
    let (started_tx, started) = channel();
    let f2 = finished.clone();
    alarms.after(ms(1), move || {
        started_tx.send(()).unwrap();
        std::thread::sleep(ms(50));
        f2.store(true, Ordering::SeqCst);
    });
    let left = Arc::new(());
    let ran = Arc::new(AtomicUsize::new(0));
    for _ in 0..20 {
        let (keep, ran) = (left.clone(), ran.clone());
        alarms.after(Duration::from_secs(60), move || {
            ran.fetch_add(1, Ordering::SeqCst);
            drop(keep);
        });
    }
    got(&started);
    drop(clock);
    check!("the running alarm finished first", finished.load(Ordering::SeqCst), true);
    check!("the others were dropped", Arc::strong_count(&left), 1);
    check!("and never ran", ran.load(Ordering::SeqCst), 0);
    check!("nothing pending", alarms.pending(), 0);
}

#[test]
fn after_the_clock_is_gone() {
    let clock = AlarmClock::start();
    let alarms = clock.alarms();
    drop(clock);
    let captured = Arc::new(());
    let keep = captured.clone();
    let id = alarms.after(ms(1), move || drop(keep));
    check!("dropped at once", Arc::strong_count(&captured), 1);
    check!("nothing to cancel", alarms.cancel(id), false);
    check!("nothing pending", alarms.pending(), 0);
}

#[test]
fn the_thread_has_exited_after_drop() {
    struct OnThreadExit(Sender<()>);
    impl Drop for OnThreadExit {
        fn drop(&mut self) {
            std::thread::sleep(ms(20));
            let _ = self.0.send(());
        }
    }
    thread_local! {
        static SENTINEL: std::cell::RefCell<Option<OnThreadExit>> = const { std::cell::RefCell::new(None) };
    }
    let clock = AlarmClock::start();
    let (exit_tx, exit) = channel();
    let (tx, rx) = channel();
    clock.alarms().after(ms(1), move || {
        SENTINEL.with(|s| *s.borrow_mut() = Some(OnThreadExit(exit_tx)));
        tx.send(()).unwrap();
    });
    got(&rx);
    drop(clock);
    check!("the clock's thread ran its exit code before drop returned", exit.try_recv().is_ok(), true);
}

#[test]
fn handles_are_shared_between_threads() {
    fn is<T: Clone + Send + Sync>() {}
    is::<Alarms>();
}

#[test]
fn many_random_alarms_run_in_order_and_never_early() {
    let mut rng = anneal_prelude::Rng::new(110_701);
    let clock = AlarmClock::start();
    let alarms = clock.alarms();
    let (tx, rx) = channel();
    let base = Instant::now() + ms(250);
    let mut want = Vec::new();
    let mut ids = Vec::new();
    for i in 0..400usize {
        let offset = ms(rng.below(60) as u64);
        let when = base + offset;
        let tx = tx.clone();
        ids.push((alarms.at(when, move || tx.send((i, Instant::now() >= when)).unwrap()), i, offset));
    }
    let mut cancelled = 0;
    for &(id, i, offset) in &ids {
        if rng.below(4) == 0 {
            assert!(alarms.cancel(id), "alarm {i} could not be cancelled before its time");
            cancelled += 1;
        } else {
            want.push((offset, i));
        }
    }
    check!("pending after cancels", alarms.pending(), 400 - cancelled);
    want.sort();
    let ran: Vec<(usize, bool)> = (0..want.len()).map(|_| got(&rx)).collect();
    check!("order by time, then by setting", ran.iter().map(|r| r.0).collect::<Vec<_>>(), want.iter().map(|w| w.1).collect::<Vec<_>>());
    check!("none early", ran.iter().all(|r| r.1), true);
}

#[test]
fn setting_alarms_from_many_threads() {
    let clock = AlarmClock::start();
    let (tx, rx) = channel();
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let (alarms, tx) = (clock.alarms(), tx.clone());
            std::thread::spawn(move || {
                for i in 0..200 {
                    let tx = tx.clone();
                    alarms.after(Duration::from_micros(i * 10), move || tx.send(1usize).unwrap());
                }
            })
        })
        .collect();
    threads.into_iter().for_each(|t| t.join().unwrap());
    check!("all ran", (0..800).map(|_| got(&rx)).sum::<usize>(), 800);
}
