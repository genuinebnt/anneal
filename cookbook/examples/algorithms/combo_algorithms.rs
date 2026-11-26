//! RECIPE 162 (algorithms): problems that need several data structures working together.
//! LEVEL: advanced
//! Use in: caches, route planning, tree/hierarchy queries, offline network analysis.   Run: cargo run --release --example combo_algorithms
#![allow(dead_code)]
//!
//! WHAT: six classic "one structure is not enough" problems, each solved efficiently AND by an obviously
//! correct brute force, compared on seeded random inputs (xorshift, fully deterministic).
//!   (a) O(1) LFU cache          = HashMap + frequency buckets of doubly linked lists in arenas
//!   (b) cheapest route, <= K stops = Bellman-Ford rounds (a "layered graph" without building the layers)
//!   (c) route through all checkpoints = Dijkstra (heap) + bitmask DP over subsets (travelling salesman style)
//!   (d) subtree / path sums with updates = Euler tour (DFS order) + Fenwick tree
//!   (e) LCA + tree distance      = binary lifting (jump pointers)
//!   (f) offline "connected using edges <= w?" = sort edges and queries + union-find
//! WHY combine: each structure answers one question fast (hash: "where?", list: "who is oldest?", heap:
//! "who is next?", Fenwick: "what is the prefix sum under updates?"); the trick is flattening the problem
//! (tree -> array ranges, queries -> sorted order, graph -> layers) so the right tool applies.
//! COMMON USES: LFU/LRU caches in CDNs and databases; flight/transit planners with a transfer limit; delivery
//! and tour planning for few stops; org-chart or file-tree rollups with live updates; nearest common manager,
//! class-hierarchy or network-distance queries; "which sites are linked at latency <= x" reports.
//! Read: https://cp-algorithms.com/graph/lca_binary_lifting.html and https://cp-algorithms.com/data_structures/fenwick.html
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// ============================ (a) O(1) LFU cache ============================
const NIL: usize = usize::MAX;

struct LfuNode {
    key: u32,
    val: u32,
    freq: u32,
    prev: usize, // toward more recently used inside the same frequency list
    next: usize,
}

#[derive(Clone, Copy)]
struct List {
    head: usize, // most recent
    tail: usize, // least recent: the eviction candidate for this frequency
}

struct Lfu {
    cap: usize,
    nodes: Vec<LfuNode>, // arena; indices are stable handles
    free: Vec<usize>,
    map: HashMap<u32, usize>,
    buckets: HashMap<u32, List>, // frequency -> list
    min_freq: u32,
}

impl Lfu {
    fn new(cap: usize) -> Self {
        Lfu {
            cap,
            nodes: vec![],
            free: vec![],
            map: HashMap::new(),
            buckets: HashMap::new(),
            min_freq: 0,
        }
    }

    /// detach from its frequency list; returns true if that list became empty (and was removed)
    fn unlink(&mut self, i: usize) -> bool {
        let (p, n, f) = (self.nodes[i].prev, self.nodes[i].next, self.nodes[i].freq);
        let list = self.buckets.get_mut(&f).unwrap();
        if p == NIL {
            list.head = n
        } else {
            self.nodes[p].next = n
        }
        if n == NIL {
            list.tail = p
        } else {
            self.nodes[n].prev = p
        }
        if list.head == NIL {
            self.buckets.remove(&f);
            return true;
        }
        false
    }

    fn push_front(&mut self, i: usize) {
        let f = self.nodes[i].freq;
        let list = self.buckets.entry(f).or_insert(List {
            head: NIL,
            tail: NIL,
        });
        self.nodes[i].prev = NIL;
        self.nodes[i].next = list.head;
        if list.head != NIL {
            self.nodes[list.head].prev = i;
        } else {
            list.tail = i;
        }
        list.head = i;
    }

    fn touch(&mut self, i: usize) {
        let f = self.nodes[i].freq;
        if self.unlink(i) && self.min_freq == f {
            self.min_freq = f + 1; // the old minimum bucket vanished: the node itself moves to f+1
        }
        self.nodes[i].freq = f + 1;
        self.push_front(i);
    }

