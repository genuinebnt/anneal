//! RECIPE 891 (algorithms): linear programming: the simplex method, duality, sensitivity, LP models, branch and bound.
//! LEVEL: advanced
//! Use in: resource allocation, scheduling relaxations, flow/matching, any "maximise a linear objective under linear limits" problem.   Run: cargo run --release --example linear_programming_simplex
//! COMMON USES: production planning and diet/blending models, network flow and transport, LP relaxations inside MIP solvers (branch and bound), pricing/revenue management, cluster schedulers that round LP solutions.
//! Course: CMU 15-850 / 15-859 style LP lectures; Chvatal "Linear Programming" chapters 2-5 (tableaus, degeneracy, duality).
#![allow(dead_code)]
//!
//! WHAT: a dense-tableau simplex solver for  max c.x  s.t.  A x <= b,  x >= 0  (rows with b < 0 are allowed).
//!   * Phase 1 / phase 2: rows with negative b are multiplied by -1 and receive an artificial variable;
//!     phase 1 maximises -(sum of artificials); a negative optimum proves INFEASIBILITY. Phase 2 optimises c.
//!   * Two pivot rules: Dantzig (most negative reduced cost, fast in practice, can CYCLE on degenerate vertices)
//!     and Bland (smallest index for entering and leaving variable, provably terminates). Beale's classic example
//!     below cycles under Dantzig and is detected by remembering every basis visited.
//!   * UNBOUNDED is detected when the entering column has no positive entry (the ratio test has no leaving row).
//!   * DUAL values are read off the final tableau (reduced cost of each slack column). We check dual feasibility
//!     (y >= 0, A^T y >= c), strong duality (b.y == c.x), complementary slackness, and cross-check by solving the
//!     dual LP as an independent LP.
//!   * SENSITIVITY: the interval of each b_i over which the final basis stays feasible (so the optimum moves exactly
//!     y_i per unit), and how far a non-basic cost can rise before the variable enters the basis (its reduced cost).
//!   * MODELS: diet problem (>= constraints => phase 1), max-flow as an LP (checked against Edmonds-Karp), bipartite
//!     matching (the LP is integral: totally unimodular matrix), and a tiny branch and bound for integer programs
//!     (0/1 knapsack against a DP oracle, assignment against brute-force permutations).
//! WHY: the simplex method walks along vertices of the feasible polytope, improving the objective each pivot.
//! Degenerate vertices (several bases for one vertex) are where the walk can stall or loop; Bland's rule trades speed
//! for a termination proof. Duality gives a certificate of optimality and economic meaning (shadow prices).
//! Verification: 1500 random small LPs (made bounded by a box row) are compared with brute-force enumeration of all
//! vertices (every choice of n tight constraints among the m rows and n non-negativity bounds).
//! Read: Chvatal, "Linear Programming" (W.H. Freeman, 1983); Beale 1955 "Cycling in the dual simplex algorithm" (title only, not read);
//! https://en.wikipedia.org/wiki/Bland%27s_rule and https://en.wikipedia.org/wiki/Simplex_algorithm and https://en.wikipedia.org/wiki/Linear_programming
//! TRY NEXT:
//!   - [S] In `solve`, change the phase-2 `allowed` bound from `n + m` to `ncols` (let artificial columns re-enter). Predict which random-LP check fails first, then run it.
//!   - [S] Predict, then measure with `iters`, whether Dantzig or Bland needs fewer pivots on the 1500 random LPs; print the two totals and explain the gap.
//!   - [M] Add cost ranging for BASIC variables (needs the row of the basic variable in the final tableau) and test it by re-solving with c_j moved to each end of the range.
//!   - [M] Replace the `Vec<Vec<f64>>` tableau with a revised simplex that keeps only B^-1 and recomputes reduced costs; assert it matches `solve` on the random LPs.
//!   - [A] PROJECT: combine with `max_flow_dinic` (or `parallel_sample_sort_radix` for the sorted-index trick) by building a min-cost-flow LP, solving it here, and comparing with a successive-shortest-paths implementation on random graphs; also port `branch_and_bound` to best-first order using a BinaryHeap keyed on the relaxation bound.
use std::collections::{HashSet, VecDeque};

const EPS: f64 = 1e-9;

// ---------- seeded randomness ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// uniform integer in [lo, hi]
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    fn chance(&mut self, num: u64, den: u64) -> bool {
        self.next() % den < num
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * (1.0 + b.abs())
}

// ---------- the LP type ----------
/// maximise c.x subject to A x <= b, x >= 0
#[derive(Clone, Debug)]
struct Lp {
    a: Vec<Vec<f64>>,
    b: Vec<f64>,
    c: Vec<f64>,
}

