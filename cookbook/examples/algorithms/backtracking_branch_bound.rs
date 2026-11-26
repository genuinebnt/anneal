//! RECIPE 271 (algorithms): backtracking, branch and bound, and constraint propagation on NP-hard problems.
//! LEVEL: advanced
//! Use in: exact solvers for small/medium NP-hard instances, puzzle solvers, schedulers, anywhere "try, prune, undo" fits.   Run: cargo run --release --example backtracking_branch_bound
#![allow(dead_code)]
//!
//! Backtracking = depth-first search over partial solutions with `apply` / `undo`. It becomes practical when you
//! PRUNE: cut a subtree as soon as it provably cannot beat what you have (bounds), or cannot be completed
//! (constraint propagation). Skiena ch. 9 and TAOCP 7.2.2 both stress the same three levers: a good variable ORDER
//! (fail first), a good BOUND, and cheap incremental state with undo. Every section prints the search-tree size with
//! and without the lever so the benefit is visible, and asserts the answer against an independent oracle.
//!
//! 1. A generic `Problem` trait with a recursive and an explicit-stack driver (demo: N-queens, 92 solutions at n=8).
//! 2. Graph colouring: naive vertex order vs DSATUR (most saturated vertex first); same chromatic number.
//! 3. Subset sum: meet-in-the-middle O(2^(n/2) log) vs brute force O(2^n).
//! 4. 0/1 knapsack: branch and bound with the fractional-relaxation upper bound vs the DP recurrence.
//! 5. TSP: branch and bound with a minimum-spanning-tree lower bound vs Held-Karp (n <= 12).
//! 6. Max clique: Bron-Kerbosch with and without pivoting vs brute force.
//! 7. Vertex cover: maximal-matching 2-approximation and the O(2^k) fixed-parameter branching algorithm.
//! 8. CSP map colouring: plain backtracking vs forward checking vs forward checking + MRV.
//!
//! COMMON USES: exact optimisation inside larger systems (MIP solvers are branch-and-bound/cut engines), SAT and
//! CSP solvers, compilers' register allocation by graph colouring (heuristics), timetabling, puzzle generators,
//! cryptanalysis (meet-in-the-middle), bioinformatics clique problems, parameterised-complexity kernels.
//! Read: Skiena, The Algorithm Design Manual (ch. 9 Combinatorial Search and Heuristic Methods; problem catalog:
//! Vertex Coloring, Clique, Vertex Cover, TSP, Knapsack); Knuth, The Art of Computer Programming Vol 4B, 7.2.2
//! Backtrack Programming; Brelaz, "New methods to color the vertices of a graph" (DSATUR, 1979).
//! https://www.algorist.com/  https://www-cs-faculty.stanford.edu/~knuth/taocp.html
//! https://en.wikipedia.org/wiki/Conflict-driven_clause_learning (the modern descendant of backtracking+learning)

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }
}

// ================================================================ 1. generic framework
trait Problem {
    type Choice: Copy;
    /// Choices extending the current partial solution (empty = dead end).
    fn candidates(&mut self) -> Vec<Self::Choice>;
    fn is_solution(&self) -> bool;
    fn apply(&mut self, c: Self::Choice);
    fn undo(&mut self, c: Self::Choice);
    /// Pruning hook, called after `apply`; false cuts the subtree.
    fn feasible(&self) -> bool {
        true
    }
}

#[derive(Default, Debug, Clone, Copy)]
struct Stats {
    nodes: u64,
    solutions: u64,
}

/// Recursive driver. `visit` returns true to stop the search.
fn search<P: Problem>(p: &mut P, st: &mut Stats, visit: &mut dyn FnMut(&P) -> bool) -> bool {
    st.nodes += 1;
    if p.is_solution() {
        st.solutions += 1;
        return visit(p);
    }
    for c in p.candidates() {
        p.apply(c);
        let stop = p.feasible() && search(p, st, visit);
        p.undo(c);
        if stop {
            return true;
        }
    }
    false
}

