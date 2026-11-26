//! RECIPE 751 (database): join enumeration: DPsize, DPsub, DPccp, DPhyp, top-down partitioning, and GOO/IDP/linearised heuristics for huge queries
//! LEVEL: advanced
//! Use in: query optimizers choosing a join order, plan-space explorers, benchmarks of join-ordering algorithms.   Run: cargo run --release --example join_enumeration_dpccp_dphyp
//! COMMON USES: PostgreSQL (System-R style bottom-up DP, switches to the genetic GEQO search for big joins), DuckDB (DPhyp-style
//! enumeration over a hypergraph with a greedy fallback for large queries), HyPer/Umbra (DPhyp plus the adaptive linearised
//! search of Neumann et al. for hundreds of relations), MySQL 8 (greedy search with depth bound), Orca/Calcite (join-order
//! rules inside a memo), and any ORM-free SQL generator that must keep a 30-table join tractable.
//! Course: CMU 15-799 Query Optimization (Spring 2025), lectures 07-08 (join ordering: bottom-up and top-down)
#![allow(dead_code)]
//!
//! WHAT: a join QUERY GRAPH has one node per relation and one edge per join predicate (with a selectivity). A plan is a bushy
//! binary tree; with the classic C_out cost (the sum of the cardinalities of all intermediate results) the best tree can be
//! found by dynamic programming over CONNECTED SUBSETS of relations, never forming a cross product. The algorithms differ only
//! in HOW they walk the subsets, which decides how many useless candidates they test before reaching the same optimum.
//!
//! * DPsize  : by plan size s, combine every plan of size s1 with every plan of size s-s1; test disjointness + connectedness.
//!             Wastes work on overlapping or unconnected pairs (chains and stars especially).
//! * DPsub   : for every subset S (numeric order) try every split S = S1 + S2. Wastes work on unconnected splits.
//! * DPccp   : Moerkotte/Neumann. Enumerate each connected-subgraph / connected-complement PAIR exactly once with
//!             EnumerateCsg / EnumerateCmp. The number of calls equals the number of pairs, which has closed forms:
//!             chain (n^3-n)/6, star (n-1)*2^(n-2), clique (3^n - 2^(n+1) + 1)/2. Nothing is wasted, but it needs a simple graph.
//! * DPhyp   : the same neighbourhood idea over a HYPERgraph. A hyperedge (u, v) joins two SETS of relations and models complex
//!             predicates (`R0.a + R1.b = R2.c`) or reordering constraints (an outer join may not move past a relation, so its
//!             hyperedge's endpoints contain all of that relation's inputs). Representatives (the minimal element of each
//!             non-subsumed hypernode) keep the enumeration duplicate-free.
//! * TOP-DOWN: memoised recursion on the set S asking a PARTITIONING function for all (S1, S2) splits. `naive` generates all
//!             2^(|S|-1) subsets and tests both halves for connectedness (the generate-and-test of DeHaan/Tompa); `branch` grows a
//!             connected side C branch by branch with an exclusion set, so every connected C is produced exactly once and only the
//!             complement is tested (a simplified cousin of Fender/Moerkotte's MinCutBranch, which also avoids testing the
//!             complement). A top-down search can add branch-and-bound pruning, which bottom-up DP cannot.
//! * HEURISTICS for 50-200 relations (exact DP is exponential): GOO (greedy: always join the two linked clusters with the smallest
//!   result), IDP (iterative DP: run exact DP over at most k clusters, collapse the best sub-plan into one cluster, repeat) and the
//!   ADAPTIVE LINEARISED search (order the relations by a greedy linear order, then solve the interval DP optimally in O(n^3) so
//!   each subtree is a contiguous run of that order; Neumann et al. choose per query between exact, linearised and IDP by
//!   estimated search-space size, mirrored by `adaptive`).
//!
//! WHY: the optimizer's own runtime is part of query latency. DPccp/DPhyp touch only useful pairs; chains and cycles are cheap
//! even at 20+ relations, cliques explode at ~15, and beyond that a heuristic trades plan quality for time (see the table).
//!
//! CHECKS (asserts): on 2000 random queries (n <= 10, chain/cycle/star/clique/random) DPsize, DPsub, DPccp, DPhyp and both
//! top-down variants return the SAME optimal cost and the same number of csg-cmp pairs; for n <= 6 also equal to memo-less brute
//! force; DPccp's pair counts match the closed forms; DPhyp equals DPsub and brute force on random HYPERgraphs; every heuristic
//! never beats the optimum, IDP with k = n equals it, and the linearised DP never loses to its own left-deep order.
//! Timings are printed, never asserted.
//! Read: Adaptive Optimization of Very Large Join Queries (T. Neumann et al., SIGMOD 2018)   Optimal Top-Down Join Enumeration
//!       (D. DeHaan et al., SIGMOD 2007)   Counter Strike: Generic Top-Down Join Enumeration for Hypergraphs (P. Fender et al.,
//!       VLDB 2013)   A New, Highly Efficient, and Easy To Implement Top-Down Join Enumeration Algorithm (P. Fender et al., VLDB
//!       2011)   Dynamic Programming Strikes Back (G. Moerkotte, T. Neumann, SIGMOD 2008)   schedule:
//!       https://15799.courses.cs.cmu.edu/spring2025/schedule.html

use std::collections::{HashMap, HashSet};
use std::time::Instant;

const INF: f64 = f64::INFINITY;
/// IDP examines at most this many connected cluster sets per level (keeps star-shaped queries tractable).
const LEVEL_BUDGET: usize = 3000;

// ---------- utilities ----------
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n.max(1)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Costs come from different summation orders, so equality is relative.
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-300)
}

fn bits(mut m: u32) -> impl Iterator<Item = u32> {
    std::iter::from_fn(move || {
        if m == 0 {
            None
        } else {
            let b = m.trailing_zeros();
            m &= m - 1;
            Some(b)
        }
    })
}