impl Lp {
    fn n(&self) -> usize {
        self.c.len()
    }
    fn m(&self) -> usize {
        self.b.len()
    }
    fn objective(&self, x: &[f64]) -> f64 {
        self.c.iter().zip(x).map(|(c, x)| c * x).sum()
    }
    fn is_feasible(&self, x: &[f64], tol: f64) -> bool {
        x.iter().all(|&v| v >= -tol)
            && self
                .a
                .iter()
                .zip(&self.b)
                .all(|(row, &b)| row.iter().zip(x).map(|(a, x)| a * x).sum::<f64>() <= b + tol)
    }
    fn with_row(&self, row: Vec<f64>, rhs: f64) -> Lp {
        let mut l = self.clone();
        l.a.push(row);
        l.b.push(rhs);
        l
    }
    /// The dual as another LP in the same form: min b.y s.t. A^T y >= c, y >= 0
    /// becomes max -b.y s.t. -A^T y <= -c.
    fn dual(&self) -> Lp {
        let (n, m) = (self.n(), self.m());
        let a = (0..n)
            .map(|j| (0..m).map(|i| -self.a[i][j]).collect())
            .collect();
        Lp {
            a,
            b: self.c.iter().map(|c| -c).collect(),
            c: self.b.iter().map(|b| -b).collect(),
        }
    }
}

// ---------- the simplex solver ----------
#[derive(Clone, Copy, PartialEq, Debug)]
enum Rule {
    Dantzig,
    Bland,
}

#[derive(Debug, PartialEq)]
enum Step {
    Optimal,
    Unbounded,
    Cycled,
}

#[derive(Clone, Debug)]
struct Solution {
    x: Vec<f64>,
    obj: f64,
    /// dual values, one per constraint row
    y: Vec<f64>,
    /// reduced costs of the original variables (>= 0 at optimum; 0 for basic ones)
    reduced: Vec<f64>,
    /// final tableau column of slack i (= how the basic values move when b_i grows by 1)
    slack_cols: Vec<Vec<f64>>,
    /// final basic values
    rhs: Vec<f64>,
    artificial_left: bool,
    iters: usize,
}

#[derive(Debug)]
enum Outcome {
    Optimal(Solution),
    Infeasible,
    Unbounded,
    Cycled,
}

impl Outcome {
    fn optimal(self) -> Option<Solution> {
        if let Outcome::Optimal(s) = self {
            Some(s)
        } else {
            None
        }
    }
}

fn pivot(t: &mut [Vec<f64>], basis: &mut [usize], r: usize, c: usize) {
    let p = t[r][c];
    for v in t[r].iter_mut() {
        *v /= p;
    }
    let pivot_row = t[r].clone();
    for (i, row) in t.iter_mut().enumerate() {
        if i == r {
            continue;
        }
        let f = row[c];
        if f != 0.0 {
            for (v, pv) in row.iter_mut().zip(&pivot_row) {
                *v -= f * pv;
            }
        }
    }
    if r < basis.len() {
        basis[r] = c;
    }
}

/// Run simplex iterations on the tableau (last row = objective row `z - c.x = 0`, last column = rhs).
/// Only columns `< allowed` may enter the basis.
fn iterate(
    t: &mut Vec<Vec<f64>>,
    basis: &mut Vec<usize>,
    allowed: usize,
    rule: Rule,
    iters: &mut usize,
) -> Step {
    let m = basis.len();
    let rhs = t[0].len() - 1;
    let mut seen: HashSet<Vec<usize>> = HashSet::new();
    seen.insert(basis.clone());
    loop {
        // entering column: negative reduced cost in the z row
        let mut enter = None;
        for j in 0..allowed {
            if t[m][j] < -EPS {
                match rule {
                    Rule::Bland => {
                        enter = Some(j);
                        break; // smallest index
                    }
                    Rule::Dantzig => {
                        if enter.is_none_or(|e| t[m][j] < t[m][e]) {
                            enter = Some(j); // most negative, first index wins ties
                        }
                    }
                }
            }
        }
        let Some(j) = enter else { return Step::Optimal };
        // leaving row: minimum ratio test; ties broken per rule (this is exactly where cycling lives)
        let mut leave: Option<(usize, f64)> = None;
        for i in 0..m {
            if t[i][j] > EPS {
                let ratio = t[i][rhs] / t[i][j];
                let better = match leave {
                    None => true,
                    Some((li, lr)) => {
                        if ratio < lr - 1e-12 {
                            true
                        } else if (ratio - lr).abs() <= 1e-12 {
                            rule == Rule::Bland && basis[i] < basis[li]
                        } else {
                            false
                        }
                    }
                };
                if better {
                    leave = Some((i, ratio));
                }
            }
        }
        let Some((r, _)) = leave else {
            return Step::Unbounded;
        };
        pivot(t, basis, r, j);
        *iters += 1;
        if !seen.insert(basis.clone()) || *iters > 100_000 {
            return Step::Cycled;
        }
    }
}

