use solution::*;

#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
#[allow(unused_imports)]
use std::time::Duration;
#[allow(unused_imports)]
use tokio::time::{sleep, timeout};

#[allow(dead_code)]
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(dead_code)]
pub fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

#[allow(dead_code)]
pub async fn settle() {
    sleep(ms(1)).await;
}

#[allow(dead_code)]
pub fn shutting_down(door: &Door) -> tokio::task::JoinHandle<u64> {
    let door = door.clone();
    tokio::spawn(async move { door.shutdown().await })
}

#[allow(dead_code)]
pub async fn returned(handle: tokio::task::JoinHandle<u64>) -> u64 {
    timeout(Duration::from_secs(3600), handle).await.expect("shutdown never returned").unwrap()
}

#[allow(dead_code)]
pub struct Wakes(pub AtomicUsize);

#[allow(dead_code)]
impl futures::task::ArcWake for Wakes {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn refused_visits_are_not_counted() {
    run(async {
        let door = Door::new();
        drop(door.enter());
        door.close();
        door.close();
        for _ in 0..5 {
            let _ = door.enter();
        }
        check!("one given, five refused", door.shutdown().await, 1);
        check!("inside", door.inside(), 0);
    });
}

#[test]
fn every_waiter_waits() {
    run(async {
        let door = Door::new();
        let visit = door.enter();
        let first = shutting_down(&door);
        settle().await;
        let second = shutting_down(&door);
        let third = shutting_down(&door);
        settle().await;
        check!("one visitor: finished", [first.is_finished(), second.is_finished(), third.is_finished()], [false; 3]);
        drop(visit);
        check!("results", (returned(first).await, returned(second).await, returned(third).await), (1, 1, 1));
        check!("a later one", door.shutdown().await, 1);
    });
}

#[test]
fn close_first_then_wait() {
    run(async {
        let door = Door::new();
        let visit = door.enter();
        door.close();
        let waiting = shutting_down(&door);
        settle().await;
        check!("finished with a visitor inside", waiting.is_finished(), false);
        drop(visit);
        check!("result", returned(waiting).await, 1);
    });
}

#[test]
fn giving_up_leaves_it_closed() {
    run(async {
        let door = Door::new();
        let visit = door.enter();
        let gave_up = timeout(ms(10), door.shutdown()).await.is_err();
        check!("(gave up, closed, new visit refused, inside)", (gave_up, door.is_closed(), door.enter().is_none(), door.inside()), (true, true, true, 1));
        let again = shutting_down(&door);
        settle().await;
        check!("the second wait finished early", again.is_finished(), false);
        drop(visit);
        check!("result", returned(again).await, 1);
    });
}

#[test]
fn a_panic_leaves_too() {
    run(async {
        let door = Door::new();
        let visit = door.enter().unwrap();
        let task = tokio::spawn(async move {
            let _visit = visit;
            sleep(ms(5)).await;
            panic!("handler bug");
        });
        check!("shutdown after the panic", timeout(ms(100), door.shutdown()).await.ok(), Some(1));
        check!("the task panicked", task.await.is_err(), true);
    });
}

#[test]
fn a_visit_outlives_the_door_handles() {
    let visit = {
        let door = Door::new();
        door.enter()
    };
    std::thread::spawn(move || drop(visit)).join().unwrap();
}

#[test]
fn clones_are_one_door() {
    run(async {
        let door = Door::new();
        let other = door.clone();
        let visit = door.enter();
        check!("inside, seen from the clone", other.inside(), 1);
        other.close();
        check!("closed through the clone", door.enter().is_none(), true);
        drop(visit);
        check!("shutdown on the clone", other.shutdown().await, 1);
    });
}

#[test]
fn waiting_does_not_spin() {
    run(async {
        use std::future::Future;
        use std::task::{Context, Poll};
        let door = Door::new();
        let visit = door.enter();
        let wakes = Arc::new(Wakes(AtomicUsize::new(0)));
        let waker = futures::task::waker(wakes.clone());
        let mut cx = Context::from_waker(&waker);
        let mut waiting = Box::pin(door.shutdown());
        let pending = waiting.as_mut().poll(&mut cx).is_pending();
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        sleep(Duration::from_secs(5)).await;
        check!("one visitor: (pending, wake-ups in 5 s)", (pending, wakes.0.load(Ordering::SeqCst)), (true, 0));
        drop(visit);
        check!("woken when the last visitor leaves", wakes.0.load(Ordering::SeqCst) > 0, true);
        check!("and finished", waiting.as_mut().poll(&mut cx), Poll::Ready(1));
    });
}

#[test]
fn nobody_gets_in_after_shutdown_returned() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_time().build().unwrap();
    for round in 0..400 {
        let door = Door::new();
        let over = Arc::new(AtomicBool::new(false));
        let given = Arc::new(AtomicU64::new(0));
        let late = Arc::new(AtomicUsize::new(0));
        let visitors: Vec<_> = (0..4)
            .map(|_| {
                let (door, over, given, late) = (door.clone(), over.clone(), given.clone(), late.clone());
                std::thread::spawn(move || {
                    while let Some(visit) = door.enter() {
                        given.fetch_add(1, Ordering::SeqCst);
                        for _ in 0..20 {
                            if over.load(Ordering::SeqCst) {
                                late.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                        drop(visit);
                    }
                })
            })
            .collect();
        while given.load(Ordering::SeqCst) < 5 + (round % 50) {
            std::hint::spin_loop();
        }
        let total = runtime.block_on(async { timeout(Duration::from_secs(5), door.shutdown()).await.expect("shutdown never returned") });
        over.store(true, Ordering::SeqCst);
        for v in visitors {
            v.join().unwrap();
        }
        check!(format!("round {round}: (visits seen inside after shutdown returned, total reported)"), (late.load(Ordering::SeqCst), total), (0, given.load(Ordering::SeqCst)));
    }
}

#[test]
fn random_sessions_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(21308);
    for _ in 0..300 {
        run(async {
            let door = Door::new();
            let mut visits = Vec::new();
            let mut waiters: Vec<tokio::task::JoinHandle<u64>> = Vec::new();
            let (mut closed, mut admitted) = (false, 0u64);
            let mut log = Vec::new();
            for _ in 0..20 {
                match rng.below(8) {
                    0..=3 => {
                        log.push("enter");
                        let visit = door.enter();
                        check!(format!("{log:?}: admitted"), visit.is_some(), !closed);
                        if let Some(v) = visit {
                            admitted += 1;
                            visits.push(v);
                        }
                    }
                    4 | 5 if !visits.is_empty() => {
                        log.push("leave");
                        visits.swap_remove(rng.below(visits.len()));
                    }
                    6 if rng.below(3) == 0 => {
                        log.push("start shutdown");
                        waiters.push(shutting_down(&door));
                        closed = true;
                    }
                    7 if !waiters.is_empty() => {
                        log.push("give up on a shutdown");
                        waiters.pop().unwrap().abort();
                    }
                    _ => {}
                }
                settle().await;
                check!(format!("{log:?}: (inside, closed)"), (door.inside(), door.is_closed()), (visits.len(), closed));
                for w in &waiters {
                    check!(format!("{log:?}: shutdown finished"), w.is_finished(), visits.is_empty());
                }
                if visits.is_empty() {
                    for w in waiters.drain(..) {
                        check!(format!("{log:?}: total"), w.await.unwrap(), admitted);
                    }
                }
            }
        });
    }
}
