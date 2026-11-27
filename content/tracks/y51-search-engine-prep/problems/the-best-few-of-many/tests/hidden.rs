use solution::*;
use std::cell::Cell;
use std::cmp::Ordering;
use std::rc::Rc;

fn entry(id: u64, score: f64) -> Entry {
    Entry::new(id, score).expect("a score")
}

fn bits_of(list: &[Entry]) -> Vec<(u64, u64)> {
    list.iter().map(|e| (e.parts().0, e.parts().1.to_bits())).collect()
}

#[test]
fn k_of_zero_keeps_nothing() {
    let mut b = Best::new(0);
    check!("offer and cutoff", (b.offer(1), b.cutoff()), (false, None));
    check!("sorted", b.into_sorted(), Vec::<i32>::new());
}

#[test]
fn a_huge_k_is_not_a_huge_reservation() {
    for k in [usize::MAX, usize::MAX / 2, 1 << 40] {
        let mut b = Best::new(k);
        check!(format!("k = {k}"), (b.offer(1u64), b.cutoff()), (true, None));
        check!(format!("k = {k}, sorted"), b.into_sorted(), vec![1]);
    }
}

#[test]
fn zeros_and_infinities_have_a_fixed_order() {
    let mut list: Vec<Entry> = [(1, -0.0), (2, 0.0), (3, f64::NEG_INFINITY), (4, f64::INFINITY), (5, -1.0)]
        .iter()
        .map(|&(id, s)| entry(id, s))
        .collect();
    list.sort();
    let got: Vec<u64> = list.iter().rev().map(|e| e.parts().0).collect();
    check!("ids, best first", got, vec![4, 2, 1, 5, 3]);
}

#[test]
fn the_order_of_entries_and_equality_agree() {
    let mut rng = anneal_prelude::Rng::new(5105);
    let scores = [-1.0, -0.0, 0.0, 1.0, f64::INFINITY, f64::NEG_INFINITY];
    for _ in 0..2_000 {
        let (a, x) = (rng.below(3) as u64, *rng.pick(&scores));
        let (b, y) = (rng.below(3) as u64, *rng.pick(&scores));
        let want = x.total_cmp(&y).then(b.cmp(&a));
        let (left, right) = (entry(a, x), entry(b, y));
        let got = (left.cmp(&right), left.partial_cmp(&right), left == right);
        check!(format!("({a}, {x}) against ({b}, {y})"), got, (want, Some(want), want == Ordering::Equal));
    }
}

#[test]
fn ids_at_the_ends_of_the_range() {
    let mut b = Best::new(1);
    check!("largest id first", b.offer(entry(u64::MAX, 1.0)), true);
    check!("then id 0 with the same score", b.offer(entry(0, 1.0)), true);
    check!("then the largest again", b.offer(entry(u64::MAX, 1.0)), false);
    check!("kept", bits_of(&b.into_sorted()), vec![(0, 1.0f64.to_bits())]);
}

/// Ordered by `key` only; `tag` tells equal items apart.
#[derive(Debug)]
struct Tagged {
    key: u8,
    tag: u32,
}

impl Ord for Tagged {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}
impl PartialOrd for Tagged {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Tagged {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for Tagged {}

#[test]
fn equal_items_are_kept_apart_and_do_not_displace() {
    let mut b = Best::new(3);
    let stayed: Vec<bool> = (0..5).map(|tag| b.offer(Tagged { key: 7, tag })).collect();
    check!("five equal items into three places", stayed, vec![true, true, true, false, false]);
    let mut tags: Vec<u32> = b.into_sorted().iter().map(|t| t.tag).collect();
    tags.sort();
    check!("the first three", tags, vec![0, 1, 2]);
}

#[test]
fn matches_sorting_everything() {
    let mut rng = anneal_prelude::Rng::new(5113);
    let scores = [-1.0, -0.0, 0.0, 0.5, 1.0, 2.0, f64::INFINITY];
    for _ in 0..300 {
        let k = rng.below(6);
        let mut b = Best::new(k);
        let mut model: Vec<Entry> = Vec::new();
        for _ in 0..rng.below(25) {
            let id = rng.below(8) as u64;
            let score = *rng.pick(&scores);
            let better = |a: &Entry, id: u64, score: f64| a.parts().1.total_cmp(&score).then_with(|| id.cmp(&a.parts().0)).is_ge();
            let before = model.iter().filter(|e| better(e, id, score)).count();
            model.insert(before, entry(id, score));
            model.truncate(k);
            check!(format!("k = {k}, offer ({id}, {score})"), b.offer(entry(id, score)), before < k);
            let cut = (k > 0 && model.len() == k).then(|| bits_of(&model[k - 1..]));
            let got = b.cutoff().map(|e| bits_of(std::slice::from_ref(e)));
            check!(format!("k = {k}, cutoff after ({id}, {score})"), got, cut);
        }
        check!(format!("k = {k}, final"), bits_of(&b.into_sorted()), bits_of(&model));
    }
}

/// A number whose every comparison is counted.
struct Counted {
    value: u64,
    seen: Rc<Cell<u64>>,
    budget: u64,
}

impl Counted {
    fn count(&self) {
        self.seen.set(self.seen.get() + 1);
        assert!(self.seen.get() <= self.budget, "more than the budget of {} comparisons", self.budget);
    }
}
impl Ord for Counted {
    fn cmp(&self, other: &Self) -> Ordering {
        self.count();
        self.value.cmp(&other.value)
    }
}
impl PartialOrd for Counted {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.count();
        Some(self.value.cmp(&other.value))
    }
}
impl PartialEq for Counted {
    fn eq(&self, other: &Self) -> bool {
        self.count();
        self.value == other.value
    }
}
impl Eq for Counted {}

/// Offers every value, asking for the cutoff after each, and returns what is kept.
fn within_budget(k: usize, values: &[u64]) -> Vec<u64> {
    let n = values.len() as u64;
    let m = (k as u64).min(n);
    let bits = u64::from(u64::BITS - m.leading_zeros());
    let budget = (n + 1) * (6 * bits + 12);
    let seen = Rc::new(Cell::new(0));
    let mut b = Best::new(k);
    for &value in values {
        b.offer(Counted { value, seen: seen.clone(), budget });
        let _ = b.cutoff();
    }
    b.into_sorted().iter().map(|c| c.value).collect()
}

fn largest(values: &[u64], k: usize) -> Vec<u64> {
    let mut all = values.to_vec();
    all.sort_by(|a, b| b.cmp(a));
    all.truncate(k);
    all
}

#[test]
fn the_budget_with_random_values() {
    let mut rng = anneal_prelude::Rng::new(5106);
    let values: Vec<u64> = (0..20_000).map(|_| rng.below(1_000_000_000) as u64).collect();
    for k in [1, 10, 1_000] {
        check!(format!("k = {k}: the right items"), within_budget(k, &values) == largest(&values, k), true);
    }
}

#[test]
fn the_budget_with_rising_falling_and_equal_values() {
    let rising: Vec<u64> = (0..20_000).collect();
    let falling: Vec<u64> = (0..20_000).rev().collect();
    let equal = vec![5; 20_000];
    for (name, values) in [("rising", rising), ("falling", falling), ("equal", equal)] {
        for k in [1_000, usize::MAX] {
            check!(format!("{name}, k = {k}"), within_budget(k, &values) == largest(&values, k), true);
        }
    }
}