fn solve(lp: &Lp, rule: Rule) -> Outcome {
    let (n, m) = (lp.n(), lp.m());
    let flipped: Vec<bool> = lp.b.iter().map(|&b| b < 0.0).collect();
    let k = flipped.iter().filter(|&&f| f).count();
    let ncols = n + m + k;
    let mut t = vec![vec![0.0; ncols + 1]; m + 1];
    let mut basis = vec![0usize; m];
    let mut art = n + m;
    for i in 0..m {
        let s = if flipped[i] { -1.0 } else { 1.0 };
        for j in 0..n {
            t[i][j] = s * lp.a[i][j];
        }
        t[i][n + i] = s; // slack, +1 in the ORIGINAL row
        t[i][ncols] = s * lp.b[i];
        if flipped[i] {
            t[i][art] = 1.0;
            basis[i] = art;
            art += 1;
        } else {
            basis[i] = n + i;
        }
    }
    let mut iters = 0;
    if k > 0 {
        // phase 1: maximise -(sum of artificials)
        for j in n + m..ncols {
            t[m][j] = 1.0;
        }
        for i in 0..m {
            if basis[i] >= n + m {
                let row = t[i].clone();
                for (z, v) in t[m].iter_mut().zip(&row) {
                    *z -= v;
                }
            }
        }
        if iterate(&mut t, &mut basis, n + m, rule, &mut iters) == Step::Cycled {
            return Outcome::Cycled;
        }
        if t[m][ncols] < -1e-7 {
            return Outcome::Infeasible;
        }
        // drive artificial variables out of the basis (they sit at value 0)
        for i in 0..m {
            if basis[i] >= n + m {
                if let Some(j) = (0..n + m).find(|&j| t[i][j].abs() > 1e-9) {
                    pivot(&mut t, &mut basis, i, j);
                } // else: the row is redundant (all zero), leave it
            }
        }
    }
    // phase 2: real objective
    for j in 0..=ncols {
        t[m][j] = 0.0;
    }
    for j in 0..n {
        t[m][j] = -lp.c[j];
    }
    for i in 0..m {
        let z = t[m][basis[i]];
        if z != 0.0 {
            let row = t[i].clone();
            for (zv, v) in t[m].iter_mut().zip(&row) {
                *zv -= z * v;
            }
        }
    }
    match iterate(&mut t, &mut basis, n + m, rule, &mut iters) {
        Step::Unbounded => return Outcome::Unbounded,
        Step::Cycled => return Outcome::Cycled,
        Step::Optimal => {}
    }
    let mut x = vec![0.0; n];
    for i in 0..m {
        if basis[i] < n {
            x[basis[i]] = t[i][ncols];
        }
    }
    Outcome::Optimal(Solution {
        obj: t[m][ncols],
        y: (0..m).map(|i| t[m][n + i]).collect(),
        reduced: (0..n).map(|j| t[m][j]).collect(),
        slack_cols: (0..m)
            .map(|i| (0..m).map(|r| t[r][n + i]).collect())
            .collect(),
        rhs: (0..m).map(|r| t[r][ncols]).collect(),
        artificial_left: basis.iter().any(|&b| b >= n + m),
        x,
        iters,
    })
}

/// Interval [lo, hi] of changes delta to b_i over which the final basis stays feasible.
fn rhs_range(sol: &Solution, i: usize) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for r in 0..sol.rhs.len() {
        let col = sol.slack_cols[i][r];
        if col > EPS {
            lo = lo.max(-sol.rhs[r] / col);
        } else if col < -EPS {
            hi = hi.min(-sol.rhs[r] / col);
        }
    }
    (lo, hi)
}

// ---------- brute-force oracle: enumerate vertices ----------
fn gauss_solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let p = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[p][col].abs() < 1e-10 {
            return None;
        }
        a.swap(col, p);
        b.swap(col, p);
        for r in col + 1..n {
            let f = a[r][col] / a[col][col];
            for c in col..n {
                let v = a[col][c];
                a[r][c] -= f * v;
            }
            b[r] -= f * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|j| a[i][j] * x[j]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    Some(x)
}

