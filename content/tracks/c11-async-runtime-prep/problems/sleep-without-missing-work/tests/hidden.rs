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
fn two_notifications_before_either_sleeps() {
    within(5, || {
        let gate = Gate::new();
        let a = gate.prepare();
        let b = gate.prepare();
        gate.notify_one();
        gate.notify_one();
        gate.sleep(a);
        gate.sleep(b);
    });
}

#[test]
fn a_cancelled_ticket_does_not_swallow_the_notification() {
    for cancel_first_one in [true, false] {
        within(5, move || {
            let gate = Gate::new();
            let a = gate.prepare();
            let b = gate.prepare();
            gate.notify_one();
            let (gone, kept) = if cancel_first_one { (a, b) } else { (b, a) };
            gate.cancel(gone);
            gate.sleep(kept);
        });
    }
}

#[test]
fn a_cancelled_ticket_with_a_sleeper() {
    let gate = Arc::new(Gate::new());
    let rx = sleeper(&gate, gate.prepare());
    let other = gate.prepare();
    std::thread::sleep(Duration::from_millis(20));
    gate.notify_one();
    gate.cancel(other);
    assert!(released(&rx), "the notification vanished with the cancelled ticket");
}

#[test]
fn notify_all_covers_tickets_not_asleep_yet() {
    within(5, || {
        let gate = Gate::new();
        let tickets: Vec<_> = (0..10).map(|_| gate.prepare()).collect();
        gate.notify_all();
        for t in tickets {
            gate.sleep(t);
        }
    });
}

#[test]
fn a_ticket_prepared_after_notify_all_is_not_covered() {
    let gate = Arc::new(Gate::new());
    let early = gate.prepare();
    gate.notify_all();
    let late = sleeper(&gate, gate.prepare());
    gate.sleep(early);
    assert!(stays_asleep(&late), "released by a notification older than its ticket");
    gate.notify_all();
    assert!(released(&late));
}

#[test]
fn notifications_with_nobody_waiting_are_forgotten() {
    let gate = Arc::new(Gate::new());
    for _ in 0..1000 {
        gate.notify_one();
    }
    let t = gate.prepare();
    gate.cancel(t);
    let rx = sleeper(&gate, gate.prepare());
    assert!(stays_asleep(&rx), "remembered a notification for nobody");
    gate.notify_one();
    assert!(released(&rx));
}

#[test]
fn random_calls_on_one_thread() {
    // sleep is only called where the rules say it must return at once
    let mut rng = anneal_prelude::Rng::new(111_001);
    within(10, move || {
        for _ in 0..500 {
            let gate = Gate::new();
            // (ticket, was there a notification since its prepare?)
            let mut tickets: Vec<(Ticket, bool)> = Vec::new();
            for _ in 0..40 {
                match rng.below(5) {
                    0 | 1 => tickets.push((gate.prepare(), false)),
                    2 => {
                        if rng.bool() {
                            gate.notify_all();
                        } else {
                            gate.notify_one();
                            // with one waiter, that one must be released
                            if tickets.len() != 1 {
                                continue;
                            }
                        }
                        tickets.iter_mut().for_each(|t| t.1 = true);
                    }
                    3 if !tickets.is_empty() => {
                        let i = rng.below(tickets.len());
                        gate.cancel(tickets.remove(i).0);
                    }
                    4 => {
                        if let Some(i) = tickets.iter().position(|t| t.1) {
                            gate.sleep(tickets.remove(i).0);
                        }
                    }
                    _ => {}
                }
                assert_eq!(gate.waiting(), tickets.len());
            }
        }
    });
}

/// Consumers use the gate exactly as in the statement. The producer waits for each item to be taken before it adds the
/// next, so the consumers are idle almost all the time and one lost notification stops everything.
#[test]
fn no_work_is_ever_slept_through() {
    for consumers in [1, 2, 4] {
        within(12, move || {
            const ITEMS: usize = 10_000;
            let gate = Arc::new(Gate::new());
            let queue = Arc::new(Mutex::new(VecDeque::new()));
            let taken = Arc::new(AtomicUsize::new(0));
            let stop = Arc::new(AtomicBool::new(false));
            let threads: Vec<_> = (0..consumers)
                .map(|_| {
                    let (gate, queue, taken, stop) = (gate.clone(), queue.clone(), taken.clone(), stop.clone());
                    std::thread::spawn(move || loop {
                        let pop = || queue.lock().unwrap().pop_front() as Option<usize>;
                        if pop().is_some() {
                            taken.fetch_add(1, Ordering::SeqCst);
                            continue;
                        }
                        let ticket = gate.prepare();
                        if pop().is_some() {
                            gate.cancel(ticket);
                            taken.fetch_add(1, Ordering::SeqCst);
                        } else if stop.load(Ordering::SeqCst) {
                            gate.cancel(ticket);
                            return;
                        } else {
                            gate.sleep(ticket);
                        }
                    })
                })
                .collect();
            for i in 0..ITEMS {
                queue.lock().unwrap().push_back(i);
                gate.notify_one();
                while taken.load(Ordering::SeqCst) <= i {
                    std::thread::yield_now();
                }
            }
            stop.store(true, Ordering::SeqCst);
            gate.notify_all();
            threads.into_iter().for_each(|t| t.join().unwrap());
        });
    }
}

