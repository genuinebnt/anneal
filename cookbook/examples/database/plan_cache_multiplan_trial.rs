//! RECIPE 765 (database): plan cache, plan baselines, MongoDB-style multi-plan trial, UDF optimisation and hints.
//! LEVEL: advanced
//! Use in: query engines with prepared/ad-hoc statement caches, plan-regression tooling, document databases, engines with user-defined functions.   Run: cargo run --release --example plan_cache_multiplan_trial
//! COMMON USES: PostgreSQL / MySQL / SQL Server statement-plan caches keyed by normalised text (literals replaced by parameters), SQL Server Query Store
//! plan forcing and automatic plan correction, Oracle SQL plan baselines and SQL profiles, MongoDB's plan cache with a multi-planner trial
//! (candidate plans race for a bounded number of works; the winner is cached and evicted when it later underperforms), optimizer hints
//! (`INDEX`, `LEADING`, `USE_HASH`) in Oracle, MySQL and SQL Server, inlining of scalar UDFs (Froid in SQL Server 2019) and batching of
//! UDF calls in analytics engines.
//! Course: CMU 15-799 Query Optimization (Spring 2025), lectures 19-23 (UDF optimizations; Calcite, Orca, MongoDB and Fabric implementations)
#![allow(dead_code)]
//!
//! WHAT: the production plumbing around an optimizer, on a two-table engine whose executor counts work units (sequential row 1, index entry fetch 4,
//! index descent 2, hash build 1.5 per row, probe 1, UDF call 20). Every number below is EXECUTED work.
//! * PLAN CACHE: statements are normalised (literals -> `?`, conjuncts sorted, IN lists collapsed) into a fingerprint; the cache maps fingerprint ->
//!   plan optimised for the FIRST literals seen ("parameter sniffing"). Entries record the schema and statistics version of the tables they read, and are
//!   invalidated exactly when one of those changes (create/drop index, ANALYZE), not when an unrelated table changes.
//! * PLAN BASELINES: after a statistics refresh the next plan may be WORSE than the previous one (a refresh from a biased sample is simulated). On
//!   the first execution after invalidation the engine runs the candidate and the previous (baseline) plan, keeps the cheaper, and counts rollbacks.
//! * MULTI-PLAN TRIAL (MongoDB): candidate plans (collection scan, each usable index) run interleaved in quanta of work; the first to produce
//!   100 results or reach EOF wins; if the budget (29% of the collection) runs out, the best results-per-work (or progress) wins. The winner
//!   continues where it stopped, so no work is repeated. No statistics are consulted, so a bad ANALYZE cannot mislead it.
//! * UDF OPTIMISATION: a pure scalar UDF is (1) called opaquely per row, (2) INLINED by substituting the argument into the body and constant
//!   folding (same semantics, no call overhead), (3) evaluated in batches of 1024 (amortised call overhead) or (4) memoised by argument value;
//!   the cost model carries a per-call COST ANNOTATION so that conjuncts are ordered by rank (1 - sel) / cost and the expensive UDF runs last.
//! * HINTS: `/*+ SEQ */`, `/*+ INDEX(col) */`, `/*+ BUILD(u) */` override the cost-based choice (and are part of the fingerprint); an impossible hint
//!   falls back to the optimizer instead of failing.
//!
//! WHY: optimisation is expensive, plans are reused, and statistics change; the engineering question is when a reused plan stops being valid and how to
//! notice a regression. A trial trades a bounded amount of duplicate work for independence from estimates.
//!
//! CHECKS (asserted): fingerprints equal for literal/conjunct-order variants and differ otherwise; cached execution equals fresh optimisation on 300 random queries;
//! scripted scenarios invalidate exactly the dependent entries; baselines never cost more than plain caching over a bad-ANALYZE window and
//! roll back at least once; the trial's work stays within 3x of the hindsight-best plan on average over 500 workloads and beats the stale-statistics
//! cost-based plans in total; every UDF strategy returns identical results, inlining equals calling on 300 random UDF bodies; hints change the plan, not the answer.
//! Read: Implementation: MongoDB / Calcite / Orca / Fabric, CMU 15-799 lectures 19-23 https://15799.courses.cs.cmu.edu/spring2025/schedule.html
//!       First Past the Post: Evaluating Query Optimization in MongoDB (Tao et al., ADC 2024)
//!       ROME: Robust Query Optimization via Parallel Multi-Plan Execution (Wei et al., SIGMOD 2024)
//!       Froid: Optimization of Imperative Programs in a Relational Database (Ramachandra et al., VLDB 2017)
//!       Dear User-Defined Functions, Inlining isn't working out so great for us. Let's try batching to make our relationship work. (Franz et al., CIDR 2024)
//!       PostgreSQL PREPARE https://www.postgresql.org/docs/current/sql-prepare.html

use std::collections::HashMap;

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
    fn log_uniform(&mut self, lo: f64, hi: f64) -> f64 {
        (lo.ln() + self.unit() * (hi.ln() - lo.ln())).exp()
    }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() - 1) as f64 * p).round() as usize]
}

// ---------------------------------------------------------------------------------------------------------------------
// Engine: tables T (40000 rows) and U (3000 rows), columns a, b, c; optional sorted indexes on a and b
// ---------------------------------------------------------------------------------------------------------------------

const NT: usize = 40_000;
const NU: usize = 3_000;
const T: usize = 0;
const U: usize = 1;
const COLS: [&str; 3] = ["a", "b", "c"];

struct Table {
    col: [Vec<i64>; 3],
    idx: [Option<Vec<(i64, u32)>>; 3],
}

impl Table {
    fn rows(&self) -> usize {
        self.col[0].len()
    }
    fn build_index(&mut self, c: usize) {
        let mut v: Vec<(i64, u32)> = self.col[c]
            .iter()
            .enumerate()
            .map(|(i, &x)| (x, i as u32))
            .collect();
        v.sort();
        self.idx[c] = Some(v);
    }
}

struct Db {
    t: [Table; 2],
    schema_ver: [u64; 2],
    stats_ver: [u64; 2],
    stats: [Stats; 2],
}

