//! RECIPE 212 (algorithms): heuristics and approximation zoo, each checked against an exact solver on small instances.
//! LEVEL: advanced
//! Use in: NP-hard optimisation you cannot solve exactly at scale (packing, covering, routing, join ordering).   Run: cargo run --release --example heuristics_zoo
#![allow(dead_code)]
//!
//! WHAT: when the exact problem is NP-hard you pick one of three tools. The trick that makes them trustworthy is to test every
//! one against an EXACT solver on tiny seeded instances, so claims like "within 11/9 OPT" are asserted rather than believed.
//!
//!  | kind                       | guarantee                 | here                                            | typical use                              |
//!  |----------------------------|---------------------------|-------------------------------------------------|------------------------------------------|
//!  | exact (B&B, DP, Held-Karp) | optimal, exponential cost | bin packing B&B, subset enumeration, Held-Karp  | small n, ground truth for tests          |
//!  | approximation algorithm    | PROVEN ratio to OPT       | FFD <= 11/9 OPT+1, greedy set cover <= H(n),    | schedulers, covering, when you need a    |
//!  |                            |                           | MST-doubling TSP <= 2 OPT (metric)              | worst-case promise                       |
//!  | heuristic / metaheuristic  | none (empirically good)   | 2-opt, annealing, restarts, beam, genetic       | many-join optimisers, routing, tuning    |
//!
//! Where used: PostgreSQL optimises joins with exhaustive dynamic programming for few tables and switches to a genetic
//! algorithm (GEQO) above a table-count threshold; compilers allocate registers by graph-colouring heuristics or linear scan
//! because optimal allocation is NP-hard; bin-packing style first-fit/best-fit heuristics are the core of resource placement
//! and cutting-stock tools; A* with an admissible heuristic is exact, with an inadmissible one it trades optimality for speed.
//! HOW: seeded xorshift everywhere (deterministic); every section ends with assert!s on ratios against the exact answer.
//! COMMON USES: query join ordering, VM/container placement, cutting stock, vehicle routing, test-suite minimisation (set cover),
//! chip placement (annealing), feature selection (beam/genetic), pathfinding in games and robotics.
//! Read: https://15445.courses.cs.cmu.edu/ (join ordering), https://www.cs.cmu.edu/~15721/ , https://theory.stanford.edu/~amitp/GameProgramming/Heuristics.html
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
// ================================================================ 1. bin packing
fn ffd(items: &[u32], cap: u32) -> usize {
    let mut v = items.to_vec();
    v.sort_by(|a, b| b.cmp(a));
    let mut bins: Vec<u32> = vec![];
    for x in v {
        match bins.iter_mut().find(|b| **b + x <= cap) {
            Some(b) => *b += x,
            None => bins.push(x),
        }
    }
    bins.len()
}
/// Exact branch and bound: place items (largest first) into an existing bin or a new one; prune when bins >= best.
fn bp_exact(items: &[u32], cap: u32) -> usize {
    let mut v = items.to_vec();
    v.sort_by(|a, b| b.cmp(a));
    let mut best = ffd(items, cap); // FFD gives a first upper bound
    fn go(v: &[u32], i: usize, bins: &mut Vec<u32>, cap: u32, best: &mut usize) {
        if bins.len() >= *best {
            return;
        }
        if i == v.len() {
            *best = bins.len();
            return;
        }
        for b in 0..bins.len() {
            if bins[b] + v[i] <= cap {
                bins[b] += v[i];
                go(v, i + 1, bins, cap, best);
                bins[b] -= v[i];
            }
        }
        bins.push(v[i]);
        go(v, i + 1, bins, cap, best);
        bins.pop();
    }
    go(&v, 0, &mut vec![], cap, &mut best);
    best
}
// ================================================================ 2. set cover
fn greedy_cover(universe: u32, sets: &[u32]) -> usize {
    let (mut covered, mut used) = (0u32, 0);
    while covered != universe {
        let best = sets
            .iter()
            .max_by_key(|s| (**s & !covered).count_ones())
            .unwrap();
        covered |= best;
        used += 1;
    }
    used
}
fn exact_cover(universe: u32, sets: &[u32]) -> usize {
    let mut best = sets.len();
    for mask in 0u32..(1 << sets.len()) {
        let k = mask.count_ones() as usize;
        if k >= best {
            continue;
        }
        let cov = (0..sets.len())
            .filter(|i| mask >> i & 1 == 1)
            .fold(0, |a, i| a | sets[i]);
        if cov == universe {
            best = k;
        }
    }
    best
}
// ================================================================ 3. metric TSP
type Pts = Vec<(f64, f64)>;
fn d(p: &Pts, a: usize, b: usize) -> f64 {
    ((p[a].0 - p[b].0).powi(2) + (p[a].1 - p[b].1).powi(2)).sqrt()
}
fn tour_len(p: &Pts, t: &[usize]) -> f64 {
    (0..t.len()).map(|i| d(p, t[i], t[(i + 1) % t.len()])).sum()
}
fn rand_pts(r: &mut Rng, n: usize) -> Pts {
    (0..n).map(|_| (r.f64() * 100.0, r.f64() * 100.0)).collect()
}
fn held_karp(p: &Pts) -> f64 {
    let n = p.len();
    let full = 1usize << (n - 1); // city n-1 is the fixed start; subsets over the other n-1 cities
    let mut dp = vec![vec![f64::INFINITY; n - 1]; full];
    for j in 0..n - 1 {
        dp[1 << j][j] = d(p, n - 1, j);
    }
    for s in 1..full {
        for j in 0..n - 1 {
            if s >> j & 1 == 0 || !dp[s][j].is_finite() {
                continue;
            }
            for k in 0..n - 1 {
                if s >> k & 1 == 1 {
                    continue;
                }
                let v = dp[s][j] + d(p, j, k);
                let t = &mut dp[s | 1 << k][k];
                if v < *t {
                    *t = v;
                }
            }
        }
    }
    (0..n - 1)
        .map(|j| dp[full - 1][j] + d(p, j, n - 1))
        .fold(f64::INFINITY, f64::min)
}
/// 2-approximation: MST (Prim), then a preorder walk (shortcuts via the triangle inequality). Tour <= 2*MST <= 2*OPT.
fn mst_double_tour(p: &Pts) -> Vec<usize> {
    let n = p.len();
    let (mut in_t, mut parent, mut key) = (vec![false; n], vec![0; n], vec![f64::INFINITY; n]);
    key[0] = 0.0;
    for _ in 0..n {
        let u = (0..n)
            .filter(|&i| !in_t[i])
            .min_by(|&a, &b| key[a].partial_cmp(&key[b]).unwrap())
            .unwrap();
        in_t[u] = true;
        for v in 0..n {
            if !in_t[v] && d(p, u, v) < key[v] {
                key[v] = d(p, u, v);
                parent[v] = u;
            }
        }
    }
    let mut kids = vec![vec![]; n];
    for v in 1..n {
        kids[parent[v]].push(v);
    }
    let (mut order, mut stack) = (vec![], vec![0]);
    while let Some(u) = stack.pop() {
        order.push(u); // preorder = the Euler walk with repeated vertices shortcut
        stack.extend(kids[u].iter().rev());
    }
    order
}
/// 2-opt local search: reverse a segment while that shortens the tour; stops at a 2-opt local optimum.
fn two_opt(p: &Pts, mut t: Vec<usize>) -> Vec<usize> {
    let n = t.len();
    loop {
        let mut improved = false;
        for i in 0..n - 1 {
            for j in i + 2..n {
                if i == 0 && j == n - 1 {
                    continue;
                }
                let (a, b, c, e) = (t[i], t[i + 1], t[j], t[(j + 1) % n]);
                if d(p, a, c) + d(p, b, e) < d(p, a, b) + d(p, c, e) - 1e-12 {
                    t[i + 1..=j].reverse();
                    improved = true;
                }
            }
        }
        if !improved {
            return t;
        }
    }
}
/// Simulated annealing with a geometric cooling schedule T <- alpha*T. Accept worse moves with prob exp(-delta/T).
fn anneal(
    p: &Pts,
    r: &mut Rng,
    t0: f64,
    alpha: f64,
    per_temp: usize,
    trace: &mut Vec<(f64, f64)>,
) -> f64 {
    let n = p.len();
    let mut cur: Vec<usize> = (0..n).collect();
    let (mut cur_len, mut best, mut temp) = (tour_len(p, &cur), f64::INFINITY, t0);
    while temp > 0.01 {
        for _ in 0..per_temp {
            let (i, j) = (r.below(n), r.below(n));
            let (i, j) = (i.min(j), i.max(j));
            if i == j {
                continue;
            }
            cur[i..=j].reverse(); // neighbour move: 2-opt style segment reversal
            let new_len = tour_len(p, &cur);
            let delta = new_len - cur_len;
            if delta < 0.0 || r.f64() < (-delta / temp).exp() {
                cur_len = new_len;
                best = best.min(cur_len);
            } else {
                cur[i..=j].reverse(); // undo
            }
        }
        trace.push((temp, cur_len));
        temp *= alpha;
    }
    best
}
/// Beam search: grow partial tours level by level keeping only the `w` cheapest. w=1 is nearest-neighbour; w=inf is exhaustive.
fn beam_tsp(p: &Pts, w: usize) -> f64 {
    let n = p.len();
    let mut beam: Vec<(f64, usize, u32)> = vec![(0.0, 0, 1)]; // (cost, last city, visited mask), start at city 0
    for _ in 1..n {
        let mut next = vec![];
        for &(c, last, mask) in &beam {
            for k in 1..n {
                if mask >> k & 1 == 0 {
                    next.push((c + d(p, last, k), k, mask | 1 << k));
                }
            }
        }
        next.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        next.truncate(w);
        beam = next;
    }
    beam.iter()
        .map(|&(c, last, _)| c + d(p, last, 0))
        .fold(f64::INFINITY, f64::min)
}
// ================================================================ hill climbing with restarts
fn landscape(x: i64) -> f64 {
    // many local maxima (the sine) on top of one broad hill peaking near x=7300
    let xf = x as f64;
    -((xf - 7300.0) / 400.0).powi(2) + 3.0 * (xf / 37.0).sin()
}
fn climb(mut x: i64) -> i64 {
    loop {
        let best = [x - 1, x + 1]
            .into_iter()
            .filter(|c| (0..10_000).contains(c))
            .max_by(|a, b| landscape(*a).partial_cmp(&landscape(*b)).unwrap());
        match best {
            Some(c) if landscape(c) > landscape(x) => x = c,
            _ => return x,
        }
    }
}
// ================================================================ genetic algorithm on 0/1 knapsack
fn knapsack_dp(w: &[usize], v: &[u32], cap: usize) -> u32 {
    let mut dp = vec![0u32; cap + 1];
    for i in 0..w.len() {
        for c in (w[i]..=cap).rev() {
            dp[c] = dp[c].max(dp[c - w[i]] + v[i]);
        }
    }
    dp[cap]
}
fn ga_knapsack(r: &mut Rng, w: &[usize], v: &[u32], cap: usize) -> u32 {
    let n = w.len();
    let fit = |g: u32| -> u32 {
        let (mut ww, mut vv) = (0, 0);
        for i in 0..n {
            if g >> i & 1 == 1 {
                ww += w[i];
                vv += v[i];
            }
        }
        if ww <= cap { vv } else { 0 } // infeasible genomes die
    };
    let mut pop: Vec<u32> = (0..60)
        .map(|_| (r.next() as u32) & ((1 << n) - 1) & (r.next() as u32))
        .collect();
    for _ in 0..120 {
        pop.sort_by_key(|&g| Reverse(fit(g)));
        let mut next = pop[..4].to_vec(); // elitism
        while next.len() < pop.len() {
            let pick = |r: &mut Rng, pop: &[u32]| {
                let (a, b) = (pop[r.below(pop.len())], pop[r.below(pop.len())]);
                if fit(a) >= fit(b) { a } else { b } // tournament of 2
            };
            let (a, b) = (pick(r, &pop), pick(r, &pop));
            let cut = r.below(n);
            let mask = (1u32 << cut) - 1;
            let mut child = (a & mask) | (b & !mask); // one-point crossover
            if r.below(4) == 0 {
                child ^= 1 << r.below(n); // mutation: flip one bit
            }
            next.push(child);
        }
        pop = next;
    }
    pop.iter().map(|&g| fit(g)).max().unwrap()
}
// ================================================================ A* heuristic checker
struct Grid {
    n: usize,
    wall: Vec<bool>,
}
impl Grid {
    fn random(r: &mut Rng, n: usize, density: f64) -> Grid {
        let mut wall: Vec<bool> = (0..n * n).map(|_| r.f64() < density).collect();
        wall[0] = false;
        wall[n * n - 1] = false;
        Grid { n, wall }
    }
    fn nbrs(&self, i: usize) -> Vec<usize> {
        let (x, y, n) = (i % self.n, i / self.n, self.n);
        let mut v = vec![];
        if x > 0 {
            v.push(i - 1)
        }
        if x + 1 < n {
            v.push(i + 1)
        }
        if y > 0 {
            v.push(i - n)
        }
        if y + 1 < n {
            v.push(i + n)
        }
        v.retain(|&j| !self.wall[j]);
        v
    }
    /// True distance from every cell to `goal` (BFS), usize::MAX when unreachable.
    fn dist_to(&self, goal: usize) -> Vec<usize> {
        let mut dist = vec![usize::MAX; self.n * self.n];
        dist[goal] = 0;
        let mut q = VecDeque::from([goal]);
        while let Some(u) = q.pop_front() {
            for v in self.nbrs(u) {
                if dist[v] == usize::MAX {
                    dist[v] = dist[u] + 1;
                    q.push_back(v);
                }
            }
        }
        dist
    }
    fn astar(&self, goal: usize, h: &dyn Fn(usize) -> usize) -> Option<usize> {
        let mut g = vec![usize::MAX; self.n * self.n];
        g[0] = 0;
        let mut open = BinaryHeap::from([Reverse((h(0), 0usize))]);
        while let Some(Reverse((_, u))) = open.pop() {
            if u == goal {
                return Some(g[u]); // optimal only if h is admissible
            }
            for v in self.nbrs(u) {
                if g[u] + 1 < g[v] {
                    g[v] = g[u] + 1; // re-opens nodes, needed when h is inconsistent
                    open.push(Reverse((g[v] + h(v), v)));
                }
            }
        }
        None
    }
}
fn manhattan(n: usize, goal: usize) -> impl Fn(usize) -> usize {
    move |i| (i % n).abs_diff(goal % n) + (i / n).abs_diff(goal / n)
}
/// (admissible, consistent) over all reachable cells.
fn check_heuristic(g: &Grid, goal: usize, h: &dyn Fn(usize) -> usize) -> (bool, bool) {
    let dist = g.dist_to(goal);
    let (mut adm, mut cons) = (true, true);
    for u in 0..g.n * g.n {
        if g.wall[u] || dist[u] == usize::MAX {
            continue;
        }
        adm &= h(u) <= dist[u]; // never overestimate the true remaining cost
        for v in g.nbrs(u) {
            cons &= h(u) <= 1 + h(v); // triangle inequality along every unit-cost edge
        }
    }
    (adm, cons)
}
fn main() {
    let mut r = Rng(0xDEAD_BEEF_1234_5678);
    // 1. bin packing
    let (mut worst, mut tight) = (0.0f64, 0);
    for _ in 0..300 {
        let n = 6 + r.below(7); // 6..=12 items
        let items: Vec<u32> = (0..n).map(|_| 10 + r.below(55) as u32).collect();
        let (h, opt) = (ffd(&items, 100), bp_exact(&items, 100));
        assert!(
            opt <= h && h * 9 <= 11 * opt + 9,
            "FFD bound violated: {h} vs {opt}"
        ); // FFD <= 11/9 OPT + 1
        worst = worst.max(h as f64 / opt as f64);
        tight += (h == opt) as usize;
    }
    println!(
        "bin packing: FFD optimal in {tight}/300 instances, worst ratio {worst:.3} (bound 11/9 OPT + 1)"
    );
    // 2. set cover
    let h_n = |n: u32| (1..=n).map(|i| 1.0 / i as f64).sum::<f64>();
    let mut worst = 0.0f64;
    for _ in 0..300 {
        let (n, m) = (8 + r.below(5) as u32, 6 + r.below(8));
        let universe = (1u32 << n) - 1;
        let mut sets: Vec<u32> = (0..m)
            .map(|_| (r.next() as u32) & (r.next() as u32) & universe)
            .collect();
        sets.push(universe & !sets.iter().fold(0, |a, s| a | s)); // guarantee coverage
        sets.retain(|&s| s != 0);
        sets.truncate(14);
        let universe = sets.iter().fold(0, |a, s| a | s);
        let (g, opt) = (greedy_cover(universe, &sets), exact_cover(universe, &sets));
        assert!(
            g >= opt && g as f64 <= opt as f64 * h_n(universe.count_ones()) + 1e-9,
            "greedy cover exceeded H(n)"
        );
        worst = worst.max(g as f64 / opt as f64);
    }
    println!(
        "set cover: greedy worst ratio {worst:.2} (guarantee H(n) = {:.2} for n=12)",
        h_n(12)
    );
    // 3. metric TSP: MST doubling, 2-opt, beam, annealing vs Held-Karp
    let (mut r_mst, mut r_2opt, mut r_nn, mut r_beam, mut worst_mst) = (0.0, 0.0, 0.0, 0.0, 0.0f64);
    let trials = 60;
    for _ in 0..trials {
        let p = rand_pts(&mut r, 10);
        let opt = held_karp(&p);
        let mst = mst_double_tour(&p);
        let (lm, l2) = (tour_len(&p, &mst), tour_len(&p, &two_opt(&p, mst.clone())));
        assert!(
            lm <= 2.0 * opt + 1e-9 && lm >= opt - 1e-9,
            "MST doubling must be within 2*OPT"
        );
        assert!(l2 <= lm + 1e-9 && l2 >= opt - 1e-9);
        let (nn, beam) = (beam_tsp(&p, 1), beam_tsp(&p, 40));
        assert!(nn >= opt - 1e-9 && beam >= opt - 1e-9);
        r_mst += lm / opt;
        r_2opt += l2 / opt;
        r_nn += nn / opt;
        r_beam += beam / opt;
        worst_mst = worst_mst.max(lm / opt);
    }
    let t = trials as f64;
    println!(
        "TSP n=10 avg tour/OPT: MST-doubling {:.3} (worst {worst_mst:.3} <= 2), +2-opt {:.3}, beam w=1 {:.3}, beam w=40 {:.3}",
        r_mst / t,
        r_2opt / t,
        r_nn / t,
        r_beam / t
    );
    assert!(
        r_2opt <= r_mst && r_beam <= r_nn,
        "local search and wider beams help on average"
    );
    let p8 = rand_pts(&mut r, 8);
    assert!(
        (beam_tsp(&p8, 1_000_000) - held_karp(&p8)).abs() < 1e-9,
        "unbounded beam = exhaustive = exact"
    );
    // simulated annealing, geometric cooling
    let p = rand_pts(&mut r, 11);
    let opt = held_karp(&p);
    let random_avg = (0..200)
        .map(|_| {
            let mut t: Vec<usize> = (0..11).collect();
            for i in (1..11).rev() {
                t.swap(i, r.below(i + 1));
            }
            tour_len(&p, &t)
        })
        .sum::<f64>()
        / 200.0;
    let mut trace = vec![];
    let best = anneal(&p, &mut r, 50.0, 0.90, 400, &mut trace);
    println!("\nannealing n=11 (T0=50, alpha=0.90, 400 moves/temp): cost vs temperature");
    for k in (0..trace.len()).step_by(trace.len() / 8) {
        println!("  T={:>8.3}  current tour {:>7.2}", trace[k].0, trace[k].1);
    }
    println!(
        "  random avg {random_avg:.1}, annealed best {best:.2}, optimal {opt:.2} (ratio {:.3})",
        best / opt
    );
    assert!(
        best < random_avg * 0.6,
        "annealing must beat random tours clearly"
    );
    assert!(
        best <= opt * 1.10 + 1e-9,
        "annealing should be close to optimal at n=11"
    );
    assert!(
        trace.first().unwrap().1 > trace.last().unwrap().1,
        "tour cost falls as the system cools"
    );
    // hill climbing with random restarts
    let global = (0..10_000).map(landscape).fold(f64::NEG_INFINITY, f64::max);
    let single = landscape(climb(1000));
    let (mut best_r, mut local_maxima) = (f64::NEG_INFINITY, std::collections::BTreeSet::new());
    for _ in 0..100 {
        let x = climb(r.below(10_000) as i64);
        local_maxima.insert(x);
        best_r = best_r.max(landscape(x));
    }
    println!(
        "\nhill climbing: global max {global:.2}; one start {single:.2}; 100 restarts {best_r:.2} ({} distinct local maxima found)",
        local_maxima.len()
    );
    assert!(
        local_maxima.len() > 5 && single < global - 1.0,
        "a single climb gets stuck in a local maximum"
    );
    assert!(
        best_r >= single && global - best_r < 1.0,
        "restarts escape the trap"
    );
    // genetic algorithm vs knapsack DP
    let (mut worst_ga, trials) = (1.0f64, 20);
    for _ in 0..trials {
        let n = 18;
        let w: Vec<usize> = (0..n).map(|_| 5 + r.below(30)).collect();
        let v: Vec<u32> = (0..n).map(|_| 10 + r.below(90) as u32).collect();
        let cap = w.iter().sum::<usize>() / 2;
        let (opt, ga) = (knapsack_dp(&w, &v, cap), ga_knapsack(&mut r, &w, &v, cap));
        assert!(
            ga <= opt,
            "a feasible GA solution can never beat the exact optimum"
        );
        worst_ga = worst_ga.min(ga as f64 / opt as f64);
    }
    println!("genetic knapsack n=18: worst GA/OPT over {trials} instances = {worst_ga:.3}");
    assert!(worst_ga >= 0.92);
    // A* heuristic admissibility / consistency checker
    let (mut suboptimal, mut checked) = (0, 0);
    for _ in 0..200 {
        let g = Grid::random(&mut r, 12, 0.25);
        let goal = g.n * g.n - 1;
        let man = manhattan(g.n, goal);
        let inflated = |i: usize| 3 * man(i);
        assert_eq!(
            check_heuristic(&g, goal, &man),
            (true, true),
            "Manhattan is admissible and consistent on a 4-connected unit grid"
        );
        let Some(opt) = g.astar(goal, &man) else {
            continue;
        };
        assert_eq!(
            opt,
            g.dist_to(goal)[0],
            "A* with an admissible heuristic is optimal"
        );
        checked += 1;
        let (adm, _) = check_heuristic(&g, goal, &inflated);
        let weighted = g.astar(goal, &inflated).unwrap();
        assert!(
            weighted >= opt && weighted <= 3 * opt,
            "weighted A* is bounded by w * OPT"
        );
        if weighted > opt {
            suboptimal += 1;
            assert!(
                !adm,
                "only an inadmissible heuristic can return a suboptimal path"
            );
        }
    }
    println!(
        "\nA* on {checked} random 12x12 grids: Manhattan admissible+consistent everywhere; 3x Manhattan returned a longer path on {suboptimal} grids"
    );
    assert!(suboptimal > 0);
    println!("all assertions passed");
}