#[test]
fn bursts_from_several_producers() {
    within(12, || {
        let gate = Arc::new(Gate::new());
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let taken = Arc::new(AtomicUsize::new(0));
        let consumers: Vec<_> = (0..3)
            .map(|_| {
                let (gate, queue, taken) = (gate.clone(), queue.clone(), taken.clone());
                std::thread::spawn(move || {
                    while taken.load(Ordering::SeqCst) < 12_000 {
                        let pop = || queue.lock().unwrap().pop_front() as Option<()>;
                        let mut got = pop();
                        if got.is_none() {
                            let ticket = gate.prepare();
                            got = pop();
                            if got.is_some() || taken.load(Ordering::SeqCst) >= 12_000 {
                                gate.cancel(ticket);
                            } else {
                                gate.sleep(ticket);
                            }
                        }
                        if got.is_some() && taken.fetch_add(1, Ordering::SeqCst) + 1 == 12_000 {
                            gate.notify_all();
                        }
                    }
                })
            })
            .collect();
        let producers: Vec<_> = (0..3)
            .map(|_| {
                let (gate, queue) = (gate.clone(), queue.clone());
                std::thread::spawn(move || {
                    for i in 0..4000 {
                        queue.lock().unwrap().push_back(());
                        gate.notify_one();
                        if i % 37 == 0 {
                            std::thread::yield_now();
                        }
                    }
                })
            })
            .collect();
        producers.into_iter().chain(consumers).for_each(|t| t.join().unwrap());
    });
}

/// Sleeps on `ticket` on another thread. The first receiver gets a message when it returns, the second one as soon as it
/// has either gone to sleep on the `Signal` or returned.
#[allow(dead_code)]
fn watched_sleeper(gate: &Arc<Gate>, ticket: Ticket) -> (Receiver<()>, Receiver<()>) {
    let (done_tx, done) = channel();
    let (settled_tx, settled) = channel();
    let gate = gate.clone();
    std::thread::spawn(move || {
        let asleep = settled_tx.clone();
        monitor::when_next_asleep(move || {
            let _ = asleep.send(());
        });
        gate.sleep(ticket);
        let _ = settled_tx.send(());
        let _ = done_tx.send(());
    });
    (done, settled)
}

/// After this thread's next release of a lock, a new ticket goes to sleep on a thread of its own (this one may still hold
/// a lock that `prepare` needs). If it can't get there yet, the same is tried again after the following release.
#[allow(dead_code)]
fn newcomer_after_unlock(gate: Arc<Gate>, started: Option<Receiver<()>>) {
    monitor::after_next_unlock(move || {
        let settled = started.unwrap_or_else(|| {
            let (settled_tx, settled) = channel();
            let gate = gate.clone();
            std::thread::spawn(move || {
                let asleep = settled_tx.clone();
                monitor::when_next_asleep(move || {
                    let _ = asleep.send(());
                });
                let ticket = gate.prepare();
                gate.sleep(ticket);
                let _ = settled_tx.send(());
            });
            settled
        });
        // two seconds are enough to fall asleep unless a lock that is still held is in the way
        if settled.recv_timeout(Duration::from_secs(2)).is_err() {
            newcomer_after_unlock(gate, Some(settled));
        }
    });
}

/// Forces one schedule: a newer ticket goes to sleep the moment `notify_one` first releases a lock. The `Signal` wakes
/// the newest sleeper, so a gate that signals after unlocking hands its only signal to the newcomer.
#[test]
fn a_newer_sleeper_cannot_take_the_signal() {
    within(10, || {
        let gate = Arc::new(Gate::new());
        let (first_done, first_settled) = watched_sleeper(&gate, gate.prepare());
        first_settled
            .recv_timeout(Duration::from_secs(5))
            .expect("`sleep` did not go to sleep on the given `Signal`");
        newcomer_after_unlock(gate.clone(), None);
        gate.notify_one();
        assert!(
            released(&first_done),
            "the ticket that was waiting when `notify_one` was called is still asleep: a newer sleeper took the signal"
        );
        gate.notify_all();
    });
}

/// A condition variable may wake a thread for no reason. Here the first wait does, with nothing notified.
#[test]
fn a_wake_up_for_no_reason_releases_nothing() {
    let gate = Arc::new(Gate::new());
    let ticket = gate.prepare();
    let (tx, rx) = channel();
    let g = gate.clone();
    std::thread::spawn(move || {
        monitor::wake_once_for_no_reason();
        let asleep = tx.clone();
        monitor::when_next_asleep(move || {
            let _ = asleep.send("asleep");
        });
        g.sleep(ticket);
        let _ = tx.send("returned");
    });
    let next = || rx.recv_timeout(Duration::from_secs(5));
    assert_eq!(next(), Ok("asleep"), "`sleep` must wait again on the given `Signal` after a wake-up for no reason");
    gate.notify_one();
    assert_eq!(next(), Ok("returned"));
}

#[test]
fn it_is_shared_between_threads() {
    fn both<T: Send + Sync>() {}
    both::<Gate>();
}
