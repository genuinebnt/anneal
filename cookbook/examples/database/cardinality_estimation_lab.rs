//! RECIPE 761 (database): cardinality estimation lab: independence, histogram-join, wander-join sampling, Fast-AGMS sketches and a learned regressor on an IMDB-like schema.
//! LEVEL: advanced
//! Use in: query optimizers, benchmark harnesses for estimators, approximate query processing, learned-optimizer research.   Run: cargo run --release --example cardinality_estimation_lab
//! COMMON USES: PostgreSQL / System R style estimators (independence, uniformity, inclusion: join selectivity 1/max(ndv)), sampling-based
//! estimation (index-based join sampling in HyPer-style research systems, wander join in online aggregation), AGMS / Fast-AGMS sketches for
//! join-size estimation in stream and distributed engines, equi-depth histogram joins in commercial optimizers, MSCN-style learned
//! estimators evaluated with q-error in the Join Order Benchmark literature.
//! Course: CMU 15-799 Query Optimization (Spring 2025), lectures 13-15 (Cardinality Estimation, More Cardinality Estimation, Learned Approaches)
#![allow(dead_code)]
//!
//! WHAT: a LAB. A seeded generator builds six tables of an IMDB-like schema (title, person, cast_info, company, movie_company, movie_keyword)
//! with the properties that break textbook estimators: Zipf-skewed foreign keys (a few blockbuster titles own most cast rows, companies
//! and keywords), a column correlation inside one table (title.kind determines title.genre 85% of the time), a correlation ACROSS a join
//! (person.gender determines cast.role) and a fanout correlation (hot titles are mostly kind 0, so `kind = 0` selects the high-fanout titles).
//! Random 1-4 table conjunctive queries (FK/PK equi-joins plus range/equality filters) are then estimated five ways and compared with the
//! TRUE cardinality obtained by executing the query (bottom-up weight propagation over the join tree, exact):
//! * INDEPENDENCE: prod(filtered base cardinalities) / prod over joins of max(ndv). Perfect single-column marginals are given on purpose,
//!   so every remaining error is caused by the independence / uniformity / inclusion assumptions, not by coarse histograms.
//! * HISTOGRAM JOIN: per join edge, intersect aligned key buckets (64 equi-width buckets with count and ndv, size = sum c1*c2/max(ndv1,ndv2));
//!   fixes skewed fanout across buckets, still assumes predicates independent of join keys.
//! * WANDER JOIN (index-based join sampling): pick a random row of the filtered root, follow indexes to a random matching row of each
//!   neighbour, multiply the observed fanouts. Each walk is an unbiased (Horvitz-Thompson) estimate; more walks, less variance.
//! * FAST-AGMS SKETCH: signed counters per (row, bucket); the dot product of two key sketches is an unbiased estimate of the join size of
//!   the FILTERED inputs, so predicate selectivity and key skew are both captured per edge; multiple edges still combine by independence.
//! * LEARNED: ridge regression (normal equations, hand-written Gaussian elimination) of ln(true cardinality) on a query featurisation: table
//!   and edge indicators, per-column ln-selectivities, the independence and histogram-join estimates, and a few interaction features
//!   (a linear stand-in for the hidden layers of an MSCN). Trained on 1200 queries, evaluated on 500 unseen ones.
//!
//! WHY: Leis et al. showed estimation errors grow exponentially with the number of joins and that they, not the cost model, dominate bad
//! plans. The second half of the lab measures exactly that: for 100 queries over 3-5 tables, every connected left-deep join order is costed
//! with C_out (sum of intermediate result sizes) using TRUE vs ESTIMATED cardinalities, and the REGRET of the order each estimator picks
//! (true cost of its pick / true cost of the optimum) is reported.
//!
//! CHECKS (asserted): the wander-join estimator is unbiased on average (many walks); on queries with correlated predicates the median
//! q-error of the independence estimator exceeds those of the sampling and the learned estimator; independence gets worse with the
//! number of joins; the true-cardinality planner has regret exactly 1 and every estimator regret >= 1.
//! Read: Cost Models: Cardinality Estimation, CMU 15-799 lectures 13-15 https://15799.courses.cs.cmu.edu/spring2025/schedule.html
//!       How Good are Query Optimizers, Really? (Leis et al., VLDB 2015)
//!       Query Optimization Through the Looking Glass, and What We Found Running the Join Order Benchmark (Leis et al., VLDB Journal 2017)
//!       Analyzing the Impact of Cardinality Estimation on Execution Plans in Microsoft SQL Server (Lee et al., VLDB 2023)
//!       Learned Cardinality Estimation: An In-depth Study (Kim et al., SIGMOD 2022)
//!       PostgreSQL planner statistics https://www.postgresql.org/docs/current/planner-stats.html

