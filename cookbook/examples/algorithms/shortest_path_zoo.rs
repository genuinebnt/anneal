//! RECIPE 134 (graphs): shortest-path zoo: Bellman-Ford, Floyd-Warshall, A*, bidirectional BFS, 0-1 BFS (and which to choose).
//! LEVEL: intermediate
//! Use in: routing, pathfinding in games/robotics, currency arbitrage, all-pairs tables, grid mazes.   Run: cargo run --release --example shortest_path_zoo
#![allow(dead_code)]
//!
//! No single algorithm wins; the right one depends on the weights and the question:
//!
//! | situation                              | choose                    | cost                    |
//! |----------------------------------------|---------------------------|-------------------------|
//! | unweighted, one source                 | BFS                       | O(V+E)                  |
//! | unweighted, one source->one target     | bidirectional BFS         | ~ 2*b^(d/2) vs b^d      |
//! | weights only 0 or 1                    | 0-1 BFS (deque)           | O(V+E)                  |
//! | non-negative weights                   | Dijkstra (see dijkstra)        | O((V+E) log V)          |
//! | negative weights / detect neg. cycles  | Bellman-Ford              | O(V*E)                  |
//! | all pairs, small dense graph           | Floyd-Warshall            | O(V^3)                  |
//! | one target + admissible heuristic      | A*                        | fewer expansions        |
//!
//! HOW: Bellman-Ford relaxes every edge V-1 times; if a V-th pass still improves something, a negative cycle is
//! reachable. Floyd-Warshall tries every node k as an intermediate and keeps a `next` matrix for path rebuilding.
//! A* is Dijkstra ordered by g+h; with a CONSISTENT heuristic (Manhattan on a 4-connected unit grid) the first time a
//! node is closed its g is optimal, so it returns the same cost as BFS while expanding fewer nodes.
//! Every algorithm is checked against a reference on seeded random graphs below (differential testing).
//! COMMON USES: Bellman-Ford underlies distance-vector routing (RIP) and arbitrage detection (log-rates, negative
//! cycle = profit loop); Floyd-Warshall for small transitive-closure / distance tables; A* in game and robot
//! navigation; 0-1 BFS for "free vs paid" moves (e.g. grid with doors, edit-distance-like graphs); bidirectional
//! search in social-graph "degrees of separation" queries.
//! Read: https://cp-algorithms.com/graph/bellman_ford.html , https://cp-algorithms.com/graph/all-pair-shortest-path-floyd-warshall.html
//! and https://cp-algorithms.com/graph/01_bfs.html
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

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

type Edge = (usize, usize, i64);

fn adjacency(n: usize, edges: &[Edge]) -> Vec<Vec<(usize, i64)>> {
    let mut adj = vec![vec![]; n];
    for &(u, v, w) in edges {
        adj[u].push((v, w));
    }
    adj
}

fn random_edges(rng: &mut Rng, n: usize, m: usize, max_w: u64) -> Vec<Edge> {
    (0..m)
        .map(|_| {
            (
                rng.below(n as u64) as usize,
                rng.below(n as u64) as usize,
                rng.below(max_w + 1) as i64,
            )
        })
        .collect()
}

// ---------- Bellman-Ford ----------

#[derive(Debug, PartialEq)]
struct NegativeCycle;

/// Distances from `src` (None = unreachable). Err if a negative cycle is reachable from `src`.
fn bellman_ford(n: usize, edges: &[Edge], src: usize) -> Result<Vec<Option<i64>>, NegativeCycle> {
    let mut dist: Vec<Option<i64>> = vec![None; n];
    dist[src] = Some(0);
    let relax_pass = |dist: &mut Vec<Option<i64>>| {
        let mut changed = false;
        for &(u, v, w) in edges {
            if let Some(du) = dist[u] {
                if dist[v].is_none_or(|dv| du + w < dv) {
                    dist[v] = Some(du + w);
                    changed = true;
                }
            }
        }
        changed
    };
    for _ in 0..n.saturating_sub(1) {
        if !relax_pass(&mut dist) {
            return Ok(dist); // early exit: converged
        }
    }
    // A shortest simple path has <= n-1 edges, so any further improvement proves a negative cycle.
    if relax_pass(&mut dist) {
        Err(NegativeCycle)
    } else {
        Ok(dist)
    }
}

// ---------- Floyd-Warshall ----------

const INF: i64 = i64::MAX / 4;