/// Best objective over all vertices of {A x <= b, x >= 0}; None if no feasible vertex.
/// Only valid when the polytope is bounded (callers add a box row).
fn brute_force(lp: &Lp) -> Option<f64> {
    let n = lp.n();
    let mut rows: Vec<(Vec<f64>, f64)> = lp.a.iter().cloned().zip(lp.b.iter().cloned()).collect();
    for j in 0..n {
        let mut r = vec![0.0; n];
        r[j] = -1.0;
        rows.push((r, 0.0));
    }
    let total = rows.len();
    let mut best: Option<f64> = None;
    for mask in 0u32..(1 << total) {
        if mask.count_ones() as usize != n {
            continue;
        }
        let chosen: Vec<&(Vec<f64>, f64)> = (0..total)
            .filter(|i| mask >> i & 1 == 1)
            .map(|i| &rows[i])
            .collect();
        let a: Vec<Vec<f64>> = chosen.iter().map(|r| r.0.clone()).collect();
        let b: Vec<f64> = chosen.iter().map(|r| r.1).collect();
        if let Some(x) = gauss_solve(a, b) {
            if lp.is_feasible(&x, 1e-7) {
                let v = lp.objective(&x);
                best = Some(best.map_or(v, |bv: f64| bv.max(v)));
            }
        }
    }
    best
}

fn random_lp(rng: &mut Rng) -> Lp {
    let n = rng.range(2, 3) as usize;
    let m = rng.range(2, 4) as usize;
    let mut a: Vec<Vec<f64>> = (0..m)
        .map(|_| (0..n).map(|_| rng.range(-3, 4) as f64).collect())
        .collect();
    let mut b: Vec<f64> = (0..m).map(|_| rng.range(-2, 10) as f64).collect();
    a.push(vec![1.0; n]); // box row keeps the polytope bounded
    b.push(10.0);
    let c = (0..n).map(|_| rng.range(-3, 5) as f64).collect();
    Lp { a, b, c }
}

// ---------- demos and checks ----------
fn beale_lp() -> Lp {
    // max 3/4 x1 - 20 x2 + 1/2 x3 - 6 x4  (Beale's cycling example, textbook form)
    Lp {
        a: vec![
            vec![0.25, -8.0, -1.0, 9.0],
            vec![0.5, -12.0, -0.5, 3.0],
            vec![0.0, 0.0, 1.0, 0.0],
        ],
        b: vec![0.0, 0.0, 1.0],
        c: vec![0.75, -20.0, 0.5, -6.0],
    }
}

fn demo_basics() {
    // max 3x + 5y  s.t. x <= 4, 2y <= 12, 3x + 2y <= 18   (textbook: optimum 36 at (2, 6))
    let lp = Lp {
        a: vec![vec![1.0, 0.0], vec![0.0, 2.0], vec![3.0, 2.0]],
        b: vec![4.0, 12.0, 18.0],
        c: vec![3.0, 5.0],
    };
    let s = solve(&lp, Rule::Bland).optimal().unwrap();
    println!(
        "textbook LP: obj {} at x = {:?}, duals {:?}",
        s.obj, s.x, s.y
    );
    assert!(close(s.obj, 36.0) && close(s.x[0], 2.0) && close(s.x[1], 6.0));
    assert!(close(s.y[0], 0.0) && close(s.y[1], 1.5) && close(s.y[2], 1.0)); // shadow prices
    let (lo, hi) = rhs_range(&s, 2);
    println!(
        "  b_3 may move in [{lo:.2}, {hi:.2}] with shadow price {} per unit",
        s.y[2]
    );
    assert!(close(lo, -6.0) && close(hi, 6.0));
    let bumped = Lp {
        b: vec![4.0, 12.0, 21.0],
        ..lp.clone()
    };
    assert!(close(
        solve(&bumped, Rule::Bland).optimal().unwrap().obj,
        36.0 + 3.0 * 1.0
    ));

    // unbounded: max x + y s.t. x - y <= 1
    let unb = Lp {
        a: vec![vec![1.0, -1.0]],
        b: vec![1.0],
        c: vec![1.0, 1.0],
    };
    assert!(matches!(solve(&unb, Rule::Bland), Outcome::Unbounded));
    // infeasible: x <= -1 with x >= 0 (needs phase 1 to notice)
    let inf = Lp {
        a: vec![vec![1.0]],
        b: vec![-1.0],
        c: vec![1.0],
    };
    assert!(matches!(solve(&inf, Rule::Bland), Outcome::Infeasible));
    // phase 1 that succeeds: max -x - y  s.t. x + y >= 2 (as -x - y <= -2), x <= 5
    let two = Lp {
        a: vec![vec![-1.0, -1.0], vec![1.0, 0.0]],
        b: vec![-2.0, 5.0],
        c: vec![-1.0, -1.0],
    };
    let s2 = solve(&two, Rule::Bland).optimal().unwrap();
    assert!(close(s2.obj, -2.0));
    println!("basics ok: optimum, shadow prices, unbounded, infeasible, phase 1");
}