/// All non-empty subsets of `n` in ASCENDING numeric order. The `(s-1) & n` trick yields them descending; DPccp/DPhyp need
/// ascending so that a smaller (sub)graph is always emitted before any larger graph that is built from it.
fn subsets(n: u32) -> Vec<u32> {
    let mut v = Vec::new();
    let mut s = n;
    while s != 0 {
        v.push(s);
        s = (s - 1) & n;
    }
    v.reverse();
    v
}

/// B_i = {R_0 .. R_i}: the nodes "smaller or equal" to i. Excluding them is what makes DPccp duplicate-free.
fn bset(i: u32) -> u32 {
    if i >= 31 {
        u32::MAX
    } else {
        (1u32 << (i + 1)) - 1
    }
}

/// B_min(S): all nodes up to and including the lowest node of S.
fn bmin(s: u32) -> u32 {
    bset(s.trailing_zeros())
}

// ---------- large-query representation (pairwise predicates, n up to a few hundred) ----------
#[derive(Clone)]
struct Query {
    n: usize,
    card: Vec<f64>,
    pairs: Vec<(usize, usize, f64)>,
    sel: Vec<f64>,  // dense n*n selectivity, 1.0 when no predicate
    has: Vec<bool>, // dense n*n edge presence
    adj: Vec<Vec<usize>>,
}

impl Query {
    fn new(n: usize, card: Vec<f64>, pairs: Vec<(usize, usize, f64)>) -> Query {
        let mut sel = vec![1.0; n * n];
        let mut has = vec![false; n * n];
        let mut adj = vec![Vec::new(); n];
        for &(a, b, s) in &pairs {
            sel[a * n + b] = s;
            sel[b * n + a] = s;
            has[a * n + b] = true;
            has[b * n + a] = true;
            adj[a].push(b);
            adj[b].push(a);
        }
        Query {
            n,
            card,
            pairs,
            sel,
            has,
            adj,
        }
    }

    /// Cardinality of joining exactly `members`: product of base cards times every predicate fully inside the set.
    fn card_of(&self, members: &[usize]) -> f64 {
        // summed in log space: 200 base cardinalities multiplied first would overflow f64 before the selectivities shrink it
        let mut inside = vec![false; self.n];
        let mut ln = 0.0;
        for &m in members {
            inside[m] = true;
            ln += self.card[m].ln();
        }
        for &(a, b, s) in &self.pairs {
            if inside[a] && inside[b] {
                ln += s.ln();
            }
        }
        ln.exp()
    }

