//! RECIPE 763 (database): learning from execution: LEO-style feedback repository, mid-query re-optimization with checkpoints, plan stitching.
//! LEVEL: advanced
//! Use in: cost-based optimizers with runtime statistics, adaptive executors, plan-regression repair, self-tuning databases.   Run: cargo run --release --example adaptive_feedback_reoptimization
//! COMMON USES: IBM DB2 LEO (learning optimizer: store actual-vs-estimated ratios per predicate/subplan and apply them to later plans), progressive
//! optimization with CHECK operators (DB2 POP), Oracle statistics feedback / SQL plan directives and adaptive plans, SQL Server batch-mode
//! adaptive joins and cardinality feedback, Spark AQE re-planning at shuffle (materialisation) boundaries, plan-stitching research for
//! cloud engines that stitch the best sub-plans of previously executed plans.
//! Course: CMU 15-799 Query Optimization (Spring 2025), lectures 17-18 (Adaptivity: Feedback; Adaptivity: Runtime Processing)
#![allow(dead_code)]
//!
//! WHAT: three ways an optimizer uses what it observed while executing.
//! * FEEDBACK REPOSITORY (LEO): while a query runs, the actual cardinality of every executed sub-plan is recorded under a CANONICAL FINGERPRINT
//!   (the set of joined tables plus the sorted predicates inside it, independent of join order). Later optimisations first look the
//!   fingerprint up; a miss falls back to the largest KNOWN sub-expression scaled by the independence ratio
//!   est(S) = actual(K) * indep(S) / indep(K). The estimate of repeated work converges to the truth over repeated runs of a workload.
//! * MID-QUERY RE-OPTIMISATION (POP / progressive optimisation): at plan time a VALIDITY RANGE [lo, hi] is computed for each checkpoint, the
//!   interval of cardinalities of the intermediate result for which the rest of the plan stays within 5% of optimal (found by re-optimising the
//!   remaining joins over a geometric sweep of that cardinality). Execution materialises the intermediate at the checkpoint; when its ACTUAL
//!   size leaves the range, the remaining query is re-planned around the materialised result (no work is thrown away) and execution continues.
//! * PLAN STITCHING (Ding et al.): logical sub-expressions produce identical results whichever plan computed them, so measured sub-plan costs
//!   from several executed plans can be recombined. A DP over the lattice of table intervals picks the cheapest combination of OBSERVED steps,
//!   which is never worse than the best executed plan and can beat all of them.
//!
//! The testbed is a five-table chain R0-R1-R2-R3-R4 (R_i.fk -> R_{i-1}.id) with Zipf foreign keys, a predicate/fan-out correlation (hot parents
//! have v1 = 0) and a correlated column pair (v2 = v1 90% of the time), so the independence estimator is badly wrong. A query is an interval of
//! tables plus filters; plans are left-deep orders that grow an interval left or right. WORK = rows processed: reading the start table (index on its filtered column, else a full scan) plus, per
//! join step, the cost of its algorithm: index nested loop (4 per left row + output) or hash join (scan + filter the right table, 0.5 per probe + output). Executed work is the ground truth for every number below.
//!
//! WHY: static optimisation commits to a plan on estimates that can be off by orders of magnitude. Feedback fixes the NEXT run, checkpoints fix
//! THIS run, stitching reuses history; together they trade a small, measured overhead when estimates were right for large savings when wrong.
//!
//! CHECKS (asserted): the geometric-mean q-error of the repository-backed estimator never increases over 6 runs of a workload and ends below
//! the start; static and re-optimised execution return identical results (count and order-independent checksum) on 200 random queries;
//! re-optimisation processes fewer rows than static execution when estimates are bad; with exact estimates no checkpoint fires and overhead
//! is bounded; the stitched plan is never costlier than the best executed plan, matches its predicted cost when executed and returns equal results.
//! Read: Adaptivity: Feedback / Runtime Processing, CMU 15-799 lectures 17-18 https://15799.courses.cs.cmu.edu/spring2025/schedule.html
//!       LEO - DB2's LEarning Optimizer (Stillger et al., VLDB 2001)
//!       Plan Stitch: Harnessing the Best of Many Plans (Ding et al., VLDB 2018)
//!       Adaptive Query Processing in the Looking Glass (Babu et al., CIDR 2015)

