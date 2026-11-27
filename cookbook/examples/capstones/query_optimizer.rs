//! CAPSTONE 08 (database): a cost-based query optimizer in miniature: statistics -> cardinality estimates -> rewrite rules -> physical costing -> join ordering -> Volcano execution.
//! LEVEL: capstone
//! Use in: understanding EXPLAIN output, why a planner picks a bad plan, building a planner for an embedded engine or a DSL.   Run: cargo run --release --example query_optimizer
#![allow(dead_code)]
//!
//! WHAT: the pipeline every cost-based optimizer shares, in one standalone file (std only, seeded xorshift, deterministic):
//!   1 CATALOG + STATISTICS   row counts, NDV, min/max, null fraction, equi-height histograms, most-common-values (MCV) from generated data.
//!   2 CARDINALITY ESTIMATION selectivity for = < range IN AND(independence) OR(inclusion-exclusion), join sel 1/max(ndv); compared with TRUE counts (q-error).
//!   3 LOGICAL PLAN + RULES   a rule engine run to a FIXPOINT: constant folding, conjunction splitting, predicate pushdown, join extraction, projection pruning.
//!   4 PHYSICAL + COST MODEL  SeqScan / IndexScan / NestedLoop / HashJoin / SortMergeJoin(+Sort), cost = io pages + cpu per tuple, interesting orders.
//!   5 JOIN ORDERING          Selinger DP over bitmask subsets (left-deep and bushy, no cross products unless forced) + greedy GOO heuristic.
//!   6 EXPLAIN                plan tree with estimated rows and cost per node.   7 EXECUTION   Volcano iterators (`next()`), so plans really run.
//! COST MODEL (all in "page fetch" units): 100 rows/page; sequential page 1.0; random page 4.0; per-tuple CPU 0.01; per-predicate/compare CPU 0.0025;
//!   hash build 0.02/row, probe 0.01/row; sort n*log2(n)*0.005. NestedLoop materialises the inner side once (so inner cost is paid once, compares are l*r).
//! WHY ESTIMATES BREAK: independence (correlated columns multiply selectivities that should not be multiplied), skew (1/ndv assumes uniformity),
//!   and stale or coarse statistics. The demo prints those q-errors on purpose.
//! INTERESTING ORDERS: a sorted input (IndexScan key order, or a MergeJoin output) lets a later SortMergeJoin skip its Sort, so the DP keeps the
//!   cheapest plan PER OUTPUT ORDER for each subset, not just the cheapest overall (Selinger's key insight).
//! NOT IMPLEMENTED: a Cascades memo (groups of equivalent expressions + transformation/implementation rules + top-down search with pruning). The DP
//!   here is the bottom-up ancestor; Cascades generalises it so new rules plug in without rewriting the search.
//! VERIFICATION (asserts): (a) DP == brute force over all join trees (n<=5); (b) optimised == naive results over 200 random queries; (c) on those
//!   non-empty tables the first rule batch never raises estimated cost; (d) greedy >= DP; (e) q-error bounded on uniform data, large on correlated data;
//!   (f) IndexScan vs SeqScan choice; (g) hash build is smaller side; (h) regressions: NULL, the ends of i64, folding (differential, with NULL in IN lists and numbers under AND/OR),
//!   empty tables, refused queries, EXPLAIN brackets.
//! LIMITS: projection pruning is demonstrated but `optimize` does not use it (this cost model charges CPU for each added Project, so it can raise the
//!   estimate). Pushing a filter below a join with an EMPTY side can raise the estimate too. Join selectivity ignores NULL fractions. The brute-force
//!   check shares the cost functions and the keep-per-order pruning with the DP, so it checks the enumeration, not those. Index fetches are capped at the
//!   table's page count, which flatters an unclustered index. At most 12 tables per query, table ids below 32, no self-joins, no depth limit on expressions.
//! COMMON USES: PostgreSQL's planner (Selinger-style DP, histograms + MCV lists, `EXPLAIN`), Apache Spark Catalyst and Apache DataFusion (rule batches run to
//!   fixpoint, predicate pushdown, projection pruning, cost-based join reorder), CockroachDB and the Volcano/Cascades papers (memo + rules); also
//!   index advisors, "why is my query slow" tooling and what-if planners.
//! Read: https://15445.courses.cs.cmu.edu/ and https://github.com/apache/datafusion and the paper "Access Path Selection in a Relational Database Management System" (Selinger et al., 1979)
use std::collections::{BTreeMap, HashMap, VecDeque};

// ---------- tiny utilities: seeded RNG, Zipf sampler ----------
const NULL: i64 = i64::MIN; // NULL sentinel; every comparison with NULL is false
/// A condition holds only for a non-zero value that is not NULL. (There is no NOT here, so UNKNOWN may be treated as false.)
fn truthy(v: i64) -> bool {
    v != 0 && v != NULL
}
type Row = Vec<i64>;

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
        (self.next() >> 16) % n
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

/// Zipf(s): value k has weight 1/(k+1)^s, so value 0 is hot. Sampling inverts the CDF by binary search.
struct Zipf(Vec<f64>);
impl Zipf {
    fn new(n: usize, s: f64) -> Self {
        let w: Vec<f64> = (1..=n).map(|k| 1.0 / (k as f64).powf(s)).collect();
        let tot: f64 = w.iter().sum();
        let mut acc = 0.0;
        Zipf(
            w.iter()
                .map(|x| {
                    acc += x / tot;
                    acc
                })
                .collect(),
        )
    }
    fn sample(&self, r: &mut Rng) -> i64 {
        let u = r.unit();
        self.0.partition_point(|&c| c < u).min(self.0.len() - 1) as i64
    }
}

// ---------- 1. CATALOG + STATISTICS ----------
// A real optimizer never looks at the data while planning; it looks at a compact summary built by ANALYZE.
const HIST_BUCKETS: usize = 32;
const MCV_SLOTS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct ColRef {
    t: usize,
    c: usize,
}

#[derive(Clone)]
struct ColStats {
    ndv: usize,
    min: i64,
    max: i64,
    null_frac: f64,
    /// Equi-HEIGHT histogram: HIST_BUCKETS+1 bounds so every bucket holds the same number of rows. Dense regions get narrow buckets, which is why it
    /// beats equi-width on skewed data.
    hist: Vec<i64>,
    /// Most common values with their frequency (fraction of ALL rows); only values clearly hotter than average are kept.
    mcv: Vec<(i64, f64)>,
}

impl ColStats {
    fn analyze(rows: &[Row], c: usize) -> ColStats {
        let mut vals: Vec<i64> = rows.iter().map(|r| r[c]).filter(|&v| v != NULL).collect();
        vals.sort_unstable();
        let n = rows.len() as f64;
        let mut counts: Vec<(i64, usize)> = Vec::new();
        for &v in &vals {
            match counts.last_mut() {
                Some((k, cnt)) if *k == v => *cnt += 1,
                _ => counts.push((v, 1)),
            }
        }
        let ndv = counts.len().max(1);
        let hist = if vals.is_empty() {
            vec![0; HIST_BUCKETS + 1]
        } else {
            (0..=HIST_BUCKETS)
                .map(|i| vals[i * (vals.len() - 1) / HIST_BUCKETS])
                .collect()
        };
        let avg = vals.len() as f64 / ndv as f64;
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mcv = counts
            .iter()
            .take(MCV_SLOTS)
            .filter(|(_, k)| *k as f64 > 1.25 * avg)
            .map(|&(v, k)| (v, k as f64 / n))
            .collect();
        ColStats {
            ndv,
            min: vals.first().copied().unwrap_or(0),
            max: vals.last().copied().unwrap_or(0),
            null_frac: if rows.is_empty() {
                0.0
            } else {
                1.0 - vals.len() as f64 / n
            },
            hist,
            mcv,
        }
    }

    /// Fraction of NON-NULL rows with value <= v, interpolating linearly inside the bucket (the "uniform within a bucket" assumption).
    fn frac_le(&self, v: i64) -> f64 {
        let (h, b) = (&self.hist, HIST_BUCKETS);
        if v < h[0] {
            return 0.0;
        }
        if v >= h[b] {
            return 1.0;
        }
        let i = h[..b].partition_point(|&x| x <= v) - 1;
        let (lo, hi) = (h[i] as f64, h[i + 1] as f64);
        ((i as f64 + (v as f64 - lo + 1.0) / (hi - lo + 1.0)) / b as f64).min(1.0)
    }
}

#[derive(Clone)]
struct TableMeta {
    name: String,
    cols: Vec<String>,
    rows: usize,
    stats: Vec<ColStats>,
    indexed: Vec<usize>,
}

#[derive(Clone)]
struct Catalog {
    tables: Vec<TableMeta>,
    use_mcv: bool,
}

const DEFAULT_SEL: f64 = 0.33; // what real systems fall back to for predicates they cannot analyse
const MIN_SEL: f64 = 1e-9;

// ---------- expressions ----------
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Mul,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum Expr {
    Lit(i64),
    Col(ColRef),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    In(Box<Expr>, Vec<i64>),
}
use Expr::*;

fn col(t: usize, c: usize) -> Expr {
    Col(ColRef { t, c })
}
fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Bin(op, Box::new(a), Box::new(b))
}
fn and2(a: Expr, b: Expr) -> Expr {
    And(Box::new(a), Box::new(b))
}
fn or2(a: Expr, b: Expr) -> Expr {
    Or(Box::new(a), Box::new(b))
}
fn flatten_and(e: &Expr, out: &mut Vec<Expr>) {
    if let And(a, b) = e {
        flatten_and(a, out);
        flatten_and(b, out);
    } else {
        out.push(e.clone());
    }
}
fn conjuncts(e: &Expr) -> Vec<Expr> {
    let mut v = Vec::new();
    flatten_and(e, &mut v);
    v
}
fn and_all(v: &[Expr]) -> Expr {
    v.iter().cloned().reduce(and2).unwrap_or(Lit(1))
}
fn refs(e: &Expr, out: &mut Vec<ColRef>) {
    match e {
        Lit(_) => {}
        Col(c) => out.push(*c),
        Bin(_, a, b) | And(a, b) | Or(a, b) => {
            refs(a, out);
            refs(b, out);
        }
        In(x, _) => refs(x, out),
    }
}
fn col_refs(e: &Expr) -> Vec<ColRef> {
    let mut v = Vec::new();
    refs(e, &mut v);
    v
}
/// Bitmask of table ids an expression touches.
fn etables(e: &Expr) -> u32 {
    col_refs(e).iter().fold(0, |m, c| m | 1 << c.t)
}

fn eval(e: &Expr, row: &[i64], schema: &[ColRef]) -> i64 {
    match e {
        Lit(v) => *v,
        Col(c) => {
            row[schema
                .iter()
                .position(|s| s == c)
                .expect("column in schema")]
        }
        Bin(op, a, b) => {
            let (x, y) = (eval(a, row, schema), eval(b, row, schema));
            match op {
                // NULL in, NULL out. An overflow also gives NULL (so does a result equal to the sentinel itself).
                BinOp::Add | BinOp::Mul if x == NULL || y == NULL => NULL,
                BinOp::Add => x.checked_add(y).unwrap_or(NULL),
                BinOp::Mul => x.checked_mul(y).unwrap_or(NULL),
                _ if x == NULL || y == NULL => 0,
                BinOp::Eq => (x == y) as i64,
                BinOp::Ne => (x != y) as i64,
                BinOp::Lt => (x < y) as i64,
                BinOp::Le => (x <= y) as i64,
                BinOp::Gt => (x > y) as i64,
                BinOp::Ge => (x >= y) as i64,
            }
        }
        And(a, b) => (truthy(eval(a, row, schema)) && truthy(eval(b, row, schema))) as i64,
        Or(a, b) => (truthy(eval(a, row, schema)) || truthy(eval(b, row, schema))) as i64,
        In(x, vs) => {
            let v = eval(x, row, schema);
            (v != NULL && vs.contains(&v)) as i64
        }
    }
}

