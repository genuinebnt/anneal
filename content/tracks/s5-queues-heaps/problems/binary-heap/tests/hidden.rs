use solution::*;

use std::cell::Cell;
use std::cmp::Ordering;
use std::rc::Rc;

fn drain<T>(q: &mut PriorityQueue<T>) -> Vec<T> {
    std::iter::from_fn(|| q.pop()).collect()
}

/// Counts how often items are compared.
struct Counted {
    value: u64,
    count: Rc<Cell<u64>>,
}

impl PartialEq for Counted {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Counted {}
impl PartialOrd for Counted {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Counted {
    fn cmp(&self, other: &Self) -> Ordering {
        self.count.set(self.count.get() + 1);
        self.value.cmp(&other.value)
    }
}

#[test]
fn core_single_item_and_reuse() {
    let mut q = PriorityQueue::new();
    q.push(1);
    check!("pop", (q.pop(), q.pop()), (Some(1), None));
    q.push(2);
    check!("after refill", (q.len(), q.pop()), (1, Some(2)));
}

#[test]
fn core_random_operations_match_a_sorted_vec() {
    let mut rng = anneal_prelude::Rng::new(5101);
    for _ in 0..400 {
        let mut q = PriorityQueue::new();
        let mut model: Vec<i64> = Vec::new();
        let mut trace = Vec::new();
        for _ in 0..60 {
            if rng.below(3) == 0 {
                trace.push("pop".to_string());
                check!(format!("{trace:?}"), q.pop(), model.pop());
            } else {
                let x = rng.int(-8, 8);
                trace.push(format!("push {x}"));
                q.push(x);
                model.push(x);
                model.sort_unstable();
            }
            check!(format!("{trace:?}: (len, peek)"), (q.len(), q.peek().copied()), (model.len(), model.last().copied()));
        }
    }
}

#[test]
fn core_items_need_not_be_clone() {
    #[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
    struct Job(u8);
    let mut q = PriorityQueue::new();
    q.push(Job(1));
    q.push(Job(2));
    check!("pop", q.pop(), Some(Job(2)));
}

#[test]
fn core_logarithmic_comparisons() {
    let count = Rc::new(Cell::new(0));
    let n = 4096u64;
    let mut q = PriorityQueue::new();
    // Scrambled: 1031 is coprime with 4096.
    for i in 0..n {
        q.push(Counted { value: (i * 1031) % n, count: count.clone() });
    }
    let mut got = Vec::new();
    while let Some(item) = q.pop() {
        got.push(item.value);
    }
    check!("pop order", got, (0..n).rev().collect::<Vec<_>>());
    let limit = 3 * n * 12;
    check!(format!("{} comparisons for {n} pushes and pops, limit {limit}", count.get()), count.get() <= limit, true);
}

#[test]
fn core_ascending_pushes_stay_logarithmic() {
    let count = Rc::new(Cell::new(0));
    let n = 4096u64;
    let mut q = PriorityQueue::new();
    for i in 0..n {
        q.push(Counted { value: i, count: count.clone() });
    }
    check!(format!("{} comparisons for {n} ascending pushes", count.get()), count.get() <= 2 * n * 12, true);
}

#[test]
fn core_scale_large_items_move_little() {
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    struct Big {
        key: u64,
        body: [u64; 63],
    }
    let n = 50_000u64;
    let mut q = PriorityQueue::new();
    // Ascending pops from the far end of a sorted list, so push in descending order too.
    for i in 0..n / 2 {
        q.push(Big { key: n - i, body: [i; 63] });
    }
    for i in 0..n / 2 {
        q.push(Big { key: (i * 7919) % (n / 2), body: [i; 63] });
    }
    for i in 0..n / 2 {
        q.push(Big { key: 0, body: [i; 63] });
    }
    let mut last = u64::MAX;
    let mut popped = 0;
    while let Some(item) = q.pop() {
        assert!(item.key <= last, "pops went up: {} after {last}", item.key);
        last = item.key;
        popped += 1;
    }
    check!("items popped", popped, 75_000);
}

#[test]
fn part2_is_linear() {
    let count = Rc::new(Cell::new(0));
    let n = 1u64 << 16;
    let items: Vec<Counted> = (0..n).map(|value| Counted { value, count: count.clone() }).collect();
    let q = PriorityQueue::from_vec(items);
    check!(format!("{} comparisons to build from {n} ascending items, limit {}", count.get(), 4 * n), count.get() <= 4 * n, true);
    check!("greatest", q.peek().map(|c| c.value), Some(n - 1));
}

#[test]
fn part2_is_linear_on_scrambled_items() {
    // Sorted input is cheap for a sort, so this one is scrambled: 1031 is coprime with 2^16.
    let count = Rc::new(Cell::new(0));
    let n = 1u64 << 16;
    let items: Vec<Counted> = (0..n).map(|i| Counted { value: (i * 1031) % n, count: count.clone() }).collect();
    let q = PriorityQueue::from_vec(items);
    check!(format!("{} comparisons to build from {n} scrambled items, limit {}", count.get(), 4 * n), count.get() <= 4 * n, true);
    check!("greatest", q.peek().map(|c| c.value), Some(n - 1));
}

#[test]
fn part2_matches_sorting() {
    let mut rng = anneal_prelude::Rng::new(5102);
    for _ in 0..400 {
        let len = rng.below(30);
        let items: Vec<i64> = rng.vec(len, -9, 9);
        let mut want = items.clone();
        want.sort_unstable_by(|a, b| b.cmp(a));
        let mut q = PriorityQueue::from_vec(items.clone());
        check!(format!("from_vec({items:?})"), drain(&mut q), want);
    }
}

#[test]
fn part2_push_after_building() {
    let mut q = PriorityQueue::from_vec(vec![1, 5, 3]);
    q.push(4);
    q.push(6);
    check!("pops", drain(&mut q), vec![6, 5, 4, 3, 1]);
}

#[test]
fn part3_comparator_captures_state() {
    let weights = [30, 10, 20];
    let mut q = PriorityQueue::with_comparator(move |a: &usize, b: &usize| weights[*a].cmp(&weights[*b]));
    for i in 0..3 {
        q.push(i);
    }
    check!("indices by weight", drain(&mut q), vec![0, 2, 1]);
}

#[test]
fn part3_items_without_ord() {
    let mut q = PriorityQueue::with_comparator(|a: &f64, b: &f64| a.total_cmp(b));
    for x in [1.5, -2.0, 9.25] {
        q.push(x);
    }
    check!("floats", drain(&mut q), vec![9.25, 1.5, -2.0]);
}

#[test]
fn part4_edges() {
    check!("k = 0", top_k(vec![1, 2, 3], 0), Vec::<i32>::new());
    check!("empty stream", top_k(Vec::<i32>::new(), 3), vec![]);
    check!("duplicates", top_k(vec![4, 4, 4, 1], 2), vec![4, 4]);
}

#[test]
fn part4_matches_sorting() {
    let mut rng = anneal_prelude::Rng::new(5103);
    for _ in 0..400 {
        let len = rng.below(30);
        let k = rng.below(8);
        let items: Vec<i64> = rng.vec(len, -9, 9);
        let mut want = items.clone();
        want.sort_unstable_by(|a, b| b.cmp(a));
        want.truncate(k);
        check!(format!("top_k({items:?}, {k})"), top_k(items.clone(), k), want);
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Live {
    value: u64,
    guard: Guard,
}

struct Guard(Rc<Cell<i64>>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
impl PartialEq for Guard {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for Guard {}
impl PartialOrd for Guard {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Guard {
    fn cmp(&self, _: &Self) -> Ordering {
        Ordering::Equal
    }
}

#[test]
fn part4_keeps_at_most_k_plus_one_alive() {
    let alive = Rc::new(Cell::new(0i64));
    let peak = Rc::new(Cell::new(0i64));
    let (a, p) = (alive.clone(), peak.clone());
    let stream = (0..10_000u64).map(move |i| {
        a.set(a.get() + 1);
        p.set(p.get().max(a.get()));
        Live { value: (i * 7919) % 10_000, guard: Guard(a.clone()) }
    });
    let got: Vec<u64> = top_k(stream, 5).iter().map(|l| l.value).collect();
    check!("top 5", got, vec![9999, 9998, 9997, 9996, 9995]);
    check!(format!("at most {} items alive at once, limit 6", peak.get()), peak.get() <= 6, true);
}