struct AllPairs {
    dist: Vec<Vec<i64>>,
    next: Vec<Vec<usize>>, // next[i][j] = first hop on a shortest i->j path
}

fn floyd_warshall(n: usize, edges: &[Edge]) -> AllPairs {
    let mut dist = vec![vec![INF; n]; n];
    let mut next = vec![vec![usize::MAX; n]; n];
    for i in 0..n {
        dist[i][i] = 0;
        next[i][i] = i;
    }
    for &(u, v, w) in edges {
        if w < dist[u][v] {
            dist[u][v] = w;
            next[u][v] = v;
        }
    }
    for k in 0..n {
        for i in 0..n {
            if dist[i][k] >= INF {
                continue;
            }
            for j in 0..n {
                if dist[k][j] < INF && dist[i][k] + dist[k][j] < dist[i][j] {
                    dist[i][j] = dist[i][k] + dist[k][j];
                    next[i][j] = next[i][k];
                }
            }
        }
    }
    AllPairs { dist, next }
}

impl AllPairs {
    fn path(&self, mut i: usize, j: usize) -> Option<Vec<usize>> {
        if self.next[i][j] == usize::MAX {
            return None;
        }
        let mut p = vec![i];
        while i != j {
            i = self.next[i][j];
            p.push(i);
        }
        Some(p)
    }
}

// ---------- Dijkstra (reference), 0-1 BFS, BFS, bidirectional BFS ----------

fn dijkstra(adj: &[Vec<(usize, i64)>], src: usize) -> Vec<Option<i64>> {
    let mut dist: Vec<Option<i64>> = vec![None; adj.len()];
    let mut heap = BinaryHeap::from([Reverse((0i64, src))]);
    dist[src] = Some(0);
    while let Some(Reverse((d, u))) = heap.pop() {
        if dist[u] != Some(d) {
            continue;
        }
        for &(v, w) in &adj[u] {
            if dist[v].is_none_or(|dv| d + w < dv) {
                dist[v] = Some(d + w);
                heap.push(Reverse((d + w, v)));
            }
        }
    }
    dist
}

/// Weights must be 0 or 1. A 0-edge goes to the FRONT of the deque, a 1-edge to the back, so the deque
/// stays sorted by distance (it only ever holds d and d+1) without a heap.
fn zero_one_bfs(adj: &[Vec<(usize, i64)>], src: usize) -> Vec<Option<i64>> {
    let mut dist: Vec<Option<i64>> = vec![None; adj.len()];
    let mut dq = VecDeque::from([src]);
    dist[src] = Some(0);
    while let Some(u) = dq.pop_front() {
        let du = dist[u].unwrap();
        for &(v, w) in &adj[u] {
            debug_assert!(w == 0 || w == 1);
            if dist[v].is_none_or(|dv| du + w < dv) {
                dist[v] = Some(du + w);
                if w == 0 {
                    dq.push_front(v)
                } else {
                    dq.push_back(v)
                }
            }
        }
    }
    dist
}

fn bfs(adj: &[Vec<usize>], src: usize) -> Vec<Option<usize>> {
    let mut dist = vec![None; adj.len()];
    let mut q = VecDeque::from([src]);
    dist[src] = Some(0);
    while let Some(u) = q.pop_front() {
        for &v in &adj[u] {
            if dist[v].is_none() {
                dist[v] = Some(dist[u].unwrap() + 1);
                q.push_back(v);
            }
        }
    }
    dist
}

/// Undirected unweighted s->t distance. Grows the smaller frontier one FULL level at a time; finishing the level
/// before answering matters, because the first meeting found is not necessarily the shortest.
fn bidirectional_bfs(adj: &[Vec<usize>], s: usize, t: usize) -> Option<usize> {
    if s == t {
        return Some(0);
    }
    let n = adj.len();
    let mut dist = [vec![usize::MAX; n], vec![usize::MAX; n]];
    let mut frontier = [vec![s], vec![t]];
    dist[0][s] = 0;
    dist[1][t] = 0;
    while !frontier[0].is_empty() && !frontier[1].is_empty() {
        let side = if frontier[0].len() <= frontier[1].len() {
            0
        } else {
            1
        };
        let mut best = usize::MAX;
        let mut next = vec![];
        for &u in &frontier[side] {
            for &v in &adj[u] {
                let other = dist[1 - side][v];
                if other != usize::MAX {
                    best = best.min(dist[side][u] + 1 + other); // the two searches touch
                }
                if dist[side][v] == usize::MAX {
                    dist[side][v] = dist[side][u] + 1;
                    next.push(v);
                }
            }
        }
        if best != usize::MAX {
            return Some(best);
        }
        frontier[side] = next;
    }
    None
}

