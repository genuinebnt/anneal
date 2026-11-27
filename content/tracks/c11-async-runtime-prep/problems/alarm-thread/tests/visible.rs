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
fn an_alarm_runs_and_not_early() {
    let clock = AlarmClock::start();
    let (tx, rx) = channel();
    let set = Instant::now();
    clock.alarms().after(ms(30), move || tx.send(Instant::now()).unwrap());
    let ran = got(&rx);
    assert!(ran >= set + ms(30), "ran after only {:?}", ran - set);
}

#[test]
fn it_runs_on_the_clocks_thread() {
    let clock = AlarmClock::start();
    let (tx, rx) = channel();
    clock.alarms().after(ms(1), move || tx.send(std::thread::current().id()).unwrap());
    check!("not the caller's thread", got(&rx) == std::thread::current().id(), false);
}

#[test]
fn alarms_run_in_time_order() {
    let clock = AlarmClock::start();
    let (tx, rx) = channel();
    let base = Instant::now() + ms(200);
    for offset in [30u64, 10, 20, 5] {
        let tx = tx.clone();
        clock.alarms().at(base + ms(offset), move || tx.send(offset).unwrap());
    }
    check!("by time", (0..4).map(|_| got(&rx)).collect::<Vec<_>>(), vec![5, 10, 20, 30]);
}

#[test]
fn a_cancelled_alarm_never_runs() {
    let clock = AlarmClock::start();
    let alarms = clock.alarms();
    let (tx, rx) = channel();
    let t2 = tx.clone();
    let id = alarms.after(ms(500), move || t2.send("cancelled").unwrap());
    alarms.after(ms(520), move || tx.send("later").unwrap());
    check!("cancelled in time", alarms.cancel(id), true);
    check!("only the later one", got(&rx), "later");
    check!("cancel again", alarms.cancel(id), false);
}

#[test]
fn an_earlier_alarm_overtakes_a_far_one() {
    let clock = AlarmClock::start();
    let alarms = clock.alarms();
    let (tx, rx) = channel();
    let t2 = tx.clone();
    alarms.after(Duration::from_secs(3600), move || t2.send("far").unwrap());
    // give the thread a moment to go to sleep on the far one; correctness does not depend on this
    std::thread::sleep(ms(30));
    alarms.after(ms(10), move || tx.send("near").unwrap());
    check!("the near one runs", got(&rx), "near");
    check!("one still pending", alarms.pending(), 1);
}

#[test]
fn dropping_the_clock_does_not_wait_for_a_far_alarm() {
    let clock = AlarmClock::start();
    clock.alarms().after(Duration::from_secs(3600), || {});
    std::thread::sleep(ms(20));
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        drop(clock);
        tx.send(()).unwrap();
    });
    got(&rx);
}
