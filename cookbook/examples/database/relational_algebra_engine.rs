//! RECIPE 723 (database): relational model and algebra: bags vs sets, operators, a typed expression evaluator, equivalence rewrites.
//! LEVEL: intermediate
//! Use in: SQL planners and optimizers, dataframe query layers, rule-based rewrite engines, teaching how a query becomes a plan.   Run: cargo run --release --example relational_algebra_engine
//! COMMON USES: the logical-plan layer of every SQL engine (PostgreSQL's parse tree -> Query -> Plan, Apache Calcite's RelNode
//! with rewrite rules, Spark Catalyst's logical optimizer, DataFusion's LogicalPlan): predicate pushdown, projection pushdown,
//! join reordering and "cross product + filter -> join" are exactly the rules implemented below.
//! Course: CMU 15-445 Lecture 01 (relational model and relational algebra); the rewrites also prepare 15-445's query optimization lectures.
#![allow(dead_code)]
//!
//! WHAT: a relation is a SCHEMA (named, typed columns) plus a multiset of rows. SQL works on BAGS (duplicates are kept); the
//! textbook algebra works on SETS; `Distinct` (delta) converts. Operators (SQL in brackets):
//! * select sigma_p(R)        [WHERE p]            * project pi_A(R)       [SELECT A: bag projection keeps duplicates]
//! * rename rho               [AS]                 * UNION ALL / INTERSECT / EXCEPT with bag counts (sum, min, monus)
//! * cross product R x S      [FROM R, S]          * theta join R |x|_p S  [JOIN ... ON p]
//! * natural join R |x| S     [NATURAL JOIN]       * semi join (EXISTS) and anti join (NOT EXISTS)
//! * division R / S           [relational "for all": a such that (a,b) in R for EVERY b in S]
//! * grouping/aggregation gamma_{G; f(x)}(R) [GROUP BY G]. A global aggregate over an EMPTY input returns no row here (SQL
//!   returns one row of COUNT = 0); everything else follows SQL.
//! Expressions are a small typed language (Int, Bool, Str) with a type CHECKER (`a + 'x'` and `a = 'x'` are rejected before
//! execution, as a planner's binder would) and an evaluator.
//!
//! SQL to algebra, by hand:
//!   SELECT a, d FROM R, U, T WHERE b = x AND y = c AND a < 2 AND d >= 3
//!     = pi_{a,d}( sigma_{b=x AND y=c AND a<2 AND d>=3}( (R x U) x T ) )          <- the naive plan, huge intermediate result
//!     = pi_{a,d}( (R' |x|_{b=x} U) |x|_{y=c} T' )  with R' = sigma_{a<2}(R), T' = sigma_{d>=3}(T)   <- after the rewrites below
//!   SELECT DISTINCT b FROM R           = delta(pi_b(R))          SELECT b FROM R   = pi_b(R)           (bag: differs when b repeats)
//!   SELECT a FROM R WHERE NOT EXISTS (SELECT 1 FROM S WHERE S.b = R.b)  = R anti-join_{R.b = S.b} S
//!   SELECT a, COUNT(*) FROM R GROUP BY a                 = gamma_{a; count(*)}(R)
//!
//! EQUIVALENCE RULES (each is a function Plan -> Option<Plan> that fires at the ROOT of the plan it is given):
//!   cascade   sigma_{p AND q}(R)  =  sigma_p(sigma_q(R))        merge  is the reverse
//!   commute   sigma_p(sigma_q(R)) =  sigma_q(sigma_p(R))
//!   pushdown  sigma_p(R x S) = sigma_{p_R}(R) x sigma_{p_S}(S) when each conjunct only mentions one side (also for theta/natural join)
//!   cross2join  sigma_p(R x S) = R |x|_p S
//!   join commutativity  R |x| S = S |x| R      associativity  (R |x| S) |x| T = R |x| (S |x| T)   (columns reorder; compared by name)
//!   projection pushdown  pi_A(R |x| S) = pi_A( pi_{A_R + join cols}(R) |x| pi_{A_S + join cols}(S) )   (join columns must survive)
//!   sigma through gamma  sigma_p(gamma_G(R)) = gamma_G(sigma_p(R))  when p only mentions grouping columns
//! WHY the optimizer wants them: selection and projection shrink inputs BEFORE the expensive operator; join order and
//! cross2join turn a quadratic cross product into a keyed join. The demo prints the plan tree before/after and the total number
//! of rows every operator produced (a proxy for work).
//!
//! BAG vs SET: rewrites are verified under BAG semantics (multiset equality), the strict one. Some laws hold only for sets:
//! the demo shows R INTERSECT S == R natural-join S when both have the same schema holds for SETS but not for bags (a
//! natural join multiplies duplicate counts, intersection takes the minimum) and that projection/union change counts.
//!
//! CHECKS: 500 random database instances (empty relations, duplicates, tiny value domains so joins match often): every rewrite
//! fires on shaped plans and preserves the result (canonical form: columns sorted by name, rows sorted); operators are checked
//! against independent naive implementations (nested-loop join, BTreeMap grouping, remove-first-match bag ops); division equals its
//! textbook derivation pi_A(R) - pi_A((pi_A(R) x S) - R); type errors are rejected; the optimizer's output equals the naive plan.
//! Read: Relational algebra https://en.wikipedia.org/wiki/Relational_algebra   Codd, A Relational Model of Data for Large Shared Data Banks https://www.seas.upenn.edu/~zives/03f/cis550/codd.pdf
//!       CMU 15-445 https://15445.courses.cs.cmu.edu/fall2024/

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

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

// ---------------------------------------------------------------- values, types, relations
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Type {
    Int,
    Bool,
    Str,
}
type Schema = Vec<(String, Type)>;
type Row = Vec<Value>;
type Db = HashMap<String, Relation>;
type Catalog = HashMap<String, Schema>;

