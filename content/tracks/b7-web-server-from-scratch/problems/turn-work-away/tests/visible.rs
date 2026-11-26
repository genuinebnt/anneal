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
fn items_are_handled() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let service = Service::start(1, 8, move |i: u32| s.lock().unwrap().push(i));
    for i in 0..5 {
        service.offer(i).unwrap();
    }
    service.close();
    check!("five items, one worker", seen.lock().unwrap().clone(), vec![0, 1, 2, 3, 4]);
}

#[test]
fn the_room_fills_and_then_offers_are_refused() {
    let hold = Hold::default();
    let service = busy(&hold, 1, 2);
    let got = within(move || {
        let got = (service.offer(10), service.offer(11), service.offer(12), service.waiting());
        hold.open();
        got
    });
    check!("1 worker busy, room 2", got, Some((Ok(()), Ok(()), Err(12), 2)));
}

#[test]
fn workers_run_at_the_same_time() {
    let hold = Hold::default();
    let service = busy(&hold, 3, 1);
    check!("three inside at once", hold.wait_for(3).len(), 3);
    hold.open();
    service.close();
}

#[test]
fn close_handles_what_was_accepted() {
    let hold = Hold::default();
    let service = busy(&hold, 1, 4);
    for i in 1..5 {
        service.offer(i).unwrap();
    }
    hold.open();
    let totals = within(move || service.close());
    check!("totals", totals, Some(Totals { accepted: 5, refused: 0, handled: 5, panicked: 0 }));
}

#[test]
fn a_panic_costs_only_its_item() {
    let service = Service::start(1, 8, |i: u32| assert!(i != 1, "item 1 is bad"));
    for i in 0..4 {
        service.offer(i).unwrap();
    }
    check!("totals", within(move || service.close()), Some(Totals { accepted: 4, refused: 0, handled: 3, panicked: 1 }));
}
