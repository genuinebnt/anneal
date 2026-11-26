//! RECIPE 278 (algorithms): problem modeling by reduction (Skiena): puzzles as shortest paths, 2-SAT, assignment, bipartite, scheduling, Steiner, Kadane, sorting.
//! LEVEL: advanced
//! Use in: turning a real problem you have never seen into a classic one you already own; interview and contest modeling.   Run: cargo run --release --example problem_modeling_reductions
#![allow(dead_code)]
//!
//! WHAT: Skiena's central advice (The Algorithm Design Manual, ch. 10 "modeling" and the catalog): you rarely invent an
//! algorithm; you RECOGNISE the problem as a known one in disguise and reduce to it. Each section below states the
//! reduction, solves it with a small re-implemented classic, and checks against an independent oracle or known answer.
//!  (a) PUZZLES AS SHORTEST PATHS: states are nodes, legal moves are edges. Water jugs and the 8-puzzle by BFS (the whole
//!      8-puzzle graph: 181440 states, diameter 31); the 15-puzzle by IDA* with the Manhattan-distance heuristic on
//!      instances scrambled by 25 random moves (solutions replayed and validated; BFS and IDA* agree on 8-puzzle).
//!  (b) 2-SAT -> implication graph -> strongly connected components (iterative Tarjan; satisfiable iff no x shares a
//!      component with not-x). Verified against brute force on 1000 random formulas.
//!  (c) ASSIGNMENT -> min-cost bipartite matching: the Hungarian algorithm (O(n^3), potentials) AND min-cost flow by
//!      successive shortest paths; both equal brute force over all permutations for n <= 8.
//!  (d) BIPARTITENESS by BFS 2-colouring, with an ODD CYCLE as the certificate when it fails (a no-answer you can prove).
//!  (e) SCHEDULING: weighted interval scheduling (sort by end, DP + binary search) and room allocation (sweep + heap =
//!      minimum rooms = maximum overlap = interval-graph colouring).
//!  (f) STEINER TREE: the metric-closure MST heuristic (2-approximation, bound 2(1 - 1/t)) against exact Dreyfus-Wagner DP.
//!  (g) MAXIMUM SUBARRAY (Kadane) and its 2-D version by REDUCTION: fix a pair of rows, collapse columns, run Kadane.
//!  (h) REDUCE TO SORTING: element uniqueness, closest pair on a line, anagram groups.
//!  Finally a mini catalog: "if you see X, think Y".
//!
//! COMMON USES: sliding-puzzle and planning solvers (A*/IDA* over state graphs); 2-SAT in constraint solving, layout
//! and configuration problems, and as the polynomial base case inside SAT solvers; the assignment problem for matching
//! workers to jobs, tracking (matching detections between video frames) and resource allocation; bipartite tests in graph
//! colouring and scheduling conflicts; interval scheduling and room allocation in calendars and register allocation on
//! straight-line code; Steiner trees in network and VLSI routing; Kadane in signal and time-series analysis and the 2-D
//! form in image processing.
//! Read: The Algorithm Design Manual (Skiena), chapter 10 "How to Design Algorithms" / "Modeling", the war stories, and the catalog of algorithmic problems (Part II)
//! Read: The Art of Computer Programming Vol 3 (Sorting and Searching), 5 "Sorting" (everything that sorts as a subroutine) and Vol 4A for puzzles and backtracking
//! Read: https://en.wikipedia.org/wiki/2-satisfiability
//! Read: https://en.wikipedia.org/wiki/Hungarian_algorithm
//! Read: https://en.wikipedia.org/wiki/Steiner_tree_problem
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

// ================================================================= (a) puzzles as shortest paths
/// Water jugs: min steps to get `target` litres in either jug, or None.
fn water_jug(ca: usize, cb: usize, target: usize) -> Option<(usize, Vec<(usize, usize)>)> {
    let mut parent: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
    let mut q = VecDeque::from([(0usize, 0usize)]);
    parent.insert((0, 0), (0, 0));
    while let Some((a, b)) = q.pop_front() {
        if a == target || b == target {
            let mut path = vec![(a, b)];
            let mut cur = (a, b);
            while cur != (0, 0) {
                cur = parent[&cur];
                path.push(cur);
            }
            path.reverse();
            return Some((path.len() - 1, path));
        }
        let pour_ab = a.min(cb - b);
        let pour_ba = b.min(ca - a);
        for next in [
            (ca, b),
            (a, cb),
            (0, b),
            (a, 0),
            (a - pour_ab, b + pour_ab),
            (a + pour_ba, b - pour_ba),
        ] {
            if !parent.contains_key(&next) {
                parent.insert(next, (a, b));
                q.push_back(next);
            }
        }
    }
    None
}

