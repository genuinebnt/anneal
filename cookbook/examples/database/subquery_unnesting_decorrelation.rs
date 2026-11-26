//! RECIPE 752 (database): subquery unnesting and decorrelation with dependent joins (correlated scalar/EXISTS/IN/ANY/ALL/LATERAL -> joins).
//! LEVEL: advanced
//! Use in: SQL planners and query rewriters, understanding why `NOT IN` and `COUNT(*)` subqueries are special, EXPLAIN reading.   Run: cargo run --release --example subquery_unnesting_decorrelation
//! COMMON USES: HyPer/Umbra (the Neumann-Kemper dependent-join algorithm this file follows), PostgreSQL (pulls up EXISTS/IN into semi/anti joins, keeps
//!   other correlated subqueries as re-executed SubPlans), SQL Server (apply operators + "orthogonal optimization of subqueries"), Apache Calcite
//!   (SubQueryRemoveRule + decorrelator), Spark Catalyst (RewriteCorrelatedScalarSubquery, RewritePredicateSubquery), DuckDB (dependent joins flattened
//!   like in the paper); also any DSL compiler that lowers "for each row run a nested query" to set-at-a-time operators.
//! Course: CMU 15-799 Query Optimization (Spring 2025), lecture 11 (Unnesting Queries)
#![allow(dead_code)]
//!
//! WHAT: a mini algebra (Scan, Select, Project, Join{inner,left,semi,anti}, Aggregate, Union, Distinct) with three-valued logic (NULL = `None`) and a
//! DEPENDENT JOIN operator `DepJoin{kind, L, X}`: for every row l of L, evaluate X with the columns of l visible as `$j` (outer references) and combine.
//! SQL subqueries are translated mechanically to dependent joins:
//!   scalar subquery (WHERE or SELECT list)  -> DepJoin(inner) of L with a scalar aggregate       EXISTS / NOT EXISTS -> DepJoin(semi / anti)
//!   x IN / NOT IN / op ANY / op ALL         -> DepJoin(inner) with a scalar BOOL_OR / BOOL_AND over the tri-valued comparison `x op d`
//!   LATERAL / LEFT JOIN LATERAL             -> DepJoin(inner / left)
//! Two evaluators exist: NAIVE (the executor re-runs X once per outer row: the nested-loop semantics that DEFINES the meaning of the query) and
//! UNNESTED (the dependent joins are removed by the rewrite below, then ordinary hash/nested-loop joins run).
//!
//! HOW (Neumann & Kemper, "Unnesting Arbitrary Queries"): let `cols` be the outer columns X actually references. Build the DOMAIN
//! `D = DISTINCT pi_cols(L)`: duplicate-eliminated bindings, so a correlated predicate is evaluated once per DISTINCT binding, not once per row.
//! Then `pd(D, X)` pushes the dependent join down through X until the correlation disappears; its output is `[D cols | X cols]`, i.e. every result
//! row remembers which binding produced it. The result joins back to L with NULL-SAFE equality (`IS NOT DISTINCT FROM`) on the binding, because an
//! outer value may itself be NULL and must still find its own sub-result. Push-down rules (k = |cols|):
//!   X not correlated              pd = D x X (cross product, becomes an equi-join once the parent Select is merged)
//!   Select(p, Y)                  Select(p', pd(Y))       (outer refs in p become D columns; local columns shift by k)
//!   Project(es, Y)                Project(D cols ++ es', pd(Y))
//!   Join(Y, Z)                    pd(Y) JOIN pd(Z) on p' AND D-equality, then drop the duplicated D (both sides carry the binding)
//!   Aggregate(g, f, Y)            Aggregate(D cols ++ g, f', pd(Y))   -- grouping by the binding keeps bindings apart
//!   Union(Y, Z) / Distinct(Y)     Union(pd(Y), pd(Z)) / Distinct(pd(Y))
//! THE COUNT BUG (Kim 1982, Ganski & Wong 1987): a SCALAR aggregate over an empty set still returns a row (COUNT = 0, BOOL_AND = true, SUM = NULL),
//! but after pushing the aggregate below the join GROUP BY the binding simply has NO group when no inner row matched, so an inner join with L loses
//! the outer row (e.g. `WHERE 0 = (SELECT COUNT(*) ...)` returns nothing). The fix implemented here: LEFT OUTER join D with the grouped result and
//! replace a missing group by the aggregate's value over the empty set (`CASE WHEN marker IS NULL THEN agg(empty) ELSE agg END`; the marker is an
//! extra COUNT(*) column, so a genuine NULL aggregate (SUM of NULLs) is not confused with a missing group). `Decorrelator::count_fix = false` turns the
//! fix off, and the checks below prove that this variant really returns wrong answers.
//! NOT IN PITFALL: `x NOT IN (S)` is TRUE only if x differs from EVERY element; one NULL in S makes it UNKNOWN (row dropped) and `NOT IN (empty)` is
//! TRUE. The tempting rewrite `NOT EXISTS (... WHERE d = x)` treats UNKNOWN as TRUE and is WRONG whenever S contains NULL; here NOT IN is evaluated as
//! BOOL_AND over `x <> d` with full three-valued logic, so it is correct, and a check shows the naive anti-join rewrite differs.
//!
//! WHY unnest: nested-loop re-execution costs |L| x cost(X); the unnested plan evaluates X once for all distinct bindings with hash joins, and
//! lets the join orderer treat the former subquery like any other relation. Not every subquery is worth unnesting (a tiny L with an indexed X is
//! best left correlated), which is why real systems keep both strategies.
//!
//! CHECKS (asserts): 600 random (database, query) pairs over all 10 query shapes (NULLs, duplicates, empty tables, correlated equality/inequality,
//! UNION and self-join bodies): unnested result multiset == naive result multiset and no DepJoin survives; classic COUNT-bug and NOT-IN-with-NULL
//! cases; the count-bug-disabled variant and the NOT-EXISTS rewrite of NOT IN are shown to be wrong; on a scaled database the unnested plans process
//! fewer rows than naive re-execution (counts printed; timings are never asserted).
//! Read: CMU 15-799 Spring 2025 lecture 11 papers: "Unnesting Arbitrary Queries" (Neumann, Kemper, BTW 2015)
//!       https://15799.courses.cs.cmu.edu/spring2025/papers/11-unnesting/neumann-btw2015.pdf
//!       and "Improving Unnesting of Complex Queries" (Neumann, BTW 2025) https://15799.courses.cs.cmu.edu/spring2025/papers/11-unnesting/neumann-btw2025.pdf
//!       Schedule https://15799.courses.cs.cmu.edu/spring2025/schedule.html

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

