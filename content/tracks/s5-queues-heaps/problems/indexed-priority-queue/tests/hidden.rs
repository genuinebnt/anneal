use solution::*;

use std::cell::Cell;
use std::cmp::Ordering;
use std::rc::Rc;

#[test]
fn core_pop_then_push_again() {
    let mut q = UrgencyQueue::new();
    q.push(1u8, 1u8);
    check!("pop", q.pop(), Some((1, 1)));
    check!("push the popped key", (q.push(1, 9), q.priority(&1).copied()), (true, Some(9)));
}

#[test]
fn core_keys_without_order() {
    #[derive(Clone, PartialEq, Eq, Hash, Debug)]
    struct Id(u8);
    let mut q = UrgencyQueue::new();
    q.push(Id(1), 2);
    q.push(Id(2), 1);
    check!("pop", q.pop(), Some((Id(2), 1)));
}

#[test]
fn core_random_push_pop_distinct_priorities() {
    let mut rng = anneal_prelude::Rng::new(5201);
    for _ in 0..300 {
        let mut priorities: Vec<u32> = (0..40).collect();
        rng.shuffle(&mut priorities);
        let mut q = UrgencyQueue::new();
        let mut model: Vec<(u32, usize)> = Vec::new();
        for (key, &p) in priorities.iter().enumerate() {
            q.push(key, p);
            model.push((p, key));
            if rng.below(3) == 0 {
                model.sort_unstable_by(|a, b| b.cmp(a));
                check!(format!("priorities {priorities:?}, after pushing key {key}"), q.pop(), model.pop().map(|(p, k)| (k, p)));
            }
        }
        model.sort_unstable();
        let got: Vec<(usize, u32)> = std::iter::from_fn(|| q.pop()).collect();
        check!(format!("priorities {priorities:?}"), got, model.iter().map(|&(p, k)| (k, p)).collect::<Vec<_>>());
    }
}

#[test]
fn part2_missing_key_adds_nothing() {
    let mut q: UrgencyQueue<u8, u8> = UrgencyQueue::new();
    check!("change a missing key", (q.change_priority(&3, 1), q.len(), q.pop()), (None, 0, None));
}

#[test]
fn part2_same_priority_again() {
    let mut q = UrgencyQueue::new();
    q.push('a', 2);
    q.push('b', 3);
    check!("no-op change", q.change_priority(&'b', 3), Some(3));
    check!("pops", (q.pop(), q.pop()), (Some(('a', 2)), Some(('b', 3))));
}

