//! RECIPE 136 (graphs): max-flow with Dinic's algorithm, min-cut extraction, bipartite matching and project selection.
//! LEVEL: advanced
//! Use in: assignment/matching, scheduling relaxations, image segmentation, "what to build vs what it needs" selection.   Run: cargo run --release --example max_flow_dinic
#![allow(dead_code)]
//!
//! MAX-FLOW: push as much as possible from source s to sink t without exceeding edge capacities. Each edge is stored
//! with a paired reverse edge (ids `e` and `e^1`) in one Vec; the reverse edge's capacity is the flow pushed so far,
//! so "undoing" flow is just another residual edge.
//! DINIC: repeat { BFS builds a LEVEL graph (distance from s in the residual graph); a DFS finds a BLOCKING flow that
//! only walks edges going exactly one level deeper } until t is unreachable. The `it[u]` CURRENT-ARC pointer remembers
//! how far u's edge list has been exhausted, so no dead edge is rescanned within a phase: O(V^2 E) worst case, and
//! O(E sqrt(V)) on unit-capacity bipartite graphs. The DFS here is iterative (explicit path stack), no recursion.
//! MIN-CUT: after max-flow, nodes still reachable from s in the residual graph form side S; original edges from S to
//! the rest are saturated and their capacities sum to the max-flow value (max-flow min-cut theorem).
//! REDUCTIONS: bipartite matching = flow with unit capacities (s->left, left->right, right->t). Project selection =
//! min cut: profitable projects hang off s, costly tools off t, "project needs tool" edges are infinite, so no cut can
//! take a project without paying for all its tools. Answer = sum of profits - min cut.
//! COMMON USES: job/worker/task assignment and roster scheduling, bin-packing / transport relaxations (LP bounds),
//! image segmentation by graph cuts (foreground/background as s/t sides), supply-chain and network capacity planning,
//! baseball-elimination style "can this team still win" questions, open-pit mining / dependency-closure problems.
//! Differential tests: Dinic vs a simple Edmonds-Karp on 2000 random networks; flow matching vs Kuhn's algorithm;
//! project selection vs brute force over all subsets.
//! Read: https://cp-algorithms.com/graph/dinic.html and https://cp-algorithms.com/graph/edmonds_karp.html
use std::collections::VecDeque;

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

#[derive(Clone)]
struct Edge {
    to: usize,
    cap: i64, // residual capacity
}

struct Dinic {
    edges: Vec<Edge>,     // edge e and its reverse e^1 sit next to each other
    adj: Vec<Vec<usize>>, // adj[u] = edge ids leaving u
    level: Vec<i32>,
    it: Vec<usize>, // current-arc pointer per node
}

impl Dinic {
    fn new(n: usize) -> Self {
        Dinic {
            edges: vec![],
            adj: vec![vec![]; n],
            level: vec![-1; n],
            it: vec![0; n],
        }
    }

    /// Returns the id of the forward edge; its flow later equals `edges[id ^ 1].cap`.
    fn add_edge(&mut self, u: usize, v: usize, cap: i64) -> usize {
        let id = self.edges.len();
        self.edges.push(Edge { to: v, cap });
        self.edges.push(Edge { to: u, cap: 0 });
        self.adj[u].push(id);
        self.adj[v].push(id + 1);
        id
    }

    fn bfs(&mut self, s: usize, t: usize) -> bool {
        self.level.fill(-1);
        self.level[s] = 0;
        let mut q = VecDeque::from([s]);
        while let Some(u) = q.pop_front() {
            for &e in &self.adj[u] {
                let v = self.edges[e].to;
                if self.edges[e].cap > 0 && self.level[v] < 0 {
                    self.level[v] = self.level[u] + 1;
                    q.push_back(v);
                }
            }
        }
        self.level[t] >= 0
    }