#[derive(Clone, Debug)]
struct Relation {
    schema: Schema,
    rows: Vec<Row>, // a BAG: duplicates allowed, order irrelevant
}
impl Relation {
    fn distinct(&self) -> Relation {
        let mut seen = BTreeSet::new();
        let rows = self
            .rows
            .iter()
            .filter(|r| seen.insert((*r).clone()))
            .cloned()
            .collect();
        Relation {
            schema: self.schema.clone(),
            rows,
        }
    }
    /// Canonical form for bag equality: columns sorted by name, then rows sorted. Equal canon <=> same bag up to column order.
    fn canon(&self) -> (Schema, Vec<Row>) {
        let mut idx: Vec<usize> = (0..self.schema.len()).collect();
        idx.sort_by(|&a, &b| self.schema[a].0.cmp(&self.schema[b].0));
        let schema = idx.iter().map(|&i| self.schema[i].clone()).collect();
        let mut rows: Vec<Row> = self
            .rows
            .iter()
            .map(|r| idx.iter().map(|&i| r[i].clone()).collect())
            .collect();
        rows.sort();
        (schema, rows)
    }
}
fn pos(schema: &Schema, name: &str) -> Result<usize, String> {
    schema
        .iter()
        .position(|(n, _)| n == name)
        .ok_or_else(|| format!("unknown column {name}"))
}

// ---------------------------------------------------------------- typed expressions
#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Add,
    Sub,
}
#[derive(Clone, Debug)]
enum Expr {
    Col(String),
    Lit(Value),
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
}
fn col(s: &str) -> Expr {
    Expr::Col(s.into())
}
fn int(i: i64) -> Expr {
    Expr::Lit(Value::Int(i))
}
fn text(s: &str) -> Expr {
    Expr::Lit(Value::Str(s.into()))
}
fn bin(op: Op, a: Expr, b: Expr) -> Expr {
    Expr::Bin(op, Box::new(a), Box::new(b))
}
fn and(a: Expr, b: Expr) -> Expr {
    bin(Op::And, a, b)
}

impl Expr {
    fn cols(&self, out: &mut BTreeSet<String>) {
        match self {
            Expr::Col(c) => {
                out.insert(c.clone());
            }
            Expr::Lit(_) => {}
            Expr::Not(e) => e.cols(out),
            Expr::Bin(_, a, b) => {
                a.cols(out);
                b.cols(out);
            }
        }
    }
    fn col_set(&self) -> BTreeSet<String> {
        let mut s = BTreeSet::new();
        self.cols(&mut s);
        s
    }
    fn conjuncts(self) -> Vec<Expr> {
        match self {
            Expr::Bin(Op::And, a, b) => a.conjuncts().into_iter().chain(b.conjuncts()).collect(),
            e => vec![e],
        }
    }
    fn typecheck(&self, schema: &Schema) -> Result<Type, String> {
        Ok(match self {
            Expr::Col(c) => schema[pos(schema, c)?].1,
            Expr::Lit(Value::Int(_)) => Type::Int,
            Expr::Lit(Value::Bool(_)) => Type::Bool,
            Expr::Lit(Value::Str(_)) => Type::Str,
            Expr::Not(e) => match e.typecheck(schema)? {
                Type::Bool => Type::Bool,
                t => return Err(format!("NOT needs Bool, got {t:?}")),
            },
            Expr::Bin(op, a, b) => {
                let (ta, tb) = (a.typecheck(schema)?, b.typecheck(schema)?);
                match op {
                    Op::And | Op::Or if ta == Type::Bool && tb == Type::Bool => Type::Bool,
                    Op::Add | Op::Sub if ta == Type::Int && tb == Type::Int => Type::Int,
                    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge if ta == tb => Type::Bool,
                    _ => return Err(format!("operator {op:?} cannot take {ta:?} and {tb:?}")),
                }
            }
        })
    }
    fn eval(&self, schema: &Schema, row: &[Value]) -> Value {
        match self {
            Expr::Col(c) => row[pos(schema, c).expect("typechecked")].clone(),
            Expr::Lit(v) => v.clone(),
            Expr::Not(e) => match e.eval(schema, row) {
                Value::Bool(b) => Value::Bool(!b),
                v => panic!("NOT {v:?}"),
            },
            Expr::Bin(op, a, b) => {
                let (x, y) = (a.eval(schema, row), b.eval(schema, row));
                match (op, &x, &y) {
                    (Op::And, Value::Bool(p), Value::Bool(q)) => Value::Bool(*p && *q),
                    (Op::Or, Value::Bool(p), Value::Bool(q)) => Value::Bool(*p || *q),
                    (Op::Add, Value::Int(p), Value::Int(q)) => Value::Int(p.wrapping_add(*q)),
                    (Op::Sub, Value::Int(p), Value::Int(q)) => Value::Int(p.wrapping_sub(*q)),
                    (Op::Eq, ..) => Value::Bool(x == y),
                    (Op::Ne, ..) => Value::Bool(x != y),
                    (Op::Lt, ..) => Value::Bool(x < y),
                    (Op::Le, ..) => Value::Bool(x <= y),
                    (Op::Gt, ..) => Value::Bool(x > y),
                    (Op::Ge, ..) => Value::Bool(x >= y),
                    _ => panic!("ill-typed {op:?} {x:?} {y:?}"),
                }
            }
        }
    }
    fn show(&self, top: bool) -> String {
        match self {
            Expr::Col(c) => c.clone(),
            Expr::Lit(Value::Int(i)) => i.to_string(),
            Expr::Lit(Value::Bool(b)) => b.to_string(),
            Expr::Lit(Value::Str(s)) => format!("'{s}'"),
            Expr::Not(e) => format!("NOT {}", e.show(false)),
            Expr::Bin(op, a, b) => {
                let s = match op {
                    Op::Eq => "=",
                    Op::Ne => "<>",
                    Op::Lt => "<",
                    Op::Le => "<=",
                    Op::Gt => ">",
                    Op::Ge => ">=",
                    Op::And => "AND",
                    Op::Or => "OR",
                    Op::Add => "+",
                    Op::Sub => "-",
                };
                let body = format!("{} {s} {}", a.show(false), b.show(false));
                if top { body } else { format!("({body})") }
            }
        }
    }
}
fn conj(mut es: Vec<Expr>) -> Option<Expr> {
    let last = es.pop()?;
    Some(es.into_iter().rev().fold(last, |acc, e| and(e, acc)))
}

