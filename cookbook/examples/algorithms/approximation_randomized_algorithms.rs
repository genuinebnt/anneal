//! RECIPE 890 (algorithms): approximation algorithms with proven ratios, and randomised algorithms with measured error probabilities.
//! LEVEL: advanced
//! Use in: NP-hard optimisation where "within a guaranteed factor, fast" beats "optimal, maybe never" (routing, scheduling, covering, clustering), primality testing, randomised verification, global min-cut.   Run: cargo run --release --example approximation_randomized_algorithms
//! COMMON USES: cryptographic prime generation (Miller-Rabin), verifying big matrix products or pipelines cheaply (Freivalds-style fingerprints), list scheduling and LPT in job schedulers, greedy set cover for test-suite minimisation and monitor placement, Gonzalez k-center for picking cluster seeds and replica sites, quickselect in std's select_nth_unstable, network reliability via Karger min-cut.
//! Course: CMU 15-451/651 (approximation and randomised algorithms lectures); Vazirani, "Approximation Algorithms"; Motwani and Raghavan, "Randomized Algorithms".
#![allow(dead_code)]
//!
//! PART 1, approximation: an algorithm is a rho-approximation if on EVERY instance it returns a feasible answer within a
//! factor rho of the optimum. Each routine below is paired with a brute-force optimum on tiny instances, and the
//! ratio is ASSERTED on 1500 random instances (never timings):
//!   - vertex cover, maximal matching: take both ends of each greedily picked disjoint edge; size <= 2*OPT because OPT needs
//!     one endpoint per matched edge.
//!   - set cover, greedy (take the set covering the most uncovered elements): <= H(d)*OPT, d = largest set size, H = harmonic.
//!   - metric TSP: MST doubling (shortcut a preorder walk) <= 2*OPT since MST <= OPT; Christofides-lite (MST + minimum
//!     matching on odd-degree vertices, brute-forced since the instances are tiny, Euler tour, shortcut) <= 1.5*OPT.
//!   - max-cut: a uniformly random side per vertex cuts m/2 edges in expectation; local search (flip a vertex while it improves)
//!     ends with every vertex having at least half its edges cut, hence cut >= m/2 >= OPT/2.
//!   - k-center, Gonzalez farthest-first traversal: radius <= 2*OPT (a pigeonhole argument on k+1 chosen points).
//!   - knapsack FPTAS: scale values by K = eps*vmax/n, exact DP on the scaled values, result >= (1-eps)*OPT, running time
//!     polynomial in n and 1/eps (this is the textbook way to trade accuracy for time on a weakly NP-hard problem).
//!   - load balancing, LPT (sort jobs descending, assign to least loaded machine): <= (4/3 - 1/(3m))*OPT (Graham 1969).
//! PART 2, randomised algorithms (Las Vegas: always right, random time; Monte Carlo: random answer, bounded error):
//!   - randomised quicksort: expected comparisons exactly 2(n+1)H_n - 4n; the deterministic first-pivot rule degrades to n^2/2 on
//!     sorted input, the random rule does not (no input is bad, only unlucky coin flips). Quickselect: expected ~3.4n for the median.
//!   - Karger contraction: a fixed min cut survives with probability >= 2/(n(n-1)); repeating C(n,2) ln n times drives the failure
//!     below 1/n. We MEASURE the single-run success rate and the amplified failure rate.
//!   - Miller-Rabin: for an odd composite at most 1/4 of the bases are strong liars; we count the liars exactly for every odd
//!     composite below 5000, then compare the deterministic 12-base test with a sieve.
//!   - Freivalds: checking A*B == C in O(n^2) per round with a random 0/1 vector; a wrong C is missed with probability <= 1/2 per round
//!     (we measure exactly 1/2 for a single-entry error) and 2^-k after k rounds.
//!   Referenced only (not implemented here): skip lists and treaps (random priorities give expected O(log n) height), and MinHash
//!   (the collision probability of min-of-hash equals Jaccard similarity), see the hashing/probabilistic recipes for sketches.
//! WHY exact optima on tiny instances: a ratio can only be "asserted" against the true OPT; for n <= 10 brute force is cheap and
//! the proofs' worst cases (factor close to rho) show up often enough on random small inputs to catch an off-by-one in the algorithm.
//! Read: https://en.wikipedia.org/wiki/Approximation_algorithm ; https://en.wikipedia.org/wiki/Karger%27s_algorithm ;
//! https://en.wikipedia.org/wiki/Miller%E2%80%93Rabin_primality_test ; https://en.wikipedia.org/wiki/Freivalds%27_algorithm ;
//! Vazirani "Approximation Algorithms" and Motwani/Raghavan "Randomized Algorithms" (books, cited by title only).
//!
//! TRY NEXT:
//!   - [S] In `vertex_cover_matching`, iterate the edges in a different order (reverse, or by degree). Does the worst measured ratio
//!         change? Predict, then print it; then explain why the 2-factor proof does not care.
//!   - [S] Lower `Freivalds` rounds from 30 to 1 in `freivalds_demo` and count how many of the 1500 wrong products slip through;
//!         predict the number first from the 1/2 per-round miss rate.
//!   - [M] Replace `greedy` set cover by "pick sets in index order" and find, by random search, an instance where its ratio exceeds H(d);
//!         then build the classic instance that makes the REAL greedy lose by ~ln n.
//!   - [M] Real-world port: use `gonzalez` to choose k replica sites among 200 random points on a plane, and compare its radius to a
//!         swap-based local search; which wins, and by how much on clustered data?
//!   - [A] PROJECT: combine this recipe with `amortized_analysis_online_algorithms` and `sorting_searching`: build an online job
//!         scheduler benchmark where `lpt` is the offline benchmark and greedy list scheduling is the online algorithm; measure the
//!         ratio between them on random and adversarial job sequences, and sort the jobs with the quickselect/median pivot from here.