fn demo_cycling() {
    let lp = beale_lp();
    let d = solve(&lp, Rule::Dantzig);
    let b = solve(&lp, Rule::Bland);
    println!(
        "Beale's example: Dantzig rule -> {}",
        match &d {
            Outcome::Cycled => "CYCLES (a basis repeats, no progress)".to_string(),
            Outcome::Optimal(s) =>
                format!("optimal {} (did not cycle with this tie-breaking)", s.obj),
            o => format!("{o:?}"),
        }
    );
    let sb = b.optimal().expect("Bland's rule must terminate");
    println!(
        "                 Bland rule   -> optimal {} in {} pivots",
        sb.obj, sb.iters
    );
    assert!(close(sb.obj, 1.25));
    assert!(
        matches!(d, Outcome::Cycled),
        "Beale's LP is the classic example where Dantzig with lowest-index ties cycles"
    );
}

fn check_certificates(lp: &Lp, s: &Solution) {
    assert!(lp.is_feasible(&s.x, 1e-6), "primal feasibility");
    assert!(s.y.iter().all(|&y| y >= -1e-7), "dual feasibility: y >= 0");
    for j in 0..lp.n() {
        let aty: f64 = (0..lp.m()).map(|i| lp.a[i][j] * s.y[i]).sum();
        assert!(
            aty >= lp.c[j] - 1e-6,
            "dual feasibility: A^T y >= c (col {j})"
        );
        // complementary slackness: x_j > 0 => dual constraint tight
        assert!(
            s.x[j].abs() < 1e-7 || (aty - lp.c[j]).abs() < 1e-6,
            "complementary slackness (vars)"
        );
    }
    for i in 0..lp.m() {
        let ax: f64 = lp.a[i].iter().zip(&s.x).map(|(a, x)| a * x).sum();
        assert!(
            s.y[i].abs() < 1e-7 || (lp.b[i] - ax).abs() < 1e-6,
            "complementary slackness (rows)"
        );
    }
    let dual_obj: f64 = lp.b.iter().zip(&s.y).map(|(b, y)| b * y).sum();
    assert!(
        close(dual_obj, s.obj),
        "strong duality: b.y == c.x ({dual_obj} vs {})",
        s.obj
    );
}

fn random_verification() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let (mut opt, mut infeas) = (0, 0);
    let (mut dantzig_pivots, mut bland_pivots, mut cycles) = (0, 0, 0);
    let mut ranged = 0;
    for _ in 0..1500 {
        let lp = random_lp(&mut rng);
        let brute = brute_force(&lp);
        let out = solve(&lp, Rule::Bland);
        match (&out, brute) {
            (Outcome::Optimal(s), Some(v)) => {
                opt += 1;
                assert!(
                    close(s.obj, v),
                    "simplex {} vs brute force {v} on {lp:?}",
                    s.obj
                );
                check_certificates(&lp, s);
                bland_pivots += s.iters;
                // independent dual solve gives the same value
                let dual = solve(&lp.dual(), Rule::Bland)
                    .optimal()
                    .expect("dual of a feasible bounded LP");
                assert!(
                    close(-dual.obj, s.obj),
                    "dual LP optimum equals primal optimum"
                );
                // sensitivity: move b_i inside the allowable range, optimum moves by y_i * delta
                if !s.artificial_left {
                    for i in 0..lp.m() {
                        let (lo, hi) = rhs_range(s, i);
                        for bound in [lo, hi] {
                            if bound.is_finite() && bound.abs() < 1e6 {
                                let delta = 0.9 * bound;
                                let mut moved = lp.clone();
                                moved.b[i] += delta;
                                let ns = solve(&moved, Rule::Bland)
                                    .optimal()
                                    .expect("still feasible inside range");
                                assert!(
                                    close(ns.obj, s.obj + s.y[i] * delta),
                                    "range check row {i}"
                                );
                                ranged += 1;
                            }
                        }
                    }
                    // a non-basic variable only enters once its cost rises by its reduced cost
                    for j in 0..lp.n() {
                        if s.reduced[j] > 1e-7 {
                            let mut c2 = lp.clone();
                            c2.c[j] += 0.9 * s.reduced[j];
                            assert!(close(
                                solve(&c2, Rule::Bland).optimal().unwrap().obj,
                                s.obj + s.x[j] * 0.9 * s.reduced[j]
                            ));
                        }
                    }
                }
                match solve(&lp, Rule::Dantzig) {
                    Outcome::Optimal(d) => {
                        assert!(close(d.obj, s.obj));
                        dantzig_pivots += d.iters;
                    }
                    Outcome::Cycled => cycles += 1,
                    o => panic!("Dantzig disagrees: {o:?}"),
                }
            }
            (Outcome::Infeasible, None) => infeas += 1,
            (o, b) => panic!("mismatch: simplex {o:?} vs brute force {b:?} on {lp:?}"),
        }
    }
    println!(
        "1500 random LPs: {opt} optimal, {infeas} infeasible, all objectives equal brute force; strong duality, complementary slackness, dual-LP solve and {ranged} RHS-range checks passed"
    );
    println!(
        "  pivots (optimal LPs): Bland {bland_pivots}, Dantzig {dantzig_pivots}; Dantzig cycles on {cycles}"
    );
    assert!(opt > 300 && infeas > 20);
}