use std::collections::HashMap;

// ---------------------------------------------------------------------------------------------------------------------
// Randomness
// ---------------------------------------------------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn zipf_cdf(n: usize, s: f64) -> Vec<f64> {
    let mut acc = 0.0;
    let mut cdf: Vec<f64> = (1..=n)
        .map(|k| {
            acc += 1.0 / (k as f64).powf(s);
            acc
        })
        .collect();
    cdf.iter_mut().for_each(|c| *c /= acc);
    cdf
}

fn qerr(est: f64, actual: f64) -> f64 {
    let (e, a) = (est.max(1.0), actual.max(1.0));
    (e / a).max(a / e)
}

// ---------------------------------------------------------------------------------------------------------------------
// Data: chain R0 - R1 - R2 - R3 - R4. Column layout per table: fk (row of parent), v1, v2 (domain 0..20).
// ---------------------------------------------------------------------------------------------------------------------

const NT: usize = 5;
const SIZES: [usize; NT] = [300, 2000, 8000, 12000, 15000];
const V: usize = 20;

struct Db {
    fk: Vec<Vec<u32>>,
    v: Vec<[Vec<i64>; 2]>,        // v[t][0] = v1, v[t][1] = v2
    children: Vec<Vec<Vec<u32>>>, // children[t][parent row] = rows of t referencing it
    marg: Vec<[[f64; V]; 2]>, // exact per-column value counts (best possible single-column statistics)
}

fn gen_db(seed: u64) -> Db {
    let mut r = Rng(seed);
    let (mut fk, mut v, mut children, mut marg) = (vec![], vec![], vec![], vec![]);
    for t in 0..NT {
        let n = SIZES[t];
        let cdf = if t > 0 {
            zipf_cdf(SIZES[t - 1], 1.0)
        } else {
            vec![]
        };
        let f: Vec<u32> = (0..n)
            .map(|_| {
                if t == 0 {
                    0
                } else {
                    cdf.partition_point(|&c| c < r.unit()).min(SIZES[t - 1] - 1) as u32
                }
            })
            .collect();
        let (mut v1, mut v2) = (vec![], vec![]);
        for i in 0..n {
            // hot parents (low ids, which Zipf makes the most referenced) have v1 = 0 with probability 0.7: predicate/fanout correlation
            let a = if i < 30 && r.chance(0.7) {
                0
            } else {
                r.below(V as u64) as i64
            };
            v1.push(a);
            v2.push(if r.chance(0.9) {
                a
            } else {
                r.below(V as u64) as i64
            });
        }
        let mut ch = if t > 0 {
            vec![vec![]; SIZES[t - 1]]
        } else {
            vec![]
        };
        if t > 0 {
            f.iter()
                .enumerate()
                .for_each(|(i, &p)| ch[p as usize].push(i as u32));
        }
        let mut m = [[0.0; V]; 2];
        v1.iter().for_each(|&x| m[0][x as usize] += 1.0);
        v2.iter().for_each(|&x| m[1][x as usize] += 1.0);
        fk.push(f);
        v.push([v1, v2]);
        children.push(ch);
        marg.push(m);
    }
    Db {
        fk,
        v,
        children,
        marg,
    }
}

#[derive(Clone, Debug)]
struct Pred {
    t: usize,
    c: usize, // 0 = v1, 1 = v2
    lo: i64,
    hi: i64,
}

type Tup = [u32; NT];
type Span = (usize, usize); // interval of tables [a, b] joined so far

struct Query {
    lo: usize,
    hi: usize,
    preds: Vec<Pred>,
    filt: Vec<f64>,            // filtered row count per table (exact)
    truth: HashMap<Span, f64>, // exact cardinality of every sub-interval
}

fn passes(db: &Db, q: &Query, t: usize, r: usize) -> bool {
    q.preds
        .iter()
        .filter(|p| p.t == t)
        .all(|p| (p.lo..=p.hi).contains(&db.v[t][p.c][r]))
}