    /// Iterative blocking flow. `path` holds the edge ids from s to the current node `u`.
    fn blocking_flow(&mut self, s: usize, t: usize) -> i64 {
        let (mut total, mut path, mut u) = (0, Vec::<usize>::new(), s);
        loop {
            if u == t {
                let f = path.iter().map(|&e| self.edges[e].cap).min().unwrap();
                for &e in &path {
                    self.edges[e].cap -= f;
                    self.edges[e ^ 1].cap += f;
                }
                total += f;
                // retreat to the tail of the first saturated edge and continue from there
                let k = path.iter().position(|&e| self.edges[e].cap == 0).unwrap();
                path.truncate(k);
                u = if k == 0 {
                    s
                } else {
                    self.edges[path[k - 1]].to
                };
            } else if self.it[u] < self.adj[u].len() {
                let e = self.adj[u][self.it[u]];
                let v = self.edges[e].to;
                if self.edges[e].cap > 0 && self.level[v] == self.level[u] + 1 {
                    path.push(e);
                    u = v;
                } else {
                    self.it[u] += 1; // dead for this phase: never look at it again
                }
            } else if let Some(e) = path.pop() {
                u = self.edges[e ^ 1].to; // dead end: back up and skip the edge that led here
                self.it[u] += 1;
            } else {
                return total; // back at s with nothing left
            }
        }
    }

    fn max_flow(&mut self, s: usize, t: usize) -> i64 {
        let mut flow = 0;
        while self.bfs(s, t) {
            self.it.fill(0);
            flow += self.blocking_flow(s, t);
        }
        flow
    }

    /// Call after `max_flow`: which nodes can still be reached from s in the residual graph.
    fn source_side(&self, s: usize) -> Vec<bool> {
        let mut seen = vec![false; self.adj.len()];
        let mut stack = vec![s];
        seen[s] = true;
        while let Some(u) = stack.pop() {
            for &e in &self.adj[u] {
                let v = self.edges[e].to;
                if self.edges[e].cap > 0 && !seen[v] {
                    seen[v] = true;
                    stack.push(v);
                }
            }
        }
        seen
    }

    /// Forward edges (even ids) crossing from the source side to the sink side: (from, to, original capacity).
    fn min_cut(&self, s: usize) -> Vec<(usize, usize, i64)> {
        let side = self.source_side(s);
        (0..self.edges.len())
            .step_by(2)
            .filter_map(|e| {
                let (u, v) = (self.edges[e ^ 1].to, self.edges[e].to);
                (side[u] && !side[v]).then(|| (u, v, self.edges[e].cap + self.edges[e ^ 1].cap))
            })
            .collect()
    }
}

/// Oracle: Edmonds-Karp on a dense capacity matrix (shortest augmenting path each time). Intentionally independent.
fn edmonds_karp(n: usize, edges: &[(usize, usize, i64)], s: usize, t: usize) -> i64 {
    let mut cap = vec![vec![0i64; n]; n];
    for &(u, v, c) in edges {
        cap[u][v] += c;
    }
    let mut flow = 0;
    loop {
        let mut parent = vec![usize::MAX; n];
        parent[s] = s;
        let mut q = VecDeque::from([s]);
        while let Some(u) = q.pop_front() {
            for v in 0..n {
                if parent[v] == usize::MAX && cap[u][v] > 0 {
                    parent[v] = u;
                    q.push_back(v);
                }
            }
        }
        if parent[t] == usize::MAX {
            return flow;
        }
        let (mut f, mut v) = (i64::MAX, t);
        while v != s {
            f = f.min(cap[parent[v]][v]);
            v = parent[v];
        }
        v = t;
        while v != s {
            cap[parent[v]][v] -= f;
            cap[v][parent[v]] += f;
            v = parent[v];
        }
        flow += f;
    }
}

/// Maximum bipartite matching through max-flow. Returns the matched (left, right) pairs.
fn flow_matching(nl: usize, nr: usize, pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let (s, t) = (nl + nr, nl + nr + 1);
    let mut g = Dinic::new(nl + nr + 2);
    (0..nl).for_each(|l| {
        g.add_edge(s, l, 1);
    });
    (0..nr).for_each(|r| {
        g.add_edge(nl + r, t, 1);
    });
    let ids: Vec<usize> = pairs
        .iter()
        .map(|&(l, r)| g.add_edge(l, nl + r, 1))
        .collect();
    g.max_flow(s, t);
    // a middle edge carries flow iff its reverse edge gained capacity
    pairs
        .iter()
        .zip(ids)
        .filter(|&(_, id)| g.edges[id ^ 1].cap == 1)
        .map(|(&p, _)| p)
        .collect()
}

