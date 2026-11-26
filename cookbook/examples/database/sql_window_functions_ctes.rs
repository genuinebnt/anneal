//! RECIPE 724 (database): modern SQL over in-memory tables: window functions, CTEs, recursive CTEs, LATERAL, GROUPING SETS.
//! LEVEL: advanced
//! Use in: SQL engines and analytics layers; anywhere "top-N per group", running totals, graph traversal or subtotals are needed.   Run: cargo run --release --example sql_window_functions_ctes
//! COMMON USES: window operators in PostgreSQL, DuckDB, SQL Server and Snowflake (sort + frame evaluation, segment trees for
//! non-invertible aggregates); WITH [RECURSIVE] for org charts, bill-of-materials and graph reachability (PostgreSQL, SQLite,
//! MySQL 8); LATERAL joins for top-N-per-group; ROLLUP / CUBE / GROUPING SETS for report subtotals in BI tools.
//! Course: CMU 15-445 Lecture 02 (modern SQL: window functions, CTEs, nested queries, lateral joins).
#![allow(dead_code)]
//!
//! WHAT: executors for the constructs, over tables of nullable integers (`Vec<Option<i64>>`, `None` = NULL):
//! * WINDOW FUNCTIONS: ROW_NUMBER, RANK, DENSE_RANK, NTILE, LAG, LEAD, FIRST_VALUE, LAST_VALUE and the aggregates SUM, AVG, MIN,
//!   MAX, COUNT over a FRAME, with PARTITION BY, ORDER BY (ASC/DESC, NULLS FIRST/LAST) and frames `ROWS|RANGE BETWEEN <start> AND
//!   <end>` where a bound is UNBOUNDED PRECEDING, n PRECEDING, CURRENT ROW, n FOLLOWING or UNBOUNDED FOLLOWING.
//! * CTEs: non-recursive `WITH` (evaluated once and materialised vs inlined per reference) and WITH RECURSIVE via SEMI-NAIVE
//!   fixpoint (number series, org chart, transitive closure).
//! * LATERAL / correlated subqueries as a nested-loop with the outer row as parameter, and their decorrelation.
//! * GROUPING SETS, ROLLUP and CUBE with the GROUPING() bitmask.
//!
//! WINDOW ALGORITHM (the standard one): 1. SORT the row ids by (partition keys, order keys); ties keep input order (stable), so
//! ROW_NUMBER is deterministic. 2. Walk the sorted array in runs of equal partition key. 3. Per partition, every row gets a
//! frame [lo, hi) of sorted positions. ROWS frames are position arithmetic. RANGE frames work on ORDER BY VALUES: a row's PEERS
//! are the rows with an equal order key (RANK needs them; CURRENT ROW in RANGE mode means the whole peer group) and
//! `RANGE n PRECEDING` selects rows whose key is within n of the current key: found by binary search on the sorted keys.
//! NULLs form their own peer group at the chosen end; a NULL current row treats offset bounds as CURRENT ROW (PostgreSQL
//! does the same). 4. Evaluate the function over the frame: SUM/COUNT/AVG use PREFIX SUMS (O(1) per row, valid because they
//! are invertible), MIN/MAX use a SEGMENT TREE (O(log n) per row, since max cannot be "un-added" when a row leaves the
//! frame), FIRST/LAST_VALUE are index lookups, ranking functions need only positions and peer boundaries.
//! The naive reference re-derives each frame from the definition by scanning every row of the partition: O(n * frame) or worse.
//! GOTCHA shown in the demo: the DEFAULT frame with ORDER BY is RANGE UNBOUNDED PRECEDING..CURRENT ROW, so LAST_VALUE returns
//! the last PEER, not the last row of the partition, unless the frame is widened to UNBOUNDED FOLLOWING.
//!
//! RECURSIVE CTE: `WITH RECURSIVE t AS (base UNION step(t))` is the least fixpoint of t = base U step(t). NAIVE evaluation
//! re-runs `step` on the whole t each round (cost grows with iterations x |t|). SEMI-NAIVE feeds `step` only the DELTA, the
//! rows that were new in the previous round: for linear recursion (one reference to t, as here) the result is identical and
//! each derived row is expanded exactly once. UNION dedups (so cyclic graphs terminate), UNION ALL keeps duplicates and
//! needs a terminating condition in `step` (a cycle with UNION ALL never ends; the iteration cap reports it).
//!
//! GROUPING SETS: `ROLLUP(a,b)` = sets (a,b), (a), (); `CUBE(a,b)` = all four subsets. Because SUM and COUNT are distributive
//! the sets are derived from ONE pass that groups by all dimensions (the finest set), not by rescanning the table per set.
//! GROUPING(col) bits tell a rolled-up NULL apart from a real NULL value in the data.
//!
//! CHECKS: 500 random tables (NULLs in partition/order/value columns, ties, empty tables, one-row partitions) x random specs
//! (partition/order/direction/NULLS FIRST|LAST, ROWS and RANGE frames incl. offsets) x all 14 window functions: efficient
//! implementation == naive reference. Recursive CTEs are checked against Warshall's closure, a parent-chain depth oracle and
//! naive fixpoint evaluation; LATERAL top-2 == the ROW_NUMBER formulation; decorrelated == correlated; grouping sets == per-set
//! rescans on 200 random tables (incl. NULL dimension values). Timings are printed, never asserted.
//! Read: PostgreSQL window functions https://www.postgresql.org/docs/current/tutorial-window.html   WITH queries https://www.postgresql.org/docs/current/queries-with.html
//!       Efficient Processing of Window Functions in Analytical SQL Queries https://www.vldb.org/pvldb/vol8/p1058-leis.pdf   CMU 15-445 https://15445.courses.cs.cmu.edu/fall2024/

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

type Row = Vec<Option<i64>>;

