use solution::*;

use std::sync::{Condvar, Mutex};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

fn add(xs: &[u64]) -> u64 {
    xs.iter().sum()
}

/// Every call waits until `expected` calls are in progress at once, or 3 seconds pass.
struct Gate {
    state: Mutex<GateState>,
    arrived: Condvar,
}

struct GateState {
    expected: usize,
    threads: Vec<ThreadId>,
    gave_up: bool,
}

impl Gate {
    const fn new() -> Self {
        Gate { state: Mutex::new(GateState { expected: 0, threads: Vec::new(), gave_up: false }), arrived: Condvar::new() }
    }

    fn expect(&self, n: usize) {
        *self.state.lock().unwrap() = GateState { expected: n, threads: Vec::new(), gave_up: false };
    }

    fn sum(&self, xs: &[u64]) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut s = self.state.lock().unwrap();
        s.threads.push(thread::current().id());
        self.arrived.notify_all();
        while s.threads.len() < s.expected && !s.gave_up {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                s.gave_up = true;
                self.arrived.notify_all();
                break;
            }
            s = self.arrived.wait_timeout(s, left).unwrap().0;
        }
        xs.iter().sum()
    }

    /// How many pieces went through `sum`, whether they were all in it at once, and whether each ran on its
    /// own thread, not the caller's.
    fn report(&self) -> (usize, bool, bool) {
        let s = self.state.lock().unwrap();
        let mut ids = s.threads.clone();
        ids.sort_by_key(|id| format!("{id:?}"));
        ids.dedup();
        let own = ids.len() == s.threads.len() && !s.threads.contains(&thread::current().id());
        (s.threads.len(), !s.gave_up && s.threads.len() == s.expected, own)
    }
}

#[test]
fn chunks_in_order() {
    check!(r#"sum_chunks([[1, 2], [3], [4, 5, 6]])"#, sum_chunks(vec![vec![1, 2], vec![3], vec![4, 5, 6]], add), vec![3, 3, 15]);
}

#[test]
fn no_chunks() {
    check!(r#"sum_chunks([])"#, sum_chunks(vec![], add), Vec::<u64>::new());
}

#[test]
fn parts_split() {
    let data: Vec<u64> = (1..=10).collect();
    check!(r#"sum_parts(1..=10, 3): pieces of 4, 3, 3"#, sum_parts(&data, 3, add), vec![10, 18, 27]);
}

#[test]
fn data_still_usable() {
    let data = vec![1u64, 2, 3];
    check!(r#"sum_parts borrows: data used afterwards"#, { let s = sum_parts(&data, 2, add); (s, data.len()) }, (vec![3, 3], 3));
}

#[test]
fn more_parts_than_items() {
    check!(r#"sum_parts([5, 6], 4)"#, sum_parts(&[5, 6], 4, add), vec![5, 6, 0, 0]);
}

static PARTS_GATE: Gate = Gate::new();

fn parts_gate(xs: &[u64]) -> u64 {
    PARTS_GATE.sum(xs)
}

#[test]
fn pieces_run_at_once() {
    let data: Vec<u64> = (1..=9).collect();
    PARTS_GATE.expect(3);
    let sums = sum_parts(&data, 3, parts_gate);
    let (summed, at_once, own_threads) = PARTS_GATE.report();
    check!("sum_parts(1..=9, 3) with a sum that waits until all 3 pieces are being summed: (sums, pieces summed, all at once, each on its own thread)", (sums, summed, at_once, own_threads), (vec![6, 15, 24], 3, true, true));
}