    fn to_graph(&self) -> Graph {
        let edges = self
            .pairs
            .iter()
            .map(|&(a, b, s)| HEdge {
                u: 1 << a,
                v: 1 << b,
                sel: s,
            })
            .collect();
        Graph::new(self.card.clone(), edges)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Shape {
    Chain,
    Cycle,
    Star,
    Clique,
    Random,
}

const SHAPES: [Shape; 5] = [
    Shape::Chain,
    Shape::Cycle,
    Shape::Star,
    Shape::Clique,
    Shape::Random,
];

fn gen_query(rng: &mut Rng, shape: Shape, n: usize) -> Query {
    let card: Vec<f64> = (0..n)
        .map(|_| 10f64.powf(1.0 + 4.0 * rng.unit()).round())
        .collect();
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut raw: Vec<(usize, usize)> = Vec::new();
    let mut add = |a: usize, b: usize, raw: &mut Vec<(usize, usize)>| {
        let key = (a.min(b), a.max(b));
        if a != b && seen.insert(key) {
            raw.push(key);
        }
    };
    match shape {
        Shape::Chain => (1..n).for_each(|i| add(i - 1, i, &mut raw)),
        Shape::Cycle => {
            (1..n).for_each(|i| add(i - 1, i, &mut raw));
            if n >= 3 {
                add(n - 1, 0, &mut raw);
            }
        }
        Shape::Star => (1..n).for_each(|i| add(0, i, &mut raw)),
        Shape::Clique => (0..n).for_each(|i| (i + 1..n).for_each(|j| add(i, j, &mut raw))),
        Shape::Random => {
            // a random spanning tree keeps it connected, extra edges add cycles
            for k in 1..n {
                let p = rng.below(k as u64) as usize;
                add(p, k, &mut raw);
            }
            for _ in 0..n / 2 {
                let (a, b) = (rng.below(n as u64) as usize, rng.below(n as u64) as usize);
                add(a, b, &mut raw);
            }
        }
    }
    // selectivity ~ 1/max(|A|,|B|) (a foreign-key-like join) jittered by 0.3..3, never above 1
    let pairs = raw
        .into_iter()
        .map(|(a, b)| {
            let s = (1.0 / card[a].max(card[b])) * (0.3 + 2.7 * rng.unit());
            (a, b, s.min(1.0))
        })
        .collect();
    Query::new(n, card, pairs)
}

// ---------- small-query hypergraph + DP table (bitmask sets, n <= ~20) ----------
#[derive(Clone, Copy, Debug)]
struct HEdge {
    u: u32,
    v: u32,
    sel: f64,
}

struct Graph {
    n: usize,
    card: Vec<f64>,
    edges: Vec<HEdge>,
    adj: Vec<u32>,     // neighbours through SIMPLE edges only
    simple: bool,      // every edge joins two single relations
    card_of: Vec<f64>, // cardinality of the join result of every subset
}

impl Graph {
    fn new(card: Vec<f64>, edges: Vec<HEdge>) -> Graph {
        let n = card.len();
        let mut adj = vec![0u32; n];
        let mut simple = true;
        for e in &edges {
            if e.u.count_ones() == 1 && e.v.count_ones() == 1 {
                adj[e.u.trailing_zeros() as usize] |= e.v;
                adj[e.v.trailing_zeros() as usize] |= e.u;
            } else {
                simple = false;
            }
        }
        // card(S) = product of base cards * every edge fully inside S: a pure function of the SET, so the same
        // for every join order (this is what makes the DP's principle of optimality valid).
        let mut card_of = vec![1.0; 1 << n];
        for s in 1..(1u32 << n) {
            let mut c: f64 = bits(s).map(|i| card[i as usize]).product();
            for e in &edges {
                if (e.u | e.v) & !s == 0 {
                    c *= e.sel;
                }
            }
            card_of[s as usize] = c;
        }
        Graph {
            n,
            card,
            edges,
            adj,
            simple,
            card_of,
        }
    }

    fn full(&self) -> u32 {
        ((1u64 << self.n) - 1) as u32
    }

    /// Simple-edge neighbourhood of a set (excluding the set).
    fn neigh(&self, s: u32) -> u32 {
        bits(s).fold(0, |acc, i| acc | self.adj[i as usize]) & !s
    }

    /// Is there a hyperedge with one end inside `a` and the other inside `b`?
    fn edge_between(&self, a: u32, b: u32) -> bool {
        if self.simple {
            return self.neigh(a) & b != 0;
        }
        self.edges
            .iter()
            .any(|e| (e.u & a == e.u && e.v & b == e.v) || (e.v & a == e.v && e.u & b == e.u))
    }

    /// Connectivity of the sub-graph induced on `s` (simple edges only).
    fn connected(&self, s: u32) -> bool {
        let mut reach = s & s.wrapping_neg();
        loop {
            let next = reach | (self.neigh(reach) & s);
            if next == reach {
                return reach == s;
            }
            reach = next;
        }
    }
}

/// DP table: best cost per connected subset + the split that achieved it + instrumentation.
struct Dp {
    cost: Vec<f64>,
    split: Vec<u32>,
    pairs: u64,  // valid (connected, adjacent, disjoint) pairs handed to the cost function
    checks: u64, // candidate pairs examined, valid or not
}

impl Dp {
    fn new(g: &Graph) -> Dp {
        let mut cost = vec![INF; 1 << g.n];
        for i in 0..g.n {
            cost[1 << i] = 0.0;
        }
        Dp {
            cost,
            split: vec![0; 1 << g.n],
            pairs: 0,
            checks: 0,
        }
    }

    /// Cost the join S1 x S2. The asserts prove that the enumeration order always has both inputs solved already.
    fn relax(&mut self, g: &Graph, a: u32, b: u32) {
        assert!(
            self.cost[a as usize].is_finite() && self.cost[b as usize].is_finite(),
            "input plan not ready"
        );
        self.pairs += 1;
        let s = (a | b) as usize;
        let c = self.cost[a as usize] + self.cost[b as usize] + g.card_of[s];
        if c < self.cost[s] {
            self.cost[s] = c;
            self.split[s] = a;
        }
    }

    fn best(&self, g: &Graph) -> f64 {
        self.cost[g.full() as usize]
    }
}

// ---------- DPsize ----------
fn dp_size(g: &Graph) -> Dp {
    let mut dp = Dp::new(g);
    let mut by_size: Vec<Vec<u32>> = vec![Vec::new(); g.n + 1];
    by_size[1] = (0..g.n).map(|i| 1u32 << i).collect();
    for s in 2..=g.n {
        for s1 in 1..s {
            let s2 = s - s1;
            for &a in &by_size[s1] {
                for &b in &by_size[s2] {
                    dp.checks += 1;
                    if a & b != 0 || !g.edge_between(a, b) {
                        continue;
                    }
                    dp.relax(g, a, b);
                }
            }
        }
        // plans of size s now exist exactly for the connected subsets of size s
        by_size[s] = (1..=g.full())
            .filter(|m| m.count_ones() as usize == s && dp.cost[*m as usize].is_finite())
            .collect();
    }
    dp.pairs /= 2; // every unordered pair was seen as (a,b) and (b,a)
    dp
}

// ---------- DPsub ----------
fn dp_sub(g: &Graph) -> Dp {
    let mut dp = Dp::new(g);
    for s in 1..=g.full() {
        if s.count_ones() < 2 {
            continue;
        }
        let low = s & s.wrapping_neg(); // S1 always holds the lowest bit: each unordered split is tried once
        let rest = s ^ low;
        let mut sub = rest;
        loop {
            let a = low | sub;
            let b = s ^ a;
            if b != 0 {
                dp.checks += 1;
                if dp.cost[a as usize].is_finite()
                    && dp.cost[b as usize].is_finite()
                    && g.edge_between(a, b)
                {
                    dp.relax(g, a, b);
                }
            }
            if sub == 0 {
                break;
            }
            sub = (sub - 1) & rest;
        }
    }
    dp
}

// ---------- DPccp: EnumerateCsg / EnumerateCmp exactly as in the paper ----------
struct Ccp<'a> {
    g: &'a Graph,
    dp: Dp,
}

impl Ccp<'_> {
    fn solve(mut self) -> Dp {
        assert!(self.g.simple, "DPccp needs a simple graph");
        for i in (0..self.g.n as u32).rev() {
            self.emit_csg(1 << i);
            self.enumerate_csg_rec(1 << i, bset(i));
        }
        self.dp
    }

    /// Grow connected subgraph `s` by every non-empty subset of its not-yet-forbidden neighbours; X forbids nodes that
    /// would let the same csg be reached from a different start node.
    fn enumerate_csg_rec(&mut self, s: u32, x: u32) {
        let n = self.g.neigh(s) & !x;
        for sub in subsets(n) {
            self.emit_csg(s | sub);
        }
        for sub in subsets(n) {
            self.enumerate_csg_rec(s | sub, x | n);
        }
    }

    /// For csg `s1`: every neighbour node is a seed of a complement, forbidding smaller neighbours to avoid duplicates.
    fn emit_csg(&mut self, s1: u32) {
        let x = s1 | bmin(s1);
        let n = self.g.neigh(s1) & !x;
        let mut seeds: Vec<u32> = bits(n).collect();
        seeds.reverse();
        for v in seeds {
            self.emit_pair(s1, 1 << v);
            self.enumerate_cmp_rec(s1, 1 << v, x | (bset(v) & n));
        }
    }

    fn enumerate_cmp_rec(&mut self, s1: u32, s2: u32, x: u32) {
        let n = self.g.neigh(s2) & !x;
        for sub in subsets(n) {
            self.emit_pair(s1, s2 | sub);
        }
        for sub in subsets(n) {
            self.enumerate_cmp_rec(s1, s2 | sub, x | n);
        }
    }