/// Explicit-stack driver: finds the first solution and leaves `p` in the solved state. No recursion depth limit.
fn search_first<P: Problem>(p: &mut P, st: &mut Stats) -> bool {
    st.nodes += 1;
    if p.is_solution() {
        st.solutions += 1;
        return true;
    }
    let mut frames: Vec<(Vec<P::Choice>, usize)> = vec![(p.candidates(), 0)];
    let mut applied: Vec<P::Choice> = vec![];
    while let Some(top) = frames.last_mut() {
        if top.1 < top.0.len() {
            let c = top.0[top.1];
            top.1 += 1;
            p.apply(c);
            if p.feasible() {
                applied.push(c);
                st.nodes += 1;
                if p.is_solution() {
                    st.solutions += 1;
                    return true;
                }
                let cs = p.candidates();
                frames.push((cs, 0));
            } else {
                p.undo(c);
            }
        } else {
            frames.pop();
            if let Some(c) = applied.pop() {
                p.undo(c);
            }
        }
    }
    false
}

struct Queens {
    n: usize,
    cols: Vec<usize>,
}
impl Problem for Queens {
    type Choice = usize;
    fn candidates(&mut self) -> Vec<usize> {
        let r = self.cols.len();
        (0..self.n)
            .filter(|&c| {
                self.cols
                    .iter()
                    .enumerate()
                    .all(|(i, &x)| x != c && x.abs_diff(c) != r - i)
            })
            .collect()
    }
    fn is_solution(&self) -> bool {
        self.cols.len() == self.n
    }
    fn apply(&mut self, c: usize) {
        self.cols.push(c);
    }
    fn undo(&mut self, _: usize) {
        self.cols.pop();
    }
}

// ================================================================ 2. graph colouring
fn random_graph(rng: &mut Rng, n: usize, p: f64) -> Vec<Vec<usize>> {
    let mut adj = vec![vec![]; n];
    for i in 0..n {
        for j in i + 1..n {
            if rng.chance(p) {
                adj[i].push(j);
                adj[j].push(i);
            }
        }
    }
    adj
}

struct Colouring<'a> {
    adj: &'a [Vec<usize>],
    col: Vec<i32>,
    k: usize,
    used: usize,
    hist: Vec<usize>,
    done: usize,
    dsatur: bool,
}
impl Colouring<'_> {
    fn neigh_mask(&self, v: usize) -> u32 {
        self.adj[v]
            .iter()
            .filter(|&&w| self.col[w] >= 0)
            .fold(0, |m, &w| m | 1 << self.col[w])
    }
}
impl Problem for Colouring<'_> {
    type Choice = (usize, usize);
    fn candidates(&mut self) -> Vec<(usize, usize)> {
        let unc = (0..self.col.len()).filter(|&v| self.col[v] < 0);
        let v = if self.dsatur {
            unc.max_by_key(|&v| (self.neigh_mask(v).count_ones(), self.adj[v].len()))
                .unwrap()
        } else {
            unc.min().unwrap()
        };
        let m = self.neigh_mask(v);
        // symmetry breaking: never open a colour beyond used+1
        (0..self.k.min(self.used + 1))
            .filter(|&c| m >> c & 1 == 0)
            .map(|c| (v, c))
            .collect()
    }
    fn is_solution(&self) -> bool {
        self.done == self.col.len()
    }
    fn apply(&mut self, (v, c): (usize, usize)) {
        self.hist.push(self.used);
        self.col[v] = c as i32;
        self.used = self.used.max(c + 1);
        self.done += 1;
    }
    fn undo(&mut self, (v, _): (usize, usize)) {
        self.col[v] = -1;
        self.used = self.hist.pop().unwrap();
        self.done -= 1;
    }
}

/// Smallest k with a proper k-colouring; returns (k, total search nodes over all k tried).
fn chromatic(adj: &[Vec<usize>], dsatur: bool) -> (usize, u64) {
    let mut total = 0;
    for k in 1.. {
        let mut p = Colouring {
            adj,
            col: vec![-1; adj.len()],
            k,
            used: 0,
            hist: vec![],
            done: 0,
            dsatur,
        };
        let mut st = Stats::default();
        let found = search_first(&mut p, &mut st);
        total += st.nodes;
        if found {
            for v in 0..adj.len() {
                assert!(
                    adj[v].iter().all(|&w| p.col[w] != p.col[v]),
                    "proper colouring"
                );
            }
            return (k, total);
        }
    }
    unreachable!()
}