/// Reference: Kuhn's augmenting-path matching (recursive; fine at these sizes).
fn kuhn_matching_size(nl: usize, nr: usize, pairs: &[(usize, usize)]) -> usize {
    fn try_kuhn(l: usize, adj: &[Vec<usize>], seen: &mut [bool], owner: &mut [usize]) -> bool {
        for &r in &adj[l] {
            if !seen[r] {
                seen[r] = true;
                if owner[r] == usize::MAX || try_kuhn(owner[r], adj, seen, owner) {
                    owner[r] = l;
                    return true;
                }
            }
        }
        false
    }
    let mut adj = vec![vec![]; nl];
    for &(l, r) in pairs {
        adj[l].push(r);
    }
    let mut owner = vec![usize::MAX; nr];
    (0..nl)
        .filter(|&l| try_kuhn(l, &adj, &mut vec![false; nr], &mut owner))
        .count()
}

/// Project selection: profits[p] (can be negative = a mandatory-cost project), tool_cost[k], needs[p] = tools required.
/// Returns (best profit, chosen projects).
fn select_projects(profits: &[i64], tool_cost: &[i64], needs: &[Vec<usize>]) -> (i64, Vec<usize>) {
    const INF: i64 = i64::MAX / 4;
    let (np, nt) = (profits.len(), tool_cost.len());
    let (s, t) = (np + nt, np + nt + 1);
    let mut g = Dinic::new(np + nt + 2);
    let mut positive = 0;
    for (p, &pr) in profits.iter().enumerate() {
        if pr >= 0 {
            positive += pr;
            g.add_edge(s, p, pr);
        } else {
            g.add_edge(p, t, -pr); // a loss-making project is a cost on the sink side
        }
        for &k in &needs[p] {
            g.add_edge(p, np + k, INF); // infinite: choosing p forces paying for k
        }
    }
    for (k, &c) in tool_cost.iter().enumerate() {
        g.add_edge(np + k, t, c);
    }
    let cut = g.max_flow(s, t);
    let side = g.source_side(s);
    (positive - cut, (0..np).filter(|&p| side[p]).collect())
}

fn project_value(
    profits: &[i64],
    tool_cost: &[i64],
    needs: &[Vec<usize>],
    chosen: &[usize],
) -> i64 {
    let mut tools = vec![false; tool_cost.len()];
    for &p in chosen {
        needs[p].iter().for_each(|&k| tools[k] = true);
    }
    chosen.iter().map(|&p| profits[p]).sum::<i64>()
        - (0..tools.len())
            .filter(|&k| tools[k])
            .map(|k| tool_cost[k])
            .sum::<i64>()
}