fn flip(op: BinOp) -> BinOp {
    match op {
        BinOp::Lt => BinOp::Gt,
        BinOp::Le => BinOp::Ge,
        BinOp::Gt => BinOp::Lt,
        BinOp::Ge => BinOp::Le,
        o => o,
    }
}
/// Normalise `col op lit` / `lit op col` into (col, op, lit).
fn col_lit(e: &Expr) -> Option<(ColRef, BinOp, i64)> {
    match e {
        Bin(op, a, b) => match (&**a, &**b) {
            (Col(c), Lit(v)) => Some((*c, *op, *v)),
            (Lit(v), Col(c)) => Some((*c, flip(*op), *v)),
            _ => None,
        },
        _ => None,
    }
}

// ---------- 2. CARDINALITY ESTIMATION ----------
impl Catalog {
    fn stats(&self, c: ColRef) -> &ColStats {
        &self.tables[c.t].stats[c.c]
    }
    fn name(&self, c: ColRef) -> String {
        format!("{}.{}", self.tables[c.t].name, self.tables[c.t].cols[c.c])
    }

    /// P(col = v). With MCV: hot values use their measured frequency, the rest share the leftover mass over the leftover NDV. Without MCV: 1/ndv (uniformity).
    fn eq_sel(&self, c: ColRef, v: i64) -> f64 {
        let s = self.stats(c);
        if v < s.min || v > s.max {
            return 0.0;
        }
        let nn = 1.0 - s.null_frac;
        if !self.use_mcv {
            return nn / s.ndv as f64;
        }
        if let Some(&(_, f)) = s.mcv.iter().find(|m| m.0 == v) {
            return f;
        }
        let hot: f64 = s.mcv.iter().map(|m| m.1).sum();
        (nn - hot).max(0.0) / (s.ndv - s.mcv.len()).max(1) as f64
    }

    /// P(lo <= col <= hi) from the histogram: difference of two CDF lookups.
    fn range_sel(&self, c: ColRef, lo: i64, hi: i64) -> f64 {
        if lo > hi {
            return 0.0;
        }
        if lo == hi {
            return self.eq_sel(c, lo);
        }
        let s = self.stats(c);
        (1.0 - s.null_frac) * (s.frac_le(hi) - s.frac_le(lo - 1)).max(0.0)
    }

    /// Selectivity of one predicate (fraction of rows kept).
    fn sel(&self, e: &Expr) -> f64 {
        let s = match e {
            Lit(v) => (*v != 0) as i64 as f64,
            And(..) => self.sel_conj(&conjuncts(e)),
            Or(a, b) => {
                let (x, y) = (self.sel(a), self.sel(b));
                x + y - x * y
            } // inclusion-exclusion
            In(x, vs) => match &**x {
                Col(c) => {
                    let mut d = vs.clone();
                    d.sort();
                    d.dedup();
                    d.iter().map(|&v| self.eq_sel(*c, v)).sum::<f64>().min(1.0)
                }
                _ => DEFAULT_SEL,
            },
            Bin(op, a, b) => match (col_lit(e), &**a, &**b) {
                (Some((c, BinOp::Eq, v)), ..) => self.eq_sel(c, v),
                (Some((c, BinOp::Ne, v)), ..) => {
                    (1.0 - self.stats(c).null_frac - self.eq_sel(c, v)).max(0.0)
                }
                (Some((c, op, v)), ..) => {
                    let (lo, hi) = bounds(op, v);
                    self.range_sel(c, lo, hi)
                }
                // equi-join (or same-table col = col): 1 / max(ndv_l, ndv_r), assuming containment of value sets
                (None, Col(x), Col(y)) if *op == BinOp::Eq => {
                    1.0 / self.stats(*x).ndv.max(self.stats(*y).ndv) as f64
                }
                _ => DEFAULT_SEL,
            },
            Col(_) => DEFAULT_SEL,
        };
        s.clamp(MIN_SEL, 1.0)
    }

    /// AND: multiply (INDEPENDENCE assumption, wrong for correlated columns), except that several range bounds on ONE column are merged into one range
    /// (otherwise `x > 10 AND x < 20` would be estimated as two independent predicates).
    fn sel_conj(&self, list: &[Expr]) -> f64 {
        let mut ranges: BTreeMap<ColRef, (i64, i64)> = BTreeMap::new();
        let mut rest = 1.0;
        for e in list {
            match col_lit(e) {
                Some((c, op, v)) if matches!(op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge) => {
                    let (lo, hi) = bounds(op, v);
                    let r = ranges.entry(c).or_insert((NULL + 1, i64::MAX));
                    *r = (r.0.max(lo), r.1.min(hi));
                }
                _ => rest *= self.sel(e),
            }
        }
        let rs: f64 = ranges
            .iter()
            .map(|(c, (lo, hi))| self.range_sel(*c, *lo, *hi).clamp(MIN_SEL, 1.0))
            .product();
        (rest * rs).clamp(MIN_SEL, 1.0)
    }
}

/// An inclusive range that holds nothing.
const NO_VALUES: (i64, i64) = (1, 0);

/// Inclusive integer range implied by `col op v` (NULL+1 is the smallest real value).
/// A comparison with NULL keeps nothing, and so does `col > i64::MAX`; both must stay empty when ranges are intersected.
fn bounds(op: BinOp, v: i64) -> (i64, i64) {
    if v == NULL {
        return NO_VALUES;
    }
    match op {
        BinOp::Lt => (NULL + 1, v - 1),
        BinOp::Le => (NULL + 1, v),
        BinOp::Gt => v.checked_add(1).map_or(NO_VALUES, |lo| (lo, i64::MAX)),
        BinOp::Ge => (v, i64::MAX),
        _ => (v, v),
    }
}

/// q-error = max(est/actual, actual/est), both clamped to >= 1 row; 1.0 is perfect, and it is symmetric for over/under-estimation.
fn q_error(est: f64, actual: f64) -> f64 {
    let (e, a) = (est.max(1.0), actual.max(1.0));
    (e / a).max(a / e)
}

// ---------- data generation ----------
struct Db {
    cat: Catalog,
    data: Vec<Vec<Row>>,
    /// Sorted (key, row id) pairs per indexed column: a stand-in for a B-tree. NULLs are not indexed.
    index: HashMap<(usize, usize), Vec<(i64, usize)>>,
}

struct TableSpec {
    name: String,
    cols: Vec<String>,
    indexed: Vec<usize>,
    rows: Vec<Row>,
}

fn spec(name: &str, cols: &[&str], indexed: &[usize], rows: Vec<Row>) -> TableSpec {
    TableSpec {
        name: name.into(),
        cols: cols.iter().map(|s| s.to_string()).collect(),
        indexed: indexed.to_vec(),
        rows,
    }
}

fn make_db(specs: Vec<TableSpec>) -> Db {
    let (mut tables, mut data, mut index) = (Vec::new(), Vec::new(), HashMap::new());
    for (t, s) in specs.into_iter().enumerate() {
        let stats = (0..s.cols.len())
            .map(|c| ColStats::analyze(&s.rows, c))
            .collect();
        for &c in &s.indexed {
            let mut ix: Vec<(i64, usize)> = s
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r[c] != NULL)
                .map(|(i, r)| (r[c], i))
                .collect();
            ix.sort_unstable();
            index.insert((t, c), ix);
        }
        tables.push(TableMeta {
            name: s.name,
            cols: s.cols,
            rows: s.rows.len(),
            stats,
            indexed: s.indexed,
        });
        data.push(s.rows);
    }
    Db {
        cat: Catalog {
            tables,
            use_mcv: true,
        },
        data,
        index,
    }
}

/// The demo schema. users.country/users.city are CORRELATED (city = country*5 + 0..4), users.tier is ZIPF-skewed,
/// users.age has 5% NULLs; orders columns are independent uniform.
fn shop_db(rng: &mut Rng) -> Db {
    let zipf = Zipf::new(50, 1.3);
    let users = (0..5000i64)
        .map(|i| {
            let country = rng.range(0, 39);
            vec![
                i,
                if rng.chance(0.05) {
                    NULL
                } else {
                    rng.range(18, 80)
                },
                country,
                country * 5 + rng.range(0, 4),
                zipf.sample(rng),
            ]
        })
        .collect();
    let orders = (0..20000i64)
        .map(|i| vec![i, rng.range(0, 4999), rng.range(0, 499), rng.range(1, 1000)])
        .collect();
    let products = (0..500i64)
        .map(|i| vec![i, rng.range(0, 19), rng.range(1, 200)])
        .collect();
    let shipments = (0..12000i64)
        .map(|i| vec![i, rng.range(0, 19999), rng.range(0, 14)])
        .collect();
    make_db(vec![
        spec(
            "users",
            &["id", "age", "country", "city", "tier"],
            &[0, 2],
            users,
        ),
        spec(
            "orders",
            &["id", "user_id", "product_id", "amount"],
            &[0, 1, 3],
            orders,
        ),
        spec("products", &["id", "category", "price"], &[0], products),
        spec("shipments", &["id", "order_id", "days"], &[1], shipments),
    ])
}

/// Small random database for differential testing: c0 ~ join key (small domain), c1 in 0..8, c2 in 0..50 with 8% NULLs.
fn rand_db(rng: &mut Rng, nt: usize, max_rows: i64) -> Db {
    let specs = (0..nt)
        .map(|t| {
            let n = rng.range(max_rows / 2, max_rows);
            let k = (n / 2).max(2);
            let rows = (0..n)
                .map(|_| {
                    vec![
                        rng.range(0, k - 1),
                        rng.range(0, 7),
                        if rng.chance(0.08) {
                            NULL
                        } else {
                            rng.range(0, 49)
                        },
                    ]
                })
                .collect();
            let indexed: Vec<usize> = (0..3).filter(|_| rng.chance(0.4)).collect();
            spec(&format!("t{t}"), &["c0", "c1", "c2"], &indexed, rows)
        })
        .collect();
    make_db(specs)
}

// ---------- 3. LOGICAL PLAN + REWRITE RULES ----------
#[derive(Clone, PartialEq, Debug)]
enum Logical {
    Scan(usize),
    Filter(Expr, Box<Logical>),
    Cross(Box<Logical>, Box<Logical>),
    Join(Expr, Box<Logical>, Box<Logical>),
    Project(Vec<ColRef>, Box<Logical>),
}
use Logical as L;

fn bx(l: Logical) -> Box<Logical> {
    Box::new(l)
}

fn lschema(l: &Logical, cat: &Catalog) -> Vec<ColRef> {
    match l {
        L::Scan(t) => (0..cat.tables[*t].cols.len())
            .map(|c| ColRef { t: *t, c })
            .collect(),
        L::Filter(_, x) => lschema(x, cat),
        L::Cross(a, b) | L::Join(_, a, b) => [lschema(a, cat), lschema(b, cat)].concat(),
        L::Project(cols, _) => cols.clone(),
    }
}
/// Tables scanned below a node (a bitmask); used to decide where a predicate may sink.
fn ltables(l: &Logical) -> u32 {
    match l {
        L::Scan(t) => 1 << t,
        L::Filter(_, x) | L::Project(_, x) => ltables(x),
        L::Cross(a, b) | L::Join(_, a, b) => ltables(a) | ltables(b),
    }
}

/// Whether an expression can only be 0 or 1.
fn is_boolean(e: &Expr) -> bool {
    match e {
        Lit(v) => *v == 0 || *v == 1,
        Bin(op, ..) => !matches!(op, BinOp::Add | BinOp::Mul),
        And(..) | Or(..) | In(..) => true,
        Col(_) => false,
    }
}