// ================================================================ 3. subset sum
fn subset_sums(items: &[i64]) -> Vec<(i64, u32)> {
    let mut v = vec![(0i64, 0u32)];
    for (i, &x) in items.iter().enumerate() {
        let len = v.len();
        for j in 0..len {
            v.push((v[j].0 + x, v[j].1 | 1 << i));
        }
    }
    v
}

/// Meet in the middle: sums of each half, sort one, two-pointer scan. Returns a subset mask over `items`.
fn subset_sum_mitm(items: &[i64], target: i64) -> Option<u64> {
    let h = items.len() / 2;
    let a = subset_sums(&items[..h]);
    let mut b = subset_sums(&items[h..]);
    b.sort();
    let (mut i, mut j) = (0, b.len());
    let mut a = a;
    a.sort();
    while i < a.len() && j > 0 {
        let s = a[i].0 + b[j - 1].0;
        if s == target {
            return Some(a[i].1 as u64 | (b[j - 1].1 as u64) << h);
        } else if s < target {
            i += 1;
        } else {
            j -= 1;
        }
    }
    None
}

fn subset_sum_brute(items: &[i64], target: i64) -> bool {
    (0u64..1 << items.len()).any(|m| {
        (0..items.len())
            .filter(|&i| m >> i & 1 == 1)
            .map(|i| items[i])
            .sum::<i64>()
            == target
    })
}

// ================================================================ 4. knapsack
fn knapsack_dp(w: &[usize], v: &[u64], cap: usize) -> u64 {
    let mut best = vec![0u64; cap + 1];
    for i in 0..w.len() {
        for c in (w[i]..=cap).rev() {
            best[c] = best[c].max(best[c - w[i]] + v[i]);
        }
    }
    best[cap]
}

struct Knap<'a> {
    w: &'a [usize],
    v: &'a [u64],
    cap: usize,
    best: u64,
    nodes: u64,
    use_bound: bool,
}
impl Knap<'_> {
    /// Upper bound: fill greedily by ratio, taking a FRACTION of the first item that does not fit.
    fn bound(&self, i: usize, weight: usize, value: u64) -> f64 {
        let mut b = value as f64;
        let mut room = (self.cap - weight) as f64;
        for j in i..self.w.len() {
            if self.w[j] as f64 <= room {
                room -= self.w[j] as f64;
                b += self.v[j] as f64;
            } else {
                b += self.v[j] as f64 * room / self.w[j] as f64;
                break;
            }
        }
        b
    }
    fn go(&mut self, i: usize, weight: usize, value: u64) {
        self.nodes += 1;
        self.best = self.best.max(value);
        if i == self.w.len() {
            return;
        }
        if self.use_bound && self.bound(i, weight, value).floor() as u64 <= self.best {
            return;
        }
        if weight + self.w[i] <= self.cap {
            self.go(i + 1, weight + self.w[i], value + self.v[i]);
        }
        self.go(i + 1, weight, value);
    }
}

// ================================================================ 5. TSP
fn held_karp(d: &[Vec<u32>]) -> u32 {
    let n = d.len();
    let m = n - 1; // city 0 is the start; subsets over cities 1..n
    let inf = u32::MAX / 2;
    let mut dp = vec![vec![inf; m]; 1 << m];
    for j in 0..m {
        dp[1 << j][j] = d[0][j + 1];
    }
    for s in 1usize..1 << m {
        for j in 0..m {
            if s >> j & 1 == 0 || dp[s][j] >= inf {
                continue;
            }
            for k in 0..m {
                if s >> k & 1 == 0 {
                    let t = s | 1 << k;
                    dp[t][k] = dp[t][k].min(dp[s][j] + d[j + 1][k + 1]);
                }
            }
        }
    }
    (0..m)
        .map(|j| dp[(1 << m) - 1][j] + d[j + 1][0])
        .min()
        .unwrap()
}

