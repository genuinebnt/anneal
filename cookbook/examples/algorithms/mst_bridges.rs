//! RECIPE 135 (graphs): minimum spanning tree (Prim, Kruskal) and bridges / articulation points via iterative low-link DFS.
//! LEVEL: advanced
//! Use in: network design and reliability, single-linkage clustering, finding single points of failure.   Run: cargo run --release --example mst_bridges
#![allow(dead_code)]
//!
//! MST: the cheapest set of edges connecting every node. Prim grows one tree from a start node, always taking the
//! cheapest edge leaving it (binary heap, lazy deletion). Kruskal sorts all edges and adds each one that joins two
//! different components (union-find). Different tie-breaks may give different trees but ALWAYS the same total weight,
//! which is what the test asserts.
//! BRIDGES: an edge whose removal disconnects its component. ARTICULATION POINTS: the same for vertices.
//! One DFS records `tin[v]` (discovery time) and `low[v]` (earliest tin reachable from v's subtree using at most one
//! back edge). Tree edge v->c is a bridge iff `low[c] > tin[v]`; v is an articulation point iff some child has
//! `low[c] >= tin[v]` (v non-root), or v is a root with 2+ DFS children. The DFS is ITERATIVE (explicit stack):
//! a 200k-node path would overflow the call stack recursively. The parent is skipped by EDGE ID, not by vertex, so
//! parallel edges are handled (two parallel edges are not bridges).
//! 2-edge-connected components = what remains after deleting all bridges.
//! A brute-force oracle (delete each edge/vertex, recount components) cross-checks everything on small random graphs.
//! COMMON USES: network reliability (which links/routers are single points of failure), cable / pipe / circuit
//! layout (MST = least wiring), single-linkage clustering (build the MST, cut the k-1 heaviest edges), approximate
//! TSP (2x MST bound), road-network resilience analysis, finding critical dependencies in service graphs.
//! Read: https://cp-algorithms.com/graph/mst_kruskal.html , https://cp-algorithms.com/graph/mst_prim.html
//! , https://cp-algorithms.com/graph/bridge-searching.html and https://cp-algorithms.com/graph/cutpoints.html
use std::cmp::Reverse;
use std::collections::BinaryHeap;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

type Edge = (usize, usize, u64);