// ================================================================ WINDOW FUNCTIONS
#[derive(Clone, Copy, Debug)]
struct OrderKey {
    col: usize,
    desc: bool,
    nulls_first: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Bound {
    UnboundedPreceding,
    Preceding(i64),
    CurrentRow,
    Following(i64),
    UnboundedFollowing,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Rows,
    Range,
}
#[derive(Clone, Copy, Debug)]
struct Frame {
    mode: Mode,
    start: Bound,
    end: Bound,
}
/// SQL default when no frame is written: RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW (= the whole partition if there is no ORDER BY).
const DEFAULT_FRAME: Frame = Frame {
    mode: Mode::Range,
    start: Bound::UnboundedPreceding,
    end: Bound::CurrentRow,
};

#[derive(Clone, Debug)]
struct WindowSpec {
    part: Vec<usize>,
    order: Vec<OrderKey>,
    frame: Option<Frame>,
}
#[derive(Clone, Copy, Debug)]
enum WFn {
    RowNumber,
    Rank,
    DenseRank,
    Ntile(usize),
    Lag(usize, usize, Option<i64>), // column, offset, default
    Lead(usize, usize, Option<i64>),
    FirstValue(usize),
    LastValue(usize),
    Sum(usize),
    Avg(usize),
    Min(usize),
    Max(usize),
    Count(usize),
}
impl WFn {
    fn column(&self) -> Option<usize> {
        match *self {
            WFn::RowNumber | WFn::Rank | WFn::DenseRank | WFn::Ntile(_) => None,
            WFn::Lag(c, ..)
            | WFn::Lead(c, ..)
            | WFn::FirstValue(c)
            | WFn::LastValue(c)
            | WFn::Sum(c)
            | WFn::Avg(c)
            | WFn::Min(c)
            | WFn::Max(c)
            | WFn::Count(c) => Some(c),
        }
    }
    fn uses_frame(&self) -> bool {
        matches!(
            self,
            WFn::FirstValue(_)
                | WFn::LastValue(_)
                | WFn::Sum(_)
                | WFn::Avg(_)
                | WFn::Min(_)
                | WFn::Max(_)
                | WFn::Count(_)
        )
    }
}

/// Sort key of one row: per ORDER BY column (rank, value); rank 0/2 puts NULLs first/last, 1 = a real value (negated for DESC),
/// so ONE lexicographic comparison implements ASC/DESC and NULLS FIRST/LAST for every column.
type Key = Vec<(u8, i64)>;
fn key_of(row: &Row, order: &[OrderKey]) -> Key {
    order
        .iter()
        .map(|k| match row[k.col] {
            None => (if k.nulls_first { 0 } else { 2 }, 0),
            Some(v) => (1, if k.desc { -v } else { v }),
        })
        .collect()
}
/// Key that a RANGE offset bound points at: current key +- n for a value row; a NULL row points at itself (its peers).
fn range_target(key: &Key, b: Bound) -> Key {
    let shift = |d: i64| {
        if key[0].0 == 1 {
            vec![(1, key[0].1 + d)]
        } else {
            key.clone()
        }
    };
    match b {
        Bound::Preceding(n) => shift(-n),
        Bound::Following(n) => shift(n),
        _ => key.clone(),
    }
}

// ---- segment tree for MIN / MAX over a frame
struct SegTree {
    n: usize,
    t: Vec<Option<i64>>,
    is_min: bool,
}
impl SegTree {
    fn combine(is_min: bool, a: Option<i64>, b: Option<i64>) -> Option<i64> {
        match (a, b) {
            (None, x) | (x, None) => x, // NULLs are ignored by aggregates
            (Some(x), Some(y)) => Some(if is_min { x.min(y) } else { x.max(y) }),
        }
    }
    fn build(vals: &[Option<i64>], is_min: bool) -> Self {
        let n = vals.len().max(1);
        let mut t = vec![None; 2 * n];
        t[n..n + vals.len()].copy_from_slice(vals);
        for i in (1..n).rev() {
            t[i] = Self::combine(is_min, t[2 * i], t[2 * i + 1]);
        }
        SegTree { n, t, is_min }
    }
    /// Aggregate of positions [lo, hi) in O(log n).
    fn query(&self, lo: usize, hi: usize) -> Option<i64> {
        let (mut l, mut r, mut acc) = (lo + self.n, hi + self.n, None);
        while l < r {
            if l & 1 == 1 {
                acc = Self::combine(self.is_min, acc, self.t[l]);
                l += 1;
            }
            if r & 1 == 1 {
                r -= 1;
                acc = Self::combine(self.is_min, acc, self.t[r]);
            }
            l >>= 1;
            r >>= 1;
        }
        acc
    }
}

/// Frame [lo, hi) of sorted position p in a partition of k rows. Empty frames come back as lo == hi.
fn frame_fast(f: &Frame, p: usize, k: usize, keys: &[Key]) -> (usize, usize) {
    let (lo, hi) = match f.mode {
        Mode::Rows => {
            let (pi, ki) = (p as i64, k as i64);
            let lo = match f.start {
                Bound::UnboundedPreceding => 0,
                Bound::Preceding(m) => pi - m,
                Bound::CurrentRow => pi,
                Bound::Following(m) => pi + m,
                Bound::UnboundedFollowing => ki,
            };
            let hi = match f.end {
                Bound::UnboundedPreceding => 0,
                Bound::Preceding(m) => pi - m + 1,
                Bound::CurrentRow => pi + 1,
                Bound::Following(m) => pi + m + 1,
                Bound::UnboundedFollowing => ki,
            };
            (lo.clamp(0, ki) as usize, hi.clamp(0, ki) as usize)
        }
        Mode::Range => {
            // Binary search on the sorted keys: first position >= start target, one past the last position <= end target.
            let lo = match f.start {
                Bound::UnboundedPreceding => 0,
                Bound::UnboundedFollowing => k,
                b => {
                    let t = range_target(&keys[p], b);
                    keys.partition_point(|x| *x < t)
                }
            };
            let hi = match f.end {
                Bound::UnboundedFollowing => k,
                Bound::UnboundedPreceding => 0,
                b => {
                    let t = range_target(&keys[p], b);
                    keys.partition_point(|x| *x <= t)
                }
            };
            (lo, hi)
        }
    };
    if lo >= hi { (0, 0) } else { (lo, hi) }
}

fn ntile(p: usize, k: usize, n: usize) -> usize {
    // First k % n buckets get one extra row.
    let (q, r) = (k / n, k % n);
    if p < r * (q + 1) {
        p / (q + 1) + 1
    } else {
        r + (p - r * (q + 1)) / q + 1
    }
}

fn window_fast(rows: &[Row], spec: &WindowSpec, f: &WFn) -> Vec<Option<f64>> {
    let n = rows.len();
    let keys: Vec<Key> = rows.iter().map(|r| key_of(r, &spec.order)).collect();
    let pk: Vec<Vec<Option<i64>>> = rows
        .iter()
        .map(|r| spec.part.iter().map(|&c| r[c]).collect())
        .collect();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| pk[a].cmp(&pk[b]).then_with(|| keys[a].cmp(&keys[b]))); // stable: ties keep input order
    let frame = spec.frame.unwrap_or(DEFAULT_FRAME);
    let mut out = vec![None; n];
    let mut s = 0;
    while s < n {
        let mut e = s + 1;
        while e < n && pk[idx[e]] == pk[idx[s]] {
            e += 1;
        }
        let part = &idx[s..e];
        let k = part.len();
        let pkeys: Vec<Key> = part.iter().map(|&i| keys[i].clone()).collect();
        let vals: Vec<Option<i64>> = part
            .iter()
            .map(|&i| f.column().and_then(|c| rows[i][c]))
            .collect();
        // Prefix sums and counts of non-NULL values: SUM/COUNT/AVG of any frame in O(1).
        let (mut ps, mut pc) = (vec![0i64; k + 1], vec![0i64; k + 1]);
        for i in 0..k {
            ps[i + 1] = ps[i] + vals[i].unwrap_or(0);
            pc[i + 1] = pc[i] + vals[i].is_some() as i64;
        }
        let tree = matches!(f, WFn::Min(_) | WFn::Max(_))
            .then(|| SegTree::build(&vals, matches!(f, WFn::Min(_))));
        let (mut first_of_peers, mut dense) = (0usize, 0usize);
        for p in 0..k {
            if p == 0 || pkeys[p] != pkeys[p - 1] {
                first_of_peers = p; // peers = equal order key; both RANK and DENSE_RANK are driven by peer-group starts
                dense += 1;
            }
            let (lo, hi) = if f.uses_frame() {
                frame_fast(&frame, p, k, &pkeys)
            } else {
                (0, 0)
            };
            let sum = ps[hi] - ps[lo];
            let cnt = pc[hi] - pc[lo];
            let r: Option<f64> = match *f {
                WFn::RowNumber => Some((p + 1) as f64),
                WFn::Rank => Some((first_of_peers + 1) as f64),
                WFn::DenseRank => Some(dense as f64),
                WFn::Ntile(m) => Some(ntile(p, k, m) as f64),
                WFn::Lag(_, off, d) => if p >= off { vals[p - off] } else { d }.map(|v| v as f64),
                WFn::Lead(_, off, d) => {
                    if p + off < k { vals[p + off] } else { d }.map(|v| v as f64)
                }
                WFn::FirstValue(_) => (lo < hi).then(|| vals[lo]).flatten().map(|v| v as f64),
                WFn::LastValue(_) => (lo < hi).then(|| vals[hi - 1]).flatten().map(|v| v as f64),
                WFn::Sum(_) => (cnt > 0).then_some(sum as f64),
                WFn::Avg(_) => (cnt > 0).then(|| sum as f64 / cnt as f64),
                WFn::Count(_) => Some(cnt as f64),
                WFn::Min(_) | WFn::Max(_) => tree.as_ref().unwrap().query(lo, hi).map(|v| v as f64),
            };
            out[part[p]] = r;
        }
        s = e;
    }
    out
}