    fn get(&mut self, key: u32) -> Option<u32> {
        let i = *self.map.get(&key)?;
        self.touch(i);
        Some(self.nodes[i].val)
    }

    fn put(&mut self, key: u32, val: u32) {
        if let Some(&i) = self.map.get(&key) {
            self.nodes[i].val = val;
            self.touch(i);
            return;
        }
        if self.map.len() == self.cap {
            let victim = self.buckets[&self.min_freq].tail; // least frequent, then least recent
            self.unlink(victim);
            self.map.remove(&self.nodes[victim].key);
            self.free.push(victim);
        }
        let node = LfuNode {
            key,
            val,
            freq: 1,
            prev: NIL,
            next: NIL,
        };
        let i = match self.free.pop() {
            Some(slot) => {
                self.nodes[slot] = node;
                slot
            }
            None => {
                self.nodes.push(node);
                self.nodes.len() - 1
            }
        };
        self.map.insert(key, i);
        self.min_freq = 1;
        self.push_front(i);
    }
}

/// oracle: O(n) scan; evict min (freq, last-used tick)
struct NaiveLfu {
    cap: usize,
    tick: u64,
    items: Vec<(u32, u32, u32, u64)>, // key, val, freq, last used
}

impl NaiveLfu {
    fn get(&mut self, key: u32) -> Option<u32> {
        self.tick += 1;
        let t = self.tick;
        let it = self.items.iter_mut().find(|e| e.0 == key)?;
        it.2 += 1;
        it.3 = t;
        Some(it.1)
    }
    fn put(&mut self, key: u32, val: u32) {
        self.tick += 1;
        let t = self.tick;
        if let Some(it) = self.items.iter_mut().find(|e| e.0 == key) {
            it.1 = val;
            it.2 += 1;
            it.3 = t;
            return;
        }
        if self.items.len() == self.cap {
            let victim = (0..self.items.len())
                .min_by_key(|&i| (self.items[i].2, self.items[i].3))
                .unwrap();
            self.items.swap_remove(victim);
        }
        self.items.push((key, val, 1, t));
    }
}

fn section_lfu(rng: &mut Rng) {
    println!(
        "(a) LFU cache: combines HashMap + frequency buckets + doubly linked lists in arenas."
    );
    println!("    real use: CDN / database page caches that keep hot keys through scans.");
    for cap in [1, 2, 5, 12] {
        let (mut fast, mut slow) = (
            Lfu::new(cap),
            NaiveLfu {
                cap,
                tick: 0,
                items: vec![],
            },
        );
        for step in 0..20_000u32 {
            let key = rng.below(cap * 2 + 3) as u32;
            if rng.below(2) == 0 {
                assert_eq!(fast.get(key), slow.get(key));
            } else {
                fast.put(key, step);
                slow.put(key, step);
            }
        }
    }
    println!("    ok: 4 capacities x 20000 random ops identical to the O(n) oracle");
}

// ============================ (b) cheapest route with at most K stops ============================
fn cheapest_k_stops(
    n: usize,
    edges: &[(usize, usize, u64)],
    src: usize,
    dst: usize,
    k: usize,
) -> Option<u64> {
    let mut dist: Vec<Option<u64>> = vec![None; n];
    dist[src] = Some(0);
    for _ in 0..=k {
        // allow at most K+1 flights; read ONLY last round's values so each round adds exactly one edge
        let prev = dist.clone();
        for &(a, b, w) in edges {
            if let Some(d) = prev[a] {
                if dist[b].is_none_or(|c| d + w < c) {
                    dist[b] = Some(d + w);
                }
            }
        }
    }
    dist[dst]
}