/// Constant folding: evaluate literal-only subtrees and simplify AND/OR with 0/1 literals.
/// The folded expression has the same VALUE as the original on every row, not just the same truth: it may sit inside a comparison.
fn fold(e: &Expr) -> Expr {
    match e {
        Bin(op, a, b) => {
            let (a, b) = (fold(a), fold(b));
            if let (Lit(_), Lit(_)) = (&a, &b) {
                Lit(eval(&bin(*op, a, b), &[], &[]))
            } else {
                bin(*op, a, b)
            }
        }
        // `1 AND x` is x only if x is already 0 or 1: AND turns 5 into 1
        And(a, b) => match (fold(a), fold(b)) {
            (Lit(0), _) | (_, Lit(0)) => Lit(0),
            (x @ Lit(_), y @ Lit(_)) => Lit(eval(&and2(x, y), &[], &[])),
            (Lit(1), x) | (x, Lit(1)) if is_boolean(&x) => x,
            (x, y) => and2(x, y),
        },
        Or(a, b) => match (fold(a), fold(b)) {
            (Lit(1), _) | (_, Lit(1)) => Lit(1),
            (x @ Lit(_), y @ Lit(_)) => Lit(eval(&or2(x, y), &[], &[])),
            (Lit(0), x) | (x, Lit(0)) if is_boolean(&x) => x,
            (x, y) => or2(x, y),
        },
        In(x, vs) => match fold(x) {
            // through eval, because NULL is in no list, not even in one that holds NULL
            Lit(v) => Lit(eval(&In(Box::new(Lit(v)), vs.clone()), &[], &[])),
            x => In(Box::new(x), vs.clone()),
        },
        other => other.clone(),
    }
}

type Rule = fn(&Logical, &Catalog) -> Option<Logical>;

fn r_fold(l: &Logical, _: &Catalog) -> Option<Logical> {
    match l {
        L::Filter(p, x) => match fold(p) {
            Lit(1) => Some((**x).clone()),
            f if f != *p => Some(L::Filter(f, x.clone())),
            _ => None,
        },
        L::Join(p, a, b) => match fold(p) {
            Lit(1) => Some(L::Cross(a.clone(), b.clone())),
            f if f != *p => Some(L::Join(f, a.clone(), b.clone())),
            _ => None,
        },
        _ => None,
    }
}

/// Filter(a AND b) -> Filter(a, Filter(b)): each conjunct can now move independently.
fn r_split(l: &Logical, _: &Catalog) -> Option<Logical> {
    match l {
        L::Filter(And(a, b), x) => Some(L::Filter(
            (**a).clone(),
            bx(L::Filter((**b).clone(), x.clone())),
        )),
        _ => None,
    }
}

/// Predicate pushdown through joins + join-predicate extraction: a filter over a cross product that mentions both sides becomes a Join predicate.
fn r_pushdown(l: &Logical, _: &Catalog) -> Option<Logical> {
    let L::Filter(p, x) = l else { return None };
    let tp = etables(p);
    let (a, b, q) = match &**x {
        L::Cross(a, b) => (a, b, None),
        L::Join(q, a, b) => (a, b, Some(q)),
        _ => return None,
    };
    let rebuild = |a: Logical, b: Logical, pred: Option<Expr>| match pred {
        Some(e) => L::Join(e, bx(a), bx(b)),
        None => L::Cross(bx(a), bx(b)),
    };
    let keep = q.cloned();
    Some(if tp & !ltables(a) == 0 {
        rebuild(L::Filter(p.clone(), a.clone()), (**b).clone(), keep)
    } else if tp & !ltables(b) == 0 {
        rebuild((**a).clone(), L::Filter(p.clone(), b.clone()), keep)
    } else {
        L::Join(
            match keep {
                Some(q) => and2(q, p.clone()),
                None => p.clone(),
            },
            a.clone(),
            b.clone(),
        )
    })
}

/// Wrap `x` in a Project keeping only the needed columns (None if nothing would be dropped or it is already a Project).
fn narrow(x: &Logical, need: &[ColRef], cat: &Catalog) -> Option<Logical> {
    if matches!(x, L::Project(..)) {
        return None;
    }
    let sch = lschema(x, cat);
    let keep: Vec<ColRef> = sch.iter().filter(|c| need.contains(c)).copied().collect();
    (keep.len() < sch.len()).then(|| L::Project(keep, bx(x.clone())))
}

/// Projection pruning: below a Project, drop columns nobody above needs (output columns + predicate columns) as early as possible.
fn r_prune(l: &Logical, cat: &Catalog) -> Option<Logical> {
    let L::Project(cols, x) = l else { return None };
    let with = |p: Option<&Expr>| {
        let mut n = cols.clone();
        if let Some(p) = p {
            n.extend(col_refs(p));
        }
        n
    };
    let out = |inner: Logical| Some(L::Project(cols.clone(), bx(inner)));
    match &**x {
        L::Join(p, a, b) => {
            let need = with(Some(p));
            let (na, nb) = (narrow(a, &need, cat), narrow(b, &need, cat));
            if na.is_none() && nb.is_none() {
                return None;
            }
            out(L::Join(
                p.clone(),
                bx(na.unwrap_or((**a).clone())),
                bx(nb.unwrap_or((**b).clone())),
            ))
        }
        L::Cross(a, b) => {
            let need = with(None);
            let (na, nb) = (narrow(a, &need, cat), narrow(b, &need, cat));
            if na.is_none() && nb.is_none() {
                return None;
            }
            out(L::Cross(
                bx(na.unwrap_or((**a).clone())),
                bx(nb.unwrap_or((**b).clone())),
            ))
        }
        L::Filter(p, y) => narrow(y, &with(Some(p)), cat)
            .map(|n| L::Filter(p.clone(), bx(n)))
            .and_then(out),
        _ => None,
    }
}

const PHASE1: [(&str, Rule); 3] = [
    ("constant-fold", r_fold),
    ("split-conjunction", r_split),
    ("pushdown+join-extract", r_pushdown),
];
const PHASE2: [(&str, Rule); 1] = [("projection-prune", r_prune)];

fn map_children(l: &Logical, f: &mut impl FnMut(&Logical) -> Logical) -> Logical {
    match l {
        L::Scan(t) => L::Scan(*t),
        L::Filter(p, x) => L::Filter(p.clone(), bx(f(x))),
        L::Cross(a, b) => L::Cross(bx(f(a)), bx(f(b))),
        L::Join(p, a, b) => L::Join(p.clone(), bx(f(a)), bx(f(b))),
        L::Project(c, x) => L::Project(c.clone(), bx(f(x))),
    }
}

