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
fn rows_land_in_their_places() {
    check!(
        "3 by 4",
        paint_sheet(3, 4, 2, &pattern),
        Some(expected(3, 4))
    );
}

#[test]
fn each_row_once() {
    let calls: Vec<AtomicUsize> = (0..20).map(|_| AtomicUsize::new(0)).collect();
    let sheet = paint_sheet(5, 20, 4, &|number, row| {
        assert_eq!(row.len(), 5);
        calls[number].fetch_add(1, Ordering::SeqCst);
    });
    check!("sheet starts as zeros", sheet, Some(vec![0; 100]));
    check!(
        "once each",
        calls.iter().all(|c| c.load(Ordering::SeqCst) == 1),
        true
    );
}

#[test]
fn no_sheet() {
    check!("no columns", paint_sheet(0, 5, 2, &pattern), None);
    check!("no rows", paint_sheet(5, 0, 2, &pattern), None);
}

#[test]
fn one_slow_row_holds_up_nobody() {
    // The first row to be painted is not finished until all the others are.
    let gate = Gate::default();
    let first = AtomicBool::new(true);
    let sheet = paint_sheet(2, 12, 2, &|number, row| {
        if first.swap(false, Ordering::SeqCst) {
            gate.wait_for(11);
        } else {
            gate.arrive();
        }
        pattern(number, row);
    });
    check!(
        "the other worker painted the other eleven rows",
        gate.stuck(),
        false
    );
    check!("sheet", sheet, Some(expected(2, 12)));
}

#[test]
fn workers_really_work_together() {
    // Each of the first four rows waits until four rows have started.
    let gate = Gate::default();
    let sheet = paint_sheet(1, 10, 4, &|_, _| {
        gate.arrive();
        gate.wait_for(4);
    });
    check!(
        "four rows at once",
        (sheet.is_some(), gate.stuck()),
        (true, false)
    );
}