fn cheapest_dfs(
    g: &[Vec<(usize, u64)>],
    u: usize,
    dst: usize,
    edges_left: usize,
    visited: &mut Vec<bool>,
) -> Option<u64> {
    if u == dst {
        return Some(0);
    }
    if edges_left == 0 {
        return None;
    }
    visited[u] = true;
    let mut best: Option<u64> = None;
    for &(v, w) in &g[u] {
        if !visited[v] {
            if let Some(rest) = cheapest_dfs(g, v, dst, edges_left - 1, visited) {
                best = Some(best.map_or(w + rest, |b| b.min(w + rest)));
            }
        }
    }
    visited[u] = false;
    best // simple paths suffice: weights are non-negative, so cycles never help
}

fn section_stops(rng: &mut Rng) {
    println!(
        "(b) route with <= K stops: combines edge list + per-round distance arrays (layers) vs DFS oracle."
    );
    println!("    real use: flight/transit planners ('cheapest fare with at most one transfer').");
    for _ in 0..400 {
        let n = 2 + rng.below(7);
        let edges: Vec<(usize, usize, u64)> = (0..rng.below(20))
            .map(|_| (rng.below(n), rng.below(n), 1 + rng.below(30) as u64))
            .collect();
        let mut g = vec![vec![]; n];
        edges.iter().for_each(|&(a, b, w)| g[a].push((b, w)));
        let (src, dst, k) = (rng.below(n), rng.below(n), rng.below(4));
        let want = cheapest_dfs(&g, src, dst, k + 1, &mut vec![false; n]);
        assert_eq!(cheapest_k_stops(n, &edges, src, dst, k), want);
    }
    println!("    ok: 400 random graphs, K in 0..=3");
}

// ============================ (c) visit every checkpoint ============================
fn dijkstra(g: &[Vec<(usize, u64)>], src: usize) -> Vec<u64> {
    let mut dist = vec![u64::MAX; g.len()];
    let mut h = BinaryHeap::new();
    dist[src] = 0;
    h.push(Reverse((0, src)));
    while let Some(Reverse((d, u))) = h.pop() {
        if d > dist[u] {
            continue;
        }
        for &(v, w) in &g[u] {
            if d + w < dist[v] {
                dist[v] = d + w;
                h.push(Reverse((d + w, v)));
            }
        }
    }
    dist
}

/// shortest walk that starts at checkpoints[0] and touches all others (any order, no return)
fn tour_dp(g: &[Vec<(usize, u64)>], cps: &[usize]) -> u64 {
    let m = cps.len();
    let d: Vec<Vec<u64>> = cps.iter().map(|&c| dijkstra(g, c)).collect(); // m heap searches, not n^2
    let mut dp = vec![vec![u64::MAX; m]; 1 << m]; // dp[visited set][last checkpoint]
    dp[1][0] = 0;
    for mask in 1usize..1 << m {
        for last in 0..m {
            let cur = dp[mask][last];
            if mask & (1 << last) == 0 || cur == u64::MAX {
                continue;
            }
            for next in 0..m {
                if mask & (1 << next) == 0 {
                    let nm = mask | 1 << next;
                    dp[nm][next] = dp[nm][next].min(cur + d[last][cps[next]]);
                }
            }
        }
    }
    (0..m).map(|l| dp[(1 << m) - 1][l]).min().unwrap()
}

fn tour_brute(all: &[Vec<u64>], cps: &[usize]) -> u64 {
    fn go(
        all: &[Vec<u64>],
        cps: &[usize],
        used: &mut Vec<bool>,
        last: usize,
        cost: u64,
        left: usize,
    ) -> u64 {
        if left == 0 {
            return cost;
        }
        let mut best = u64::MAX;
        for i in 1..cps.len() {
            if !used[i] {
                used[i] = true;
                best = best.min(go(
                    all,
                    cps,
                    used,
                    i,
                    cost + all[cps[last]][cps[i]],
                    left - 1,
                ));
                used[i] = false;
            }
        }
        best
    }
    go(all, cps, &mut vec![false; cps.len()], 0, 0, cps.len() - 1)
}