type V = Option<i64>; // None = SQL NULL
type Row = Vec<V>;

// ---------- seeded RNG ----------
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n.max(1)
    }
}

// ---------- expressions with three-valued logic ----------
#[derive(Clone, Copy, PartialEq, Debug)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
impl Op {
    fn test(self, x: i64, y: i64) -> bool {
        match self {
            Op::Eq => x == y,
            Op::Ne => x != y,
            Op::Lt => x < y,
            Op::Le => x <= y,
            Op::Gt => x > y,
            Op::Ge => x >= y,
        }
    }
    fn sym(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Ne => "<>",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }
    }
}

#[derive(Clone, Debug)]
enum Expr {
    Col(usize),   // column of the current row
    Outer(usize), // column of the outer row of the enclosing dependent join ($j)
    Lit(V),
    Cmp(Op, Box<Expr>, Box<Expr>), // NULL if either side is NULL
    NullEq(Box<Expr>, Box<Expr>),  // IS NOT DISTINCT FROM: NULL equals NULL, never UNKNOWN
    And(Vec<Expr>),                // FALSE dominates UNKNOWN
    Not(Box<Expr>),
    IsNull(Box<Expr>),
    Case(Box<Expr>, Box<Expr>, Box<Expr>), // CASE WHEN c THEN t ELSE f END (c must be TRUE)
}
use Expr::*;

fn col(i: usize) -> Expr {
    Col(i)
}
fn outer(j: usize) -> Expr {
    Outer(j)
}
fn lit(v: i64) -> Expr {
    Lit(Some(v))
}
fn cmp(op: Op, a: Expr, b: Expr) -> Expr {
    Cmp(op, Box::new(a), Box::new(b))
}
fn null_eq(a: Expr, b: Expr) -> Expr {
    NullEq(Box::new(a), Box::new(b))
}

/// Booleans are Some(1) / Some(0) / None (UNKNOWN); a Select keeps a row only when its predicate is exactly TRUE.
fn eval(e: &Expr, row: &[V], outer_row: &[V]) -> V {
    match e {
        Col(i) => row[*i],
        Outer(j) => outer_row[*j],
        Lit(v) => *v,
        Cmp(op, a, b) => match (eval(a, row, outer_row), eval(b, row, outer_row)) {
            (Some(x), Some(y)) => Some(op.test(x, y) as i64),
            _ => None,
        },
        NullEq(a, b) => Some((eval(a, row, outer_row) == eval(b, row, outer_row)) as i64),
        And(es) => {
            let mut unknown = false;
            for e in es {
                match eval(e, row, outer_row) {
                    Some(0) => return Some(0),
                    None => unknown = true,
                    _ => {}
                }
            }
            if unknown { None } else { Some(1) }
        }
        Not(a) => eval(a, row, outer_row).map(|x| 1 - x),
        IsNull(a) => Some(eval(a, row, outer_row).is_none() as i64),
        Case(c, t, f) => {
            if eval(c, row, outer_row) == Some(1) {
                eval(t, row, outer_row)
            } else {
                eval(f, row, outer_row)
            }
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Col(i) => write!(f, "#{i}"),
            Outer(j) => write!(f, "${j}"),
            Lit(None) => write!(f, "NULL"),
            Lit(Some(v)) => write!(f, "{v}"),
            Cmp(op, a, b) => write!(f, "{a} {} {b}", op.sym()),
            NullEq(a, b) => write!(f, "{a} IS NOT DISTINCT FROM {b}"),
            And(es) if es.is_empty() => write!(f, "true"),
            And(es) => {
                let parts: Vec<String> = es.iter().map(|e| e.to_string()).collect();
                write!(f, "{}", parts.join(" AND "))
            }
            Not(a) => write!(f, "NOT ({a})"),
            IsNull(a) => write!(f, "{a} IS NULL"),
            Case(c, t, e) => write!(f, "CASE WHEN {c} THEN {t} ELSE {e} END"),
        }
    }
}

/// Rewrite column and outer references (used when pushing a dependent join down: outer refs become columns of the domain D).
fn remap(e: &Expr, cm: &dyn Fn(usize) -> usize, om: &dyn Fn(usize) -> Expr) -> Expr {
    let r = |x: &Expr| Box::new(remap(x, cm, om));
    match e {
        Col(i) => Col(cm(*i)),
        Outer(j) => om(*j),
        Lit(v) => Lit(*v),
        Cmp(o, a, b) => Cmp(*o, r(a), r(b)),
        NullEq(a, b) => NullEq(r(a), r(b)),
        And(es) => And(es.iter().map(|x| remap(x, cm, om)).collect()),
        Not(a) => Not(r(a)),
        IsNull(a) => IsNull(r(a)),
        Case(c, t, f) => Case(r(c), r(t), r(f)),
    }
}

