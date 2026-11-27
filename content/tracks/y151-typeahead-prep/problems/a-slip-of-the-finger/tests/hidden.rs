use solution::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone)]
struct Counted {
    value: u32,
    count: Rc<Cell<u64>>,
    limit: u64,
}

impl PartialEq for Counted {
    fn eq(&self, other: &Self) -> bool {
        self.count.set(self.count.get() + 1);
        assert!(self.count.get() <= self.limit, "more than {} comparisons, which is over the budget", self.limit);
        self.value == other.value
    }
}

fn within_budget(a: &[u32], b: &[u32], max: usize) -> Option<usize> {
    let w = max.saturating_mul(2).saturating_add(1).min(b.len() + 1) as u64;
    let limit = 4 * (a.len() as u64 + 1) * w + 16;
    let count = Rc::new(Cell::new(0));
    let wrap = |s: &[u32]| -> Vec<Counted> {
        s.iter().map(|&value| Counted { value, count: count.clone(), limit }).collect()
    };
    slips(&wrap(a), &wrap(b), max)
}

/// The whole table, with no shortcut.
fn table(a: &[u32], b: &[u32]) -> usize {
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        for j in 0..=b.len() {
            d[i][j] = if i == 0 || j == 0 {
                i + j
            } else {
                let mut best = (d[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1])).min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
                if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                    best = best.min(d[i - 2][j - 2] + 1);
                }
                best
            };
        }
    }
    d[a.len()][b.len()]
}

#[test]
fn empty_sides() {
    check!("both empty", slips::<u8>(&[], &[], 0), Some(0));
    check!("one empty, enough", slips(&[], &[1, 2, 3], 3), Some(3));
    check!("one empty, not enough", slips(&[1, 2, 3], &[], 2), None);
}

#[test]
fn a_limit_of_nothing_means_equal() {
    check!("equal", slips(&[1, 2], &[1, 2], 0), Some(0));
    check!("swapped", slips(&[1, 2], &[2, 1], 0), None);
    check!("replaced", slips(&[1, 2], &[1, 3], 0), None);
}

#[test]
fn any_limit_is_accepted() {
    check!("usize::MAX", slips(&[1, 2, 3], &[3, 2, 1, 0], usize::MAX), Some(table(&[1, 2, 3], &[3, 2, 1, 0])));
    check!("half of it", slips(&[7], &[8], usize::MAX / 2 + 1), Some(1));
}

#[test]
fn long_codes_with_a_slip_near_each_end() {
    let a: Vec<u32> = (0..5000).collect();
    let mut b = a.clone();
    b.swap(1, 2);
    b.remove(4990);
    check!("two slips, two allowed", within_budget(&a, &b, 2), Some(2));
    check!("two slips, one allowed", within_budget(&a, &b, 1), None);
    check!("equal", within_budget(&a, &a, 0), Some(0));
}

#[test]
fn long_codes_that_share_nothing() {
    let a: Vec<u32> = (0..5000).collect();
    let b: Vec<u32> = (10_000..15_000).collect();
    check!("far apart", within_budget(&a, &b, 3), None);
    check!("lengths too different", within_budget(&a, &b[..10], 3), None);
}

#[test]
fn many_slips_in_long_codes() {
    let mut rng = anneal_prelude::Rng::new(15102);
    let a: Vec<u32> = (0..1500).map(|_| rng.below(1000) as u32).collect();
    let mut b = a.clone();
    for at in (10..1400).step_by(70) {
        b[at] += 1000;
    }
    check!("20 replaced items", within_budget(&a, &b, 25), Some(20));
    check!("20 replaced items, 19 allowed", within_budget(&a, &b, 19), None);
}

#[test]
fn agrees_with_the_whole_table() {
    let mut rng = anneal_prelude::Rng::new(15103);
    for _ in 0..6000 {
        let letters = 2 + rng.below(3);
        let n = rng.below(9);
        let a: Vec<u32> = (0..n).map(|_| rng.below(letters) as u32).collect();
        let b: Vec<u32> = if rng.bool() {
            let m = rng.below(9);
            (0..m).map(|_| rng.below(letters) as u32).collect()
        } else {
            let mut b = a.clone();
            for _ in 0..rng.below(4) {
                match rng.below(4) {
                    0 => b.insert(rng.below(b.len() + 1), rng.below(letters) as u32),
                    1 if !b.is_empty() => {
                        b.remove(rng.below(b.len()));
                    }
                    2 if !b.is_empty() => {
                        let at = rng.below(b.len());
                        b[at] = rng.below(letters) as u32;
                    }
                    3 if b.len() > 1 => {
                        let at = rng.below(b.len() - 1);
                        b.swap(at, at + 1);
                    }
                    _ => {}
                }
            }
            b
        };
        let max = rng.below(6);
        let real = table(&a, &b);
        let want = (real <= max).then_some(real);
        check!(format!("slips({a:?}, {b:?}, {max})"), within_budget(&a, &b, max), want);
    }
}

#[test]
fn slips_at_the_very_ends() {
    let a: Vec<u32> = (0..3000).collect();
    let mut b = vec![7_000];
    b.extend(&a);
    b.push(7_001);
    check!("one added at each end", within_budget(&a, &b, 2), Some(2));
    check!("the other way round", within_budget(&b, &a, 2), Some(2));
}