/// One bottom-up pass applying at most one rule per node.
fn rewrite_pass(
    l: &Logical,
    rules: &[(&'static str, Rule)],
    cat: &Catalog,
    fired: &mut BTreeMap<&'static str, usize>,
) -> Logical {
    let node = map_children(l, &mut |c| rewrite_pass(c, rules, cat, fired));
    for (name, rule) in rules {
        if let Some(n) = rule(&node, cat) {
            *fired.entry(name).or_default() += 1;
            return n;
        }
    }
    node
}

/// The rule engine: repeat passes until the plan stops changing (a FIXPOINT). Every rule strictly moves work down or removes work, so it terminates.
fn rewrite(
    l: &Logical,
    rules: &[(&'static str, Rule)],
    cat: &Catalog,
    fired: &mut BTreeMap<&'static str, usize>,
) -> Logical {
    let mut cur = l.clone();
    loop {
        let next = rewrite_pass(&cur, rules, cat, fired);
        if next == cur {
            return cur;
        }
        cur = next;
    }
}

// ---------- queries ----------
/// Declarative query: SELECT out FROM tables WHERE pred. The NAIVE plan is Project(Filter(pred, Cross(Cross(t0,t1),t2)...)).
struct Query {
    tables: Vec<usize>,
    pred: Expr,
    out: Vec<ColRef>,
}

fn naive_logical(q: &Query) -> Logical {
    let from = q.tables[1..].iter().fold(L::Scan(q.tables[0]), |acc, &t| {
        L::Cross(bx(acc), bx(L::Scan(t)))
    });
    L::Project(q.out.clone(), bx(L::Filter(q.pred.clone(), bx(from))))
}

// ---------- 4. PHYSICAL OPERATORS + COST MODEL ----------
const PAGE_ROWS: f64 = 100.0;
const SEQ_PAGE: f64 = 1.0;
const RAND_PAGE: f64 = 4.0;
const CPU_TUPLE: f64 = 0.01;
const CPU_OP: f64 = 0.0025;
const HASH_BUILD: f64 = 0.02;
const HASH_PROBE: f64 = 0.01;
const SORT_CMP: f64 = 0.005;
const INDEX_DESCENT: f64 = 3.0;

#[derive(Clone, Debug)]
enum Op {
    SeqScan(usize),
    IndexScan {
        t: usize,
        c: usize,
        lo: i64,
        hi: i64,
    },
    Filter(Expr),
    Project(Vec<ColRef>),
    Nlj(Option<Expr>),
    /// kids[0] = probe side, kids[1] = BUILD side (hashed).
    HashJoin {
        lk: ColRef,
        rk: ColRef,
        resid: Option<Expr>,
    },
    MergeJoin {
        lk: ColRef,
        rk: ColRef,
        resid: Option<Expr>,
    },
    Sort(ColRef),
}

#[derive(Clone, Debug)]
struct Plan {
    op: Op,
    kids: Vec<Plan>,
    rows: f64,
    cost: f64,
    schema: Vec<ColRef>,
    /// Interesting order: the column the output is sorted by, if any.
    order: Option<ColRef>,
}

fn pages(cat: &Catalog, t: usize) -> f64 {
    (cat.tables[t].rows as f64 / PAGE_ROWS).ceil().max(1.0)
}
fn full_schema(cat: &Catalog, t: usize) -> Vec<ColRef> {
    lschema(&L::Scan(t), cat)
}

fn seq_scan(cat: &Catalog, t: usize) -> Plan {
    let n = cat.tables[t].rows as f64;
    Plan {
        op: Op::SeqScan(t),
        kids: vec![],
        rows: n,
        cost: pages(cat, t) * SEQ_PAGE + n * CPU_TUPLE,
        schema: full_schema(cat, t),
        order: None,
    }
}

/// IndexScan over key range [lo, hi]: descend the tree, then fetch one random page per matching row (capped at the table's page count).
/// Cheap for few rows, worse than a sequential scan once matches approach ~pages/RAND_PAGE rows (the crossover shown in the demo).
fn index_scan(cat: &Catalog, t: usize, c: usize, lo: i64, hi: i64) -> Plan {
    let m = cat.tables[t].rows as f64 * cat.range_sel(ColRef { t, c }, lo, hi);
    let cost = (INDEX_DESCENT + m.min(pages(cat, t))) * RAND_PAGE + m * CPU_TUPLE;
    Plan {
        op: Op::IndexScan { t, c, lo, hi },
        kids: vec![],
        rows: m,
        cost,
        schema: full_schema(cat, t),
        order: Some(ColRef { t, c }),
    }
}

fn filter_plan(cat: &Catalog, input: Plan, conj: &[Expr]) -> Plan {
    if conj.is_empty() {
        return input;
    }
    let rows = input.rows * cat.sel_conj(conj);
    let cost = input.cost + input.rows * CPU_OP * conj.len() as f64;
    Plan {
        op: Op::Filter(and_all(conj)),
        schema: input.schema.clone(),
        order: input.order,
        rows,
        cost,
        kids: vec![input],
    }
}

fn project_plan(input: Plan, cols: &[ColRef]) -> Plan {
    let order = input.order.filter(|o| cols.contains(o));
    Plan {
        op: Op::Project(cols.to_vec()),
        rows: input.rows,
        cost: input.cost + input.rows * CPU_OP,
        schema: cols.to_vec(),
        order,
        kids: vec![input],
    }
}

fn sort_plan(input: Plan, key: ColRef) -> Plan {
    let n = input.rows.max(2.0);
    Plan {
        op: Op::Sort(key),
        rows: input.rows,
        cost: input.cost + n * n.log2() * SORT_CMP,
        schema: input.schema.clone(),
        order: Some(key),
        kids: vec![input],
    }
}

fn nlj_plan(l: Plan, r: Plan, pred: Option<Expr>, rows: f64) -> Plan {
    let cost = l.cost + r.cost + l.rows * r.rows * CPU_OP + rows * CPU_TUPLE;
    Plan {
        schema: [l.schema.clone(), r.schema.clone()].concat(),
        order: l.order,
        op: Op::Nlj(pred),
        rows,
        cost,
        kids: vec![l, r],
    }
}

/// All physical ways to join `l` (outer/probe) with `r` (inner/build) given the join predicates that span them. `rows` is the estimated output.
fn join_candidates(l: &Plan, r: &Plan, preds: &[Expr], rows: f64) -> Vec<Plan> {
    let schema = [l.schema.clone(), r.schema.clone()].concat();
    let equi = preds.iter().enumerate().find_map(|(i, p)| match p {
        Bin(BinOp::Eq, a, b) => match (&**a, &**b) {
            (Col(x), Col(y)) if l.schema.contains(x) && r.schema.contains(y) => Some((i, *x, *y)),
            (Col(x), Col(y)) if l.schema.contains(y) && r.schema.contains(x) => Some((i, *y, *x)),
            _ => None,
        },
        _ => None,
    });
    let all = (!preds.is_empty()).then(|| and_all(preds));
    let mut out = vec![nlj_plan(l.clone(), r.clone(), all, rows)];
    if let Some((i, lk, rk)) = equi {
        let rest: Vec<Expr> = preds
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, p)| p.clone())
            .collect();
        let resid = (!rest.is_empty()).then(|| and_all(&rest));
        let extra = if resid.is_some() { rows * CPU_OP } else { 0.0 };
        let hash_cost =
            l.cost + r.cost + r.rows * HASH_BUILD + l.rows * HASH_PROBE + rows * CPU_TUPLE + extra;
        out.push(Plan {
            op: Op::HashJoin {
                lk,
                rk,
                resid: resid.clone(),
            },
            kids: vec![l.clone(), r.clone()],
            rows,
            cost: hash_cost,
            schema: schema.clone(),
            order: None,
        });
        // INTERESTING ORDER: add a Sort only when the input is not already sorted on its join key.
        let side = |p: &Plan, k: ColRef| {
            if p.order == Some(k) {
                p.clone()
            } else {
                sort_plan(p.clone(), k)
            }
        };
        let (ls, rs) = (side(l, lk), side(r, rk));
        let cost = ls.cost + rs.cost + (ls.rows + rs.rows) * CPU_OP + rows * CPU_TUPLE + extra;
        out.push(Plan {
            op: Op::MergeJoin { lk, rk, resid },
            kids: vec![ls, rs],
            rows,
            cost,
            schema,
            order: Some(lk),
        });
    }
    out
}

/// The cheapest plan per output order. A plan with order X is only discarded by a cheaper plan with the SAME order,
/// because a pricier ordered plan may let a later merge join skip its sort.
#[derive(Clone, Default)]
struct PlanSet(BTreeMap<Option<ColRef>, Plan>);
impl PlanSet {
    fn of(plans: Vec<Plan>) -> Self {
        let mut s = PlanSet::default();
        plans.into_iter().for_each(|p| s.insert(p));
        s
    }
    fn insert(&mut self, p: Plan) {
        match self.0.get(&p.order) {
            Some(old) if old.cost <= p.cost => {}
            _ => {
                self.0.insert(p.order, p);
            }
        }
    }
    fn cheapest(&self) -> &Plan {
        self.0
            .values()
            .min_by(|a, b| a.cost.total_cmp(&b.cost))
            .expect("non-empty plan set")
    }
}

/// Sargable predicate analysis: merge `col op lit` conjuncts on one column into [lo, hi], remembering which conjuncts the index consumes.
fn sargable(conj: &[Expr], c: ColRef) -> Option<(i64, i64, Vec<usize>)> {
    let (mut lo, mut hi, mut used) = (NULL + 1, i64::MAX, vec![]);
    for (i, e) in conj.iter().enumerate() {
        if let Some((_, op, v)) = col_lit(e)
            .filter(|(cc, op, _)| *cc == c && !matches!(op, BinOp::Ne | BinOp::Add | BinOp::Mul))
        {
            let (l, h) = bounds(op, v);
            (lo, hi) = (lo.max(l), hi.min(h));
            used.push(i);
        }
    }
    (!used.is_empty()).then_some((lo, hi, used))
}

/// Access-path selection for one table: SeqScan+Filter versus an IndexScan per usable index (+ residual Filter).
fn leaf_plans(cat: &Catalog, t: usize, conj: &[Expr]) -> Vec<Plan> {
    let total = cat.tables[t].rows as f64
        * if conj.is_empty() {
            1.0
        } else {
            cat.sel_conj(conj)
        };
    let fix = |mut p: Plan| {
        p.rows = total;
        p
    };
    let mut out = vec![fix(filter_plan(cat, seq_scan(cat, t), conj))];
    for &c in &cat.tables[t].indexed {
        if let Some((lo, hi, used)) = sargable(conj, ColRef { t, c }) {
            let rest: Vec<Expr> = conj
                .iter()
                .enumerate()
                .filter(|(i, _)| !used.contains(i))
                .map(|(_, e)| e.clone())
                .collect();
            out.push(fix(filter_plan(cat, index_scan(cat, t, c, lo, hi), &rest)));
        }
    }
    out
}

fn cheapest_of(plans: Vec<Plan>) -> Plan {
    plans
        .into_iter()
        .min_by(|a, b| a.cost.total_cmp(&b.cost))
        .unwrap()
}

/// Unoptimised logical -> physical (SeqScan, NestedLoop, Filter on top). Used as the differential oracle and to cost rewrites.
fn naive_physical(l: &Logical, cat: &Catalog) -> Plan {
    match l {
        L::Scan(t) => seq_scan(cat, *t),
        L::Filter(p, x) => filter_plan(cat, naive_physical(x, cat), &conjuncts(p)),
        L::Cross(a, b) => {
            let (a, b) = (naive_physical(a, cat), naive_physical(b, cat));
            let rows = a.rows * b.rows;
            nlj_plan(a, b, None, rows)
        }
        L::Join(p, a, b) => {
            let (a, b) = (naive_physical(a, cat), naive_physical(b, cat));
            let rows = a.rows * b.rows * cat.sel(p);
            nlj_plan(a, b, Some(p.clone()), rows)
        }
        L::Project(c, x) => project_plan(naive_physical(x, cat), c),
    }
}

// ---------- 5. JOIN ORDERING ----------
struct Edge {
    e: Expr,
    mask: u32,
    sel: f64,
}
struct Graph {
    leaves: Vec<(usize, Vec<Expr>)>,
    edges: Vec<Edge>,
    out: Vec<ColRef>,
}

fn base_table(l: &Logical) -> Option<usize> {
    match l {
        L::Scan(t) => Some(*t),
        L::Filter(_, x) | L::Project(_, x) => base_table(x),
        _ => None,
    }
}

fn collect(l: &Logical, leaves: &mut Vec<(usize, Vec<Expr>)>, preds: &mut Vec<Expr>) {
    match l {
        L::Scan(t) => leaves.push((*t, vec![])),
        L::Project(_, x) => collect(x, leaves, preds),
        L::Filter(p, x) if base_table(x).is_some() => {
            collect(x, leaves, preds);
            leaves.last_mut().unwrap().1.extend(conjuncts(p));
        }
        L::Filter(p, x) => {
            preds.extend(conjuncts(p));
            collect(x, leaves, preds);
        }
        L::Cross(a, b) => {
            collect(a, leaves, preds);
            collect(b, leaves, preds);
        }
        L::Join(p, a, b) => {
            preds.extend(conjuncts(p));
            collect(a, leaves, preds);
            collect(b, leaves, preds);
        }
    }
}

/// Flatten a rewritten plan into a join graph: nodes = tables with their local filters, hyperedges = multi-table predicates with their selectivity.
fn build_graph(l: &Logical, cat: &Catalog) -> Graph {
    let (out, body) = match l {
        L::Project(c, x) => (c.clone(), &**x),
        _ => (lschema(l, cat), l),
    };
    let (mut leaves, mut preds) = (Vec::new(), Vec::new());
    collect(body, &mut leaves, &mut preds);
    let leaf_of = |t: usize| leaves.iter().position(|(lt, _)| *lt == t).unwrap();
    let mut edges = Vec::new();
    let mut local: Vec<(usize, Expr)> = Vec::new();
    for p in preds {
        let tm = etables(&p);
        let mask = (0..32)
            .filter(|t| tm >> t & 1 == 1)
            .fold(0u32, |m, t| m | 1 << leaf_of(t));
        if mask.count_ones() <= 1 {
            local.push((
                if mask == 0 {
                    0
                } else {
                    mask.trailing_zeros() as usize
                },
                p,
            ));
        } else {
            edges.push(Edge {
                sel: cat.sel(&p),
                e: p,
                mask,
            });
        }
    }
    for (i, p) in local {
        leaves[i].1.push(p);
    }
    Graph { leaves, edges, out }
}

struct Ctx<'a> {
    cat: &'a Catalog,
    g: &'a Graph,
    leaf: Vec<Vec<Plan>>,
    card: Vec<f64>,
}

impl<'a> Ctx<'a> {
    fn new(cat: &'a Catalog, g: &'a Graph) -> Self {
        let leaf: Vec<Vec<Plan>> = g
            .leaves
            .iter()
            .map(|(t, c)| leaf_plans(cat, *t, c))
            .collect();
        // card(S) = product of leaf cardinalities * product of selectivities of edges fully inside S. Depends only on the SET, so every plan
        // for S (any order, any operator) has the same output size: costs are comparable.
        let card = (0..1u32 << leaf.len())
            .map(|m| {
                let rows: f64 = (0..leaf.len())
                    .filter(|i| m >> i & 1 == 1)
                    .map(|i| leaf[i][0].rows)
                    .product();
                g.edges
                    .iter()
                    .filter(|e| e.mask & !m == 0)
                    .fold(rows, |r, e| r * e.sel)
            })
            .collect();
        Ctx { cat, g, leaf, card }
    }

    /// Join every plan in `a` (outer, set s1) with every plan in `b` (inner, set s2). Skipped when no predicate connects them (a CROSS PRODUCT) unless allowed.
    fn join_sets(
        &self,
        s1: u32,
        a: &PlanSet,
        s2: u32,
        b: &PlanSet,
        cross: bool,
        acc: &mut PlanSet,
    ) {
        let mask = s1 | s2;
        let preds: Vec<Expr> = self
            .g
            .edges
            .iter()
            .filter(|e| e.mask & !mask == 0 && e.mask & !s1 != 0 && e.mask & !s2 != 0)
            .map(|e| e.e.clone())
            .collect();
        if preds.is_empty() && !cross {
            return;
        }
        for pa in a.0.values() {
            for pb in b.0.values() {
                join_candidates(pa, pb, &preds, self.card[mask as usize])
                    .into_iter()
                    .for_each(|p| acc.insert(p));
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    LeftDeep,
    Bushy,
}

fn splits(mask: u32, shape: Shape) -> Vec<(u32, u32)> {
    match shape {
        // left-deep: composite outer + ONE base table inner
        Shape::LeftDeep => (0..32)
            .filter(|i| mask >> i & 1 == 1)
            .map(|i| (mask ^ (1 << i), 1 << i))
            .collect(),
        // bushy: every proper non-empty subset as the outer side (both orientations come for free)
        Shape::Bushy => {
            let mut v = Vec::new();
            let mut sub = (mask - 1) & mask;
            while sub > 0 {
                v.push((sub, mask ^ sub));
                sub = (sub - 1) & mask;
            }
            v
        }
    }
}

/// Selinger dynamic programming over subsets: best[S] = cheapest plan(s) joining exactly the tables in S, built from best[S1] x best[S2].
/// Optimal substructure holds because card(S) is fixed, once interesting orders are kept as separate entries.
fn dp(ctx: &Ctx, shape: Shape, cross: bool) -> Option<Plan> {
    let n = ctx.leaf.len();
    let mut sets: Vec<Option<PlanSet>> = vec![None; 1 << n];
    for i in 0..n {
        sets[1 << i] = Some(PlanSet::of(ctx.leaf[i].clone()));
    }
    for mask in 1..(1u32 << n) {
        if mask.count_ones() < 2 {
            continue;
        }
        let mut acc = PlanSet::default();
        for (s1, s2) in splits(mask, shape) {
            if let (Some(a), Some(b)) = (&sets[s1 as usize], &sets[s2 as usize]) {
                ctx.join_sets(s1, a, s2, b, cross, &mut acc);
            }
        }
        if !acc.0.is_empty() {
            sets[mask as usize] = Some(acc);
        }
    }
    sets[(1usize << n) - 1]
        .as_ref()
        .map(|s| s.cheapest().clone())
}

/// GOO (Greedy Operator Ordering): repeatedly join the pair of components whose result is SMALLEST. O(n^3), no optimality guarantee.
fn greedy(ctx: &Ctx) -> Plan {
    let mut comps: Vec<(u32, Plan)> = (0..ctx.leaf.len())
        .map(|i| (1u32 << i, cheapest_of(ctx.leaf[i].clone())))
        .collect();
    while comps.len() > 1 {
        let connected = |a: u32, b: u32| {
            ctx.g
                .edges
                .iter()
                .any(|e| e.mask & !(a | b) == 0 && e.mask & !a != 0 && e.mask & !b != 0)
        };
        let mut best: Option<(f64, usize, usize)> = None;
        for only_connected in [true, false] {
            for i in 0..comps.len() {
                for j in i + 1..comps.len() {
                    if only_connected && !connected(comps[i].0, comps[j].0) {
                        continue;
                    }
                    let c = ctx.card[(comps[i].0 | comps[j].0) as usize];
                    if best.is_none_or(|b| c < b.0) {
                        best = Some((c, i, j));
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        let (_, i, j) = best.unwrap();
        let (mb, pb) = comps.remove(j);
        let (ma, pa) = comps.remove(i);
        let mut acc = PlanSet::default();
        ctx.join_sets(
            ma,
            &PlanSet::of(vec![pa.clone()]),
            mb,
            &PlanSet::of(vec![pb.clone()]),
            true,
            &mut acc,
        );
        ctx.join_sets(
            mb,
            &PlanSet::of(vec![pb]),
            ma,
            &PlanSet::of(vec![pa]),
            true,
            &mut acc,
        );
        comps.push((ma | mb, acc.cheapest().clone()));
    }
    comps.pop().unwrap().1
}

/// Brute force oracle: enumerate EVERY ordered binary join tree (n! * Catalan(n-1)), evaluate each with the same operator choices, take the minimum.
enum Tree {
    Leaf(usize),
    Node(Box<Tree>, Box<Tree>),
}

fn all_trees(mask: u32, memo: &mut HashMap<u32, Vec<std::rc::Rc<Tree>>>) -> Vec<std::rc::Rc<Tree>> {
    if let Some(v) = memo.get(&mask) {
        return v.clone();
    }
    let v = if mask.count_ones() == 1 {
        vec![std::rc::Rc::new(Tree::Leaf(mask.trailing_zeros() as usize))]
    } else {
        let mut v = Vec::new();
        for (s1, s2) in splits(mask, Shape::Bushy) {
            for a in all_trees(s1, memo) {
                for b in all_trees(s2, memo) {
                    v.push(std::rc::Rc::new(Tree::Node(
                        Box::new(clone_tree(&a)),
                        Box::new(clone_tree(&b)),
                    )));
                }
            }
        }
        v
    };
    memo.insert(mask, v.clone());
    v
}
fn clone_tree(t: &Tree) -> Tree {
    match t {
        Tree::Leaf(i) => Tree::Leaf(*i),
        Tree::Node(a, b) => Tree::Node(Box::new(clone_tree(a)), Box::new(clone_tree(b))),
    }
}
fn tree_mask(t: &Tree) -> u32 {
    match t {
        Tree::Leaf(i) => 1 << i,
        Tree::Node(a, b) => tree_mask(a) | tree_mask(b),
    }
}

fn eval_tree(ctx: &Ctx, t: &Tree, cross: bool) -> Option<PlanSet> {
    match t {
        Tree::Leaf(i) => Some(PlanSet::of(ctx.leaf[*i].clone())),
        Tree::Node(a, b) => {
            let (pa, pb) = (eval_tree(ctx, a, cross)?, eval_tree(ctx, b, cross)?);
            let mut acc = PlanSet::default();
            ctx.join_sets(tree_mask(a), &pa, tree_mask(b), &pb, cross, &mut acc);
            (!acc.0.is_empty()).then_some(acc)
        }
    }
}

fn brute_force_cost(ctx: &Ctx, cross: bool) -> Option<f64> {
    let full = (1u32 << ctx.leaf.len()) - 1;
    all_trees(full, &mut HashMap::new())
        .iter()
        .filter_map(|t| eval_tree(ctx, t, cross))
        .map(|s| s.cheapest().cost)
        .min_by(f64::total_cmp)
}

/// Table ids are bits of a u32, and the DP keeps a table of 2^n subsets whose split enumeration grows as 3^n.
const MAX_TABLE_ID: usize = 31;
const MAX_QUERY_TABLES: usize = 12;

/// What this planner can represent. A table may appear once only: columns are named by table id, so a self-join cannot be written.
fn check_query(q: &Query, cat: &Catalog) -> Result<(), String> {
    if q.tables.is_empty() || q.tables.len() > MAX_QUERY_TABLES {
        return Err(format!(
            "a query needs 1..={MAX_QUERY_TABLES} tables, got {}",
            q.tables.len()
        ));
    }
    for (i, &t) in q.tables.iter().enumerate() {
        if t > MAX_TABLE_ID || t >= cat.tables.len() {
            return Err(format!("unknown or unsupported table id {t}"));
        }
        if q.tables[..i].contains(&t) {
            return Err(format!(
                "table {t} appears twice (self-joins are not supported)"
            ));
        }
    }
    for c in col_refs(&q.pred).iter().chain(&q.out) {
        if !q.tables.contains(&c.t) || c.c >= cat.tables[c.t].cols.len() {
            return Err(format!("unknown column {}.{}", c.t, c.c));
        }
    }
    Ok(())
}

/// Full pipeline: rewrite -> join graph -> DP (bushy) -> Project on top.
fn optimize(q: &Query, cat: &Catalog) -> Result<Plan, String> {
    check_query(q, cat)?;
    let rewritten = rewrite(&naive_logical(q), &PHASE1, cat, &mut BTreeMap::new());
    let g = build_graph(&rewritten, cat);
    let ctx = Ctx::new(cat, &g);
    let best = dp(&ctx, Shape::Bushy, false)
        .or_else(|| dp(&ctx, Shape::Bushy, true))
        .expect("a plan exists");
    Ok(project_plan(best, &g.out))
}

// ---------- 6. EXPLAIN ----------
fn show(e: &Expr, cat: &Catalog) -> String {
    // an operator inside an operator keeps its brackets: (a + 1) * 2 is not a + 1 * 2
    let operand = |x: &Expr| match x {
        Bin(..) => format!("({})", show(x, cat)),
        _ => show(x, cat),
    };
    match e {
        Lit(v) if *v == NULL => "NULL".into(),
        Lit(v) => v.to_string(),
        Col(c) => cat.name(*c),
        Bin(op, a, b) => format!(
            "{} {} {}",
            operand(a),
            ["=", "<>", "<", "<=", ">", ">=", "+", "*"][*op as usize],
            operand(b)
        ),
        And(a, b) => format!("({} AND {})", show(a, cat), show(b, cat)),
        Or(a, b) => format!("({} OR {})", show(a, cat), show(b, cat)),
        In(x, vs) => format!("{} IN {:?}", show(x, cat), vs),
    }
}

fn describe(p: &Plan, cat: &Catalog) -> String {
    let opt = |e: &Option<Expr>| {
        e.as_ref()
            .map(|e| format!(" +[{}]", show(e, cat)))
            .unwrap_or_default()
    };
    match &p.op {
        Op::SeqScan(t) => format!("SeqScan {}", cat.tables[*t].name),
        Op::IndexScan { t, c, lo, hi } => format!(
            "IndexScan {}.{} [{}..{}]",
            cat.tables[*t].name,
            cat.tables[*t].cols[*c],
            if *lo <= NULL + 1 {
                "-inf".into()
            } else {
                lo.to_string()
            },
            if *hi == i64::MAX {
                "inf".into()
            } else {
                hi.to_string()
            }
        ),
        Op::Filter(e) => format!("Filter {}", show(e, cat)),
        Op::Project(c) => format!(
            "Project {}",
            c.iter().map(|c| cat.name(*c)).collect::<Vec<_>>().join(",")
        ),
        Op::Nlj(e) => format!("NestedLoop{}", opt(e)),
        Op::HashJoin { lk, rk, resid } => format!(
            "HashJoin probe {} = build {}{}",
            cat.name(*lk),
            cat.name(*rk),
            opt(resid)
        ),
        Op::MergeJoin { lk, rk, resid } => format!(
            "MergeJoin {} = {}{}",
            cat.name(*lk),
            cat.name(*rk),
            opt(resid)
        ),
        Op::Sort(k) => format!("Sort {}", cat.name(*k)),
    }
}

fn explain(p: &Plan, cat: &Catalog, depth: usize, out: &mut String) {
    out.push_str(&format!(
        "{}-> {}  (rows={:.0} cost={:.1})\n",
        "   ".repeat(depth),
        describe(p, cat),
        p.rows,
        p.cost
    ));
    p.kids.iter().for_each(|k| explain(k, cat, depth + 1, out));
}
fn explained(p: &Plan, cat: &Catalog) -> String {
    let mut s = String::new();
    explain(p, cat, 1, &mut s);
    s
}

fn show_logical(l: &Logical, cat: &Catalog, depth: usize, out: &mut String) {
    let pad = "   ".repeat(depth);
    match l {
        L::Scan(t) => out.push_str(&format!("{pad}Scan {}\n", cat.tables[*t].name)),
        L::Filter(p, x) => {
            out.push_str(&format!("{pad}Filter {}\n", show(p, cat)));
            show_logical(x, cat, depth + 1, out);
        }
        L::Cross(a, b) => {
            out.push_str(&format!("{pad}Cross\n"));
            show_logical(a, cat, depth + 1, out);
            show_logical(b, cat, depth + 1, out);
        }
        L::Join(p, a, b) => {
            out.push_str(&format!("{pad}Join {}\n", show(p, cat)));
            show_logical(a, cat, depth + 1, out);
            show_logical(b, cat, depth + 1, out);
        }
        L::Project(c, x) => {
            out.push_str(&format!(
                "{pad}Project {}\n",
                c.iter().map(|c| cat.name(*c)).collect::<Vec<_>>().join(",")
            ));
            show_logical(x, cat, depth + 1, out);
        }
    }
}

// ---------- 7. EXECUTION: Volcano iterators ----------
trait RowIter {
    fn next(&mut self) -> Option<Row>;
}
type BoxIt<'a> = Box<dyn RowIter + 'a>;

struct SeqIt<'a> {
    rows: &'a [Row],
    i: usize,
}
impl RowIter for SeqIt<'_> {
    fn next(&mut self) -> Option<Row> {
        let r = self.rows.get(self.i)?.clone();
        self.i += 1;
        Some(r)
    }
}
struct IdxIt<'a> {
    rows: &'a [Row],
    ids: &'a [(i64, usize)],
    i: usize,
}
impl RowIter for IdxIt<'_> {
    fn next(&mut self) -> Option<Row> {
        let (_, id) = self.ids.get(self.i)?;
        self.i += 1;
        Some(self.rows[*id].clone())
    }
}
struct FilterIt<'a> {
    src: BoxIt<'a>,
    pred: Expr,
    schema: Vec<ColRef>,
}
impl RowIter for FilterIt<'_> {
    fn next(&mut self) -> Option<Row> {
        loop {
            let r = self.src.next()?;
            if truthy(eval(&self.pred, &r, &self.schema)) {
                return Some(r);
            }
        }
    }
}
struct ProjIt<'a> {
    src: BoxIt<'a>,
    pos: Vec<usize>,
}
impl RowIter for ProjIt<'_> {
    fn next(&mut self) -> Option<Row> {
        let r = self.src.next()?;
        Some(self.pos.iter().map(|&p| r[p]).collect())
    }
}
struct NljIt<'a> {
    left: BoxIt<'a>,
    right: Option<BoxIt<'a>>,
    inner: Vec<Row>,
    cur: Option<Row>,
    i: usize,
    pred: Option<Expr>,
    schema: Vec<ColRef>,
}
impl RowIter for NljIt<'_> {
    fn next(&mut self) -> Option<Row> {
        if let Some(mut r) = self.right.take() {
            while let Some(x) = r.next() {
                self.inner.push(x);
            }
        } // materialise inner once
        loop {
            if self.cur.is_none() {
                self.cur = Some(self.left.next()?);
                self.i = 0;
            }
            let l = self.cur.as_ref().unwrap();
            while self.i < self.inner.len() {
                let mut row = l.clone();
                row.extend_from_slice(&self.inner[self.i]);
                self.i += 1;
                if self
                    .pred
                    .as_ref()
                    .is_none_or(|p| truthy(eval(p, &row, &self.schema)))
                {
                    return Some(row);
                }
            }
            self.cur = None;
        }
    }
}
struct HashIt<'a> {
    probe: BoxIt<'a>,
    build: Option<BoxIt<'a>>,
    map: HashMap<i64, Vec<Row>>,
    lpos: usize,
    rpos: usize,
    queue: VecDeque<Row>,
    resid: Option<Expr>,
    schema: Vec<ColRef>,
}
impl RowIter for HashIt<'_> {
    fn next(&mut self) -> Option<Row> {
        if let Some(mut b) = self.build.take() {
            while let Some(r) = b.next() {
                if r[self.rpos] != NULL {
                    self.map.entry(r[self.rpos]).or_default().push(r);
                }
            }
        }
        loop {
            if let Some(r) = self.queue.pop_front() {
                return Some(r);
            }
            let p = self.probe.next()?;
            if let Some(ms) = self.map.get(&p[self.lpos]).filter(|_| p[self.lpos] != NULL) {
                for m in ms {
                    let mut row = p.clone();
                    row.extend_from_slice(m);
                    if self
                        .resid
                        .as_ref()
                        .is_none_or(|e| truthy(eval(e, &row, &self.schema)))
                    {
                        self.queue.push_back(row);
                    }
                }
            }
        }
    }
}
/// Blocking operators (Sort, MergeJoin here) do their work on the first `next()` and then stream the buffer.
struct Blocking<'a> {
    job: Option<Box<dyn FnOnce() -> Vec<Row> + 'a>>,
    out: std::vec::IntoIter<Row>,
}
impl RowIter for Blocking<'_> {
    fn next(&mut self) -> Option<Row> {
        if let Some(j) = self.job.take() {
            self.out = j().into_iter();
        }
        self.out.next()
    }
}
fn blocking<'a>(job: impl FnOnce() -> Vec<Row> + 'a) -> BoxIt<'a> {
    Box::new(Blocking {
        job: Some(Box::new(job)),
        out: Vec::new().into_iter(),
    })
}
fn drain(mut it: BoxIt<'_>) -> Vec<Row> {
    let mut v = Vec::new();
    while let Some(r) = it.next() {
        v.push(r);
    }
    v
}
fn pos(schema: &[ColRef], c: ColRef) -> usize {
    schema.iter().position(|s| *s == c).expect("key in schema")
}