const DR: [i32; 4] = [-1, 1, 0, 0];
const DC: [i32; 4] = [0, 0, -1, 1];
fn goal_board(n: usize) -> Vec<u8> {
    (1..=(n * n) as u8)
        .map(|t| if t as usize == n * n { 0 } else { t })
        .collect()
}
/// Position the blank would reach moving in direction d, if legal.
fn blank_move(blank: usize, d: usize, n: usize) -> Option<usize> {
    let (r, c) = ((blank / n) as i32 + DR[d], (blank % n) as i32 + DC[d]);
    if r < 0 || c < 0 || r >= n as i32 || c >= n as i32 {
        None
    } else {
        Some(r as usize * n + c as usize)
    }
}
fn scramble(n: usize, moves: usize, rng: &mut Rng) -> Vec<u8> {
    let mut b = goal_board(n);
    let (mut blank, mut last) = (n * n - 1, 4usize);
    for _ in 0..moves {
        loop {
            let d = rng.below(4) as usize;
            if last < 4 && d == last ^ 1 {
                continue;
            }
            if let Some(nb) = blank_move(blank, d, n) {
                b.swap(blank, nb);
                blank = nb;
                last = d;
                break;
            }
        }
    }
    b
}
fn encode(b: &[u8]) -> u64 {
    b.iter().fold(0, |k, &t| k << 4 | t as u64)
}
/// BFS over the entire 8-puzzle graph from the goal: distance of every reachable state.
fn bfs_table_8() -> HashMap<u64, u8> {
    let mut dist = HashMap::new();
    let g = goal_board(3);
    dist.insert(encode(&g), 0u8);
    let mut q = VecDeque::from([g]);
    while let Some(b) = q.pop_front() {
        let d = dist[&encode(&b)];
        let blank = b.iter().position(|&t| t == 0).unwrap();
        for dir in 0..4 {
            if let Some(nb) = blank_move(blank, dir, 3) {
                let mut nbd = b.clone();
                nbd.swap(blank, nb);
                if !dist.contains_key(&encode(&nbd)) {
                    dist.insert(encode(&nbd), d + 1);
                    q.push_back(nbd);
                }
            }
        }
    }
    dist
}
fn manhattan(tile: u8, pos: usize, n: usize) -> usize {
    let g = tile as usize - 1;
    (pos / n).abs_diff(g / n) + (pos % n).abs_diff(g % n)
}
fn total_manhattan(b: &[u8], n: usize) -> usize {
    b.iter()
        .enumerate()
        .filter(|&(_, &t)| t != 0)
        .map(|(p, &t)| manhattan(t, p, n))
        .sum()
}
struct Ida {
    n: usize,
    path: Vec<usize>,
    nodes: u64,
}
impl Ida {
    /// Returns (found, smallest f that exceeded the bound).
    fn dfs(
        &mut self,
        b: &mut Vec<u8>,
        blank: usize,
        g: usize,
        h: usize,
        bound: usize,
        last: usize,
    ) -> (bool, usize) {
        let f = g + h;
        if f > bound {
            return (false, f);
        }
        if h == 0 {
            return (true, f);
        }
        self.nodes += 1;
        let mut next_bound = usize::MAX;
        for d in 0..4 {
            if last < 4 && d == last ^ 1 {
                continue;
            }
            let Some(nb) = blank_move(blank, d, self.n) else {
                continue;
            };
            let tile = b[nb];
            let nh = h + manhattan(tile, blank, self.n) - manhattan(tile, nb, self.n);
            b.swap(blank, nb);
            self.path.push(d);
            let (found, t) = self.dfs(b, nb, g + 1, nh, bound, d);
            if found {
                return (true, t);
            }
            self.path.pop();
            b.swap(blank, nb);
            next_bound = next_bound.min(t);
        }
        (false, next_bound)
    }
}
/// Optimal solution (as blank directions) by iterative-deepening A* with the Manhattan heuristic.
fn ida_star(start: &[u8], n: usize) -> (Vec<usize>, u64) {
    let mut b = start.to_vec();
    let blank = b.iter().position(|&t| t == 0).unwrap();
    let mut ida = Ida {
        n,
        path: vec![],
        nodes: 0,
    };
    let mut bound = total_manhattan(&b, n);
    loop {
        let (found, next) = ida.dfs(&mut b, blank, 0, total_manhattan(start, n), bound, 4);
        if found {
            return (ida.path, ida.nodes);
        }
        bound = next;
    }
}
fn replay(start: &[u8], moves: &[usize], n: usize) -> Vec<u8> {
    let mut b = start.to_vec();
    let mut blank = b.iter().position(|&t| t == 0).unwrap();
    for &d in moves {
        let nb = blank_move(blank, d, n).expect("illegal move in solution");
        b.swap(blank, nb);
        blank = nb;
    }
    b
}

