use solution::*;

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}

        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        fn wait_until_waiting(s: &Semaphore, n: usize) {
            for _ in 0..400 {
                if s.waiting() == n {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("never saw {n} waiting threads");
        }

        #[test]
        fn a_waiter_is_woken_by_a_release() {
            let s = Arc::new(Semaphore::new(1));
            let p = s.acquire(1);
            let s2 = s.clone();
            let h = std::thread::spawn(move || {
                let _q = s2.acquire(1);
                7
            });
            wait_until_waiting(&s, 1);
            drop(p);
            check!("blocked acquire completes after the drop", within(2000, move || h.join().unwrap()), Some(7));
        }

        #[test]
        fn big_request_is_not_overtaken_by_small_ones() {
            // total 2; main holds 1. B asks for 2 and must wait. C asks for 1 later: one permit IS free, but B is first in line.
            let s = Arc::new(Semaphore::new(2));
            let order = Arc::new(Mutex::new(Vec::new()));
            let held = s.acquire(1);
            let (sb, ob) = (s.clone(), order.clone());
            let b = std::thread::spawn(move || {
                let _p = sb.acquire(2);
                ob.lock().unwrap().push("B");
            });
            wait_until_waiting(&s, 1);
            let (sc, oc) = (s.clone(), order.clone());
            let c = std::thread::spawn(move || {
                let _p = sc.acquire(1);
                oc.lock().unwrap().push("C");
            });
            wait_until_waiting(&s, 2);
            std::thread::sleep(Duration::from_millis(60));
            check!("C must still be waiting behind B", order.lock().unwrap().len(), 0);
            check!("try_acquire must not jump the queue either", s.try_acquire(1).is_none(), true);
            drop(held);
            within(3000, move || {
                b.join().unwrap();
                c.join().unwrap();
            })
            .expect("waiters never finished");
            check!("B before C", order.lock().unwrap().clone(), vec!["B", "C"]);
            check!("all permits back", s.available(), 2);
        }

        #[test]
        fn waiters_are_served_in_arrival_order() {
            let s = Arc::new(Semaphore::new(1));
            let order = Arc::new(Mutex::new(Vec::new()));
            let held = s.acquire(1);
            let mut hs = Vec::new();
            for i in 0..5usize {
                let (s2, o2) = (s.clone(), order.clone());
                hs.push(std::thread::spawn(move || {
                    let _p = s2.acquire(1);
                    o2.lock().unwrap().push(i);
                }));
                wait_until_waiting(&s, i + 1);
            }
            drop(held);
            within(3000, move || {
                for h in hs {
                    h.join().unwrap();
                }
            })
            .expect("waiters never finished");
            check!("served in the order they arrived", order.lock().unwrap().clone(), vec![0, 1, 2, 3, 4]);
        }

        #[test]
        fn never_more_holders_than_permits() {
            let s = Arc::new(Semaphore::new(3));
            let inside = Arc::new(AtomicUsize::new(0));
            let peak = Arc::new(AtomicUsize::new(0));
            let hs: Vec<_> = (0..8usize)
                .map(|t| {
                    let (s, inside, peak) = (s.clone(), inside.clone(), peak.clone());
                    std::thread::spawn(move || {
                        for i in 0..300 {
                            let n = 1 + (t + i) % 3;
                            let _p = s.acquire(n);
                            let now = inside.fetch_add(n, Ordering::SeqCst) + n;
                            peak.fetch_max(now, Ordering::SeqCst);
                            std::thread::yield_now();
                            inside.fetch_sub(n, Ordering::SeqCst);
                        }
                    })
                })
                .collect();
            let done = within(15_000, move || {
                for h in hs {
                    h.join().unwrap();
                }
            });
            assert!(done.is_some(), "threads deadlocked");
            check!("peak permits held at once, available at the end", (peak.load(Ordering::SeqCst) <= 3, s.available()), (true, 3));
        }

        #[test]
        fn try_acquire_after_queue_drains() {
            let s = Arc::new(Semaphore::new(2));
            let p = s.acquire(2);
            let s2 = s.clone();
            let h = std::thread::spawn(move || drop(s2.acquire(1)));
            wait_until_waiting(&s, 1);
            drop(p);
            within(2000, move || h.join().unwrap()).unwrap();
            check!("queue empty again: try_acquire works", s.try_acquire(2).is_some(), true);
        }

        #[test]
        fn waiting_counts_only_queued_threads() {
            let s = Arc::new(Semaphore::new(1));
            check!("fresh", s.waiting(), 0);
            let p = s.acquire(1);
            check!("holder is not waiting", s.waiting(), 0);
            let s2 = s.clone();
            let h = std::thread::spawn(move || drop(s2.acquire(1)));
            wait_until_waiting(&s, 1);
            drop(p);
            within(2000, move || h.join().unwrap()).unwrap();
            check!("after everyone is served", s.waiting(), 0);
        }

#[test]
fn single_permit_is_a_mutex() {
    check!(r#"total 1: second try fails until drop"#, { let s = Semaphore::new(1); let a = s.try_acquire(1); let b = s.try_acquire(1).is_none(); drop(a); let c = s.try_acquire(1).is_some(); (b, c) }, (true, true));
}

#[test]
fn full_total_in_one_request() {
    check!(r#"total 4, take all"#, { let s = Semaphore::new(4); let p = s.acquire(4); let a = s.available(); drop(p); (a, s.available()) }, (0, 4));
}