/// A priority that counts how many values of it are alive.
struct Weighed(u64, Rc<Cell<i64>>);
impl Weighed {
    fn new(v: u64, alive: &Rc<Cell<i64>>) -> Self {
        alive.set(alive.get() + 1);
        Weighed(v, alive.clone())
    }
}
impl Clone for Weighed {
    fn clone(&self) -> Self {
        Weighed::new(self.0, &self.1)
    }
}
impl Drop for Weighed {
    fn drop(&mut self) {
        self.1.set(self.1.get() - 1);
    }
}
impl PartialEq for Weighed {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for Weighed {}
impl PartialOrd for Weighed {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Weighed {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

#[test]
fn part2_memory_follows_live_keys() {
    let alive = Rc::new(Cell::new(0));
    let mut q = UrgencyQueue::new();
    for k in 0..100u64 {
        q.push(k, Weighed::new(k, &alive));
    }
    for i in 0..50_000u64 {
        drop(q.change_priority(&(i % 100), Weighed::new((i * 7919) % 1000, &alive)));
    }
    check!(format!("{} priority values alive for 100 waiting keys", alive.get()), alive.get() <= 300, true);
    drop(q);
    check!("alive after dropping the queue", alive.get(), 0);
}

#[test]
fn part3_remove_each_position() {
    for victim in 0..15u32 {
        let mut q = UrgencyQueue::new();
        // A heap shape in which the last entry must sometimes move up after replacing the removed one.
        let priorities: [u32; 15] = [1, 20, 2, 21, 22, 3, 4, 23, 24, 25, 26, 5, 6, 7, 8];
        for (k, &p) in priorities.iter().enumerate() {
            q.push(k as u32, p);
        }
        check!(format!("remove key {victim}"), q.remove(&victim), Some(priorities[victim as usize]));
        let got: Vec<u32> = std::iter::from_fn(|| q.pop()).map(|e| e.1).collect();
        let mut want: Vec<u32> = priorities.to_vec();
        want.remove(victim as usize);
        want.sort_unstable();
        check!(format!("pops after removing key {victim}"), got, want);
    }
}

#[test]
fn part3_removed_key_is_gone_and_reusable() {
    let mut q = UrgencyQueue::new();
    q.push("a", 1);
    q.remove(&"a");
    check!("(len, priority, change)", (q.len(), q.priority(&"a").copied(), q.change_priority(&"a", 2)), (0, None, None));
    check!("push again", (q.push("a", 3), q.pop()), (true, Some(("a", 3))));
}

#[test]
fn part4_change_keeps_arrival_order() {
    let mut q = UrgencyQueue::new();
    q.push("first", 9);
    q.push("second", 5);
    q.change_priority(&"first", 5);
    check!("first was pushed first", (q.pop(), q.pop()), (Some(("first", 5)), Some(("second", 5))));
}

#[test]
fn part4_repush_goes_to_the_back() {
    let mut q = UrgencyQueue::new();
    q.push(1, 0);
    q.push(2, 0);
    q.remove(&1);
    q.push(1, 0);
    check!("pops", (q.pop(), q.pop()), (Some((2, 0)), Some((1, 0))));
}

#[test]
fn part4_random_operations_match_a_list() {
    let mut rng = anneal_prelude::Rng::new(5202);
    for _ in 0..500 {
        let mut q = UrgencyQueue::new();
        // (key, priority) in push order: the first smallest priority is served.
        let mut model: Vec<(u8, u8)> = Vec::new();
        let mut trace = Vec::new();
        for _ in 0..60 {
            let key = rng.below(7) as u8;
            let p = rng.below(4) as u8;
            let found = model.iter().position(|e| e.0 == key);
            match rng.below(5) {
                0 => {
                    trace.push("pop".to_string());
                    let want = model.iter().map(|e| e.1).min().map(|m| {
                        let i = model.iter().position(|e| e.1 == m).unwrap();
                        model.remove(i)
                    });
                    check!(format!("{trace:?}"), q.pop(), want);
                }
                1 => {
                    trace.push(format!("remove {key}"));
                    let want = found.map(|i| model.remove(i).1);
                    check!(format!("{trace:?}"), q.remove(&key), want);
                }
                2 => {
                    trace.push(format!("change {key} {p}"));
                    let want = found.map(|i| std::mem::replace(&mut model[i].1, p));
                    check!(format!("{trace:?}"), q.change_priority(&key, p), want);
                }
                _ => {
                    trace.push(format!("push {key} {p}"));
                    if found.is_none() {
                        model.push((key, p));
                    }
                    check!(format!("{trace:?}"), q.push(key, p), found.is_none());
                }
            }
            check!(format!("{trace:?}: len"), q.len(), model.len());
            check!(format!("{trace:?}: priority of {key}"), q.priority(&key).copied(), model.iter().find(|e| e.0 == key).map(|e| e.1));
        }
    }
}

#[test]
fn part4_scale_updates_are_logarithmic() {
    let n = 100_000u64;
    let mut q = UrgencyQueue::new();
    for k in 0..n {
        q.push(k, n + k);
    }
    for i in 0..2 * n {
        let k = (i * 7919) % n;
        q.change_priority(&k, (i * 104_729) % (3 * n));
    }
    for k in (0..n).step_by(2) {
        q.remove(&k);
    }
    for k in 0..n / 2 {
        q.change_priority(&(2 * k + 1), k);
    }
    let mut popped = Vec::new();
    while let Some((k, p)) = q.pop() {
        popped.push((p, k));
    }
    check!("popped count", popped.len(), 50_000);
    check!("pops are in priority order", popped.is_sorted_by_key(|e| e.0), true);
    check!("first three", popped[..3].to_vec(), vec![(0, 1), (1, 3), (2, 5)]);
}
