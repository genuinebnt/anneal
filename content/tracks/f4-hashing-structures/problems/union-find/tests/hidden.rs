use solution::*;

/// The obvious model: every element carries a group label.
#[derive(Clone)]
struct Model(Vec<usize>);

impl Model {
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (from, to) = (self.0[b], self.0[a]);
        if from == to {
            return false;
        }
        for label in &mut self.0 {
            if *label == from {
                *label = to;
            }
        }
        true
    }
    fn size(&self, a: usize) -> usize {
        self.0.iter().filter(|&&l| l == self.0[a]).count()
    }
    fn count(&self) -> usize {
        let mut labels = self.0.clone();
        labels.sort_unstable();
        labels.dedup();
        labels.len()
    }
    fn largest(&self) -> usize {
        (0..self.0.len()).map(|a| self.size(a)).max().unwrap_or(0)
    }
}

#[test]
fn core_one_machine() {
    let mut g = Groups::new(1);
    check!("(connected to itself, union with itself, count)", (g.connected(0, 0), g.union(0, 0), g.count()), (true, false, 1));
}

#[test]
fn core_two_trees_merge_by_inner_nodes() {
    let mut g = Groups::new(8);
    for (a, b) in [(0, 1), (2, 3), (1, 3), (4, 5), (6, 7), (5, 7)] {
        g.union(a, b);
    }
    check!("before", (g.count(), g.connected(0, 4)), (2, false));
    g.union(3, 7);
    check!("after", (g.count(), g.connected(0, 4), g.connected(2, 6)), (1, true, true));
}

#[test]
fn core_random_unions_match_labels() {
    let mut rng = anneal_prelude::Rng::new(5301);
    for _ in 0..300 {
        let n = 1 + rng.below(10);
        let mut g = Groups::new(n);
        let mut model = Model((0..n).collect());
        let mut trace = vec![format!("n = {n}")];
        for _ in 0..25 {
            let (a, b) = (rng.below(n), rng.below(n));
            if rng.bool() {
                trace.push(format!("union {a} {b}"));
                check!(format!("{trace:?}"), g.union(a, b), model.union(a, b));
            } else {
                check!(format!("{trace:?}: connected {a} {b}"), g.connected(a, b), model.0[a] == model.0[b]);
            }
            check!(format!("{trace:?}: count"), g.count(), model.count());
        }
    }
}

#[test]
fn core_scale_unfriendly_orders() {
    let n = 200_000;
    // Each union grows one group by one, from both argument orders.
    for flipped in [false, true] {
        let mut g = Groups::new(n);
        for i in 1..n {
            if flipped {
                g.union(i, 0);
            } else {
                g.union(0, i);
            }
        }
        let mut hits = 0;
        for i in 0..n {
            hits += usize::from(g.connected(i, (i * 7919) % n)) + usize::from(g.connected(0, i));
        }
        check!("(count, true answers)", (g.count(), hits), (1, 2 * n));
    }
    let mut g = Groups::new(n);
    for i in 1..n {
        g.union(i - 1, i);
    }
    let mut hits = 0;
    for i in 0..n {
        hits += usize::from(g.connected(0, i));
    }
    check!("a long chain", (g.count(), hits), (1, n));
}

#[test]
fn part2_size_from_any_member() {
    let mut g = Groups::new(6);
    for (a, b) in [(0, 1), (2, 3), (3, 4), (1, 4)] {
        g.union(a, b);
    }
    check!("sizes", (0..6).map(|i| g.group_size(i)).collect::<Vec<_>>(), vec![5, 5, 5, 5, 5, 1]);
}

#[test]
fn part2_no_machines() {
    check!("largest", Groups::new(0).largest(), 0);
}

#[test]
fn part2_redundant_union_keeps_sizes() {
    let mut g = Groups::new(3);
    g.union(0, 1);
    g.union(0, 1);
    g.union(1, 0);
    check!("(size, largest)", (g.group_size(0), g.largest()), (2, 2));
}

#[test]
fn part3_grow_from_nothing() {
    let mut g = Groups::new(0);
    check!("ids", (g.add(), g.add(), g.add()), (0, 1, 2));
    check!("(count, largest)", (g.count(), g.largest()), (3, 1));
}

#[test]
fn part4_nested_and_repeated() {
    let mut g = Groups::new(5);
    let start = g.checkpoint();
    g.union(0, 1);
    let mid = g.checkpoint();
    g.union(2, 3);
    g.union(0, 3);
    g.rollback(mid);
    check!("back to mid", (g.count(), g.largest()), (4, 2));
    g.union(3, 4);
    g.rollback(mid);
    check!("mid again", (g.count(), g.connected(3, 4)), (4, false));
    g.rollback(start);
    check!("back to the start", (g.count(), g.largest(), g.connected(0, 1)), (5, 1, false));
}

#[test]
fn part4_added_machines_disappear() {
    let mut g = Groups::new(1);
    let mark = g.checkpoint();
    let a = g.add();
    g.union(0, a);
    g.rollback(mark);
    check!("(count, size of 0, largest)", (g.count(), g.group_size(0), g.largest()), (1, 1, 1));
    check!("the id is free again", g.add(), 1);
}

#[test]
fn part4_random_scripts_match_saved_copies() {
    let mut rng = anneal_prelude::Rng::new(5302);
    for _ in 0..300 {
        let n = 1 + rng.below(5);
        let mut g = Groups::new(n);
        let mut model = Model((0..n).collect());
        let mut next_label = n;
        let mut saved: Vec<(Checkpoint, Model)> = Vec::new();
        let mut trace = vec![format!("n = {n}")];
        for _ in 0..40 {
            let len = model.0.len();
            match rng.below(6) {
                0 => {
                    trace.push("add".into());
                    check!(format!("{trace:?}"), g.add(), len);
                    model.0.push(next_label);
                    next_label += 1;
                }
                1 => {
                    trace.push(format!("checkpoint #{}", saved.len()));
                    saved.push((g.checkpoint(), model.clone()));
                }
                2 if !saved.is_empty() => {
                    let i = rng.below(saved.len());
                    trace.push(format!("rollback #{i}"));
                    g.rollback(saved[i].0);
                    model = saved[i].1.clone();
                    saved.truncate(i + 1);
                }
                _ => {
                    let (a, b) = (rng.below(len), rng.below(len));
                    trace.push(format!("union {a} {b}"));
                    check!(format!("{trace:?}"), g.union(a, b), model.union(a, b));
                }
            }
            check!(format!("{trace:?}: (count, largest)"), (g.count(), g.largest()), (model.count(), model.largest()));
            let len = model.0.len();
            let (a, b) = (rng.below(len), rng.below(len));
            check!(format!("{trace:?}: connected {a} {b}"), g.connected(a, b), model.0[a] == model.0[b]);
            check!(format!("{trace:?}: size of {a}"), g.group_size(a), model.size(a));
        }
    }
}

#[test]
fn part4_scale_many_checkpoints() {
    let n = 200_000;
    let mut g = Groups::new(n);
    for i in 1..n / 2 {
        g.union(0, i);
    }
    for i in 0..100_000 {
        let mark = g.checkpoint();
        g.union(n / 2 + i % (n / 2), (i * 31) % (n / 2));
        g.rollback(mark);
    }
    check!("(count, largest)", (g.count(), g.largest()), (n - n / 2 + 1, n / 2));
}
