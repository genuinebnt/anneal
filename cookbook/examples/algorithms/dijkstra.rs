//! RECIPE 69 (graphs): Dijkstra's shortest paths with a binary heap (lazy deletion) and path reconstruction.
//! LEVEL: intermediate
//! Use in: weighted routing, cost-aware scheduling, anything "cheapest way to get from A to B" with non-negative weights.   Run: cargo run --release --example dijkstra
//! COMMON USES: routing, cheapest paths, cost-aware planning, latency-aware load balancing.
#![allow(dead_code)]
//!
//! Maintain `dist[]`; repeatedly take the closest unfinalised node from a min-heap and relax its edges.
//! "Lazy deletion": instead of decreasing a key in the heap, push a NEW (smaller) entry and ignore stale ones when popped.
//! O((V+E) log V). REQUIRES non-negative weights (use Bellman-Ford for negatives).  BFS (index_graph) is Dijkstra with all weights 1.
//! Read: https://cp-algorithms.com/graph/dijkstra_sparse.html and petgraph's `dijkstra`.
use std::cmp::Reverse;
use std::collections::BinaryHeap;

struct Shortest {
    dist: Vec<Option<u64>>,
    prev: Vec<Option<usize>>,
}

fn dijkstra(adj: &[Vec<(usize, u64)>], src: usize) -> Shortest {
    let n = adj.len();
    let mut dist: Vec<Option<u64>> = vec![None; n];
    let mut prev = vec![None; n];
    let mut heap = BinaryHeap::new();
    dist[src] = Some(0);
    heap.push(Reverse((0u64, src)));
    while let Some(Reverse((d, u))) = heap.pop() {
        if dist[u].is_some_and(|best| d > best) {
            continue; // stale entry: a shorter path to u was already found
        }
        for &(v, w) in &adj[u] {
            let nd = d + w;
            if dist[v].is_none_or(|cur| nd < cur) {
                dist[v] = Some(nd);
                prev[v] = Some(u);
                heap.push(Reverse((nd, v)));
            }
        }
    }
    Shortest { dist, prev }
}

fn path(prev: &[Option<usize>], mut to: usize) -> Vec<usize> {
    let mut p = vec![to];
    while let Some(u) = prev[to] {
        p.push(u);
        to = u;
    }
    p.reverse();
    p
}

fn main() {
    let names = ["home", "bakery", "park", "school", "work", "gym", "island"];
    let roads = [
        (0, 1, 7),
        (0, 2, 9),
        (0, 5, 14),
        (1, 2, 10),
        (1, 3, 15),
        (2, 3, 11),
        (2, 5, 2),
        (3, 4, 6),
        (5, 4, 9),
    ];
    let mut adj = vec![vec![]; names.len()];
    for (a, b, w) in roads {
        adj[a].push((b, w));
        adj[b].push((a, w)); // undirected roads
    }
    let s = dijkstra(&adj, 0);
    for (i, d) in s.dist.iter().enumerate() {
        match d {
            Some(d) => println!(
                "{:>7}: {d:>2} via {}",
                names[i],
                path(&s.prev, i)
                    .iter()
                    .map(|&x| names[x])
                    .collect::<Vec<_>>()
                    .join(" > ")
            ),
            None => println!("{:>7}: unreachable", names[i]),
        }
    }
    assert_eq!(s.dist[4], Some(20));
    assert_eq!(path(&s.prev, 4), vec![0, 2, 5, 4]);
    assert_eq!(s.dist[6], None);
}