fn merge_join(
    l: Vec<Row>,
    r: Vec<Row>,
    (lp, rp): (usize, usize),
    resid: &Option<Expr>,
    schema: &[ColRef],
) -> Vec<Row> {
    let (mut out, mut i, mut j) = (Vec::new(), 0, 0);
    while i < l.len() && j < r.len() {
        let (a, b) = (l[i][lp], r[j][rp]);
        if a == NULL {
            i += 1;
            continue;
        }
        if b == NULL {
            j += 1;
            continue;
        }
        match a.cmp(&b) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                let ie = i + l[i..].iter().take_while(|x| x[lp] == a).count();
                let je = j + r[j..].iter().take_while(|x| x[rp] == a).count();
                for x in &l[i..ie] {
                    for y in &r[j..je] {
                        let row: Row = x.iter().chain(y.iter()).copied().collect();
                        if resid.as_ref().is_none_or(|e| truthy(eval(e, &row, schema))) {
                            out.push(row);
                        }
                    }
                }
                (i, j) = (ie, je);
            }
        }
    }
    out
}

fn build_iter<'a>(p: &'a Plan, db: &'a Db) -> BoxIt<'a> {
    let kid = |i: usize| build_iter(&p.kids[i], db);
    match &p.op {
        Op::SeqScan(t) => Box::new(SeqIt {
            rows: &db.data[*t],
            i: 0,
        }),
        Op::IndexScan { t, c, lo, hi } => {
            let ix = &db.index[&(*t, *c)];
            let (a, b) = (
                ix.partition_point(|(k, _)| k < lo),
                ix.partition_point(|(k, _)| k <= hi),
            );
            Box::new(IdxIt {
                rows: &db.data[*t],
                ids: &ix[a..b.max(a)],
                i: 0,
            })
        }
        Op::Filter(e) => Box::new(FilterIt {
            src: kid(0),
            pred: e.clone(),
            schema: p.schema.clone(),
        }),
        Op::Project(cols) => Box::new(ProjIt {
            src: kid(0),
            pos: cols.iter().map(|c| pos(&p.kids[0].schema, *c)).collect(),
        }),
        Op::Nlj(pred) => Box::new(NljIt {
            left: kid(0),
            right: Some(kid(1)),
            inner: vec![],
            cur: None,
            i: 0,
            pred: pred.clone(),
            schema: p.schema.clone(),
        }),
        Op::HashJoin { lk, rk, resid } => Box::new(HashIt {
            probe: kid(0),
            build: Some(kid(1)),
            map: HashMap::new(),
            lpos: pos(&p.kids[0].schema, *lk),
            rpos: pos(&p.kids[1].schema, *rk),
            queue: VecDeque::new(),
            resid: resid.clone(),
            schema: p.schema.clone(),
        }),
        Op::MergeJoin { lk, rk, resid } => {
            let (l, r) = (kid(0), kid(1));
            let keys = (pos(&p.kids[0].schema, *lk), pos(&p.kids[1].schema, *rk));
            let (resid, schema) = (resid.clone(), p.schema.clone());
            blocking(move || merge_join(drain(l), drain(r), keys, &resid, &schema))
        }
        Op::Sort(k) => {
            let (src, at) = (kid(0), pos(&p.schema, *k));
            blocking(move || {
                let mut v = drain(src);
                v.sort_by_key(|r| r[at]);
                v
            })
        }
    }
}