fn expr_outers(e: &Expr, out: &mut BTreeSet<usize>) {
    match e {
        Outer(j) => {
            out.insert(*j);
        }
        Cmp(_, a, b) | NullEq(a, b) => {
            expr_outers(a, out);
            expr_outers(b, out);
        }
        And(es) => es.iter().for_each(|x| expr_outers(x, out)),
        Not(a) | IsNull(a) => expr_outers(a, out),
        Case(c, t, f) => {
            expr_outers(c, out);
            expr_outers(t, out);
            expr_outers(f, out);
        }
        Col(_) | Lit(_) => {}
    }
}

// ---------- aggregates ----------
#[derive(Clone, Copy, PartialEq, Debug)]
enum AggFn {
    CountStar,
    Count,
    Sum,
    Min,
    Max,
    BoolOr, // SQL EVERY/BOOL_OR with 3VL: any TRUE -> TRUE, else any NULL -> NULL, else FALSE (empty -> FALSE)
    BoolAnd, // any FALSE -> FALSE, else any NULL -> NULL, else TRUE (empty -> TRUE)
}
impl AggFn {
    fn finish(self, vals: &[V]) -> V {
        let nn: Vec<i64> = vals.iter().flatten().copied().collect();
        match self {
            AggFn::CountStar => Some(vals.len() as i64),
            AggFn::Count => Some(nn.len() as i64),
            AggFn::Sum => (!nn.is_empty()).then(|| nn.iter().sum()),
            AggFn::Min => nn.iter().min().copied(),
            AggFn::Max => nn.iter().max().copied(),
            AggFn::BoolOr if vals.contains(&Some(1)) => Some(1),
            AggFn::BoolOr => {
                if vals.contains(&None) {
                    None
                } else {
                    Some(0)
                }
            }
            AggFn::BoolAnd if vals.contains(&Some(0)) => Some(0),
            AggFn::BoolAnd => {
                if vals.contains(&None) {
                    None
                } else {
                    Some(1)
                }
            }
        }
    }
    /// Value over the EMPTY input: the thing the COUNT bug loses.
    fn empty(self) -> V {
        self.finish(&[])
    }
}

// ---------- plans ----------
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Inner,
    Left,
    Semi,
    Anti,
}

#[derive(Clone, Debug)]
enum Plan {
    Scan(usize), // 0 = R(a, b), 1 = S(c, d)
    Select(Expr, Box<Plan>),
    Project(Vec<Expr>, Box<Plan>),
    Join {
        kind: Kind,
        cond: Expr,
        l: Box<Plan>,
        r: Box<Plan>,
    },
    DepJoin {
        kind: Kind,
        l: Box<Plan>,
        r: Box<Plan>,
    },
    Agg {
        group: Vec<usize>,
        aggs: Vec<(AggFn, Expr)>,
        input: Box<Plan>,
    },
    Union(Box<Plan>, Box<Plan>),
    Distinct(Box<Plan>),
}
use Plan::*;

fn bx(p: Plan) -> Box<Plan> {
    Box::new(p)
}

impl Plan {
    fn arity(&self) -> usize {
        match self {
            Scan(_) => 2,
            Select(_, c) | Distinct(c) => c.arity(),
            Project(es, _) => es.len(),
            Join { kind, l, r, .. } | DepJoin { kind, l, r } => match kind {
                Kind::Inner | Kind::Left => l.arity() + r.arity(),
                Kind::Semi | Kind::Anti => l.arity(),
            },
            Agg { group, aggs, .. } => group.len() + aggs.len(),
            Union(a, _) => a.arity(),
        }
    }
    fn children(&self) -> Vec<&Plan> {
        match self {
            Scan(_) => vec![],
            Select(_, c) | Project(_, c) | Distinct(c) => vec![c],
            Agg { input, .. } => vec![input],
            Join { l, r, .. } | DepJoin { l, r, .. } => vec![l, r],
            Union(a, b) => vec![a, b],
        }
    }
    fn exprs(&self) -> Vec<&Expr> {
        match self {
            Select(e, _) => vec![e],
            Project(es, _) => es.iter().collect(),
            Join { cond, .. } => vec![cond],
            Agg { aggs, .. } => aggs.iter().map(|(_, e)| e).collect(),
            _ => vec![],
        }
    }
}

fn outer_cols(p: &Plan) -> BTreeSet<usize> {
    let mut s = BTreeSet::new();
    p.exprs().into_iter().for_each(|e| expr_outers(e, &mut s));
    for c in p.children() {
        s.extend(outer_cols(c));
    }
    s
}
fn correlated(p: &Plan) -> bool {
    !outer_cols(p).is_empty()
}
fn has_depjoin(p: &Plan) -> bool {
    matches!(p, DepJoin { .. }) || p.children().into_iter().any(has_depjoin)
}

/// EXPLAIN-style plan tree.
fn explain(p: &Plan, ind: usize, out: &mut String) {
    let pad = "  ".repeat(ind);
    let tname = |t: &usize| if *t == 0 { "R(a,b)" } else { "S(c,d)" };
    let line = match p {
        Scan(t) => format!("Scan {}", tname(t)),
        Select(e, _) => format!("Select [{e}]"),
        Project(es, _) => {
            let v: Vec<String> = es.iter().map(|e| e.to_string()).collect();
            format!("Project [{}]", v.join(", "))
        }
        Join { kind, cond, .. } => format!("Join {kind:?} [{cond}]"),
        DepJoin { kind, .. } => format!("DependentJoin {kind:?}"),
        Agg { group, aggs, .. } => {
            let v: Vec<String> = aggs.iter().map(|(f, e)| format!("{f:?}({e})")).collect();
            format!("Aggregate group={group:?} [{}]", v.join(", "))
        }
        Union(..) => "UnionAll".to_string(),
        Distinct(_) => "Distinct".to_string(),
    };
    out.push_str(&format!("{pad}{line}\n"));
    for c in p.children() {
        explain(c, ind + 1, out);
    }
}
fn show(title: &str, p: &Plan) {
    let mut s = String::new();
    explain(p, 1, &mut s);
    println!("  {title}:\n{s}");
}