use std::collections::{BTreeMap, HashMap};

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

struct Zipf(Vec<f64>);
impl Zipf {
    fn new(n: usize, s: f64) -> Self {
        let mut acc = 0.0;
        let mut cdf: Vec<f64> = (1..=n)
            .map(|k| {
                acc += 1.0 / (k as f64).powf(s);
                acc
            })
            .collect();
        cdf.iter_mut().for_each(|c| *c /= acc);
        Zipf(cdf)
    }
    fn sample(&self, r: &mut Rng) -> i64 {
        let u = r.unit();
        self.0.partition_point(|&c| c < u).min(self.0.len() - 1) as i64
    }
}

fn qerr(est: f64, actual: f64) -> f64 {
    let (e, a) = (est.max(1.0), actual.max(1.0));
    (e / a).max(a / e)
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() - 1) as f64 * p).round() as usize]
}

// ---------------------------------------------------------------------------------------------------------------------
// Schema and data. Column-major: db.t[table][column][row]. Join keys are dense ids, so a PK lookup is an array index.
// ---------------------------------------------------------------------------------------------------------------------

const TITLE: usize = 0;
const PERSON: usize = 1;
const CAST: usize = 2;
const COMPANY: usize = 3;
const MCOMP: usize = 4;
const MKEY: usize = 5;
const NT: usize = 6;
const TNAMES: [&str; NT] = [
    "title",
    "person",
    "cast_info",
    "company",
    "movie_company",
    "movie_keyword",
];

/// (child table, fk column, parent table, pk column): every join in the workload is one of these FK/PK equi-joins.
const EDGES: [(usize, usize, usize, usize); 5] = [
    (CAST, 0, TITLE, 0),
    (CAST, 1, PERSON, 0),
    (MCOMP, 0, TITLE, 0),
    (MCOMP, 1, COMPANY, 0),
    (MKEY, 0, TITLE, 0),
];
/// Columns that queries may filter on: (table, column). title(id,kind,year,genre) person(id,gender,birth) cast(movie,person,role)
/// company(id,country) movie_company(movie,company,ctype) movie_keyword(movie,kw)
const FCOLS: [(usize, usize); 9] = [
    (TITLE, 1),
    (TITLE, 2),
    (TITLE, 3),
    (PERSON, 1),
    (PERSON, 2),
    (CAST, 2),
    (COMPANY, 1),
    (MCOMP, 2),
    (MKEY, 1),
];

struct Db {
    t: Vec<Vec<Vec<i64>>>,
}

impl Db {
    fn rows(&self, t: usize) -> usize {
        self.t[t][0].len()
    }
}

