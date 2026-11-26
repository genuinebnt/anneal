use solution::*;

#[allow(dead_code, unused_imports)]
use std::sync::{Arc, Condvar, Mutex};
#[allow(dead_code, unused_imports)]
use std::time::{Duration, Instant};

/// Items wait here inside the handler until the test lets them go.
/// The flag, and the items that have arrived.
#[allow(dead_code, unused_imports)]
type Held = (Mutex<(bool, Vec<u32>)>, Condvar);

#[derive(Clone, Default)]
#[allow(dead_code, unused_imports)]
struct Hold(Arc<Held>);

#[allow(dead_code, unused_imports)]
impl Hold {
    fn enter(&self, item: u32) {
        let (lock, cv) = &*self.0;
        let mut s = lock.lock().unwrap();
        s.1.push(item);
        cv.notify_all();
        let _ = cv.wait_timeout_while(s, Duration::from_secs(4), |s| !s.0).unwrap();
    }
    fn open(&self) {
        let (lock, cv) = &*self.0;
        lock.lock().unwrap().0 = true;
        cv.notify_all();
    }
    /// Waits until `n` items are inside the handler (or have been), and returns them.
    fn wait_for(&self, n: usize) -> Vec<u32> {
        let (lock, cv) = &*self.0;
        let s = lock.lock().unwrap();
        let (s, _) = cv.wait_timeout_while(s, Duration::from_secs(3), |s| s.1.len() < n).unwrap();
        assert!(s.1.len() >= n, "only {} of {n} items reached a worker", s.1.len());
        s.1.clone()
    }
    fn service(&self, workers: usize, room: usize) -> Service<u32> {
        let hold = self.clone();
        Service::start(workers, room, move |item| hold.enter(item))
    }
}

/// Runs `f` on another thread; `None` if it has not finished within three seconds.
#[allow(dead_code, unused_imports)]
fn within<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(Duration::from_secs(3)).ok()
}

/// One worker busy with item 0, so that what happens next does not depend on timing.
#[allow(dead_code, unused_imports)]
fn busy(hold: &Hold, workers: usize, room: usize) -> Service<u32> {
    let service = hold.service(workers, room);
    for i in 0..workers {
        assert_eq!(service.offer(i as u32), Ok(()), "offering to an idle service");
        hold.wait_for(i + 1);
    }
    service
}

#[test]
fn zero_workers_or_zero_room_panics() {
    for (w, r) in [(0, 1), (1, 0)] {
        let result = std::panic::catch_unwind(|| drop(Service::<u32>::start(w, r, |_| {})));
        check!(format!("{w} workers, room {r}"), result.is_err(), true);
    }
}

#[test]
fn offer_never_blocks_however_full() {
    let hold = Hold::default();
    let service = busy(&hold, 2, 3);
    let got = within(move || {
        let results: Vec<bool> = (0..50).map(|i| service.offer(100 + i).is_ok()).collect();
        hold.open();
        let totals = service.close();
        (results.iter().filter(|ok| **ok).count(), totals)
    });
    check!("50 offers to a full service", got, Some((3, Totals { accepted: 5, refused: 47, handled: 5, panicked: 0 })));
}

#[test]
fn a_refused_item_comes_back_and_is_never_handled() {
    let hold = Hold::default();
    let service = busy(&hold, 1, 1);
    service.offer(1).unwrap();
    check!("refused", service.offer(99), Err(99));
    hold.open();
    within(move || service.close()).expect("close did not return");
    check!("handled items", hold.wait_for(2), vec![0, 1]);
}

#[test]
fn room_frees_up_as_workers_take_items() {
    let (go, steps) = std::sync::mpsc::channel::<()>();
    let steps = Mutex::new(steps);
    let hold = Hold::default();
    let h = hold.clone();
    // each item enters, then waits for one permit
    h.open();
    let service = Service::start(1, 1, move |i: u32| {
        h.enter(i);
        let _ = steps.lock().unwrap().recv_timeout(Duration::from_secs(4));
    });
    service.offer(0).unwrap();
    hold.wait_for(1);
    service.offer(1).unwrap();
    check!("full", service.offer(2), Err(2));
    go.send(()).unwrap();
    hold.wait_for(2);
    check!("room again", (service.waiting(), service.offer(3)), (0, Ok(())));
    drop(go);
    within(move || service.close()).expect("close did not return");
}