/// Naive reference: per row, collect its partition, sort it with a key-only comparator, derive the frame by testing EVERY member
/// against the definition of the bounds, then fold. Quadratic or worse; written to be obviously right, not fast.
fn window_naive(rows: &[Row], spec: &WindowSpec, f: &WFn) -> Vec<Option<f64>> {
    let frame = spec.frame.unwrap_or(DEFAULT_FRAME);
    let mut out = vec![None; rows.len()];
    for (i, row) in rows.iter().enumerate() {
        let mut members: Vec<usize> = (0..rows.len())
            .filter(|&j| spec.part.iter().all(|&c| rows[j][c] == row[c]))
            .collect();
        members.sort_by_key(|&j| (key_of(&rows[j], &spec.order), j)); // (key, input index) == stable sort
        let k = members.len();
        let p = members.iter().position(|&j| j == i).unwrap();
        let key = |q: usize| key_of(&rows[members[q]], &spec.order);
        let val = |q: usize| f.column().and_then(|c| rows[members[q]][c]);
        // Membership of position q in the frame of position p, straight from the definition of each bound.
        let in_frame = |q: usize| -> bool {
            let (qi, pi) = (q as i64, p as i64);
            let (s_ok, e_ok) = match frame.mode {
                Mode::Rows => (
                    match frame.start {
                        Bound::UnboundedPreceding => true,
                        Bound::Preceding(m) => qi >= pi - m,
                        Bound::CurrentRow => qi >= pi,
                        Bound::Following(m) => qi >= pi + m,
                        Bound::UnboundedFollowing => false,
                    },
                    match frame.end {
                        Bound::UnboundedPreceding => false,
                        Bound::Preceding(m) => qi <= pi - m,
                        Bound::CurrentRow => qi <= pi,
                        Bound::Following(m) => qi <= pi + m,
                        Bound::UnboundedFollowing => true,
                    },
                ),
                Mode::Range => (
                    match frame.start {
                        Bound::UnboundedPreceding => true,
                        Bound::UnboundedFollowing => false,
                        b => key(q) >= range_target(&key(p), b),
                    },
                    match frame.end {
                        Bound::UnboundedFollowing => true,
                        Bound::UnboundedPreceding => false,
                        b => key(q) <= range_target(&key(p), b),
                    },
                ),
            };
            s_ok && e_ok
        };
        let fr: Vec<usize> = if f.uses_frame() {
            (0..k).filter(|&q| in_frame(q)).collect()
        } else {
            vec![]
        };
        let nn: Vec<i64> = fr.iter().filter_map(|&q| val(q)).collect();
        out[i] = match *f {
            WFn::RowNumber => Some((p + 1) as f64),
            WFn::Rank => Some((1 + (0..k).filter(|&q| key(q) < key(p)).count()) as f64),
            WFn::DenseRank => Some(
                (0..k)
                    .map(key)
                    .filter(|kq| *kq <= key(p))
                    .collect::<HashSet<_>>()
                    .len() as f64,
            ),
            WFn::Ntile(m) => Some(
                (0..m)
                    .find(|&b| {
                        p < (0..=b)
                            .map(|x| k / m + usize::from(x < k % m))
                            .sum::<usize>()
                    })
                    .unwrap() as f64
                    + 1.0,
            ),
            WFn::Lag(_, off, d) => if p >= off { val(p - off) } else { d }.map(|v| v as f64),
            WFn::Lead(_, off, d) => if p + off < k { val(p + off) } else { d }.map(|v| v as f64),
            WFn::FirstValue(_) => fr.first().and_then(|&q| val(q)).map(|v| v as f64),
            WFn::LastValue(_) => fr.last().and_then(|&q| val(q)).map(|v| v as f64),
            WFn::Sum(_) => (!nn.is_empty()).then(|| nn.iter().sum::<i64>() as f64),
            WFn::Avg(_) => {
                (!nn.is_empty()).then(|| nn.iter().sum::<i64>() as f64 / nn.len() as f64)
            }
            WFn::Count(_) => Some(nn.len() as f64),
            WFn::Min(_) => nn.iter().min().map(|&v| v as f64),
            WFn::Max(_) => nn.iter().max().map(|&v| v as f64),
        };
    }
    out
}

