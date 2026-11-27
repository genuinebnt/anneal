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
fn enter_and_leave() {
    let door = Door::new();
    let a = door.enter();
    let b = door.enter();
    check!("two came in: (given, inside)", ((a.is_some(), b.is_some()), door.inside()), ((true, true), 2));
    drop(a);
    check!("one left", door.inside(), 1);
}

#[test]
fn closed_means_closed() {
    let door = Door::new();
    let visit = door.enter();
    door.close();
    check!("after close: (closed, a new visit is refused, still inside)", (door.is_closed(), door.enter().is_none(), door.inside()), (true, true, 1));
    drop(visit);
    check!("the early visitor left", door.inside(), 0);
}

#[test]
fn shutdown_waits_for_the_last_one_out() {
    run(async {
        let door = Door::new();
        let a = door.enter();
        let b = door.enter();
        let waiting = shutting_down(&door);
        settle().await;
        check!("two inside: finished", waiting.is_finished(), false);
        drop(a);
        settle().await;
        check!("one inside: finished", waiting.is_finished(), false);
        drop(b);
        check!("visits ever given", returned(waiting).await, 2);
    });
}

#[test]
fn an_empty_door_shuts_at_once() {
    run(async {
        let door = Door::new();
        drop(door.enter());
        check!("shutdown", timeout(ms(0), door.shutdown()).await.ok(), Some(1));
    });
}

#[test]
fn refused_after_shutdown() {
    run(async {
        let door = Door::new();
        let total = door.shutdown().await;
        check!("an unused door: (total, a later visit is refused)", (total, door.enter().is_none()), (0, true));
    });
}
