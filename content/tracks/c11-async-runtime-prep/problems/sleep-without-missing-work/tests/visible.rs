use solution::*;

#[allow(dead_code, unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(dead_code, unused_imports)]
use std::sync::mpsc::{channel, Receiver};
#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Mutex};
#[allow(dead_code, unused_imports)]
use std::collections::VecDeque;
#[allow(dead_code, unused_imports)]
use std::time::Duration;

/// Turns a hang into a failure.
#[allow(dead_code)]
fn within<T: Send + 'static>(seconds: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(seconds))
        .expect("a sleeper was never released, or the test thread panicked")
}

/// Sleeps on `ticket` on another thread; the receiver gets a message when it returns.
#[allow(dead_code)]
fn sleeper(gate: &Arc<Gate>, ticket: Ticket) -> Receiver<()> {
    let (tx, rx) = channel();
    let gate = gate.clone();
    std::thread::spawn(move || {
        gate.sleep(ticket);
        let _ = tx.send(());
    });
    rx
}

#[allow(dead_code)]
fn released(rx: &Receiver<()>) -> bool {
    rx.recv_timeout(Duration::from_secs(5)).is_ok()
}

/// It should still be asleep. Waiting longer can only make this check stricter, never wrong for a correct gate.
#[allow(dead_code)]
fn stays_asleep(rx: &Receiver<()>) -> bool {
    rx.recv_timeout(Duration::from_millis(100)).is_err()
}

#[test]
fn a_notification_before_sleep_counts() {
    within(5, || {
        let gate = Gate::new();
        let ticket = gate.prepare();
        gate.notify_one();
        gate.sleep(ticket);
        check!("nobody is waiting now", gate.waiting(), 0);
    });
}

#[test]
fn a_notification_wakes_a_sleeper() {
    let gate = Arc::new(Gate::new());
    let rx = sleeper(&gate, gate.prepare());
    assert!(stays_asleep(&rx), "returned without a notification");
    gate.notify_one();
    assert!(released(&rx));
}

#[test]
fn an_old_notification_does_not_count() {
    let gate = Arc::new(Gate::new());
    gate.notify_one();
    gate.notify_all();
    let rx = sleeper(&gate, gate.prepare());
    assert!(stays_asleep(&rx), "released by a notification made before prepare");
    gate.notify_one();
    assert!(released(&rx));
}

#[test]
fn waiting_counts_tickets() {
    let gate = Gate::new();
    check!("none", gate.waiting(), 0);
    let a = gate.prepare();
    let b = gate.prepare();
    check!("two", gate.waiting(), 2);
    gate.cancel(a);
    check!("one", gate.waiting(), 1);
    gate.notify_one();
    gate.sleep(b);
    check!("none", gate.waiting(), 0);
}

#[test]
fn notify_all_releases_everyone() {
    let gate = Arc::new(Gate::new());
    let sleepers: Vec<_> = (0..5).map(|_| sleeper(&gate, gate.prepare())).collect();
    gate.notify_all();
    assert!(sleepers.iter().all(released), "somebody slept through notify_all");
}

#[test]
fn two_notifications_release_two() {
    let gate = Arc::new(Gate::new());
    let a = sleeper(&gate, gate.prepare());
    let b = sleeper(&gate, gate.prepare());
    // let them fall asleep first; the rule holds either way
    std::thread::sleep(Duration::from_millis(20));
    gate.notify_one();
    gate.notify_one();
    assert!(released(&a) && released(&b), "two notifications, two waiters, not both released");
}