fn gen_db(seed: u64) -> Db {
    let mut r = Rng(seed);
    let (nt, np, nc, ncast, nmc, nmk) = (
        3000usize,
        3000usize,
        300usize,
        20_000usize,
        8000usize,
        12_000usize,
    );
    let zt = Zipf::new(nt, 0.9); // popularity of titles: low id = blockbuster
    let zkind = Zipf::new(7, 0.8);
    // title: kind correlates with popularity (hot titles are kind 0) and genre correlates with kind.
    let (mut kind, mut year, mut genre) = (vec![], vec![], vec![]);
    for id in 0..nt {
        let k = if id < 150 && r.chance(0.7) {
            0
        } else {
            zkind.sample(&mut r)
        };
        kind.push(k);
        year.push(1950 + (70.0 * r.unit().sqrt()) as i64); // recent years more frequent
        genre.push(if r.chance(0.85) {
            (k * 3 + r.below(2) as i64) % 20
        } else {
            r.below(20) as i64
        });
    }
    let title = vec![(0..nt as i64).collect(), kind, year, genre];
    let gender: Vec<i64> = (0..np).map(|_| r.below(2) as i64).collect();
    let person = vec![
        (0..np as i64).collect(),
        gender.clone(),
        (0..np).map(|_| 1900 + r.below(100) as i64).collect(),
    ];
    let zp = Zipf::new(np, 0.7);
    let (mut c0, mut c1, mut c2) = (vec![], vec![], vec![]);
    for _ in 0..ncast {
        let p = zp.sample(&mut r);
        c0.push(zt.sample(&mut r));
        c1.push(p);
        // role depends on the gender of the person it joins to: a correlation that spans the cast-person join.
        c2.push(if r.chance(0.9) {
            gender[p as usize] * 6 + r.below(6) as i64
        } else {
            r.below(12) as i64
        });
    }
    let zc = Zipf::new(30, 1.0);
    let company = vec![
        (0..nc as i64).collect(),
        (0..nc).map(|_| zc.sample(&mut r)).collect(),
    ];
    let zco = Zipf::new(nc, 0.9);
    let mcomp = vec![
        (0..nmc).map(|_| zt.sample(&mut r)).collect(),
        (0..nmc).map(|_| zco.sample(&mut r)).collect(),
        (0..nmc).map(|_| r.below(4) as i64).collect(),
    ];
    let zk = Zipf::new(800, 1.0);
    let mkey = vec![
        (0..nmk).map(|_| zt.sample(&mut r)).collect(),
        (0..nmk).map(|_| zk.sample(&mut r)).collect(),
    ];
    Db {
        t: vec![title, person, vec![c0, c1, c2], company, mcomp, mkey],
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Queries and exact execution
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct Pred {
    t: usize,
    c: usize,
    lo: i64,
    hi: i64,
}

#[derive(Clone, Debug)]
struct Query {
    mask: u32, // set of joined tables
    preds: Vec<Pred>,
}

impl Query {
    fn has(&self, t: usize) -> bool {
        self.mask >> t & 1 == 1
    }
    fn tables(&self) -> Vec<usize> {
        (0..NT).filter(|&t| self.has(t)).collect()
    }
    fn edges(&self) -> Vec<usize> {
        (0..EDGES.len())
            .filter(|&e| self.has(EDGES[e].0) && self.has(EDGES[e].2))
            .collect()
    }
    fn pred(&self, t: usize, c: usize) -> Option<&Pred> {
        self.preds.iter().find(|p| p.t == t && p.c == c)
    }
    /// The same predicates restricted to a subset of the tables (a sub-plan of the query).
    fn restrict(&self, mask: u32) -> Query {
        Query {
            mask,
            preds: self
                .preds
                .iter()
                .filter(|p| mask >> p.t & 1 == 1)
                .cloned()
                .collect(),
        }
    }
}

fn passes(db: &Db, q: &Query, t: usize, r: usize) -> bool {
    q.preds
        .iter()
        .filter(|p| p.t == t)
        .all(|p| (p.lo..=p.hi).contains(&db.t[t][p.c][r]))
}

/// The edges of the query touching table `t`, as (edge id, my column, other table, other column), excluding the way back to `from`.
fn incident(q: &Query, t: usize, from: Option<usize>) -> Vec<(usize, usize, usize, usize)> {
    q.edges()
        .into_iter()
        .filter_map(|e| {
            let (ct, cc, pt, pc) = EDGES[e];
            let (mine, o, oc) = if ct == t {
                (cc, pt, pc)
            } else if pt == t {
                (pc, ct, cc)
            } else {
                return None;
            };
            (Some(o) != from).then_some((e, mine, o, oc))
        })
        .collect()
}

/// Exact row weights of `t`: the number of join results of t's subtree that each surviving row participates in. The join graph of
/// every query is a tree, so a bottom-up pass multiplies per-key sums of the children into the parent rows.
fn weights(db: &Db, q: &Query, t: usize, from: Option<usize>) -> Vec<f64> {
    let mut w: Vec<f64> = (0..db.rows(t))
        .map(|r| passes(db, q, t, r) as u8 as f64)
        .collect();
    for (_, mine, o, oc) in incident(q, t, from) {
        let ow = weights(db, q, o, Some(t));
        let max_key = *db.t[o][oc].iter().max().unwrap() as usize;
        let mut sum = vec![0.0; max_key + 1];
        for (r, &x) in ow.iter().enumerate() {
            sum[db.t[o][oc][r] as usize] += x;
        }
        for r in 0..w.len() {
            if w[r] > 0.0 {
                w[r] *= sum.get(db.t[t][mine][r] as usize).copied().unwrap_or(0.0);
            }
        }
    }
    w
}

fn true_card(db: &Db, q: &Query) -> f64 {
    let root = q.tables()[0];
    weights(db, q, root, None).iter().sum()
}

// ---------------------------------------------------------------------------------------------------------------------
// Random workload
// ---------------------------------------------------------------------------------------------------------------------

fn neighbours(mask: u32) -> Vec<usize> {
    (0..NT)
        .filter(|&t| mask >> t & 1 == 0)
        .filter(|&t| {
            EDGES.iter().any(|&(c, _, p, _)| {
                (c == t && mask >> p & 1 == 1) || (p == t && mask >> c & 1 == 1)
            })
        })
        .collect()
}

/// A random connected set of `k` tables, by growing from a random seed table along join edges.
fn random_mask(r: &mut Rng, k: usize) -> u32 {
    let mut mask = 1u32 << r.below(NT as u64);
    while (mask.count_ones() as usize) < k {
        let nb = neighbours(mask);
        mask |= 1 << nb[r.below(nb.len() as u64) as usize];
    }
    mask
}

fn random_pred(db: &Db, r: &mut Rng, t: usize, c: usize) -> Pred {
    let v = db.t[t][c][r.below(db.rows(t) as u64) as usize]; // value taken from a real row: filters hit existing data
    let width = if (t, c) == (TITLE, 2) || (t, c) == (PERSON, 2) {
        r.below(20) as i64
    } else {
        0
    };
    Pred {
        t,
        c,
        lo: v - width,
        hi: v + width,
    }
}

fn random_query(db: &Db, r: &mut Rng, k: usize) -> Query {
    let mask = random_mask(r, k);
    let mut preds: Vec<Pred> = vec![];
    for t in (0..NT).filter(|&t| mask >> t & 1 == 1) {
        let cols: Vec<usize> = FCOLS.iter().filter(|f| f.0 == t).map(|f| f.1).collect();
        if !cols.is_empty() && r.chance(0.5) {
            let c = cols[r.below(cols.len() as u64) as usize];
            preds.push(random_pred(db, r, t, c));
        }
    }
    // Correlated predicate pairs: the situations where multiplying selectivities is wrong.
    if mask & 1 == 1 && r.chance(0.4) {
        let k = r.below(7) as i64;
        preds.retain(|p| !(p.t == TITLE && (p.c == 1 || p.c == 3)));
        preds.push(Pred {
            t: TITLE,
            c: 1,
            lo: k,
            hi: k,
        });
        preds.push(Pred {
            t: TITLE,
            c: 3,
            lo: k * 3 % 20,
            hi: k * 3 % 20,
        });
    }
    if mask >> PERSON & 1 == 1 && mask >> CAST & 1 == 1 && r.chance(0.4) {
        let g = r.below(2) as i64;
        preds.retain(|p| !((p.t == PERSON && p.c == 1) || (p.t == CAST && p.c == 2)));
        preds.push(Pred {
            t: PERSON,
            c: 1,
            lo: g,
            hi: g,
        });
        preds.push(Pred {
            t: CAST,
            c: 2,
            lo: g * 6,
            hi: g * 6 + 5,
        });
    }
    Query { mask, preds }
}

fn correlated(q: &Query) -> bool {
    (q.pred(TITLE, 1).is_some() && q.pred(TITLE, 3).is_some())
        || (q.pred(PERSON, 1).is_some() && q.pred(CAST, 2).is_some())
}

// ---------------------------------------------------------------------------------------------------------------------
// Statistics shared by the estimators
// ---------------------------------------------------------------------------------------------------------------------

const BKT: usize = 64; // buckets of the aligned-bucket join histogram

struct Stats {
    rows: Vec<f64>,
    freq: Vec<Vec<BTreeMap<i64, u64>>>, // exact per-column marginals: the best possible single-column statistics
    edge_ndv: Vec<f64>,                 // max(ndv(fk), ndv(pk)) per edge
    children: Vec<Vec<Vec<u32>>>, // per edge: parent key -> child rows (the index wander join walks)
}

fn build_stats(db: &Db) -> Stats {
    let rows = (0..NT).map(|t| db.rows(t) as f64).collect();
    let freq = (0..NT)
        .map(|t| {
            db.t[t]
                .iter()
                .map(|col| {
                    let mut m = BTreeMap::new();
                    col.iter().for_each(|&v| *m.entry(v).or_default() += 1);
                    m
                })
                .collect()
        })
        .collect::<Vec<Vec<BTreeMap<i64, u64>>>>();
    let (mut edge_ndv, mut children) = (vec![], vec![]);
    for &(ct, cc, pt, pc) in &EDGES {
        edge_ndv.push(freq[ct][cc].len().max(freq[pt][pc].len()) as f64);
        let mut idx = vec![vec![]; db.rows(pt)];
        for (r, &k) in db.t[ct][cc].iter().enumerate() {
            idx[k as usize].push(r as u32);
        }
        children.push(idx);
    }
    Stats {
        rows,
        freq,
        edge_ndv,
        children,
    }
}

impl Stats {
    fn sel(&self, p: &Pred) -> f64 {
        let hit: u64 = self.freq[p.t][p.c]
            .range(p.lo..=p.hi)
            .map(|(_, &c)| c)
            .sum();
        hit as f64 / self.rows[p.t]
    }
    /// Filtered cardinality of table t assuming its predicates are independent.
    fn base(&self, q: &Query, t: usize) -> f64 {
        q.preds
            .iter()
            .filter(|p| p.t == t)
            .fold(self.rows[t], |acc, p| acc * self.sel(p))
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Estimator 1 and 2: classic and histogram join
// ---------------------------------------------------------------------------------------------------------------------

fn est_independence(s: &Stats, q: &Query) -> f64 {
    let base: f64 = q.tables().iter().map(|&t| s.base(q, t)).product();
    q.edges().iter().fold(base, |acc, &e| acc / s.edge_ndv[e])
}

/// Aligned-bucket join per edge on the FILTERED inputs: bucket the join key into BKT equi-width buckets on both sides (count and ndv
/// per bucket), then size = sum over buckets of c1 * c2 / max(ndv1, ndv2). Captures key skew AND predicate/key correlation per edge.
fn est_histjoin(db: &Db, s: &Stats, q: &Query) -> f64 {
    let mut est: f64 = q.tables().iter().map(|&t| s.base(q, t)).product();
    for e in q.edges() {
        let (ct, cc, pt, pc) = EDGES[e];
        let side = |t: usize, c: usize| {
            let (mut cnt, mut seen) = (vec![0.0; BKT], vec![std::collections::HashSet::new(); BKT]);
            let mut n = 0.0;
            for r in (0..db.rows(t)).filter(|&r| passes(db, q, t, r)) {
                let k = db.t[t][c][r];
                let b = (k * BKT as i64 / db.rows(pt) as i64) as usize;
                cnt[b] += 1.0;
                seen[b].insert(k);
                n += 1.0;
            }
            (
                cnt,
                seen.iter().map(|x| x.len() as f64).collect::<Vec<f64>>(),
                n,
            )
        };
        let ((ca, na, n1), (cb, nb, n2)) = (side(ct, cc), side(pt, pc));
        if n1 == 0.0 || n2 == 0.0 {
            return 0.0;
        }
        let size: f64 = (0..BKT)
            .filter(|&b| na[b].max(nb[b]) > 0.0)
            .map(|b| ca[b] * cb[b] / na[b].max(nb[b]))
            .sum();
        est *= size / (n1 * n2);
    }
    est
}

// ---------------------------------------------------------------------------------------------------------------------
// Estimator 3: wander join (random walks along the indexes)
// ---------------------------------------------------------------------------------------------------------------------

/// One random walk from `row` of table `t` through its subtree; returns the Horvitz-Thompson weight (0 when the walk dies).
fn walk(
    db: &Db,
    s: &Stats,
    q: &Query,
    t: usize,
    row: usize,
    from: Option<usize>,
    r: &mut Rng,
) -> f64 {
    let mut w = 1.0;
    for (e, mine, o, _) in incident(q, t, from) {
        let next = if EDGES[e].2 == t {
            // I am the parent: any of the children whose fk equals my key; fanout = number of candidates.
            let cands = &s.children[e][db.t[t][EDGES[e].3][row] as usize];
            if cands.is_empty() {
                return 0.0;
            }
            w *= cands.len() as f64;
            cands[r.below(cands.len() as u64) as usize] as usize
        } else {
            db.t[t][mine][row] as usize // I am the child: the parent is the single row addressed by the fk
        };
        if !passes(db, q, o, next) {
            return 0.0;
        }
        w *= walk(db, s, q, o, next, Some(t), r);
        if w == 0.0 {
            return 0.0;
        }
    }
    w
}

fn est_wander(db: &Db, s: &Stats, q: &Query, walks: usize, r: &mut Rng) -> f64 {
    // Root = the table with the smallest estimated filtered size; its filtered rows are enumerated once (a cheap scan).
    let root = *q
        .tables()
        .iter()
        .min_by(|&&a, &&b| s.base(q, a).partial_cmp(&s.base(q, b)).unwrap())
        .unwrap();
    let rows: Vec<usize> = (0..db.rows(root))
        .filter(|&i| passes(db, q, root, i))
        .collect();
    if rows.is_empty() {
        return 0.0;
    }
    let total: f64 = (0..walks)
        .map(|_| {
            walk(
                db,
                s,
                q,
                root,
                rows[r.below(rows.len() as u64) as usize],
                None,
                r,
            )
        })
        .sum();
    rows.len() as f64 * total / walks as f64
}

// ---------------------------------------------------------------------------------------------------------------------
// Estimator 4: Fast-AGMS join sketches
// ---------------------------------------------------------------------------------------------------------------------

const SW: usize = 1024;
const SD: usize = 3;

struct Agms(Vec<Vec<f64>>);
impl Agms {
    fn new() -> Self {
        Agms(vec![vec![0.0; SW]; SD])
    }
    fn add(&mut self, key: i64) {
        for d in 0..SD {
            let h = mix(key as u64 ^ (d as u64 + 1).wrapping_mul(0x51_7C_C1_B7_27_22_0A_95));
            self.0[d][(h % SW as u64) as usize] += if h >> 63 == 0 { 1.0 } else { -1.0 }; // +-1 sign: cross terms cancel in expectation
        }
    }
    /// Median over rows of the bucket-wise dot product: an unbiased estimate of sum_k fa(k) * fb(k).
    fn join(&self, o: &Agms) -> f64 {
        let mut v: Vec<f64> = (0..SD)
            .map(|d| self.0[d].iter().zip(&o.0[d]).map(|(a, b)| a * b).sum())
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[SD / 2]
    }
}

fn est_sketch(db: &Db, s: &Stats, q: &Query) -> f64 {
    let base: f64 = q.tables().iter().map(|&t| s.base(q, t)).product();
    let mut est = base;
    for e in q.edges() {
        let (ct, cc, pt, pc) = EDGES[e];
        let (mut a, mut b) = (Agms::new(), Agms::new());
        let (mut na, mut nb) = (0.0, 0.0);
        (0..db.rows(ct))
            .filter(|&r| passes(db, q, ct, r))
            .for_each(|r| {
                a.add(db.t[ct][cc][r]);
                na += 1.0
            });
        (0..db.rows(pt))
            .filter(|&r| passes(db, q, pt, r))
            .for_each(|r| {
                b.add(db.t[pt][pc][r]);
                nb += 1.0
            });
        if na == 0.0 || nb == 0.0 {
            return 0.0;
        }
        est *= a.join(&b).max(0.0) / (na * nb); // join selectivity of the FILTERED endpoints
    }
    est
}

// ---------------------------------------------------------------------------------------------------------------------
// Estimator 5: learned ridge regression on query features
// ---------------------------------------------------------------------------------------------------------------------

fn features(db: &Db, s: &Stats, q: &Query) -> Vec<f64> {
    let mut f = vec![1.0];
    f.extend((0..NT).map(|t| q.has(t) as u8 as f64));
    f.push(q.edges().len() as f64);
    f.push((est_independence(s, q) + 1.0).ln());
    f.push((est_histjoin(db, s, q) + 1.0).ln());
    f.extend(
        FCOLS
            .iter()
            .map(|&(t, c)| q.pred(t, c).map_or(0.0, |p| s.sel(p).max(1e-9).ln())),
    );
    // Interaction features (hand-written hidden units): does the predicate pair follow the correlation in the data?
    let kg = match (q.pred(TITLE, 1), q.pred(TITLE, 3)) {
        (Some(k), Some(g)) => (k.lo == k.hi && g.lo == g.hi && g.lo == k.lo * 3 % 20) as u8 as f64,
        _ => 0.0,
    };
    let rg = match (q.pred(PERSON, 1), q.pred(CAST, 2)) {
        (Some(g), Some(r)) => {
            (g.lo == g.hi && r.lo >= g.lo * 6 && r.hi <= g.lo * 6 + 5) as u8 as f64
        }
        _ => 0.0,
    };
    let k0 = q
        .pred(TITLE, 1)
        .map_or(0.0, |p| (p.lo <= 0 && p.hi >= 0) as u8 as f64);
    f.extend([
        kg,
        rg,
        k0,
        k0 * q.has(CAST) as u8 as f64,
        k0 * q.has(MCOMP) as u8 as f64,
        k0 * q.has(MKEY) as u8 as f64,
    ]);
    f
}

/// Solve (X^T X + lambda I) w = X^T y by Gaussian elimination with partial pivoting; the bias term is not regularised.
fn ridge(xs: &[Vec<f64>], ys: &[f64], lambda: f64) -> Vec<f64> {
    let n = xs[0].len();
    let mut a = vec![vec![0.0; n + 1]; n];
    for (x, &y) in xs.iter().zip(ys) {
        for i in 0..n {
            for j in 0..n {
                a[i][j] += x[i] * x[j];
            }
            a[i][n] += x[i] * y;
        }
    }
    (1..n).for_each(|i| a[i][i] += lambda);
    for c in 0..n {
        let p = (c..n)
            .max_by(|&i, &j| a[i][c].abs().partial_cmp(&a[j][c].abs()).unwrap())
            .unwrap();
        a.swap(c, p);
        for r in 0..n {
            if r != c && a[c][c].abs() > 1e-12 {
                let m = a[r][c] / a[c][c];
                for k in c..=n {
                    a[r][k] -= m * a[c][k];
                }
            }
        }
    }
    (0..n)
        .map(|i| {
            if a[i][i].abs() > 1e-12 {
                a[i][n] / a[i][i]
            } else {
                0.0
            }
        })
        .collect()
}

struct Model(Vec<f64>);
impl Model {
    fn est(&self, db: &Db, s: &Stats, q: &Query) -> f64 {
        let z: f64 = features(db, s, q)
            .iter()
            .zip(&self.0)
            .map(|(x, w)| x * w)
            .sum();
        (z.exp() - 1.0).max(0.0)
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Experiments
// ---------------------------------------------------------------------------------------------------------------------

fn check_wander_unbiased(db: &Db, s: &Stats, r: &mut Rng) {
    println!("\n== wander join unbiasedness: 12 queries x 200000 walks ==");
    let (mut ratios, mut n) = (vec![], 0);
    while ratios.len() < 12 {
        n += 1;
        let q = random_query(db, r, 3);
        let truth = true_card(db, &q);
        if truth < 200.0 {
            continue;
        }
        let est = est_wander(db, s, &q, 200_000, r);
        ratios.push(est / truth);
    }
    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    println!(
        "  est/true ratios: {}",
        ratios
            .iter()
            .map(|x| format!("{x:.3}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!("  mean ratio {mean:.3} (candidates drawn: {n})");
    assert!(
        (mean - 1.0).abs() < 0.1,
        "wander join must be unbiased, mean ratio {mean}"
    );
}

type EstFn<'a> = Box<dyn FnMut(&Query) -> f64 + 'a>;

fn all_cards(est: &mut EstFn, q: &Query) -> HashMap<u32, f64> {
    let mut m = HashMap::new();
    let full = q.mask;
    let mut sub = full;
    while sub > 0 {
        if sub.count_ones() >= 2 && connected(sub) {
            m.insert(sub, est(&q.restrict(sub)));
        }
        sub = (sub - 1) & full;
    }
    m
}

fn connected(mask: u32) -> bool {
    let first = mask.trailing_zeros();
    let (mut seen, mut grew) = (1u32 << first, true);
    while grew {
        grew = false;
        for t in 0..NT {
            if mask >> t & 1 == 1
                && seen >> t & 1 == 0
                && EDGES.iter().any(|&(c, _, p, _)| {
                    (c == t && seen >> p & 1 == 1) || (p == t && seen >> c & 1 == 1)
                })
            {
                seen |= 1 << t;
                grew = true;
            }
        }
    }
    seen == mask
}

/// All left-deep orders where every prefix is a connected join (no cross products).
fn orders(mask: u32) -> Vec<Vec<usize>> {
    fn go(mask: u32, cur: &mut Vec<usize>, set: u32, out: &mut Vec<Vec<usize>>) {
        if set == mask {
            out.push(cur.clone());
            return;
        }
        for t in 0..NT {
            if mask >> t & 1 == 1 && set >> t & 1 == 0 && (set == 0 || neighbours(set).contains(&t))
            {
                cur.push(t);
                go(mask, cur, set | 1 << t, out);
                cur.pop();
            }
        }
    }
    let mut out = vec![];
    go(mask, &mut vec![], 0, &mut out);
    out
}

fn order_cost(order: &[usize], cards: &HashMap<u32, f64>) -> f64 {
    let mut set = 1u32 << order[0];
    order[1..]
        .iter()
        .map(|&t| {
            set |= 1 << t;
            cards[&set]
        })
        .sum()
}

fn main() {
    let db = gen_db(761);
    let stats = build_stats(&db);
    println!("== synthetic IMDB-like schema ==");
    for t in 0..NT {
        println!(
            "  {:<14} {:>6} rows, {} columns",
            TNAMES[t],
            db.rows(t),
            db.t[t].len()
        );
    }
    let mut rq = Rng(100);
    check_wander_unbiased(&db, &stats, &mut rq);

    // Train the learned model on 1200 queries; test everything on 500 fresh ones.
    let mut tr = Rng(1);
    let train: Vec<Query> = (0..1200)
        .map(|i| random_query(&db, &mut tr, 1 + i % 4))
        .collect();
    let xs: Vec<Vec<f64>> = train.iter().map(|q| features(&db, &stats, q)).collect();
    let ys: Vec<f64> = train
        .iter()
        .map(|q| (true_card(&db, q) + 1.0).ln())
        .collect();
    let model = Model(ridge(&xs, &ys, 1.0));
    let mut te = Rng(2);
    let test: Vec<Query> = (0..500)
        .map(|i| random_query(&db, &mut te, 1 + i % 4))
        .collect();
    let truth: Vec<f64> = test.iter().map(|q| true_card(&db, q)).collect();

    let mut rs = Rng(3);
    type Method<'a> = (&'a str, Vec<f64>);
    let methods: Vec<Method> = vec![
        (
            "independence",
            test.iter().map(|q| est_independence(&stats, q)).collect(),
        ),
        (
            "histogram-join",
            test.iter().map(|q| est_histjoin(&db, &stats, q)).collect(),
        ),
        (
            "wander (1000)",
            test.iter()
                .map(|q| est_wander(&db, &stats, q, 1000, &mut rs))
                .collect(),
        ),
        (
            "fast-agms",
            test.iter().map(|q| est_sketch(&db, &stats, q)).collect(),
        ),
        (
            "learned ridge",
            test.iter().map(|q| model.est(&db, &stats, q)).collect(),
        ),
    ];
    let qe = |m: &Vec<f64>, pick: &dyn Fn(usize) -> bool| -> Vec<f64> {
        (0..test.len())
            .filter(|&i| pick(i))
            .map(|i| qerr(m[i], truth[i]))
            .collect()
    };
    println!(
        "\n== q-error over 500 unseen queries with 1-4 tables ({} correlated-predicate queries) ==",
        test.iter().filter(|q| correlated(q)).count()
    );
    println!(
        "{:<16} {:>8} {:>8} {:>8} {:>10} | {:>12}",
        "method", "median", "p90", "p99", "max", "median(corr)"
    );
    let mut med_corr: HashMap<&str, f64> = HashMap::new();
    for (name, m) in &methods {
        let all = qe(m, &|_| true);
        let c = qe(m, &|i| correlated(&test[i]));
        let mc = pct(&c, 0.5);
        med_corr.insert(name, mc);
        println!(
            "{:<16} {:>8.2} {:>8.1} {:>8.1} {:>10.1} | {:>12.2}",
            name,
            pct(&all, 0.5),
            pct(&all, 0.9),
            pct(&all, 0.99),
            pct(&all, 1.0),
            mc
        );
    }
    println!("\nindependence q-error by number of tables (errors compound with joins):");
    let indep = &methods[0].1;
    let mut by_n = vec![];
    for n in 1..=4u32 {
        let v = qe(indep, &|i| test[i].mask.count_ones() == n);
        let (m, p90) = (pct(&v, 0.5), pct(&v, 0.9));
        println!(
            "  {n} table(s): n={:>3} median {:>7.2} p90 {:>9.1}",
            v.len(),
            m,
            p90
        );
        by_n.push(m);
    }
    assert!(
        med_corr["independence"] > med_corr["wander (1000)"],
        "independence must lose to sampling on correlated queries"
    );
    assert!(
        med_corr["independence"] > med_corr["learned ridge"],
        "independence must lose to the learned model on correlated queries"
    );
    assert!(
        by_n[3] > by_n[0],
        "independence error must compound with the number of joins"
    );

    // Plan choice: true best left-deep order vs the order chosen with each estimator's cardinalities.
    println!("\n== plan regret: 100 queries over 3-5 tables, C_out = sum of intermediate sizes ==");
    let mut rp = Rng(4);
    let plan_qs: Vec<Query> = (0..100)
        .map(|i| random_query(&db, &mut rp, 3 + i % 3))
        .collect();
    let mut rw = Rng(5);
    let mut regrets: Vec<(&str, Vec<f64>)> = [
        "truth",
        "independence",
        "histogram-join",
        "wander (300)",
        "learned ridge",
    ]
    .iter()
    .map(|&n| (n, vec![]))
    .collect();
    for q in &plan_qs {
        let mut estimators: Vec<EstFn> = vec![
            Box::new(|s: &Query| true_card(&db, s)),
            Box::new(|s: &Query| est_independence(&stats, s)),
            Box::new(|s: &Query| est_histjoin(&db, &stats, s)),
            Box::new(|s: &Query| est_wander(&db, &stats, s, 300, &mut rw)),
            Box::new(|s: &Query| model.est(&db, &stats, s)),
        ];
        let cards: Vec<HashMap<u32, f64>> =
            estimators.iter_mut().map(|e| all_cards(e, q)).collect();
        let ords = orders(q.mask);
        let true_cost = |o: &Vec<usize>| order_cost(o, &cards[0]);
        let best = ords.iter().map(&true_cost).fold(f64::INFINITY, f64::min);
        for (i, (_, v)) in regrets.iter_mut().enumerate() {
            let pick = ords
                .iter()
                .min_by(|a, b| {
                    order_cost(a, &cards[i])
                        .partial_cmp(&order_cost(b, &cards[i]))
                        .unwrap()
                })
                .unwrap();
            v.push((true_cost(pick) + 1.0) / (best + 1.0)); // +1: a plan costing 0 rows is still 'optimal'
        }
    }
    println!(
        "{:<16} {:>10} {:>10} {:>10} {:>12}",
        "estimator", "mean", "p90", "max", "plans optimal"
    );
    for (name, v) in &regrets {
        let opt = v.iter().filter(|&&x| x < 1.0 + 1e-9).count();
        println!(
            "{:<16} {:>10.3} {:>10.3} {:>10.2} {:>10}/100",
            name,
            v.iter().sum::<f64>() / v.len() as f64,
            pct(v, 0.9),
            pct(v, 1.0),
            opt
        );
        assert!(
            v.iter().all(|&x| x >= 1.0 - 1e-9),
            "{name}: regret below 1 is impossible"
        );
    }
    assert!(
        regrets[0].1.iter().all(|&x| (x - 1.0).abs() < 1e-9),
        "the planner fed true cardinalities must be optimal"
    );
    println!("\nall cardinality lab checks passed");
}