struct Dsu {
    parent: Vec<usize>,
    size: Vec<usize>,
}
impl Dsu {
    fn new(n: usize) -> Self {
        Dsu {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path halving
            x = self.parent[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return false;
        }
        if self.size[a] < self.size[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.parent[b] = a;
        self.size[a] += self.size[b];
        true
    }
}

/// Returns the chosen edges, in the order added.
fn kruskal(n: usize, edges: &[Edge]) -> Vec<Edge> {
    let mut sorted = edges.to_vec();
    sorted.sort_by_key(|e| e.2);
    let mut dsu = Dsu::new(n);
    sorted
        .into_iter()
        .filter(|&(u, v, _)| dsu.union(u, v))
        .collect()
}

/// Single-linkage clustering: stop Kruskal early at `k` components.
fn kruskal_clusters(n: usize, edges: &[Edge], k: usize) -> Vec<usize> {
    let mut sorted = edges.to_vec();
    sorted.sort_by_key(|e| e.2);
    let (mut dsu, mut comps) = (Dsu::new(n), n);
    for (u, v, _) in sorted {
        if comps == k {
            break;
        }
        comps -= dsu.union(u, v) as usize;
    }
    (0..n).map(|v| dsu.find(v)).collect()
}

/// Prim from node 0; the graph must be connected (returns None otherwise).
fn prim(n: usize, edges: &[Edge]) -> Option<Vec<Edge>> {
    let adj = adjacency(n, edges);
    let mut in_tree = vec![false; n];
    let mut heap = BinaryHeap::new();
    let mut chosen = vec![];
    in_tree[0] = true;
    for &(v, w, _) in &adj[0] {
        heap.push(Reverse((w, 0, v)));
    }
    while let Some(Reverse((w, u, v))) = heap.pop() {
        if in_tree[v] {
            continue; // lazy deletion: this edge now leads inside the tree
        }
        in_tree[v] = true;
        chosen.push((u, v, w));
        for &(x, wx, _) in &adj[v] {
            if !in_tree[x] {
                heap.push(Reverse((wx, v, x)));
            }
        }
    }
    (chosen.len() + 1 == n).then_some(chosen)
}

/// adj[u] = (neighbour, weight, edge id). Undirected: each edge is stored twice with the same id.
fn adjacency(n: usize, edges: &[Edge]) -> Vec<Vec<(usize, u64, usize)>> {
    let mut adj = vec![vec![]; n];
    for (id, &(u, v, w)) in edges.iter().enumerate() {
        adj[u].push((v, w, id));
        adj[v].push((u, w, id));
    }
    adj
}

struct LowLink {
    bridges: Vec<usize>, // edge ids
    articulation: Vec<bool>,
}

fn low_link(n: usize, edges: &[Edge]) -> LowLink {
    const UNSEEN: usize = usize::MAX;
    let adj = adjacency(n, edges);
    let (mut tin, mut low) = (vec![UNSEEN; n], vec![0; n]);
    let (mut bridges, mut articulation) = (vec![], vec![false; n]);
    let mut timer = 0;
    for root in 0..n {
        if tin[root] != UNSEEN {
            continue;
        }
        let mut root_children = 0;
        // frame = (vertex, edge id used to enter it, next adjacency index)
        let mut stack = vec![(root, usize::MAX, 0usize)];
        tin[root] = timer;
        low[root] = timer;
        timer += 1;
        while let Some(&(v, parent_edge, i)) = stack.last() {
            if i < adj[v].len() {
                stack.last_mut().unwrap().2 += 1;
                let (to, _, id) = adj[v][i];
                if id == parent_edge {
                    continue;
                }
                if tin[to] == UNSEEN {
                    tin[to] = timer;
                    low[to] = timer;
                    timer += 1;
                    stack.push((to, id, 0)); // "recurse"
                } else {
                    low[v] = low[v].min(tin[to]); // back edge
                }
            } else {
                stack.pop(); // "return" from v to its parent p
                if let Some(&(p, _, _)) = stack.last() {
                    low[p] = low[p].min(low[v]);
                    if low[v] > tin[p] {
                        bridges.push(parent_edge);
                    }
                    if p == root {
                        root_children += 1;
                    } else if low[v] >= tin[p] {
                        articulation[p] = true;
                    }
                }
            }
        }
        articulation[root] = root_children >= 2;
    }
    bridges.sort();
    LowLink {
        bridges,
        articulation,
    }
}

/// Label vertices by 2-edge-connected component: union everything except across bridges.
fn two_edge_components(n: usize, edges: &[Edge], bridges: &[usize]) -> Vec<usize> {
    let mut dsu = Dsu::new(n);
    for (id, &(u, v, _)) in edges.iter().enumerate() {
        if bridges.binary_search(&id).is_err() {
            dsu.union(u, v);
        }
    }
    canonical((0..n).map(|v| dsu.find(v)).collect())
}

/// Relabel by first occurrence so two partitions compare equal regardless of representative choice.
fn canonical(labels: Vec<usize>) -> Vec<usize> {
    let mut map = std::collections::HashMap::new();
    labels
        .iter()
        .map(|&l| {
            let k = map.len();
            *map.entry(l).or_insert(k)
        })
        .collect()
}

/// Oracle: components among vertices not `skip_v`, using edges not `skip_e`.
fn count_components(
    n: usize,
    edges: &[Edge],
    skip_e: Option<usize>,
    skip_v: Option<usize>,
) -> usize {
    let mut dsu = Dsu::new(n);
    let mut comps = n - skip_v.is_some() as usize;
    for (id, &(u, v, _)) in edges.iter().enumerate() {
        if Some(id) != skip_e && Some(u) != skip_v && Some(v) != skip_v {
            comps -= dsu.union(u, v) as usize;
        }
    }
    comps
}

fn random_connected(rng: &mut Rng, n: usize, extra: usize) -> Vec<Edge> {
    let mut edges: Vec<Edge> = (1..n)
        .map(|v| (rng.below(v as u64) as usize, v, 1 + rng.below(50)))
        .collect();
    for _ in 0..extra {
        let (u, v) = (rng.below(n as u64) as usize, rng.below(n as u64) as usize);
        if u != v {
            edges.push((u, v, 1 + rng.below(50)));
        }
    }
    edges
}

fn main() {
    let mut rng = Rng(0xC0FFEE_1234_5678);

    // MST: Prim and Kruskal agree on total weight; the result is a tree.
    for _ in 0..500 {
        let n = 2 + rng.below(60) as usize;
        let edges = random_connected(&mut rng, n, 3 * n);
        let (k, p) = (kruskal(n, &edges), prim(n, &edges).expect("connected"));
        assert_eq!((k.len(), p.len()), (n - 1, n - 1));
        let total = |t: &[Edge]| t.iter().map(|e| e.2).sum::<u64>();
        assert_eq!(total(&k), total(&p));
    }
    println!("Prim == Kruskal total weight on 500 random connected graphs");

    // Clustering demo: two tight groups joined by one long edge split cleanly at k = 2.
    let demo = [(0, 1, 1), (1, 2, 2), (3, 4, 1), (4, 5, 2), (2, 3, 100)];
    let labels = canonical(kruskal_clusters(6, &demo, 2));
    assert_eq!(labels, vec![0, 0, 0, 1, 1, 1]);

    // Bridges / articulation points / 2ECC vs the brute-force oracle on small graphs (parallel edges and self-loops allowed).
    let (mut bridge_total, mut cut_total) = (0, 0);
    for _ in 0..3000 {
        let n = 1 + rng.below(12) as usize;
        let m = rng.below(2 * n as u64 + 3) as usize;
        let edges: Vec<Edge> = (0..m)
            .map(|_| {
                (
                    rng.below(n as u64) as usize,
                    rng.below(n as u64) as usize,
                    1,
                )
            })
            .collect();
        let ll = low_link(n, &edges);
        let base = count_components(n, &edges, None, None);
        let want_bridges: Vec<usize> = (0..m)
            .filter(|&e| count_components(n, &edges, Some(e), None) > base)
            .collect();
        assert_eq!(ll.bridges, want_bridges, "bridges differ on {edges:?}");
        for v in 0..n {
            let is_cut = count_components(n, &edges, None, Some(v)) > base;
            assert_eq!(
                ll.articulation[v], is_cut,
                "articulation {v} differs on {edges:?}"
            );
            cut_total += is_cut as usize;
        }
        // 2ECC oracle: delete the brute-force bridges, label by connectivity with a fresh DSU
        let mut dsu = Dsu::new(n);
        for (id, &(u, v, _)) in edges.iter().enumerate() {
            if !want_bridges.contains(&id) {
                dsu.union(u, v);
            }
        }
        let want = canonical((0..n).map(|v| dsu.find(v)).collect());
        assert_eq!(two_edge_components(n, &edges, &ll.bridges), want);
        bridge_total += want_bridges.len();
    }
    println!(
        "low-link == brute force on 3000 small graphs ({bridge_total} bridges, {cut_total} articulation points seen)"
    );

    // Deep graph: 200k-node path. Recursion would overflow; the iterative DFS does not.
    let n = 200_000;
    let path: Vec<Edge> = (1..n).map(|v| (v - 1, v, 1)).collect();
    let ll = low_link(n, &path);
    assert_eq!(ll.bridges.len(), n - 1);
    assert_eq!(ll.articulation.iter().filter(|&&a| a).count(), n - 2);
    let comps = two_edge_components(n, &path, &ll.bridges);
    assert_eq!(*comps.iter().max().unwrap() + 1, n); // every vertex alone
    // close the path into a cycle: no bridges, no articulation points
    let mut ring = path.clone();
    ring.push((n - 1, 0, 1));
    let ll = low_link(n, &ring);
    assert!(ll.bridges.is_empty() && ll.articulation.iter().all(|&a| !a));
    println!(
        "200k path: {} bridges, {} cut vertices; closed into a ring: none",
        n - 1,
        n - 2
    );
    println!("OK");
}