use std::collections::HashMap;

// ---------------------------------------------------------------------------------------------------------------------
// Seeded randomness.
// ---------------------------------------------------------------------------------------------------------------------
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

type Edge = (usize, usize);

fn random_graph(rng: &mut Rng, n: usize, p: f64) -> Vec<Edge> {
    let mut e = vec![];
    for a in 0..n {
        for b in a + 1..n {
            if rng.chance(p) {
                e.push((a, b));
            }
        }
    }
    e
}

fn harmonic(d: usize) -> f64 {
    (1..=d).map(|i| 1.0 / i as f64).sum()
}

// ---------------------------------------------------------------------------------------------------------------------
// 1. Vertex cover: maximal matching, 2-approximation.
// ---------------------------------------------------------------------------------------------------------------------
fn vertex_cover_matching(n: usize, edges: &[Edge]) -> Vec<usize> {
    let mut covered = vec![false; n];
    for &(a, b) in edges {
        if !covered[a] && !covered[b] {
            // (a, b) joins the matching; every cover must contain a or b, we take BOTH.
            covered[a] = true;
            covered[b] = true;
        }
    }
    (0..n).filter(|&v| covered[v]).collect()
}

fn is_cover(edges: &[Edge], cover: &[usize]) -> bool {
    edges
        .iter()
        .all(|(a, b)| cover.contains(a) || cover.contains(b))
}

fn vertex_cover_opt(n: usize, edges: &[Edge]) -> usize {
    (0u32..1 << n)
        .filter(|&m| {
            edges
                .iter()
                .all(|&(a, b)| m >> a & 1 == 1 || m >> b & 1 == 1)
        })
        .map(|m| m.count_ones() as usize)
        .min()
        .unwrap()
}

fn vertex_cover_demo() {
    let mut rng = Rng::new(1);
    let mut worst = 0f64;
    for _ in 0..1500 {
        let n = 2 + rng.below(9);
        let dens = 0.2 + 0.4 * rng.below(3) as f64 / 2.0;
        let edges = random_graph(&mut rng, n, dens);
        let cover = vertex_cover_matching(n, &edges);
        let opt = vertex_cover_opt(n, &edges);
        assert!(is_cover(&edges, &cover), "returned set must be a cover");
        assert!(cover.len() <= 2 * opt, "{} > 2*{opt}", cover.len());
        if opt > 0 {
            worst = worst.max(cover.len() as f64 / opt as f64);
        }
    }
    println!("vertex cover (matching): worst ratio {worst:.2} (bound 2)");
}

// ---------------------------------------------------------------------------------------------------------------------
// 2. Set cover: greedy, H(d)-approximation.
// ---------------------------------------------------------------------------------------------------------------------
fn greedy_set_cover(universe: u32, sets: &[u32]) -> Vec<usize> {
    let (mut left, mut chosen) = (universe, vec![]);
    while left != 0 {
        let best = (0..sets.len())
            .max_by_key(|&i| ((sets[i] & left).count_ones(), std::cmp::Reverse(i)))
            .unwrap();
        assert!(sets[best] & left != 0, "instance must be coverable");
        left &= !sets[best];
        chosen.push(best);
    }
    chosen
}

fn set_cover_opt(universe: u32, sets: &[u32]) -> usize {
    (0u32..1 << sets.len())
        .filter(|&m| {
            (0..sets.len())
                .filter(|&i| m >> i & 1 == 1)
                .fold(0, |acc, i| acc | sets[i])
                & universe
                == universe
        })
        .map(|m| m.count_ones() as usize)
        .min()
        .unwrap()
}