// ================================================================= (b) 2-SAT
/// Iterative Tarjan (minimal copy of recipe tarjan_scc); returns comp[v], numbered in emission (reverse topological) order.
fn tarjan_comp(adj: &[Vec<usize>]) -> Vec<usize> {
    const UN: usize = usize::MAX;
    let n = adj.len();
    let (mut index, mut low, mut comp) = (vec![UN; n], vec![0; n], vec![UN; n]);
    let (mut stack, mut next, mut ncomp) = (vec![], 0, 0);
    for root in 0..n {
        if index[root] != UN {
            continue;
        }
        let mut call = vec![(root, 0usize)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        while let Some(&(v, ei)) = call.last() {
            if ei < adj[v].len() {
                call.last_mut().unwrap().1 += 1;
                let w = adj[v][ei];
                if index[w] == UN {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    call.push((w, 0));
                } else if comp[w] == UN {
                    low[v] = low[v].min(index[w]); // w is still on the stack
                }
            } else {
                call.pop();
                if let Some(&(p, _)) = call.last() {
                    low[p] = low[p].min(low[v]);
                }
                if low[v] == index[v] {
                    while let Some(w) = stack.pop() {
                        comp[w] = ncomp;
                        if w == v {
                            break;
                        }
                    }
                    ncomp += 1;
                }
            }
        }
    }
    comp
}
/// Literal encoding: variable v true = 2v, false = 2v+1. Clause (a OR b).
fn two_sat(nvars: usize, clauses: &[(usize, usize)]) -> Option<Vec<bool>> {
    let mut adj = vec![vec![]; 2 * nvars];
    for &(a, b) in clauses {
        adj[a ^ 1].push(b); // not a -> b
        adj[b ^ 1].push(a); // not b -> a
    }
    let comp = tarjan_comp(&adj);
    let mut val = vec![false; nvars];
    for v in 0..nvars {
        if comp[2 * v] == comp[2 * v + 1] {
            return None;
        }
        val[v] = comp[2 * v] < comp[2 * v + 1]; // the literal later in topological order is made true
    }
    Some(val)
}
fn lit_true(l: usize, a: &[bool]) -> bool {
    a[l / 2] == (l % 2 == 0)
}

// ================================================================= (c) assignment
/// Hungarian algorithm (potentials), square cost matrix. Returns (cost, column for each row).
fn hungarian(a: &[Vec<i64>]) -> (i64, Vec<usize>) {
    let n = a.len();
    const INF: i64 = i64::MAX / 4;
    let (mut u, mut v, mut p, mut way) = (
        vec![0i64; n + 1],
        vec![0i64; n + 1],
        vec![0usize; n + 1],
        vec![0usize; n + 1],
    );
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0;
        let (mut minv, mut used) = (vec![INF; n + 1], vec![false; n + 1]);
        loop {
            used[j0] = true;
            let (i0, mut delta, mut j1) = (p[j0], INF, 0);
            for j in 1..=n {
                if !used[j] {
                    let cur = a[i0 - 1][j - 1] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=n {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut col = vec![0; n];
    for j in 1..=n {
        col[p[j] - 1] = j - 1;
    }
    (col.iter().enumerate().map(|(i, &j)| a[i][j]).sum(), col)
}
struct Mcmf {
    to: Vec<usize>,
    cap: Vec<i64>,
    cost: Vec<i64>,
    adj: Vec<Vec<usize>>,
}
impl Mcmf {
    fn new(n: usize) -> Self {
        Mcmf {
            to: vec![],
            cap: vec![],
            cost: vec![],
            adj: vec![vec![]; n],
        }
    }
    fn add(&mut self, a: usize, b: usize, cap: i64, cost: i64) {
        for (f, t, c, w) in [(a, b, cap, cost), (b, a, 0, -cost)] {
            self.adj[f].push(self.to.len());
            self.to.push(t);
            self.cap.push(c);
            self.cost.push(w);
        }
    }
    /// Successive shortest paths with Bellman-Ford (SPFA): returns (flow, cost).
    fn run(&mut self, s: usize, t: usize) -> (i64, i64) {
        let n = self.adj.len();
        let (mut flow, mut total) = (0, 0);
        loop {
            let (mut dist, mut prev_e) = (vec![i64::MAX; n], vec![usize::MAX; n]);
            let (mut inq, mut q) = (vec![false; n], VecDeque::from([s]));
            dist[s] = 0;
            while let Some(u) = q.pop_front() {
                inq[u] = false;
                for &e in &self.adj[u] {
                    let v = self.to[e];
                    if self.cap[e] > 0 && dist[u] + self.cost[e] < dist[v] {
                        dist[v] = dist[u] + self.cost[e];
                        prev_e[v] = e;
                        if !inq[v] {
                            inq[v] = true;
                            q.push_back(v);
                        }
                    }
                }
            }
            if dist[t] == i64::MAX {
                return (flow, total);
            }
            let (mut v, mut push) = (t, i64::MAX);
            while v != s {
                let e = prev_e[v];
                push = push.min(self.cap[e]);
                v = self.to[e ^ 1];
            }
            v = t;
            while v != s {
                let e = prev_e[v];
                self.cap[e] -= push;
                self.cap[e ^ 1] += push;
                v = self.to[e ^ 1];
            }
            flow += push;
            total += push * dist[t];
        }
    }
}
fn assign_by_flow(a: &[Vec<i64>]) -> i64 {
    let n = a.len();
    let mut g = Mcmf::new(2 * n + 2);
    let (s, t) = (2 * n, 2 * n + 1);
    for i in 0..n {
        g.add(s, i, 1, 0);
        g.add(n + i, t, 1, 0);
        for j in 0..n {
            g.add(i, n + j, 1, a[i][j]);
        }
    }
    let (flow, cost) = g.run(s, t);
    assert_eq!(flow, n as i64);
    cost
}
fn assign_brute(a: &[Vec<i64>]) -> i64 {
    fn go(a: &[Vec<i64>], row: usize, used: &mut Vec<bool>, acc: i64, best: &mut i64) {
        if row == a.len() {
            *best = (*best).min(acc);
            return;
        }
        for j in 0..a.len() {
            if !used[j] {
                used[j] = true;
                go(a, row + 1, used, acc + a[row][j], best);
                used[j] = false;
            }
        }
    }
    let mut best = i64::MAX;
    go(a, 0, &mut vec![false; a.len()], 0, &mut best);
    best
}

// ================================================================= (d) bipartite + odd cycle certificate
enum Bip {
    Colors(Vec<u8>),
    OddCycle(Vec<usize>),
}
fn bipartite(adj: &[Vec<usize>]) -> Bip {
    let n = adj.len();
    let (mut color, mut parent, mut depth) = (vec![2u8; n], vec![usize::MAX; n], vec![0usize; n]);
    for s in 0..n {
        if color[s] != 2 {
            continue;
        }
        color[s] = 0;
        let mut q = VecDeque::from([s]);
        while let Some(u) = q.pop_front() {
            for &v in &adj[u] {
                if color[v] == 2 {
                    color[v] = 1 - color[u];
                    parent[v] = u;
                    depth[v] = depth[u] + 1;
                    q.push_back(v);
                } else if color[v] == color[u] {
                    // same colour across an edge: climb both to the LCA; tree path + this edge is an odd cycle
                    let (mut a, mut b) = (u, v);
                    let (mut left, mut right) = (vec![a], vec![b]);
                    while a != b {
                        if depth[a] >= depth[b] {
                            a = parent[a];
                            left.push(a);
                        } else {
                            b = parent[b];
                            right.push(b);
                        }
                    }
                    right.pop(); // lca already in `left`
                    right.reverse();
                    left.extend(right);
                    return Bip::OddCycle(left);
                }
            }
        }
    }
    Bip::Colors(color)
}
fn brute_bipartite(n: usize, edges: &[(usize, usize)]) -> bool {
    (0u32..1 << n).any(|m| edges.iter().all(|&(a, b)| (m >> a & 1) != (m >> b & 1)))
}

// ================================================================= (e) scheduling
/// Intervals (start, end, weight), compatible when end <= start of the next. Maximum total weight.
fn weighted_intervals(iv: &[(u32, u32, u64)]) -> u64 {
    let mut v = iv.to_vec();
    v.sort_by_key(|x| x.1);
    let ends: Vec<u32> = v.iter().map(|x| x.1).collect();
    let mut dp = vec![0u64; v.len() + 1];
    for i in 0..v.len() {
        let p = ends.partition_point(|&e| e <= v[i].0); // intervals 0..p end before this one starts
        dp[i + 1] = dp[i].max(v[i].2 + dp[p.min(i)]);
    }
    dp[v.len()]
}
fn brute_intervals(iv: &[(u32, u32, u64)]) -> u64 {
    let mut best = 0;
    for m in 0u32..1 << iv.len() {
        let pick: Vec<_> = (0..iv.len())
            .filter(|&i| m >> i & 1 == 1)
            .map(|i| iv[i])
            .collect();
        let ok = pick
            .iter()
            .enumerate()
            .all(|(i, a)| pick[i + 1..].iter().all(|b| a.1 <= b.0 || b.1 <= a.0));
        if ok {
            best = best.max(pick.iter().map(|x| x.2).sum());
        }
    }
    best
}
/// Greedy room allocation: rooms[i] for each interval, and the room count.
fn allocate_rooms(iv: &[(u32, u32)]) -> (Vec<usize>, usize) {
    let mut order: Vec<usize> = (0..iv.len()).collect();
    order.sort_by_key(|&i| iv[i].0);
    let (mut heap, mut room, mut count) = (BinaryHeap::new(), vec![0; iv.len()], 0);
    for i in order {
        let r = match heap.peek() {
            Some(&Reverse((end, r))) if end <= iv[i].0 => {
                heap.pop();
                r
            }
            _ => {
                count += 1;
                count - 1
            }
        };
        room[i] = r;
        heap.push(Reverse((iv[i].1, r)));
    }
    (room, count)
}

// ================================================================= (f) Steiner tree
const INF: u64 = u64::MAX / 4;
fn floyd(n: usize, edges: &[(usize, usize, u64)]) -> Vec<Vec<u64>> {
    let mut d = vec![vec![INF; n]; n];
    for i in 0..n {
        d[i][i] = 0;
    }
    for &(a, b, w) in edges {
        d[a][b] = d[a][b].min(w);
        d[b][a] = d[b][a].min(w);
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                d[i][j] = d[i][j].min(d[i][k] + d[k][j]);
            }
        }
    }
    d
}
/// Exact minimum Steiner tree cost by Dreyfus-Wagner on the metric closure: O(3^t n + 2^t n^2).
fn steiner_exact(d: &[Vec<u64>], term: &[usize]) -> u64 {
    let (n, t) = (d.len(), term.len());
    let mut dp = vec![vec![INF; n]; 1 << t];
    for (i, &ti) in term.iter().enumerate() {
        dp[1 << i] = d[ti].clone();
    }
    for mask in 1usize..1 << t {
        if mask.count_ones() < 2 {
            continue;
        }
        let mut comb = vec![INF; n];
        let mut sub = (mask - 1) & mask;
        while sub > 0 {
            for v in 0..n {
                comb[v] = comb[v].min(dp[sub][v] + dp[mask ^ sub][v]);
            }
            sub = (sub - 1) & mask;
        }
        for v in 0..n {
            dp[mask][v] = (0..n).map(|u| comb[u] + d[u][v]).min().unwrap();
        }
    }
    dp[(1 << t) - 1][term[0]]
}
/// 2-approximation: MST (Prim) over the terminals under the shortest-path metric.
fn steiner_mst_heuristic(d: &[Vec<u64>], term: &[usize]) -> u64 {
    let t = term.len();
    let (mut in_tree, mut key, mut total) = (vec![false; t], vec![INF; t], 0);
    key[0] = 0;
    for _ in 0..t {
        let u = (0..t)
            .filter(|&i| !in_tree[i])
            .min_by_key(|&i| key[i])
            .unwrap();
        in_tree[u] = true;
        total += key[u];
        for v in 0..t {
            if !in_tree[v] {
                key[v] = key[v].min(d[term[u]][term[v]]);
            }
        }
    }
    total
}

// ================================================================= (g) Kadane
/// Maximum non-empty subarray: (sum, start, end inclusive).
fn kadane(a: &[i64]) -> (i64, usize, usize) {
    let (mut best, mut cur, mut start) = ((a[0], 0, 0), a[0], 0);
    for i in 1..a.len() {
        if cur < 0 {
            cur = a[i];
            start = i;
        } else {
            cur += a[i];
        }
        if cur > best.0 {
            best = (cur, start, i);
        }
    }
    best
}
/// Maximum-sum rectangle: fix top and bottom rows, collapse each column to a sum, and the rest is 1-D Kadane.
fn max_submatrix(m: &[Vec<i64>]) -> i64 {
    let (r, c) = (m.len(), m[0].len());
    let mut best = i64::MIN;
    for top in 0..r {
        let mut col = vec![0i64; c];
        for bottom in top..r {
            for j in 0..c {
                col[j] += m[bottom][j];
            }
            best = best.max(kadane(&col).0);
        }
    }
    best
}
fn max_submatrix_brute(m: &[Vec<i64>]) -> i64 {
    let (r, c) = (m.len(), m[0].len());
    let mut pre = vec![vec![0i64; c + 1]; r + 1];
    for i in 0..r {
        for j in 0..c {
            pre[i + 1][j + 1] = m[i][j] + pre[i][j + 1] + pre[i + 1][j] - pre[i][j];
        }
    }
    let mut best = i64::MIN;
    for (i0, i1) in (0..r).flat_map(|a| (a + 1..=r).map(move |b| (a, b))) {
        for (j0, j1) in (0..c).flat_map(|a| (a + 1..=c).map(move |b| (a, b))) {
            best = best.max(pre[i1][j1] - pre[i0][j1] - pre[i1][j0] + pre[i0][j0]);
        }
    }
    best
}

fn norm_groups(mut g: Vec<Vec<&str>>) -> Vec<Vec<&str>> {
    g.iter_mut().for_each(|x| x.sort());
    g.sort();
    g
}

fn main() {
    let mut rng = Rng::new(278);

    // ---- (a) puzzles
    println!("== (a) puzzles as shortest paths");
    let (steps, path) = water_jug(3, 5, 4).unwrap();
    assert_eq!(steps, 6);
    println!("  jugs 3/5 -> 4 litres in {steps} steps: {path:?}");
    for ca in 1..=12 {
        for cb in 1..=12 {
            for t in 0..=12 {
                let reachable = t <= ca.max(cb) && t % gcd(ca, cb) == 0;
                assert_eq!(
                    water_jug(ca, cb, t).is_some(),
                    reachable,
                    "jugs {ca} {cb} target {t}"
                );
            }
        }
    }
    let table = bfs_table_8();
    let diameter = *table.values().max().unwrap();
    println!(
        "  8-puzzle graph: {} reachable states (9!/2 = 181440), diameter {diameter}",
        table.len()
    );
    assert_eq!((table.len(), diameter), (181_440, 31));
    let mut nodes8 = 0;
    for _ in 0..200 {
        let b = scramble(3, 40, &mut rng);
        let (sol, nodes) = ida_star(&b, 3);
        nodes8 += nodes;
        assert_eq!(
            sol.len() as u8,
            table[&encode(&b)],
            "IDA* and BFS must agree"
        );
        assert_eq!(replay(&b, &sol, 3), goal_board(3));
    }
    println!(
        "  IDA*(Manhattan) length == BFS distance on 200 scrambled 8-puzzles ({:.0} nodes each)",
        nodes8 as f64 / 200.0
    );
    let (mut lens, mut nodes15) = (vec![], 0);
    for _ in 0..10 {
        let b = scramble(4, 25, &mut rng);
        let (sol, nodes) = ida_star(&b, 4);
        assert_eq!(replay(&b, &sol, 4), goal_board(4), "invalid solution");
        assert!(
            sol.len() <= 25 && sol.len() % 2 == 25 % 2,
            "parity: each move flips the blank colour"
        );
        lens.push(sol.len());
        nodes15 += nodes;
    }
    println!(
        "  15-puzzle, 10 instances scrambled by 25 moves: optimal lengths {lens:?}, {nodes15} IDA* nodes total"
    );

    // ---- (b) 2-SAT
    let (mut sat, mut unsat) = (0, 0);
    for _ in 0..1000 {
        let nv = 1 + rng.below(8) as usize;
        let m = 1 + rng.below(3 * nv as u64 + 1) as usize;
        let clauses: Vec<(usize, usize)> = (0..m)
            .map(|_| {
                (
                    rng.below(2 * nv as u64) as usize,
                    rng.below(2 * nv as u64) as usize,
                )
            })
            .collect();
        let brute = (0u32..1 << nv).any(|mask| {
            let a: Vec<bool> = (0..nv).map(|i| mask >> i & 1 == 1).collect();
            clauses
                .iter()
                .all(|&(x, y)| lit_true(x, &a) || lit_true(y, &a))
        });
        let got = two_sat(nv, &clauses);
        assert_eq!(got.is_some(), brute, "{nv} vars {clauses:?}");
        if let Some(a) = got {
            assert!(
                clauses
                    .iter()
                    .all(|&(x, y)| lit_true(x, &a) || lit_true(y, &a)),
                "assignment must satisfy"
            );
            sat += 1;
        } else {
            unsat += 1;
        }
    }
    println!(
        "== (b) 2-SAT via implication graph + SCC: 1000 random formulas agree with brute force ({sat} satisfiable, {unsat} not)"
    );
    assert!(sat > 100 && unsat > 100);

    // ---- (c) assignment
    for _ in 0..200 {
        let n = 1 + rng.below(8) as usize;
        let a: Vec<Vec<i64>> = (0..n)
            .map(|_| (0..n).map(|_| rng.below(100) as i64).collect())
            .collect();
        let (cost, col) = hungarian(&a);
        let mut seen = col.clone();
        seen.sort();
        assert_eq!(seen, (0..n).collect::<Vec<_>>(), "must be a permutation");
        assert_eq!(cost, assign_brute(&a));
        assert_eq!(cost, assign_by_flow(&a));
    }
    let big: Vec<Vec<i64>> = (0..150)
        .map(|_| (0..150).map(|_| rng.below(1000) as i64).collect())
        .collect();
    let (hc, _) = hungarian(&big);
    assert_eq!(hc, assign_by_flow(&big));
    println!(
        "== (c) assignment: Hungarian == min-cost flow == brute force on 200 matrices (n <= 8); n = 150: both give {hc}"
    );

    // ---- (d) bipartite
    let (mut yes, mut no) = (0, 0);
    for _ in 0..400 {
        let n = 2 + rng.below(9) as usize;
        let m = rng.below(n as u64 + 4) as usize;
        let mut edges = vec![];
        for _ in 0..m {
            let (a, b) = (rng.below(n as u64) as usize, rng.below(n as u64) as usize);
            if a != b {
                edges.push((a, b));
            }
        }
        let mut adj = vec![vec![]; n];
        for &(a, b) in &edges {
            adj[a].push(b);
            adj[b].push(a);
        }
        let truth = brute_bipartite(n, &edges);
        match bipartite(&adj) {
            Bip::Colors(c) => {
                assert!(truth);
                assert!(edges.iter().all(|&(a, b)| c[a] != c[b]));
                yes += 1;
            }
            Bip::OddCycle(cyc) => {
                assert!(!truth);
                assert!(cyc.len() % 2 == 1 && cyc.len() >= 3, "cycle {cyc:?}");
                let uniq: HashSet<_> = cyc.iter().collect();
                assert_eq!(uniq.len(), cyc.len());
                for i in 0..cyc.len() {
                    assert!(
                        adj[cyc[i]].contains(&cyc[(i + 1) % cyc.len()]),
                        "not a cycle: {cyc:?}"
                    );
                }
                no += 1;
            }
        }
    }
    println!(
        "== (d) bipartite test: 400 random graphs vs brute-force 2-colouring ({yes} bipartite, {no} with a verified odd-cycle certificate)"
    );
    assert!(yes > 20 && no > 20);

    // ---- (e) scheduling
    for _ in 0..300 {
        let n = 1 + rng.below(12) as usize;
        let iv: Vec<(u32, u32, u64)> = (0..n)
            .map(|_| {
                let s = rng.below(30) as u32;
                (s, s + 1 + rng.below(10) as u32, 1 + rng.below(50))
            })
            .collect();
        assert_eq!(weighted_intervals(&iv), brute_intervals(&iv), "{iv:?}");
    }
    let mut max_rooms = 0;
    for _ in 0..300 {
        let n = 1 + rng.below(60) as usize;
        let iv: Vec<(u32, u32)> = (0..n)
            .map(|_| {
                let s = rng.below(100) as u32;
                (s, s + 1 + rng.below(20) as u32)
            })
            .collect();
        let (room, count) = allocate_rooms(&iv);
        let overlap = (0..120u32)
            .map(|t| iv.iter().filter(|x| x.0 <= t && t < x.1).count())
            .max()
            .unwrap();
        assert_eq!(
            count, overlap,
            "rooms = max overlap (interval graphs are perfect)"
        );
        for i in 0..n {
            for j in i + 1..n {
                if room[i] == room[j] {
                    assert!(
                        iv[i].1 <= iv[j].0 || iv[j].1 <= iv[i].0,
                        "double-booked room"
                    );
                }
            }
        }
        max_rooms = max_rooms.max(count);
    }
    println!(
        "== (e) weighted interval scheduling == brute force (300 cases); room allocation == max overlap (300 cases, up to {max_rooms} rooms)"
    );

    // ---- (f) Steiner
    let (mut worst, mut exact_hits) = (1.0f64, 0);
    for _ in 0..150 {
        let n = 14;
        let mut edges = vec![];
        for v in 1..n {
            edges.push((v, rng.below(v as u64) as usize, 1 + rng.below(30))); // random spanning tree keeps it connected
        }
        for _ in 0..10 {
            edges.push((
                rng.below(n as u64) as usize,
                rng.below(n as u64) as usize,
                1 + rng.below(30),
            ));
        }
        let d = floyd(n, &edges);
        let t = 2 + rng.below(7) as usize; // 2..=8 terminals
        let mut term: Vec<usize> = vec![];
        while term.len() < t {
            let c = rng.below(n as u64) as usize;
            if !term.contains(&c) {
                term.push(c);
            }
        }
        let (opt, heur) = (steiner_exact(&d, &term), steiner_mst_heuristic(&d, &term));
        assert!(opt <= heur, "heuristic cannot beat the optimum");
        assert!(
            heur * t as u64 <= 2 * opt * (t as u64 - 1),
            "2(1-1/t) bound violated: heur {heur} opt {opt} t {t}"
        );
        worst = worst.max(heur as f64 / opt as f64);
        exact_hits += (heur == opt) as u32;
    }
    println!(
        "== (f) Steiner: MST-on-metric-closure within 2(1-1/t) of Dreyfus-Wagner on 150 graphs; worst ratio {worst:.3}, optimal in {exact_hits}/150"
    );

    // ---- (g) Kadane
    assert_eq!(kadane(&[-2, 1, -3, 4, -1, 2, 1, -5, 4]), (6, 3, 6));
    assert_eq!(kadane(&[-3, -1, -2]), (-1, 1, 1));
    for _ in 0..300 {
        let n = 1 + rng.below(30) as usize;
        let a: Vec<i64> = (0..n).map(|_| rng.below(41) as i64 - 20).collect();
        let (s, l, r) = kadane(&a);
        let brute = (0..n)
            .flat_map(|i| (i..n).map(move |j| (i, j)))
            .map(|(i, j)| a[i..=j].iter().sum::<i64>())
            .max()
            .unwrap();
        assert_eq!(s, brute);
        assert_eq!(a[l..=r].iter().sum::<i64>(), s);
    }
    for _ in 0..200 {
        let (r, c) = (1 + rng.below(8) as usize, 1 + rng.below(8) as usize);
        let m: Vec<Vec<i64>> = (0..r)
            .map(|_| (0..c).map(|_| rng.below(41) as i64 - 20).collect())
            .collect();
        assert_eq!(max_submatrix(&m), max_submatrix_brute(&m));
    }
    println!(
        "== (g) Kadane == O(n^2) brute force; 2-D maximum rectangle (rows pair + Kadane) == O(r^2 c^2) brute force on 200 matrices"
    );

    // ---- (h) reduce to sorting
    for _ in 0..300 {
        let n = rng.below(30) as usize;
        let a: Vec<u32> = (0..n).map(|_| rng.below(40) as u32).collect();
        let mut s = a.clone();
        s.sort();
        let dup_sorted = s.windows(2).any(|w| w[0] == w[1]);
        let dup_hash = a.iter().collect::<HashSet<_>>().len() != a.len();
        assert_eq!(dup_sorted, dup_hash);
        if n >= 2 {
            let closest = s.windows(2).map(|w| w[1] - w[0]).min().unwrap(); // closest pair on a line = adjacent after sorting
            let brute = (0..n)
                .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
                .map(|(i, j)| a[i].abs_diff(a[j]))
                .min()
                .unwrap();
            assert_eq!(closest, brute);
        }
    }
    let words: Vec<String> = (0..400)
        .map(|_| {
            (0..3 + rng.below(3))
                .map(|_| (b'a' + rng.below(4) as u8) as char)
                .collect()
        })
        .collect();
    let mut by_sorted: HashMap<Vec<u8>, Vec<&str>> = HashMap::new();
    for w in &words {
        let mut k = w.clone().into_bytes();
        k.sort();
        by_sorted.entry(k).or_default().push(w);
    }
    let mut by_counts: HashMap<[u8; 26], Vec<&str>> = HashMap::new();
    for w in &words {
        let mut c = [0u8; 26];
        for b in w.bytes() {
            c[(b - b'a') as usize] += 1;
        }
        by_counts.entry(c).or_default().push(w);
    }

    let (g1, g2) = (
        norm_groups(by_sorted.into_values().collect()),
        norm_groups(by_counts.into_values().collect()),
    );
    assert_eq!(g1, g2);
    println!(
        "== (h) reduce to sorting: uniqueness, closest pair on a line, {} anagram groups from 400 words (sorted-letters key == count-signature key)",
        g1.len()
    );

    println!("\n  mini catalog: if you see ...                           think ...");
    for (p, c) in [
        (
            "fewest moves / steps in a puzzle or game",
            "BFS on the state graph (IDA*/A* with a heuristic if it is huge)",
        ),
        (
            "either/or constraints, x OR y, 'at most one of'",
            "2-SAT -> SCC (3+ literals per clause: NP-hard, use a SAT solver)",
        ),
        (
            "match n workers to n jobs, minimise total cost",
            "assignment -> Hungarian / min-cost flow",
        ),
        (
            "max matching, no weights",
            "bipartite -> max flow / Hopcroft-Karp; König: = min vertex cover",
        ),
        (
            "split into two groups, no conflict inside a group",
            "bipartite test (odd cycle = proof it is impossible)",
        ),
        (
            "pick non-overlapping intervals, maximise weight",
            "sort by end + DP + binary search",
        ),
        (
            "fewest rooms / machines / registers",
            "interval colouring = max overlap (sweep + heap)",
        ),
        (
            "connect these points as cheaply as possible",
            "MST if you may use only the points; Steiner tree if extra hubs help",
        ),
        (
            "best contiguous block / rectangle",
            "Kadane; 2-D = collapse rows, then Kadane",
        ),
        (
            "duplicates, closest pair, groups, 'is it a permutation of'",
            "sort first (or hash); adjacent elements tell all",
        ),
        (
            "precedence / ordering constraints",
            "DAG; see the graph recipes (cycle check via SCC)",
        ),
        (
            "count paths / ways, optimal substructure",
            "dynamic programming (see r137)",
        ),
        (
            "nearest thing to this point",
            "spatial index (r277) or approximate NN (r140-r143)",
        ),
        (
            "shortest route with weights",
            "Dijkstra (non-negative) / Bellman-Ford (negative) / BFS (unit)",
        ),
        (
            "is x prime, factor n, modular roots, discrete log",
            "number theory toolkit (r275)",
        ),
    ] {
        println!("  {p:<55} -> {c}");
    }
    println!("OK");
}