fn random_table(rng: &mut Rng, n: usize) -> Vec<Row> {
    // columns: 0 = partition key, 1 = order key, 2 = second order key, 3 = value; ~15% NULLs, tiny domains to force ties.
    let mut cell = |dom: u64| {
        if rng.below(100) < 15 {
            None
        } else {
            Some(rng.below(dom) as i64 - 2)
        }
    };
    (0..n)
        .map(|_| vec![cell(3), cell(6), cell(3), cell(20)])
        .collect()
}

fn random_spec(rng: &mut Rng) -> WindowSpec {
    let part = if rng.below(3) == 0 { vec![] } else { vec![0] };
    let mk = |rng: &mut Rng, col| OrderKey {
        col,
        desc: rng.below(2) == 0,
        nulls_first: rng.below(2) == 0,
    };
    let order = match rng.below(4) {
        0 => vec![],
        1 | 2 => vec![mk(rng, 1)],
        _ => vec![mk(rng, 1), mk(rng, 2)],
    };
    let frame = if rng.below(3) == 0 {
        None
    } else {
        let range = rng.below(2) == 0 && !order.is_empty();
        let off = |rng: &mut Rng| rng.below(4) as i64;
        let (start, end) = if range && order.len() > 1 {
            // multi-column RANGE only allows UNBOUNDED / CURRENT ROW bounds
            (
                if rng.below(2) == 0 {
                    Bound::UnboundedPreceding
                } else {
                    Bound::CurrentRow
                },
                if rng.below(2) == 0 {
                    Bound::CurrentRow
                } else {
                    Bound::UnboundedFollowing
                },
            )
        } else {
            (
                [
                    Bound::UnboundedPreceding,
                    Bound::Preceding(off(rng)),
                    Bound::CurrentRow,
                    Bound::Following(off(rng)),
                ][rng.below(4) as usize],
                [
                    Bound::Preceding(off(rng)),
                    Bound::CurrentRow,
                    Bound::Following(off(rng)),
                    Bound::UnboundedFollowing,
                ][rng.below(4) as usize],
            )
        };
        Some(Frame {
            mode: if range { Mode::Range } else { Mode::Rows },
            start,
            end,
        })
    };
    WindowSpec { part, order, frame }
}

const ALL_FNS: [WFn; 14] = [
    WFn::RowNumber,
    WFn::Rank,
    WFn::DenseRank,
    WFn::Ntile(3),
    WFn::Lag(3, 1, None),
    WFn::Lead(3, 2, Some(-1)),
    WFn::FirstValue(3),
    WFn::LastValue(3),
    WFn::Sum(3),
    WFn::Avg(3),
    WFn::Min(3),
    WFn::Max(3),
    WFn::Count(3),
    WFn::Ntile(1),
];

fn window_test() {
    let mut rng = Rng(0x7A13_D0A5_0724_0001);
    let mut checks = 0;
    for t in 0..500 {
        let n = [0usize, 1, 2, 5, 12, 30][t % 6] + rng.below(8) as usize;
        let rows = random_table(&mut rng, n);
        for _ in 0..12 {
            let spec = random_spec(&mut rng);
            for f in ALL_FNS {
                assert_eq!(
                    window_fast(&rows, &spec, &f),
                    window_naive(&rows, &spec, &f),
                    "table {t} n={n} {f:?} {spec:?}\nrows={rows:?}"
                );
                checks += 1;
            }
        }
    }
    println!(
        "500 random tables x 12 random window specs x 14 functions: efficient == naive ({checks} comparisons)"
    );
}

fn window_demo() {
    // Sales per region and day. SQL:
    //   SELECT day, amount, ROW_NUMBER() OVER w, RANK() OVER w, DENSE_RANK() OVER w,
    //          SUM(amount) OVER (w ROWS BETWEEN 1 PRECEDING AND CURRENT ROW), LAST_VALUE(amount) OVER w
    //   FROM sales WINDOW w AS (PARTITION BY region ORDER BY amount DESC)
    let rows: Vec<Row> = [(1, 50), (1, 70), (1, 70), (1, 20), (2, 10), (2, 30)]
        .iter()
        .map(|&(r, a)| vec![Some(r), Some(a)])
        .collect();
    let spec = WindowSpec {
        part: vec![0],
        order: vec![OrderKey {
            col: 1,
            desc: true,
            nulls_first: false,
        }],
        frame: None,
    };
    let sliding = WindowSpec {
        frame: Some(Frame {
            mode: Mode::Rows,
            start: Bound::Preceding(1),
            end: Bound::CurrentRow,
        }),
        ..spec.clone()
    };
    let whole = WindowSpec {
        frame: Some(Frame {
            mode: Mode::Rows,
            start: Bound::UnboundedPreceding,
            end: Bound::UnboundedFollowing,
        }),
        ..spec.clone()
    };
    let show = |v: Vec<Option<f64>>| {
        v.iter()
            .map(|x| x.map_or("NULL".to_string(), |f| format!("{f}")))
            .collect::<Vec<_>>()
            .join(" ")
    };
    println!(
        "region,amount rows: {:?}",
        rows.iter()
            .map(|r| (r[0].unwrap(), r[1].unwrap()))
            .collect::<Vec<_>>()
    );
    println!(
        "ROW_NUMBER           : {}",
        show(window_fast(&rows, &spec, &WFn::RowNumber))
    );
    println!(
        "RANK                 : {}",
        show(window_fast(&rows, &spec, &WFn::Rank))
    );
    println!(
        "DENSE_RANK           : {}",
        show(window_fast(&rows, &spec, &WFn::DenseRank))
    );
    println!(
        "SUM ROWS 1 PRECEDING : {}",
        show(window_fast(&rows, &sliding, &WFn::Sum(1)))
    );
    println!(
        "LAST_VALUE (default) : {}   <- last PEER, not last row",
        show(window_fast(&rows, &spec, &WFn::LastValue(1)))
    );
    println!(
        "LAST_VALUE (whole)   : {}",
        show(window_fast(&rows, &whole, &WFn::LastValue(1)))
    );
    let f = |v: &[f64]| v.iter().map(|&x| Some(x)).collect::<Vec<_>>();
    assert_eq!(
        window_fast(&rows, &spec, &WFn::RowNumber),
        f(&[3., 1., 2., 4., 2., 1.])
    );
    assert_eq!(
        window_fast(&rows, &spec, &WFn::Rank),
        f(&[3., 1., 1., 4., 2., 1.])
    );
    assert_eq!(
        window_fast(&rows, &spec, &WFn::DenseRank),
        f(&[2., 1., 1., 3., 2., 1.])
    );
    assert_eq!(
        window_fast(&rows, &spec, &WFn::LastValue(1)),
        f(&[50., 70., 70., 20., 10., 30.]),
        "default frame stops at the last peer: each row sees itself"
    );
    assert_eq!(
        window_fast(&rows, &whole, &WFn::LastValue(1)),
        f(&[20., 20., 20., 20., 10., 10.])
    );
}