// ---------------------------------------------------------------- plans
#[derive(Clone, Copy, Debug, PartialEq)]
enum AggFn {
    Count,
    Sum,
    Min,
    Max,
}
#[derive(Clone, Debug)]
enum Plan {
    Scan(String),
    Select(Expr, Box<Plan>),
    Project(Vec<String>, Box<Plan>),
    Rename(Vec<(String, String)>, Box<Plan>),
    Distinct(Box<Plan>),
    UnionAll(Box<Plan>, Box<Plan>),
    Intersect(Box<Plan>, Box<Plan>),
    Except(Box<Plan>, Box<Plan>),
    Cross(Box<Plan>, Box<Plan>),
    Join(Expr, Box<Plan>, Box<Plan>),
    Natural(Box<Plan>, Box<Plan>),
    Semi(Expr, Box<Plan>, Box<Plan>),
    Anti(Expr, Box<Plan>, Box<Plan>),
    Divide(Box<Plan>, Box<Plan>),
    Aggregate {
        group: Vec<String>,
        aggs: Vec<(AggFn, String, String)>,
        input: Box<Plan>,
    }, // (fn, input col, output name)
}
use Plan::*;

fn b(p: Plan) -> Box<Plan> {
    Box::new(p)
}
fn scan(n: &str) -> Plan {
    Scan(n.into())
}
fn select(e: Expr, p: Plan) -> Plan {
    Select(e, b(p))
}
fn project(cols: &[&str], p: Plan) -> Plan {
    Project(cols.iter().map(|c| c.to_string()).collect(), b(p))
}
fn cross(l: Plan, r: Plan) -> Plan {
    Cross(b(l), b(r))
}
fn natural(l: Plan, r: Plan) -> Plan {
    Natural(b(l), b(r))
}