fn start(db: &Db, q: &Query, s: usize) -> Vec<Tup> {
    (0..SIZES[s])
        .filter(|&r| passes(db, q, s, r))
        .map(|r| {
            let mut t = [u32::MAX; NT];
            t[s] = r as u32;
            t
        })
        .collect()
}

/// Extend the intermediate result of interval `set` by the adjacent table `t` (index lookups along the foreign key).
fn join_step(db: &Db, q: &Query, tuples: &[Tup], set: Span, t: usize) -> Vec<Tup> {
    let mut out = vec![];
    for tup in tuples {
        if t == set.1 + 1 {
            for &c in &db.children[t][tup[set.1] as usize] {
                if passes(db, q, t, c as usize) {
                    let mut n = *tup;
                    n[t] = c;
                    out.push(n);
                }
            }
        } else {
            let p = db.fk[set.0][tup[set.0] as usize];
            if passes(db, q, t, p as usize) {
                let mut n = *tup;
                n[t] = p;
                out.push(n);
            }
        }
    }
    out
}

fn extend(set: Span, t: usize) -> Span {
    (set.0.min(t), set.1.max(t))
}

/// Reading the first table: through an index on the filtered column when the query filters it (touching only the qualifying rows),
/// otherwise a full scan.
fn start_cost(q: &Query, s: usize) -> f64 {
    if q.preds.iter().any(|p| p.t == s) {
        q.filt[s]
    } else {
        SIZES[s] as f64
    }
}

fn checksum(tuples: &[Tup]) -> u64 {
    tuples.iter().fold(0u64, |acc, t| {
        acc.wrapping_add(t.iter().fold(7u64, |h, &x| mix(h ^ x as u64)))
    })
}

fn make_query(db: &Db, r: &mut Rng, lo: usize, hi: usize) -> Query {
    let mut preds = vec![];
    for t in lo..=hi {
        // Filters concentrate on the two ends of the interval (that is where join order is decided). Half of the equality values are 0,
        // the value that the hot parents carry, so the filter also selects the high-fan-out rows: a predicate/join-key correlation.
        let p_filter = if t == lo || t == hi { 0.85 } else { 0.2 };
        if r.chance(p_filter) {
            let a = if r.chance(0.5) {
                0
            } else {
                db.v[t][0][r.below(SIZES[t] as u64) as usize]
            };
            preds.push(Pred {
                t,
                c: 0,
                lo: a,
                hi: a,
            });
            if r.chance(0.6) {
                preds.push(Pred {
                    t,
                    c: 1,
                    lo: a,
                    hi: a,
                }); // correlated second predicate: independence squares the selectivity
            }
        }
    }
    let mut q = Query {
        lo,
        hi,
        preds,
        filt: vec![0.0; NT],
        truth: HashMap::new(),
    };
    for t in lo..=hi {
        q.filt[t] = start(db, &q, t).len() as f64;
        q.truth.insert((t, t), q.filt[t]);
    }
    for i in lo..=hi {
        let mut tuples = start(db, &q, i);
        for j in i + 1..=hi {
            tuples = join_step(db, &q, &tuples, (i, j - 1), j);
            q.truth.insert((i, j), tuples.len() as f64);
        }
    }
    q
}

// ---------------------------------------------------------------------------------------------------------------------
// Estimators: independence baseline, feedback repository, oracle
// ---------------------------------------------------------------------------------------------------------------------

trait Estimator {
    fn est(&self, db: &Db, q: &Query, set: Span) -> f64;
}

fn sel(db: &Db, p: &Pred) -> f64 {
    (p.lo..=p.hi)
        .map(|v| db.marg[p.t][p.c][v as usize])
        .sum::<f64>()
        / SIZES[p.t] as f64
}

/// Textbook: product of filtered table sizes (predicates independent) divided by max(ndv) = |parent| for every FK join in the interval.
fn indep(db: &Db, q: &Query, set: Span) -> f64 {
    let mut e = 1.0;
    for t in set.0..=set.1 {
        e *= q
            .preds
            .iter()
            .filter(|p| p.t == t)
            .fold(SIZES[t] as f64, |a, p| a * sel(db, p));
        if t > set.0 {
            e /= SIZES[t - 1] as f64;
        }
    }
    e.max(1.0)
}