fn window_speed() {
    let mut rng = Rng(0x5EED_0724_0002_5EED);
    let n = 120_000;
    let w = 1000i64;
    let rows: Vec<Row> = (0..n)
        .map(|i| {
            vec![
                Some((i / 12_000) as i64),
                Some(i as i64),
                Some(rng.below(1_000_000) as i64),
            ]
        })
        .collect();
    let spec = WindowSpec {
        part: vec![0],
        order: vec![OrderKey {
            col: 1,
            desc: false,
            nulls_first: false,
        }],
        frame: Some(Frame {
            mode: Mode::Rows,
            start: Bound::Preceding(w - 1),
            end: Bound::CurrentRow,
        }),
    };
    println!(
        "\nsliding MIN over ROWS {} PRECEDING, {n} rows in 10 partitions:",
        w - 1
    );
    let t = Instant::now();
    let fast = window_fast(&rows, &spec, &WFn::Min(2));
    let t_fast = t.elapsed().as_secs_f64();
    // Naive per-frame rescan on the already sorted layout (rows are generated in partition/order order).
    let t = Instant::now();
    let mut naive = vec![None; n];
    for i in 0..n {
        let part_start = (i / 12_000) * 12_000;
        let lo = part_start.max(i.saturating_sub(w as usize - 1));
        naive[i] = (lo..=i).filter_map(|j| rows[j][2]).min().map(|v| v as f64);
    }
    let t_naive = t.elapsed().as_secs_f64();
    assert_eq!(fast, naive);
    let t = Instant::now();
    let sum = window_fast(
        &rows,
        &WindowSpec {
            frame: spec.frame,
            ..spec.clone()
        },
        &WFn::Sum(2),
    );
    println!(
        "  segment tree {:>8.1} ms   naive rescan {:>8.1} ms   (prefix-sum SUM: {:.1} ms, checksum {:.0})",
        t_fast * 1e3,
        t_naive * 1e3,
        t.elapsed().as_secs_f64() * 1e3,
        sum.iter().flatten().sum::<f64>()
    );
}

// ================================================================ CTEs
type IRow = Vec<i64>;

#[derive(Clone, Debug)]
enum Query {
    Scan(String),
    FilterGt(Box<Query>, usize, i64), // WHERE col > k
    Project(Box<Query>, Vec<usize>),
    JoinEq(Box<Query>, Box<Query>, usize, usize), // l JOIN r ON l.c1 = r.c2, output = l cols ++ r cols
    UnionAll(Box<Query>, Box<Query>),
    Distinct(Box<Query>),
    SumBy(Box<Query>, usize, usize), // SELECT g, SUM(v) ... GROUP BY g
}
#[derive(Default)]
struct Env {
    tables: HashMap<String, Vec<IRow>>,
    ctes: HashMap<String, Vec<IRow>>, // materialised CTE results
    inline: HashMap<String, Query>,   // CTE definitions to re-evaluate at every reference
    base_scans: usize,                // how often a real table was read: shows materialisation
}
fn eval_query(q: &Query, env: &mut Env) -> Vec<IRow> {
    match q {
        Query::Scan(n) => {
            if let Some(r) = env.ctes.get(n) {
                return r.clone();
            }
            if let Some(def) = env.inline.get(n).cloned() {
                return eval_query(&def, env);
            }
            env.base_scans += 1;
            env.tables[n].clone()
        }
        Query::FilterGt(q, c, k) => eval_query(q, env)
            .into_iter()
            .filter(|r| r[*c] > *k)
            .collect(),
        Query::Project(q, cs) => eval_query(q, env)
            .into_iter()
            .map(|r| cs.iter().map(|&c| r[c]).collect())
            .collect(),
        Query::JoinEq(l, r, lc, rc) => {
            let (l, r) = (eval_query(l, env), eval_query(r, env));
            let mut idx: HashMap<i64, Vec<&IRow>> = HashMap::new();
            for y in &r {
                idx.entry(y[*rc]).or_default().push(y);
            }
            l.iter()
                .flat_map(|x| {
                    idx.get(&x[*lc])
                        .into_iter()
                        .flatten()
                        .map(move |y| x.iter().chain(y.iter()).copied().collect())
                })
                .collect()
        }
        Query::UnionAll(a, b) => {
            let mut v = eval_query(a, env);
            v.extend(eval_query(b, env));
            v
        }
        Query::Distinct(q) => {
            let mut seen = HashSet::new();
            eval_query(q, env)
                .into_iter()
                .filter(|r| seen.insert(r.clone()))
                .collect()
        }
        Query::SumBy(q, g, v) => {
            let mut m: BTreeMap<i64, i64> = BTreeMap::new();
            for r in eval_query(q, env) {
                *m.entry(r[*g]).or_insert(0) += r[*v];
            }
            m.into_iter().map(|(k, s)| vec![k, s]).collect()
        }
    }
}
/// WITH name AS (def), ... main. `materialize` = evaluate each CTE once (PostgreSQL's MATERIALIZED); otherwise inline per reference.
fn with_ctes(defs: &[(&str, Query)], main: &Query, env: &mut Env, materialize: bool) -> Vec<IRow> {
    for (name, def) in defs {
        if materialize {
            let r = eval_query(def, env); // may reference earlier CTEs
            env.ctes.insert(name.to_string(), r);
        } else {
            env.inline.insert(name.to_string(), def.clone());
        }
    }
    let r = eval_query(main, env);
    env.ctes.clear();
    env.inline.clear();
    r
}