impl Plan {
    fn children(&self) -> Vec<&Plan> {
        match self {
            Scan(_) => vec![],
            Select(_, p) | Project(_, p) | Rename(_, p) | Distinct(p) => vec![p],
            Aggregate { input, .. } => vec![input],
            UnionAll(l, r)
            | Intersect(l, r)
            | Except(l, r)
            | Cross(l, r)
            | Join(_, l, r)
            | Natural(l, r)
            | Semi(_, l, r)
            | Anti(_, l, r)
            | Divide(l, r) => vec![l, r],
        }
    }
    /// Rebuild this node with each child replaced by `f(child)`.
    fn map_children(&self, f: &mut dyn FnMut(&Plan) -> Plan) -> Plan {
        match self {
            Scan(n) => Scan(n.clone()),
            Select(e, p) => Select(e.clone(), b(f(p))),
            Project(c, p) => Project(c.clone(), b(f(p))),
            Rename(m, p) => Rename(m.clone(), b(f(p))),
            Distinct(p) => Distinct(b(f(p))),
            Aggregate { group, aggs, input } => Aggregate {
                group: group.clone(),
                aggs: aggs.clone(),
                input: b(f(input)),
            },
            UnionAll(l, r) => UnionAll(b(f(l)), b(f(r))),
            Intersect(l, r) => Intersect(b(f(l)), b(f(r))),
            Except(l, r) => Except(b(f(l)), b(f(r))),
            Cross(l, r) => Cross(b(f(l)), b(f(r))),
            Join(e, l, r) => Join(e.clone(), b(f(l)), b(f(r))),
            Natural(l, r) => Natural(b(f(l)), b(f(r))),
            Semi(e, l, r) => Semi(e.clone(), b(f(l)), b(f(r))),
            Anti(e, l, r) => Anti(e.clone(), b(f(l)), b(f(r))),
            Divide(l, r) => Divide(b(f(l)), b(f(r))),
        }
    }
    fn label(&self) -> String {
        match self {
            Scan(n) => n.clone(),
            Select(e, _) => format!("σ[{}]", e.show(true)),
            Project(c, _) => format!("π[{}]", c.join(",")),
            Rename(m, _) => format!(
                "ρ[{}]",
                m.iter()
                    .map(|(a, b)| format!("{a}→{b}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Distinct(_) => "δ".into(),
            UnionAll(..) => "∪ALL".into(),
            Intersect(..) => "∩".into(),
            Except(..) => "−".into(),
            Cross(..) => "×".into(),
            Join(e, ..) => format!("⋈[{}]", e.show(true)),
            Natural(..) => "⋈".into(),
            Semi(e, ..) => format!("⋉[{}]", e.show(true)),
            Anti(e, ..) => format!("▷[{}]", e.show(true)),
            Divide(..) => "÷".into(),
            Aggregate { group, aggs, .. } => {
                let a: Vec<String> = aggs
                    .iter()
                    .map(|(f, c, o)| format!("{f:?}({c})→{o}").to_lowercase())
                    .collect();
                format!("γ[{}; {}]", group.join(","), a.join(", "))
            }
        }
    }
    /// Indented tree printer: one node per line, children below with a branch marker.
    fn tree(&self) -> String {
        fn go(p: &Plan, depth: usize, out: &mut String) {
            let _ = writeln!(
                out,
                "{}{}{}",
                "  ".repeat(depth),
                if depth > 0 { "└ " } else { "" },
                p.label()
            );
            for c in p.children() {
                go(c, depth + 1, out);
            }
        }
        let mut s = String::new();
        go(self, 0, &mut s);
        s
    }

    /// Output schema, with full type checking of every predicate.
    fn schema(&self, cat: &Catalog) -> Result<Schema, String> {
        let same = |l: &Schema, r: &Schema| {
            if l == r {
                Ok(l.clone())
            } else {
                Err(format!("schema mismatch {l:?} vs {r:?}"))
            }
        };
        let concat = |l: Schema, r: Schema| -> Result<Schema, String> {
            if let Some((n, _)) = l.iter().find(|(n, _)| r.iter().any(|(m, _)| m == n)) {
                return Err(format!("duplicate column {n}; rename first"));
            }
            Ok(l.into_iter().chain(r).collect())
        };
        match self {
            Scan(n) => cat.get(n).cloned().ok_or(format!("unknown table {n}")),
            Select(e, p) => {
                let s = p.schema(cat)?;
                (e.typecheck(&s)? == Type::Bool)
                    .then_some(s)
                    .ok_or("predicate must be Bool".into())
            }
            Project(cs, p) => {
                let s = p.schema(cat)?;
                cs.iter().map(|c| Ok(s[pos(&s, c)?].clone())).collect()
            }
            Rename(m, p) => {
                let mut s = p.schema(cat)?;
                for (old, new) in m {
                    let i = pos(&s, old)?;
                    s[i].0 = new.clone();
                }
                Ok(s)
            }
            Distinct(p) => p.schema(cat),
            UnionAll(l, r) | Intersect(l, r) | Except(l, r) => {
                same(&l.schema(cat)?, &r.schema(cat)?)
            }
            Cross(l, r) => concat(l.schema(cat)?, r.schema(cat)?),
            Join(e, l, r) | Semi(e, l, r) | Anti(e, l, r) => {
                let (ls, rs) = (l.schema(cat)?, r.schema(cat)?);
                let both = concat(ls.clone(), rs)?;
                if e.typecheck(&both)? != Type::Bool {
                    return Err("join predicate must be Bool".into());
                }
                Ok(if matches!(self, Join(..)) { both } else { ls })
            }
            Natural(l, r) => {
                let (ls, rs) = (l.schema(cat)?, r.schema(cat)?);
                for (n, t) in &rs {
                    if let Some((_, lt)) = ls.iter().find(|(m, _)| m == n) {
                        if lt != t {
                            return Err(format!("shared column {n} has two types"));
                        }
                    }
                }
                Ok(ls
                    .iter()
                    .cloned()
                    .chain(
                        rs.iter()
                            .filter(|(n, _)| !ls.iter().any(|(m, _)| m == n))
                            .cloned(),
                    )
                    .collect())
            }
            Divide(l, r) => {
                let (ls, rs) = (l.schema(cat)?, r.schema(cat)?);
                for c in &rs {
                    if !ls.contains(c) {
                        return Err(format!("divisor column {} missing in dividend", c.0));
                    }
                }
                Ok(ls.into_iter().filter(|c| !rs.contains(c)).collect())
            }
            Aggregate { group, aggs, input } => {
                let s = input.schema(cat)?;
                let mut out: Schema = group
                    .iter()
                    .map(|g| Ok(s[pos(&s, g)?].clone()))
                    .collect::<Result<_, String>>()?;
                for (f, c, name) in aggs {
                    let t = if *f == AggFn::Count {
                        Type::Int
                    } else {
                        s[pos(&s, c)?].1
                    };
                    if *f == AggFn::Sum && t != Type::Int {
                        return Err("SUM needs Int".into());
                    }
                    out.push((name.clone(), if *f == AggFn::Count { Type::Int } else { t }));
                }
                Ok(out)
            }
        }
    }
}

// ---------------------------------------------------------------- evaluator (bag semantics)
fn counts(rows: &[Row]) -> HashMap<Row, usize> {
    let mut m = HashMap::new();
    for r in rows {
        *m.entry(r.clone()).or_insert(0) += 1;
    }
    m
}

/// `work` accumulates the number of rows every operator emitted: a crude but honest cost proxy.
fn eval(plan: &Plan, db: &Db, work: &mut u64) -> Relation {
    let rel = match plan {
        Scan(n) => db[n].clone(),
        Select(e, p) => {
            let r = eval(p, db, work);
            let rows = r
                .rows
                .iter()
                .filter(|row| e.eval(&r.schema, row) == Value::Bool(true))
                .cloned()
                .collect();
            Relation {
                schema: r.schema,
                rows,
            }
        }
        Project(cs, p) => {
            let r = eval(p, db, work);
            let idx: Vec<usize> = cs.iter().map(|c| pos(&r.schema, c).unwrap()).collect();
            Relation {
                schema: idx.iter().map(|&i| r.schema[i].clone()).collect(),
                rows: r
                    .rows
                    .iter()
                    .map(|row| idx.iter().map(|&i| row[i].clone()).collect())
                    .collect(),
            }
        }
        Rename(m, p) => {
            let mut r = eval(p, db, work);
            for (old, new) in m {
                let i = pos(&r.schema, old).unwrap();
                r.schema[i].0 = new.clone();
            }
            r
        }
        Distinct(p) => eval(p, db, work).distinct(),
        UnionAll(l, r) => {
            let (mut a, c) = (eval(l, db, work), eval(r, db, work));
            a.rows.extend(c.rows);
            a
        }
        Intersect(l, r) | Except(l, r) => {
            let (a, c) = (eval(l, db, work), eval(r, db, work));
            let mut right = counts(&c.rows);
            let take_if_present = matches!(plan, Intersect(..)); // min(m,n) for INTERSECT, max(m-n,0) for EXCEPT
            let rows = a
                .rows
                .into_iter()
                .filter(|row| match right.get_mut(row) {
                    Some(k) if *k > 0 => {
                        *k -= 1;
                        take_if_present
                    }
                    _ => !take_if_present,
                })
                .collect();
            Relation {
                schema: a.schema,
                rows,
            }
        }
        Cross(l, r) => {
            let (a, c) = (eval(l, db, work), eval(r, db, work));
            let rows = a
                .rows
                .iter()
                .flat_map(|x| {
                    c.rows
                        .iter()
                        .map(move |y| x.iter().chain(y).cloned().collect())
                })
                .collect();
            Relation {
                schema: a.schema.into_iter().chain(c.schema).collect(),
                rows,
            }
        }
        Join(e, l, r) => {
            let (a, c) = (eval(l, db, work), eval(r, db, work));
            let schema: Schema = a
                .schema
                .iter()
                .cloned()
                .chain(c.schema.iter().cloned())
                .collect();
            let mut rows = vec![];
            for x in &a.rows {
                for y in &c.rows {
                    let row: Row = x.iter().chain(y).cloned().collect();
                    if e.eval(&schema, &row) == Value::Bool(true) {
                        rows.push(row);
                    }
                }
            }
            Relation { schema, rows }
        }
        Semi(e, l, r) | Anti(e, l, r) => {
            let (a, c) = (eval(l, db, work), eval(r, db, work));
            let both: Schema = a
                .schema
                .iter()
                .cloned()
                .chain(c.schema.iter().cloned())
                .collect();
            let want = matches!(plan, Semi(..));
            let rows = a
                .rows
                .iter()
                .filter(|x| {
                    c.rows.iter().any(|y| {
                        e.eval(&both, &x.iter().chain(y).cloned().collect::<Vec<_>>())
                            == Value::Bool(true)
                    }) == want
                })
                .cloned()
                .collect();
            Relation {
                schema: a.schema,
                rows,
            }
        }
        Natural(l, r) => {
            let (a, c) = (eval(l, db, work), eval(r, db, work));
            let shared: Vec<(usize, usize)> = a
                .schema
                .iter()
                .enumerate()
                .filter_map(|(i, (n, _))| pos(&c.schema, n).ok().map(|j| (i, j)))
                .collect();
            let extra: Vec<usize> = (0..c.schema.len())
                .filter(|j| !shared.iter().any(|s| s.1 == *j))
                .collect();
            let mut table: HashMap<Row, Vec<&Row>> = HashMap::new(); // hash join on the shared columns
            for y in &c.rows {
                table
                    .entry(shared.iter().map(|&(_, j)| y[j].clone()).collect())
                    .or_default()
                    .push(y);
            }
            let mut rows = vec![];
            for x in &a.rows {
                let key: Row = shared.iter().map(|&(i, _)| x[i].clone()).collect();
                for y in table.get(&key).into_iter().flatten() {
                    rows.push(
                        x.iter()
                            .cloned()
                            .chain(extra.iter().map(|&j| y[j].clone()))
                            .collect(),
                    );
                }
            }
            Relation {
                schema: a
                    .schema
                    .iter()
                    .cloned()
                    .chain(extra.iter().map(|&j| c.schema[j].clone()))
                    .collect(),
                rows,
            }
        }
        Divide(l, r) => {
            // Set semantics: group the dividend by the kept columns; keep a group iff its divisor-part set covers the divisor.
            let (a, c) = (eval(l, db, work), eval(r, db, work));
            let div_idx: Vec<usize> = c
                .schema
                .iter()
                .map(|(n, _)| pos(&a.schema, n).unwrap())
                .collect();
            let keep: Vec<usize> = (0..a.schema.len())
                .filter(|i| !div_idx.contains(i))
                .collect();
            let need: BTreeSet<Row> = c.rows.iter().cloned().collect();
            let mut groups: BTreeMap<Row, BTreeSet<Row>> = BTreeMap::new();
            for x in &a.rows {
                groups
                    .entry(keep.iter().map(|&i| x[i].clone()).collect())
                    .or_default()
                    .insert(div_idx.iter().map(|&i| x[i].clone()).collect());
            }
            let rows = groups
                .into_iter()
                .filter(|(_, have)| need.is_subset(have))
                .map(|(k, _)| k)
                .collect();
            Relation {
                schema: keep.iter().map(|&i| a.schema[i].clone()).collect(),
                rows,
            }
        }
        Aggregate { group, aggs, input } => {
            let r = eval(input, db, work);
            let gi: Vec<usize> = group.iter().map(|g| pos(&r.schema, g).unwrap()).collect();
            let mut groups: BTreeMap<Row, Vec<&Row>> = BTreeMap::new();
            for row in &r.rows {
                groups
                    .entry(gi.iter().map(|&i| row[i].clone()).collect())
                    .or_default()
                    .push(row);
            }
            let schema = plan
                .schema(
                    &db.iter()
                        .map(|(k, v)| (k.clone(), v.schema.clone()))
                        .collect(),
                )
                .unwrap();
            let mut rows = vec![];
            for (key, members) in groups {
                let mut out = key;
                for (f, c, _) in aggs {
                    let vals: Vec<&Value> = if *f == AggFn::Count {
                        vec![]
                    } else {
                        let i = pos(&r.schema, c).unwrap();
                        members.iter().map(|m| &m[i]).collect()
                    };
                    out.push(match f {
                        AggFn::Count => Value::Int(members.len() as i64),
                        AggFn::Sum => Value::Int(
                            vals.iter()
                                .map(|v| if let Value::Int(i) = v { *i } else { 0 })
                                .sum(),
                        ),
                        AggFn::Min => (*vals.iter().min().unwrap()).clone(),
                        AggFn::Max => (*vals.iter().max().unwrap()).clone(),
                    });
                }
                rows.push(out);
            }
            Relation { schema, rows }
        }
    };
    *work += rel.rows.len() as u64;
    rel
}

fn run(plan: &Plan, db: &Db) -> Relation {
    eval(plan, db, &mut 0)
}
fn catalog(db: &Db) -> Catalog {
    db.iter()
        .map(|(k, v)| (k.clone(), v.schema.clone()))
        .collect()
}

// ---------------------------------------------------------------- rewrite rules
type Rule = fn(&Plan, &Catalog) -> Option<Plan>;

fn schema_names(p: &Plan, cat: &Catalog) -> BTreeSet<String> {
    p.schema(cat).unwrap().into_iter().map(|c| c.0).collect()
}

fn rule_cascade(p: &Plan, _: &Catalog) -> Option<Plan> {
    let Select(Expr::Bin(Op::And, x, y), input) = p else {
        return None;
    };
    Some(select(
        (**x).clone(),
        select((**y).clone(), (**input).clone()),
    ))
}
fn rule_merge(p: &Plan, _: &Catalog) -> Option<Plan> {
    let Select(x, inner) = p else { return None };
    let Select(y, input) = &**inner else {
        return None;
    };
    Some(select(and(x.clone(), y.clone()), (**input).clone()))
}
fn rule_commute_select(p: &Plan, _: &Catalog) -> Option<Plan> {
    let Select(x, inner) = p else { return None };
    let Select(y, input) = &**inner else {
        return None;
    };
    Some(select(y.clone(), select(x.clone(), (**input).clone())))
}
/// sigma_p over a cross / theta / natural join: route each conjunct to the side that has all of its columns.
fn rule_push_select(p: &Plan, cat: &Catalog) -> Option<Plan> {
    let Select(pred, inner) = p else { return None };
    let (l, r) = match &**inner {
        Cross(l, r) | Join(_, l, r) | Natural(l, r) => (l, r),
        _ => return None,
    };
    let (ls, rs) = (schema_names(l, cat), schema_names(r, cat));
    let (mut to_l, mut to_r, mut stay) = (vec![], vec![], vec![]);
    for c in pred.clone().conjuncts() {
        let cs = c.col_set();
        if cs.is_subset(&ls) {
            to_l.push(c);
        } else if cs.is_subset(&rs) {
            to_r.push(c);
        } else {
            stay.push(c);
        }
    }
    if to_l.is_empty() && to_r.is_empty() {
        return None;
    }
    let wrap =
        |es: Vec<Expr>, side: &Plan| conj(es).map_or(side.clone(), |e| select(e, side.clone()));
    let (nl, nr) = (wrap(to_l, l), wrap(to_r, r));
    let joined = match &**inner {
        Cross(..) => Cross(b(nl), b(nr)),
        Join(e, ..) => Join(e.clone(), b(nl), b(nr)),
        _ => Natural(b(nl), b(nr)),
    };
    Some(conj(stay).map_or(joined.clone(), |e| select(e, joined)))
}
fn rule_cross_to_join(p: &Plan, _: &Catalog) -> Option<Plan> {
    let Select(e, inner) = p else { return None };
    let Cross(l, r) = &**inner else { return None };
    Some(Join(e.clone(), l.clone(), r.clone()))
}
fn rule_join_commute(p: &Plan, _: &Catalog) -> Option<Plan> {
    match p {
        Cross(l, r) => Some(Cross(r.clone(), l.clone())),
        Natural(l, r) => Some(Natural(r.clone(), l.clone())),
        Join(e, l, r) => Some(Join(e.clone(), r.clone(), l.clone())),
        _ => None,
    }
}
fn rule_join_assoc(p: &Plan, _: &Catalog) -> Option<Plan> {
    match p {
        Cross(ab, c) => match &**ab {
            Cross(a, bb) => Some(Cross(a.clone(), b(Cross(bb.clone(), c.clone())))),
            _ => None,
        },
        Natural(ab, c) => match &**ab {
            Natural(a, bb) => Some(Natural(a.clone(), b(Natural(bb.clone(), c.clone())))),
            _ => None,
        },
        _ => None,
    }
}
/// pi_A(L |x| R): each input only needs its own wanted columns plus the join columns.
fn rule_push_project(p: &Plan, cat: &Catalog) -> Option<Plan> {
    let Project(want, inner) = p else { return None };
    let (l, r) = match &**inner {
        Cross(l, r) | Natural(l, r) => (l, r),
        _ => return None,
    };
    let (ls, rs) = (l.schema(cat).ok()?, r.schema(cat).ok()?);
    let shared: BTreeSet<String> = ls
        .iter()
        .filter(|c| rs.iter().any(|d| d.0 == c.0))
        .map(|c| c.0.clone())
        .collect();
    let keep = |s: &Schema| -> Vec<String> {
        s.iter()
            .filter(|c| want.contains(&c.0) || shared.contains(&c.0))
            .map(|c| c.0.clone())
            .collect()
    };
    let (kl, kr) = (keep(&ls), keep(&rs));
    if kl.len() == ls.len() && kr.len() == rs.len() {
        return None; // nothing to prune: do not fire (prevents infinite rewriting)
    }
    let narrowed = |keep: Vec<String>, s: &Schema, side: &Plan| {
        if keep.len() == s.len() {
            side.clone()
        } else {
            Project(keep, b(side.clone()))
        }
    };
    let (nl, nr) = (narrowed(kl, &ls, l), narrowed(kr, &rs, r));
    let joined = if matches!(&**inner, Cross(..)) {
        Cross(b(nl), b(nr))
    } else {
        Natural(b(nl), b(nr))
    };
    Some(Project(want.clone(), b(joined)))
}
fn rule_select_through_agg(p: &Plan, _: &Catalog) -> Option<Plan> {
    let Select(e, inner) = p else { return None };
    let Aggregate { group, aggs, input } = &**inner else {
        return None;
    };
    e.col_set()
        .iter()
        .all(|c| group.contains(c))
        .then(|| Aggregate {
            group: group.clone(),
            aggs: aggs.clone(),
            input: b(select(e.clone(), (**input).clone())),
        })
}

/// Apply rules bottom-up until nothing fires (capped: a buggy rule set that cycles would otherwise hang).
fn optimize(plan: &Plan, cat: &Catalog, rules: &[Rule]) -> Plan {
    fn go(p: &Plan, cat: &Catalog, rules: &[Rule], fuel: &mut u32) -> Plan {
        let p = p.map_children(&mut |c| go(c, cat, rules, fuel));
        for rule in rules {
            if *fuel == 0 {
                return p;
            }
            if let Some(q) = rule(&p, cat) {
                *fuel -= 1;
                return go(&q, cat, rules, fuel); // the new node's children may enable further rules
            }
        }
        p
    }
    let mut fuel = 200;
    go(plan, cat, rules, &mut fuel)
}

// ---------------------------------------------------------------- random data
fn random_db(rng: &mut Rng, max_rows: u64, dom: u64) -> Db {
    let mut db = Db::new();
    let mut table = |name: &str, cols: &[&str]| {
        let schema: Schema = cols.iter().map(|c| (c.to_string(), Type::Int)).collect();
        let rows = (0..rng.below(max_rows + 1))
            .map(|_| {
                cols.iter()
                    .map(|_| Value::Int(rng.below(dom) as i64))
                    .collect()
            })
            .collect();
        db.insert(name.into(), Relation { schema, rows });
    };
    table("R", &["a", "b"]);
    table("S", &["b", "c"]);
    table("T", &["c", "d"]);
    table("U", &["x", "y"]);
    db
}

fn random_cmp(rng: &mut Rng, c: &str, dom: u64) -> Expr {
    let op = [Op::Eq, Op::Ne, Op::Lt, Op::Le, Op::Gt, Op::Ge][rng.below(6) as usize];
    bin(op, col(c), int(rng.below(dom) as i64))
}

/// (rule name, rule, plan on which the rule must fire) for the current random instance.
fn shaped_cases(rng: &mut Rng, dom: u64) -> Vec<(&'static str, Rule, Plan)> {
    let k = |rng: &mut Rng, c: &str| random_cmp(rng, c, dom);
    let (p1, p2, p3, p4) = (k(rng, "a"), k(rng, "b"), k(rng, "c"), k(rng, "y"));
    vec![
        (
            "selection cascade",
            rule_cascade,
            select(and(p1.clone(), p2.clone()), scan("R")),
        ),
        (
            "selection merge",
            rule_merge,
            select(p1.clone(), select(p2.clone(), scan("R"))),
        ),
        (
            "selection commutation",
            rule_commute_select,
            select(p1.clone(), select(p2.clone(), scan("R"))),
        ),
        (
            "pushdown through cross",
            rule_push_select,
            select(
                and(and(p1.clone(), p4.clone()), bin(Op::Eq, col("a"), col("x"))),
                cross(scan("R"), scan("U")),
            ),
        ),
        (
            "pushdown through natural join",
            rule_push_select,
            select(and(p1.clone(), p3.clone()), natural(scan("R"), scan("S"))),
        ),
        (
            "pushdown through theta join",
            rule_push_select,
            select(
                p4.clone(),
                Join(bin(Op::Lt, col("a"), col("x")), b(scan("R")), b(scan("U"))),
            ),
        ),
        (
            "cross + select -> join",
            rule_cross_to_join,
            select(
                and(bin(Op::Eq, col("a"), col("x")), p2.clone()),
                cross(scan("R"), scan("U")),
            ),
        ),
        (
            "join commutativity (cross)",
            rule_join_commute,
            cross(scan("R"), scan("U")),
        ),
        (
            "join commutativity (natural)",
            rule_join_commute,
            natural(scan("R"), scan("S")),
        ),
        (
            "join commutativity (theta)",
            rule_join_commute,
            Join(bin(Op::Le, col("a"), col("x")), b(scan("R")), b(scan("U"))),
        ),
        (
            "associativity (cross)",
            rule_join_assoc,
            cross(cross(scan("R"), scan("U")), scan("T")),
        ),
        (
            "associativity (natural)",
            rule_join_assoc,
            natural(natural(scan("R"), scan("S")), scan("T")),
        ),
        (
            "projection pushdown (natural)",
            rule_push_project,
            project(&["a"], natural(scan("R"), scan("S"))),
        ),
        (
            "projection pushdown (cross)",
            rule_push_project,
            project(&["y", "a"], cross(scan("R"), scan("U"))),
        ),
        (
            "selection through aggregate",
            rule_select_through_agg,
            select(
                random_cmp(rng, "a", dom),
                Aggregate {
                    group: vec!["a".into()],
                    aggs: vec![
                        (AggFn::Sum, "b".into(), "sb".into()),
                        (AggFn::Count, "b".into(), "n".into()),
                    ],
                    input: b(scan("R")),
                },
            ),
        ),
    ]
}

fn rewrite_test() {
    let mut rng = Rng(0xA17E_B2A0_0723_0001);
    let mut fired: BTreeMap<&str, usize> = BTreeMap::new();
    for inst in 0..500 {
        let dom = [2, 3, 5][inst % 3];
        let db = random_db(&mut rng, 8, dom);
        let cat = catalog(&db);
        for (name, rule, plan) in shaped_cases(&mut rng, dom) {
            plan.schema(&cat)
                .unwrap_or_else(|e| panic!("{name}: lhs ill-typed: {e}"));
            let out = rule(&plan, &cat).unwrap_or_else(|| panic!("rule {name} did not fire"));
            out.schema(&cat).unwrap_or_else(|e| {
                panic!("{name}: rewritten plan ill-typed: {e}\n{}", out.tree())
            });
            assert_eq!(
                run(&plan, &db).canon(),
                run(&out, &db).canon(),
                "rewrite `{name}` changed the BAG result\nbefore:\n{}after:\n{}",
                plan.tree(),
                out.tree()
            );
            *fired.entry(name).or_insert(0) += 1;
        }
    }
    println!(
        "500 random instances x {} rewrites preserve the bag result (canonical form: columns by name, rows sorted)",
        fired.len()
    );
}

// ---------------------------------------------------------------- operator oracles
fn operator_test() {
    let mut rng = Rng(0x0BE2_A70B_0723_0002);
    for _ in 0..300 {
        let db = random_db(&mut rng, 9, 4);
        let (r, s) = (&db["R"], &db["S"]);
        // natural join vs nested loop
        let got = run(&natural(scan("R"), scan("S")), &db).canon().1;
        let mut want: Vec<Row> = vec![];
        for x in &r.rows {
            for y in &s.rows {
                if x[1] == y[0] {
                    want.push(vec![x[0].clone(), x[1].clone(), y[1].clone()]);
                }
            }
        }
        want.sort();
        assert_eq!(
            got, want,
            "natural join (columns a,b,c are already in name order)"
        );
        // bag intersect / except with a remove-first-match oracle
        let r2 = Relation {
            schema: r.schema.clone(),
            rows: (0..rng.below(8))
                .map(|_| {
                    vec![
                        Value::Int(rng.below(4) as i64),
                        Value::Int(rng.below(4) as i64),
                    ]
                })
                .collect(),
        };
        let mut db2 = db.clone();
        db2.insert("R2".into(), r2.clone());
        for (plan, take) in [
            (Intersect(b(scan("R")), b(scan("R2"))), true),
            (Except(b(scan("R")), b(scan("R2"))), false),
        ] {
            let mut pool = r2.rows.clone();
            let mut want = vec![];
            for x in &r.rows {
                let hit = pool
                    .iter()
                    .position(|y| y == x)
                    .map(|i| pool.remove(i))
                    .is_some();
                if hit == take {
                    want.push(x.clone());
                }
            }
            want.sort();
            assert_eq!(run(&plan, &db2).canon().1, want, "{}", plan.label());
        }
        // aggregation vs BTreeMap
        let agg = Aggregate {
            group: vec!["a".into()],
            aggs: vec![
                (AggFn::Count, "b".into(), "n".into()),
                (AggFn::Sum, "b".into(), "sb".into()),
                (AggFn::Min, "b".into(), "lo".into()),
                (AggFn::Max, "b".into(), "hi".into()),
            ],
            input: b(scan("R")),
        };
        let mut groups: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        for x in &r.rows {
            if let (Value::Int(a), Value::Int(bv)) = (&x[0], &x[1]) {
                groups.entry(*a).or_default().push(*bv);
            }
        }
        let want: Vec<Row> = groups
            .iter()
            .map(|(a, v)| {
                vec![
                    Value::Int(*a),
                    Value::Int(v.len() as i64),
                    Value::Int(v.iter().sum()),
                    Value::Int(*v.iter().min().unwrap()),
                    Value::Int(*v.iter().max().unwrap()),
                ]
            })
            .collect();
        let got = run(&agg, &db).rows;
        assert_eq!(
            got, want,
            "aggregation (output groups come out in key order)"
        );
        // semi / anti partition R
        let pred = bin(Op::Eq, col("b"), col("sb"));
        let renamed = Rename(vec![("b".into(), "sb".into())], b(scan("S")));
        let semi = run(&Semi(pred.clone(), b(scan("R")), b(renamed.clone())), &db)
            .rows
            .len();
        let anti = run(&Anti(pred, b(scan("R")), b(renamed)), &db).rows.len();
        assert_eq!(
            semi + anti,
            r.rows.len(),
            "semi + anti join partition the left input"
        );
        // division == textbook derivation (on sets)
        let div = Divide(b(scan("R")), b(project(&["b"], scan("S"))));
        let pa = Distinct(b(project(&["a"], scan("R"))));
        let derived = Except(
            b(pa.clone()),
            b(Distinct(b(project(
                &["a"],
                Except(
                    b(Cross(b(pa), b(Distinct(b(project(&["b"], scan("S"))))))),
                    b(Distinct(b(scan("R")))),
                ),
            )))),
        );
        assert_eq!(
            run(&div, &db).distinct().canon(),
            run(&derived, &db).distinct().canon(),
            "division"
        );
    }
    println!(
        "operators == naive oracles on 300 instances: natural join, bag INTERSECT/EXCEPT, GROUP BY, semi/anti, division"
    );
}

fn type_test() {
    let schema: Schema = vec![("a".into(), Type::Int), ("name".into(), Type::Str)];
    let ok = [
        bin(Op::Lt, bin(Op::Add, col("a"), int(1)), int(5)),
        bin(Op::Eq, col("name"), text("bob")),
        and(
            bin(Op::Gt, col("a"), int(0)),
            Expr::Not(Box::new(bin(Op::Eq, col("name"), text("x")))),
        ),
    ];
    for e in &ok {
        assert_eq!(e.typecheck(&schema), Ok(Type::Bool), "{}", e.show(true));
    }
    let bad = [
        bin(Op::Add, col("a"), text("x")),
        bin(Op::Eq, col("a"), text("x")),
        and(col("a"), bin(Op::Lt, col("a"), int(1))),
        col("nope"),
    ];
    for e in &bad {
        assert!(
            e.typecheck(&schema).is_err(),
            "{} must be rejected",
            e.show(true)
        );
    }
    let row = vec![Value::Int(3), Value::Str("bob".into())];
    assert_eq!(ok[0].eval(&schema, &row), Value::Bool(true));
    assert_eq!(ok[1].eval(&schema, &row), Value::Bool(true));
    assert_eq!(ok[2].eval(&schema, &row), Value::Bool(true));
    println!(
        "type checker: 3 well-typed predicates accepted, 4 ill-typed rejected (e.g. `a + 'x'`, `a = 'x'`, unknown column)"
    );
}

fn set_vs_bag() {
    let mk = |rows: &[(i64, i64)]| Relation {
        schema: vec![("a".into(), Type::Int), ("b".into(), Type::Int)],
        rows: rows
            .iter()
            .map(|&(a, b)| vec![Value::Int(a), Value::Int(b)])
            .collect(),
    };
    let mut db = Db::new();
    db.insert("R".into(), mk(&[(1, 1), (1, 1), (2, 2)]));
    db.insert("S".into(), mk(&[(1, 1), (1, 1), (3, 3)]));
    let pb = run(&project(&["a"], scan("R")), &db);
    println!(
        "pi_a(R) as bag: {} rows, as set: {} rows",
        pb.rows.len(),
        pb.distinct().rows.len()
    );
    assert_eq!((pb.rows.len(), pb.distinct().rows.len()), (3, 2));
    let u = run(&UnionAll(b(scan("R")), b(scan("S"))), &db);
    println!(
        "R UNION ALL S: {} rows; as sets (UNION): {} rows",
        u.rows.len(),
        u.distinct().rows.len()
    );
    assert_eq!((u.rows.len(), u.distinct().rows.len()), (6, 3));
    // Intersection vs natural join on identical schemas: equal as sets, different as bags.
    let inter = run(&Intersect(b(scan("R")), b(scan("S"))), &db);
    let join = run(&natural(scan("R"), scan("S")), &db);
    println!(
        "R INTERSECT S: {} rows (min of counts 2,2); R NATURAL JOIN S: {} rows (product 2*2); as sets both have {}",
        inter.rows.len(),
        join.rows.len(),
        join.distinct().rows.len()
    );
    assert_ne!(
        inter.canon(),
        join.canon(),
        "bag semantics: the 'intersection = natural join' law fails"
    );
    assert_eq!(
        inter.distinct().canon(),
        join.distinct().canon(),
        "set semantics: the law holds"
    );
}

fn optimizer_demo() {
    let mut rng = Rng(0xD3A0_0723_0003_0001);
    let db = random_db(&mut rng, 60, 25);
    let mut db = db;
    for t in ["R", "U", "T"] {
        // make the demo tables bigger than the random cap so the cross product visibly hurts
        let rel = db.get_mut(t).unwrap();
        while rel.rows.len() < 40 {
            rel.rows.push(vec![
                Value::Int(rng.below(25) as i64),
                Value::Int(rng.below(25) as i64),
            ]);
        }
    }
    let cat = catalog(&db);
    // SELECT a, d FROM R, U, T WHERE b = x AND y = c AND a < 5 AND d >= 20
    let pred = conj(vec![
        bin(Op::Eq, col("b"), col("x")),
        bin(Op::Eq, col("y"), col("c")),
        bin(Op::Lt, col("a"), int(5)),
        bin(Op::Ge, col("d"), int(20)),
    ])
    .unwrap();
    let naive = project(
        &["a", "d"],
        select(pred, cross(cross(scan("R"), scan("U")), scan("T"))),
    );
    let rules: [Rule; 4] = [
        rule_cascade,
        rule_push_select,
        rule_cross_to_join,
        rule_push_project,
    ];
    let opt = optimize(&naive, &cat, &rules);
    println!(
        "\nnaive plan:\n{}optimized plan:\n{}",
        naive.tree(),
        opt.tree()
    );
    let (mut w0, mut w1) = (0, 0);
    let (a, o) = (eval(&naive, &db, &mut w0), eval(&opt, &db, &mut w1));
    assert_eq!(
        a.canon(),
        o.canon(),
        "optimizer must preserve the bag result"
    );
    assert!(w1 < w0, "pushdown must reduce intermediate rows");
    println!(
        "result rows: {}   rows produced by all operators: naive {w0}, optimized {w1} ({:.0}x less)",
        a.rows.len(),
        w0 as f64 / w1.max(1) as f64
    );
    assert!(
        opt.tree().contains("⋈[") && !opt.tree().contains("×"),
        "every cross product became a keyed join"
    );
}

fn main() {
    type_test();
    set_vs_bag();
    operator_test();
    rewrite_test();
    optimizer_demo();
    println!("ok");
}