// ---------- executor (counts rows processed) ----------
struct Db {
    t: [Vec<Row>; 2],
}
#[derive(Default)]
struct Stats {
    work: u64,
}

/// Equality conjuncts `left col = right col` of a join condition: they let the executor hash instead of nested-loop.
fn equi_keys(cond: &Expr, nl: usize, out: &mut Vec<(usize, usize)>) {
    match cond {
        And(es) => es.iter().for_each(|e| equi_keys(e, nl, out)),
        NullEq(a, b) | Cmp(Op::Eq, a, b) => {
            if let (Col(i), Col(j)) = (&**a, &**b) {
                if *i < nl && *j >= nl {
                    out.push((*i, *j - nl));
                } else if *j < nl && *i >= nl {
                    out.push((*j, *i - nl));
                }
            }
        }
        _ => {}
    }
}

/// Combine one left row with the right rows that matched it, per join kind (shared by Join and the naive DepJoin).
fn combine(kind: Kind, l: &Row, matches: &[Row], r_arity: usize) -> Vec<Row> {
    match kind {
        Kind::Inner => matches.iter().map(|m| [l.as_slice(), m].concat()).collect(),
        Kind::Left if matches.is_empty() => vec![[l.clone(), vec![None; r_arity]].concat()],
        Kind::Left => matches.iter().map(|m| [l.as_slice(), m].concat()).collect(),
        Kind::Semi => {
            if matches.is_empty() {
                vec![]
            } else {
                vec![l.clone()]
            }
        }
        Kind::Anti => {
            if matches.is_empty() {
                vec![l.clone()]
            } else {
                vec![]
            }
        }
    }
}

fn exec(p: &Plan, db: &Db, outer_row: &[V], st: &mut Stats) -> Vec<Row> {
    match p {
        Scan(t) => {
            st.work += db.t[*t].len() as u64;
            db.t[*t].clone()
        }
        Select(e, c) => {
            let rows = exec(c, db, outer_row, st);
            st.work += rows.len() as u64;
            rows.into_iter()
                .filter(|r| eval(e, r, outer_row) == Some(1))
                .collect()
        }
        Project(es, c) => {
            let rows = exec(c, db, outer_row, st);
            st.work += rows.len() as u64;
            rows.iter()
                .map(|r| es.iter().map(|e| eval(e, r, outer_row)).collect())
                .collect()
        }
        Join { kind, cond, l, r } => {
            let (lr, rr) = (exec(l, db, outer_row, st), exec(r, db, outer_row, st));
            let (nl, nr) = (l.arity(), r.arity());
            let mut keys = vec![];
            equi_keys(cond, nl, &mut keys);
            // Hash the right side on the key columns; with no keys every right row is a candidate (nested loop).
            let mut buckets: HashMap<Vec<V>, Vec<usize>> = HashMap::new();
            for (i, row) in rr.iter().enumerate() {
                buckets
                    .entry(keys.iter().map(|&(_, j)| row[j]).collect())
                    .or_default()
                    .push(i);
            }
            let all: Vec<usize> = (0..rr.len()).collect();
            let none: Vec<usize> = vec![];
            st.work += (lr.len() + rr.len()) as u64;
            let mut out = vec![];
            for left in &lr {
                let cands = if keys.is_empty() {
                    &all
                } else {
                    let k: Vec<V> = keys.iter().map(|&(i, _)| left[i]).collect();
                    buckets.get(&k).unwrap_or(&none)
                };
                st.work += cands.len() as u64;
                let matched: Vec<Row> = cands
                    .iter()
                    .filter(|&&i| {
                        eval(cond, &[left.as_slice(), &rr[i]].concat(), outer_row) == Some(1)
                    })
                    .map(|&i| rr[i].clone())
                    .collect();
                out.extend(combine(*kind, left, &matched, nr));
            }
            out
        }
        DepJoin { kind, l, r } => {
            let lr = exec(l, db, outer_row, st);
            let nr = r.arity();
            let mut out = vec![];
            for left in &lr {
                // THE nested-loop semantics: re-run the whole right side for every single outer row.
                let matched = exec(r, db, left, st);
                out.extend(combine(*kind, left, &matched, nr));
            }
            out
        }
        Agg { group, aggs, input } => {
            let rows = exec(input, db, outer_row, st);
            st.work += rows.len() as u64;
            let mut groups: BTreeMap<Vec<V>, Vec<Vec<V>>> = BTreeMap::new();
            for row in &rows {
                let key = group.iter().map(|&g| row[g]).collect();
                let e = groups
                    .entry(key)
                    .or_insert_with(|| vec![vec![]; aggs.len()]);
                for (i, (_, ex)) in aggs.iter().enumerate() {
                    e[i].push(eval(ex, row, outer_row));
                }
            }
            // A scalar aggregate (no GROUP BY) over an empty input still yields one row: the root of the COUNT bug.
            if group.is_empty() && groups.is_empty() {
                groups.insert(vec![], vec![vec![]; aggs.len()]);
            }
            groups
                .into_iter()
                .map(|(mut k, cols)| {
                    k.extend(
                        aggs.iter()
                            .enumerate()
                            .map(|(i, (f, _))| f.finish(&cols[i])),
                    );
                    k
                })
                .collect()
        }
        Union(a, b) => {
            let mut x = exec(a, db, outer_row, st);
            x.extend(exec(b, db, outer_row, st));
            st.work += x.len() as u64;
            x
        }
        Distinct(c) => {
            let rows = exec(c, db, outer_row, st);
            st.work += rows.len() as u64;
            rows.into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        }
    }
}