fn set_cover_demo() {
    let mut rng = Rng::new(2);
    let mut worst = 0f64;
    for _ in 0..1500 {
        let n = 3 + rng.below(8);
        let universe = (1u32 << n) - 1;
        let k = 3 + rng.below(6);
        let mut sets: Vec<u32> = (0..k)
            .map(|_| (rng.next() as u32 & universe) | 1 << rng.below(n))
            .collect();
        // Guarantee coverage: put every uncovered element into a random set.
        let covered = sets.iter().fold(0, |a, &s| a | s);
        for e in 0..n {
            if covered >> e & 1 == 0 {
                let i = rng.below(k);
                sets[i] |= 1 << e;
            }
        }
        let g = greedy_set_cover(universe, &sets);
        let opt = set_cover_opt(universe, &sets);
        let d = sets.iter().map(|s| s.count_ones() as usize).max().unwrap();
        let bound = harmonic(d) * opt as f64;
        assert!(
            g.len() as f64 <= bound + 1e-9,
            "greedy {} > H({d})*{opt}",
            g.len()
        );
        worst = worst.max(g.len() as f64 / opt as f64);
    }
    println!(
        "set cover (greedy): worst ratio {worst:.2} (bound H(d) <= H(10) = {:.2})",
        harmonic(10)
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 3. Metric TSP: MST doubling (2) and Christofides-lite (1.5).
// ---------------------------------------------------------------------------------------------------------------------
type Pt = (f64, f64);
fn dist(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}
fn tour_len(p: &[Pt], t: &[usize]) -> f64 {
    (0..t.len())
        .map(|i| dist(p[t[i]], p[t[(i + 1) % t.len()]]))
        .sum()
}

/// Prim's MST in O(n^2): returns the n-1 edges.
fn mst(p: &[Pt]) -> Vec<Edge> {
    let n = p.len();
    let (mut in_tree, mut best, mut from) = (vec![false; n], vec![f64::INFINITY; n], vec![0; n]);
    best[0] = 0.0;
    let mut edges = vec![];
    for _ in 0..n {
        let u = (0..n)
            .filter(|&v| !in_tree[v])
            .min_by(|&a, &b| best[a].total_cmp(&best[b]))
            .unwrap();
        in_tree[u] = true;
        if u != 0 {
            edges.push((from[u], u));
        }
        for v in 0..n {
            if !in_tree[v] && dist(p[u], p[v]) < best[v] {
                best[v] = dist(p[u], p[v]);
                from[v] = u;
            }
        }
    }
    edges
}

/// Shortcutting an Euler circuit = keep the first visit of each vertex. The triangle inequality makes this never longer.
fn shortcut(circuit: &[usize], n: usize) -> Vec<usize> {
    let mut seen = vec![false; n];
    circuit
        .iter()
        .copied()
        .filter(|&v| !std::mem::replace(&mut seen[v], true))
        .collect()
}

/// Euler circuit by Hierholzer on a multigraph given as an edge list (all degrees must be even, graph connected).
fn euler_circuit(n: usize, edges: &[Edge]) -> Vec<usize> {
    let mut adj: Vec<Vec<(usize, usize)>> = vec![vec![]; n];
    for (id, &(a, b)) in edges.iter().enumerate() {
        adj[a].push((b, id));
        adj[b].push((a, id));
    }
    assert!(
        adj.iter().all(|a| a.len() % 2 == 0),
        "Euler circuit needs even degrees"
    );
    let (mut used, mut ptr) = (vec![false; edges.len()], vec![0; n]);
    let (mut stack, mut circuit) = (vec![0], vec![]);
    while let Some(&v) = stack.last() {
        while ptr[v] < adj[v].len() && used[adj[v][ptr[v]].1] {
            ptr[v] += 1;
        }
        if ptr[v] < adj[v].len() {
            let (to, id) = adj[v][ptr[v]];
            used[id] = true;
            stack.push(to);
        } else {
            circuit.push(v);
            stack.pop();
        }
    }
    assert_eq!(circuit.len(), edges.len() + 1, "graph must be connected");
    circuit
}

fn tsp_mst_doubling(p: &[Pt]) -> Vec<usize> {
    // Doubling every MST edge makes all degrees even; the Euler circuit then equals a DFS walk. Shortcut it.
    let edges = mst(p);
    let doubled: Vec<Edge> = edges.iter().flat_map(|&e| [e, e]).collect();
    shortcut(&euler_circuit(p.len(), &doubled), p.len())
}

/// Minimum-weight perfect matching on `odd` by plain recursion (<= 7*5*3*1 = 105 leaves for 8 vertices).
fn min_matching(p: &[Pt], odd: &[usize]) -> (f64, Vec<Edge>) {
    if odd.is_empty() {
        return (0.0, vec![]);
    }
    let a = odd[0];
    let mut best = (f64::INFINITY, vec![]);
    for j in 1..odd.len() {
        let rest: Vec<usize> = odd
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != 0 && i != j)
            .map(|(_, &v)| v)
            .collect();
        let (c, mut m) = min_matching(p, &rest);
        let c = c + dist(p[a], p[odd[j]]);
        if c < best.0 {
            m.push((a, odd[j]));
            best = (c, m);
        }
    }
    best
}

fn tsp_christofides_lite(p: &[Pt]) -> Vec<usize> {
    let mut edges = mst(p);
    let mut deg = vec![0; p.len()];
    edges.iter().for_each(|&(a, b)| {
        deg[a] += 1;
        deg[b] += 1;
    });
    let odd: Vec<usize> = (0..p.len()).filter(|&v| deg[v] % 2 == 1).collect();
    assert!(odd.len() % 2 == 0, "handshake lemma");
    edges.extend(min_matching(p, &odd).1);
    shortcut(&euler_circuit(p.len(), &edges), p.len())
}

fn tsp_opt(p: &[Pt]) -> f64 {
    fn go(p: &[Pt], path: &mut Vec<usize>, used: &mut Vec<bool>, len: f64, best: &mut f64) {
        if len >= *best {
            return;
        }
        if path.len() == p.len() {
            *best = best.min(len + dist(p[*path.last().unwrap()], p[0]));
            return;
        }
        for v in 1..p.len() {
            if !used[v] {
                used[v] = true;
                path.push(v);
                go(
                    p,
                    path,
                    used,
                    len + dist(p[path[path.len() - 2]], p[v]),
                    best,
                );
                path.pop();
                used[v] = false;
            }
        }
    }
    let mut best = f64::INFINITY;
    go(p, &mut vec![0], &mut vec![false; p.len()], 0.0, &mut best);
    best
}

fn is_perm(t: &[usize], n: usize) -> bool {
    let mut s = t.to_vec();
    s.sort();
    s == (0..n).collect::<Vec<_>>()
}

fn tsp_demo() {
    let mut rng = Rng::new(3);
    let (mut w2, mut w15) = (0f64, 0f64);
    for _ in 0..1500 {
        let n = 3 + rng.below(6);
        let p: Vec<Pt> = (0..n)
            .map(|_| (rng.below(100) as f64, rng.below(100) as f64))
            .collect();
        let opt = tsp_opt(&p);
        let (t2, t15) = (tsp_mst_doubling(&p), tsp_christofides_lite(&p));
        assert!(is_perm(&t2, n) && is_perm(&t15, n));
        let (l2, l15) = (tour_len(&p, &t2), tour_len(&p, &t15));
        assert!(l2 <= 2.0 * opt + 1e-9, "doubling {l2} > 2*{opt}");
        assert!(
            l15 <= 1.5 * opt + 1e-9,
            "christofides-lite {l15} > 1.5*{opt}"
        );
        assert!(l2 + 1e-9 >= opt && l15 + 1e-9 >= opt);
        if opt > 0.0 {
            w2 = w2.max(l2 / opt);
            w15 = w15.max(l15 / opt);
        }
    }
    println!(
        "metric TSP: MST-doubling worst {w2:.3} (bound 2), Christofides-lite worst {w15:.3} (bound 1.5)"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 4. Max-cut: random assignment and local search.
// ---------------------------------------------------------------------------------------------------------------------
fn cut_size(edges: &[Edge], side: &[bool]) -> usize {
    edges.iter().filter(|&&(a, b)| side[a] != side[b]).count()
}

fn local_search_cut(n: usize, edges: &[Edge], side: &mut Vec<bool>) -> usize {
    loop {
        let mut improved = false;
        for v in 0..n {
            // flipping v changes cut by (same-side neighbours) - (cross neighbours)
            let (mut same, mut cross) = (0i32, 0i32);
            for &(a, b) in edges {
                let other = if a == v {
                    b
                } else if b == v {
                    a
                } else {
                    continue;
                };
                if side[other] == side[v] {
                    same += 1
                } else {
                    cross += 1
                }
            }
            if same > cross {
                side[v] = !side[v];
                improved = true;
            }
        }
        if !improved {
            return cut_size(edges, side);
        }
    }
}

fn max_cut_opt(n: usize, edges: &[Edge]) -> usize {
    (0u32..1 << n)
        .map(|m| {
            edges
                .iter()
                .filter(|&&(a, b)| (m >> a ^ m >> b) & 1 == 1)
                .count()
        })
        .max()
        .unwrap()
}

fn max_cut_demo() {
    let mut rng = Rng::new(4);
    let (mut rand_total, mut m_total, mut worst_local) = (0usize, 0usize, 1f64);
    for _ in 0..1500 {
        let n = 2 + rng.below(9);
        let edges = random_graph(&mut rng, n, 0.5);
        let m = edges.len();
        let opt = max_cut_opt(n, &edges);
        let mut side: Vec<bool> = (0..n).map(|_| rng.chance(0.5)).collect();
        rand_total += cut_size(&edges, &side);
        m_total += m;
        let local = local_search_cut(n, &edges, &mut side);
        assert!(
            2 * local >= m,
            "local optimum must cut at least half of the edges"
        );
        assert!(2 * local >= opt && local <= opt);
        if opt > 0 {
            worst_local = worst_local.min(local as f64 / opt as f64);
        }
    }
    let frac = rand_total as f64 / m_total as f64;
    assert!(
        (frac - 0.5).abs() < 0.02,
        "random cut fraction {frac} should be ~1/2"
    );
    println!(
        "max-cut: random assignment cuts {frac:.3} of edges (expect 0.5); local search worst local/OPT {worst_local:.2} (bound 0.5)"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 5. k-center: Gonzalez farthest-first, 2-approximation.
// ---------------------------------------------------------------------------------------------------------------------
fn radius(p: &[Pt], centers: &[usize]) -> f64 {
    p.iter()
        .map(|&q| {
            centers
                .iter()
                .map(|&c| dist(q, p[c]))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0.0, f64::max)
}

fn gonzalez(p: &[Pt], k: usize) -> Vec<usize> {
    let mut centers = vec![0];
    while centers.len() < k {
        // the point farthest from its nearest center becomes the next center
        let far = (0..p.len())
            .max_by(|&a, &b| {
                let da = centers
                    .iter()
                    .map(|&c| dist(p[a], p[c]))
                    .fold(f64::INFINITY, f64::min);
                let db = centers
                    .iter()
                    .map(|&c| dist(p[b], p[c]))
                    .fold(f64::INFINITY, f64::min);
                da.total_cmp(&db)
            })
            .unwrap();
        centers.push(far);
    }
    centers
}

fn k_center_opt(p: &[Pt], k: usize) -> f64 {
    (1u32..1 << p.len())
        .filter(|m| m.count_ones() as usize == k)
        .map(|m| {
            radius(
                p,
                &(0..p.len())
                    .filter(|&i| m >> i & 1 == 1)
                    .collect::<Vec<_>>(),
            )
        })
        .fold(f64::INFINITY, f64::min)
}

fn k_center_demo() {
    let mut rng = Rng::new(5);
    let mut worst = 0f64;
    for _ in 0..1500 {
        let n = 3 + rng.below(8);
        let k = 1 + rng.below(3.min(n - 1));
        let p: Vec<Pt> = (0..n)
            .map(|_| (rng.below(100) as f64, rng.below(100) as f64))
            .collect();
        let g = radius(&p, &gonzalez(&p, k));
        let opt = k_center_opt(&p, k);
        assert!(g <= 2.0 * opt + 1e-9, "gonzalez radius {g} > 2*{opt}");
        if opt > 0.0 {
            worst = worst.max(g / opt);
        }
    }
    println!("k-center (Gonzalez): worst radius ratio {worst:.3} (bound 2)");
}

// ---------------------------------------------------------------------------------------------------------------------
// 6. Knapsack: exact DP and the FPTAS.
// ---------------------------------------------------------------------------------------------------------------------
fn knapsack_exact(items: &[(u64, u64)], cap: u64) -> u64 {
    let mut dp = vec![0u64; cap as usize + 1];
    for &(v, w) in items {
        for c in (w as usize..=cap as usize).rev() {
            dp[c] = dp[c].max(dp[c - w as usize] + v);
        }
    }
    dp[cap as usize]
}

/// Returns the TRUE value of the chosen set. Values are scaled down by K = eps*vmax/n, so the DP table has
/// at most n^2/eps entries instead of depending on the magnitude of the values.
fn knapsack_fptas(items: &[(u64, u64)], cap: u64, eps: f64) -> u64 {
    let fit: Vec<usize> = (0..items.len()).filter(|&i| items[i].1 <= cap).collect();
    if fit.is_empty() {
        return 0;
    }
    let n = fit.len() as f64;
    let vmax = fit.iter().map(|&i| items[i].0).max().unwrap() as f64;
    let k = eps * vmax / n;
    let scaled: Vec<usize> = fit
        .iter()
        .map(|&i| (items[i].0 as f64 / k).floor() as usize)
        .collect();
    let total: usize = scaled.iter().sum();
    // dp[s] = (minimum weight to reach scaled value s, the chosen subset as a bit mask)
    let mut dp: Vec<Option<(u64, u32)>> = vec![None; total + 1];
    dp[0] = Some((0, 0));
    for (j, &i) in fit.iter().enumerate() {
        for s in (scaled[j]..=total).rev() {
            if let Some((w, mask)) = dp[s - scaled[j]] {
                let cand = (w + items[i].1, mask | 1 << j);
                if cand.0 <= cap && dp[s].is_none_or(|(w0, _)| cand.0 < w0) {
                    dp[s] = Some(cand);
                }
            }
        }
    }
    let (_, mask) = dp.iter().rev().flatten().next().copied().unwrap();
    (0..fit.len())
        .filter(|&j| mask >> j & 1 == 1)
        .map(|j| items[fit[j]].0)
        .sum()
}

fn knapsack_demo() {
    let mut rng = Rng::new(6);
    let mut worst = [1f64; 3];
    for _ in 0..1500 {
        let n = 1 + rng.below(10);
        let items: Vec<(u64, u64)> = (0..n)
            .map(|_| (1 + rng.below(1000) as u64, 1 + rng.below(20) as u64))
            .collect();
        let cap = 5 + rng.below(60) as u64;
        let opt = knapsack_exact(&items, cap);
        for (slot, eps) in [0.5, 0.25, 0.1].into_iter().enumerate() {
            let v = knapsack_fptas(&items, cap, eps);
            assert!(v <= opt, "FPTAS value cannot exceed OPT");
            assert!(
                v as f64 >= (1.0 - eps) * opt as f64 - 1e-9,
                "eps={eps}: {v} < (1-eps)*{opt}"
            );
            if opt > 0 {
                worst[slot] = worst[slot].min(v as f64 / opt as f64);
            }
        }
    }
    println!(
        "knapsack FPTAS: worst value/OPT {:.3} (eps .5), {:.3} (eps .25), {:.3} (eps .1); bounds .5/.75/.9",
        worst[0], worst[1], worst[2]
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 7. Load balancing: LPT.
// ---------------------------------------------------------------------------------------------------------------------
fn lpt(jobs: &[u64], m: usize) -> u64 {
    let mut sorted = jobs.to_vec();
    sorted.sort_by(|a, b| b.cmp(a));
    let mut load = vec![0u64; m];
    for j in sorted {
        let i = (0..m).min_by_key(|&i| load[i]).unwrap();
        load[i] += j;
    }
    *load.iter().max().unwrap()
}

fn makespan_opt(jobs: &[u64], m: usize) -> u64 {
    fn go(i: usize, jobs: &[u64], load: &mut Vec<u64>, best: &mut u64) {
        let cur = *load.iter().max().unwrap();
        if cur >= *best {
            return;
        }
        if i == jobs.len() {
            *best = cur;
            return;
        }
        for k in 0..load.len() {
            load[k] += jobs[i];
            go(i + 1, jobs, load, best);
            load[k] -= jobs[i];
        }
    }
    let mut best = u64::MAX;
    go(0, jobs, &mut vec![0; m], &mut best);
    best
}

fn lpt_demo() {
    let mut rng = Rng::new(7);
    let mut worst = 0f64;
    for _ in 0..1500 {
        let m = 2 + rng.below(3);
        let n = 1 + rng.below(8);
        let jobs: Vec<u64> = (0..n).map(|_| 1 + rng.below(30) as u64).collect();
        let (l, o) = (lpt(&jobs, m), makespan_opt(&jobs, m));
        // (4/3 - 1/(3m)) * OPT = (4m-1)/(3m) * OPT, kept in integers
        assert!(
            3 * m as u64 * l <= (4 * m as u64 - 1) * o,
            "LPT {l} vs OPT {o}, m={m}, jobs={jobs:?}"
        );
        worst = worst.max(l as f64 / o as f64);
    }
    // Graham's tight instance for m=2: jobs 3,3,2,2,2 -> LPT 7, OPT 6 = exactly 7/6 = 4/3 - 1/6.
    assert_eq!(
        (lpt(&[3, 3, 2, 2, 2], 2), makespan_opt(&[3, 3, 2, 2, 2], 2)),
        (7, 6)
    );
    println!(
        "LPT: worst ratio {worst:.3}; tight m=2 instance {{3,3,2,2,2}} gives 7/6 = 4/3 - 1/(3m)"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 8. Karger's contraction algorithm.
// ---------------------------------------------------------------------------------------------------------------------
fn find(p: &mut Vec<usize>, x: usize) -> usize {
    if p[x] != x {
        let r = find(p, p[x]);
        p[x] = r;
    }
    p[x]
}

/// One contraction run: processing edges in a uniformly random order and skipping self-loops is the same as repeatedly
/// contracting a uniformly random remaining edge.
fn karger_once(n: usize, edges: &[Edge], rng: &mut Rng) -> usize {
    let mut order: Vec<usize> = (0..edges.len()).collect();
    rng.shuffle(&mut order);
    let mut parent: Vec<usize> = (0..n).collect();
    let mut comps = n;
    for &i in &order {
        if comps == 2 {
            break;
        }
        let (a, b) = (find(&mut parent, edges[i].0), find(&mut parent, edges[i].1));
        if a != b {
            parent[a] = b;
            comps -= 1;
        }
    }
    edges
        .iter()
        .filter(|&&(a, b)| find(&mut parent, a) != find(&mut parent, b))
        .count()
}

fn min_cut_opt(n: usize, edges: &[Edge]) -> usize {
    (1u32..1 << (n - 1))
        .map(|m| {
            edges
                .iter()
                .filter(|&&(a, b)| (m >> a ^ m >> b) & 1 == 1)
                .count()
        })
        .min()
        .unwrap()
}

fn is_connected(n: usize, edges: &[Edge]) -> bool {
    let mut parent: Vec<usize> = (0..n).collect();
    for &(a, b) in edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        parent[ra] = rb;
    }
    (0..n).all(|v| find(&mut parent, v) == find(&mut parent, 0))
}

fn karger_demo() {
    let mut rng = Rng::new(8);
    let n = 8;
    let bound = 2.0 / (n * (n - 1)) as f64;
    let mut graphs = 0;
    while graphs < 6 {
        let edges = random_graph(&mut rng, n, 0.35 + 0.1 * graphs as f64);
        if !is_connected(n, &edges) {
            continue;
        }
        graphs += 1;
        let opt = min_cut_opt(n, &edges);
        let trials = 20_000;
        let hits = (0..trials)
            .filter(|_| karger_once(n, &edges, &mut rng) == opt)
            .count();
        let phat = hits as f64 / trials as f64;
        let sigma = (phat * (1.0 - phat) / trials as f64).sqrt();
        // single run never returns less than the min cut, and finds it with probability >= 2/(n(n-1))
        assert!(
            phat + 5.0 * sigma >= bound,
            "graph {graphs}: success {phat} below bound {bound}"
        );
        println!(
            "  karger graph {graphs}: m={} min cut={opt}  single-run success {phat:.3} (guarantee >= {bound:.4})",
            edges.len()
        );
    }
    // Amplification: C(n,2) ln n independent runs, keep the best; failure probability <= 1/n.
    let runs = ((n * (n - 1) / 2) as f64 * (n as f64).ln()).ceil() as usize;
    let edges = loop {
        let e = random_graph(&mut rng, n, 0.4);
        if is_connected(n, &e) {
            break e;
        }
    };
    let opt = min_cut_opt(n, &edges);
    let reps = 2000;
    let fails = (0..reps)
        .filter(|_| {
            (0..runs)
                .map(|_| karger_once(n, &edges, &mut rng))
                .min()
                .unwrap()
                != opt
        })
        .count();
    let rate = fails as f64 / reps as f64;
    assert!(
        rate <= 1.0 / n as f64 + 0.04,
        "amplified failure rate {rate} exceeds 1/n"
    );
    println!(
        "karger amplification: {runs} runs per attempt, failure rate {rate:.4} over {reps} attempts (bound 1/n = {:.3})",
        1.0 / n as f64
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 9. Randomised quicksort and quickselect: expected comparison counts.
// ---------------------------------------------------------------------------------------------------------------------
fn quicksort(a: &mut [i64], rng: &mut Rng, random_pivot: bool, cmps: &mut u64) {
    if a.len() <= 1 {
        return;
    }
    let p = if random_pivot { rng.below(a.len()) } else { 0 };
    a.swap(0, p);
    let mut i = 1;
    for j in 1..a.len() {
        if a[j] < a[0] {
            a.swap(i, j);
            i += 1;
        }
    }
    *cmps += a.len() as u64 - 1;
    a.swap(0, i - 1);
    let (lo, hi) = a.split_at_mut(i - 1);
    quicksort(lo, rng, random_pivot, cmps);
    quicksort(&mut hi[1..], rng, random_pivot, cmps);
}

fn quickselect(a: &mut [i64], k: usize, rng: &mut Rng, cmps: &mut u64) -> i64 {
    let (mut lo, mut hi) = (0usize, a.len());
    loop {
        if hi - lo == 1 {
            return a[lo];
        }
        let p = lo + rng.below(hi - lo);
        a.swap(lo, p);
        let mut i = lo + 1;
        for j in lo + 1..hi {
            if a[j] < a[lo] {
                a.swap(i, j);
                i += 1;
            }
        }
        *cmps += (hi - lo) as u64 - 1;
        a.swap(lo, i - 1);
        let pivot_pos = i - 1;
        if k == pivot_pos {
            return a[pivot_pos];
        } else if k < pivot_pos {
            hi = pivot_pos;
        } else {
            lo = pivot_pos + 1;
        }
    }
}

fn sorting_demo() {
    let mut rng = Rng::new(9);
    // correctness on 2000 random arrays (with duplicates handled by strict < partition: still terminates and sorts)
    for _ in 0..2000 {
        let n = rng.below(40);
        let a: Vec<i64> = (0..n).map(|_| rng.below(25) as i64).collect();
        let mut sorted = a.clone();
        sorted.sort();
        let mut b = a.clone();
        quicksort(&mut b, &mut rng, true, &mut 0);
        assert_eq!(b, sorted);
        if n > 0 {
            let k = rng.below(n);
            let mut c = a.clone();
            assert_eq!(quickselect(&mut c, k, &mut rng, &mut 0), sorted[k]);
        }
    }
    // expected comparisons for distinct keys
    let n = 1000usize;
    let expected = 2.0 * (n as f64 + 1.0) * harmonic(n) - 4.0 * n as f64;
    let trials = 400;
    let mut total = 0u64;
    for t in 0..trials {
        let mut a: Vec<i64> = (0..n as i64).collect();
        rng.shuffle(&mut a);
        let mut c = 0;
        quicksort(&mut a, &mut rng, true, &mut c);
        total += c;
        let _ = t;
    }
    let mean = total as f64 / trials as f64;
    assert!(
        (mean / expected - 1.0).abs() < 0.02,
        "mean {mean} vs expected {expected}"
    );
    // deterministic first pivot on SORTED input: n(n-1)/2 comparisons exactly
    let mut sorted: Vec<i64> = (0..n as i64).collect();
    let mut c_det = 0;
    quicksort(&mut sorted, &mut rng, false, &mut c_det);
    assert_eq!(c_det, (n * (n - 1) / 2) as u64);
    let mut sorted: Vec<i64> = (0..n as i64).collect();
    let mut c_rand = 0;
    quicksort(&mut sorted, &mut rng, true, &mut c_rand);
    assert!(c_rand < 3 * expected as u64);
    // quickselect: median costs ~ 3.39n comparisons on average
    let mut tot = 0u64;
    for _ in 0..trials {
        let mut a: Vec<i64> = (0..n as i64).collect();
        rng.shuffle(&mut a);
        let mut c = 0;
        assert_eq!(quickselect(&mut a, n / 2, &mut rng, &mut c), (n / 2) as i64);
        tot += c;
    }
    let qs = tot as f64 / trials as f64 / n as f64;
    assert!(qs < 3.8, "quickselect median should cost ~3.4n, got {qs}n");
    println!(
        "quicksort n={n}: mean comparisons {mean:.0} vs exact expectation {expected:.0}; first-pivot on sorted input {c_det} vs random pivot {c_rand}; quickselect median {qs:.2}n"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 10. Miller-Rabin.
// ---------------------------------------------------------------------------------------------------------------------
fn mulmod(a: u64, b: u64, m: u64) -> u64 {
    (a as u128 * b as u128 % m as u128) as u64
}
fn powmod(mut b: u64, mut e: u64, m: u64) -> u64 {
    let mut r = 1 % m;
    b %= m;
    while e > 0 {
        if e & 1 == 1 {
            r = mulmod(r, b, m);
        }
        b = mulmod(b, b, m);
        e >>= 1;
    }
    r
}

/// true = "n passes base a" (n is a strong probable prime to base a); false = a PROVES n composite.
fn passes_base(n: u64, a: u64) -> bool {
    let (mut d, mut s) = (n - 1, 0);
    while d % 2 == 0 {
        d /= 2;
        s += 1;
    }
    let mut x = powmod(a, d, n);
    if x == 1 || x == n - 1 {
        return true;
    }
    for _ in 1..s {
        x = mulmod(x, x, n);
        if x == n - 1 {
            return true;
        }
    }
    false
}

/// Deterministic for all u64: the first 12 primes as bases are a proven sufficient set.
fn is_prime(n: u64) -> bool {
    const BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    if n < 2 {
        return false;
    }
    for &p in &BASES {
        if n % p == 0 {
            return n == p;
        }
    }
    BASES.iter().all(|&a| passes_base(n, a))
}

fn miller_rabin_demo() {
    // Exhaustive liar census for odd composites: at most 1/4 of the bases lie.
    let mut worst = (0f64, 0u64);
    for n in (9u64..5000).step_by(2).filter(|&n| !is_prime(n)) {
        let liars = (2..n - 1).filter(|&a| passes_base(n, a)).count();
        let frac = liars as f64 / (n - 3) as f64;
        assert!(frac <= 0.25 + 1e-12, "n={n}: liar fraction {frac}");
        if frac > worst.0 {
            worst = (frac, n);
        }
    }
    println!(
        "miller-rabin: worst liar fraction among odd composites < 5000 is {:.3} at n={} (theory: <= 0.25)",
        worst.0, worst.1
    );

    // Deterministic test vs sieve.
    let limit = 200_000usize;
    let mut sieve = vec![true; limit];
    sieve[0] = false;
    sieve[1] = false;
    for i in 2..limit {
        if sieve[i] {
            for j in (i * i..limit).step_by(i) {
                sieve[j] = false;
            }
        }
    }
    assert!((0..limit).all(|i| is_prime(i as u64) == sieve[i]));
    assert!(is_prime((1u64 << 61) - 1));
    assert!(
        !is_prime(561),
        "Carmichael number fools the Fermat test but not Miller-Rabin"
    );
    assert!(
        !is_prime(3_215_031_751),
        "strong pseudoprime to bases 2,3,5,7 is caught by the 12-base set"
    );
    assert!(!is_prime(((1u64 << 31) - 1) * ((1u64 << 31) - 1)));

    // Randomised error: n = 2047 is a strong liar for base 2. Measure the miss rate of 2 random bases.
    let n = 2047u64;
    let liars = (2..n - 1).filter(|&a| passes_base(n, a)).count() as f64 / (n - 3) as f64;
    let mut rng = Rng::new(10);
    let trials = 40_000;
    let miss = (0..trials)
        .filter(|_| (0..2).all(|_| passes_base(n, 2 + rng.below((n - 3) as usize) as u64)))
        .count() as f64
        / trials as f64;
    let expect = liars * liars;
    let sigma = (expect * (1.0 - expect) / trials as f64).sqrt();
    assert!(
        (miss - expect).abs() < 5.0 * sigma + 1e-4,
        "miss {miss} vs {expect}"
    );
    println!(
        "miller-rabin n=2047: liar fraction {liars:.4}; two random bases miss it {miss:.5} (expected {expect:.5})"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 11. Freivalds' algorithm.
// ---------------------------------------------------------------------------------------------------------------------
type Mat = Vec<Vec<i64>>;
fn matmul(a: &Mat, b: &Mat) -> Mat {
    let n = a.len();
    (0..n)
        .map(|i| {
            (0..n)
                .map(|j| (0..n).map(|k| a[i][k] * b[k][j]).sum())
                .collect()
        })
        .collect()
}
fn matvec(a: &Mat, v: &[i64]) -> Vec<i64> {
    a.iter()
        .map(|row| row.iter().zip(v).map(|(x, y)| x * y).sum())
        .collect()
}
/// One Freivalds round: compare A(Br) with Cr in O(n^2). `true` = "looks equal".
fn freivalds_round(a: &Mat, b: &Mat, c: &Mat, rng: &mut Rng) -> bool {
    let r: Vec<i64> = (0..a.len()).map(|_| rng.below(2) as i64).collect();
    matvec(a, &matvec(b, &r)) == matvec(c, &r)
}

fn freivalds_demo() {
    let mut rng = Rng::new(11);
    let rand_mat = |rng: &mut Rng, n: usize| -> Mat {
        (0..n)
            .map(|_| (0..n).map(|_| rng.below(10) as i64).collect())
            .collect()
    };
    // Correct products are never rejected (one-sided error).
    for _ in 0..1000 {
        let n = 1 + rng.below(8);
        let (a, b) = (rand_mat(&mut rng, n), rand_mat(&mut rng, n));
        let c = matmul(&a, &b);
        assert!(freivalds_round(&a, &b, &c, &mut rng));
    }
    // A product wrong in a single entry: per-round miss probability is exactly 1/2 for 0/1 vectors.
    let n = 6;
    let (a, b) = (rand_mat(&mut rng, n), rand_mat(&mut rng, n));
    let mut c = matmul(&a, &b);
    c[2][3] += 1;
    let trials = 20_000;
    let miss = (0..trials)
        .filter(|_| freivalds_round(&a, &b, &c, &mut rng))
        .count() as f64
        / trials as f64;
    let sigma = (0.25 / trials as f64).sqrt();
    assert!(
        (miss - 0.5).abs() < 5.0 * sigma,
        "per-round miss rate {miss}"
    );
    // 30 rounds: every one of 1500 wrong products is caught (miss probability 2^-30 each).
    for _ in 0..1500 {
        let n = 2 + rng.below(7);
        let (a, b) = (rand_mat(&mut rng, n), rand_mat(&mut rng, n));
        let mut c = matmul(&a, &b);
        let (i, j) = (rng.below(n), rng.below(n));
        c[i][j] += 1 + rng.below(5) as i64;
        assert!(
            !(0..30).all(|_| freivalds_round(&a, &b, &c, &mut rng)),
            "wrong product accepted"
        );
    }
    println!(
        "freivalds: per-round miss rate {miss:.3} (expect 0.5); 30 rounds caught all 1500 wrong products"
    );
}

fn main() {
    println!("== approximation ==");
    vertex_cover_demo();
    set_cover_demo();
    tsp_demo();
    max_cut_demo();
    k_center_demo();
    knapsack_demo();
    lpt_demo();
    println!("== randomised ==");
    karger_demo();
    sorting_demo();
    miller_rabin_demo();
    freivalds_demo();
    let _unused: HashMap<u8, u8> = HashMap::new();
    println!("all approximation ratios and probabilities verified");
}