// ---------- model 1: diet ----------
fn diet() {
    // foods: bread, milk, cheese, potato, fish; nutrients: calories, protein, calcium (all must reach a minimum)
    let cost = [2.0, 3.5, 8.0, 1.5, 11.0];
    let nutrients = [
        [90.0, 120.0, 106.0, 77.0, 190.0],
        [3.0, 8.0, 7.0, 2.0, 22.0],
        [20.0, 285.0, 200.0, 10.0, 15.0],
    ];
    let need = [2000.0, 55.0, 800.0];
    // min cost.x s.t. N x >= need  ->  max -cost.x s.t. -N x <= -need, plus x_j <= 10 servings
    let mut a: Vec<Vec<f64>> = nutrients
        .iter()
        .map(|r| r.iter().map(|v| -v).collect())
        .collect();
    let mut b: Vec<f64> = need.iter().map(|v| -v).collect();
    for j in 0..5 {
        let mut r = vec![0.0; 5];
        r[j] = 1.0;
        a.push(r);
        b.push(10.0);
    }
    let lp = Lp {
        a,
        b,
        c: cost.iter().map(|c| -c).collect(),
    };
    let s = solve(&lp, Rule::Bland).optimal().expect("diet is feasible");
    check_certificates(&lp, &s);
    println!(
        "diet: cheapest plan costs {:.3}, servings {:?}",
        -s.obj,
        s.x.iter()
            .map(|v| (v * 100.0).round() / 100.0)
            .collect::<Vec<_>>()
    );
    println!(
        "  shadow prices (cost per extra unit of nutrient): calories {:.4}, protein {:.4}, calcium {:.4}",
        s.y[0], s.y[1], s.y[2]
    );
    assert!(close(-s.obj, -brute_force(&lp).unwrap()));
    for (i, row) in nutrients.iter().enumerate() {
        let got: f64 = row.iter().zip(&s.x).map(|(a, x)| a * x).sum();
        assert!(got >= need[i] - 1e-6);
    }
}

// ---------- model 2: max flow as an LP ----------
fn edmonds_karp(n: usize, cap: &mut Vec<Vec<i64>>, s: usize, t: usize) -> i64 {
    let mut flow = 0;
    loop {
        let mut prev = vec![usize::MAX; n];
        prev[s] = s;
        let mut q = VecDeque::from([s]);
        while let Some(u) = q.pop_front() {
            for v in 0..n {
                if prev[v] == usize::MAX && cap[u][v] > 0 {
                    prev[v] = u;
                    q.push_back(v);
                }
            }
        }
        if prev[t] == usize::MAX {
            return flow;
        }
        let mut bott = i64::MAX;
        let mut v = t;
        while v != s {
            bott = bott.min(cap[prev[v]][v]);
            v = prev[v];
        }
        v = t;
        while v != s {
            cap[prev[v]][v] -= bott;
            cap[v][prev[v]] += bott;
            v = prev[v];
        }
        flow += bott;
    }
}

fn maxflow_lp(rng: &mut Rng) {
    let mut total_edges = 0;
    for _ in 0..300 {
        let n = rng.range(4, 7) as usize;
        let (s, t) = (0, n - 1);
        let mut edges: Vec<(usize, usize, i64)> = vec![];
        for u in 0..n {
            for v in 0..n {
                if u != v && v != s && u != t && rng.chance(2, 5) {
                    edges.push((u, v, rng.range(1, 6)));
                }
            }
        }
        total_edges += edges.len();
        let e = edges.len();
        if e == 0 {
            continue;
        }
        // variables: flow on each edge. rows: f_e <= cap_e ; conservation at inner nodes as <= 0 and >= 0
        let mut a: Vec<Vec<f64>> = vec![];
        let mut b: Vec<f64> = vec![];
        for (i, &(_, _, c)) in edges.iter().enumerate() {
            let mut r = vec![0.0; e];
            r[i] = 1.0;
            a.push(r);
            b.push(c as f64);
        }
        for v in 1..n - 1 {
            let mut r = vec![0.0; e];
            for (i, &(u, w, _)) in edges.iter().enumerate() {
                if w == v {
                    r[i] += 1.0;
                }
                if u == v {
                    r[i] -= 1.0;
                }
            }
            a.push(r.iter().map(|x| -x).collect());
            b.push(0.0);
            a.push(r);
            b.push(0.0);
        }
        let c: Vec<f64> = edges
            .iter()
            .map(|&(u, _, _)| if u == s { 1.0 } else { 0.0 })
            .collect();
        let lp = Lp { a, b, c };
        let got = solve(&lp, Rule::Bland)
            .optimal()
            .expect("flow LP is feasible (zero flow) and bounded")
            .obj;
        let mut cap = vec![vec![0i64; n]; n];
        for &(u, v, c) in &edges {
            cap[u][v] += c;
        }
        let want = edmonds_karp(n, &mut cap, s, t);
        assert!(
            close(got, want as f64),
            "LP flow {got} vs Edmonds-Karp {want}"
        );
    }
    println!("max-flow as LP: 300 random networks ({total_edges} edges) match Edmonds-Karp");
}