fn main() {
    let mut rng = Rng(0x1234_ABCD_9876_5432);

    // Classic textbook network, answer 23, plus min-cut check.
    let classic = [
        (0, 1, 16),
        (0, 2, 13),
        (1, 2, 10),
        (2, 1, 4),
        (1, 3, 12),
        (3, 2, 9),
        (2, 4, 14),
        (4, 3, 7),
        (3, 5, 20),
        (4, 5, 4),
    ];
    let mut g = Dinic::new(6);
    classic.iter().for_each(|&(u, v, c)| {
        g.add_edge(u, v, c);
    });
    let f = g.max_flow(0, 5);
    let cut = g.min_cut(0);
    assert_eq!(f, 23);
    assert_eq!(cut.iter().map(|c| c.2).sum::<i64>(), f);
    println!(
        "classic network: max-flow {f}, min-cut edges {:?}",
        cut.iter().map(|c| (c.0, c.1)).collect::<Vec<_>>()
    );

    // Dinic == Edmonds-Karp, and min-cut value == flow, on 2000 random networks.
    for _ in 0..2000 {
        let n = 2 + rng.below(9) as usize;
        let m = rng.below(4 * n as u64) as usize;
        let edges: Vec<(usize, usize, i64)> = (0..m)
            .map(|_| {
                (
                    rng.below(n as u64) as usize,
                    rng.below(n as u64) as usize,
                    rng.below(20) as i64,
                )
            })
            .collect();
        let mut g = Dinic::new(n);
        edges.iter().for_each(|&(u, v, c)| {
            g.add_edge(u, v, c);
        });
        let flow = g.max_flow(0, n - 1);
        assert_eq!(flow, edmonds_karp(n, &edges, 0, n - 1), "network {edges:?}");
        assert_eq!(g.min_cut(0).iter().map(|c| c.2).sum::<i64>(), flow);
    }
    println!("Dinic == Edmonds-Karp (and cut == flow) on 2000 random networks");

    // Bipartite matching via flow == Kuhn.
    let mut total = 0;
    for _ in 0..300 {
        let (nl, nr) = (1 + rng.below(25) as usize, 1 + rng.below(25) as usize);
        let m = rng.below((nl * 2 + 1) as u64) as usize;
        let mut pairs: Vec<(usize, usize)> = (0..m)
            .map(|_| (rng.below(nl as u64) as usize, rng.below(nr as u64) as usize))
            .collect();
        pairs.sort();
        pairs.dedup();
        let matching = flow_matching(nl, nr, &pairs);
        assert_eq!(matching.len(), kuhn_matching_size(nl, nr, &pairs));
        // validity: each node used at most once and every pair is a real edge
        let (mut ls, mut rs) = (vec![false; nl], vec![false; nr]);
        for &(l, r) in &matching {
            assert!(!ls[l] && !rs[r] && pairs.contains(&(l, r)));
            ls[l] = true;
            rs[r] = true;
        }
        total += matching.len();
    }
    println!("flow matching == Kuhn on 300 random bipartite graphs ({total} pairs matched)");

    // Big unit-capacity bipartite graph: Dinic's best case (O(E sqrt V)).
    let n = 2000;
    let pairs: Vec<(usize, usize)> = (0..n * 5)
        .map(|_| (rng.below(n as u64) as usize, rng.below(n as u64) as usize))
        .collect();
    let big = flow_matching(n, n, &pairs).len();
    assert_eq!(big, kuhn_matching_size(n, n, &pairs));
    println!("2000x2000 bipartite graph with 10k edges: matching size {big}");

    // Project selection: hand example, then brute force over all project subsets.
    let profits = [100, 200, 50, -30];
    let tool_cost = [150, 60, 40];
    let needs = vec![vec![0], vec![0, 1], vec![2], vec![1, 2]];
    let (best, chosen) = select_projects(&profits, &tool_cost, &needs);
    assert_eq!(best, project_value(&profits, &tool_cost, &needs, &chosen));
    println!("project selection example: profit {best} choosing projects {chosen:?}");
    for _ in 0..300 {
        let (np, nt) = (1 + rng.below(8) as usize, 1 + rng.below(6) as usize);
        let profits: Vec<i64> = (0..np).map(|_| rng.below(120) as i64 - 20).collect();
        let tool_cost: Vec<i64> = (0..nt).map(|_| rng.below(100) as i64).collect();
        let needs: Vec<Vec<usize>> = (0..np)
            .map(|_| (0..nt).filter(|_| rng.below(3) == 0).collect())
            .collect();
        let (best, chosen) = select_projects(&profits, &tool_cost, &needs);
        let brute = (0..1u32 << np)
            .map(|mask| {
                project_value(
                    &profits,
                    &tool_cost,
                    &needs,
                    &(0..np).filter(|&p| mask >> p & 1 == 1).collect::<Vec<_>>(),
                )
            })
            .max()
            .unwrap();
        assert_eq!(best, brute);
        assert_eq!(project_value(&profits, &tool_cost, &needs, &chosen), best);
    }
    println!("project selection (min-cut) == brute force over all subsets on 300 instances");
    println!("OK");
}