/// Weight of a minimum spanning tree over the cities in `mask` (Prim, O(m^2)).
fn mst_weight(d: &[Vec<u32>], mask: u32) -> u32 {
    let nodes: Vec<usize> = (0..d.len()).filter(|&i| mask >> i & 1 == 1).collect();
    if nodes.len() < 2 {
        return 0;
    }
    let mut dist = vec![u32::MAX; nodes.len()];
    let mut in_tree = vec![false; nodes.len()];
    dist[0] = 0;
    let mut total = 0;
    for _ in 0..nodes.len() {
        let u = (0..nodes.len())
            .filter(|&i| !in_tree[i])
            .min_by_key(|&i| dist[i])
            .unwrap();
        in_tree[u] = true;
        total += dist[u];
        for v in 0..nodes.len() {
            if !in_tree[v] {
                dist[v] = dist[v].min(d[nodes[u]][nodes[v]]);
            }
        }
    }
    total
}

struct Tsp<'a> {
    d: &'a [Vec<u32>],
    best: u32,
    nodes: u64,
    use_bound: bool,
}
impl Tsp<'_> {
    fn go(&mut self, cur: usize, visited: u32, cost: u32) {
        self.nodes += 1;
        let n = self.d.len();
        if visited.count_ones() as usize == n {
            self.best = self.best.min(cost + self.d[cur][0]);
            return;
        }
        // any completion is a Hamiltonian path cur -> unvisited -> 0, which spans {cur, 0, unvisited}: weight >= MST
        let lb = if self.use_bound {
            let rest = !visited & ((1 << n) - 1);
            mst_weight(self.d, rest | 1 << cur | 1)
        } else {
            0
        };
        if cost + lb >= self.best {
            return;
        }
        for next in 0..n {
            if visited >> next & 1 == 0 && cost + self.d[cur][next] < self.best {
                self.go(next, visited | 1 << next, cost + self.d[cur][next]);
            }
        }
    }
}

// ================================================================ 6. clique
fn bk(adj: &[u32], r: u32, mut p: u32, mut x: u32, pivot: bool, best: &mut u32, calls: &mut u64) {
    *calls += 1;
    if p == 0 && x == 0 {
        *best = (*best).max(r.count_ones());
        return;
    }
    let cand = if pivot {
        // pivot u in P|X maximising |P & N(u)|: only branch on vertices NOT adjacent to u
        let u = (0..adj.len())
            .filter(|&u| (p | x) >> u & 1 == 1)
            .max_by_key(|&u| (p & adj[u]).count_ones())
            .unwrap();
        p & !adj[u]
    } else {
        p
    };
    for v in 0..adj.len() {
        if cand >> v & 1 == 1 {
            bk(adj, r | 1 << v, p & adj[v], x & adj[v], pivot, best, calls);
            p &= !(1 << v);
            x |= 1 << v;
        }
    }
}

fn clique_brute(adj: &[u32]) -> u32 {
    let n = adj.len();
    let mut best = 0;
    for m in 0u32..1 << n {
        if (0..n)
            .filter(|&v| m >> v & 1 == 1)
            .all(|v| (m & !(1 << v)) & !adj[v] == 0)
        {
            best = best.max(m.count_ones());
        }
    }
    best
}

fn mask_graph(rng: &mut Rng, n: usize, p: f64) -> Vec<u32> {
    let mut adj = vec![0u32; n];
    for i in 0..n {
        for j in i + 1..n {
            if rng.chance(p) {
                adj[i] |= 1 << j;
                adj[j] |= 1 << i;
            }
        }
    }
    adj
}

// ================================================================ 7. vertex cover
fn edges_of(adj: &[u32]) -> Vec<(usize, usize)> {
    let mut e = vec![];
    for i in 0..adj.len() {
        for j in i + 1..adj.len() {
            if adj[i] >> j & 1 == 1 {
                e.push((i, j));
            }
        }
    }
    e
}

/// 2-approximation: take both endpoints of a greedily built maximal matching.
fn vc_two_approx(edges: &[(usize, usize)], n: usize) -> Vec<usize> {
    let mut taken = vec![false; n];
    for &(u, v) in edges {
        if !taken[u] && !taken[v] {
            taken[u] = true;
            taken[v] = true;
        }
    }
    (0..n).filter(|&v| taken[v]).collect()
}