    fn emit_pair(&mut self, s1: u32, s2: u32) {
        self.dp.checks += 1; // every call is a useful pair: checks == pairs
        self.dp.relax(self.g, s1, s2);
    }
}

fn dp_ccp(g: &Graph) -> Dp {
    Ccp { g, dp: Dp::new(g) }.solve()
}

// ---------- DPhyp: the same walk over hyperedges ----------
struct Hyp<'a> {
    g: &'a Graph,
    dp: Dp,
}

impl Hyp<'_> {
    fn solve(mut self) -> Dp {
        for i in (0..self.g.n as u32).rev() {
            self.emit_csg(1 << i);
            self.csg_rec(1 << i, bset(i));
        }
        self.dp
    }

    /// Neighbourhood of `s` avoiding `x`: for each hyperedge whose near end lies inside `s` and whose far end is untouched,
    /// the far end is a candidate; subsumed candidates (strict supersets of another) are dropped and each survivor is
    /// represented by its minimal node. Reaching the whole hypernode happens through the recursion.
    fn neighborhood(&self, s: u32, x: u32) -> u32 {
        let forbidden = s | x;
        let mut cands: Vec<u32> = Vec::new();
        for e in &self.g.edges {
            for (near, far) in [(e.u, e.v), (e.v, e.u)] {
                if near & s == near && far & forbidden == 0 {
                    cands.push(far);
                }
            }
        }
        let mut reps = 0;
        for &c in &cands {
            if !cands.iter().any(|&d| d != c && d & c == d) {
                reps |= c & c.wrapping_neg();
            }
        }
        reps
    }

    fn csg_rec(&mut self, s: u32, x: u32) {
        let n = self.neighborhood(s, x);
        for sub in subsets(n) {
            // only subsets that are themselves connected (have a plan) are csgs
            if self.dp.cost[(s | sub) as usize].is_finite() {
                self.emit_csg(s | sub);
            }
        }
        for sub in subsets(n) {
            self.csg_rec(s | sub, x | n);
        }
    }

    fn emit_csg(&mut self, s1: u32) {
        let x = s1 | bmin(s1);
        let n = self.neighborhood(s1, x);
        let mut seeds: Vec<u32> = bits(n).collect();
        seeds.reverse();
        for v in seeds {
            let s2 = 1u32 << v;
            self.dp.checks += 1;
            if self.g.edge_between(s1, s2) {
                self.dp.relax(self.g, s1, s2);
            }
            self.cmp_rec(s1, s2, x | (bset(v) & n));
        }
    }

    fn cmp_rec(&mut self, s1: u32, s2: u32, x: u32) {
        let n = self.neighborhood(s2, x);
        for sub in subsets(n) {
            let t = s2 | sub;
            self.dp.checks += 1;
            if self.dp.cost[t as usize].is_finite() && self.g.edge_between(s1, t) {
                self.dp.relax(self.g, s1, t);
            }
        }
        for sub in subsets(n) {
            self.cmp_rec(s1, s2 | sub, x | n);
        }
    }
}

fn dp_hyp(g: &Graph) -> Dp {
    Hyp { g, dp: Dp::new(g) }.solve()
}

// ---------- top-down enumeration with a partitioning function ----------
struct TopDown<'a> {
    g: &'a Graph,
    memo: HashMap<u32, f64>,
    branch: bool, // false: generate-and-test partitioning, true: connected-side branching
    checks: u64,
    pairs: u64,
}

impl TopDown<'_> {
    fn best(&mut self, s: u32) -> f64 {
        if s.count_ones() == 1 {
            return 0.0;
        }
        if let Some(&c) = self.memo.get(&s) {
            return c;
        }
        let parts = if self.branch {
            self.parts_branch(s)
        } else {
            self.parts_naive(s)
        };
        let mut best = INF;
        for (a, b) in parts {
            self.pairs += 1;
            let c = self.best(a) + self.best(b) + self.g.card_of[s as usize];
            best = best.min(c);
        }
        self.memo.insert(s, best);
        best
    }

    /// Generate all 2^(|S|-1)-1 splits, test both halves for connectedness.
    fn parts_naive(&mut self, s: u32) -> Vec<(u32, u32)> {
        let low = s & s.wrapping_neg();
        let rest = s ^ low;
        let mut out = Vec::new();
        for sub in std::iter::once(0).chain(subsets(rest)) {
            let a = low | sub;
            let b = s ^ a;
            if b == 0 {
                continue;
            }
            self.checks += 1;
            if self.g.connected(a) && self.g.connected(b) {
                out.push((a, b));
            }
        }
        out
    }

    fn parts_branch(&mut self, s: u32) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        self.grow(s, s & s.wrapping_neg(), 0, &mut out);
        out
    }

    /// Enumerate every connected C that contains the lowest node, once: extend by a neighbour v, recurse, then forbid v
    /// for the remaining siblings. Only the complement needs a connectivity test.
    fn grow(&mut self, s: u32, c: u32, x: u32, out: &mut Vec<(u32, u32)>) {
        let rest = s ^ c;
        if rest != 0 {
            self.checks += 1;
            if self.g.connected(rest) {
                out.push((c, rest));
            }
        }
        let mut excluded = x;
        for v in bits(self.g.neigh(c) & s & !x) {
            self.grow(s, c | (1 << v), excluded, out);
            excluded |= 1 << v;
        }
    }
}

fn top_down(g: &Graph, branch: bool) -> (f64, u64, u64) {
    let mut td = TopDown {
        g,
        memo: HashMap::new(),
        branch,
        checks: 0,
        pairs: 0,
    };
    let c = td.best(g.full());
    (c, td.pairs, td.checks)
}