/// Statistics = a sorted sample of every column (what a histogram is built from).
#[derive(Clone)]
struct Stats {
    sample: [Vec<i64>; 3],
}

#[derive(Clone, Copy)]
enum Sample {
    Good, // every 8th row: representative
    Bad,  // the first 400 rows only: a biased sample, since `a` grows with the insertion order
}

fn analyze(t: &Table, how: Sample) -> Stats {
    let rows: Vec<usize> = match how {
        Sample::Good => (0..t.rows()).step_by(8).collect(),
        Sample::Bad => (0..400.min(t.rows())).collect(),
    };
    let mut s: [Vec<i64>; 3] = Default::default();
    for c in 0..3 {
        s[c] = rows.iter().map(|&r| t.col[c][r]).collect();
        s[c].sort();
    }
    Stats { sample: s }
}

fn gen_db(seed: u64) -> Db {
    let mut r = Rng(seed);
    let t = Table {
        col: [
            (0..NT as i64).map(|i| i / 4).collect(),
            (0..NT).map(|_| r.below(1000) as i64).collect(),
            (0..NT).map(|_| r.below(100) as i64).collect(),
        ],
        idx: [None, None, None],
    };
    let u = Table {
        col: [
            (0..NU).map(|_| r.below(10_000) as i64).collect(),
            (0..NU).map(|_| r.below(1000) as i64).collect(),
            (0..NU).map(|_| r.below(100) as i64).collect(),
        ],
        idx: [None, None, None],
    };
    let mut db = Db {
        stats: [analyze(&t, Sample::Good), analyze(&u, Sample::Good)],
        t: [t, u],
        schema_ver: [0; 2],
        stats_ver: [0; 2],
    };
    db.create_index(T, 0);
    db.create_index(T, 1);
    db
}

