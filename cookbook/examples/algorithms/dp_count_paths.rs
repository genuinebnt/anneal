//! RECIPE 73 (dp): counting routes through a DAG: top-down memoization vs bottom-up, on the same recurrence.
//! LEVEL: intermediate
//! Use in: any "combine results from predecessors" computation over a DAG (counts, sums, best-of); learn the SHAPE.   Run: cargo run --release --example dp_count_paths
//! COMMON USES: counting/aggregating over DAGs: build graphs, probabilistic inference, combinatorics.
#![allow(dead_code)]
//!
//! DAG DP pattern: the answer for a node depends only on the answers of its PREDECESSORS, so process nodes in an order
//! where predecessors come first. Two equivalent ways:
//!   * bottom-up: iterate nodes in that order, filling a table (no recursion, no stack risk)
//!   * top-down: recursive function + memo table `Vec<Option<T>>` (computes only what is asked, simple to write)
//! Here: number of distinct routes from node 0 to each node. The combine step is SUM; think about which operation and
//! which per-node weight you would use for a different question (longest, cheapest, earliest).
//! Domain: delivery hubs, nodes are numbered so every edge goes from a lower to a higher number (a valid order for free).
//! Read: https://cp-algorithms.com/dynamic_programming/intro-to-dp.html and https://en.wikipedia.org/wiki/Directed_acyclic_graph#Computational_problems
fn routes_bottom_up(n: usize, edges: &[(usize, usize)]) -> Vec<u64> {
    let mut out_edges = vec![vec![]; n];
    for &(a, b) in edges {
        out_edges[a].push(b);
    }
    let mut ways = vec![0u64; n];
    ways[0] = 1;
    for u in 0..n {
        // by now `ways[u]` is final because all its predecessors have smaller numbers
        for &v in &out_edges[u] {
            ways[v] = ways[v].checked_add(ways[u]).expect("route count overflow");
        }
    }
    ways
}

fn routes_top_down(v: usize, preds: &[Vec<usize>], memo: &mut Vec<Option<u64>>) -> u64 {
    if let Some(x) = memo[v] {
        return x;
    }
    let r = if v == 0 {
        1
    } else {
        preds[v]
            .iter()
            .map(|&p| routes_top_down(p, preds, memo))
            .sum()
    };
    memo[v] = Some(r);
    r
}

fn main() {
    // layers: 0 -> {1,2,3} -> {4,5,6} -> 7   (fully connected between layers)
    let mut edges = vec![];
    for a in 1..=3 {
        edges.push((0, a));
        for b in 4..=6 {
            edges.push((a, b));
        }
    }
    for b in 4..=6 {
        edges.push((b, 7));
    }
    let bottom = routes_bottom_up(8, &edges);
    println!("bottom-up: {bottom:?}");
    let mut preds = vec![vec![]; 8];
    for &(a, b) in &edges {
        preds[b].push(a);
    }
    let mut memo = vec![None; 8];
    let top = routes_top_down(7, &preds, &mut memo);
    println!(
        "top-down route count to node 7 = {top}; memo filled for {} nodes",
        memo.iter().flatten().count()
    );
    assert_eq!(bottom[7], 9);
    assert_eq!(top, 9);

    // scale: 20 layers of 3 nodes: 3^19 routes, instant because each node is computed once (naive recursion would be exponential)
    let layers = 20;
    let n = layers * 3 + 2;
    let mut e = vec![];
    for k in 0..3 {
        e.push((0, 1 + k));
    }
    for l in 0..layers - 1 {
        for a in 0..3 {
            for b in 0..3 {
                e.push((1 + l * 3 + a, 1 + (l + 1) * 3 + b));
            }
        }
    }
    for a in 0..3 {
        e.push((1 + (layers - 1) * 3 + a, n - 1));
    }
    println!("{layers} layers: {} routes", routes_bottom_up(n, &e)[n - 1]);
    assert_eq!(routes_bottom_up(n, &e)[n - 1], 3u64.pow(layers as u32));
}
