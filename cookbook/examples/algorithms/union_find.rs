//! RECIPE 68 (graphs): union-find (disjoint sets): "are these two connected?" in near-constant time.
//! LEVEL: intermediate
//! Use in: connected components of UNDIRECTED relations, Kruskal's MST, grouping duplicates, incremental connectivity.   Run: cargo run --release --example union_find
//! COMMON USES: network connectivity, Kruskal MST, image segmentation, deduplicating equivalent records.
#![allow(dead_code)]
//!
//! Each set is a tree; `find` returns the root. Two optimisations make it effectively O(1) amortised
//! (inverse Ackermann): PATH COMPRESSION (flatten trees while finding) and UNION BY SIZE (attach the small tree under the big).
//! Note: this answers connectivity for UNDIRECTED edges and can only ADD edges; it cannot detect directed cycles.
//! Read: https://cp-algorithms.com/data_structures/disjoint_set_union.html
struct Dsu {
    parent: Vec<usize>,
    size: Vec<usize>,
    sets: usize,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Dsu {
            parent: (0..n).collect(),
            size: vec![1; n],
            sets: n,
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path halving
            x = self.parent[x];
        }
        x
    }
    /// returns true if a and b were in different sets (i.e. this edge MERGED two components)
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (mut ra, mut rb) = (self.find(a), self.find(b));
        if ra == rb {
            return false;
        }
        if self.size[ra] < self.size[rb] {
            std::mem::swap(&mut ra, &mut rb);
        }
        self.parent[rb] = ra;
        self.size[ra] += self.size[rb];
        self.sets -= 1;
        true
    }
    fn connected(&mut self, a: usize, b: usize) -> bool {
        self.find(a) == self.find(b)
    }
}

fn main() {
    // computers in a network, cables are undirected
    let names = ["a", "b", "c", "d", "e", "f"];
    let cables = [(0, 1), (1, 2), (3, 4), (2, 0)]; // last one is redundant (forms a loop)
    let mut dsu = Dsu::new(names.len());
    for (x, y) in cables {
        let merged = dsu.union(x, y);
        println!(
            "cable {}-{}: {}",
            names[x],
            names[y],
            if merged {
                "joins two groups"
            } else {
                "redundant (already connected)"
            }
        );
    }
    println!("groups: {}", dsu.sets);
    let root_f = dsu.find(5);
    let f_alone = dsu.size[root_f] == 1;
    println!(
        "a~c {} a~d {} d~e {} f alone {f_alone}",
        dsu.connected(0, 2),
        dsu.connected(0, 3),
        dsu.connected(3, 4)
    );
    assert_eq!(dsu.sets, 3);

    // Kruskal-style: minimum spanning tree = take cheapest edges that merge components
    let mut weighted = vec![(4, 0, 1), (1, 1, 2), (3, 0, 2), (2, 2, 3), (7, 1, 3)];
    weighted.sort();
    let mut dsu = Dsu::new(4);
    let mst: Vec<_> = weighted
        .into_iter()
        .filter(|&(_, a, b)| dsu.union(a, b))
        .collect();
    println!(
        "MST edges (w,a,b): {mst:?} total={}",
        mst.iter().map(|e| e.0).sum::<i32>()
    );
    assert_eq!(mst.iter().map(|e| e.0).sum::<i32>(), 6);

    let mut big = Dsu::new(1_000_000);
    for i in 1..1_000_000 {
        big.union(i - 1, i);
    }
    println!(
        "1M chain merged: {} set(s), find is fast thanks to compression",
        big.sets
    );
}