// ---------- A* on a grid ----------

type Grid = Vec<Vec<bool>>; // true = wall

fn random_maze(rng: &mut Rng, h: usize, w: usize, wall_pct: u64) -> Grid {
    let mut g: Grid = (0..h)
        .map(|_| (0..w).map(|_| rng.below(100) < wall_pct).collect())
        .collect();
    g[0][0] = false;
    g[h - 1][w - 1] = false;
    g
}

fn neighbors4(g: &Grid, (r, c): (usize, usize)) -> impl Iterator<Item = (usize, usize)> + '_ {
    let (h, w) = (g.len() as i64, g[0].len() as i64);
    [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)]
        .into_iter()
        .filter_map(move |(dr, dc)| {
            let (nr, nc) = (r as i64 + dr, c as i64 + dc);
            (nr >= 0 && nc >= 0 && nr < h && nc < w && !g[nr as usize][nc as usize])
                .then_some((nr as usize, nc as usize))
        })
}

fn manhattan(a: (usize, usize), b: (usize, usize)) -> usize {
    a.0.abs_diff(b.0) + a.1.abs_diff(b.1)
}

/// Returns (cost, nodes expanded). Expansion counts a node when it is first popped as the best open node.
fn astar(g: &Grid, start: (usize, usize), goal: (usize, usize)) -> (Option<usize>, usize) {
    let mut best_g = vec![vec![usize::MAX; g[0].len()]; g.len()];
    let mut closed = vec![vec![false; g[0].len()]; g.len()];
    let mut heap = BinaryHeap::new();
    best_g[start.0][start.1] = 0;
    heap.push(Reverse((manhattan(start, goal), 0usize, start)));
    let mut expanded = 0;
    while let Some(Reverse((_, gc, p))) = heap.pop() {
        if closed[p.0][p.1] {
            continue; // stale heap entry
        }
        closed[p.0][p.1] = true;
        expanded += 1;
        if p == goal {
            return (Some(gc), expanded);
        }
        for q in neighbors4(g, p) {
            if gc + 1 < best_g[q.0][q.1] {
                best_g[q.0][q.1] = gc + 1;
                heap.push(Reverse((gc + 1 + manhattan(q, goal), gc + 1, q)));
            }
        }
    }
    (None, expanded)
}

/// Plain BFS with the same early stop, counting pops, so the expansion counts are comparable.
fn grid_bfs(g: &Grid, start: (usize, usize), goal: (usize, usize)) -> (Option<usize>, usize) {
    let mut dist = vec![vec![usize::MAX; g[0].len()]; g.len()];
    let mut q = VecDeque::from([start]);
    dist[start.0][start.1] = 0;
    let mut expanded = 0;
    while let Some(p) = q.pop_front() {
        expanded += 1;
        if p == goal {
            return (Some(dist[p.0][p.1]), expanded);
        }
        for nb in neighbors4(g, p) {
            if dist[nb.0][nb.1] == usize::MAX {
                dist[nb.0][nb.1] = dist[p.0][p.1] + 1;
                q.push_back(nb);
            }
        }
    }
    (None, expanded)
}

// ---------- tests ----------

fn undirected_adj(n: usize, edges: &[Edge]) -> Vec<Vec<usize>> {
    let mut adj = vec![vec![]; n];
    for &(u, v, _) in edges {
        adj[u].push(v);
        adj[v].push(u);
    }
    adj
}