fn floyd(g: &[Vec<(usize, u64)>]) -> Vec<Vec<u64>> {
    let n = g.len();
    let mut d = vec![vec![u64::MAX / 4; n]; n];
    for u in 0..n {
        d[u][u] = 0;
        for &(v, w) in &g[u] {
            d[u][v] = d[u][v].min(w);
        }
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

fn section_tour(rng: &mut Rng) {
    println!(
        "(c) visit all checkpoints: combines Dijkstra (heap) + bitmask DP over subsets vs permutations + Floyd-Warshall."
    );
    println!(
        "    real use: delivery rounds, inspection tours, multi-stop trip planning with a handful of stops."
    );
    for _ in 0..60 {
        let n = 8 + rng.below(5);
        let mut g: Vec<Vec<(usize, u64)>> = vec![vec![]; n];
        let add = |g: &mut Vec<Vec<(usize, u64)>>, a: usize, b: usize, w: u64| {
            g[a].push((b, w));
            g[b].push((a, w));
        };
        for u in 0..n {
            add(&mut g, u, (u + 1) % n, 1 + rng.below(20) as u64); // ring keeps it connected
        }
        for _ in 0..n {
            add(&mut g, rng.below(n), rng.below(n), 1 + rng.below(40) as u64);
        }
        let mut cps: Vec<usize> = (0..n).collect();
        for i in 0..cps.len() {
            cps.swap(i, i + rng.below(n - i)); // shuffle, keep the first 5
        }
        cps.truncate(5);
        assert_eq!(tour_dp(&g, &cps), tour_brute(&floyd(&g), &cps));
    }
    println!("    ok: 60 random graphs, 5 checkpoints, DP == all 24 permutations");
}

// ============================ trees ============================
/// parent[i] < i, so a plain loop gives a valid rooted tree; `deep` biases towards long chains
fn random_tree(rng: &mut Rng, n: usize, deep: bool) -> Vec<usize> {
    (0..n)
        .map(|i| {
            if i == 0 {
                0
            } else if deep {
                i.saturating_sub(1 + rng.below(3))
            } else {
                rng.below(i)
            }
        })
        .collect()
}

struct Fenwick(Vec<i64>);

impl Fenwick {
    fn new(n: usize) -> Self {
        Fenwick(vec![0; n + 1])
    }
    fn add(&mut self, i: usize, d: i64) {
        let mut i = i + 1; // 1-based internally; each step adds the lowest set bit
        while i < self.0.len() {
            self.0[i] += d;
            i += i & i.wrapping_neg();
        }
    }
    fn prefix(&self, i: usize) -> i64 {
        // sum of [0, i)
        let (mut i, mut s) = (i, 0);
        while i > 0 {
            s += self.0[i];
            i -= i & i.wrapping_neg();
        }
        s
    }
}

/// iterative DFS entry/exit times: subtree of v is exactly the index range tin[v]..tout[v]
fn euler(parent: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let n = parent.len();
    let mut kids = vec![vec![]; n];
    (1..n).for_each(|v| kids[parent[v]].push(v));
    let (mut tin, mut tout, mut t) = (vec![0; n], vec![0; n], 0);
    let mut stack = vec![(0usize, 0usize)];
    tin[0] = 0;
    t += 1;
    while let Some((v, i)) = stack.pop() {
        if i < kids[v].len() {
            stack.push((v, i + 1));
            let c = kids[v][i];
            tin[c] = t;
            t += 1;
            stack.push((c, 0));
        } else {
            tout[v] = t;
        }
    }
    (tin, tout)
}

fn section_euler(rng: &mut Rng) {
    println!(
        "(d) subtree / root-path sums with point updates: combines Euler tour (tree -> array ranges) + Fenwick tree."
    );
    println!(
        "    real use: org-chart budget rollups, file-system size per directory, live hierarchy dashboards."
    );
    for deep in [false, true] {
        let n = 200;
        let parent = random_tree(rng, n, deep);
        let (tin, tout) = euler(&parent);
        let mut val = vec![0i64; n];
        let mut sub = Fenwick::new(n); // point update, range query: subtree sums
        let mut path = Fenwick::new(n + 1); // range update (difference array), point query: root-to-node sums
        for _ in 0..3000 {
            let v = rng.below(n);
            if rng.below(2) == 0 {
                let d = rng.below(21) as i64 - 10;
                val[v] += d;
                sub.add(tin[v], d);
                path.add(tin[v], d); // adding d at the start of v's range ...
                path.add(tout[v], -d); // ... and removing it after the end = "add d to every descendant"
            } else {
                let naive_sub: i64 = (0..n)
                    .filter(|&u| in_subtree(&parent, u, v))
                    .map(|u| val[u])
                    .sum();
                assert_eq!(sub.prefix(tout[v]) - sub.prefix(tin[v]), naive_sub);
                let (mut u, mut naive_path) = (v, val[v]);
                while u != 0 {
                    u = parent[u];
                    naive_path += val[u];
                }
                assert_eq!(path.prefix(tin[v] + 1), naive_path);
            }
        }
    }
    println!(
        "    ok: 6000 mixed updates/queries on a shallow and a deep tree match naive DFS/walks"
    );
}

fn in_subtree(parent: &[usize], mut u: usize, root: usize) -> bool {
    loop {
        if u == root {
            return true;
        }
        if u == 0 {
            return false;
        }
        u = parent[u];
    }
}

// ============================ (e) LCA via binary lifting ============================
struct Lca {
    up: Vec<Vec<usize>>, // up[j][v] = the 2^j-th ancestor of v (root points to itself)
    depth: Vec<usize>,
}

impl Lca {
    fn new(parent: &[usize]) -> Self {
        let n = parent.len();
        let levels = (usize::BITS - n.leading_zeros()) as usize;
        let mut depth = vec![0; n];
        (1..n).for_each(|v| depth[v] = depth[parent[v]] + 1); // parent[v] < v: already computed
        let mut up = vec![parent.to_vec()];
        for j in 1..levels {
            let prev = &up[j - 1];
            up.push((0..n).map(|v| prev[prev[v]]).collect()); // 2^j = 2^(j-1) + 2^(j-1)
        }
        Lca { up, depth }
    }

    fn lca(&self, mut a: usize, mut b: usize) -> usize {
        if self.depth[a] < self.depth[b] {
            std::mem::swap(&mut a, &mut b);
        }
        let diff = self.depth[a] - self.depth[b];
        for j in 0..self.up.len() {
            if diff >> j & 1 == 1 {
                a = self.up[j][a]; // lift the deeper node to the same depth
            }
        }
        if a == b {
            return a;
        }
        for j in (0..self.up.len()).rev() {
            if self.up[j][a] != self.up[j][b] {
                a = self.up[j][a]; // jump while the ancestors still differ
                b = self.up[j][b];
            }
        }
        self.up[0][a]
    }

    fn dist(&self, a: usize, b: usize) -> usize {
        self.depth[a] + self.depth[b] - 2 * self.depth[self.lca(a, b)]
    }
}

fn naive_lca(parent: &[usize], a: usize, b: usize) -> usize {
    let mut anc = vec![false; parent.len()];
    let mut u = a;
    loop {
        anc[u] = true;
        if u == 0 {
            break;
        }
        u = parent[u];
    }
    let mut v = b;
    while !anc[v] {
        v = parent[v];
    }
    v
}

fn section_lca(rng: &mut Rng) {
    println!(
        "(e) LCA + distance: combines parent array + binary-lifting table (O(n log n) memory, O(log n) query)."
    );
    println!(
        "    real use: nearest common manager/ancestor, class hierarchies, path length between two nodes in a network tree."
    );
    for deep in [false, true] {
        let n = 500;
        let parent = random_tree(rng, n, deep);
        let l = Lca::new(&parent);
        for _ in 0..3000 {
            let (a, b) = (rng.below(n), rng.below(n));
            let want = naive_lca(&parent, a, b);
            assert_eq!(l.lca(a, b), want);
            let naive_dist = {
                let depth_of = |mut x: usize| {
                    let mut d = 0;
                    while x != 0 {
                        x = parent[x];
                        d += 1;
                    }
                    d
                };
                depth_of(a) + depth_of(b) - 2 * depth_of(want)
            };
            assert_eq!(l.dist(a, b), naive_dist);
        }
    }
    println!("    ok: 6000 LCA + distance queries on a shallow and a chain-like tree");
}

// ============================ (f) offline connectivity ============================
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
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            let root = self.find(self.parent[x]);
            self.parent[x] = root; // path compression
        }
        self.parent[x]
    }
    fn union(&mut self, a: usize, b: usize) {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return;
        }
        if self.size[a] < self.size[b] {
            std::mem::swap(&mut a, &mut b); // union by size keeps trees shallow
        }
        self.parent[b] = a;
        self.size[a] += self.size[b];
    }
}