fn cte_test() {
    let mut rng = Rng(0xC7E0_0724_0003_1111);
    let (mut mat_scans, mut inl_scans) = (0, 0);
    for _ in 0..100 {
        let rows: Vec<IRow> = (0..rng.below(30))
            .map(|i| vec![i as i64, rng.below(4) as i64, rng.below(10) as i64])
            .collect();
        // WITH big AS (SELECT * FROM t WHERE v > 4)
        // SELECT a.id, b.id FROM big a JOIN big b ON a.g = b.g          -- the CTE is referenced twice
        let defs = [(
            "big",
            Query::FilterGt(Box::new(Query::Scan("t".into())), 2, 4),
        )];
        let main = Query::Project(
            Box::new(Query::JoinEq(
                Box::new(Query::Scan("big".into())),
                Box::new(Query::Scan("big".into())),
                1,
                1,
            )),
            vec![0, 3],
        );
        let mut env = Env::default();
        env.tables.insert("t".into(), rows.clone());
        let mut a = with_ctes(&defs, &main, &mut env, true);
        mat_scans += env.base_scans;
        env.base_scans = 0;
        let mut b = with_ctes(&defs, &main, &mut env, false);
        inl_scans += env.base_scans;
        let mut want: Vec<IRow> = vec![];
        for x in rows.iter().filter(|r| r[2] > 4) {
            for y in rows.iter().filter(|r| r[2] > 4 && r[1] == x[1]) {
                want.push(vec![x[0], y[0]]);
            }
        }
        (a.sort(), b.sort(), want.sort());
        assert_eq!(a, want);
        assert_eq!(b, want);
    }
    println!(
        "non-recursive CTE referenced twice: materialised == inlined == oracle on 100 tables; base-table scans {mat_scans} (materialised) vs {inl_scans} (inlined)"
    );
    assert_eq!(inl_scans, 2 * mat_scans);
}

// ================================================================ recursive CTE
#[derive(Default, Debug, Clone, Copy)]
struct FixStats {
    iterations: usize,
    step_input_rows: usize, // total rows handed to the recursive term: the work measure semi-naive reduces
}
/// WITH RECURSIVE t AS (base UNION [ALL] step(t)). `step` sees the delta (semi-naive) or all of t (naive).
fn recursive_cte(
    base: Vec<IRow>,
    step: &dyn Fn(&[IRow]) -> Vec<IRow>,
    distinct: bool,
    semi_naive: bool,
    max_iter: usize,
) -> Result<(Vec<IRow>, FixStats), String> {
    let mut st = FixStats::default();
    if !distinct {
        assert!(semi_naive, "UNION ALL is only defined through the delta");
        let (mut all, mut delta) = (base.clone(), base);
        while !delta.is_empty() {
            st.iterations += 1;
            if st.iterations > max_iter {
                return Err(format!(
                    "UNION ALL did not converge within {max_iter} iterations (cycle without a terminating condition?)"
                ));
            }
            st.step_input_rows += delta.len();
            delta = step(&delta);
            all.extend(delta.iter().cloned());
        }
        return Ok((all, st));
    }
    let mut seen: HashSet<IRow> = HashSet::new();
    let mut all: Vec<IRow> = vec![];
    let mut delta: Vec<IRow> = base
        .into_iter()
        .filter(|r| seen.insert(r.clone()))
        .collect();
    all.extend(delta.iter().cloned());
    while !delta.is_empty() {
        st.iterations += 1;
        let input: &[IRow] = if semi_naive { &delta } else { &all };
        st.step_input_rows += input.len();
        let new = step(input);
        delta = new.into_iter().filter(|r| seen.insert(r.clone())).collect(); // keep only rows not derived before
        all.extend(delta.iter().cloned());
    }
    Ok((all, st))
}

fn closure_oracle(n: usize, edges: &[(usize, usize)]) -> HashSet<IRow> {
    let mut reach = vec![vec![false; n]; n];
    for &(a, b) in edges {
        reach[a][b] = true;
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                if reach[i][k] && reach[k][j] {
                    reach[i][j] = true;
                }
            }
        }
    }
    let reach = &reach;
    (0..n)
        .flat_map(|i| {
            (0..n)
                .filter(move |&j| reach[i][j])
                .map(move |j| vec![i as i64, j as i64])
        })
        .collect()
}