/// Fixed-parameter branching: pick an uncovered edge (u,v); one endpoint must be in the cover. Depth <= k, so 2^k leaves.
fn vc_fpt(edges: &[(usize, usize)], k: usize, nodes: &mut u64) -> bool {
    *nodes += 1;
    let Some(&(u, v)) = edges.first() else {
        return true;
    };
    if k == 0 {
        return false;
    }
    for w in [u, v] {
        let rest: Vec<_> = edges
            .iter()
            .cloned()
            .filter(|&(a, b)| a != w && b != w)
            .collect();
        if vc_fpt(&rest, k - 1, nodes) {
            return true;
        }
    }
    false
}

fn vc_brute(edges: &[(usize, usize)], n: usize) -> usize {
    (0u32..1 << n)
        .filter(|&m| {
            edges
                .iter()
                .all(|&(u, v)| m >> u & 1 == 1 || m >> v & 1 == 1)
        })
        .map(|m| m.count_ones() as usize)
        .min()
        .unwrap()
}

// ================================================================ 8. CSP with forward checking
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Plain,
    Forward,
    ForwardMrv,
}

/// 3-colour a graph (domains are bitmasks). Returns (assignment, nodes).
fn csp_colour(adj: &[Vec<usize>], colours: u32, mode: Mode) -> (Option<Vec<usize>>, u64) {
    fn go(
        adj: &[Vec<usize>],
        dom: &mut Vec<u32>,
        col: &mut Vec<i32>,
        mode: Mode,
        nodes: &mut u64,
    ) -> bool {
        *nodes += 1;
        let n = adj.len();
        let unc: Vec<usize> = (0..n).filter(|&v| col[v] < 0).collect();
        if unc.is_empty() {
            return true;
        }
        let v = if mode == Mode::ForwardMrv {
            *unc.iter()
                .min_by_key(|&&v| (dom[v].count_ones(), usize::MAX - adj[v].len()))
                .unwrap()
        } else {
            unc[0]
        };
        for c in 0..32 {
            if dom[v] >> c & 1 == 0 {
                continue;
            }
            if mode == Mode::Plain && adj[v].iter().any(|&w| col[w] == c) {
                continue; // plain: check consistency only against assigned neighbours
            }
            col[v] = c;
            let saved = dom.clone();
            let mut wipeout = false;
            if mode != Mode::Plain {
                for &w in &adj[v] {
                    if col[w] < 0 {
                        dom[w] &= !(1 << c);
                        wipeout |= dom[w] == 0; // forward check: a neighbour has no value left
                    }
                }
            }
            if !wipeout && go(adj, dom, col, mode, nodes) {
                return true;
            }
            *dom = saved;
            col[v] = -1;
        }
        false
    }
    let n = adj.len();
    let (mut dom, mut col, mut nodes) = (vec![(1u32 << colours) - 1; n], vec![-1i32; n], 0);
    let ok = go(adj, &mut dom, &mut col, mode, &mut nodes);
    (ok.then(|| col.iter().map(|&c| c as usize).collect()), nodes)
}

