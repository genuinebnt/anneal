#[allow(unused_imports)]
use anneal_prelude::Rng;
use solution::*;
#[allow(unused_imports)]
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// Only so that a program that is stuck ends with a message. A correct program never waits this long:
/// what it waits for is already on its way.
#[allow(dead_code)]
const STUCK: Duration = Duration::from_secs(8);

/// A count that threads can wait on. After one wait has given up, nobody waits again.
#[allow(dead_code)]
#[derive(Default)]
struct Gate {
    count: Mutex<usize>,
    changed: Condvar,
    gave_up: AtomicBool,
}

#[allow(dead_code)]
impl Gate {
    fn arrive(&self) {
        *self.count.lock().unwrap() += 1;
        self.changed.notify_all();
    }

    fn wait_for(&self, wanted: usize) {
        if self.gave_up.load(Ordering::SeqCst) {
            return;
        }
        let guard = self.count.lock().unwrap();
        let (_guard, result) = self
            .changed
            .wait_timeout_while(guard, STUCK, |n| *n < wanted)
            .unwrap();
        if result.timed_out() {
            self.gave_up.store(true, Ordering::SeqCst);
        }
    }

    fn stuck(&self) -> bool {
        self.gave_up.load(Ordering::SeqCst)
    }
}

#[allow(dead_code)]
fn pattern(number: usize, row: &mut [u32]) {
    for (column, cell) in row.iter_mut().enumerate() {
        *cell = (number * 1000 + column) as u32;
    }
}

#[allow(dead_code)]
fn expected(width: usize, height: usize) -> Vec<u32> {
    (0..width * height)
        .map(|i| ((i / width) * 1000 + i % width) as u32)
        .collect()
}

#[test]
fn any_number_of_workers_gives_the_same_sheet() {
    let mut rng = Rng::new(1635);
    for case in 0..60 {
        let (width, height) = (rng.int(1, 30) as usize, rng.int(1, 40) as usize);
        let workers = *rng.pick(&[0, 1, 2, 3, 7, 64, 65, 1000, usize::MAX]);
        check!(
            format!("case {case}: {width} x {height}, {workers} workers"),
            paint_sheet(width, height, workers, &pattern) == Some(expected(width, height)),
            true
        );
    }
}

/// Paints and reports the largest number of rows in progress at one moment, and how many threads painted.
#[allow(dead_code)]
fn watch(height: usize, workers: usize) -> (usize, usize) {
    let now = AtomicUsize::new(0);
    let most = AtomicUsize::new(0);
    let who = Mutex::new(HashSet::new());
    paint_sheet(1, height, workers, &|_, _| {
        most.fetch_max(now.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
        who.lock().unwrap().insert(std::thread::current().id());
        std::thread::yield_now();
        now.fetch_sub(1, Ordering::SeqCst);
    })
    .unwrap();
    let threads = who.lock().unwrap().len();
    (most.load(Ordering::SeqCst), threads)
}

#[test]
fn never_more_than_asked() {
    for workers in [0, 1, 2, 3, 8] {
        let (most, threads) = watch(400, workers);
        check!(
            format!("{workers} workers: {most} rows at once, {threads} threads"),
            most <= workers.max(1) && threads <= workers.max(1),
            true
        );
    }
}

#[test]
fn never_more_than_sixty_four() {
    for workers in [65, 5000, usize::MAX] {
        let (most, threads) = watch(5000, workers);
        check!(
            format!("{workers} asked: {most} rows at once on {threads} threads"),
            most <= MAX_WORKERS && threads <= MAX_WORKERS,
            true
        );
    }
}

#[test]
fn no_more_threads_than_rows() {
    let (_, threads) = watch(3, 64);
    check!(format!("3 rows on {threads} threads"), threads <= 3, true);
}

#[test]
fn eight_at_once_with_a_slow_one() {
    // Seven rows meet, then one of them stays until all the rest of 100 rows are done.
    let meet = Gate::default();
    let rest = Gate::default();
    let order = AtomicUsize::new(0);
    paint_sheet(1, 100, 8, &|_, _| {
        let turn = order.fetch_add(1, Ordering::SeqCst);
        if turn < 8 {
            meet.arrive();
            meet.wait_for(8);
        }
        if turn == 3 {
            rest.wait_for(99);
        } else {
            rest.arrive();
        }
    })
    .unwrap();
    check!("eight met", meet.stuck(), false);
    check!(
        "the other 99 rows finished while one was still being painted",
        rest.stuck(),
        false
    );
}

#[test]
fn a_slow_row_in_the_middle() {
    for slow in [0, 7, 19] {
        let gate = Gate::default();
        paint_sheet(4, 20, 3, &|number, _| {
            if number == slow {
                gate.wait_for(19);
            } else {
                gate.arrive();
            }
        })
        .unwrap();
        check!(
            format!("row {slow} slow: the rest got done"),
            gate.stuck(),
            false
        );
    }
}

#[test]
fn a_panic_reaches_the_caller() {
    for workers in [1, 4] {
        let result = std::panic::catch_unwind(|| {
            paint_sheet(2, 30, workers, &|number, _| {
                if number == 11 {
                    panic!("row 11 is broken");
                }
            })
        });
        check!(
            format!("{workers} workers: the call panics instead of returning a half painted sheet"),
            result.is_err(),
            true
        );
    }
}

#[test]
fn sheets_too_large() {
    for (w, h) in [
        (usize::MAX, 2),
        (2, usize::MAX),
        (1 << 33, 1 << 33),
        (MAX_CELLS + 1, 1),
        (2049, 2048),
    ] {
        check!(format!("{w} x {h}"), paint_sheet(w, h, 4, &pattern), None);
    }
    check!(
        "the largest is allowed",
        paint_sheet(MAX_CELLS, 1, 4, &|_, _| {}).map(|s| s.len()),
        Some(MAX_CELLS)
    );
}