#[test]
fn panics_leave_all_workers() {
    let hold = Hold::default();
    let h = hold.clone();
    let service = Service::start(3, 32, move |i: u32| {
        assert!(i >= 100, "bad item");
        h.enter(i);
    });
    for i in 0..9 {
        service.offer(i).unwrap();
    }
    for i in 100..103 {
        service.offer(i).unwrap();
    }
    check!("three good items inside at once after nine panics", hold.wait_for(3).len(), 3);
    hold.open();
    check!("totals", within(move || service.close()), Some(Totals { accepted: 12, refused: 0, handled: 3, panicked: 9 }));
}

#[test]
fn no_more_threads_than_workers() {
    let ids = Arc::new(Mutex::new(std::collections::HashSet::new()));
    let seen = ids.clone();
    let service = Service::start(3, 500, move |_: u32| {
        seen.lock().unwrap().insert(std::thread::current().id());
    });
    for i in 0..400 {
        service.offer(i).unwrap();
    }
    service.close();
    let n = ids.lock().unwrap().len();
    check!(format!("{n} distinct threads handled 400 items"), (1..=3).contains(&n), true);
}

#[test]
fn drop_finishes_the_queue_and_releases_the_handler() {
    let owned = Arc::new(Mutex::new(0u32));
    let hold = Hold::default();
    let (o, h) = (owned.clone(), hold.clone());
    let service = Service::start(1, 5, move |i: u32| {
        h.enter(i);
        *o.lock().unwrap() += 1;
    });
    service.offer(0).unwrap();
    hold.wait_for(1);
    for i in 1..6 {
        service.offer(i).unwrap();
    }
    hold.open();
    within(move || drop(service)).expect("drop did not return");
    check!("handled, and owners of the handler's data", (*owned.lock().unwrap(), Arc::strong_count(&owned)), (6, 1));
}

#[test]
fn close_waits_for_the_item_in_progress() {
    let hold = Hold::default();
    let service = busy(&hold, 1, 1);
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(service.close());
    });
    check!("not finished while an item is inside", finished.recv_timeout(Duration::from_millis(150)).is_err(), true);
    hold.open();
    check!("then finishes", finished.recv_timeout(Duration::from_secs(3)).ok().map(|t| t.handled), Some(1));
}

#[test]
fn waiting_never_goes_wrong_while_busy() {
    let service = Arc::new(Service::start(4, 2, |_: u32| {}));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let offerers: Vec<_> = (0..2)
        .map(|_| {
            let (service, stop) = (service.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut ok = 0u64;
                for i in 0..15_000 {
                    ok += u64::from(service.offer(i).is_ok());
                }
                stop.store(true, std::sync::atomic::Ordering::SeqCst);
                ok
            })
        })
        .collect();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut worst = 0;
    while !stop.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < deadline {
        worst = worst.max(service.waiting());
    }
    let accepted: u64 = offerers.into_iter().map(|h| h.join().unwrap()).sum();
    check!(format!("largest waiting() seen was {worst}"), worst <= 8, true);
    let service = Arc::try_unwrap(service).ok().expect("still shared");
    let totals = service.close();
    check!("every accepted item was handled", (totals.accepted, totals.handled, totals.accepted + totals.refused), (accepted, accepted, 30_000));
}

#[test]
fn random_sizes_fill_exactly() {
    let mut rng = anneal_prelude::Rng::new(70401);
    for _ in 0..25 {
        let workers = 1 + rng.below(4);
        let room = 1 + rng.below(5);
        let extra = rng.below(6);
        let hold = Hold::default();
        let service = busy(&hold, workers, room);
        let accepted: Vec<bool> = (0..room + extra).map(|i| service.offer(1000 + i as u32).is_ok()).collect();
        let want: Vec<bool> = (0..room + extra).map(|i| i < room).collect();
        let waiting = service.waiting();
        hold.open();
        check!(format!("{workers} workers, room {room}, {extra} extra"), (accepted, waiting), (want, room));
        let totals = service.close();
        check!(format!("{workers} workers, room {room}: handled"), totals.handled, (workers + room) as u64);
    }
}