fn recursive_test() {
    let mut rng = Rng(0x2EC0_0724_0004_2222);
    let (mut naive_work, mut semi_work) = (0usize, 0usize);
    // 1. Transitive closure on random (cyclic) graphs:
    //   WITH RECURSIVE reach(a,b) AS (SELECT a,b FROM edge UNION SELECT r.a, e.b FROM reach r JOIN edge e ON r.b = e.a)
    for _ in 0..300 {
        let n = 1 + rng.below(10) as usize;
        let edges: Vec<(usize, usize)> = (0..rng.below(25))
            .map(|_| (rng.below(n as u64) as usize, rng.below(n as u64) as usize))
            .collect();
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        for &(a, b) in &edges {
            adj.entry(a as i64).or_default().push(b as i64);
        }
        let step = |delta: &[IRow]| -> Vec<IRow> {
            delta
                .iter()
                .flat_map(|r| {
                    adj.get(&r[1])
                        .into_iter()
                        .flatten()
                        .map(move |&z| vec![r[0], z])
                })
                .collect()
        };
        let base: Vec<IRow> = edges
            .iter()
            .map(|&(a, b)| vec![a as i64, b as i64])
            .collect();
        let (semi, s1) = recursive_cte(base.clone(), &step, true, true, 1000).unwrap();
        let (naive, s2) = recursive_cte(base, &step, true, false, 1000).unwrap();
        let want = closure_oracle(n, &edges);
        assert_eq!(
            semi.iter().cloned().collect::<HashSet<_>>(),
            want,
            "semi-naive closure"
        );
        assert_eq!(semi.len(), want.len(), "UNION must not emit duplicates");
        assert_eq!(
            naive.iter().cloned().collect::<HashSet<_>>(),
            want,
            "naive closure"
        );
        semi_work += s1.step_input_rows;
        naive_work += s2.step_input_rows;
    }
    println!(
        "transitive closure on 300 random cyclic graphs: semi-naive == naive == Warshall; rows fed to the recursive term: semi-naive {semi_work}, naive {naive_work}"
    );
    assert!(semi_work < naive_work);

    // 2. Org chart with UNION ALL: employees (id, mgr); WITH RECURSIVE chain(id, lvl) AS (SELECT 0,0 UNION ALL SELECT e.id, c.lvl+1 FROM emp e JOIN chain c ON e.mgr = c.id)
    for _ in 0..200 {
        let n = 1 + rng.below(40) as usize;
        let mgr: Vec<usize> = (0..n)
            .map(|i| {
                if i == 0 {
                    0
                } else {
                    rng.below(i as u64) as usize
                }
            })
            .collect();
        let mut kids: HashMap<i64, Vec<i64>> = HashMap::new();
        for i in 1..n {
            kids.entry(mgr[i] as i64).or_default().push(i as i64);
        }
        let step = |delta: &[IRow]| -> Vec<IRow> {
            delta
                .iter()
                .flat_map(|r| {
                    kids.get(&r[0])
                        .into_iter()
                        .flatten()
                        .map(move |&k| vec![k, r[1] + 1])
                })
                .collect()
        };
        let (got, _) = recursive_cte(vec![vec![0, 0]], &step, false, true, 1000).unwrap();
        let depth = |mut i: usize| {
            let mut d = 0;
            while i != 0 {
                i = mgr[i];
                d += 1;
            }
            d
        };
        let mut got = got;
        got.sort();
        let want: Vec<IRow> = (0..n).map(|i| vec![i as i64, depth(i) as i64]).collect();
        assert_eq!(got, want, "org chart levels");
    }
    // 3. Number series: WITH RECURSIVE s(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM s WHERE n < 50) SELECT * FROM s
    let step = |delta: &[IRow]| -> Vec<IRow> {
        delta
            .iter()
            .filter(|r| r[0] < 50)
            .map(|r| vec![r[0] + 1])
            .collect()
    };
    let (series, st) = recursive_cte(vec![vec![1]], &step, false, true, 100).unwrap();
    assert_eq!(series, (1..=50).map(|i| vec![i]).collect::<Vec<_>>());
    println!(
        "org chart levels (200 random trees, UNION ALL) == parent-chain oracle; series 1..=50 in {} iterations",
        st.iterations
    );
    // 4. UNION ALL over a cycle never converges: the guard fires.
    let cyc =
        |delta: &[IRow]| -> Vec<IRow> { delta.iter().map(|r| vec![(r[0] + 1) % 3]).collect() };
    let e = recursive_cte(vec![vec![0]], &cyc, false, true, 50).unwrap_err();
    println!("cycle with UNION ALL: {e}");
    let (ok, _) = recursive_cte(vec![vec![0]], &cyc, true, true, 50).unwrap();
    assert_eq!(
        ok.len(),
        3,
        "the same recursion with UNION stops after visiting 3 nodes"
    );
}

// ================================================================ LATERAL / correlated subqueries
fn lateral_test() {
    let mut rng = Rng(0x1A7E_0724_0005_3333);
    let mut corr_runs = 0usize;
    for _ in 0..300 {
        let nc = 1 + rng.below(6) as usize;
        // orders(id, cust, amount); amounts tie often so the tie-break by id matters
        let orders: Vec<IRow> = (0..rng.below(25))
            .map(|i| {
                vec![
                    i as i64,
                    rng.below(nc as u64 + 2) as i64,
                    rng.below(5) as i64,
                ]
            })
            .collect();
        // SELECT c.id, o.id FROM customers c, LATERAL (SELECT id FROM orders o WHERE o.cust = c.id ORDER BY amount DESC, id LIMIT 2) o
        let mut lateral: Vec<(i64, i64)> = vec![];
        for c in 0..nc as i64 {
            corr_runs += 1; // the subquery body runs once per outer row
            let mut inner: Vec<&IRow> = orders.iter().filter(|o| o[1] == c).collect();
            inner.sort_by_key(|o| (std::cmp::Reverse(o[2]), o[0]));
            lateral.extend(inner.iter().take(2).map(|o| (c, o[0])));
        }
        // The same question as a window function: ROW_NUMBER() OVER (PARTITION BY cust ORDER BY amount DESC, id) <= 2
        let rows: Vec<Row> = orders
            .iter()
            .map(|o| o.iter().map(|&v| Some(v)).collect())
            .collect();
        let spec = WindowSpec {
            part: vec![1],
            order: vec![
                OrderKey {
                    col: 2,
                    desc: true,
                    nulls_first: false,
                },
                OrderKey {
                    col: 0,
                    desc: false,
                    nulls_first: false,
                },
            ],
            frame: None,
        };
        let rn = window_fast(&rows, &spec, &WFn::RowNumber);
        let mut via_window: Vec<(i64, i64)> = (0..orders.len())
            .filter(|&i| rn[i].unwrap() <= 2.0 && orders[i][1] < nc as i64)
            .map(|i| (orders[i][1], orders[i][0]))
            .collect();
        lateral.sort();
        via_window.sort();
        assert_eq!(lateral, via_window, "LATERAL top-2 == ROW_NUMBER <= 2");
        // LEFT JOIN LATERAL keeps customers without orders, padded with NULL.
        let with_orders: HashSet<i64> = lateral.iter().map(|p| p.0).collect();
        let left_rows = lateral.len() + (0..nc as i64).filter(|c| !with_orders.contains(c)).count();
        assert!(
            left_rows >= nc,
            "every customer appears at least once in the LEFT lateral join"
        );
        // Correlated scalar subquery (SELECT MAX(amount) FROM orders WHERE cust = c.id) vs decorrelated GROUP BY + join.
        let correlated: Vec<Option<i64>> = (0..nc as i64)
            .map(|c| orders.iter().filter(|o| o[1] == c).map(|o| o[2]).max())
            .collect();
        let mut grouped: HashMap<i64, i64> = HashMap::new(); // one pass over orders instead of one per customer
        for o in &orders {
            let e = grouped.entry(o[1]).or_insert(i64::MIN);
            *e = (*e).max(o[2]);
        }
        let decorrelated: Vec<Option<i64>> =
            (0..nc as i64).map(|c| grouped.get(&c).copied()).collect();
        assert_eq!(correlated, decorrelated);
    }
    println!(
        "LATERAL top-2 per customer == ROW_NUMBER formulation; decorrelated MAX == correlated on 300 instances (correlated body ran {corr_runs}x, decorrelated: 300 passes)"
    );
}