fn run(p: &Plan, db: &Db) -> Vec<Row> {
    drain(build_iter(p, db))
}
fn sorted(mut v: Vec<Row>) -> Vec<Row> {
    v.sort();
    v
}

// ---------- random queries for differential testing ----------
fn rand_query(rng: &mut Rng, db: &Db, n: usize) -> Query {
    let mut ts: Vec<usize> = (0..db.cat.tables.len()).collect();
    for i in (1..ts.len()).rev() {
        ts.swap(i, rng.below(i as u64 + 1) as usize);
    }
    ts.truncate(n);
    let mut conj = Vec::new();
    for i in 1..n {
        // a random spanning tree of equi-join edges keeps the query connected
        let j = rng.below(i as u64) as usize;
        conj.push(bin(
            BinOp::Eq,
            col(ts[i], rng.below(2) as usize),
            col(ts[j], rng.below(2) as usize),
        ));
    }
    for _ in 0..rng.below(4) {
        let (t, c) = (ts[rng.below(n as u64) as usize], rng.below(3) as usize);
        let v = rng.range(0, [5, 7, 49][c]);
        let ops = [BinOp::Lt, BinOp::Le, BinOp::Gt, BinOp::Eq, BinOp::Ne];
        conj.push(match rng.below(6) {
            0 | 1 => bin(ops[rng.below(5) as usize], col(t, c), Lit(v)),
            2 => In(Box::new(col(t, c)), vec![v, v + 1, rng.range(0, 7)]),
            3 => or2(
                bin(BinOp::Lt, col(t, c), Lit(v)),
                bin(BinOp::Eq, col(t, c), Lit(rng.range(0, 7))),
            ),
            4 => bin(BinOp::Lt, bin(BinOp::Add, Lit(2), Lit(v % 3)), col(t, c)), // folds to `k < col`
            _ => [
                bin(BinOp::Eq, Lit(1), Lit(1)),
                bin(BinOp::Gt, Lit(1), Lit(2)),
                and2(
                    bin(BinOp::Eq, Lit(2), Lit(2)),
                    bin(BinOp::Ge, col(t, c), Lit(0)),
                ),
            ][rng.below(3) as usize]
                .clone(),
        });
    }
    for i in (1..conj.len()).rev() {
        conj.swap(i, rng.below(i as u64 + 1) as usize);
    }
    let all: Vec<ColRef> = ts
        .iter()
        .flat_map(|&t| (0..3).map(move |c| ColRef { t, c }))
        .collect();
    let out = (0..rng.range(2, 4))
        .map(|_| all[rng.below(all.len() as u64) as usize])
        .collect();
    Query {
        tables: ts,
        pred: and_all(&conj),
        out,
    }
}

fn count_ops(p: &Plan, f: &dyn Fn(&Op) -> bool) -> usize {
    f(&p.op) as usize + p.kids.iter().map(|k| count_ops(k, f)).sum::<usize>()
}
fn uses_index(p: &Plan) -> bool {
    count_ops(p, &|o| matches!(o, Op::IndexScan { .. })) > 0
}
fn hash_builds_smaller(p: &Plan) -> bool {
    let ok =
        !matches!(p.op, Op::HashJoin { .. }) || p.kids[1].rows <= p.kids[0].rows * (1.0 + 1e-9);
    ok && p.kids.iter().all(hash_builds_smaller)
}

fn actual_count(db: &Db, t: usize, e: &Expr) -> f64 {
    let schema = full_schema(&db.cat, t);
    db.data[t]
        .iter()
        .filter(|r| truthy(eval(e, r, &schema)))
        .count() as f64
}

/// Values that make folding hard: NULL, the ends of i64, and numbers that are true but not 1.
const AWKWARD: [i64; 8] = [NULL, NULL + 1, i64::MAX, -1, 0, 1, 2, 5];