fn main() {
    let mut rng = Rng(0xDEADBEEF12345);

    // Bellman-Ford / Floyd-Warshall / Dijkstra agree on non-negative graphs; FW paths are real and optimal.
    for _ in 0..300 {
        let n = 2 + rng.below(25) as usize;
        let edges = random_edges(&mut rng, n, n * 3, 20);
        let adj = adjacency(n, &edges);
        let fw = floyd_warshall(n, &edges);
        let mut cheapest = vec![vec![INF; n]; n]; // dense min edge weights to validate paths
        for &(u, v, w) in &edges {
            cheapest[u][v] = cheapest[u][v].min(w);
        }
        for src in 0..n {
            let d = dijkstra(&adj, src);
            assert_eq!(bellman_ford(n, &edges, src).unwrap(), d);
            for t in 0..n {
                assert_eq!(d[t].unwrap_or(INF), fw.dist[src][t]);
                match fw.path(src, t) {
                    Some(p) => {
                        let cost: i64 = p.windows(2).map(|e| cheapest[e[0]][e[1]]).sum();
                        assert_eq!((p[0], *p.last().unwrap(), cost), (src, t, fw.dist[src][t]));
                    }
                    None => assert!(d[t].is_none()),
                }
            }
        }
    }
    println!("BF == Dijkstra == Floyd-Warshall on 300 random non-negative graphs (paths verified)");

    // Negative edges WITHOUT negative cycles: reweight w' = w + p[u] - p[v]; every cycle keeps its original sum.
    let mut saw_negative = false;
    for _ in 0..300 {
        let n = 2 + rng.below(20) as usize;
        let pot: Vec<i64> = (0..n).map(|_| rng.below(30) as i64).collect();
        let base = random_edges(&mut rng, n, n * 3, 10);
        let edges: Vec<Edge> = base
            .iter()
            .map(|&(u, v, w)| (u, v, w + pot[u] - pot[v]))
            .collect();
        saw_negative |= edges.iter().any(|e| e.2 < 0);
        let adj0 = adjacency(n, &base);
        let fw = floyd_warshall(n, &edges);
        for src in 0..n {
            let (bf, d) = (bellman_ford(n, &edges, src).unwrap(), dijkstra(&adj0, src));
            for v in 0..n {
                assert_eq!(bf[v], d[v].map(|x| x + pot[src] - pot[v])); // path sum shifts by p[src]-p[v]
                assert_eq!(fw.dist[src][v] < INF, bf[v].is_some());
                if let Some(x) = bf[v] {
                    assert_eq!(fw.dist[src][v], x);
                }
            }
        }
    }
    assert!(saw_negative);
    println!("BF == FW on 300 graphs with negative edges (checked against reweighted Dijkstra)");

    // Negative cycle: detected when reachable, ignored when not.
    let ring = [(0, 1, 2), (1, 2, -5), (2, 0, 1), (2, 3, 1)];
    assert_eq!(bellman_ford(4, &ring, 0), Err(NegativeCycle));
    assert!(bellman_ford(4, &ring, 3).is_ok()); // node 3 cannot reach the cycle
    println!("negative cycle detected from 0, not from 3");

    // 0-1 BFS == Dijkstra
    for _ in 0..300 {
        let n = 2 + rng.below(40) as usize;
        let edges = random_edges(&mut rng, n, n * 3, 1);
        let adj = adjacency(n, &edges);
        for src in 0..n {
            assert_eq!(zero_one_bfs(&adj, src), dijkstra(&adj, src));
        }
    }
    println!("0-1 BFS == Dijkstra on 300 random 0/1 graphs");

    // Bidirectional BFS == BFS
    for _ in 0..300 {
        let n = 2 + rng.below(60) as usize;
        let m = n + rng.below(2 * n as u64) as usize;
        let edges = random_edges(&mut rng, n, m, 0);
        let adj = undirected_adj(n, &edges);
        for _ in 0..10 {
            let (s, t) = (rng.below(n as u64) as usize, rng.below(n as u64) as usize);
            assert_eq!(bidirectional_bfs(&adj, s, t), bfs(&adj, s)[t]);
        }
    }
    println!("bidirectional BFS == BFS on 300 random undirected graphs");

    // A* == BFS cost, with fewer expansions
    let (mut tot_astar, mut tot_bfs, mut solved) = (0, 0, 0);
    for _ in 0..200 {
        let g = random_maze(&mut rng, 40, 40, 28);
        let goal = (39, 39);
        let (ca, ea) = astar(&g, (0, 0), goal);
        let (cb, eb) = grid_bfs(&g, (0, 0), goal);
        assert_eq!(ca, cb);
        assert!(
            ea <= eb,
            "A* may never expand more than BFS here: {ea} vs {eb}"
        );
        tot_astar += ea;
        tot_bfs += eb;
        solved += ca.is_some() as usize;
    }
    assert!(solved > 50 && tot_astar < tot_bfs);
    println!(
        "A* == BFS cost on 200 mazes ({solved} solvable); expansions A* {tot_astar} vs BFS {tot_bfs}"
    );
    println!("OK");
}