// ---------- model 3: bipartite matching LP is integral ----------
fn kuhn(adj: &[Vec<usize>], r: usize) -> usize {
    fn try_k(u: usize, adj: &[Vec<usize>], seen: &mut [bool], mate: &mut [Option<usize>]) -> bool {
        for &v in &adj[u] {
            if !seen[v] {
                seen[v] = true;
                if mate[v].is_none() || try_k(mate[v].unwrap(), adj, seen, mate) {
                    mate[v] = Some(u);
                    return true;
                }
            }
        }
        false
    }
    let mut mate = vec![None; r];
    (0..adj.len())
        .filter(|&u| try_k(u, adj, &mut vec![false; r], &mut mate))
        .count()
}

fn matching_lp(rng: &mut Rng) {
    let mut fractional_seen = 0;
    for _ in 0..300 {
        let (l, r) = (rng.range(2, 5) as usize, rng.range(2, 5) as usize);
        let mut edges = vec![];
        let mut adj = vec![vec![]; l];
        for u in 0..l {
            for v in 0..r {
                if rng.chance(1, 2) {
                    edges.push((u, v));
                    adj[u].push(v);
                }
            }
        }
        if edges.is_empty() {
            continue;
        }
        let mut a = vec![];
        for u in 0..l {
            a.push(
                edges
                    .iter()
                    .map(|&(x, _)| (x == u) as i32 as f64)
                    .collect::<Vec<_>>(),
            );
        }
        for v in 0..r {
            a.push(
                edges
                    .iter()
                    .map(|&(_, y)| (y == v) as i32 as f64)
                    .collect::<Vec<_>>(),
            );
        }
        let lp = Lp {
            a: a.clone(),
            b: vec![1.0; l + r],
            c: vec![1.0; edges.len()],
        };
        let s = solve(&lp, Rule::Bland).optimal().unwrap();
        assert!(close(s.obj, kuhn(&adj, r) as f64));
        // the constraint matrix is totally unimodular, so every vertex is 0/1
        for &x in &s.x {
            if x.abs() > 1e-7 && (x - 1.0).abs() > 1e-7 {
                fractional_seen += 1;
            }
        }
    }
    assert_eq!(
        fractional_seen, 0,
        "bipartite matching polytope has integral vertices"
    );
    println!(
        "matching LP: 300 random bipartite graphs, optimum == Kuhn, every returned vertex is 0/1 (total unimodularity)"
    );
}

// ---------- model 4: branch and bound ----------
struct BnB {
    best: Option<(f64, Vec<f64>)>,
    nodes: usize,
}

/// Depth-first branch and bound on max c.x, A x <= b, x >= 0 integer. Relaxation = the LP without integrality.
fn branch_and_bound(lp: &Lp, st: &mut BnB) {
    st.nodes += 1;
    let Some(rel) = solve(lp, Rule::Bland).optimal() else {
        return;
    }; // infeasible node
    if let Some((best, _)) = &st.best {
        if rel.obj <= best + 1e-9 {
            return; // bound: the relaxation cannot beat the incumbent
        }
    }
    // pick the most fractional variable
    let frac = |v: f64| (v - v.round()).abs();
    let Some(j) = (0..lp.n())
        .filter(|&j| frac(rel.x[j]) > 1e-7)
        .max_by(|&p, &q| frac(rel.x[p]).total_cmp(&frac(rel.x[q])))
    else {
        st.best = Some((rel.obj, rel.x.iter().map(|v| v.round()).collect())); // integral: new incumbent
        return;
    };
    let mut up_row = vec![0.0; lp.n()];
    up_row[j] = 1.0;
    let mut down_row = vec![0.0; lp.n()];
    down_row[j] = -1.0;
    // explore the branch the relaxation prefers first (better incumbents earlier => more pruning)
    let (fl, ce) = (rel.x[j].floor(), rel.x[j].ceil());
    let left = lp.with_row(up_row, fl); // x_j <= floor
    let right = lp.with_row(down_row, -ce); // x_j >= ceil
    if rel.x[j] - fl > 0.5 {
        branch_and_bound(&right, st);
        branch_and_bound(&left, st);
    } else {
        branch_and_bound(&left, st);
        branch_and_bound(&right, st);
    }
}