impl Db {
    fn create_index(&mut self, t: usize, c: usize) {
        self.t[t].build_index(c);
        self.schema_ver[t] += 1; // schema change: every plan on this table may now have a better (or invalid) access path
    }
    fn drop_index(&mut self, t: usize, c: usize) {
        self.t[t].idx[c] = None;
        self.schema_ver[t] += 1;
    }
    fn analyze(&mut self, t: usize, how: Sample) {
        self.stats[t] = analyze(&self.t[t], how);
        self.stats_ver[t] += 1;
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Queries, SQL text and fingerprints
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
struct Pred {
    c: usize,
    lo: i64,
    hi: i64,
}

#[derive(Clone, Debug, PartialEq)]
enum Hint {
    Seq,
    Index(usize),
    Build(usize), // table to build the hash join on
}

#[derive(Clone, Debug)]
struct Query {
    preds: Vec<Pred>,
    join: bool, // join U on t.c = u.c and count the pairs
    hints: Vec<Hint>,
}

fn sql(q: &Query) -> String {
    let hint = q
        .hints
        .iter()
        .map(|h| match h {
            Hint::Seq => "SEQ".to_string(),
            Hint::Index(c) => format!("INDEX({})", COLS[*c]),
            Hint::Build(t) => format!("BUILD({})", if *t == T { "t" } else { "u" }),
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut conj: Vec<String> = vec![];
    for p in &q.preds {
        if p.lo == p.hi {
            conj.push(format!("{} = {}", COLS[p.c], p.lo));
        } else {
            conj.push(format!("{} >= {}", COLS[p.c], p.lo));
            conj.push(format!("{} <= {}", COLS[p.c], p.hi));
        }
    }
    format!(
        "SELECT {}count(*) FROM t{} {}",
        if hint.is_empty() {
            String::new()
        } else {
            format!("/*+ {hint} */ ")
        },
        if q.join { " JOIN u ON t.c = u.c" } else { "" },
        if conj.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conj.join(" AND "))
        }
    )
}

/// Normalise: lowercase, literals -> ?, conjuncts sorted, IN lists collapsed, whitespace squeezed. Hints stay: they select a different plan.
fn fingerprint(sql: &str) -> String {
    let s = sql.to_lowercase();
    let (head, tail) = match s.find(" where ") {
        Some(i) => (&s[..i], &s[i + 7..]),
        None => (&s[..], ""),
    };
    let strip = |t: &str| -> String {
        let mut out = String::new();
        let mut in_num = false;
        let mut in_list = false;
        for ch in t.chars() {
            if in_list {
                if ch == ')' {
                    in_list = false;
                    out.push_str("(?)");
                }
                continue;
            }
            if ch.is_ascii_digit() || (ch == '-' && !in_num && out.ends_with(' ')) {
                if !in_num {
                    out.push('?');
                }
                in_num = true;
                continue;
            }
            in_num = false;
            if out.ends_with(" in ") && ch == '(' {
                in_list = true;
                continue;
            }
            out.push(ch);
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let mut conj: Vec<String> = tail
        .split(" and ")
        .filter(|c| !c.trim().is_empty())
        .map(|c| strip(c))
        .collect();
    conj.sort();
    format!("{} where {}", strip(head), conj.join(" and "))
}

// ---------------------------------------------------------------------------------------------------------------------
// Optimizer and executor
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
enum Access {
    Seq,
    Idx(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Plan {
    access: Access,
    build: usize, // join queries: which side the hash table is built on
}

fn est_sel(st: &Stats, p: &Pred) -> f64 {
    let s = &st.sample[p.c];
    (s.partition_point(|&v| v <= p.hi) - s.partition_point(|&v| v < p.lo)) as f64 / s.len() as f64
}

fn candidates(db: &Db, q: &Query) -> Vec<Access> {
    let mut v = vec![Access::Seq];
    for p in &q.preds {
        if db.t[T].idx[p.c].is_some() && !v.contains(&Access::Idx(p.c)) {
            v.push(Access::Idx(p.c));
        }
    }
    v
}

/// Cost-based choice with the CURRENT statistics; returns the plan, its estimated cost and whether a hint could not be honoured.
fn optimise(db: &Db, q: &Query) -> (Plan, f64, bool) {
    let st = &db.stats[T];
    let rows_est: f64 = NT as f64 * q.preds.iter().map(|p| est_sel(st, p)).product::<f64>();
    let access_cost = |a: Access| match a {
        Access::Seq => NT as f64,
        Access::Idx(c) => {
            2.0 + 4.0
                * NT as f64
                * q.preds
                    .iter()
                    .filter(|p| p.c == c)
                    .map(|p| est_sel(st, p))
                    .product::<f64>()
        }
    };
    let mut ignored = false;
    let mut cands = candidates(db, q);
    for h in &q.hints {
        match h {
            Hint::Seq => cands = vec![Access::Seq],
            Hint::Index(c) if cands.contains(&Access::Idx(*c)) => cands = vec![Access::Idx(*c)],
            Hint::Index(_) => ignored = true, // hinted index missing or not applicable: fall back, never fail the query
            Hint::Build(_) => {}
        }
    }
    let access = *cands
        .iter()
        .min_by(|&&a, &&b| access_cost(a).partial_cmp(&access_cost(b)).unwrap())
        .unwrap();
    let hash = |build: usize| {
        if build == T {
            1.5 * rows_est + NU as f64
        } else {
            1.5 * NU as f64 + rows_est
        }
    };
    let mut build = if hash(T) <= hash(U) { T } else { U };
    if let Some(Hint::Build(t)) = q.hints.iter().find(|h| matches!(h, Hint::Build(_))) {
        build = *t;
    }
    let cost = access_cost(access) + if q.join { hash(build) } else { 0.0 };
    (Plan { access, build }, cost, ignored)
}

fn matches(db: &Db, q: &Query, row: usize) -> bool {
    q.preds
        .iter()
        .all(|p| (p.lo..=p.hi).contains(&db.t[T].col[p.c][row]))
}

/// Execute a plan completely: (result count, work).
fn execute(db: &Db, q: &Query, plan: Plan) -> (u64, f64) {
    let (mut work, mut rows) = (0.0, vec![]);
    match plan.access {
        Access::Seq => {
            for r in 0..NT {
                work += 1.0;
                if matches(db, q, r) {
                    rows.push(r);
                }
            }
        }
        Access::Idx(c) => {
            let idx = db.t[T].idx[c].as_ref().expect("plan uses a dropped index");
            let p = q.preds.iter().find(|p| p.c == c).unwrap();
            work += 2.0;
            let (lo, hi) = (
                idx.partition_point(|e| e.0 < p.lo),
                idx.partition_point(|e| e.0 <= p.hi),
            );
            for &(_, r) in &idx[lo..hi] {
                work += 4.0;
                if matches(db, q, r as usize) {
                    rows.push(r as usize);
                }
            }
        }
    }
    if !q.join {
        return (rows.len() as u64, work);
    }
    let count = if plan.build == T {
        work += 1.5 * rows.len() as f64 + NU as f64;
        let mut ht: HashMap<i64, u64> = HashMap::new();
        rows.iter()
            .for_each(|&r| *ht.entry(db.t[T].col[2][r]).or_default() += 1);
        db.t[U].col[2]
            .iter()
            .map(|c| ht.get(c).copied().unwrap_or(0))
            .sum()
    } else {
        work += 1.5 * NU as f64 + rows.len() as f64;
        let mut ht: HashMap<i64, u64> = HashMap::new();
        db.t[U].col[2]
            .iter()
            .for_each(|&c| *ht.entry(c).or_default() += 1);
        rows.iter()
            .map(|&r| ht.get(&db.t[T].col[2][r]).copied().unwrap_or(0))
            .sum()
    };
    (count, work)
}

// ---------------------------------------------------------------------------------------------------------------------
// Plan cache
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct CacheStats {
    hits: usize,
    misses: usize,
    invalidated: usize,
}

struct Entry {
    plan: Plan,
    versions: ([u64; 2], [u64; 2]), // (schema, stats) of both tables at planning time; compared only for the tables the query reads
    tables: Vec<usize>,
}

#[derive(Default)]
struct Cache {
    map: HashMap<String, Entry>,
    st: CacheStats,
}

impl Cache {
    fn valid(db: &Db, e: &Entry) -> bool {
        e.tables
            .iter()
            .all(|&t| e.versions.0[t] == db.schema_ver[t] && e.versions.1[t] == db.stats_ver[t])
    }
    /// Returns (plan, was_hit).
    fn get_plan(&mut self, db: &Db, q: &Query) -> (Plan, bool) {
        let fp = fingerprint(&sql(q));
        if let Some(e) = self.map.get(&fp) {
            if Self::valid(db, e) {
                self.st.hits += 1;
                return (e.plan, true);
            }
            self.st.invalidated += 1;
        }
        self.st.misses += 1;
        let plan = optimise(db, q).0;
        self.map.insert(
            fp,
            Entry {
                plan,
                versions: (db.schema_ver, db.stats_ver),
                tables: if q.join { vec![T, U] } else { vec![T] },
            },
        );
        (plan, false)
    }
}

fn random_query(r: &mut Rng, join_chance: f64) -> Query {
    let mut preds = vec![];
    let ncols = 1 + r.below(2) as usize;
    for c in [0usize, 1, 2].iter().take(3).filter(|_| true) {
        if preds.len() < ncols && r.unit() < 0.7 {
            let (dom, wmax) = [(10_000.0, 10_000.0), (1000.0, 1000.0), (100.0, 100.0)][*c];
            let width = if r.unit() < 0.3 {
                0.0
            } else {
                r.log_uniform(1.0, wmax)
            };
            let lo = r.below((dom - width) as u64) as i64;
            preds.push(Pred {
                c: *c,
                lo,
                hi: lo + width as i64,
            });
        }
    }
    if preds.is_empty() {
        preds.push(Pred {
            c: 1,
            lo: 5,
            hi: 5 + r.below(100) as i64,
        });
    }
    Query {
        preds,
        join: r.unit() < join_chance,
        hints: vec![],
    }
}

fn section_fingerprints() {
    println!("== 1. normalisation: fingerprints ==");
    let fp = |s: &str| fingerprint(s);
    let base = fp("SELECT count(*) FROM t WHERE a >= 10 AND b = 5");
    println!("  {base}");
    assert_eq!(
        base,
        fp("select  count(*) from t where b = 99 and a >= 3"),
        "literals and conjunct order must not matter"
    );
    assert_ne!(
        base,
        fp("SELECT count(*) FROM t WHERE a > 10 AND b = 5"),
        "different operator = different statement"
    );
    assert_ne!(
        base,
        fp("SELECT count(*) FROM t WHERE a >= 10 AND c = 5"),
        "different column = different statement"
    );
    assert_eq!(
        fp("SELECT count(*) FROM t WHERE a IN (1, 2, 3)"),
        fp("SELECT count(*) FROM t WHERE a IN (7)"),
        "IN lists of different length share a plan"
    );
    assert_ne!(
        base,
        fp("SELECT /*+ SEQ */ count(*) FROM t WHERE a >= 10 AND b = 5"),
        "a hint selects a different plan"
    );
    let q1 = Query {
        preds: vec![
            Pred {
                c: 0,
                lo: 10,
                hi: 20,
            },
            Pred { c: 1, lo: 5, hi: 5 },
        ],
        join: false,
        hints: vec![],
    };
    let q2 = Query {
        preds: vec![
            Pred {
                c: 1,
                lo: 77,
                hi: 77,
            },
            Pred {
                c: 0,
                lo: 1,
                hi: 400,
            },
        ],
        join: false,
        hints: vec![],
    };
    assert_eq!(fingerprint(&sql(&q1)), fingerprint(&sql(&q2)));
    println!(
        "  literal / conjunct-order / whitespace variants collapse to one fingerprint; operators, columns and hints do not"
    );
}

fn section_cache_correctness(db: &Db) {
    println!("\n== 2. cached plans vs fresh optimisation: 300 random queries (30% joins) ==");
    let mut r = Rng(765);
    let mut cache = Cache::default();
    let (mut w_cached, mut w_fresh) = (0.0, 0.0);
    for _ in 0..300 {
        let q = random_query(&mut r, 0.3);
        let (plan, _) = cache.get_plan(db, &q);
        let (c1, w1) = execute(db, &q, plan);
        let (c2, w2) = execute(db, &q, optimise(db, &q).0);
        assert_eq!(
            c1,
            c2,
            "cached plan returned a different answer for {}",
            sql(&q)
        );
        w_cached += w1;
        w_fresh += w2;
    }
    println!(
        "  hits {}, misses {} (distinct fingerprints), invalidations {}",
        cache.st.hits, cache.st.misses, cache.st.invalidated
    );
    println!(
        "  executed work: cached (plan sniffed from first literals) {w_cached:.0}, fresh per query {w_fresh:.0} (cache costs {:+.1}% execution, saves 300 - {} optimiser runs)",
        (w_cached / w_fresh - 1.0) * 100.0,
        cache.st.misses
    );
    assert!(cache.st.hits > 200 && cache.st.invalidated == 0);
}

fn section_invalidation() {
    println!("\n== 3. scripted invalidation: exactly the dependent entries ==");
    let mut db = gen_db(3);
    let mut cache = Cache::default();
    let qt = Query {
        preds: vec![Pred {
            c: 0,
            lo: 100,
            hi: 200,
        }],
        join: false,
        hints: vec![],
    };
    let qj = Query {
        preds: vec![Pred { c: 1, lo: 5, hi: 5 }],
        join: true,
        hints: vec![],
    };
    let step = |name: &str, db: &Db, cache: &mut Cache, expect_t_hit: bool, expect_j_hit: bool| {
        let a = cache.get_plan(db, &qt).1;
        let b = cache.get_plan(db, &qj).1;
        println!(
            "  {name:<44} T-only query {}, join query {}",
            if a { "hit " } else { "MISS" },
            if b { "hit " } else { "MISS" }
        );
        assert_eq!((a, b), (expect_t_hit, expect_j_hit), "scenario {name}");
    };
    step("cold cache", &db, &mut cache, false, false);
    step("repeat", &db, &mut cache, true, true);
    db.analyze(U, Sample::Good);
    step(
        "ANALYZE u (only the join reads u)",
        &db,
        &mut cache,
        true,
        false,
    );
    db.analyze(T, Sample::Good);
    step("ANALYZE t (both read t)", &db, &mut cache, false, false);
    db.create_index(U, 2);
    step(
        "CREATE INDEX on u (only the join reads u)",
        &db,
        &mut cache,
        true,
        false,
    );
    db.drop_index(T, 0);
    step(
        "DROP INDEX t(a) (both read t)",
        &db,
        &mut cache,
        false,
        false,
    );
    db.create_index(T, 0);
    step("CREATE INDEX t(a)", &db, &mut cache, false, false);
    step("repeat", &db, &mut cache, true, true);
    // the cached entry survives nothing else: a dropped index must never be used by a stale plan
    db.drop_index(T, 0);
    let (plan, hit) = cache.get_plan(&db, &qt);
    assert!(
        !hit && plan.access == Access::Seq || plan.access != Access::Idx(0),
        "a plan must not use a dropped index"
    );
    println!(
        "  plan after dropping t(a): {:?} (re-optimised, no stale index)",
        plan.access
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Baselines: regression after a bad ANALYZE
// ---------------------------------------------------------------------------------------------------------------------

#[derive(PartialEq, Clone, Copy)]
enum Policy {
    Fresh,
    Cache,
    CacheBaseline,
}

const PLAN_COST: f64 = 50.0; // optimiser work charged per optimisation

fn section_baselines() {
    println!(
        "\n== 4. plan baselines: ANALYZE from a biased sample at query 100, from a good sample at query 200 (300 queries, 6 templates) =="
    );
    let templates: Vec<Box<dyn Fn(&mut Rng) -> Query>> = vec![
        Box::new(|r| {
            let x = 2000 + r.below(6000) as i64;
            Query {
                preds: vec![Pred {
                    c: 0,
                    lo: x,
                    hi: 10_000,
                }],
                join: false,
                hints: vec![],
            }
        }),
        Box::new(|r| {
            let x = 2000 + r.below(6000) as i64;
            Query {
                preds: vec![
                    Pred {
                        c: 0,
                        lo: x,
                        hi: 10_000,
                    },
                    Pred {
                        c: 2,
                        lo: 0,
                        hi: 60,
                    },
                ],
                join: false,
                hints: vec![],
            }
        }),
        Box::new(|r| {
            let x = r.below(1000) as i64;
            Query {
                preds: vec![Pred { c: 1, lo: x, hi: x }],
                join: false,
                hints: vec![],
            }
        }),
        Box::new(|r| {
            let x = r.below(900) as i64;
            Query {
                preds: vec![Pred {
                    c: 1,
                    lo: x,
                    hi: x + 80,
                }],
                join: false,
                hints: vec![],
            }
        }),
        Box::new(|r| {
            let x = 3000 + r.below(5000) as i64;
            Query {
                preds: vec![Pred {
                    c: 0,
                    lo: x,
                    hi: 10_000,
                }],
                join: true,
                hints: vec![],
            }
        }),
        Box::new(|r| {
            let x = r.below(1000) as i64;
            Query {
                preds: vec![Pred {
                    c: 1,
                    lo: x,
                    hi: x + 3,
                }],
                join: true,
                hints: vec![],
            }
        }),
    ];
    let mut results: Vec<(Vec<u64>, f64, usize, usize)> = vec![];
    for pol in [Policy::Fresh, Policy::Cache, Policy::CacheBaseline] {
        let mut db = gen_db(4);
        let mut r = Rng(44);
        let mut cache = Cache::default();
        let mut baseline: HashMap<String, Plan> = HashMap::new(); // previous plan per fingerprint
        let (mut answers, mut work, mut rollbacks, mut window) = (vec![], 0.0, 0, 0.0);
        for i in 0..300 {
            if i == 100 {
                db.analyze(T, Sample::Bad);
            }
            if i == 200 {
                db.analyze(T, Sample::Good);
            }
            let q = templates[i % templates.len()](&mut r);
            let fp = fingerprint(&sql(&q));
            let before = work;
            let (count, w) = match pol {
                Policy::Fresh => {
                    work += PLAN_COST;
                    execute(&db, &q, optimise(&db, &q).0)
                }
                Policy::Cache => {
                    let (p, hit) = cache.get_plan(&db, &q);
                    if !hit {
                        work += PLAN_COST;
                    }
                    execute(&db, &q, p)
                }
                Policy::CacheBaseline => {
                    let old = cache.map.get(&fp).map(|e| e.plan);
                    let (p, hit) = cache.get_plan(&db, &q);
                    if !hit {
                        work += PLAN_COST;
                    }
                    match (hit, baseline.get(&fp).copied().or(old)) {
                        (false, Some(prev)) if prev != p => {
                            // first execution of a NEW plan: run it and the baseline on the same literals, keep the cheaper (runtime comparison)
                            let (cn, wn) = execute(&db, &q, p);
                            let (co, wo) = execute(&db, &q, prev);
                            assert_eq!(cn, co);
                            work += wn.max(wo); // the costlier of the two runs is pure overhead; the cheaper one is charged below as the answer
                            let winner = if wo < wn {
                                rollbacks += 1;
                                prev
                            } else {
                                p
                            };
                            cache.map.get_mut(&fp).unwrap().plan = winner;
                            baseline.insert(fp.clone(), winner);
                            (cn, wn.min(wo))
                        }
                        _ => {
                            baseline.entry(fp.clone()).or_insert(p);
                            execute(&db, &q, p)
                        }
                    }
                }
            };
            work += w;
            answers.push(count);
            if (100..200).contains(&i) {
                window += work - before;
            }
        }
        results.push((answers, work, rollbacks, window as usize));
    }
    let names = [
        "fresh optimisation per query",
        "plan cache",
        "plan cache + baseline comparison",
    ];
    println!(
        "  {:<36} {:>14} {:>22} {:>10}",
        "policy", "total work", "work during bad stats", "rollbacks"
    );
    for (n, (_, w, rb, win)) in names.iter().zip(&results) {
        println!("  {n:<36} {w:>14.0} {win:>22} {rb:>10}");
    }
    assert!(
        results[0].0 == results[1].0 && results[1].0 == results[2].0,
        "answers differ between policies"
    );
    assert!(
        results[2].3 <= results[1].3,
        "baselines must not cost more than the plain cache during the bad-statistics window"
    );
    assert!(
        results[2].1 <= results[1].1,
        "baselines must not cost more overall"
    );
    assert!(
        results[2].2 >= 1,
        "at least one regression must have been rolled back"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Multi-plan trial (MongoDB style)
// ---------------------------------------------------------------------------------------------------------------------

const TRIAL_RESULTS: usize = 100;

struct Racer {
    access: Access,
    pos: usize,
    started: bool,
    work: f64,
    results: Vec<usize>,
    done: bool,
    total_steps: usize,
}

impl Racer {
    fn new(db: &Db, q: &Query, access: Access) -> Racer {
        let total_steps = match access {
            Access::Seq => NT,
            Access::Idx(c) => {
                let idx = db.t[T].idx[c].as_ref().unwrap();
                let p = q.preds.iter().find(|p| p.c == c).unwrap();
                idx.partition_point(|e| e.0 <= p.hi) - idx.partition_point(|e| e.0 < p.lo)
            }
        };
        Racer {
            access,
            pos: 0,
            started: false,
            work: 0.0,
            results: vec![],
            done: total_steps == 0,
            total_steps,
        }
    }
    /// Advance by roughly `units` of work; each step is one row (cost 1) or one index entry (cost 4).
    fn run(&mut self, db: &Db, q: &Query, units: f64) {
        let target = self.work + units;
        while !self.done && self.work < target {
            match self.access {
                Access::Seq => {
                    self.work += 1.0;
                    if matches(db, q, self.pos) {
                        self.results.push(self.pos);
                    }
                }
                Access::Idx(c) => {
                    let idx = db.t[T].idx[c].as_ref().unwrap();
                    let p = q.preds.iter().find(|p| p.c == c).unwrap();
                    if !self.started {
                        self.started = true;
                        self.work += 2.0;
                        self.pos = idx.partition_point(|e| e.0 < p.lo);
                        self.total_steps += self.pos;
                    }
                    self.work += 4.0;
                    let r = idx[self.pos].1 as usize;
                    if matches(db, q, r) {
                        self.results.push(r);
                    }
                }
            }
            self.pos += 1;
            if self.pos >= self.total_steps {
                self.done = true;
            }
        }
    }
    fn progress(&self) -> f64 {
        self.pos as f64 / self.total_steps.max(1) as f64
    }
}

/// Race the candidate access paths; returns (winner access, total work incl. completing the winner, answer count).
fn trial(db: &Db, q: &Query) -> (Access, f64, u64) {
    let mut racers: Vec<Racer> = candidates(db, q)
        .into_iter()
        .map(|a| Racer::new(db, q, a))
        .collect();
    let budget = 0.29 * NT as f64;
    let mut winner = None;
    while winner.is_none() {
        for r in racers.iter_mut() {
            r.run(db, q, 4.0);
        }
        if let Some(i) = racers
            .iter()
            .position(|r| r.results.len() >= TRIAL_RESULTS || r.done)
        {
            winner = Some(i); // first past the post: enough results, or EOF
        } else if racers.iter().all(|r| r.work >= budget) {
            // budget exhausted: best productivity (results per work), falling back to progress per work when nothing matched yet
            let score = |r: &Racer| {
                if r.results.is_empty() {
                    r.progress() / r.work * 1e-6
                } else {
                    r.results.len() as f64 / r.work
                }
            };
            winner = Some(
                (0..racers.len())
                    .max_by(|&a, &b| score(&racers[a]).partial_cmp(&score(&racers[b])).unwrap())
                    .unwrap(),
            );
        }
    }
    let w = winner.unwrap();
    let trial_work: f64 = racers.iter().map(|r| r.work).sum();
    let before = racers[w].work;
    racers[w].run(db, q, f64::INFINITY); // the winner CONTINUES: its trial work is not repeated
    let total = trial_work + (racers[w].work - before);
    (racers[w].access, total, racers[w].results.len() as u64)
}

fn section_trial() {
    println!("\n== 5. multi-plan trial vs cost-based plans: 500 random single-table workloads ==");
    let mut db = gen_db(5);
    let mut r = Rng(55);
    let qs: Vec<Query> = (0..500).map(|_| random_query(&mut r, 0.0)).collect();
    let good = db.stats[T].clone();
    let bad = analyze(&db.t[T], Sample::Bad);
    let (mut regrets, mut reg_bad, mut reg_good) = (vec![], vec![], vec![]);
    let (mut t_trial, mut t_bad, mut t_good, mut t_best, mut t_seq, mut wins) =
        (0.0, 0.0, 0.0, 0.0, 0.0, HashMap::new());
    for q in &qs {
        let full: Vec<(Access, f64, u64)> = candidates(&db, q)
            .into_iter()
            .map(|a| {
                let (c, w) = execute(
                    &db,
                    q,
                    Plan {
                        access: a,
                        build: T,
                    },
                );
                (a, w, c)
            })
            .collect();
        let best = full.iter().map(|x| x.1).fold(f64::INFINITY, f64::min);
        assert!(
            full.iter().all(|x| x.2 == full[0].2),
            "access paths disagree on the answer"
        );
        let (acc, w, cnt) = trial(&db, q);
        assert_eq!(cnt, full[0].2, "trial winner produced a wrong answer");
        db.stats[T] = bad.clone();
        let wb = execute(&db, q, optimise(&db, q).0).1;
        db.stats[T] = good.clone();
        let wg = execute(&db, q, optimise(&db, q).0).1;
        regrets.push(w / best);
        reg_bad.push(wb / best);
        reg_good.push(wg / best);
        (t_trial, t_bad, t_good, t_best, t_seq) = (
            t_trial + w,
            t_bad + wb,
            t_good + wg,
            t_best + best,
            t_seq + full[0].1,
        );
        *wins.entry(format!("{acc:?}")).or_insert(0) += 1;
    }
    println!(
        "  {:<34} {:>12} {:>10} {:>8} {:>8} {:>8}",
        "strategy", "total work", "vs best", "mean", "p95", "max"
    );
    for (n, tot, rg) in [
        ("hindsight best single plan", t_best, vec![1.0]),
        ("trial (race, 100 results)", t_trial, regrets.clone()),
        ("cost-based, good statistics", t_good, reg_good.clone()),
        ("cost-based, biased statistics", t_bad, reg_bad.clone()),
        ("always sequential scan", t_seq, vec![1.0]),
    ] {
        let mean = rg.iter().sum::<f64>() / rg.len() as f64;
        println!(
            "  {n:<34} {tot:>12.0} {:>9.2}x {mean:>8.2} {:>8.2} {:>8.2}",
            tot / t_best,
            pct(&rg, 0.95),
            pct(&rg, 1.0)
        );
    }
    let mut w: Vec<_> = wins.into_iter().collect();
    w.sort();
    println!("  trial winners: {w:?}");
    let mean = regrets.iter().sum::<f64>() / regrets.len() as f64;
    assert!(
        mean <= 3.0,
        "trial must stay within 3x of the hindsight best on average, got {mean}"
    );
    assert!(
        t_trial < t_bad,
        "the trial must beat cost-based planning from biased statistics"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// UDF optimisation
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum E {
    Arg,
    Const(i64),
    Add(Box<E>, Box<E>),
    Mul(Box<E>, Box<E>),
    Mod(Box<E>, i64), // modulus is a nonzero constant
    Lt(Box<E>, Box<E>),
}

fn eval(e: &E, arg: i64) -> i64 {
    match e {
        E::Arg => arg,
        E::Const(c) => *c,
        E::Add(a, b) => eval(a, arg).wrapping_add(eval(b, arg)),
        E::Mul(a, b) => eval(a, arg).wrapping_mul(eval(b, arg)),
        E::Mod(a, m) => eval(a, arg).rem_euclid(*m),
        E::Lt(a, b) => (eval(a, arg) < eval(b, arg)) as i64,
    }
}

fn size(e: &E) -> usize {
    match e {
        E::Arg | E::Const(_) => 1,
        E::Add(a, b) | E::Mul(a, b) | E::Lt(a, b) => 1 + size(a) + size(b),
        E::Mod(a, _) => 1 + size(a),
    }
}

fn random_expr(r: &mut Rng, depth: u32) -> E {
    if depth == 0 || r.below(5) == 0 {
        return if r.below(2) == 0 {
            E::Arg
        } else {
            E::Const(r.below(20) as i64 - 5)
        };
    }
    let (a, b) = (
        Box::new(random_expr(r, depth - 1)),
        Box::new(random_expr(r, depth - 1)),
    );
    match r.below(4) {
        0 => E::Add(a, b),
        1 => E::Mul(a, b),
        2 => E::Mod(a, 1 + r.below(12) as i64),
        _ => E::Lt(a, b),
    }
}

/// Inline: substitute the call argument for the UDF parameter, then constant-fold. The result mentions only the caller's expression.
fn substitute(body: &E, with: &E) -> E {
    match body {
        E::Arg => with.clone(),
        E::Const(c) => E::Const(*c),
        E::Add(a, b) => E::Add(Box::new(substitute(a, with)), Box::new(substitute(b, with))),
        E::Mul(a, b) => E::Mul(Box::new(substitute(a, with)), Box::new(substitute(b, with))),
        E::Mod(a, m) => E::Mod(Box::new(substitute(a, with)), *m),
        E::Lt(a, b) => E::Lt(Box::new(substitute(a, with)), Box::new(substitute(b, with))),
    }
}

fn fold(e: &E) -> E {
    let c = |x: &E| if let E::Const(v) = x { Some(*v) } else { None };
    match e {
        E::Add(a, b) => {
            let (a, b) = (fold(a), fold(b));
            match (c(&a), c(&b)) {
                (Some(x), Some(y)) => E::Const(x.wrapping_add(y)),
                (Some(0), _) => b,
                (_, Some(0)) => a,
                _ => E::Add(Box::new(a), Box::new(b)),
            }
        }
        E::Mul(a, b) => {
            let (a, b) = (fold(a), fold(b));
            match (c(&a), c(&b)) {
                (Some(x), Some(y)) => E::Const(x.wrapping_mul(y)),
                (Some(1), _) => b,
                (_, Some(1)) => a,
                (Some(0), _) | (_, Some(0)) => E::Const(0),
                _ => E::Mul(Box::new(a), Box::new(b)),
            }
        }
        E::Mod(a, m) => {
            let a = fold(a);
            match c(&a) {
                Some(x) => E::Const(x.rem_euclid(*m)),
                None => E::Mod(Box::new(a), *m),
            }
        }
        E::Lt(a, b) => {
            let (a, b) = (fold(a), fold(b));
            match (c(&a), c(&b)) {
                (Some(x), Some(y)) => E::Const((x < y) as i64),
                _ => E::Lt(Box::new(a), Box::new(b)),
            }
        }
        other => other.clone(),
    }
}

const CALL_COST: f64 = 20.0;
const BATCH: usize = 1024;

#[derive(Clone, Copy, Debug)]
enum UdfMode {
    Opaque,
    Inlined,
    Batched,
    Memoised,
}

/// WHERE udf(c) = 1 AND a < :x AND b < :y over a scan of T. `order` lists the conjuncts in evaluation order (0 = UDF, 1 = a, 2 = b).
fn udf_scan(
    db: &Db,
    body: &E,
    inlined: &E,
    mode: UdfMode,
    order: &[usize],
    x: i64,
    y: i64,
) -> (u64, f64) {
    let t = &db.t[T];
    let (mut work, mut count) = (0.0, 0u64);
    let mut memo: HashMap<i64, i64> = HashMap::new();
    let mut pending = 0usize; // rows in the current batch whose UDF argument has been collected
    for r in 0..NT {
        for &p in order {
            let ok = match p {
                0 => {
                    let arg = t.col[2][r];
                    match mode {
                        UdfMode::Opaque => {
                            work += CALL_COST;
                            eval(body, arg) == 1
                        }
                        UdfMode::Inlined => {
                            work += 0.1 * size(inlined) as f64;
                            eval(inlined, arg) == 1
                        }
                        UdfMode::Batched => {
                            pending += 1;
                            work += 1.0;
                            if pending == BATCH {
                                work += CALL_COST;
                                pending = 0;
                            }
                            eval(body, arg) == 1
                        }
                        UdfMode::Memoised => {
                            work += 1.0;
                            let v = *memo.entry(arg).or_insert_with(|| {
                                work += CALL_COST;
                                eval(body, arg)
                            });
                            v == 1
                        }
                    }
                }
                1 => {
                    work += 1.0;
                    t.col[0][r] < x
                }
                _ => {
                    work += 1.0;
                    t.col[1][r] < y
                }
            };
            if !ok {
                break;
            }
            if p == *order.last().unwrap() {
                count += 1;
            }
        }
    }
    if pending > 0 {
        work += CALL_COST;
    }
    (count, work)
}

fn section_udf() {
    println!("\n== 6. UDF optimisation: semantics, call strategies and cost annotation ==");
    let mut r = Rng(6);
    // (a) inlining preserves semantics on 300 random UDF bodies and arguments
    let mut shrunk = 0;
    for _ in 0..300 {
        let body = random_expr(&mut r, 4);
        let inner = random_expr(&mut r, 2); // the caller passes an expression, not only a column: udf(inner(c))
        let inlined = fold(&substitute(&body, &inner));
        shrunk += (size(&inlined) < size(&body) + size(&inner)) as usize;
        for _ in 0..50 {
            let c = r.below(200) as i64 - 50;
            assert_eq!(
                eval(&body, eval(&inner, c)),
                eval(&inlined, c),
                "inlining changed semantics of {body:?} on {inner:?}"
            );
        }
    }
    println!(
        "  inlining + constant folding preserved the value on 300 random bodies x 50 inputs ({shrunk} of 300 got smaller after folding)"
    );

    // (b) one realistic UDF over the table: bucket(c) = ((c * 7 + 3) % 11) < 4
    let db = gen_db(66);
    let body = E::Lt(
        Box::new(E::Mod(
            Box::new(E::Add(
                Box::new(E::Mul(Box::new(E::Arg), Box::new(E::Const(7)))),
                Box::new(E::Const(3)),
            )),
            11,
        )),
        Box::new(E::Const(4)),
    );
    let inlined = fold(&substitute(&body, &E::Arg));
    let (x, y) = (6000, 500);
    // Estimated per-conjunct (selectivity, cost): the UDF annotation says 20 per call; without annotation the optimizer assumes 1.
    let sel = [0.36, x as f64 / 10_000.0, y as f64 / 1000.0];
    let ordered = |cost: [f64; 3]| {
        let mut o = vec![0usize, 1, 2];
        o.sort_by(|&a, &b| {
            ((1.0 - sel[b]) / cost[b])
                .partial_cmp(&((1.0 - sel[a]) / cost[a]))
                .unwrap()
        });
        o
    };
    let (o_ann, o_none) = (ordered([CALL_COST, 1.0, 1.0]), ordered([1.0, 1.0, 1.0]));
    println!("  conjunct order with cost annotation {o_ann:?} (UDF last), without {o_none:?}");
    let reference = udf_scan(&db, &body, &inlined, UdfMode::Opaque, &[0, 1, 2], x, y);
    println!("  {:<28} {:>10} {:>12}", "strategy", "matches", "work");
    let mut works = HashMap::new();
    for (name, mode, order) in [
        ("opaque, UDF first (no cost)", UdfMode::Opaque, &o_none),
        ("opaque, annotated order", UdfMode::Opaque, &o_ann),
        ("inlined, annotated order", UdfMode::Inlined, &o_ann),
        ("batched 1024, annotated", UdfMode::Batched, &o_ann),
        ("memoised, annotated", UdfMode::Memoised, &o_ann),
        ("inlined, UDF first", UdfMode::Inlined, &o_none),
    ] {
        let (c, w) = udf_scan(&db, &body, &inlined, mode, order, x, y);
        println!("  {name:<28} {c:>10} {w:>12.0}");
        assert_eq!(c, reference.0, "{name}: result differs from the reference");
        works.insert(name, w);
    }
    assert!(
        works["opaque, annotated order"] < works["opaque, UDF first (no cost)"],
        "cost annotation must reorder the expensive UDF last"
    );
    assert!(works["inlined, annotated order"] < works["opaque, annotated order"]);
    assert!(
        works["memoised, annotated"] < works["opaque, annotated order"]
            && works["batched 1024, annotated"] < works["opaque, annotated order"]
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Hints
// ---------------------------------------------------------------------------------------------------------------------

fn section_hints() {
    println!("\n== 7. hints: forced access path and join build side ==");
    let db = gen_db(7);
    let base = Query {
        preds: vec![
            Pred {
                c: 0,
                lo: 9000,
                hi: 9100,
            },
            Pred {
                c: 1,
                lo: 0,
                hi: 900,
            },
        ],
        join: false,
        hints: vec![],
    };
    let free = optimise(&db, &base).0;
    let mut out = vec![];
    for hint in [Hint::Seq, Hint::Index(0), Hint::Index(1), Hint::Index(2)] {
        let q = Query {
            hints: vec![hint.clone()],
            ..base.clone()
        };
        let (plan, _, ignored) = optimise(&db, &q);
        let (c, w) = execute(&db, &q, plan);
        println!(
            "  {:<12} -> {:?}{} work {w:.0}, answer {c}",
            format!("{hint:?}"),
            plan.access,
            if ignored {
                " (hint not applicable: ignored)"
            } else {
                ""
            }
        );
        out.push((hint, plan, c, ignored));
    }
    let ans = execute(&db, &base, free).0;
    assert!(
        out.iter().all(|o| o.2 == ans),
        "hints must not change the answer"
    );
    assert_eq!(out[0].1.access, Access::Seq);
    assert_eq!(out[1].1.access, Access::Idx(0));
    assert_eq!(out[2].1.access, Access::Idx(1));
    assert!(
        out[3].3 && out[3].1.access == free.access,
        "an inapplicable INDEX hint falls back to the optimizer"
    );
    let j = Query {
        preds: vec![Pred { c: 1, lo: 5, hi: 8 }],
        join: true,
        hints: vec![],
    };
    let (pj, _, _) = optimise(&db, &j);
    let other = if pj.build == T { U } else { T };
    let jh = Query {
        hints: vec![Hint::Build(other)],
        ..j.clone()
    };
    let (ph, _, _) = optimise(&db, &jh);
    let (c1, w1) = execute(&db, &j, pj);
    let (c2, w2) = execute(&db, &jh, ph);
    println!(
        "  join: optimizer builds on {}, hint BUILD({}) builds on {}; work {w1:.0} vs {w2:.0}; same answer {c1}",
        if pj.build == T { "t" } else { "u" },
        if other == T { "t" } else { "u" },
        if ph.build == T { "t" } else { "u" }
    );
    assert_eq!(c1, c2);
    assert_eq!(ph.build, other);
}

fn main() {
    let db = gen_db(765);
    section_fingerprints();
    section_cache_correctness(&db);
    section_invalidation();
    section_baselines();
    section_trial();
    section_udf();
    section_hints();
    println!("\nall plan cache and multi-plan trial checks passed");
}
