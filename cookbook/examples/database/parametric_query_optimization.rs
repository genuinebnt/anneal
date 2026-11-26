//! RECIPE 762 (database): parametric query optimization: plan diagrams, anorexic reduction, robust plans, bouquets, generic vs custom plans.
//! LEVEL: advanced
//! Use in: prepared-statement plan caches, optimizers for parameterised workloads, robust-query-processing research, plan-regression tooling.   Run: cargo run --release --example parametric_query_optimization
//! COMMON USES: PostgreSQL prepared statements (`plan_cache_mode`: five custom plans, then compare against the generic plan), SQL Server
//! parameter sniffing and Query Store plan forcing, Oracle adaptive cursor sharing (several plans per cursor keyed by bind selectivity),
//! plan-diagram visualisation tools (Picasso) and the "anorexic" reduction used to decide how many plans to cache per template.
//! Course: CMU 15-799 Query Optimization (Spring 2025), lecture 16 (Parametric Query Optimization)
#![allow(dead_code)]
//!
//! WHAT: a query TEMPLATE `SELECT count(*) FROM R JOIN S ON R.fk = S.id WHERE R.a < :p1 AND S.b < :p2` has an optimal plan that depends on
//! the parameter values through two selectivities s1 = sel(R.a < p1) and s2 = sel(S.b < p2). Seven physical plans compete:
//! A hash join (seq scan R, build hash on filtered S); B the same with an index range scan on R.a; C index scan R then index nested loop into
//! the S primary key; D scan S, probe the R.fk index per qualifying S row (join order flipped); E sort-merge; F and G hash joins that build on
//! the filtered R side (index scan / seq scan) and probe with S.
//! * PLAN DIAGRAM: which plan is cheapest at every point of the selectivity space (printed as ASCII), with the exact switch points of the
//!   one-dimensional slice found by bisection on the cost difference of the two neighbouring plans.
//! * COST MODEL: closed formulas in the row counts R' = s1*|R| and S' = s2*|S|. They are VALIDATED against an instrumented executor: each plan is
//!   run on seeded data, counting work units (sequential tuple 1, random fetch 4, index probe 2, hash build 1.5, ...); the executed counter
//!   must equal the formula and all plans must return the same count as a naive loop.
//! * ANOREXIC REDUCTION (Harish et al., VLDB 2007): plan P may SWALLOW point x when recost(P, x) <= (1+lambda) * opt(x); greedy set cover keeps
//!   the few plans that cover the whole space. The guarantee: every point is served within (1+lambda) of optimal. Recosting a plan at a
//!   foreign point is the primitive of Dutt et al. (SIGMOD 2017).
//! * ROBUST CHOICE: the single plan that minimises the worst-case regret (max over the space of cost / optimal) versus the average-optimal one.
//! * PLAN BOUQUET: with no usable estimate, run plans on iso-cost contours with doubling budgets; sub-optimality is bounded by 4 in 1-D.
//! * GENERIC vs CUSTOM (PostgreSQL): the first five executions of a prepared statement are planned with the actual values (custom plans); then the
//!   average custom cost is compared to the generic plan optimised at default selectivities (1/3), and the cheaper policy sticks.
//!
//! WHY: re-optimising on every execution costs planning time; reusing one plan costs execution time when the parameter moves to a region where
//! another plan is far better. Parametric optimisation quantifies that trade-off and bounds it with a handful of cached plans.
//!
//! NOTE: plans E, F, G are dominated in this cost model (never optimal), which the cost-curve table makes visible; they still matter as
//! recosting candidates and as the reason the diagram has to be computed, not guessed.
//! CHECKS (asserted): executed cost == model cost and equal results for all 7 plans at 40 random points; the diagram has >= 4 distinct plans;
//! the reduced diagram's worst-case penalty over the grid is <= lambda for lambda in {0, 0.1, 0.2, 0.5} and plan counts never increase;
//! the minimax plan's worst regret <= that of any other single plan; the bouquet is within 4x on 2000 params; the PostgreSQL-style rule
//! switches at execution 6 for a stable workload and stays custom for a workload whose custom plans are cheap.
//! Read: Parametric Query Optimization, CMU 15-799 lecture 16 https://15799.courses.cs.cmu.edu/spring2025/schedule.html
//!       Leveraging Re-costing for Online Optimization of Parameterized Queries with Guarantees (Dutt et al., SIGMOD 2017)
//!       Progressive Parametric Query Optimization (Bizarro et al., TKDE 2009)
//!       Design and Analysis of Parametric Query Optimization Algorithms (Ganguly, VLDB 1998)
//!       PostgreSQL PREPARE https://www.postgresql.org/docs/current/sql-prepare.html