fn knapsack_dp(v: &[i64], w: &[i64], cap: i64) -> i64 {
    let mut dp = vec![0i64; cap as usize + 1];
    for (&vi, &wi) in v.iter().zip(w) {
        for c in (wi..=cap).rev() {
            dp[c as usize] = dp[c as usize].max(dp[(c - wi) as usize] + vi);
        }
    }
    dp[cap as usize]
}

fn knapsack_bnb(rng: &mut Rng) {
    let (mut nodes_total, mut brute_nodes) = (0, 0u64);
    for _ in 0..200 {
        let n = rng.range(5, 10) as usize;
        let v: Vec<i64> = (0..n).map(|_| rng.range(1, 30)).collect();
        let w: Vec<i64> = (0..n).map(|_| rng.range(1, 15)).collect();
        let cap = rng.range(5, 40);
        let mut a = vec![w.iter().map(|&x| x as f64).collect::<Vec<_>>()];
        let mut b = vec![cap as f64];
        for j in 0..n {
            let mut r = vec![0.0; n];
            r[j] = 1.0; // 0/1 items
            a.push(r);
            b.push(1.0);
        }
        let lp = Lp {
            a,
            b,
            c: v.iter().map(|&x| x as f64).collect(),
        };
        let mut st = BnB {
            best: None,
            nodes: 0,
        };
        branch_and_bound(&lp, &mut st);
        let (obj, x) = st.best.expect("zero items is always feasible");
        assert!(
            close(obj, knapsack_dp(&v, &w, cap) as f64),
            "B&B knapsack vs DP"
        );
        let weight: f64 = x.iter().zip(&w).map(|(x, &w)| x * w as f64).sum();
        assert!(weight <= cap as f64 + 1e-9 && x.iter().all(|&v| v == 0.0 || v == 1.0));
        nodes_total += st.nodes;
        brute_nodes += 1u64 << n;
    }
    println!(
        "0/1 knapsack B&B: 200 instances == DP; {nodes_total} LP nodes vs {brute_nodes} subsets for brute force"
    );
}

fn permutations(n: usize, cur: &mut Vec<usize>, used: &mut Vec<bool>, f: &mut dyn FnMut(&[usize])) {
    if cur.len() == n {
        f(cur);
        return;
    }
    for i in 0..n {
        if !used[i] {
            used[i] = true;
            cur.push(i);
            permutations(n, cur, used, f);
            cur.pop();
            used[i] = false;
        }
    }
}

fn assignment_bnb(rng: &mut Rng) {
    for _ in 0..100 {
        let n = rng.range(3, 5) as usize;
        let cost: Vec<Vec<i64>> = (0..n)
            .map(|_| (0..n).map(|_| rng.range(1, 20)).collect())
            .collect();
        // x[i*n+j] = 1 if worker i takes job j. equality as two inequalities.
        let mut a = vec![];
        let mut b = vec![];
        for i in 0..n {
            let row: Vec<f64> = (0..n * n).map(|k| (k / n == i) as i32 as f64).collect();
            a.push(row.iter().map(|v| -v).collect());
            b.push(-1.0);
            a.push(row);
            b.push(1.0);
        }
        for j in 0..n {
            let col: Vec<f64> = (0..n * n).map(|k| (k % n == j) as i32 as f64).collect();
            a.push(col.iter().map(|v| -v).collect());
            b.push(-1.0);
            a.push(col);
            b.push(1.0);
        }
        let c: Vec<f64> = (0..n * n).map(|k| -(cost[k / n][k % n] as f64)).collect();
        let lp = Lp { a, b, c };
        let mut st = BnB {
            best: None,
            nodes: 0,
        };
        branch_and_bound(&lp, &mut st);
        let (obj, _) = st.best.expect("a perfect matching exists");
        let mut best = i64::MAX;
        permutations(n, &mut vec![], &mut vec![false; n], &mut |p| {
            best = best.min(p.iter().enumerate().map(|(i, &j)| cost[i][j]).sum());
        });
        assert!(close(-obj, best as f64), "assignment B&B vs brute force");
        assert!(
            st.nodes <= 5,
            "LP is integral, so the root should already be optimal (nodes {})",
            st.nodes
        );
    }
    println!(
        "assignment: 100 random cost matrices == brute-force permutations, root LP integral (no branching needed)"
    );
}

fn main() {
    demo_basics();
    demo_cycling();
    random_verification();
    diet();
    let mut rng = Rng(0xD1B54A32D192ED03);
    maxflow_lp(&mut rng);
    matching_lp(&mut rng);
    knapsack_bnb(&mut rng);
    assignment_bnb(&mut rng);
    println!("all linear-programming checks passed");
}