/// answers[i] for query (u, v, w): connected using only edges of weight <= w?
fn offline_connectivity(
    n: usize,
    edges: &[(usize, usize, u32)],
    queries: &[(usize, usize, u32)],
) -> Vec<bool> {
    let mut es = edges.to_vec();
    es.sort_by_key(|e| e.2);
    let mut order: Vec<usize> = (0..queries.len()).collect();
    order.sort_by_key(|&i| queries[i].2); // answer queries by growing threshold, adding edges as we go
    let (mut dsu, mut next, mut ans) = (Dsu::new(n), 0, vec![false; queries.len()]);
    for qi in order {
        let (u, v, w) = queries[qi];
        while next < es.len() && es[next].2 <= w {
            dsu.union(es[next].0, es[next].1);
            next += 1;
        }
        ans[qi] = dsu.find(u) == dsu.find(v);
    }
    ans
}

fn bfs_connected(n: usize, edges: &[(usize, usize, u32)], u: usize, v: usize, w: u32) -> bool {
    let mut adj = vec![vec![]; n];
    for &(a, b, _) in edges.iter().filter(|e| e.2 <= w) {
        adj[a].push(b);
        adj[b].push(a);
    }
    let (mut seen, mut q) = (vec![false; n], VecDeque::from([u]));
    seen[u] = true;
    while let Some(x) = q.pop_front() {
        for &y in &adj[x] {
            if !seen[y] {
                seen[y] = true;
                q.push_back(y);
            }
        }
    }
    seen[v]
}