// ---------- memo-less brute force: the independent oracle ----------
fn brute(g: &Graph, s: u32) -> Option<f64> {
    if s.count_ones() == 1 {
        return Some(0.0);
    }
    let low = s & s.wrapping_neg();
    let mut best: Option<f64> = None;
    for sub in std::iter::once(0).chain(subsets(s ^ low)) {
        let (a, b) = (low | sub, s ^ (low | sub));
        if b == 0 || !g.edge_between(a, b) {
            continue;
        }
        if let (Some(x), Some(y)) = (brute(g, a), brute(g, b)) {
            let c = x + y + g.card_of[s as usize];
            best = Some(best.map_or(c, |p: f64| p.min(c)));
        }
    }
    best
}

// ---------- plan printing ----------
fn explain(g: &Graph, dp: &Dp, s: u32, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    if s.count_ones() == 1 {
        out.push_str(&format!(
            "{pad}Scan R{} (rows={:.0})\n",
            s.trailing_zeros(),
            g.card_of[s as usize]
        ));
        return;
    }
    let a = dp.split[s as usize];
    out.push_str(&format!(
        "{pad}Join (rows={:.1}, cost={:.1})\n",
        g.card_of[s as usize], dp.cost[s as usize]
    ));
    explain(g, dp, a, depth + 1, out);
    explain(g, dp, s ^ a, depth + 1, out);
}

// ---------- heuristics for large queries ----------
#[derive(Clone)]
struct Cl {
    mem: Vec<usize>,
    card: f64,
    cost: f64,
}

fn single(q: &Query, i: usize) -> Cl {
    Cl {
        mem: vec![i],
        card: q.card[i],
        cost: 0.0,
    }
}

/// Selectivity product between two clusters and whether any predicate links them.
fn link(q: &Query, a: &Cl, b: &Cl) -> (bool, f64) {
    let (mut linked, mut s) = (false, 1.0);
    for &x in &a.mem {
        for &y in &b.mem {
            if q.has[x * q.n + y] {
                linked = true;
                s *= q.sel[x * q.n + y];
            }
        }
    }
    (linked, s)
}

fn join_cl(a: &Cl, b: &Cl, sel: f64) -> Cl {
    let card = a.card * b.card * sel;
    Cl {
        mem: a.mem.iter().chain(&b.mem).copied().collect(),
        card,
        cost: a.cost + b.cost + card,
    }
}

/// Greedy Operator Ordering: repeatedly merge the linked cluster pair with the smallest result cardinality.
fn goo(q: &Query) -> f64 {
    let mut cls: Vec<Cl> = (0..q.n).map(|i| single(q, i)).collect();
    while cls.len() > 1 {
        let mut pick: Option<(usize, usize, f64, f64)> = None;
        for i in 0..cls.len() {
            for j in i + 1..cls.len() {
                let (linked, s) = link(q, &cls[i], &cls[j]);
                let card = cls[i].card * cls[j].card * s;
                if linked && pick.is_none_or(|p| card < p.3) {
                    pick = Some((i, j, s, card));
                }
            }
        }
        let (i, j, s, _) = pick.expect("query graph must be connected");
        let b = cls.remove(j);
        let a = cls.remove(i);
        cls.push(join_cl(&a, &b, s));
    }
    cls[0].cost
}

/// Greedy linear order: start at the smallest relation, then repeatedly append the linked relation giving the smallest prefix.
/// (Neumann's version uses IKKBZ ranks; a greedy prefix works the same way for the demo and is cheaper to read.)
fn greedy_order(q: &Query) -> Vec<usize> {
    let start = (0..q.n)
        .min_by(|&a, &b| q.card[a].partial_cmp(&q.card[b]).unwrap())
        .unwrap();
    let mut order = vec![start];
    let mut used = vec![false; q.n];
    used[start] = true;
    let mut cur = q.card[start];
    while order.len() < q.n {
        let mut best: Option<(usize, f64)> = None;
        for v in (0..q.n).filter(|&v| !used[v] && q.adj[v].iter().any(|&u| used[u])) {
            let s: f64 = order.iter().map(|&u| q.sel[u * q.n + v]).product();
            let c = cur * q.card[v] * s;
            if best.is_none_or(|b| c < b.1) {
                best = Some((v, c));
            }
        }
        let (v, c) = best.expect("connected query");
        used[v] = true;
        order.push(v);
        cur = c;
    }
    order
}

fn left_deep_cost(q: &Query, order: &[usize]) -> f64 {
    let mut cl = single(q, order[0]);
    for &v in &order[1..] {
        let nxt = single(q, v);
        let (linked, s) = link(q, &cl, &nxt);
        assert!(linked, "order must keep the prefix connected");
        cl = join_cl(&cl, &nxt, s);
    }
    cl.cost
}

/// Optimal bushy tree among those whose every subtree is a contiguous interval of `order` (O(n^3) splits).
fn linearized_dp(q: &Query, order: &[usize]) -> f64 {
    let m = order.len();
    let mut pos = vec![0usize; q.n];
    for (i, &v) in order.iter().enumerate() {
        pos[v] = i;
    }
    let idx = |i: usize, j: usize| i * m + j;
    let mut card = vec![0.0; m * m];
    let mut cost = vec![INF; m * m];
    for i in 0..m {
        card[idx(i, i)] = q.card[order[i]];
        cost[idx(i, i)] = 0.0;
    }
    for len in 2..=m {
        for i in 0..=m - len {
            let j = i + len - 1;
            let sel: f64 = (i..j).map(|a| q.sel[order[a] * q.n + order[j]]).product();
            card[idx(i, j)] = card[idx(i, j - 1)] * card[idx(j, j)] * sel;
            let mut best = INF;
            for k in i..j {
                let (l, r) = (cost[idx(i, k)], cost[idx(k + 1, j)]);
                if !l.is_finite() || !r.is_finite() {
                    continue;
                }
                // a split is legal only if some predicate crosses it (no cross products)
                let crosses =
                    (i..=k).any(|a| q.adj[order[a]].iter().any(|&b| pos[b] > k && pos[b] <= j));
                if crosses {
                    best = best.min(l + r + card[idx(i, j)]);
                }
            }
            cost[idx(i, j)] = best;
        }
    }
    cost[idx(0, m - 1)]
}