// ================================================================ GROUPING SETS / ROLLUP / CUBE
type GRow = (Vec<Option<i64>>, u32, i64, i64); // (key with None for rolled-up dims, GROUPING bitmask, SUM(m), COUNT(*))

fn rollup(k: usize) -> Vec<Vec<usize>> {
    (0..=k).rev().map(|len| (0..len).collect()).collect() // ROLLUP(d0,d1,d2) = (d0,d1,d2), (d0,d1), (d0), ()
}
fn cube(k: usize) -> Vec<Vec<usize>> {
    (0..1usize << k)
        .map(|m| (0..k).filter(|i| m >> i & 1 == 1).collect())
        .collect()
}
fn gid(set: &[usize], k: usize) -> u32 {
    (0..k)
        .filter(|i| !set.contains(i))
        .fold(0, |g, i| g | 1 << (k - 1 - i)) // GROUPING(d_i) = 1 when d_i is NOT grouped
}

/// Reference: rescan the whole table once per grouping set.
fn grouping_naive(rows: &[Row], k: usize, m: usize, sets: &[Vec<usize>]) -> Vec<GRow> {
    let mut out = vec![];
    for set in sets {
        let mut g: BTreeMap<Vec<Option<i64>>, (i64, i64)> = BTreeMap::new();
        for r in rows {
            let key: Vec<Option<i64>> = (0..k)
                .map(|i| if set.contains(&i) { r[i] } else { None })
                .collect();
            let e = g.entry(key).or_default();
            e.0 += r[m].unwrap();
            e.1 += 1;
        }
        out.extend(g.into_iter().map(|(key, (s, c))| (key, gid(set, k), s, c)));
    }
    out.sort();
    out
}
/// One pass builds the finest grouping; every other set is derived from it (SUM and COUNT are distributive).
fn grouping_fast(rows: &[Row], k: usize, m: usize, sets: &[Vec<usize>]) -> Vec<GRow> {
    let mut finest: BTreeMap<Vec<Option<i64>>, (i64, i64)> = BTreeMap::new();
    for r in rows {
        let e = finest.entry((0..k).map(|i| r[i]).collect()).or_default();
        e.0 += r[m].unwrap();
        e.1 += 1;
    }
    let mut out = vec![];
    for set in sets {
        let mut g: BTreeMap<Vec<Option<i64>>, (i64, i64)> = BTreeMap::new();
        for (key, &(s, c)) in &finest {
            let e = g
                .entry(
                    (0..k)
                        .map(|i| if set.contains(&i) { key[i] } else { None })
                        .collect(),
                )
                .or_default();
            e.0 += s;
            e.1 += c;
        }
        out.extend(g.into_iter().map(|(key, (s, c))| (key, gid(set, k), s, c)));
    }
    out.sort();
    out
}

fn grouping_test() {
    let mut rng = Rng(0x6209_0724_0006_4444);
    let mut null_data_groups = 0;
    for _ in 0..200 {
        let rows: Vec<Row> = (0..rng.below(40))
            .map(|_| {
                vec![
                    if rng.below(5) == 0 {
                        None
                    } else {
                        Some(rng.below(3) as i64)
                    },
                    Some(rng.below(3) as i64),
                    Some(rng.below(2) as i64),
                    Some(rng.below(100) as i64),
                ]
            })
            .collect();
        for sets in [
            rollup(3),
            cube(3),
            vec![vec![0], vec![1], vec![]],
            vec![vec![0, 2], vec![1, 2]],
        ] {
            let (a, b) = (
                grouping_fast(&rows, 3, 3, &sets),
                grouping_naive(&rows, 3, 3, &sets),
            );
            assert_eq!(a, b, "sets {sets:?}");
        }
        let r = grouping_fast(&rows, 3, 3, &rollup(3));
        if !rows.is_empty() {
            let total: i64 = rows.iter().map(|r| r[3].unwrap()).sum();
            let grand: Vec<&GRow> = r.iter().filter(|g| g.1 == 0b111).collect();
            assert_eq!(grand.len(), 1, "ROLLUP always ends in the grand total row");
            assert_eq!((grand[0].2, grand[0].3), (total, rows.len() as i64));
        }
        // A NULL in a grouped column with GROUPING bit 0 is DATA; with bit 1 it is a subtotal marker.
        null_data_groups += r
            .iter()
            .filter(|g| g.1 & 0b100 == 0 && g.0[0].is_none())
            .count();
    }
    assert_eq!(cube(4).len(), 16);
    assert_eq!(rollup(4).len(), 5);
    println!(
        "ROLLUP/CUBE/GROUPING SETS: one-pass derivation == per-set rescans on 200 tables ({null_data_groups} groups where a real NULL is told apart from a subtotal NULL by GROUPING())"
    );
}

fn grouping_demo() {
    // SELECT region, product, SUM(sales) FROM s GROUP BY ROLLUP(region, product)
    let rows: Vec<Row> = [(1, 1, 10), (1, 2, 20), (2, 1, 5)]
        .iter()
        .map(|&(r, p, v)| vec![Some(r), Some(p), Some(v)])
        .collect();
    println!(
        "\nROLLUP(region, product) over {} rows (key = region,product; NULL = rolled up; g = GROUPING bits):",
        rows.len()
    );
    for (key, g, s, c) in grouping_fast(&rows, 2, 2, &rollup(2)) {
        let f = |v: Option<i64>| v.map_or("NULL".to_string(), |x| x.to_string());
        println!(
            "  ({:>4}, {:>4})  g={g:02b}  sum={s:<3} count={c}",
            f(key[0]),
            f(key[1])
        );
    }
}

fn main() {
    window_demo();
    window_test();
    window_speed();
    cte_test();
    recursive_test();
    lateral_test();
    grouping_demo();
    grouping_test();
    println!("ok");
}