fn main() {
    // ---------------- 1. framework
    let mut st = Stats::default();
    let mut q = Queens { n: 8, cols: vec![] };
    search(&mut q, &mut st, &mut |_| false);
    assert_eq!(st.solutions, 92);
    let mut st1 = Stats::default();
    let mut q2 = Queens { n: 8, cols: vec![] };
    assert!(search_first(&mut q2, &mut st1));
    println!(
        "[framework] 8-queens: {} solutions in {} nodes (recursive); first solution {:?} after {} nodes (explicit stack)",
        st.solutions, st.nodes, q2.cols, st1.nodes
    );
    for (n, want) in [(1, 1), (2, 0), (3, 0), (4, 2), (5, 10), (6, 4), (7, 40)] {
        let mut s = Stats::default();
        search(&mut Queens { n, cols: vec![] }, &mut s, &mut |_| false);
        assert_eq!(s.solutions, want);
    }
    let mut stop = Stats::default();
    let mut found = 0;
    search(&mut Queens { n: 8, cols: vec![] }, &mut stop, &mut |_| {
        found += 1;
        found == 3
    });
    assert_eq!(stop.solutions, 3, "visitor can stop early");

    // ---------------- 2. colouring
    let mut rng = Rng(0x00c0_ffee_1234_5678);
    let (mut naive_nodes, mut dsatur_nodes) = (0, 0);
    for _ in 0..8 {
        let g = random_graph(&mut rng, 18, 0.5);
        let (k1, n1) = chromatic(&g, false);
        let (k2, n2) = chromatic(&g, true);
        assert_eq!(k1, k2, "same chromatic number");
        naive_nodes += n1;
        dsatur_nodes += n2;
    }
    println!(
        "[colouring] 8 random G(18,0.5): naive order {naive_nodes} nodes, DSATUR {dsatur_nodes} nodes"
    );
    let cycle5: Vec<Vec<usize>> = (0..5).map(|i| vec![(i + 1) % 5, (i + 4) % 5]).collect();
    assert_eq!(chromatic(&cycle5, true).0, 3, "odd cycle needs 3");

    // ---------------- 3. subset sum
    let items: Vec<i64> = (0..20).map(|_| 1 + rng.below(1_000_000) as i64).collect();
    for t in 0..40 {
        let target = if t % 2 == 0 {
            items.iter().filter(|_| rng.chance(0.5)).sum::<i64>()
        } else {
            rng.below(10_000_000) as i64
        };
        let m = subset_sum_mitm(&items, target);
        assert_eq!(m.is_some(), subset_sum_brute(&items, target));
        if let Some(mask) = m {
            let s: i64 = (0..20)
                .filter(|&i| mask >> i & 1 == 1)
                .map(|i| items[i])
                .sum();
            assert_eq!(s, target);
        }
    }
    let big: Vec<i64> = (0..40).map(|_| 1 + rng.below(1 << 40) as i64).collect();
    let target: i64 = big.iter().step_by(3).sum();
    let mask = subset_sum_mitm(&big, target).expect("planted subset found");
    assert_eq!(
        (0..40)
            .filter(|&i| mask >> i & 1 == 1)
            .map(|i| big[i])
            .sum::<i64>(),
        target
    );
    println!(
        "[subset sum] n=40: meet-in-the-middle touches 2*2^20 = {} sums; brute force would need 2^40 = {}",
        2 << 20,
        1u64 << 40
    );

    // ---------------- 4. knapsack
    let (mut with_b, mut without_b) = (0, 0);
    for _ in 0..5 {
        let n = 22;
        let w: Vec<usize> = (0..n).map(|_| 5 + rng.below(40) as usize).collect();
        let v: Vec<u64> = w.iter().map(|&x| x as u64 * 3 + rng.below(30)).collect();
        let cap = w.iter().sum::<usize>() / 2;
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| (v[b] * w[a] as u64).cmp(&(v[a] * w[b] as u64))); // best ratio first
        let (ws, vs): (Vec<usize>, Vec<u64>) = idx.iter().map(|&i| (w[i], v[i])).unzip();
        let opt = knapsack_dp(&w, &v, cap);
        let mut a = Knap {
            w: &ws,
            v: &vs,
            cap,
            best: 0,
            nodes: 0,
            use_bound: true,
        };
        a.go(0, 0, 0);
        let mut b = Knap {
            w: &ws,
            v: &vs,
            cap,
            best: 0,
            nodes: 0,
            use_bound: false,
        };
        b.go(0, 0, 0);
        assert_eq!((a.best, b.best), (opt, opt));
        with_b += a.nodes;
        without_b += b.nodes;
    }
    println!(
        "[knapsack] 5 instances n=22: bound pruning {with_b} nodes vs capacity-only {without_b} nodes"
    );

    // ---------------- 5. TSP
    let (mut tsp_b, mut tsp_n) = (0, 0);
    for _ in 0..3 {
        let n = 11;
        let mut d = vec![vec![0u32; n]; n];
        for i in 0..n {
            for j in i + 1..n {
                d[i][j] = 1 + rng.below(99) as u32;
                d[j][i] = d[i][j];
            }
        }
        let hk = held_karp(&d);
        let mut a = Tsp {
            d: &d,
            best: u32::MAX,
            nodes: 0,
            use_bound: true,
        };
        a.go(0, 1, 0);
        let mut b = Tsp {
            d: &d,
            best: u32::MAX,
            nodes: 0,
            use_bound: false,
        };
        b.go(0, 1, 0);
        assert_eq!((a.best, b.best), (hk, hk));
        tsp_b += a.nodes;
        tsp_n += b.nodes;
    }
    println!(
        "[tsp] 3 instances n=11: MST bound {tsp_b} nodes vs cost-only pruning {tsp_n} nodes (Held-Karp agrees)"
    );

    // ---------------- 6. clique
    let (mut calls_p, mut calls_n) = (0, 0);
    for t in 0..200 {
        let p = [0.3, 0.5, 0.7, 0.9][t % 4];
        let g = mask_graph(&mut rng, 14, p);
        let (mut b1, mut b2) = (0, 0);
        let all = (1u32 << 14) - 1;
        bk(&g, 0, all, 0, true, &mut b1, &mut calls_p);
        bk(&g, 0, all, 0, false, &mut b2, &mut calls_n);
        let brute = clique_brute(&g);
        assert_eq!((b1, b2), (brute, brute));
    }
    println!(
        "[clique] 200 random graphs n=14: Bron-Kerbosch pivot {calls_p} calls vs no pivot {calls_n} calls"
    );

    // ---------------- 7. vertex cover
    let (mut fpt_nodes, mut full) = (0u64, 0u64);
    for t in 0..60 {
        let g = mask_graph(&mut rng, 12, [0.15, 0.3, 0.5][t % 3]);
        let e = edges_of(&g);
        let opt = vc_brute(&e, 12);
        let approx = vc_two_approx(&e, 12);
        assert!(
            e.iter()
                .all(|&(u, v)| approx.contains(&u) || approx.contains(&v))
        );
        assert!(approx.len() <= 2 * opt);
        let k = (0..).find(|&k| vc_fpt(&e, k, &mut 0)).unwrap();
        assert_eq!(k, opt, "FPT branching is optimal");
        let mut nodes = 0;
        vc_fpt(&e, opt, &mut nodes);
        fpt_nodes += nodes;
        full += (1u64 << (opt + 1)) - 1;
    }
    println!(
        "[vertex cover] 60 graphs n=12: FPT nodes at k=opt {fpt_nodes} (worst-case bound 2^(k+1)-1 summed = {full}); 2-approx within factor 2"
    );

    // ---------------- 8. CSP
    let australia: Vec<Vec<usize>> = vec![
        vec![1, 2],          // WA
        vec![0, 2, 3],       // NT
        vec![0, 1, 3, 4, 5], // SA
        vec![1, 2, 4],       // Q
        vec![2, 3, 5],       // NSW
        vec![2, 4],          // V
        vec![],              // T
    ];
    let mut grid: Vec<Vec<usize>> = vec![vec![]; 36];
    for r in 0..6 {
        for c in 0..6 {
            for (dr, dc) in [(0, 1), (1, 0), (1, 1)] {
                if r + dr < 6 && c + dc < 6 {
                    let (a, b) = (r * 6 + c, (r + dr) * 6 + c + dc);
                    grid[a].push(b);
                    grid[b].push(a);
                }
            }
        }
    }
    let k4: Vec<Vec<usize>> = (0..4)
        .map(|i| (0..4).filter(|&j| j != i).collect())
        .collect();
    for (name, g, expect_sat) in [
        ("Australia", &australia, true),
        ("triangulated 6x6 grid", &grid, true),
        ("K4 with 3 colours", &k4, false),
    ] {
        let mut line = format!("[csp] {name:22}");
        let mut verdicts = vec![];
        for (label, mode) in [
            ("plain", Mode::Plain),
            ("forward", Mode::Forward),
            ("fc+mrv", Mode::ForwardMrv),
        ] {
            let (sol, nodes) = csp_colour(g, 3, mode);
            if let Some(c) = &sol {
                assert!((0..g.len()).all(|v| g[v].iter().all(|&w| c[w] != c[v])));
            }
            verdicts.push(sol.is_some());
            line += &format!(" {label}={nodes}");
        }
        assert!(verdicts.iter().all(|&s| s == expect_sat));
        println!("{line}");
    }
    println!("all searches verified against oracles");
}
