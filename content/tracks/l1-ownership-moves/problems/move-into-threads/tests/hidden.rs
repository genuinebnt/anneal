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
fn empty_chunks_inside() {
    check!(r#"sum_chunks([[], [1], []])"#, sum_chunks(vec![vec![], vec![1], vec![]], add), vec![0, 1, 0]);
}

#[test]
fn beyond_u32() {
    check!(r#"sum_chunks([[5000000000, 5000000000]])"#, sum_chunks(vec![vec![5_000_000_000, 5_000_000_000]], add), vec![10_000_000_000]);
}

#[test]
fn sixty_four_chunks() {
    check!(r#"64 chunks [i]"#, sum_chunks((0..64).map(|i| vec![i]).collect(), add), (0..64).collect::<Vec<u64>>());
}

#[test]
fn one_part() {
    check!(r#"sum_parts([1, 2, 3], 1)"#, sum_parts(&[1, 2, 3], 1, add), vec![6]);
}

#[test]
fn even_split() {
    check!(r#"sum_parts([1, 1, 1, 1, 1, 1], 3)"#, sum_parts(&[1; 6], 3, add), vec![2, 2, 2]);
}

#[test]
fn longer_pieces_first() {
    check!(r#"sum_parts([1, 10, 100, 1000, 10000], 3): pieces of 2, 2, 1"#, sum_parts(&[1, 10, 100, 1000, 10000], 3, add), vec![11, 1100, 10000]);
}

#[test]
fn empty_data() {
    check!(r#"sum_parts([], 3)"#, sum_parts(&[], 3, add), vec![0, 0, 0]);
}

#[test]
fn parts_equal_len() {
    check!(r#"sum_parts([7, 8, 9], 3)"#, sum_parts(&[7, 8, 9], 3, add), vec![7, 8, 9]);
}

#[test]
fn big_data() {
    let data: Vec<u64> = (1..=200_000).collect();
    check!(r#"sum_parts(1..=200000, 8)"#, sum_parts(&data, 8, add).iter().sum::<u64>(), 20_000_100_000);
}

static CHUNKS_GATE: Gate = Gate::new();

fn chunks_gate(xs: &[u64]) -> u64 {
    CHUNKS_GATE.sum(xs)
}

#[test]
fn chunks_run_at_once() {
    CHUNKS_GATE.expect(4);
    let sums = sum_chunks(vec![vec![1], vec![2, 2], vec![3, 3, 3], vec![4; 4]], chunks_gate);
    let (summed, at_once, own_threads) = CHUNKS_GATE.report();
    check!("sum_chunks of 4 chunks with a sum that waits until all 4 are being summed: (sums, chunks summed, all at once, each on its own thread)", (sums, summed, at_once, own_threads), (vec![1, 4, 9, 16], 4, true, true));
}

static MANY_GATE: Gate = Gate::new();

fn many_gate(xs: &[u64]) -> u64 {
    MANY_GATE.sum(xs)
}

#[test]
fn eight_pieces_run_at_once() {
    let data: Vec<u64> = (1..=100).collect();
    MANY_GATE.expect(8);
    let sums = sum_parts(&data, 8, many_gate);
    let (summed, at_once, own_threads) = MANY_GATE.report();
    check!("sum_parts(1..=100, 8) with a sum that waits until all 8 pieces are being summed: (total, pieces summed, all at once, each on its own thread)", (sums.iter().sum::<u64>(), summed, at_once, own_threads), (5050, 8, true, true));
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(6111);
    for _ in 0..100 {
        let n = rng.below(12);
        let data: Vec<u64> = rng.vec(n, 0, 1_000_000_000_000);
        let parts = rng.below(6) + 1;
        let mut want = Vec::new();
        let mut start = 0;
        for i in 0..parts {
            let len = n / parts + if i < n % parts { 1 } else { 0 };
            want.push(data[start..start + len].iter().sum::<u64>());
            start += len;
        }
        check!(format!("sum_parts({data:?}, {parts})"), sum_parts(&data, parts, add), want);
        let k = rng.below(5);
        let mut chunks = Vec::new();
        for _ in 0..k {
            let len = rng.below(5);
            chunks.push(rng.vec::<u64>(len, 0, 1000));
        }
        let want: Vec<u64> = chunks.iter().map(|c| c.iter().sum()).collect();
        check!(format!("sum_chunks({chunks:?})"), sum_chunks(chunks.clone(), add), want);
    }
}