fn run(p: &Plan, db: &Db) -> (Vec<Row>, u64) {
    let mut st = Stats::default();
    let mut rows = exec(p, db, &[], &mut st);
    rows.sort();
    (rows, st.work)
}

// ---------- the decorrelator ----------
struct Decorrelator {
    /// false reproduces the classic COUNT bug (inner join of the grouped result): kept only to demonstrate the failure.
    count_fix: bool,
}

impl Decorrelator {
    /// Remove every DepJoin from the plan (bottom-up).
    fn unnest(&self, p: &Plan) -> Plan {
        let u = |c: &Plan| bx(self.unnest(c));
        match p {
            Scan(t) => Scan(*t),
            Select(e, c) => Select(e.clone(), u(c)),
            Project(es, c) => Project(es.clone(), u(c)),
            Distinct(c) => Distinct(u(c)),
            Union(a, b) => Union(u(a), u(b)),
            Agg { group, aggs, input } => Agg {
                group: group.clone(),
                aggs: aggs.clone(),
                input: u(input),
            },
            Join { kind, cond, l, r } => Join {
                kind: *kind,
                cond: cond.clone(),
                l: u(l),
                r: u(r),
            },
            DepJoin { kind, l, r } => self.dependent_join(*kind, self.unnest(l), self.unnest(r)),
        }
    }

    /// DepJoin(L, X) -> Join(L, pd(D, X)) on the null-safe equality of the bindings.
    fn dependent_join(&self, kind: Kind, l: Plan, x: Plan) -> Plan {
        let cols: Vec<usize> = outer_cols(&x).into_iter().collect();
        let (nl, ax, k) = (l.arity(), x.arity(), cols.len());
        if cols.is_empty() {
            // Not actually correlated: a plain join.
            return Join {
                kind,
                cond: lit(1),
                l: bx(l),
                r: bx(x),
            };
        }
        // The domain: distinct outer bindings. Evaluating X once per binding is what turns |L| executions into one.
        let d = Distinct(bx(Project(
            cols.iter().map(|&c| col(c)).collect(),
            bx(l.clone()),
        )));
        let pushed = self.pd(&d, &cols, &x);
        let cond = And((0..k).map(|i| null_eq(col(cols[i]), col(nl + i))).collect());
        let j = Join {
            kind,
            cond,
            l: bx(l),
            r: bx(pushed),
        };
        match kind {
            // Inner/Left output is [L | D | X]: drop the binding columns again.
            Kind::Inner | Kind::Left => {
                let keep = (0..nl).chain(nl + k..nl + k + ax).map(col).collect();
                Project(keep, bx(j))
            }
            Kind::Semi | Kind::Anti => j,
        }
    }

    /// Push the dependent join with domain `d` down through `x`. Output columns: [D (k) | X].
    fn pd(&self, d: &Plan, cols: &[usize], x: &Plan) -> Plan {
        let k = cols.len();
        let pos = |j: usize| {
            cols.iter()
                .position(|&c| c == j)
                .expect("outer col in domain")
        };
        if !correlated(x) {
            return Join {
                kind: Kind::Inner,
                cond: lit(1),
                l: bx(d.clone()),
                r: bx(x.clone()),
            };
        }
        let rw = |e: &Expr| remap(e, &|i| i + k, &|j| Col(pos(j)));
        let prefix = || (0..k).map(col);
        match x {
            Select(p, y) => Select(rw(p), bx(self.pd(d, cols, y))),
            Project(es, y) => {
                let out = prefix().chain(es.iter().map(rw)).collect();
                Project(out, bx(self.pd(d, cols, y)))
            }
            Join {
                kind: Kind::Inner,
                cond,
                l,
                r,
            } => {
                // Both sides carry the binding; join them on it (null-safe) and keep one copy.
                let (al, ar) = (l.arity(), r.arity());
                let c = remap(cond, &|i| if i < al { i + k } else { i + 2 * k }, &|j| {
                    Col(pos(j))
                });
                let same = (0..k).map(|i| null_eq(col(i), col(k + al + i)));
                let j = Join {
                    kind: Kind::Inner,
                    cond: And(std::iter::once(c).chain(same).collect()),
                    l: bx(self.pd(d, cols, l)),
                    r: bx(self.pd(d, cols, r)),
                };
                let keep = (0..k + al)
                    .chain(2 * k + al..2 * k + al + ar)
                    .map(col)
                    .collect();
                Project(keep, bx(j))
            }
            Agg { group, aggs, input } => {
                let inner = self.pd(d, cols, input);
                let g2: Vec<usize> = (0..k).chain(group.iter().map(|g| g + k)).collect();
                let mut a2: Vec<(AggFn, Expr)> = aggs.iter().map(|(f, e)| (*f, rw(e))).collect();
                if !group.is_empty() {
                    return Agg {
                        group: g2,
                        aggs: a2,
                        input: bx(inner),
                    };
                }
                // Scalar aggregate: bindings with no matching inner row have NO group here.
                if !self.count_fix {
                    return Agg {
                        group: g2,
                        aggs: a2,
                        input: bx(inner),
                    }; // COUNT bug: those bindings vanish
                }
                let na = aggs.len();
                a2.push((AggFn::CountStar, lit(1))); // marker: non-NULL iff the binding has a group
                let grouped = Agg {
                    group: g2,
                    aggs: a2,
                    input: bx(inner),
                };
                let cond = And((0..k).map(|i| null_eq(col(i), col(k + i))).collect());
                let j = Join {
                    kind: Kind::Left,
                    cond,
                    l: bx(d.clone()),
                    r: bx(grouped),
                };
                let marker = 2 * k + na;
                let fixed = aggs.iter().enumerate().map(|(i, (f, _))| {
                    Case(
                        Box::new(IsNull(Box::new(col(marker)))),
                        Box::new(Lit(f.empty())),
                        Box::new(col(2 * k + i)),
                    )
                });
                Project(prefix().chain(fixed).collect(), bx(j))
            }
            Union(a, b) => Union(bx(self.pd(d, cols, a)), bx(self.pd(d, cols, b))),
            Distinct(y) => Distinct(bx(self.pd(d, cols, y))),
            other => panic!("pd: unsupported operator {other:?}"),
        }
    }
}