/// A random expression over the columns of one table, NOT limited to well-typed conditions: AND and OR get numbers,
/// IN lists hold NULL, and arithmetic overflows.
fn awkward_expr(rng: &mut Rng, t: usize, depth: usize) -> Expr {
    let value = |rng: &mut Rng| AWKWARD[rng.below(AWKWARD.len() as u64) as usize];
    if depth == 0 {
        return if rng.chance(0.5) {
            col(t, rng.below(2) as usize)
        } else {
            Lit(value(rng))
        };
    }
    let sub = |rng: &mut Rng| awkward_expr(rng, t, depth - 1);
    match rng.below(6) {
        0 => {
            let ops = [
                BinOp::Eq,
                BinOp::Ne,
                BinOp::Lt,
                BinOp::Le,
                BinOp::Gt,
                BinOp::Ge,
                BinOp::Add,
                BinOp::Mul,
            ];
            let op = ops[rng.below(8) as usize];
            let a = sub(rng);
            bin(op, a, sub(rng))
        }
        1 => {
            let a = sub(rng);
            and2(a, sub(rng))
        }
        2 => {
            let a = sub(rng);
            or2(a, sub(rng))
        }
        3 => {
            let a = sub(rng);
            In(Box::new(a), vec![value(rng), value(rng)])
        }
        _ => awkward_expr(rng, t, 0),
    }
}

/// Differential check of the folding function alone: the same value on every row.
fn fold_agrees_with_eval() {
    let mut rng = Rng::new(0xF01D);
    let schema = [ColRef { t: 0, c: 0 }, ColRef { t: 0, c: 1 }];
    for _ in 0..40_000 {
        let e = awkward_expr(&mut rng, 0, 3);
        let folded = fold(&e);
        for _ in 0..4 {
            let row = [
                AWKWARD[rng.below(8) as usize],
                AWKWARD[rng.below(8) as usize],
            ];
            assert_eq!(
                eval(&folded, &row, &schema),
                eval(&e, &row, &schema),
                "{e:?} folded to {folded:?} on {row:?}"
            );
        }
    }
}

/// Differential check of the whole pipeline on the same kind of predicate, with an index on the first column.
fn folded_queries_agree_with_naive() {
    let mut rng = Rng::new(0xD1FF);
    for _ in 0..4_000 {
        let rows = (0..1 + rng.below(6))
            .map(|_| {
                vec![
                    AWKWARD[rng.below(8) as usize],
                    AWKWARD[rng.below(8) as usize],
                ]
            })
            .collect();
        let db = make_db(vec![spec("t", &["a", "b"], &[0], rows)]);
        let mut pred = awkward_expr(&mut rng, 0, 3);
        if rng.chance(0.5) {
            let more = awkward_expr(&mut rng, 0, 2);
            pred = and2(pred, more);
        }
        let q = Query {
            tables: vec![0],
            pred,
            out: vec![ColRef { t: 0, c: 0 }, ColRef { t: 0, c: 1 }],
        };
        let naive = sorted(run(&naive_physical(&naive_logical(&q), &db.cat), &db));
        let plan = optimize(&q, &db.cat).expect("a valid query");
        assert_eq!(
            sorted(run(&plan, &db)),
            naive,
            "{:?} over {:?}",
            q.pred,
            db.data[0]
        );
    }
}

/// Each check here failed on an earlier version of this file.
fn regressions() {
    let schema = [ColRef { t: 0, c: 0 }];
    let plus_two = bin(BinOp::Add, col(0, 0), Lit(2));
    assert_eq!(
        eval(&bin(BinOp::Lt, plus_two, Lit(0)), &[NULL], &schema),
        0,
        "NULL + 2 is NULL, so it is not below 0"
    );
    assert_eq!(
        eval(&and2(col(0, 0), Lit(1)), &[NULL], &schema),
        0,
        "a NULL condition does not hold"
    );

    let arith = |op: BinOp, a: i64, b: i64| eval(&bin(op, Lit(a), Lit(b)), &[], &[]);
    // none of these wraps to the NULL sentinel itself, so a wrapping operator is noticed
    assert_eq!(arith(BinOp::Add, i64::MAX, 2), NULL, "overflow of +");
    assert_eq!(arith(BinOp::Add, NULL + 1, -3), NULL, "underflow of +");
    assert_eq!(arith(BinOp::Mul, i64::MAX, 2), NULL, "overflow of *");
    assert_eq!(arith(BinOp::Add, 2, 3), 5);
    assert_eq!(arith(BinOp::Mul, -4, 3), -12);
    assert_eq!(
        bounds(BinOp::Gt, i64::MAX),
        NO_VALUES,
        "nothing is above i64::MAX"
    );
    for op in [BinOp::Eq, BinOp::Lt, BinOp::Le, BinOp::Gt, BinOp::Ge] {
        assert_eq!(bounds(op, NULL), NO_VALUES, "{op:?} NULL");
    }
    fold_agrees_with_eval();
    folded_queries_agree_with_naive();

    let mut rows: Vec<Row> = (0..5000).map(|i| vec![i]).collect();
    rows.push(vec![NULL]);
    let db = make_db(vec![spec("t", &["a"], &[0], rows)]);
    let query = |pred: Expr| Query {
        tables: vec![0],
        pred,
        out: vec![ColRef { t: 0, c: 0 }],
    };
    let count = |pred: Expr| run(&optimize(&query(pred), &db.cat).expect("valid"), &db).len();
    for (op, v) in [(BinOp::Gt, i64::MAX), (BinOp::Lt, NULL), (BinOp::Gt, NULL)] {
        assert_eq!(count(bin(op, col(0, 0), Lit(v))), 0, "{op:?} {v}");
    }
    // the index consumes both conjuncts: the comparison with NULL must still empty the range
    for op in [BinOp::Ge, BinOp::Gt] {
        let pred = and2(
            bin(op, col(0, 0), Lit(NULL)),
            bin(BinOp::Le, col(0, 0), Lit(3)),
        );
        let plan = optimize(&query(pred.clone()), &db.cat).expect("valid");
        assert!(uses_index(&plan), "this check needs the index path");
        assert_eq!(count(pred), 0, "a {op:?} NULL AND a <= 3");
    }

    let empty = ColStats::analyze(&[], 0);
    assert_eq!(empty.null_frac, 0.0, "an empty column is not NaN");

    let many = make_db(
        (0..33)
            .map(|i| spec(&format!("t{i}"), &["x"], &[], vec![vec![1]]))
            .collect(),
    );
    let refused = |tables: Vec<usize>| {
        let first = tables.first().copied().unwrap_or(0);
        let q = Query {
            tables,
            pred: Lit(1),
            out: vec![ColRef { t: first, c: 0 }],
        };
        optimize(&q, &many.cat).is_err()
    };
    assert!(refused(vec![32]), "table id beyond the bitmask");
    assert!(refused((0..13).collect()), "too many tables for the DP");
    assert!(refused(vec![0, 0]), "self-join");
    assert!(refused(vec![]), "no tables");
    assert!(!refused(vec![0, 1]));

    let text = show(
        &bin(BinOp::Mul, bin(BinOp::Add, col(0, 0), Lit(1)), Lit(NULL)),
        &db.cat,
    );
    assert_eq!(text, "(t.a + 1) * NULL");
    println!("  (h) regression checks OK");
}

