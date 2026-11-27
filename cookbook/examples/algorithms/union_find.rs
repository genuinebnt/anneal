//! RECIPE 68 (graphs): union-find (disjoint sets): keep items in groups while "these two belong together" facts arrive.
//! LEVEL: intermediate
//! Use in: grouping connected items, image regions, joining equal variables.   Run: cargo run --release --example union_find
//! COMMON USES: network connectivity, image segmentation, merging matched records, type inference.
#![allow(dead_code)]
//!
//! Each group is a tree stored in a vector of parent numbers; `find` walks to the root, which stands for the group.
//! UNION BY SIZE hangs the smaller tree under the larger root, so no path is longer than log2(n).
//! Path halving makes every other item on a walked path skip one step, so later walks are shorter.
//! It can join groups but cannot split them, and it does not remember why two items are together.
//! The checks compare it with a slow version that relabels items, on seeded random pairs, and measure tree depth.
//! Read: https://cp-algorithms.com/data_structures/disjoint_set_union.html

use std::collections::BTreeMap;

struct Groups {
    parent: Vec<usize>,
    size: Vec<usize>, // only meaningful at a root
    count: usize,
}

impl Groups {
    fn new(items: usize) -> Self {
        Groups {
            parent: (0..items).collect(),
            size: vec![1; items],
            count: items,
        }
    }

    /// The root of the item's group. Panics if `item` is out of range.
    fn find(&mut self, mut item: usize) -> usize {
        while self.parent[item] != item {
            // path halving: point at the grandparent, then move there
            self.parent[item] = self.parent[self.parent[item]];
            item = self.parent[item];
        }
        item
    }

    /// Joins the two groups. `false` means they were one group already.
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (mut big, mut small) = (self.find(a), self.find(b));
        if big == small {
            return false;
        }
        if self.size[big] < self.size[small] {
            std::mem::swap(&mut big, &mut small);
        }
        self.parent[small] = big;
        self.size[big] += self.size[small];
        self.count -= 1;
        true
    }

    fn connected(&mut self, a: usize, b: usize) -> bool {
        self.find(a) == self.find(b)
    }

    /// Every group, members in rising order, groups ordered by their first member.
    fn members(&mut self) -> Vec<Vec<usize>> {
        let mut by_root: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for item in 0..self.parent.len() {
            let root = self.find(item);
            by_root.entry(root).or_default().push(item);
        }
        let mut groups: Vec<Vec<usize>> = by_root.into_values().collect();
        groups.sort();
        groups
    }

    /// Steps from the item to its root, without changing the tree.
    fn depth(&self, mut item: usize) -> usize {
        let mut steps = 0;
        while self.parent[item] != item {
            item = self.parent[item];
            steps += 1;
        }
        steps
    }

    fn deepest(&self) -> (usize, usize) {
        (0..self.parent.len())
            .map(|i| (self.depth(i), i))
            .max()
            .unwrap_or((0, 0))
    }

    fn root_without_changes(&self, mut item: usize) -> usize {
        while self.parent[item] != item {
            item = self.parent[item];
        }
        item
    }
}

/// The slow, obvious version: every item carries a group label.
struct Labels(Vec<usize>);

impl Labels {
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (keep, replace) = (self.0[a], self.0[b]);
        if keep == replace {
            return false;
        }
        for label in &mut self.0 {
            if *label == replace {
                *label = keep;
            }
        }
        true
    }

    fn members(&self) -> Vec<Vec<usize>> {
        let mut by_label: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (item, &label) in self.0.iter().enumerate() {
            by_label.entry(label).or_default().push(item);
        }
        let mut groups: Vec<Vec<usize>> = by_label.into_values().collect();
        groups.sort();
        groups
    }
}

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn check_against_labels() {
    let mut rng = Rng(0x5EED);
    for _ in 0..500 {
        let n = 1 + rng.below(30);
        let mut fast = Groups::new(n);
        let mut slow = Labels((0..n).collect());
        for _ in 0..rng.below(60) {
            let (a, b) = (rng.below(n), rng.below(n));
            assert_eq!(fast.union(a, b), slow.union(a, b), "union({a}, {b})");
            let (c, d) = (rng.below(n), rng.below(n));
            assert_eq!(fast.connected(c, d), slow.0[c] == slow.0[d]);
            let groups = slow.members();
            assert_eq!(fast.count, groups.len());
            assert_eq!(fast.members(), groups);
        }
    }
}

/// One large group takes in single items. Both argument orders are tried, so a rule that
/// follows the argument order instead of the sizes builds a long chain in one of them.
fn check_newcomers_stay_near_the_root() {
    const N: usize = 1_000;
    for newcomer_first in [false, true] {
        let mut groups = Groups::new(N);
        for item in 1..N {
            let root = groups.root_without_changes(0);
            if newcomer_first {
                groups.union(item, root);
            } else {
                groups.union(root, item);
            }
        }
        let (depth, _) = groups.deepest();
        assert!(depth <= 1, "one big group took in singles, depth {depth}");
    }
}

/// Joining equal groups, round after round, is how union by size gets its tallest tree.
/// Roots are joined directly, so no walk shortens a path while the tree is built.
fn check_tallest_tree_and_halving() {
    const N: usize = 1_024;
    let mut groups = Groups::new(N);
    let mut width = 1;
    while width < N {
        for start in (0..N).step_by(width * 2) {
            let a = groups.root_without_changes(start);
            let b = groups.root_without_changes(start + width);
            groups.union(a, b);
        }
        width *= 2;
    }
    let (before, item) = groups.deepest();
    assert_eq!(before, 10, "the tallest tree for 1,024 items has depth 10");
    groups.find(item);
    let after = groups.depth(item);
    assert_eq!(after, 5, "one find should halve the walked path");
    println!("1,024 items: deepest item {before} steps from the root, {after} after one find");
}

fn main() {
    // library records, and pairs that a librarian marked as "same work"
    let records = [
        "Emma (1816)",
        "Emma, annotated",
        "Persuasion",
        "Emma audiobook",
        "Persuasion, large print",
        "Sanditon",
    ];
    let mut works = Groups::new(records.len());
    for (a, b) in [(0, 1), (2, 4), (3, 1), (0, 3)] {
        let news = if works.union(a, b) {
            "joins two groups"
        } else {
            "already known"
        };
        println!("{} = {}: {news}", records[a], records[b]);
    }
    println!("works: {}", works.count);
    for group in works.members() {
        let titles: Vec<_> = group.iter().map(|&i| records[i]).collect();
        println!("  {}", titles.join(" | "));
    }
    assert_eq!(works.members(), [vec![0, 1, 3], vec![2, 4], vec![5]]);

    check_against_labels();
    check_newcomers_stay_near_the_root();
    check_tallest_tree_and_halving();
    println!("all checks passed");
}