/// Post-pass: Select over an inner join is just a stronger join condition, which exposes equi-keys to the hash join.
fn merge_selects(p: &Plan) -> Plan {
    let m = |c: &Plan| bx(merge_selects(c));
    match p {
        Scan(t) => Scan(*t),
        Select(e, c) => match merge_selects(c) {
            Join {
                kind: Kind::Inner,
                cond,
                l,
                r,
            } => Join {
                kind: Kind::Inner,
                cond: And(vec![cond, e.clone()]),
                l,
                r,
            },
            other => Select(e.clone(), bx(other)),
        },
        Project(es, c) => Project(es.clone(), m(c)),
        Distinct(c) => Distinct(m(c)),
        Union(a, b) => Union(m(a), m(b)),
        Agg { group, aggs, input } => Agg {
            group: group.clone(),
            aggs: aggs.clone(),
            input: m(input),
        },
        Join { kind, cond, l, r } => Join {
            kind: *kind,
            cond: cond.clone(),
            l: m(l),
            r: m(r),
        },
        DepJoin { kind, l, r } => DepJoin {
            kind: *kind,
            l: m(l),
            r: m(r),
        },
    }
}

fn unnested(p: &Plan, count_fix: bool) -> Plan {
    merge_selects(&Decorrelator { count_fix }.unnest(p))
}

// ---------- SQL shapes -> naive dependent-join plans ----------
#[derive(Clone, Copy, Debug)]
enum Shape {
    ScalarWhere(AggFn, Op), // WHERE r.b op (SELECT agg(s.d) FROM S WHERE <corr>)
    ScalarSelect(AggFn),    // SELECT r.*, (SELECT agg(s.d) ...)
    Exists,
    NotExists,
    In,
    NotIn,
    Any(Op),
    All(Op),
    Lateral,     // FROM R, LATERAL (SELECT c, d FROM S WHERE <corr>)
    LeftLateral, // FROM R LEFT JOIN LATERAL (...) ON true
}
const SHAPE_NAMES: [&str; 10] = [
    "scalar-where",
    "scalar-select",
    "exists",
    "not-exists",
    "in",
    "not-in",
    "any",
    "all",
    "lateral",
    "left-lateral",
];

/// The subquery body `SELECT .. FROM S [JOIN S] WHERE <corr> [AND d >= k] [UNION ALL ...]`.
#[derive(Clone)]
struct Body {
    corr: Expr,
    local: Option<i64>,
    union2: Option<Expr>,
    selfjoin: bool,
}

fn branch(corr: &Expr, local: Option<i64>, selfjoin: bool, proj: &[usize]) -> Plan {
    let mut f = vec![corr.clone()];
    if let Some(k) = local {
        f.push(cmp(Op::Ge, col(1), lit(k)));
    }
    let sel = Select(And(f), bx(Scan(1)));
    if selfjoin {
        // S s1 JOIN S s2 ON s1.d = s2.c: columns [c1, d1, c2, d2]; the body exposes s2's columns.
        let j = Join {
            kind: Kind::Inner,
            cond: cmp(Op::Eq, col(1), col(2)),
            l: bx(sel),
            r: bx(Scan(1)),
        };
        return Project(proj.iter().map(|&p| col(2 + p)).collect(), bx(j));
    }
    Project(proj.iter().map(|&p| col(p)).collect(), bx(sel))
}

fn body_plan(b: &Body, proj: &[usize]) -> Plan {
    let first = branch(&b.corr, b.local, b.selfjoin, proj);
    match &b.union2 {
        Some(c2) => Union(bx(first), bx(branch(c2, b.local, b.selfjoin, proj))),
        None => first,
    }
}

fn scalar(f: AggFn, input: Plan) -> Plan {
    Agg {
        group: vec![],
        aggs: vec![(f, col(0))],
        input: bx(input),
    }
}

/// The reference semantics: a dependent join per subquery, evaluated by re-execution.
fn naive_plan(shape: Shape, b: &Body) -> Plan {
    let dep = |kind, r| DepJoin {
        kind,
        l: bx(Scan(0)),
        r: bx(r),
    };
    let keep_ab = |p: Plan| Project(vec![col(0), col(1)], bx(p));
    // x op d, evaluated per body row with x = r.b (an outer reference): tri-valued, then aggregated.
    let quantified = |op: Op, f: AggFn| {
        let tri = Project(vec![cmp(op, outer(1), col(0))], bx(body_plan(b, &[1])));
        keep_ab(Select(
            cmp(Op::Eq, col(2), lit(1)),
            bx(dep(Kind::Inner, scalar(f, tri))),
        ))
    };
    match shape {
        Shape::ScalarWhere(f, op) => {
            let j = dep(Kind::Inner, scalar(f, body_plan(b, &[1])));
            keep_ab(Select(cmp(op, col(1), col(2)), bx(j)))
        }
        Shape::ScalarSelect(f) => dep(Kind::Inner, scalar(f, body_plan(b, &[1]))),
        Shape::Exists => dep(Kind::Semi, body_plan(b, &[1])),
        Shape::NotExists => dep(Kind::Anti, body_plan(b, &[1])),
        Shape::In => quantified(Op::Eq, AggFn::BoolOr),
        Shape::NotIn => quantified(Op::Ne, AggFn::BoolAnd), // NOT IN == ALL(<>)
        Shape::Any(op) => quantified(op, AggFn::BoolOr),
        Shape::All(op) => quantified(op, AggFn::BoolAnd),
        Shape::Lateral => dep(Kind::Inner, body_plan(b, &[0, 1])),
        Shape::LeftLateral => dep(Kind::Left, body_plan(b, &[0, 1])),
    }
}