fn linearized(q: &Query) -> f64 {
    linearized_dp(q, &greedy_order(q))
}

/// IDP1: exact DP over connected sets of at most k clusters, collapse the lowest-cardinality best sub-plan, repeat.
fn idp(q: &Query, k: usize) -> f64 {
    let mut cls: Vec<Cl> = (0..q.n).map(|i| single(q, i)).collect();
    while cls.len() > 1 {
        let m = cls.len();
        let kk = k.clamp(2, m);
        let mut owner = vec![0usize; q.n];
        for (ci, c) in cls.iter().enumerate() {
            c.mem.iter().for_each(|&v| owner[v] = ci);
        }
        let mut cadj: Vec<Vec<usize>> = vec![Vec::new(); m];
        for &(a, b, _) in &q.pairs {
            let (x, y) = (owner[a], owner[b]);
            if x != y {
                cadj[x].push(y);
                cadj[y].push(x);
            }
        }
        cadj.iter_mut().for_each(|l| {
            l.sort_unstable();
            l.dedup();
        });
        let mut best: HashMap<Vec<usize>, (f64, f64)> = HashMap::new(); // set of cluster ids -> (cost, card)
        let mut prev: Vec<Vec<usize>> = (0..m).map(|c| vec![c]).collect();
        for c in 0..m {
            best.insert(vec![c], (cls[c].cost, cls[c].card));
        }
        for _ in 2..=kk {
            let mut next: HashSet<Vec<usize>> = HashSet::new();
            for t in &prev {
                for &c in t {
                    for &v in cadj[c].iter().filter(|v| !t.contains(v)) {
                        let mut u = t.clone();
                        u.push(v);
                        u.sort_unstable();
                        next.insert(u);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            let mut level: Vec<Vec<usize>> = next.into_iter().collect();
            level.sort(); // hash order is random; sorting keeps the run deterministic
            // a hub with 199 neighbours has C(199,3) connected 4-sets; like real IDP implementations cap the work per round
            level.truncate(LEVEL_BUDGET);
            for t in &level {
                let members: Vec<usize> =
                    t.iter().flat_map(|&c| cls[c].mem.iter().copied()).collect();
                let card = q.card_of(&members);
                let mut bc = INF;
                for mask in 1..(1usize << t.len()) - 1 {
                    if mask & 1 == 0 {
                        continue; // unordered splits: the first cluster is always on the left
                    }
                    let a: Vec<usize> = (0..t.len())
                        .filter(|i| mask >> i & 1 == 1)
                        .map(|i| t[i])
                        .collect();
                    let b: Vec<usize> = (0..t.len())
                        .filter(|i| mask >> i & 1 == 0)
                        .map(|i| t[i])
                        .collect();
                    if let (Some(x), Some(y)) = (best.get(&a), best.get(&b)) {
                        bc = bc.min(x.0 + y.0 + card);
                    }
                }
                best.insert(t.clone(), (bc, card));
            }
            prev = level;
        }
        let pick = prev
            .iter()
            .min_by(|a, b| {
                let (x, y) = (best[*a], best[*b]);
                x.1.partial_cmp(&y.1)
                    .unwrap()
                    .then(x.0.partial_cmp(&y.0).unwrap())
            })
            .unwrap()
            .clone();
        assert!(pick.len() >= 2, "query graph must be connected");
        let (cost, card) = best[&pick];
        let mut mem = Vec::new();
        for &c in pick.iter().rev() {
            mem.extend(cls.remove(c).mem);
        }
        cls.push(Cl { mem, card, cost });
    }
    cls[0].cost
}

/// Neumann-style adaptive choice by estimated search space.
fn adaptive(q: &Query) -> (f64, &'static str) {
    if q.n <= 12 {
        let g = q.to_graph();
        (dp_ccp(&g).best(&g), "exact DPccp")
    } else if q.n <= 120 {
        // cheap candidates first, keep the best: linearised DP is O(n^3), GOO is near-free
        (linearized(q).min(goo(q)), "min(linearised DP, GOO)")
    } else {
        (idp(q, 4).min(goo(q)), "min(IDP k=4, GOO)")
    }
}

// ---------- hypergraph generator ----------
/// Relations arrive in groups of 1-2; each group is tied to 1-2 earlier relations by ONE hyperedge, like a join whose
/// predicate (or outer-join constraint) needs both of those relations on the same side.
fn gen_hyper(rng: &mut Rng, n: usize) -> Graph {
    let card: Vec<f64> = (0..n)
        .map(|_| 10f64.powf(1.0 + 3.0 * rng.unit()).round())
        .collect();
    let mut edges = Vec::new();
    let mut placed = 0usize;
    while placed < n {
        let g = if n - placed >= 2 && rng.below(2) == 0 {
            2
        } else {
            1
        };
        let group: u32 = (placed..placed + g).fold(0, |m, i| m | 1 << i);
        if g == 2 {
            edges.push(HEdge {
                u: 1 << placed,
                v: 1 << (placed + 1),
                sel: 0.01 + 0.2 * rng.unit(),
            });
        }
        if placed > 0 {
            let want = 1 + rng.below(2.min(placed) as u64) as usize;
            let mut u = 0u32;
            while (u.count_ones() as usize) < want {
                u |= 1 << rng.below(placed as u64);
            }
            edges.push(HEdge {
                u,
                v: group,
                sel: 10f64.powf(-3.0 * rng.unit()),
            });
        }
        placed += g;
    }
    Graph::new(card, edges)
}

// ---------- checks ----------
fn check_equivalence() -> [[u64; 6]; 5] {
    let mut rng = Rng::new(0x751);
    let mut totals = [[0u64; 6]; 5]; // per shape: queries, dpsize checks, dpsub checks, pairs, td naive checks, td branch checks
    for q_no in 0..2000 {
        let shape = SHAPES[q_no % 5];
        let n = 2 + rng.below(9) as usize; // 2..=10
        let q = gen_query(&mut rng, shape, n);
        let g = q.to_graph();
        let (size, sub, ccp, hyp) = (dp_size(&g), dp_sub(&g), dp_ccp(&g), dp_hyp(&g));
        let (tn, tb) = (top_down(&g, false), top_down(&g, true));
        let opt = ccp.best(&g);
        assert!(
            opt.is_finite(),
            "{shape:?} n={n}: connected query must have a plan"
        );
        for (name, c) in [
            ("DPsize", size.best(&g)),
            ("DPsub", sub.best(&g)),
            ("DPhyp", hyp.best(&g)),
            ("TD naive", tn.0),
            ("TD branch", tb.0),
        ] {
            assert!(
                same(c, opt),
                "{name} disagrees on {shape:?} n={n}: {c} vs {opt}"
            );
        }
        if n <= 6 {
            assert!(
                same(brute(&g, g.full()).unwrap(), opt),
                "brute force disagrees"
            );
        }
        // every csg-cmp pair is visited exactly once by DPccp, DPhyp, DPsub, DPsize/2 and both partitioners
        let p = ccp.pairs;
        assert_eq!(ccp.checks, p, "DPccp must not waste candidates");
        assert!(
            sub.pairs == p && size.pairs == p && hyp.pairs == p && tn.1 == p && tb.1 == p,
            "pair counts differ {shape:?} n={n}"
        );
        assert!(
            tb.2 <= tn.2,
            "branching must not test more splits than generate-and-test"
        );
        let t = &mut totals[q_no % 5];
        t[0] += 1;
        t[1] += size.checks;
        t[2] += sub.checks;
        t[3] += p;
        t[4] += tn.2;
        t[5] += tb.2;
    }
    totals
}

fn closed_form_chain(n: u64) -> u64 {
    (n * n * n - n) / 6
}
fn closed_form_star(n: u64) -> u64 {
    (n - 1) * (1 << (n - 2))
}
fn closed_form_clique(n: u64) -> u64 {
    (3u64.pow(n as u32) - (1 << (n + 1)) + 1) / 2
}

fn check_closed_forms(rng: &mut Rng) {
    for n in 2..=12usize {
        for (shape, f) in [
            (Shape::Chain, closed_form_chain as fn(u64) -> u64),
            (Shape::Star, closed_form_star),
            (Shape::Clique, closed_form_clique),
        ] {
            let g = gen_query(rng, shape, n).to_graph();
            let dp = dp_ccp(&g);
            assert_eq!(dp.pairs, f(n as u64), "{shape:?} n={n}");
        }
    }
}

fn check_hypergraphs() -> (usize, usize) {
    let mut rng = Rng::new(0xD9);
    let (mut planned, mut impossible) = (0, 0);
    for i in 0..600 {
        let n = 3 + i % 6; // 3..=8
        let g = gen_hyper(&mut rng, n);
        let (hyp, sub) = (dp_hyp(&g), dp_sub(&g));
        let oracle = brute(&g, g.full());
        match oracle {
            Some(c) => {
                planned += 1;
                assert!(
                    same(hyp.best(&g), c) && same(sub.best(&g), c),
                    "DPhyp != brute force on n={n}: {} vs {c}",
                    hyp.best(&g)
                );
                assert_eq!(
                    hyp.pairs, sub.pairs,
                    "DPhyp must enumerate exactly the valid pairs"
                );
            }
            None => {
                impossible += 1;
                assert!(!hyp.best(&g).is_finite(), "DPhyp invented a plan");
            }
        }
    }
    (planned, impossible)
}

fn check_heuristics(rng: &mut Rng) {
    for i in 0..300 {
        let n = 5 + i % 8; // 5..=12
        let q = gen_query(rng, SHAPES[i % 5], n);
        let g = q.to_graph();
        let opt = dp_ccp(&g).best(&g);
        let (gc, lc, ic, (ac, _)) = (goo(&q), linearized(&q), idp(&q, 3), adaptive(&q));
        for (name, c) in [("GOO", gc), ("linearised", lc), ("IDP-3", ic)] {
            assert!(
                c >= opt * (1.0 - 1e-9),
                "{name} beat the optimum: {c} < {opt}"
            );
        }
        assert!(same(ac, opt), "adaptive uses exact DP at n<=12");
        if n <= 9 {
            assert!(same(idp(&q, n), opt), "IDP with k=n is exact DP");
        }
        let order = greedy_order(&q);
        assert!(
            linearized_dp(&q, &order) <= left_deep_cost(&q, &order) * (1.0 + 1e-9),
            "interval DP includes the left-deep tree"
        );
    }
}

// ---------- demos ----------
fn demo_plan() {
    let mut rng = Rng::new(7);
    let q = gen_query(&mut rng, Shape::Random, 5);
    let g = q.to_graph();
    let dp = dp_ccp(&g);
    let mut s = String::new();
    explain(&g, &dp, g.full(), 1, &mut s);
    println!(
        "optimal plan for a 5-relation random query (C_out cost {:.1}):\n{s}",
        dp.best(&g)
    );
}

fn demo_hyper() {
    // R0 -- R1 (simple), ({R0,R1}) -- R2 (complex predicate / outer-join barrier), R2 -- R3 (simple)
    let card = vec![1000.0, 200.0, 50.0, 4000.0];
    let edges = vec![
        HEdge {
            u: 0b0001,
            v: 0b0010,
            sel: 0.01,
        },
        HEdge {
            u: 0b0011,
            v: 0b0100,
            sel: 0.02,
        },
        HEdge {
            u: 0b0100,
            v: 0b1000,
            sel: 0.001,
        },
    ];
    let g = Graph::new(card, edges);
    let dp = dp_hyp(&g);
    let mut s = String::new();
    explain(&g, &dp, g.full(), 1, &mut s);
    println!(
        "DPhyp on a hypergraph ({{R0,R1}}-R2 must stay together; R2 cannot join R0 or R1 alone), cost {:.1}:\n{s}",
        dp.best(&g)
    );
    assert!(same(dp.best(&g), brute(&g, g.full()).unwrap()));
    assert!(g.edge_between(0b0011, 0b0100) && !g.edge_between(0b0001, 0b0100));
}

fn counts_table(rng: &mut Rng, n: usize) {
    println!("search effort at n={n} (candidate pairs examined; useful = csg-cmp pairs):");
    println!(
        "  {:<7} {:>10} {:>10} {:>10} {:>10} {:>11} {:>11}",
        "shape", "useful", "DPsize", "DPsub", "DPccp", "TD-naive", "TD-branch"
    );
    for shape in [
        Shape::Chain,
        Shape::Cycle,
        Shape::Star,
        Shape::Clique,
        Shape::Random,
    ] {
        let g = gen_query(rng, shape, n).to_graph();
        let (size, sub, ccp) = (dp_size(&g), dp_sub(&g), dp_ccp(&g));
        let (tn, tb) = (top_down(&g, false), top_down(&g, true));
        println!(
            "  {:<7} {:>10} {:>10} {:>10} {:>10} {:>11} {:>11}",
            format!("{shape:?}"),
            ccp.pairs,
            size.checks,
            sub.checks,
            ccp.checks,
            tn.2,
            tb.2
        );
        assert!(ccp.checks <= size.checks.min(sub.checks));
    }
}

fn quality_table(rng: &mut Rng) {
    println!(
        "plan quality, n=12, mean cost ratio to the optimum over 10 queries per shape (1.00 = optimal):"
    );
    println!(
        "  {:<7} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "shape", "GOO", "IDP-3", "IDP-5", "linear", "adaptive"
    );
    for shape in SHAPES {
        let mut r = [0.0f64; 5];
        for _ in 0..10 {
            let q = gen_query(rng, shape, 12);
            let g = q.to_graph();
            let opt = dp_ccp(&g).best(&g);
            for (k, c) in [
                goo(&q),
                idp(&q, 3),
                idp(&q, 5),
                linearized(&q),
                adaptive(&q).0,
            ]
            .into_iter()
            .enumerate()
            {
                r[k] += c / opt / 10.0;
            }
        }
        println!(
            "  {:<7} {:>8.2} {:>8.2} {:>8.2} {:>8.2} {:>8.2}",
            format!("{shape:?}"),
            r[0],
            r[1],
            r[2],
            r[3],
            r[4]
        );
    }
}

fn large_table(rng: &mut Rng) {
    println!(
        "large queries: cost relative to the best heuristic on that query (1.00 = best), and milliseconds:"
    );
    println!(
        "  {:<7} {:>4} | {:>14} {:>14} {:>14} {:>14}",
        "shape", "n", "GOO", "IDP-4", "linearised", "adaptive"
    );
    for shape in [Shape::Chain, Shape::Cycle, Shape::Star, Shape::Random] {
        for n in [50usize, 100, 200] {
            let q = gen_query(rng, shape, n);
            let mut res: Vec<(f64, u128)> = Vec::new();
            for f in [
                goo as fn(&Query) -> f64,
                |q| idp(q, 4),
                linearized,
                |q| adaptive(q).0,
            ] {
                let t = Instant::now();
                let c = f(&q);
                res.push((c, t.elapsed().as_millis()));
            }
            let best = res.iter().map(|r| r.0).fold(INF, f64::min);
            assert!(
                res.iter().all(|r| r.0.is_finite() && r.0 > 0.0),
                "{shape:?} n={n}: {:?}",
                res.iter().map(|r| r.0).collect::<Vec<_>>()
            );
            let cell = |r: &(f64, u128)| format!("{:>7.2} {:>4}ms", r.0 / best, r.1);
            println!(
                "  {:<7} {:>4} | {:>14} {:>14} {:>14} {:>14}",
                format!("{shape:?}"),
                n,
                cell(&res[0]),
                cell(&res[1]),
                cell(&res[2]),
                cell(&res[3])
            );
        }
    }
}

fn main() {
    let start = Instant::now();
    demo_plan();
    demo_hyper();

    let totals = check_equivalence();
    println!(
        "2000 random queries (n<=10): DPsize = DPsub = DPccp = DPhyp = TD-naive = TD-branch (cost and pair counts), brute force n<=6 OK"
    );
    println!("candidate checks summed per shape:");
    println!(
        "  {:<7} {:>6} {:>11} {:>11} {:>10} {:>11} {:>11}",
        "shape", "n_q", "DPsize", "DPsub", "useful", "TD-naive", "TD-branch"
    );
    for (i, t) in totals.iter().enumerate() {
        println!(
            "  {:<7} {:>6} {:>11} {:>11} {:>10} {:>11} {:>11}",
            format!("{:?}", SHAPES[i]),
            t[0],
            t[1],
            t[2],
            t[3],
            t[4],
            t[5]
        );
        assert!(t[1] >= t[3] && t[2] >= t[3]);
    }
    // DPsize/DPsub must test strictly more than DPccp on the sparse shapes
    assert!(totals[0][1] > 2 * totals[0][3] && totals[2][2] > totals[2][3]);

    let mut rng = Rng::new(99);
    check_closed_forms(&mut rng);
    println!(
        "DPccp pair counts match closed forms for n=2..12: chain (n^3-n)/6, star (n-1)2^(n-2), clique (3^n-2^(n+1)+1)/2"
    );
    for n in [4u64, 8, 12] {
        println!(
            "  n={n:>2}: chain {:>7} star {:>7} clique {:>8}",
            closed_form_chain(n),
            closed_form_star(n),
            closed_form_clique(n)
        );
    }

    let (planned, impossible) = check_hypergraphs();
    println!(
        "600 random hypergraphs: DPhyp == DPsub == brute force on {planned} plannable, correctly found no plan for {impossible}"
    );
    assert!(planned >= 300);

    check_heuristics(&mut rng);
    println!(
        "300 queries: GOO / IDP-3 / linearised never beat the optimum, IDP(k=n) and adaptive are exact, interval DP <= left-deep"
    );

    counts_table(&mut rng, 10);
    quality_table(&mut rng);
    large_table(&mut rng);
    println!("total {:.1}s", start.elapsed().as_secs_f64());
}