fn section_connectivity(rng: &mut Rng) {
    println!(
        "(f) connectivity under a weight limit: combines sorting (edges + queries) + union-find vs per-query BFS."
    );
    println!(
        "    real use: 'which sites are reachable at latency <= x', clustering by similarity threshold, Kruskal-style analysis."
    );
    let mut yes = 0;
    for _ in 0..20 {
        let n = 5 + rng.below(50);
        let edges: Vec<(usize, usize, u32)> = (0..rng.below(2 * n))
            .map(|_| (rng.below(n), rng.below(n), rng.below(100) as u32))
            .collect();
        let queries: Vec<(usize, usize, u32)> = (0..300)
            .map(|_| (rng.below(n), rng.below(n), rng.below(110) as u32))
            .collect();
        let fast = offline_connectivity(n, &edges, &queries);
        for (i, &(u, v, w)) in queries.iter().enumerate() {
            assert_eq!(fast[i], bfs_connected(n, &edges, u, v, w));
        }
        yes += fast.iter().filter(|&&b| b).count();
    }
    assert!(
        yes > 100 && yes < 5900,
        "test should mix yes and no answers"
    );
    println!(
        "    ok: 6000 queries over 20 random graphs ({yes} connected, {} not)",
        6000 - yes
    );
}

fn main() {
    let mut rng = Rng::new(162);
    section_lfu(&mut rng);
    section_stops(&mut rng);
    section_tour(&mut rng);
    section_euler(&mut rng);
    section_lca(&mut rng);
    section_connectivity(&mut rng);
    println!("all combination checks passed");
}