// ---------- random generation ----------
fn rand_v(rng: &mut Rng) -> V {
    if rng.below(5) == 0 {
        None
    } else {
        Some(rng.below(4) as i64)
    }
}
fn rand_table(rng: &mut Rng, max: u64) -> Vec<Row> {
    let n = rng.below(max + 1);
    (0..n).map(|_| vec![rand_v(rng), rand_v(rng)]).collect()
}
fn rand_op(rng: &mut Rng) -> Op {
    [Op::Eq, Op::Ne, Op::Lt, Op::Le, Op::Gt, Op::Ge][rng.below(6) as usize]
}
fn rand_agg(rng: &mut Rng) -> AggFn {
    [
        AggFn::CountStar,
        AggFn::Count,
        AggFn::Sum,
        AggFn::Min,
        AggFn::Max,
    ][rng.below(5) as usize]
}
fn rand_body(rng: &mut Rng) -> Body {
    let corr = match rng.below(9) {
        0..=4 => cmp(Op::Eq, col(0), outer(0)), // the common case: s.c = r.a
        5 => cmp(Op::Lt, col(0), outer(0)),     // inequality correlation
        6 => cmp(Op::Eq, col(1), outer(1)),     // s.d = r.b
        7 => cmp(Op::Gt, col(1), outer(1)),
        _ => cmp(Op::Eq, col(0), outer(1)),
    };
    Body {
        corr,
        local: (rng.below(3) == 0).then(|| rng.below(3) as i64),
        union2: (rng.below(5) == 0).then(|| cmp(Op::Eq, col(0), outer(1))),
        selfjoin: rng.below(5) == 0,
    }
}
fn rand_shape(rng: &mut Rng, i: usize) -> Shape {
    match i % 10 {
        0 => Shape::ScalarWhere(rand_agg(rng), rand_op(rng)),
        1 => Shape::ScalarSelect(rand_agg(rng)),
        2 => Shape::Exists,
        3 => Shape::NotExists,
        4 => Shape::In,
        5 => Shape::NotIn,
        6 => Shape::Any(rand_op(rng)),
        7 => Shape::All(rand_op(rng)),
        8 => Shape::Lateral,
        _ => Shape::LeftLateral,
    }
}

fn eq_body() -> Body {
    Body {
        corr: cmp(Op::Eq, col(0), outer(0)),
        local: None,
        union2: None,
        selfjoin: false,
    }
}
fn rows(v: &[[V; 2]]) -> Vec<Row> {
    v.iter().map(|r| r.to_vec()).collect()
}

// ---------- demos and checks ----------
fn demo_count_bug() {
    println!(
        "--- 1. the COUNT bug: SELECT * FROM R WHERE b = (SELECT COUNT(*) FROM S WHERE c = R.a) ---"
    );
    let db = Db {
        t: [
            rows(&[[Some(1), Some(0)], [Some(2), Some(0)], [Some(3), Some(0)]]),
            rows(&[[Some(1), Some(5)]]),
        ],
    };
    let naive = naive_plan(Shape::ScalarWhere(AggFn::CountStar, Op::Eq), &eq_body());
    show("naive plan (dependent join)", &naive);
    let good = unnested(&naive, true);
    show(
        "unnested plan (with COUNT fix: left join + CASE marker IS NULL THEN 0)",
        &good,
    );
    let buggy = unnested(&naive, false);
    let (n, wn) = run(&naive, &db);
    let (g, wg) = run(&good, &db);
    let (bad, _) = run(&buggy, &db);
    println!(
        "  naive    -> {n:?}  (rows processed {wn})\n  unnested -> {g:?}  (rows processed {wg})\n  no-fix   -> {bad:?}  <- rows 2 and 3 (count 0) are lost\n"
    );
    assert_eq!(n, rows(&[[Some(2), Some(0)], [Some(3), Some(0)]]));
    assert_eq!(g, n);
    assert_ne!(
        bad, n,
        "the COUNT bug must be reproducible with the fix disabled"
    );
}

fn demo_not_in() {
    println!(
        "--- 2. NOT IN with NULL: SELECT * FROM R WHERE b NOT IN (SELECT d FROM S WHERE c = R.a) ---"
    );
    let db = Db {
        t: [
            rows(&[[Some(1), Some(1)], [Some(2), Some(2)]]),
            rows(&[[Some(1), None]]),
        ],
    };
    let naive = naive_plan(Shape::NotIn, &eq_body());
    let good = unnested(&naive, true);
    show("unnested plan (BOOL_AND over b <> d, count-fixed)", &good);
    let (n, _) = run(&naive, &db);
    let (g, _) = run(&good, &db);
    // The tempting rewrite NOT EXISTS (... AND d = b) is NOT equivalent: it keeps (1,1) because UNKNOWN became TRUE.
    let wrong_body = Body {
        corr: And(vec![
            cmp(Op::Eq, col(0), outer(0)),
            cmp(Op::Eq, col(1), outer(1)),
        ]),
        ..eq_body()
    };
    let (w, _) = run(
        &unnested(&naive_plan(Shape::NotExists, &wrong_body), true),
        &db,
    );
    println!(
        "  NOT IN (3VL, naive)      -> {n:?}\n  NOT IN (unnested)        -> {g:?}\n  NOT EXISTS rewrite (bad) -> {w:?}  <- keeps (1,1): NULL made the predicate UNKNOWN, not TRUE\n"
    );
    assert_eq!(
        n,
        rows(&[[Some(2), Some(2)]]),
        "row (1,1): 1 NOT IN {{NULL}} is UNKNOWN; row (2,2): NOT IN (empty) is TRUE"
    );
    assert_eq!(g, n);
    assert_ne!(w, n);
    // IN over empty / NULL-only sets
    let (i, _) = run(&unnested(&naive_plan(Shape::In, &eq_body()), true), &db);
    assert!(
        i.is_empty(),
        "b IN {{NULL}} is UNKNOWN and b IN (empty) is FALSE"
    );
}