use std::collections::HashMap;

const NR: usize = 20_000;
const NS: usize = 4_000;
const DOM: i64 = 100_000; // R.a and S.b are uniform in 0..DOM
const FAN: f64 = (NR / NS) as f64; // every S row is referenced by exactly FAN rows of R
const PLAN_COST: f64 = 300.0; // optimizer work (in executor units) to produce one custom plan

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Log-uniform in [lo, hi]: parameter selectivities span orders of magnitude, so a linear sample would never see the small ones.
    fn log_uniform(&mut self, lo: f64, hi: f64) -> f64 {
        (lo.ln() + self.unit() * (hi.ln() - lo.ln())).exp()
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Data and the instrumented executor (ground truth for the cost model)
// ---------------------------------------------------------------------------------------------------------------------

struct Data {
    a: Vec<i64>,              // R.a
    fk: Vec<u32>,             // R.fk -> S.id
    b: Vec<i64>,              // S.b (S.id is the row number)
    a_index: Vec<(i64, u32)>, // sorted index on R.a
    fk_index: Vec<Vec<u32>>,  // index on R.fk: S id -> R rows
    a_sorted: Vec<i64>,
    b_sorted: Vec<i64>,
}

fn gen_data(seed: u64) -> Data {
    let mut r = Rng(seed);
    let mut fk: Vec<u32> = (0..NR).map(|i| (i % NS) as u32).collect();
    for i in (1..NR).rev() {
        fk.swap(i, r.below(i as u64 + 1) as usize); // shuffle: exact fan-out of 5 per S row, random placement
    }
    let a: Vec<i64> = (0..NR).map(|_| r.below(DOM as u64) as i64).collect();
    let b: Vec<i64> = (0..NS).map(|_| r.below(DOM as u64) as i64).collect();
    let mut a_index: Vec<(i64, u32)> = a.iter().enumerate().map(|(i, &v)| (v, i as u32)).collect();
    a_index.sort();
    let mut fk_index = vec![vec![]; NS];
    fk.iter()
        .enumerate()
        .for_each(|(i, &k)| fk_index[k as usize].push(i as u32));
    let (mut a_sorted, mut b_sorted) = (a.clone(), b.clone());
    a_sorted.sort();
    b_sorted.sort();
    Data {
        a,
        fk,
        b,
        a_index,
        fk_index,
        a_sorted,
        b_sorted,
    }
}

impl Data {
    fn count_a(&self, p1: i64) -> usize {
        self.a_sorted.partition_point(|&v| v < p1)
    }
    fn count_b(&self, p2: i64) -> usize {
        self.b_sorted.partition_point(|&v| v < p2)
    }
    fn naive(&self, p1: i64, p2: i64) -> usize {
        (0..NR)
            .filter(|&i| self.a[i] < p1 && self.b[self.fk[i] as usize] < p2)
            .count()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Plan {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
}
const PLANS: [Plan; 7] = [
    Plan::A,
    Plan::B,
    Plan::C,
    Plan::D,
    Plan::E,
    Plan::F,
    Plan::G,
];

impl Plan {
    fn name(self) -> &'static str {
        match self {
            Plan::A => "HJ(seqR, build S')",
            Plan::B => "HJ(idxR, build S')",
            Plan::C => "INL(idxR -> S.pk)",
            Plan::D => "INL(seqS -> R.fk)",
            Plan::E => "SortMerge",
            Plan::F => "HJ(idxR build, S probe)",
            Plan::G => "HJ(seqR build, S probe)",
        }
    }
    fn letter(self) -> char {
        (b'A' + PLANS.iter().position(|&p| p == self).unwrap() as u8) as char
    }
}

/// Cost model in executor work units as a function of the filtered row counts r1 = R' and s1 = S'.
/// seq tuple 1, index descent 2, random fetch 4, index probe 2 (+1 fetch), hash build 1.5, hash probe 1, sort 0.25 * n * ceil(log2 n).
fn model(p: Plan, r1: f64, s1: f64) -> f64 {
    let (nr, ns) = (NR as f64, NS as f64);
    let sort = if r1 > 1.0 {
        0.25 * r1 * r1.log2().ceil()
    } else {
        0.0
    };
    match p {
        Plan::A => nr + ns + 1.5 * s1 + r1,
        Plan::B => 2.0 + 4.0 * r1 + ns + 1.5 * s1 + r1,
        Plan::C => 2.0 + 4.0 * r1 + 3.0 * r1,
        Plan::D => ns + s1 * (2.0 + 4.0 * FAN),
        Plan::E => nr + ns + sort + r1 + s1,
        Plan::F => 2.0 + 4.0 * r1 + 1.5 * r1 + ns + s1,
        Plan::G => nr + 1.5 * r1 + ns + s1,
    }
}

fn model_sel(p: Plan, s1: f64, s2: f64) -> f64 {
    model(p, s1 * NR as f64, s2 * NS as f64)
}

/// Run a plan and count the same work units the model charges. Returns (count(*), work).
fn execute(d: &Data, p: Plan, p1: i64, p2: i64) -> (usize, f64) {
    let mut w = 0.0;
    let r_seq = |w: &mut f64| -> Vec<usize> {
        *w += NR as f64;
        (0..NR).filter(|&i| d.a[i] < p1).collect()
    };
    let r_idx = |w: &mut f64| -> Vec<usize> {
        *w += 2.0;
        let rows: Vec<usize> = d
            .a_index
            .iter()
            .take_while(|&&(v, _)| v < p1)
            .map(|&(_, i)| i as usize)
            .collect();
        *w += 4.0 * rows.len() as f64;
        rows
    };
    let s_filtered = |w: &mut f64| -> Vec<usize> {
        *w += NS as f64;
        (0..NS).filter(|&i| d.b[i] < p2).collect()
    };
    let count = match p {
        Plan::A | Plan::B => {
            let rows = if p == Plan::A {
                r_seq(&mut w)
            } else {
                r_idx(&mut w)
            };
            let s = s_filtered(&mut w);
            w += 1.5 * s.len() as f64;
            let set: std::collections::HashSet<usize> = s.into_iter().collect();
            w += rows.len() as f64; // probes
            rows.iter()
                .filter(|&&i| set.contains(&(d.fk[i] as usize)))
                .count()
        }
        Plan::C => {
            let rows = r_idx(&mut w);
            w += 3.0 * rows.len() as f64; // probe the S primary key (2) and fetch the row (1)
            rows.iter().filter(|&&i| d.b[d.fk[i] as usize] < p2).count()
        }
        Plan::D => {
            let s = s_filtered(&mut w);
            let mut c = 0;
            for &sid in &s {
                w += 2.0; // probe the R.fk index
                for &ri in &d.fk_index[sid] {
                    w += 4.0; // random fetch of the R row
                    c += (d.a[ri as usize] < p1) as usize;
                }
            }
            c
        }
        Plan::E => {
            let mut rows: Vec<u32> = r_seq(&mut w).into_iter().map(|i| d.fk[i]).collect();
            let n = rows.len() as f64;
            if n > 1.0 {
                w += 0.25 * n * n.log2().ceil();
            }
            rows.sort();
            let s = s_filtered(&mut w); // S is stored in id order, so its filtered stream is already sorted
            w += rows.len() as f64 + s.len() as f64; // merge pass touches both inputs once
            let (mut i, mut c) = (0, 0);
            for &sid in &s {
                while i < rows.len() && (rows[i] as usize) < sid {
                    i += 1;
                }
                while i < rows.len() && rows[i] as usize == sid {
                    c += 1;
                    i += 1;
                }
            }
            c
        }
        Plan::F | Plan::G => {
            let rows = if p == Plan::F {
                r_idx(&mut w)
            } else {
                r_seq(&mut w)
            };
            w += 1.5 * rows.len() as f64; // build a multiset of R' foreign keys
            let mut ht: HashMap<u32, usize> = HashMap::new();
            rows.iter()
                .for_each(|&i| *ht.entry(d.fk[i]).or_default() += 1);
            let s = s_filtered(&mut w);
            w += s.len() as f64; // probes from the filtered S side
            s.iter()
                .map(|&sid| ht.get(&(sid as u32)).copied().unwrap_or(0))
                .sum()
        }
    };
    (count, w)
}

fn validate_model(d: &Data) {
    println!("== cost model vs instrumented executor (40 random parameter pairs, 7 plans) ==");
    let mut r = Rng(40);
    let mut worst: f64 = 0.0;
    for _ in 0..40 {
        let (p1, p2) = (
            (r.log_uniform(1e-4, 1.0) * DOM as f64) as i64,
            (r.log_uniform(1e-3, 1.0) * DOM as f64) as i64,
        );
        let (r1, s1) = (d.count_a(p1) as f64, d.count_b(p2) as f64);
        let truth = d.naive(p1, p2);
        for &p in &PLANS {
            let (c, w) = execute(d, p, p1, p2);
            assert_eq!(
                c, truth,
                "{:?} returned a wrong count at p1={p1} p2={p2}",
                p
            );
            let m = model(p, r1, s1);
            worst = worst.max((w - m).abs() / m.max(1.0));
            assert!(
                (w - m).abs() <= 1e-9 * m.max(1.0),
                "{:?}: executed work {w} != model {m}",
                p
            );
        }
    }
    println!(
        "  all plans agree with a naive loop; executed work equals the model (max relative gap {worst:.1e})"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Plan diagrams
// ---------------------------------------------------------------------------------------------------------------------

fn opt_at(s1: f64, s2: f64) -> (Plan, f64) {
    PLANS
        .iter()
        .map(|&p| (p, model_sel(p, s1, s2)))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .unwrap()
}

fn log_grid(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| (lo.ln() + (hi.ln() - lo.ln()) * i as f64 / (n - 1) as f64).exp())
        .collect()
}

struct Diagram {
    s1: Vec<f64>,
    s2: Vec<f64>,
    plan: Vec<Vec<Plan>>, // [j][i] for s2[j], s1[i]
}

fn build_diagram(n1: usize, n2: usize) -> Diagram {
    let (s1, s2) = (log_grid(1e-4, 1.0, n1), log_grid(1e-3, 1.0, n2));
    let plan = s2
        .iter()
        .map(|&b| s1.iter().map(|&a| opt_at(a, b).0).collect())
        .collect();
    Diagram { s1, s2, plan }
}

fn print_diagram(d: &Diagram, title: &str) {
    println!("{title}");
    println!("  s2=sel(S.b<p2) ^   (columns: s1=sel(R.a<p1) from 1e-4 to 1, log scale)");
    for j in (0..d.s2.len()).rev() {
        let row: String = d.plan[j].iter().map(|p| p.letter()).collect();
        println!("  {:>8.4} | {row}", d.s2[j]);
    }
}

fn distinct(plans: impl Iterator<Item = Plan>) -> Vec<Plan> {
    let mut v: Vec<Plan> = vec![];
    for p in plans {
        if !v.contains(&p) {
            v.push(p);
        }
    }
    v.sort_by_key(|p| p.letter());
    v
}

/// Exact switch point between two plans on the 1-D slice: bisection on the sign of cost(p) - cost(q).
fn crossing(p: Plan, q: Plan, s2: f64, lo: f64, hi: f64) -> f64 {
    let f = |s: f64| model_sel(p, s, s2) - model_sel(q, s, s2);
    let (mut lo, mut hi) = (lo, hi);
    for _ in 0..60 {
        let mid = (lo * hi).sqrt(); // bisect in log space
        if f(lo).signum() == f(mid).signum() {
            lo = mid
        } else {
            hi = mid
        }
    }
    (lo * hi).sqrt()
}

fn section_diagram() -> Diagram {
    println!("\n== plan diagram over (s1, s2): 60 x 20 log grid ==");
    let d = build_diagram(60, 20);
    print_diagram(&d, "  optimal plan per cell:");
    let used = distinct(d.plan.iter().flatten().copied());
    println!(
        "  plans on the diagram: {}",
        used.iter()
            .map(|p| format!("{}={}", p.letter(), p.name()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    assert!(
        used.len() >= 4,
        "the template must exhibit several optimal plans"
    );

    // One-dimensional slice at s2 = 0.2: list the switch points.
    let s2 = 0.2;
    let grid = log_grid(1e-4, 1.0, 4000);
    println!("  switch points along s1 at s2 = {s2}:");
    let mut cur = opt_at(grid[0], s2).0;
    print!("    {} from s1=1e-4", cur.letter());
    for w in grid.windows(2) {
        let nxt = opt_at(w[1], s2).0;
        if nxt != cur {
            print!(
                "; {} from s1={:.5}",
                nxt.letter(),
                crossing(cur, nxt, s2, w[0], w[1])
            );
            cur = nxt;
        }
    }
    println!();
    print_cost_curves(s2);
    d
}

/// Cost of every plan at a few selectivities of the slice, as a multiple of the optimum: shows which plans are never optimal
/// (E, F, G are dominated here) and how steeply a wrong plan degrades, which is why a plan cache needs a regret bound.
fn print_cost_curves(s2: f64) {
    println!("  cost / optimal cost of every plan along the slice s2 = {s2} (1.00 = optimal):");
    print!("    {:>9}", "s1");
    PLANS.iter().for_each(|p| print!(" {:>8}", p.letter()));
    println!();
    for &s1 in &[1e-4, 1e-3, 1e-2, 0.05, 0.13, 0.2, 0.5, 1.0] {
        let best = opt_at(s1, s2).1;
        print!("    {s1:>9.4}");
        PLANS
            .iter()
            .for_each(|&p| print!(" {:>8.2}", model_sel(p, s1, s2) / best));
        println!();
    }
    // The optimal cost is monotone in each selectivity: more rows can never make the best plan cheaper (used by the bouquet).
    let grid = log_grid(1e-4, 1.0, 500);
    assert!(
        grid.windows(2)
            .all(|w| opt_at(w[1], s2).1 >= opt_at(w[0], s2).1 - 1e-9)
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Anorexic reduction
// ---------------------------------------------------------------------------------------------------------------------

/// Greedy set cover: pick the plan that swallows most still-uncovered points, repeat; then every point goes to its cheapest kept plan.
fn reduce(d: &Diagram, lambda: f64) -> (Vec<Plan>, f64) {
    let pts: Vec<(f64, f64, f64)> =
        d.s2.iter()
            .flat_map(|&b| d.s1.iter().map(move |&a| (a, b, opt_at(a, b).1)))
            .collect();
    let cands = distinct(d.plan.iter().flatten().copied());
    let covers =
        |p: Plan, &(a, b, c): &(f64, f64, f64)| model_sel(p, a, b) <= (1.0 + lambda) * c + 1e-9;
    let mut uncovered: Vec<usize> = (0..pts.len()).collect();
    let mut kept: Vec<Plan> = vec![];
    while !uncovered.is_empty() {
        let best = *cands
            .iter()
            .max_by_key(|&&p| {
                (
                    uncovered.iter().filter(|&&i| covers(p, &pts[i])).count(),
                    std::cmp::Reverse(p.letter()),
                )
            })
            .unwrap();
        kept.push(best);
        uncovered.retain(|&i| !covers(best, &pts[i]));
    }
    // Penalty of the reduced diagram: each point is served by the cheapest KEPT plan (recosting).
    let worst = pts
        .iter()
        .map(|&(a, b, c)| {
            kept.iter()
                .map(|&p| model_sel(p, a, b))
                .fold(f64::INFINITY, f64::min)
                / c
                - 1.0
        })
        .fold(0.0, f64::max);
    (kept, worst)
}

fn section_reduction(d: &Diagram) {
    println!("\n== anorexic reduction: kept plans and worst-case penalty over the grid ==");
    let full = distinct(d.plan.iter().flatten().copied()).len();
    let mut last = full;
    for &lambda in &[0.0, 0.1, 0.2, 0.5, 1.0] {
        let (kept, worst) = reduce(d, lambda);
        println!(
            "  lambda {lambda:>4}: {} of {full} plans kept ({}), worst penalty {:.4}",
            kept.len(),
            kept.iter()
                .map(|p| p.letter().to_string())
                .collect::<String>(),
            worst
        );
        assert!(
            worst <= lambda + 1e-9,
            "guarantee violated for lambda {lambda}: {worst}"
        );
        assert!(
            kept.len() <= last,
            "a larger lambda can never need more plans"
        );
        last = kept.len();
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Robust plan choice, bouquet, generic vs custom
// ---------------------------------------------------------------------------------------------------------------------

fn grid_points(d: &Diagram) -> Vec<(f64, f64)> {
    d.s2.iter()
        .flat_map(|&b| d.s1.iter().map(move |&a| (a, b)))
        .collect()
}

fn regrets(p: Plan, pts: &[(f64, f64)]) -> Vec<f64> {
    pts.iter()
        .map(|&(a, b)| model_sel(p, a, b) / opt_at(a, b).1)
        .collect()
}

/// 1-D plan bouquet at fixed s2: contours at doubling budgets; each contour runs the plan that is optimal at the LARGEST selectivity
/// whose optimal cost fits the budget; a run that exceeds its budget is aborted (and its budget wasted). Returns total cost incurred.
fn bouquet_cost(s_true: f64, s2: f64, grid: &[f64]) -> f64 {
    let opt_cost = |s: f64| opt_at(s, s2).1;
    let mut budget = opt_cost(grid[0]);
    let mut spent = 0.0;
    loop {
        // optimal costs are monotone in s, so the contour boundary is the last grid point with opt cost <= budget
        let edge = grid
            .iter()
            .rev()
            .find(|&&s| opt_cost(s) <= budget)
            .copied()
            .unwrap_or(grid[0]);
        let plan = opt_at(edge, s2).0;
        let actual = model_sel(plan, s_true, s2);
        if actual <= budget {
            return spent + actual;
        }
        spent += budget; // aborted at the budget
        budget *= 2.0;
    }
}

fn section_robust(d: &Diagram) {
    println!(
        "\n== robust plan choice over the grid: worst-case regret vs average regret per single plan =="
    );
    let pts = grid_points(d);
    println!(
        "  {:<26} {:>12} {:>12}",
        "plan", "worst regret", "mean regret"
    );
    let mut stats = vec![];
    for &p in &PLANS {
        let r = regrets(p, &pts);
        let (mx, mean) = (
            r.iter().cloned().fold(0.0, f64::max),
            r.iter().sum::<f64>() / r.len() as f64,
        );
        println!(
            "  {:<26} {:>12.2} {:>12.2}",
            format!("{} {}", p.letter(), p.name()),
            mx,
            mean
        );
        stats.push((p, mx, mean));
    }
    let minimax = stats
        .iter()
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .unwrap();
    println!(
        "  minimax-regret plan: {} (worst regret {:.2})",
        minimax.0.letter(),
        minimax.1
    );
    assert!(stats.iter().all(|s| minimax.1 <= s.1 + 1e-12));

    println!(
        "\n== plan bouquet (1-D, s2 = 0.2): cost incurred / optimal cost for 2000 random s1 =="
    );
    let grid = log_grid(1e-4, 1.0, 2000);
    let mut r = Rng(16);
    let ratios: Vec<f64> = (0..2000)
        .map(|_| {
            let s = r.log_uniform(1e-4, 1.0);
            bouquet_cost(s, 0.2, &grid) / opt_at(s, 0.2).1
        })
        .collect();
    let (mean, mx) = (
        ratios.iter().sum::<f64>() / ratios.len() as f64,
        ratios.iter().cloned().fold(0.0, f64::max),
    );
    println!(
        "  mean {mean:.2}x, worst {mx:.2}x (theoretical bound 4x for doubling contours; no selectivity estimate was used)"
    );
    assert!(mx <= 4.0 + 1e-9);
}

/// Cost of one execution under each caching policy, evaluated at the TRUE selectivities (what the executor would really pay).
struct Stream {
    params: Vec<(f64, f64)>,
}

fn section_policies(d: &Diagram) {
    println!(
        "\n== caching policies on 2000 random parameter pairs (execution cost only, then with planning overhead {PLAN_COST}) =="
    );
    let mut r = Rng(2000);
    let st = Stream {
        params: (0..2000)
            .map(|_| (r.log_uniform(1e-4, 1.0), r.log_uniform(1e-3, 1.0)))
            .collect(),
    };
    let opt: Vec<(Plan, f64)> = st.params.iter().map(|&(a, b)| opt_at(a, b)).collect();
    let opt_total: f64 = opt.iter().map(|o| o.1).sum();

    let (kept, _) = reduce(d, 0.2);
    let generic = opt_at(1.0 / 3.0, 1.0 / 3.0).0; // PostgreSQL default selectivity for an inequality with an unknown parameter
    let first = opt[0].0; // "cache the plan of the first execution" (parameter sniffing)
    let pts = grid_points(d);
    let minimax = *PLANS
        .iter()
        .min_by(|&&p, &&q| {
            let m = |x: Plan| regrets(x, &pts).iter().cloned().fold(0.0, f64::max);
            m(p).partial_cmp(&m(q)).unwrap()
        })
        .unwrap();

    // PostgreSQL-style: 5 custom plans, then compare generic cost (estimated at default selectivities) with the average custom cost.
    let pg_switch = |params: &[(f64, f64)]| -> Option<usize> {
        let avg_custom: f64 = params[..5]
            .iter()
            .map(|&(a, b)| opt_at(a, b).1 + PLAN_COST)
            .sum::<f64>()
            / 5.0;
        let generic_est = model_sel(generic, 1.0 / 3.0, 1.0 / 3.0);
        (generic_est <= avg_custom).then_some(5)
    };
    let switch = pg_switch(&st.params);

    let mut rows: Vec<(String, f64, f64, f64)> = vec![];
    let mut add = |name: &str, exec: f64, worst: f64, plan_cost: f64| {
        rows.push((name.to_string(), exec, worst, plan_cost))
    };
    let worst_of = |f: &dyn Fn(usize) -> f64| {
        (0..st.params.len())
            .map(|i| f(i) / opt[i].1)
            .fold(0.0, f64::max)
    };
    let n = st.params.len() as f64;
    let fixed = |p: Plan| {
        (0..st.params.len())
            .map(|i| model_sel(p, st.params[i].0, st.params[i].1))
            .sum::<f64>()
    };
    add(
        "custom every time (re-optimise)",
        opt_total,
        1.0,
        n * PLAN_COST,
    );
    add(
        &format!("cache first plan ({})", first.letter()),
        fixed(first),
        worst_of(&|i| model_sel(first, st.params[i].0, st.params[i].1)),
        PLAN_COST,
    );
    add(
        &format!("generic plan ({})", generic.letter()),
        fixed(generic),
        worst_of(&|i| model_sel(generic, st.params[i].0, st.params[i].1)),
        PLAN_COST,
    );
    add(
        &format!("minimax-regret plan ({})", minimax.letter()),
        fixed(minimax),
        worst_of(&|i| model_sel(minimax, st.params[i].0, st.params[i].1)),
        PLAN_COST,
    );
    let pick_kept = |i: usize| {
        kept.iter()
            .map(|&p| model_sel(p, st.params[i].0, st.params[i].1))
            .fold(f64::INFINITY, f64::min)
    };
    add(
        &format!("{} cached plans, recost (lambda .2)", kept.len()),
        (0..st.params.len()).map(|i| pick_kept(i)).sum(),
        worst_of(&pick_kept),
        kept.len() as f64 * PLAN_COST,
    );
    let pg_exec: f64 = (0..st.params.len())
        .map(|i| {
            if i < 5 || switch.is_none() {
                opt[i].1
            } else {
                model_sel(generic, st.params[i].0, st.params[i].1)
            }
        })
        .sum();
    let pg_plan = if switch.is_some() {
        6.0 * PLAN_COST
    } else {
        n * PLAN_COST
    };
    add(
        "PostgreSQL rule (5 custom, then compare)",
        pg_exec,
        worst_of(&|i| {
            if i < 5 || switch.is_none() {
                opt[i].1
            } else {
                model_sel(generic, st.params[i].0, st.params[i].1)
            }
        }),
        pg_plan,
    );
    println!(
        "  {:<40} {:>12} {:>10} {:>12} {:>14}",
        "policy", "exec cost", "vs opt", "worst/opt", "exec+planning"
    );
    for (name, exec, worst, plan) in &rows {
        println!(
            "  {:<40} {:>12.0} {:>9.1}% {:>12.2} {:>14.0}",
            name,
            exec,
            (exec / opt_total - 1.0) * 100.0,
            worst,
            exec + plan
        );
    }
    println!(
        "  PostgreSQL rule decision after 5 custom executions: {}",
        if switch.is_some() {
            "generic plan wins, switch"
        } else {
            "keep planning custom"
        }
    );
    // reduced diagram mean extra cost on random (off-grid) points stays within lambda
    let reduced_extra = rows[4].1 / opt_total - 1.0;
    assert!(
        reduced_extra <= 0.2 + 1e-9,
        "mean extra cost of the reduced diagram on random params exceeds lambda: {reduced_extra}"
    );
    assert!(
        rows[4].1 <= rows[1].1 + 1e-9 && rows[4].1 <= rows[2].1 + 1e-9,
        "recosting among kept plans must beat single-plan reuse"
    );

    // Scripted checks of the generic/custom rule.
    let stable: Vec<(f64, f64)> = vec![(1.0 / 3.0, 1.0 / 3.0); 8];
    assert_eq!(
        pg_switch(&stable),
        Some(5),
        "stable parameters: the generic plan must be chosen at execution 6"
    );
    let tiny: Vec<(f64, f64)> = vec![(1e-4, 1e-3); 8];
    assert_eq!(
        pg_switch(&tiny),
        None,
        "cheap custom plans must stay custom"
    );
    println!(
        "  scripted: stable params -> switch to generic at execution 6; tiny params -> stay custom"
    );
}

fn main() {
    let data = gen_data(762);
    validate_model(&data);
    let diagram = section_diagram();
    section_reduction(&diagram);
    section_robust(&diagram);
    section_policies(&diagram);
    println!("\nall parametric optimization checks passed");
}