struct Indep;
impl Estimator for Indep {
    fn est(&self, db: &Db, q: &Query, set: Span) -> f64 {
        indep(db, q, set)
    }
}

/// Stale statistics: the independence estimate times a deterministic factor of 10^z, z in [-1.5, 1.5], derived from the fingerprint.
/// Models what a bad ANALYZE, data drift or a wrong join selectivity does to individual sub-plans.
struct Stale;
impl Estimator for Stale {
    fn est(&self, db: &Db, q: &Query, set: Span) -> f64 {
        let h = fingerprint(q, set)
            .bytes()
            .fold(0xCBF2_9CE4u64, |h, b| mix(h ^ b as u64));
        let z = (h >> 11) as f64 / (1u64 << 53) as f64 * 3.0 - 1.5;
        (indep(db, q, set) * 10f64.powf(z)).max(1.0)
    }
}

struct Oracle;
impl Estimator for Oracle {
    fn est(&self, _: &Db, q: &Query, set: Span) -> f64 {
        q.truth[&set].max(1.0)
    }
}

/// Canonical fingerprint of a sub-expression: table interval + sorted predicates inside it. Join ORDER is not part of it, because the
/// result of a logical expression does not depend on how it was computed.
fn fingerprint(q: &Query, set: Span) -> String {
    let mut ps: Vec<String> = q
        .preds
        .iter()
        .filter(|p| p.t >= set.0 && p.t <= set.1)
        .map(|p| format!("{}.{}[{},{}]", p.t, p.c, p.lo, p.hi))
        .collect();
    ps.sort();
    format!("{}-{}|{}", set.0, set.1, ps.join("&"))
}

#[derive(Default)]
struct Feedback {
    repo: HashMap<String, f64>,
}

impl Feedback {
    fn record(&mut self, q: &Query, set: Span, actual: f64) {
        self.repo.insert(fingerprint(q, set), actual);
    }
}