fn demo_exists_and_lateral() {
    println!(
        "--- 3. EXISTS -> semi join on the distinct domain; LATERAL with an inequality correlation ---"
    );
    let db = Db {
        t: [
            rows(&[[Some(1), Some(1)], [Some(1), Some(1)], [Some(2), Some(0)]]),
            rows(&[[Some(1), Some(7)], [Some(0), Some(3)]]),
        ],
    };
    let ex = naive_plan(Shape::Exists, &eq_body());
    show("naive EXISTS", &ex);
    show(
        "unnested EXISTS (the domain D deduplicates the duplicate outer rows)",
        &unnested(&ex, true),
    );
    let lat_body = Body {
        corr: cmp(Op::Lt, col(0), outer(0)),
        ..eq_body()
    };
    let lat = naive_plan(Shape::LeftLateral, &lat_body);
    let (n, _) = run(&lat, &db);
    let (u, _) = run(&unnested(&lat, true), &db);
    println!("  LEFT LATERAL (c < R.a): {u:?}\n");
    assert_eq!(n, u);
}

/// 600 random (database, query) pairs: the unnested plan must reproduce the naive result exactly.
fn differential(rng: &mut Rng) {
    println!("--- 4. differential test: 600 random databases x queries, all 10 shapes ---");
    let mut tally = [(0u32, 0u64, 0u64); 10];
    let (mut nonempty, mut nulls_seen, mut bug_differs) = (0, 0, 0);
    for i in 0..600 {
        let db = Db {
            t: [rand_table(rng, 8), rand_table(rng, 10)],
        };
        let shape = rand_shape(rng, i);
        let body = rand_body(rng);
        let naive = naive_plan(shape, &body);
        let good = unnested(&naive, true);
        assert!(!has_depjoin(&good), "dependent join survived for {shape:?}");
        let ((n, wn), (g, wg)) = (run(&naive, &db), run(&good, &db));
        assert_eq!(
            n,
            g,
            "mismatch for {shape:?} body corr {} local {:?} union {} selfjoin {}",
            body.corr,
            body.local,
            body.union2.is_some(),
            body.selfjoin
        );
        let (b, _) = run(&unnested(&naive, false), &db);
        if b != n {
            bug_differs += 1;
        }
        nonempty += !n.is_empty() as u32;
        nulls_seen += n.iter().any(|r| r.contains(&None)) as u32;
        let t = &mut tally[i % 10];
        *t = (t.0 + 1, t.1 + wn, t.2 + wg);
    }
    println!(
        "  {:<14}{:>8}{:>16}{:>16}",
        "shape", "queries", "naive rows", "unnested rows"
    );
    for (name, (q, wn, wg)) in SHAPE_NAMES.iter().zip(tally) {
        println!("  {name:<14}{q:>8}{wn:>16}{wg:>16}");
    }
    println!(
        "  non-empty results: {nonempty}/600, results containing NULL: {nulls_seen}, queries where the no-COUNT-fix variant is WRONG: {bug_differs}\n"
    );
    assert!(
        nonempty > 300 && nulls_seen > 50,
        "the generator must exercise real results and NULLs"
    );
    assert!(
        bug_differs > 0,
        "disabling the COUNT fix must be caught by at least one random query"
    );
}

/// Same shapes on a larger database: re-execution costs |R| x cost(S); the unnested plan hashes once.
fn scale_demo(rng: &mut Rng) {
    println!(
        "--- 5. cost: rows processed, naive re-execution vs unnested (|R| = 300, |S| = 600, equality correlation) ---"
    );
    let big = |rng: &mut Rng, n: usize| -> Vec<Row> {
        (0..n)
            .map(|_| vec![Some(rng.below(60) as i64), Some(rng.below(60) as i64)])
            .collect()
    };
    let db = Db {
        t: [big(rng, 300), big(rng, 600)],
    };
    let shapes = [
        Shape::ScalarWhere(AggFn::CountStar, Op::Gt),
        Shape::ScalarSelect(AggFn::Sum),
        Shape::Exists,
        Shape::NotExists,
        Shape::In,
        Shape::NotIn,
        Shape::Any(Op::Lt),
        Shape::All(Op::Ge),
        Shape::Lateral,
        Shape::LeftLateral,
    ];
    println!(
        "  {:<14}{:>14}{:>16}{:>10}",
        "shape", "naive rows", "unnested rows", "ratio"
    );
    for (name, shape) in SHAPE_NAMES.iter().zip(shapes) {
        let naive = naive_plan(shape, &eq_body());
        let ((n, wn), (g, wg)) = (run(&naive, &db), run(&unnested(&naive, true), &db));
        assert_eq!(n, g, "scaled mismatch for {name}");
        println!(
            "  {name:<14}{wn:>14}{wg:>16}{:>9.1}x",
            wn as f64 / wg as f64
        );
        // Lateral output is identical in size for both, so require the saving only where X is the dominating cost.
        assert!(wg < wn, "unnesting should process fewer rows for {name}");
    }
    println!();
}

fn main() {
    let mut rng = Rng::new(0x5eed_0752);
    demo_count_bug();
    demo_not_in();
    demo_exists_and_lateral();
    differential(&mut rng);
    scale_demo(&mut rng);
    println!("all subquery unnesting checks passed");
}