// ---------- demo + verification ----------
fn main() {
    let mut rng = Rng::new(0xC0FFEE_u64);
    let db = shop_db(&mut rng);
    let cat = &db.cat;
    let (users, orders, products, ships) = (0, 1, 2, 3);

    println!("== 1. statistics (ANALYZE) ==");
    for (t, tm) in cat.tables.iter().enumerate() {
        for (c, name) in tm.cols.iter().enumerate() {
            let s = &tm.stats[c];
            println!(
                "  {:>10}.{:<10} rows={:>5} ndv={:>5} range=[{},{}] null={:.2} mcv={}",
                tm.name,
                name,
                tm.rows,
                s.ndv,
                s.min,
                s.max,
                s.null_frac,
                s.mcv.len()
            );
        }
        let _ = t;
    }

    println!("\n== 2. cardinality estimation: estimate vs TRUE rows (q-error) ==");
    let no_mcv = Catalog {
        use_mcv: false,
        ..cat.clone()
    };
    let gt = |a: Expr, v: i64| bin(BinOp::Gt, a, Lit(v));
    let lt = |a: Expr, v: i64| bin(BinOp::Lt, a, Lit(v));
    let eq = |a: Expr, v: i64| bin(BinOp::Eq, a, Lit(v));
    let uniform: Vec<(&str, usize, Expr)> = vec![
        ("orders.amount = 500", orders, eq(col(1, 3), 500)),
        ("orders.amount < 250", orders, lt(col(1, 3), 250)),
        (
            "orders.amount BETWEEN 100 AND 400",
            orders,
            and2(
                bin(BinOp::Ge, col(1, 3), Lit(100)),
                bin(BinOp::Le, col(1, 3), Lit(400)),
            ),
        ),
        (
            "amount < 300 AND product_id < 100 (independent)",
            orders,
            and2(lt(col(1, 3), 300), lt(col(1, 2), 100)),
        ),
        (
            "amount < 100 OR product_id < 50",
            orders,
            or2(lt(col(1, 3), 100), lt(col(1, 2), 50)),
        ),
        (
            "product_id IN (1,2,3,4,5,6,7,8,9,10)",
            orders,
            In(Box::new(col(1, 2)), (1..=10).collect()),
        ),
        ("users.age > 50 (5% NULLs)", users, gt(col(0, 1), 50)),
    ];
    let mut worst_uniform: f64 = 1.0;
    for (name, t, e) in &uniform {
        let (est, act) = (
            cat.tables[*t].rows as f64 * cat.sel(e),
            actual_count(&db, *t, e),
        );
        let q = q_error(est, act);
        worst_uniform = worst_uniform.max(q);
        println!("  uniform   {name:<50} est={est:>8.1} actual={act:>7.0} q-error={q:.2}");
    }
    assert!(
        worst_uniform <= 1.5,
        "uniform independent data should be within q-error 1.5, got {worst_uniform}"
    );
    println!("  -> bound on uniform data: q-error <= 1.5 (worst {worst_uniform:.2})");

    let corr = and2(eq(col(0, 2), 3), eq(col(0, 3), 3 * 5 + 1));
    let (est, act) = (5000.0 * cat.sel(&corr), actual_count(&db, users, &corr));
    let q_corr = q_error(est, act);
    println!(
        "  CORRELATED country=3 AND city=16 (city implies country): est={est:.1} actual={act:.0} q-error={q_corr:.1}  <- independence assumption fails"
    );
    assert!(q_corr > 10.0);
    let hot = eq(col(0, 4), 0);
    let (est_u, est_m, act) = (
        5000.0 * no_mcv.sel(&hot),
        5000.0 * cat.sel(&hot),
        actual_count(&db, users, &hot),
    );
    println!(
        "  SKEWED    tier = 0 (Zipf hot value): uniform est={est_u:.0} (q={:.1}), with MCV est={est_m:.0} (q={:.2}), actual={act:.0}",
        q_error(est_u, act),
        q_error(est_m, act)
    );
    assert!(q_error(est_u, act) > 5.0 && q_error(est_m, act) < 1.3);

    let jq = Query {
        tables: vec![users, orders],
        pred: and_all(&[bin(BinOp::Eq, col(0, 0), col(1, 1)), lt(col(1, 3), 300)]),
        out: vec![ColRef { t: 0, c: 0 }],
    };
    let jp = optimize(&jq, cat).expect("a valid query");
    let jact = run(&jp, &db).len() as f64;
    let jest = jp.kids[0].rows;
    println!(
        "  JOIN users x orders ON id=user_id AND amount<300: est={jest:.0} actual={jact:.0} q-error={:.2}  (sel = 1/max(ndv_l, ndv_r))",
        q_error(jest, jact)
    );
    assert!(q_error(jest, jact) <= 2.0);

    println!("\n== 3. rewrite rules to fixpoint ==");
    let q4 = Query {
        tables: vec![users, orders, products, ships],
        pred: and_all(&[
            bin(BinOp::Eq, col(0, 0), col(1, 1)),
            bin(BinOp::Eq, col(1, 2), col(2, 0)),
            bin(BinOp::Eq, col(3, 1), col(1, 0)),
            eq(col(0, 2), 7),
            lt(col(2, 1), 3),
            bin(BinOp::Eq, Lit(1), Lit(1)),
            gt(col(3, 2), 10),
            bin(BinOp::Gt, col(1, 3), bin(BinOp::Add, Lit(100), Lit(50))),
        ]),
        out: vec![
            ColRef { t: 0, c: 0 },
            ColRef { t: 1, c: 3 },
            ColRef { t: 2, c: 2 },
            ColRef { t: 3, c: 2 },
        ],
    };
    let naive = naive_logical(&q4);
    let mut fired = BTreeMap::new();
    let p1 = rewrite(&naive, &PHASE1, cat, &mut fired);
    let p2 = rewrite(&p1, &PHASE2, cat, &mut fired);
    let (mut s_before, mut s_after) = (String::new(), String::new());
    show_logical(&naive, cat, 1, &mut s_before);
    show_logical(&p2, cat, 1, &mut s_after);
    println!("before:\n{s_before}after (rules fired: {fired:?}):\n{s_after}");

    println!(
        "== 4. access paths: IndexScan vs SeqScan crossover on orders.amount (20000 rows, 200 pages) =="
    );
    let amount = ColRef { t: orders, c: 3 };
    let mut flipped_at = None;
    for hi in [1, 2, 3, 5, 8, 10, 20, 50, 200, 1000] {
        let seq = seq_scan(cat, orders).cost + 20000.0 * cat.sel(&lt(col(1, 3), hi + 1)) * CPU_OP;
        let ix = index_scan(cat, orders, 3, 1, hi);
        let pick = &leaf_plans(cat, orders, &[bin(BinOp::Le, Col(amount), Lit(hi))]);
        let choice = cheapest_of(pick.clone());
        let name = if uses_index(&choice) {
            "IndexScan"
        } else {
            "SeqScan"
        };
        if name == "SeqScan" && flipped_at.is_none() {
            flipped_at = Some(ix.rows);
        }
        println!(
            "  amount <= {hi:<4} est rows={:>7.0} (sel {:.3}%)  seq={seq:>6.1} index={:>6.1}  -> {name}",
            ix.rows,
            100.0 * ix.rows / 20000.0,
            ix.cost
        );
    }
    let sel_plan = cheapest_of(leaf_plans(
        cat,
        orders,
        &[bin(BinOp::Le, Col(amount), Lit(2))],
    ));
    let unsel_plan = cheapest_of(leaf_plans(
        cat,
        orders,
        &[bin(BinOp::Le, Col(amount), Lit(500))],
    ));
    assert!(
        uses_index(&sel_plan),
        "selective predicate must use the index"
    );
    assert!(
        !uses_index(&unsel_plan),
        "unselective predicate must use SeqScan"
    );
    println!(
        "  crossover near {:.0} matching rows (~pages/RAND_PAGE = {:.0}+); selective -> IndexScan, unselective -> SeqScan: OK",
        flipped_at.unwrap_or(0.0),
        pages(cat, orders) / RAND_PAGE
    );

    println!("\n== 5. join algorithms and interesting orders ==");
    let big = seq_scan(cat, orders);
    let small = seq_scan(cat, products);
    let pred = vec![bin(BinOp::Eq, col(1, 2), col(2, 0))];
    for (l, r, tag) in [
        (&big, &small, "probe=orders build=products"),
        (&small, &big, "probe=products build=orders"),
    ] {
        for c in join_candidates(l, r, &pred, 20000.0) {
            println!(
                "  {tag:<30} {:<12} cost={:>9.1}",
                describe(&c, cat).split(' ').next().unwrap(),
                c.cost
            );
        }
    }
    let cands: Vec<Plan> = join_candidates(&big, &small, &pred, 20000.0)
        .into_iter()
        .chain(join_candidates(&small, &big, &pred, 20000.0))
        .collect();
    let best_hash = cands
        .iter()
        .filter(|c| matches!(c.op, Op::HashJoin { .. }))
        .min_by(|a, b| a.cost.total_cmp(&b.cost))
        .unwrap();
    assert!(
        best_hash.kids[1].rows < best_hash.kids[0].rows,
        "hash join must build on the smaller input"
    );
    println!(
        "  cheapest HashJoin builds on {} rows (the smaller side): OK",
        best_hash.kids[1].rows
    );
    let u_ix = index_scan(cat, users, 0, NULL + 1, i64::MAX); // ordered by users.id
    let o_ix = index_scan(cat, orders, 1, NULL + 1, i64::MAX); // ordered by orders.user_id
    let jpred = vec![bin(BinOp::Eq, col(0, 0), col(1, 1))];
    let merge_ordered = join_candidates(&u_ix, &o_ix, &jpred, 20000.0)
        .into_iter()
        .find(|c| matches!(c.op, Op::MergeJoin { .. }))
        .unwrap();
    let merge_unordered = join_candidates(
        &seq_scan(cat, users),
        &seq_scan(cat, orders),
        &jpred,
        20000.0,
    )
    .into_iter()
    .find(|c| matches!(c.op, Op::MergeJoin { .. }))
    .unwrap();
    let sorts = |p: &Plan| count_ops(p, &|o| matches!(o, Op::Sort(_)));
    assert_eq!((sorts(&merge_ordered), sorts(&merge_unordered)), (0, 2));
    println!(
        "  MergeJoin over two index-ordered inputs: {} sorts; over two heap scans: {} sorts (interesting order skips the sort)",
        sorts(&merge_ordered),
        sorts(&merge_unordered)
    );

    println!(
        "\n== 6. join ordering on the 4-table query (DP bushy / DP left-deep / greedy GOO) =="
    );
    let rewritten = rewrite(&naive, &PHASE1, cat, &mut BTreeMap::new());
    let g = build_graph(&rewritten, cat);
    let ctx = Ctx::new(cat, &g);
    let bushy = dp(&ctx, Shape::Bushy, false).unwrap();
    let left = dp(&ctx, Shape::LeftDeep, false).unwrap();
    let goo = greedy(&ctx);
    println!(
        "  cost: bushy DP={:.1}  left-deep DP={:.1}  greedy={:.1}",
        bushy.cost, left.cost, goo.cost
    );
    assert!(left.cost >= bushy.cost - 1e-9 && goo.cost >= bushy.cost - 1e-9);
    let final_plan = project_plan(bushy.clone(), &g.out);
    println!("EXPLAIN (optimised):\n{}", explained(&final_plan, cat));
    let res = sorted(run(&final_plan, &db));
    assert_eq!(res, sorted(run(&project_plan(left, &g.out), &db)));
    assert_eq!(res, sorted(run(&project_plan(goo, &g.out), &db)));
    println!(
        "  executed: {} result rows; identical for bushy, left-deep and greedy plans",
        res.len()
    );
    assert!(hash_builds_smaller(&bushy));

    println!("\n== 7. verification ==");
    // (a) DP == brute force over every join tree, n <= 5
    let mut vr = Rng::new(7);
    let mut checked = 0;
    for case in 0..40 {
        let rdb = rand_db(&mut vr, 5, 400);
        let n = 3 + case % 3;
        let q = rand_query(&mut vr, &rdb, n);
        let rw = rewrite(&naive_logical(&q), &PHASE1, &rdb.cat, &mut BTreeMap::new());
        let g = build_graph(&rw, &rdb.cat);
        let ctx = Ctx::new(&rdb.cat, &g);
        for cross in [false, true] {
            let (d, b) = (
                dp(&ctx, Shape::Bushy, cross).map(|p| p.cost),
                brute_force_cost(&ctx, cross),
            );
            match (d, b) {
                (Some(d), Some(b)) => {
                    assert!((d - b).abs() <= 1e-9 * b.max(1.0), "DP {d} != brute {b}")
                }
                (None, None) => {}
                other => panic!("DP/brute disagree on feasibility: {other:?}"),
            }
            checked += 1;
        }
        let ld = dp(&ctx, Shape::LeftDeep, true).unwrap().cost;
        assert!(ld >= dp(&ctx, Shape::Bushy, true).unwrap().cost - 1e-9);
    }
    println!(
        "  (a) DP cost == brute-force minimum over all join trees: {checked} comparisons (n=3..5, with and without cross products) OK"
    );

    // (b)+(c) differential execution + pushdown never raises estimated cost
    let mut dr = Rng::new(2024);
    let (mut nonempty, mut max_ratio) = (0, 0.0f64);
    for _ in 0..200 {
        let rdb = rand_db(&mut dr, 4, 12);
        let n = 2 + dr.below(3) as usize;
        let q = rand_query(&mut dr, &rdb, n);
        let nl = naive_logical(&q);
        let (p1, mut f) = (
            rewrite(&nl, &PHASE1, &rdb.cat, &mut BTreeMap::new()),
            BTreeMap::new(),
        );
        let p2 = rewrite(&p1, &PHASE2, &rdb.cat, &mut f);
        let (c0, c1) = (
            naive_physical(&nl, &rdb.cat).cost,
            naive_physical(&p1, &rdb.cat).cost,
        );
        assert!(c1 <= c0 * (1.0 + 1e-9), "rewrite raised cost {c0} -> {c1}");
        max_ratio = max_ratio.max(c1 / c0);
        let expect = sorted(run(&naive_physical(&nl, &rdb.cat), &rdb));
        assert_eq!(
            expect,
            sorted(run(&naive_physical(&p1, &rdb.cat), &rdb)),
            "phase-1 rewrite changed results"
        );
        assert_eq!(
            expect,
            sorted(run(&naive_physical(&p2, &rdb.cat), &rdb)),
            "pruning changed results"
        );
        let opt = optimize(&q, &rdb.cat).expect("a valid query");
        assert_eq!(
            expect,
            sorted(run(&opt, &rdb)),
            "optimised plan differs:\n{}",
            explained(&opt, &rdb.cat)
        );
        nonempty += !expect.is_empty() as usize;
    }
    println!(
        "  (b) optimised == rewritten == naive result multisets on 200 random queries ({nonempty} non-empty) OK"
    );
    println!(
        "  (c) the first rule batch never raised estimated cost here (worst new/old ratio {max_ratio:.3}) OK"
    );

    // (d) greedy vs DP on larger queries
    let mut gr = Rng::new(99);
    let (mut worse, mut sum_ratio, total) = (0, 0.0, 20);
    for _ in 0..total {
        let rdb = rand_db(&mut gr, 10, 3000);
        let q = rand_query(&mut gr, &rdb, 10);
        let rw = rewrite(&naive_logical(&q), &PHASE1, &rdb.cat, &mut BTreeMap::new());
        let g = build_graph(&rw, &rdb.cat);
        let ctx = Ctx::new(&rdb.cat, &g);
        let d = dp(&ctx, Shape::Bushy, false)
            .or_else(|| dp(&ctx, Shape::Bushy, true))
            .unwrap()
            .cost;
        let go = greedy(&ctx).cost;
        assert!(go >= d - 1e-9 * d, "greedy {go} beat DP {d}");
        worse += (go > d * 1.0001) as usize;
        sum_ratio += go / d;
    }
    println!(
        "  (d) 10-table queries: greedy >= DP always; greedy strictly worse in {worse}/{total}, mean cost ratio greedy/DP = {:.3}",
        sum_ratio / total as f64
    );
    println!(
        "  (e) q-error: uniform <= 1.5, joins <= 2.0, correlated = {q_corr:.1} (printed above) OK"
    );
    println!(
        "  (f) IndexScan for selective, SeqScan for unselective predicates OK    (g) hash join builds the smaller input OK"
    );
    regressions();
}