impl Estimator for Feedback {
    fn est(&self, db: &Db, q: &Query, set: Span) -> f64 {
        if let Some(&a) = self.repo.get(&fingerprint(q, set)) {
            return a.max(1.0);
        }
        // Largest known sub-interval, then correct the independence estimate by the ratio observed on that part (LEO's adjustment).
        for len in (1..set.1 - set.0 + 1).rev() {
            for a in set.0..=set.1 + 1 - len {
                let b = a + len - 1;
                if b > set.1 || (a, b) == set {
                    continue;
                }
                if let Some(&k) = self.repo.get(&fingerprint(q, (a, b))) {
                    return (k.max(1.0) * indep(db, q, set) / indep(db, q, (a, b))).max(1.0);
                }
            }
        }
        indep(db, q, set)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Plans: (start table, steps). A step adds one adjacent table with a join algorithm chosen from the ESTIMATED sizes.
// ---------------------------------------------------------------------------------------------------------------------

/// (table added, use hash join). Index nested loop costs 4 per left row + output; hash join scans and filters the right table once
/// (0.25 per row of it, then the filtered rows) and probes at 0.5 per left row. Hash wins only when the left input is big: a wrong
/// cardinality therefore picks the WRONG ALGORITHM, which is the non-linearity that makes validity ranges meaningful.
type Step = (usize, bool);

fn step_cost(left: f64, out: f64, t: usize, filt: f64, hash: bool) -> f64 {
    if hash {
        0.25 * SIZES[t] as f64 + filt + 0.5 * left + out
    } else {
        4.0 * left + out
    }
}

fn extensions(set: Span, lo: usize, hi: usize) -> Vec<Vec<usize>> {
    if set == (lo, hi) {
        return vec![vec![]];
    }
    let mut out = vec![];
    for t in [set.0.wrapping_sub(1), set.1 + 1] {
        if t >= lo && t <= hi && (t + 1 == set.0 || t == set.1 + 1) {
            for mut rest in extensions(extend(set, t), lo, hi) {
                rest.insert(0, t);
                out.push(rest);
            }
        }
    }
    out
}

/// Estimated work of extending `set` (intermediate of c0 rows) by `order`, each step using its cheaper algorithm under the estimates.
/// Larger sets are rescaled by c0 / est(set): how a re-optimiser plugs an observed cardinality into the remaining plan.
/// Returns the cost and the algorithms chosen.
fn plan_ext(
    db: &Db,
    q: &Query,
    est: &dyn Estimator,
    set: Span,
    order: &[usize],
    c0: f64,
) -> (f64, Vec<Step>) {
    let scale = c0 / est.est(db, q, set);
    let (mut cur, mut s, mut total, mut steps) = (c0, set, 0.0, vec![]);
    for &t in order {
        s = extend(s, t);
        let out = est.est(db, q, s) * scale;
        let filt = est.est(db, q, (t, t));
        let (inl, hj) = (
            step_cost(cur, out, t, filt, false),
            step_cost(cur, out, t, filt, true),
        );
        total += inl.min(hj);
        steps.push((t, hj < inl));
        cur = out;
    }
    (total, steps)
}

/// Cost of an already committed tail (its algorithms are fixed) under cardinality c0 for `set`.
fn cost_fixed(db: &Db, q: &Query, est: &dyn Estimator, set: Span, steps: &[Step], c0: f64) -> f64 {
    let scale = c0 / est.est(db, q, set);
    let (mut cur, mut s, mut total) = (c0, set, 0.0);
    for &(t, hash) in steps {
        s = extend(s, t);
        let out = est.est(db, q, s) * scale;
        total += step_cost(cur, out, t, est.est(db, q, (t, t)), hash);
        cur = out;
    }
    total
}

fn best_ext(db: &Db, q: &Query, est: &dyn Estimator, set: Span, c0: f64) -> (f64, Vec<Step>) {
    extensions(set, q.lo, q.hi)
        .into_iter()
        .map(|o| plan_ext(db, q, est, set, &o, c0))
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
        .unwrap()
}

fn best_plan(db: &Db, q: &Query, est: &dyn Estimator) -> (usize, Vec<Step>) {
    (q.lo..=q.hi)
        .map(|s| {
            let c0 = est.est(db, q, (s, s));
            let (cost, steps) = best_ext(db, q, est, (s, s), c0);
            (
                cost + if q.preds.iter().any(|p| p.t == s) {
                    c0
                } else {
                    SIZES[s] as f64
                },
                s,
                steps,
            )
        })
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
        .map(|(_, s, steps)| (s, steps))
        .unwrap()
}

/// Validity ranges for the checkpoints after each step of `steps` (starting from `set`, whose intermediate has c0 rows). For checkpoint m,
/// sweep the intermediate's cardinality over factors 1.5^k and keep the contiguous band around the estimate where the committed tail
/// (order AND algorithms) is within 5% of the best re-planned tail. With a single table left only the algorithm could change.
fn ranges(
    db: &Db,
    q: &Query,
    est: &dyn Estimator,
    set: Span,
    steps: &[Step],
    c0: f64,
) -> Vec<(f64, f64)> {
    let scale = c0 / est.est(db, q, set);
    let (mut out, mut s) = (vec![], set);
    for m in 0..steps.len() {
        s = extend(s, steps[m].0);
        let centre = est.est(db, q, s) * scale;
        let tail = &steps[m + 1..];
        if tail.is_empty() {
            out.push((0.0, f64::INFINITY));
            continue;
        }
        let valid = |k: i32| {
            let c = centre * 1.5f64.powi(k);
            cost_fixed(db, q, est, s, tail, c) <= best_ext(db, q, est, s, c).0 * 1.05 + 1e-9
        };
        let (mut klo, mut khi) = (0, 0);
        while klo > -20 && valid(klo - 1) {
            klo -= 1
        }
        while khi < 20 && valid(khi + 1) {
            khi += 1
        }
        let (lo, hi) = (
            centre * 1.5f64.powi(klo) / 1.5f64.sqrt(),
            centre * 1.5f64.powi(khi) * 1.5f64.sqrt(),
        );
        out.push((
            if klo == -20 { 0.0 } else { lo },
            if khi == 20 { f64::INFINITY } else { hi },
        ));
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------------
// Execution with optional checkpoints
// ---------------------------------------------------------------------------------------------------------------------

struct Outcome {
    work: f64,
    count: usize,
    sum: u64,
    replans: usize,
    steps: Vec<(Span, Step, f64, f64)>, // (set before, step, step work, output rows): observations for feedback / stitching
}

/// Execute a plan. With `reopt`, every checkpoint compares the materialised intermediate with its validity range and, when it is outside,
/// re-plans the remaining tables around the materialised rows. Checkpoint materialisation is charged 0.1 work per row.
fn run(db: &Db, q: &Query, est: &dyn Estimator, plan: (usize, Vec<Step>), reopt: bool) -> Outcome {
    let (s, mut steps) = plan;
    let (mut set, mut tuples) = ((s, s), start(db, q, s));
    let mut o = Outcome {
        work: start_cost(q, s),
        count: 0,
        sum: 0,
        replans: 0,
        steps: vec![],
    };
    let mut rg = if reopt {
        ranges(db, q, est, set, &steps, est.est(db, q, set))
    } else {
        vec![]
    };
    let mut m = 0;
    while m < steps.len() {
        let (t, hash) = steps[m];
        let out = join_step(db, q, &tuples, set, t);
        let w = step_cost(tuples.len() as f64, out.len() as f64, t, q.filt[t], hash);
        o.work += w;
        o.steps.push((set, steps[m], w, out.len() as f64));
        set = extend(set, t);
        tuples = out;
        if reopt && m + 1 < steps.len() {
            o.work += 0.1 * tuples.len() as f64; // the checkpoint materialises the intermediate
            let n = tuples.len() as f64;
            if n < rg[m].0 || n > rg[m].1 {
                // Outside the validity range: the remaining plan is no longer near-optimal. Plan again from the REAL cardinality.
                let (_, tail) = best_ext(db, q, est, set, n.max(1.0));
                o.replans += (tail != steps[m + 1..]) as usize;
                rg = ranges(db, q, est, set, &tail, n.max(1.0));
                steps = tail;
                m = 0;
                continue;
            }
        }
        m += 1;
    }
    o.count = tuples.len();
    o.sum = checksum(&tuples);
    o
}

// ---------------------------------------------------------------------------------------------------------------------
// Experiments
// ---------------------------------------------------------------------------------------------------------------------

fn random_workload(db: &Db, r: &mut Rng, n: usize) -> Vec<Query> {
    (0..n)
        .map(|_| {
            let k = 3 + r.below(3) as usize;
            let lo = r.below((NT - k + 1) as u64) as usize;
            make_query(db, r, lo, lo + k - 1)
        })
        .collect()
}

fn geo_mean(v: &[f64]) -> f64 {
    (v.iter().map(|x| x.ln()).sum::<f64>() / v.len() as f64).exp()
}

fn section_feedback(db: &Db) {
    println!("== 1. feedback repository: 40 distinct queries run 6 times ==");
    let mut r = Rng(17);
    let wl = random_workload(db, &mut r, 40);
    let mut fb = Feedback::default();
    let metric = |fb: &Feedback| -> f64 {
        let mut qs = vec![];
        for q in &wl {
            for a in q.lo..=q.hi {
                for b in a + 1..=q.hi {
                    qs.push(qerr(fb.est(db, q, (a, b)), q.truth[&(a, b)]));
                }
            }
        }
        geo_mean(&qs)
    };
    let mut hist = vec![metric(&fb)];
    println!(
        "  {:>4} {:>16} {:>14} {:>10}",
        "run", "geo q-error", "work (rows)", "repo size"
    );
    println!("  {:>4} {:>16.3} {:>14} {:>10}", "init", hist[0], "-", 0);
    for run_no in 1..=6 {
        let mut work = 0.0;
        for q in &wl {
            let plan = best_plan(db, q, &fb);
            let o = run(db, q, &fb, plan.clone(), false);
            work += o.work;
            // record the actual cardinality of every executed sub-expression (prefix) and every filtered base table
            for &(set, (t, _), _, out) in &o.steps {
                fb.record(q, extend(set, t), out);
                fb.record(q, (t, t), q.filt[t]);
            }
            fb.record(q, (plan.0, plan.0), q.filt[plan.0]);
        }
        hist.push(metric(&fb));
        println!(
            "  {:>4} {:>16.3} {:>14.0} {:>10}",
            run_no,
            hist[run_no],
            work,
            fb.repo.len()
        );
    }
    assert!(
        hist.windows(2).all(|w| w[1] <= w[0] + 1e-9),
        "feedback q-error must not increase: {hist:?}"
    );
    assert!(hist[6] < hist[0], "feedback must improve the estimates");
}

/// Exact work of a committed plan computed from the ground-truth cardinalities (validates execution; the true optimum uses best algorithms).
fn true_cost(q: &Query, s: usize, steps: &[Step]) -> f64 {
    let (mut set, mut total) = ((s, s), start_cost(q, s));
    for &(t, hash) in steps {
        let ns = extend(set, t);
        total += step_cost(q.truth[&set], q.truth[&ns], t, q.filt[t], hash);
        set = ns;
    }
    total
}

fn true_best_algos(q: &Query, s: usize, order: &[usize]) -> Vec<Step> {
    let (mut set, mut steps) = ((s, s), vec![]);
    for &t in order {
        let ns = extend(set, t);
        let (a, b) = (
            step_cost(q.truth[&set], q.truth[&ns], t, q.filt[t], false),
            step_cost(q.truth[&set], q.truth[&ns], t, q.filt[t], true),
        );
        steps.push((t, b < a));
        set = ns;
    }
    steps
}

fn true_optimum(q: &Query) -> f64 {
    (q.lo..=q.hi)
        .flat_map(|s| {
            extensions((s, s), q.lo, q.hi)
                .into_iter()
                .map(move |o| (s, o))
        })
        .map(|(s, o)| true_cost(q, s, &true_best_algos(q, s, &o)))
        .fold(f64::INFINITY, f64::min)
}

fn section_reopt(db: &Db) {
    println!("\n== 2. mid-query re-optimisation with checkpoints: 200 random 3-5 table queries ==");
    let mut r = Rng(18);
    let wl = random_workload(db, &mut r, 200);
    let wopt: f64 = wl.iter().map(true_optimum).sum();
    println!(
        "  total work of the true optimum (best order and algorithms, known cardinalities): {wopt:.0}"
    );
    println!(
        "  {:<24} {:>12} {:>12} {:>8} {:>8} {:>8}",
        "estimator", "static work", "reopt work", "saved", "replans", "queries"
    );
    for (name, est) in [
        ("independence", &Indep as &dyn Estimator),
        ("stale stats (10^+-1.5)", &Stale as &dyn Estimator),
        ("oracle (exact)", &Oracle as &dyn Estimator),
    ] {
        let (mut ws, mut wr, mut replans, mut affected) = (0.0, 0.0, 0, 0);
        for q in &wl {
            let plan = best_plan(db, q, est);
            let (a, b) = (
                run(db, q, est, plan.clone(), false),
                run(db, q, est, plan, true),
            );
            assert_eq!(
                (a.count, a.sum),
                (b.count, b.sum),
                "re-optimised execution changed the result"
            );
            assert_eq!(a.count as f64, q.truth[&(q.lo, q.hi)]);
            ws += a.work;
            wr += b.work;
            replans += b.replans;
            affected += (b.replans > 0) as usize;
        }
        println!(
            "  {:<24} {:>12.0} {:>12.0} {:>7.1}% {:>8} {:>8}",
            name,
            ws,
            wr,
            (1.0 - wr / ws) * 100.0,
            replans,
            affected
        );
        if name.starts_with("stale") {
            assert!(
                replans > 0 && wr < ws,
                "re-optimisation must fire and save work when estimates are bad"
            );
        } else if name.starts_with("oracle") {
            assert_eq!(
                replans, 0,
                "exact estimates must never leave a validity range"
            );
            assert!(
                wr <= ws * 1.10,
                "checkpoint overhead with exact estimates must stay under 10%: {} vs {}",
                wr,
                ws
            );
            println!(
                "  overhead with exact estimates (checkpoint materialisation only): {:.2}% of rows processed",
                (wr / ws - 1.0) * 100.0
            );
        }
    }
}

fn section_stitch(db: &Db) {
    println!(
        "\n== 3. plan stitching: 100 queries, 5 executed plans each, DP over observed steps =="
    );
    let mut r = Rng(19);
    let wl: Vec<Query> = (0..100)
        .map(|_| {
            let k = 4 + r.below(2) as usize;
            let lo = r.below((NT - k + 1) as u64) as usize;
            make_query(db, &mut r, lo, lo + k - 1)
        })
        .collect();
    let (mut sum_best_exec, mut sum_stitch, mut sum_opt, mut beat) = (0.0, 0.0, 0.0, 0);
    for q in &wl {
        // A history of 5 past executions: random orders with random algorithm choices (what different estimators / hints would produce).
        let mut history: Vec<(usize, Vec<Step>)> = vec![];
        while history.len() < 5 {
            let s = q.lo + r.below((q.hi - q.lo + 1) as u64) as usize;
            let orders = extensions((s, s), q.lo, q.hi);
            let order = &orders[r.below(orders.len() as u64) as usize];
            let steps: Vec<Step> = order.iter().map(|&t| (t, r.chance(0.5))).collect();
            if !history.contains(&(s, steps.clone())) {
                history.push((s, steps));
            }
        }
        let mut observed: HashMap<(Span, Step), f64> = HashMap::new();
        let (mut best_exec, mut reference) = (f64::INFINITY, None);
        for (s, steps) in &history {
            let o = run(db, q, &Indep, (*s, steps.clone()), false);
            assert!(
                (o.work - true_cost(q, *s, steps)).abs() < 1e-6,
                "executed work must equal the truth-based formula"
            );
            best_exec = best_exec.min(o.work);
            reference = Some((o.count, o.sum));
            for &(set, st, w, _) in &o.steps {
                observed.insert((set, st), w);
            }
        }
        // DP over intervals: best[(a,b)] = cheapest way to produce it from observed steps; only observed start tables are usable.
        let mut best: HashMap<Span, (f64, Option<(Span, Step)>)> = HashMap::new();
        for (s, _) in &history {
            best.insert((*s, *s), (start_cost(q, *s), None));
        }
        for len in 2..=q.hi - q.lo + 1 {
            for a in q.lo..=q.hi + 1 - len {
                let set = (a, a + len - 1);
                for (&(prev, st), &w) in &observed {
                    if extend(prev, st.0) == set {
                        if let Some(&(pc, _)) = best.get(&prev) {
                            if best.get(&set).map_or(true, |&(bc, _)| pc + w < bc) {
                                best.insert(set, (pc + w, Some((prev, st))));
                            }
                        }
                    }
                }
            }
        }
        let (cost, _) = best[&(q.lo, q.hi)];
        let (mut steps, mut cur) = (vec![], (q.lo, q.hi));
        while let Some((prev, st)) = best[&cur].1 {
            steps.push(st);
            cur = prev;
        }
        steps.reverse();
        let o = run(db, q, &Indep, (cur.0, steps), false);
        assert!(
            (o.work - cost).abs() < 1e-6,
            "stitched plan must cost what the DP predicted"
        );
        assert_eq!(
            Some((o.count, o.sum)),
            reference,
            "stitched plan returned a different result"
        );
        assert!(
            cost <= best_exec + 1e-6,
            "stitching can never lose to the best executed plan"
        );
        beat += (cost < best_exec - 1e-6) as usize;
        sum_best_exec += best_exec;
        sum_stitch += cost;
        sum_opt += true_optimum(q);
    }
    println!(
        "  total work: best executed plan {sum_best_exec:.0}, stitched {sum_stitch:.0}, true optimum {sum_opt:.0}"
    );
    println!(
        "  stitched plan strictly beat every executed plan on {beat}/100 queries; stitched/optimum = {:.3}, best-executed/optimum = {:.3}",
        sum_stitch / sum_opt,
        sum_best_exec / sum_opt
    );
}

fn main() {
    let db = gen_db(763);
    println!(
        "chain tables: {}",
        SIZES
            .iter()
            .enumerate()
            .map(|(i, n)| format!("R{i}={n}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    section_feedback(&db);
    section_reopt(&db);
    section_stitch(&db);
    println!("\nall adaptive feedback checks passed");
}
