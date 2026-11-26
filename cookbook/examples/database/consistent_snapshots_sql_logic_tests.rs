//! RECIPE 846 (database): how database engines are tested: sqllogictest scripts, differential testing, TLP and NoREC, test-case reduction, coverage-guided fuzzing.
//! LEVEL: intermediate
//! Use in: any query engine, rules engine, compiler or optimiser where "what is the right answer?" is expensive to know.   Run: cargo run --release --example consistent_snapshots_sql_logic_tests
//! COMMON USES: the sqllogictest script format from SQLite (adopted in variants by other engines; sqllogictest-rs is a Rust runner), SQLancer
//! (TLP, NoREC, PQS) against production DBMSs, C-Reduce / delta debugging for bug reports, coverage-guided fuzzers (AFL,
//! libFuzzer, SQLsmith-style generators), optimiser-on vs optimiser-off differential tests inside CI.
#![allow(dead_code)]
//!
//! THE ORACLE PROBLEM. A random query is cheap; knowing its correct result is not. Four answers, all implemented here:
//!   1. SCRIPTS (sqllogictest). A human (or a trusted engine) wrote the expected output once. Records are `statement ok`,
//!      `statement error <substring>`, `query <types> <sortmode> [label]`. Row order is unspecified in SQL without ORDER BY, so
//!      the SORT MODE (nosort / rowsort / valuesort) is part of the test. Big results are stored as "N values hashing to H"
//!      once they exceed `hash-threshold` (the real tool hashes with MD5; this one uses FNV-1a to stay std-only).
//!      Queries sharing a LABEL must give the same result.
//!   2. DIFFERENTIAL. Run two implementations (here: a naive nested-loop REFERENCE and an OPTIMISED engine with hash join and
//!      predicate pushdown) and compare multisets. Needs a second engine, and shared bugs are invisible.
//!   3. TLP (ternary logic partitioning, metamorphic: no second engine). In SQL's three-valued logic every row makes a
//!      predicate p exactly one of TRUE, FALSE or NULL, so `WHERE p`, `WHERE NOT p` and `WHERE (p) IS NULL` must partition
//!      the unfiltered result: their bag union equals it.
//!   4. NoREC (non-optimising reference engine construction). `SELECT * ... WHERE p` is a target for the optimiser; the same
//!      p moved into the SELECT list (`SELECT p ...`) has to be evaluated row by row. The row count of the first must equal
//!      the number of TRUEs in the second.
//!
//! THE INJECTED BUGS (switched on with `Bug`), both real classes of optimiser defect:
//!   * PushBelowLeftJoin: pushes a WHERE conjunct about the NULL-supplying side below a LEFT JOIN. That is only legal when
//!     the predicate rejects NULLs (and then the join can become INNER). Done blindly, filtered-out right rows turn into
//!     NULL-padded rows that never get re-checked.
//!   * PushedNullIsTrue: the pushed-down scan filter keeps rows whose predicate is "not FALSE" instead of "TRUE".
//!
//! REDUCTION. A failing case has two tables of rows and a nested predicate. Delta debugging (ddmin) removes chunks of rows
//! while the failure persists, then the predicate is shrunk by replacing a node with one of its children or a simpler leaf.
//! The loop repeats to a fixed point. The failing oracle is the only thing the reducer needs.
//!
//! COVERAGE GUIDANCE. The evaluator and optimiser record FEATURES (operator x parent x truth value, rules fired). A query
//! that reaches a new feature joins the corpus and gets mutated further. This is the AFL idea at the level of query plans.
//! Real systems use compiler edge coverage or query-plan fingerprints (as in "query plan guidance").
//!
//! Read: sqllogictest documentation https://sqlite.org/sqllogictest/doc/trunk/about.wiki
//!       sqllogictest-rs https://github.com/risinglightdb/sqllogictest-rs
//!       SQLancer https://github.com/sqlancer/sqlancer
//!       Rigger, Su, "Finding Bugs in Database Systems via Query Partitioning" (TLP), OOPSLA 2020 (cited by title)
//!       Rigger, Su, "Detecting Optimization Bugs in Database Engines via Non-Optimizing Reference Engine Construction" (NoREC)
//!       https://arxiv.org/abs/2007.08292
//!       Zeller, Hildebrandt, "Simplifying and Isolating Failure-Inducing Input" (delta debugging) (cited by title)
//!
//! TRY NEXT:
//!   - [S] Add `Bug::AndShortCircuitsNull` (NULL AND FALSE returns NULL) to the shared evaluator. Predict which of the three
//!     oracles can see it, then run them. Explain why differential testing cannot.
//!   - [S] Add a `skipif optimised` record to the script and check the runner counts it as skipped for one engine only.
//!   - [M] Extend TLP to `SELECT COUNT(*)`: the three partition counts must add up. Which injected bug does it still catch?
//!   - [M] Teach the reducer to shrink integer literals and cell values toward 0 / NULL and compare repro sizes.
//!   - [A] Project: combine with `datalog_engine_semi_naive`: design a metamorphic relation for stratified Datalog (hint: what
//!     happens to a positive program's output when you add a fact?) and run it on its random programs.

use std::collections::{BTreeSet, HashMap};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

// ================= values, expressions, queries =================
type Val = Option<i64>; // None = SQL NULL; booleans are 1 / 0 / NULL
type Row = Vec<Val>;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
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
}
impl Op {
    fn sql(self) -> &'static str {
        ["=", "<>", "<", "<=", ">", ">=", "AND", "OR", "+"][self as usize]
    }
    /// Binding power for the precedence-climbing parser: OR < AND < NOT(3) < comparison < +
    fn power(self) -> u8 {
        match self {
            Op::Or => 1,
            Op::And => 2,
            Op::Add => 5,
            _ => 4,
        }
    }
}
#[derive(Clone, PartialEq, Debug)]
enum Expr {
    Col(String), // "t.a"
    Lit(Val),
    Bin(Op, Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    IsNull(Box<Expr>),
}
impl Expr {
    fn sql(&self) -> String {
        match self {
            Expr::Col(c) => c.clone(),
            Expr::Lit(v) => v.map_or("NULL".into(), |i| i.to_string()),
            Expr::Bin(o, a, b) => format!("({} {} {})", a.sql(), o.sql(), b.sql()),
            Expr::Not(a) => format!("(NOT {})", a.sql()),
            Expr::IsNull(a) => format!("({} IS NULL)", a.sql()),
        }
    }
    fn size(&self) -> usize {
        match self {
            Expr::Col(_) | Expr::Lit(_) => 1,
            Expr::Bin(_, a, b) => 1 + a.size() + b.size(),
            Expr::Not(a) | Expr::IsNull(a) => 1 + a.size(),
        }
    }
    fn tables(&self, out: &mut BTreeSet<String>) {
        match self {
            Expr::Col(c) => {
                out.insert(c.split('.').next().unwrap().to_string());
            }
            Expr::Lit(_) => {}
            Expr::Bin(_, a, b) => {
                a.tables(out);
                b.tables(out);
            }
            Expr::Not(a) | Expr::IsNull(a) => a.tables(out),
        }
    }
    fn kind(&self) -> String {
        match self {
            Expr::Col(c) => format!("col:{}", &c[..1]),
            Expr::Lit(v) => {
                if v.is_none() {
                    "null".into()
                } else {
                    "int".into()
                }
            }
            Expr::Bin(o, _, _) => o.sql().into(),
            Expr::Not(_) => "NOT".into(),
            Expr::IsNull(_) => "ISNULL".into(),
        }
    }
}

/// Three-valued logic: this one function is shared by both engines (so differential testing cannot see bugs in it).
fn apply(op: Op, a: Val, b: Val) -> Val {
    let truth = |v: Val| v.map(|x| x != 0);
    match op {
        Op::And => match (truth(a), truth(b)) {
            (Some(false), _) | (_, Some(false)) => Some(0),
            (Some(true), Some(true)) => Some(1),
            _ => None,
        },
        Op::Or => match (truth(a), truth(b)) {
            (Some(true), _) | (_, Some(true)) => Some(1),
            (Some(false), Some(false)) => Some(0),
            _ => None,
        },
        _ => {
            let (a, b) = (a?, b?); // every comparison and + is NULL-propagating
            Some(match op {
                Op::Eq => (a == b) as i64,
                Op::Ne => (a != b) as i64,
                Op::Lt => (a < b) as i64,
                Op::Le => (a <= b) as i64,
                Op::Gt => (a > b) as i64,
                Op::Ge => (a >= b) as i64,
                Op::Add => a.wrapping_add(b),
                Op::And | Op::Or => unreachable!(),
            })
        }
    }
}

type Coverage = BTreeSet<String>;
fn eval(e: &Expr, cols: &[String], row: &[Val], cov: &mut Coverage) -> Result<Val, String> {
    Ok(match e {
        Expr::Col(c) => {
            row[cols
                .iter()
                .position(|x| x == c)
                .ok_or_else(|| format!("no such column: {c}"))?]
        }
        Expr::Lit(v) => *v,
        Expr::Bin(o, a, b) => {
            let (x, y) = (eval(a, cols, row, cov)?, eval(b, cols, row, cov)?);
            let r = apply(*o, x, y);
            cov.insert(format!(
                "{}({},{})->{:?}",
                o.sql(),
                a.kind(),
                b.kind(),
                r.map(|v| v.signum())
            ));
            r
        }
        Expr::Not(a) => {
            let r = eval(a, cols, row, cov)?.map(|v| (v == 0) as i64);
            cov.insert(format!("NOT({})->{r:?}", a.kind()));
            r
        }
        Expr::IsNull(a) => {
            let r = Some(eval(a, cols, row, cov)?.is_none() as i64);
            cov.insert(format!("ISNULL({})->{r:?}", a.kind()));
            r
        }
    })
}

#[derive(Clone, PartialEq, Debug)]
struct Join {
    table: String,
    left_outer: bool,
    on: Expr,
}
#[derive(Clone, PartialEq, Debug)]
struct Select {
    items: Option<Vec<Expr>>, // None = *
    from: String,
    join: Option<Join>,
    filter: Option<Expr>,
}
impl Select {
    fn sql(&self) -> String {
        let items = self.items.as_ref().map_or("*".into(), |v| {
            v.iter().map(Expr::sql).collect::<Vec<_>>().join(", ")
        });
        let mut s = format!("SELECT {items} FROM {}", self.from);
        if let Some(j) = &self.join {
            s += &format!(
                " {}JOIN {} ON {}",
                if j.left_outer { "LEFT " } else { "" },
                j.table,
                j.on.sql()
            );
        }
        if let Some(f) = &self.filter {
            s += &format!(" WHERE {}", f.sql());
        }
        s
    }
}
#[derive(Debug)]
enum Stmt {
    Create(String, Vec<String>),
    Insert(String, Vec<Row>),
    Select(Select),
}

// ================= parser =================
fn lex(s: &str) -> Result<Vec<String>, String> {
    let c: Vec<char> = s.chars().collect();
    let (mut i, mut out) = (0, vec![]);
    while i < c.len() {
        let start = i;
        if c[i].is_whitespace() {
            i += 1;
            continue;
        } else if c[i].is_ascii_alphanumeric() || c[i] == '_' {
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_' || c[i] == '.') {
                i += 1;
            }
        } else if "<>".contains(c[i]) {
            i += 1;
            if i < c.len() && (c[i] == '=' || (c[i - 1] == '<' && c[i] == '>')) {
                i += 1;
            }
        } else if "()=,*+-;".contains(c[i]) {
            i += 1;
        } else {
            return Err(format!("syntax error: unexpected character {:?}", c[i]));
        }
        out.push(c[start..i].iter().collect());
    }
    Ok(out)
}
struct Parser {
    t: Vec<String>,
    p: usize,
}
impl Parser {
    fn peek(&self) -> &str {
        self.t.get(self.p).map_or("", |s| s)
    }
    fn bump(&mut self) -> String {
        let s = self.peek().to_string();
        self.p += 1;
        s
    }
    fn eat(&mut self, kw: &str) -> bool {
        let hit = self.peek().eq_ignore_ascii_case(kw);
        self.p += hit as usize;
        hit
    }
    fn expect(&mut self, kw: &str) -> Result<(), String> {
        if self.eat(kw) {
            Ok(())
        } else {
            Err(format!(
                "syntax error: expected {kw}, found {:?}",
                self.peek()
            ))
        }
    }
    fn expr(&mut self, min: u8) -> Result<Expr, String> {
        let mut lhs = self.prefix()?;
        loop {
            if self.eat("IS") {
                let negated = self.eat("NOT");
                self.expect("NULL")?;
                lhs = Expr::IsNull(Box::new(lhs));
                if negated {
                    lhs = Expr::Not(Box::new(lhs));
                }
                continue;
            }
            let ops = [
                Op::Eq,
                Op::Ne,
                Op::Lt,
                Op::Le,
                Op::Gt,
                Op::Ge,
                Op::And,
                Op::Or,
                Op::Add,
            ];
            let Some(op) = ops
                .into_iter()
                .find(|o| self.peek().eq_ignore_ascii_case(o.sql()))
            else {
                return Ok(lhs);
            };
            if op.power() < min {
                return Ok(lhs);
            }
            self.p += 1;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(self.expr(op.power() + 1)?));
        }
    }
    fn prefix(&mut self) -> Result<Expr, String> {
        let t = self.bump();
        if t == "(" {
            let e = self.expr(1)?;
            self.expect(")")?;
            Ok(e)
        } else if t.eq_ignore_ascii_case("NOT") {
            Ok(Expr::Not(Box::new(self.expr(3)?)))
        } else if t.eq_ignore_ascii_case("NULL") {
            Ok(Expr::Lit(None))
        } else if t == "-" {
            let n: i64 = self
                .bump()
                .parse()
                .map_err(|_| "syntax error: expected a number after -".to_string())?;
            Ok(Expr::Lit(Some(-n)))
        } else if let Ok(n) = t.parse() {
            Ok(Expr::Lit(Some(n)))
        } else if t.contains('.') {
            Ok(Expr::Col(t))
        } else {
            Err(format!(
                "syntax error: expected table.column or literal, found {t:?}"
            ))
        }
    }
    fn list<T>(
        &mut self,
        mut f: impl FnMut(&mut Self) -> Result<T, String>,
    ) -> Result<Vec<T>, String> {
        let mut v = vec![f(self)?];
        while self.eat(",") {
            v.push(f(self)?);
        }
        Ok(v)
    }
    fn statement(&mut self) -> Result<Stmt, String> {
        if self.eat("CREATE") {
            self.expect("TABLE")?;
            let name = self.bump();
            self.expect("(")?;
            let cols = self.list(|p| Ok(p.bump()))?;
            self.expect(")")?;
            Ok(Stmt::Create(name, cols))
        } else if self.eat("INSERT") {
            self.expect("INTO")?;
            let name = self.bump();
            self.expect("VALUES")?;
            let rows = self.list(|p| {
                p.expect("(")?;
                let r = p.list(|p| match p.expr(1)? {
                    Expr::Lit(v) => Ok(v),
                    other => Err(format!("only literals in VALUES, found {}", other.sql())),
                })?;
                p.expect(")")?;
                Ok(r)
            })?;
            Ok(Stmt::Insert(name, rows))
        } else {
            self.expect("SELECT")?;
            let items = if self.eat("*") {
                None
            } else {
                Some(self.list(|p| p.expr(1))?)
            };
            self.expect("FROM")?;
            let from = self.bump();
            let left_outer = self.eat("LEFT");
            let join = if self.eat("JOIN") {
                let table = self.bump();
                self.expect("ON")?;
                Some(Join {
                    table,
                    left_outer,
                    on: self.expr(1)?,
                })
            } else if left_outer {
                return Err("syntax error: expected JOIN".into());
            } else {
                None
            };
            let filter = if self.eat("WHERE") {
                Some(self.expr(1)?)
            } else {
                None
            };
            Ok(Stmt::Select(Select {
                items,
                from,
                join,
                filter,
            }))
        }
    }
}
fn parse(sql: &str) -> Result<Stmt, String> {
    let mut p = Parser { t: lex(sql)?, p: 0 };
    let s = p.statement()?;
    p.eat(";");
    if p.p < p.t.len() {
        Err(format!("syntax error: trailing {:?}", p.peek()))
    } else {
        Ok(s)
    }
}

// ================= storage and the two engines =================
#[derive(Clone, Default, Debug)]
struct Table {
    cols: Vec<String>, // qualified: "t.a"
    rows: Vec<Row>,
}
#[derive(Clone, Default, Debug)]
struct Db {
    tables: HashMap<String, Table>,
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Bug {
    None,
    PushBelowLeftJoin,
    PushedNullIsTrue,
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Engine {
    Reference,
    Optimised(Bug),
}
impl Db {
    fn table(&self, n: &str) -> Result<&Table, String> {
        self.tables
            .get(n)
            .ok_or_else(|| format!("no such table: {n}"))
    }
    fn execute(&mut self, sql: &str, engine: Engine) -> Result<Vec<Row>, String> {
        match parse(sql)? {
            Stmt::Create(n, cols) => {
                if self.tables.contains_key(&n) {
                    return Err(format!("table {n} already exists"));
                }
                let cols = cols.iter().map(|c| format!("{n}.{c}")).collect();
                self.tables.insert(n, Table { cols, rows: vec![] });
                Ok(vec![])
            }
            Stmt::Insert(n, rows) => {
                let t = self
                    .tables
                    .get_mut(&n)
                    .ok_or_else(|| format!("no such table: {n}"))?;
                if let Some(bad) = rows.iter().find(|r| r.len() != t.cols.len()) {
                    return Err(format!(
                        "expected {} values, got {}",
                        t.cols.len(),
                        bad.len()
                    ));
                }
                t.rows.extend(rows); // all-or-nothing: validated before the first row is added
                Ok(vec![])
            }
            Stmt::Select(q) => self.select(&q, engine, &mut Coverage::new()),
        }
    }
    fn select(&self, q: &Select, engine: Engine, cov: &mut Coverage) -> Result<Vec<Row>, String> {
        match engine {
            Engine::Reference => self.reference(q, cov),
            Engine::Optimised(bug) => self.optimised(q, bug, cov),
        }
    }
    /// The textbook semantics, as directly as possible: FROM, then JOIN by nested loop, then WHERE, then SELECT.
    fn reference(&self, q: &Select, cov: &mut Coverage) -> Result<Vec<Row>, String> {
        let left = self.table(&q.from)?;
        let (mut cols, mut rows) = (left.cols.clone(), left.rows.clone());
        if let Some(j) = &q.join {
            let right = self.table(&j.table)?;
            cols.extend(right.cols.clone());
            let mut out = vec![];
            for l in &rows {
                let mut matched = false;
                for r in &right.rows {
                    let both: Row = l.iter().chain(r).copied().collect();
                    if truthy(eval(&j.on, &cols, &both, cov)?) {
                        matched = true;
                        out.push(both);
                    }
                }
                if !matched && j.left_outer {
                    out.push(
                        l.iter()
                            .copied()
                            .chain(std::iter::repeat_n(None, right.cols.len()))
                            .collect(),
                    );
                }
            }
            rows = out;
        }
        filter_project(q.filter.iter().collect(), &q.items, &cols, rows, cov)
    }
    /// Predicate pushdown + hash join for `left.x = right.y` conditions.
    fn optimised(&self, q: &Select, bug: Bug, cov: &mut Coverage) -> Result<Vec<Row>, String> {
        let mut conjuncts = vec![];
        if let Some(f) = &q.filter {
            split_and(f, &mut conjuncts);
        }
        let left = self.table(&q.from)?;
        let right = q.join.as_ref().map(|j| self.table(&j.table)).transpose()?;
        // Rule 1: a conjunct that mentions only one input may move below the join, IF that preserves semantics.
        let (mut push_l, mut push_r, mut residual) = (vec![], vec![], vec![]);
        for c in conjuncts {
            let mut used = BTreeSet::new();
            c.tables(&mut used);
            let only = |t: &str| used.len() == 1 && used.contains(t);
            if only(&q.from) {
                cov.insert("rule:push-left".into());
                push_l.push(c);
            } else if q
                .join
                .as_ref()
                .is_some_and(|j| only(&j.table) && (!j.left_outer || bug == Bug::PushBelowLeftJoin))
            {
                cov.insert(format!(
                    "rule:push-right outer={}",
                    q.join.as_ref().unwrap().left_outer
                ));
                push_r.push(c);
            } else {
                cov.insert("rule:residual".into());
                residual.push(c);
            }
        }
        let scan = |t: &Table, preds: &[&Expr], cov: &mut Coverage| -> Result<Vec<Row>, String> {
            let mut out = vec![];
            'rows: for row in &t.rows {
                for p in preds {
                    let v = eval(p, &t.cols, row, cov)?;
                    let keep = if bug == Bug::PushedNullIsTrue {
                        v != Some(0)
                    } else {
                        truthy(v)
                    };
                    if !keep {
                        continue 'rows;
                    }
                }
                out.push(row.clone());
            }
            Ok(out)
        };
        let mut rows = scan(left, &push_l, cov)?;
        let mut cols = left.cols.clone();
        if let (Some(j), Some(rt)) = (&q.join, right) {
            let rrows = scan(rt, &push_r, cov)?;
            cols.extend(rt.cols.clone());
            // Rule 2: equi-join -> hash join. NULL keys never match, so they are neither built nor probed.
            let mut keys = None;
            if let Expr::Bin(Op::Eq, a, b) = &j.on
                && let (Expr::Col(a), Expr::Col(b)) = (a.as_ref(), b.as_ref())
            {
                let pos = |t: &Table, c: &str| t.cols.iter().position(|x| x == c);
                keys = match (pos(left, a), pos(rt, b), pos(left, b), pos(rt, a)) {
                    (Some(l), Some(r), _, _) | (_, _, Some(l), Some(r)) => Some((l, r)),
                    _ => None,
                };
            }
            let mut index: HashMap<i64, Vec<usize>> = HashMap::new();
            if let Some((_, rk)) = keys {
                cov.insert(format!("rule:hash-join outer={}", j.left_outer));
                for (i, r) in rrows.iter().enumerate() {
                    if let Some(k) = r[rk] {
                        index.entry(k).or_default().push(i);
                    }
                }
            } else {
                cov.insert(format!("rule:nested-loop outer={}", j.left_outer));
            }
            let all: Vec<usize> = (0..rrows.len()).collect();
            let mut out = vec![];
            for l in &rows {
                let candidates: &[usize] = match keys {
                    Some((lk, _)) => l[lk].and_then(|k| index.get(&k)).map_or(&[], |v| v),
                    None => &all,
                };
                let before = out.len();
                for &i in candidates {
                    let both: Row = l.iter().chain(&rrows[i]).copied().collect();
                    if keys.is_some() || truthy(eval(&j.on, &cols, &both, cov)?) {
                        out.push(both);
                    }
                }
                if out.len() == before && j.left_outer {
                    cov.insert("op:null-padded row".into());
                    out.push(
                        l.iter()
                            .copied()
                            .chain(std::iter::repeat_n(None, rt.cols.len()))
                            .collect(),
                    );
                }
            }
            rows = out;
        }
        filter_project(residual, &q.items, &cols, rows, cov)
    }
}
fn truthy(v: Val) -> bool {
    v.is_some_and(|x| x != 0)
}
fn split_and<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
    match e {
        Expr::Bin(Op::And, a, b) => {
            split_and(a, out);
            split_and(b, out);
        }
        other => out.push(other),
    }
}
fn filter_project(
    preds: Vec<&Expr>,
    items: &Option<Vec<Expr>>,
    cols: &[String],
    rows: Vec<Row>,
    cov: &mut Coverage,
) -> Result<Vec<Row>, String> {
    let mut out = vec![];
    'rows: for row in rows {
        for p in &preds {
            if !truthy(eval(p, cols, &row, cov)?) {
                continue 'rows;
            }
        }
        out.push(match items {
            None => row,
            Some(es) => es
                .iter()
                .map(|e| eval(e, cols, &row, cov))
                .collect::<Result<_, _>>()?,
        });
    }
    Ok(out)
}

// ================= sqllogictest-style runner =================
fn render(rows: &[Row], sort: &str) -> Vec<String> {
    let mut text: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|v| v.map_or("NULL".into(), |i| i.to_string()))
                .collect()
        })
        .collect();
    if sort == "rowsort" {
        text.sort();
    }
    let mut flat: Vec<String> = text.into_iter().flatten().collect(); // one value per line, as in the real format
    if sort == "valuesort" {
        flat.sort();
    }
    flat
}
fn fnv(lines: &[String]) -> u64 {
    lines
        .iter()
        .flat_map(|l| l.bytes().chain(*b"\n"))
        .fold(0xcbf29ce484222325, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100000001b3)
        })
}
#[derive(Debug, Default)]
struct Report {
    passed: usize,
    skipped: usize,
    failures: Vec<(usize, String)>, // (1-based script line, message)
}
fn run_script(script: &str, engine: Engine) -> Report {
    let name = if engine == Engine::Reference {
        "reference"
    } else {
        "optimised"
    };
    let (mut db, mut rep, mut threshold) = (Db::default(), Report::default(), usize::MAX);
    let mut labels: HashMap<String, Vec<String>> = HashMap::new();
    let lines: Vec<&str> = script.lines().collect();
    let mut i = 0;
    let mut skip_next = false;
    while i < lines.len() {
        let head: Vec<&str> = lines[i].split_whitespace().collect();
        let at = i + 1;
        i += 1;
        match head.first().copied() {
            None => {}
            Some(w) if w.starts_with('#') => {}
            Some("hash-threshold") => threshold = head[1].parse().unwrap(),
            Some("skipif") => skip_next |= head[1] == name,
            Some("onlyif") => skip_next |= head[1] != name,
            Some(kind @ ("statement" | "query")) => {
                let mut sql = vec![];
                while i < lines.len() && !lines[i].trim().is_empty() && lines[i] != "----" {
                    sql.push(lines[i]);
                    i += 1;
                }
                let mut expected = vec![];
                if i < lines.len() && lines[i] == "----" {
                    i += 1;
                    while i < lines.len() && !lines[i].trim().is_empty() {
                        expected.push(lines[i].to_string());
                        i += 1;
                    }
                }
                if std::mem::take(&mut skip_next) {
                    rep.skipped += 1;
                    continue;
                }
                let result = db.execute(&sql.join(" "), engine);
                let verdict: Result<(), String> = if kind == "statement" {
                    match (head[1], result) {
                        ("ok", Ok(_)) => Ok(()),
                        ("error", Err(e)) if e.contains(&head[2..].join(" ")) => Ok(()),
                        (want, got) => Err(format!("statement {want}: got {got:?}")),
                    }
                } else {
                    // query <types: one letter per column> <sortmode> [label]
                    result.and_then(|rows| {
                        if let Some(r) = rows.iter().find(|r| r.len() != head[1].len()) {
                            return Err(format!("type string {} but row has {} columns", head[1], r.len()));
                        }
                        let got = render(&rows, head.get(2).copied().unwrap_or("nosort"));
                        if let Some(label) = head.get(3) {
                            let first = labels.entry(label.to_string()).or_insert_with(|| got.clone());
                            if *first != got {
                                return Err(format!("label {label}: result differs from an earlier query with this label"));
                            }
                        }
                        let shown = if got.len() > threshold { vec![format!("{} values hashing to {:016x}", got.len(), fnv(&got))] } else { got };
                        if shown == expected { Ok(()) } else { Err(format!("expected {expected:?}, got {shown:?}")) }
                    })
                };
                match verdict {
                    Ok(()) => rep.passed += 1,
                    Err(m) => rep.failures.push((at, m)),
                }
            }
            Some(other) => rep.failures.push((at, format!("unknown record {other}"))),
        }
    }
    rep
}

const SCRIPT: &str = "\
# NULLs, three-valued logic, outer joins
statement ok
CREATE TABLE t(a, b)

statement ok
CREATE TABLE u(a, c)

statement ok
INSERT INTO t VALUES (1, 10), (2, NULL), (3, 30), (NULL, 40)

statement ok
INSERT INTO u VALUES (1, 100), (3, NULL), (3, 7), (NULL, 8)

statement error already exists
CREATE TABLE t(x)

statement error expected 2 values
INSERT INTO t VALUES (9, 9), (9)

statement error no such table
SELECT * FROM missing

statement error no such column
SELECT t.zz FROM t

statement error syntax error
SELECT FROM t

# the failed multi-row insert above must have left nothing behind
query II rowsort
SELECT * FROM t
----
1
10
2
NULL
3
30
NULL
40

# NULL = NULL is unknown, so neither side of this partition keeps the NULL row
query I rowsort
SELECT t.a FROM t WHERE t.a = t.a
----
1
2
3

query I nosort
SELECT t.b FROM t WHERE NOT t.b > 20
----
10

query II rowsort equi
SELECT t.a, u.c FROM t JOIN u ON t.a = u.a
----
1
100
3
7
3
NULL

# same label: the commuted ON condition must give the same answer
query II rowsort equi
SELECT t.a, u.c FROM t JOIN u ON u.a = t.a
----
1
100
3
7
3
NULL

query II rowsort
SELECT t.a, u.c FROM t LEFT JOIN u ON t.a = u.a WHERE u.c IS NULL
----
2
NULL
3
NULL
NULL
NULL

query III valuesort
SELECT t.a, t.a + 1, t.a < 2 FROM t WHERE t.a <= 2
----
0
1
1
2
2
3

onlyif reference
statement ok
INSERT INTO u VALUES (50, 50)

skipif reference
statement ok
INSERT INTO u VALUES (50, 50)
";

/// "Completion mode": let a trusted engine fill in the expected output, as sqllogictest does when writing a corpus.
fn complete(setup: &str, queries: &[Select], db: &Db, threshold: usize) -> String {
    let mut s = format!("hash-threshold {threshold}\n\n{setup}");
    for q in queries {
        let rows = db
            .select(q, Engine::Reference, &mut Coverage::new())
            .unwrap();
        let width = rows.first().map_or(1, Vec::len);
        let mut out = render(&rows, "rowsort");
        if out.len() > threshold {
            out = vec![format!(
                "{} values hashing to {:016x}",
                out.len(),
                fnv(&out)
            )];
        }
        s += &format!(
            "query {} rowsort\n{}\n----\n{}\n\n",
            "I".repeat(width),
            q.sql(),
            out.join("\n")
        );
    }
    s
}

// ================= random databases and queries =================
const SCHEMA: [(&str, [&str; 2]); 2] = [("t", ["a", "b"]), ("u", ["a", "c"])];
fn gen_cell(r: &mut Rng) -> Val {
    if r.below(5) == 0 {
        None
    } else {
        Some(r.below(4) as i64)
    } // small domain: joins and equalities actually hit
}
fn gen_db(r: &mut Rng) -> Db {
    let mut db = Db::default();
    for (n, cols) in SCHEMA {
        let rows = (0..r.below(7))
            .map(|_| vec![gen_cell(r), gen_cell(r)])
            .collect();
        db.tables.insert(
            n.into(),
            Table {
                cols: cols.iter().map(|c| format!("{n}.{c}")).collect(),
                rows,
            },
        );
    }
    db
}
fn gen_col(r: &mut Rng, joined: bool) -> Expr {
    let (t, cols) = SCHEMA[if joined { r.below(2) } else { 0 }];
    Expr::Col(format!("{t}.{}", cols[r.below(2)]))
}
fn gen_scalar(r: &mut Rng, joined: bool) -> Expr {
    match r.below(8) {
        0 => Expr::Lit(None),
        1 | 2 => Expr::Lit(Some(r.below(4) as i64)),
        3 => Expr::Bin(
            Op::Add,
            Box::new(gen_col(r, joined)),
            Box::new(Expr::Lit(Some(1))),
        ),
        _ => gen_col(r, joined),
    }
}
fn gen_pred(r: &mut Rng, joined: bool, depth: usize) -> Expr {
    match if depth == 0 { r.below(2) } else { r.below(6) } {
        0 => Expr::Bin(
            [Op::Eq, Op::Ne, Op::Lt, Op::Le, Op::Gt, Op::Ge][r.below(6)],
            Box::new(gen_scalar(r, joined)),
            Box::new(gen_scalar(r, joined)),
        ),
        1 => Expr::IsNull(Box::new(gen_scalar(r, joined))),
        2 => Expr::Not(Box::new(gen_pred(r, joined, depth - 1))),
        3 => Expr::Bin(
            Op::Or,
            Box::new(gen_pred(r, joined, depth - 1)),
            Box::new(gen_pred(r, joined, depth - 1)),
        ),
        _ => Expr::Bin(
            Op::And,
            Box::new(gen_pred(r, joined, depth - 1)),
            Box::new(gen_pred(r, joined, depth - 1)),
        ),
    }
}
fn gen_on(r: &mut Rng) -> Expr {
    let col = |s: &str| Box::new(Expr::Col(s.into()));
    match r.below(5) {
        0 => gen_pred(r, true, 1),
        1 => Expr::Bin(Op::Eq, col("u.a"), col("t.a")),
        _ => Expr::Bin(Op::Eq, col("t.a"), col("u.a")),
    }
}
fn gen_query(r: &mut Rng) -> Select {
    let join = (r.below(4) > 0).then(|| Join {
        table: "u".into(),
        left_outer: r.below(2) == 0,
        on: gen_on(r),
    });
    let filter = Some(gen_pred(r, join.is_some(), 3));
    Select {
        items: None,
        from: "t".into(),
        join,
        filter,
    }
}

// ================= oracles =================
#[derive(Clone, Copy, PartialEq, Debug)]
enum Oracle {
    Differential,
    Tlp,
    NoRec,
}
const ORACLES: [Oracle; 3] = [Oracle::Differential, Oracle::Tlp, Oracle::NoRec];
fn bag(mut v: Vec<Row>) -> Vec<Row> {
    v.sort();
    v
}
/// True when the oracle says the optimised engine (with `bug`) is wrong on this case.
/// TLP and NoREC never consult the reference engine.
fn fails(oracle: Oracle, db: &Db, q: &Select, bug: Bug, cov: &mut Coverage) -> bool {
    let sut = Engine::Optimised(bug);
    let mut run = |q: &Select, e| bag(db.select(q, e, cov).expect("generated queries are valid"));
    match oracle {
        Oracle::Differential => run(q, sut) != run(q, Engine::Reference),
        Oracle::Tlp => {
            let Some(p) = q.filter.clone() else {
                return false;
            };
            let whole = run(
                &Select {
                    filter: None,
                    ..q.clone()
                },
                sut,
            );
            let mut union = vec![];
            for part in [
                p.clone(),
                Expr::Not(Box::new(p.clone())),
                Expr::IsNull(Box::new(p)),
            ] {
                union.extend(run(
                    &Select {
                        filter: Some(part),
                        ..q.clone()
                    },
                    sut,
                ));
            }
            bag(union) != whole
        }
        Oracle::NoRec => {
            let Some(p) = q.filter.clone() else {
                return false;
            };
            let optimisable = run(q, sut).len();
            let unoptimisable = run(
                &Select {
                    items: Some(vec![p]),
                    filter: None,
                    ..q.clone()
                },
                sut,
            );
            optimisable != unoptimisable.iter().filter(|r| truthy(r[0])).count()
        }
    }
}

// ================= test-case reduction =================
/// ddmin over a list: try deleting chunks, halving the chunk size until single elements cannot be removed.
fn ddmin<T: Clone>(mut items: Vec<T>, still_fails: &mut dyn FnMut(&[T]) -> bool) -> Vec<T> {
    let mut chunk = items.len().max(1);
    loop {
        let (mut start, mut removed) = (0, false);
        while start < items.len() {
            let mut candidate = items.clone();
            candidate.drain(start..(start + chunk).min(items.len()));
            if still_fails(&candidate) {
                items = candidate;
                removed = true;
            } else {
                start += chunk;
            }
        }
        if chunk > 1 {
            chunk /= 2;
        } else if !removed {
            return items; // 1-minimal: removing any single element loses the failure
        }
    }
}
/// Every expression obtained by one local simplification, anywhere in the tree. All are strictly simpler.
fn shrinks(e: &Expr) -> Vec<Expr> {
    let mut out = vec![];
    match e {
        Expr::Col(_) => out.push(Expr::Lit(None)),
        Expr::Lit(_) => {}
        Expr::Bin(o, a, b) => {
            out.extend([(**a).clone(), (**b).clone()]);
            out.extend(
                shrinks(a)
                    .into_iter()
                    .map(|a| Expr::Bin(*o, Box::new(a), b.clone())),
            );
            out.extend(
                shrinks(b)
                    .into_iter()
                    .map(|b| Expr::Bin(*o, a.clone(), Box::new(b))),
            );
        }
        Expr::Not(a) => {
            out.push((**a).clone());
            out.extend(shrinks(a).into_iter().map(|a| Expr::Not(Box::new(a))));
        }
        Expr::IsNull(a) => {
            out.push((**a).clone());
            out.extend(shrinks(a).into_iter().map(|a| Expr::IsNull(Box::new(a))));
        }
    }
    out
}
fn reduce(mut db: Db, mut q: Select, failing: &dyn Fn(&Db, &Select) -> bool) -> (Db, Select) {
    assert!(failing(&db, &q), "reducer needs a failing input");
    loop {
        let before = (rows_in(&db), q.clone());
        for (name, _) in SCHEMA {
            let base = db.clone(); // a private snapshot: each candidate is judged against the same state
            let kept = ddmin(db.tables[name].rows.clone(), &mut |rows| {
                let mut c = base.clone();
                c.tables.get_mut(name).unwrap().rows = rows.to_vec();
                failing(&c, &q)
            });
            db.tables.get_mut(name).unwrap().rows = kept;
        }
        // Greedy predicate shrinking: take the first simpler filter / ON clause that still fails.
        while let Some(better) = q.filter.as_ref().and_then(|f| {
            shrinks(f)
                .into_iter()
                .map(|f| Select {
                    filter: Some(f),
                    ..q.clone()
                })
                .find(|c| failing(&db, c))
        }) {
            q = better;
        }
        while let Some(better) = q.join.as_ref().and_then(|j| {
            shrinks(&j.on)
                .into_iter()
                .map(|on| Select {
                    join: Some(Join { on, ..j.clone() }),
                    ..q.clone()
                })
                .find(|c| failing(&db, c))
        }) {
            q = better;
        }
        if (rows_in(&db), q.clone()) == before {
            return (db, q);
        }
    }
}
fn rows_in(db: &Db) -> usize {
    db.tables.values().map(|t| t.rows.len()).sum()
}

// ================= coverage-guided mutation =================
fn mutate_expr(r: &mut Rng, e: &Expr, joined: bool) -> Expr {
    if r.below(4) == 0 {
        return match r.below(4) {
            0 => Expr::Not(Box::new(e.clone())),
            1 => Expr::IsNull(Box::new(e.clone())),
            2 => Expr::Bin(
                Op::And,
                Box::new(e.clone()),
                Box::new(gen_pred(r, joined, 1)),
            ),
            _ => gen_pred(r, joined, 2),
        };
    }
    match e {
        Expr::Bin(o, a, b) if r.below(2) == 0 => {
            Expr::Bin(*o, Box::new(mutate_expr(r, a, joined)), b.clone())
        }
        Expr::Bin(o, a, b) => Expr::Bin(*o, a.clone(), Box::new(mutate_expr(r, b, joined))),
        Expr::Not(a) => Expr::Not(Box::new(mutate_expr(r, a, joined))),
        Expr::IsNull(a) => Expr::IsNull(Box::new(mutate_expr(r, a, joined))),
        _ => gen_scalar(r, joined),
    }
}
/// Returns (features covered, corpus size). With `guided`, inputs that add a feature are kept and mutated.
fn campaign(seed: u64, budget: usize, guided: bool) -> (Coverage, usize) {
    let mut r = Rng(seed);
    let db = gen_db_nonempty(&mut r);
    let (mut seen, mut corpus) = (Coverage::new(), Vec::<Select>::new());
    for _ in 0..budget {
        // The unguided baseline draws from the deliberately shallow generator the guided run starts from.
        let q = if guided && !corpus.is_empty() && r.below(5) > 0 {
            let mut q = corpus[r.below(corpus.len())].clone();
            let joined = q.join.is_some();
            q.filter = Some(mutate_expr(&mut r, q.filter.as_ref().unwrap(), joined));
            if q.filter.as_ref().unwrap().size() > 25 {
                q.filter = Some(gen_pred(&mut r, joined, 1));
            }
            q
        } else {
            let mut q = gen_query(&mut r);
            q.filter = Some(gen_pred(&mut r, q.join.is_some(), 1));
            q
        };
        let mut cov = Coverage::new();
        db.select(&q, Engine::Optimised(Bug::None), &mut cov)
            .unwrap();
        if cov.iter().any(|f| !seen.contains(f)) {
            seen.extend(cov);
            corpus.push(q);
        }
    }
    (seen, corpus.len())
}
fn gen_db_nonempty(r: &mut Rng) -> Db {
    loop {
        let db = gen_db(r);
        if db.tables.values().all(|t| t.rows.len() >= 4) {
            return db;
        }
    }
}

// ================= checks =================
fn check_parser() {
    let mut r = Rng(0x9A85E);
    for _ in 0..3000 {
        let q = gen_query(&mut r);
        match parse(&q.sql()) {
            Ok(Stmt::Select(back)) => assert_eq!(back, q, "print/parse round trip: {}", q.sql()),
            other => panic!("{} -> {other:?}", q.sql()),
        }
    }
    // precedence: NOT binds looser than comparison, AND tighter than OR
    let Ok(Stmt::Select(q)) =
        parse("SELECT * FROM t WHERE NOT t.a = 1 AND t.b IS NOT NULL OR t.a < 2")
    else {
        panic!()
    };
    assert_eq!(
        q.filter.unwrap().sql(),
        "(((NOT (t.a = 1)) AND (NOT (t.b IS NULL))) OR (t.a < 2))"
    );
    println!("parser: 3000 random queries survive print -> parse; precedence as in SQL");
}
fn check_scripts() {
    for engine in [Engine::Reference, Engine::Optimised(Bug::None)] {
        let rep = run_script(SCRIPT, engine);
        assert!(rep.failures.is_empty(), "{engine:?}: {:?}", rep.failures);
        assert_eq!((rep.passed, rep.skipped), (17, 1), "{engine:?}");
    }
    // the runner itself must be able to FAIL: wrong value, wrong error text, label mismatch, wrong column count
    let broken = "statement ok\nCREATE TABLE t(a)\n\nstatement ok\nINSERT INTO t VALUES (2), (1)\n\nquery I nosort x\nSELECT * FROM t\n----\n1\n2\n\n\
statement error disk full\nSELECT * FROM nope\n\nquery I rowsort x\nSELECT * FROM t\n----\n1\n2\n\nquery II nosort\nSELECT * FROM t\n----\n2\n1\n";
    let rep = run_script(broken, Engine::Reference);
    assert_eq!(
        rep.failures.iter().map(|f| f.0).collect::<Vec<_>>(),
        [7, 13, 16, 22],
        "{:?}",
        rep.failures
    );
    println!(
        "sqllogictest runner: 17 records pass on both engines (1 skipif/onlyif skip each); 4 planted script errors reported by line"
    );

    // completion mode + hash threshold: a corpus written by the reference engine, replayed on the system under test
    let mut r = Rng(0x10_61C);
    let db = gen_db_nonempty(&mut r);
    let mut setup = String::new();
    for (n, cols) in SCHEMA {
        setup += &format!("statement ok\nCREATE TABLE {n}({})\n\n", cols.join(", "));
        let rows: Vec<String> = db.tables[n]
            .rows
            .iter()
            .map(|row| {
                format!(
                    "({})",
                    row.iter()
                        .map(|v| Expr::Lit(*v).sql())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        setup += &format!(
            "statement ok\nINSERT INTO {n} VALUES {}\n\n",
            rows.join(", ")
        );
    }
    let queries: Vec<Select> = (0..400).map(|_| gen_query(&mut r)).collect();
    let corpus = complete(&setup, &queries, &db, 8);
    let hashed = corpus.matches("values hashing to").count();
    assert!(hashed > 20, "the threshold should hash the larger results");
    assert!(
        run_script(&corpus, Engine::Optimised(Bug::None))
            .failures
            .is_empty()
    );
    for bug in [Bug::PushBelowLeftJoin, Bug::PushedNullIsTrue] {
        let n = run_script(&corpus, Engine::Optimised(bug)).failures.len();
        assert!(n > 0, "the generated corpus must catch {bug:?}");
        println!(
            "generated corpus (400 queries, {hashed} hashed results): {bug:?} fails {n} of them"
        );
    }
}
fn check_correct_engine() {
    let mut r = Rng(0xC0_44EC7);
    let (mut nonempty, mut cov) = (0, Coverage::new());
    for i in 0..5000 {
        let db = gen_db(&mut r);
        let q = gen_query(&mut r);
        for o in ORACLES {
            assert!(
                !fails(o, &db, &q, Bug::None, &mut cov),
                "false positive from {o:?} on query {i}: {} with {db:?}",
                q.sql()
            );
        }
        nonempty += !db
            .select(&q, Engine::Reference, &mut cov)
            .unwrap()
            .is_empty() as usize;
    }
    assert!(
        nonempty > 1000,
        "too many vacuous queries ({nonempty} non-empty): the generator is not testing much"
    );
    println!(
        "correct engine: 5000 random queries pass differential, TLP and NoREC ({nonempty} with non-empty results)"
    );
}
fn check_find_and_reduce() {
    const BOUND: usize = 2000;
    println!(
        "\n{:>20} {:>13} {:>10} {:>16} {:>16}",
        "injected bug", "oracle", "found at", "rows before->after", "pred before->after"
    );
    for bug in [Bug::PushBelowLeftJoin, Bug::PushedNullIsTrue] {
        for oracle in ORACLES {
            let mut r = Rng(0xB06 + oracle as u64);
            let found = (1..=BOUND).find_map(|n| {
                let (db, q) = (gen_db(&mut r), gen_query(&mut r));
                fails(oracle, &db, &q, bug, &mut Coverage::new()).then_some((n, db, q))
            });
            let (n, db, q) = found.unwrap_or_else(|| {
                panic!("{oracle:?} did not find {bug:?} within {BOUND} queries")
            });
            let failing = |db: &Db, q: &Select| fails(oracle, db, q, bug, &mut Coverage::new());
            let (sdb, sq) = reduce(db.clone(), q.clone(), &failing);
            assert!(failing(&sdb, &sq), "the reduced case must still fail");
            assert!(
                !fails(oracle, &sdb, &sq, Bug::None, &mut Coverage::new()),
                "and must pass without the bug"
            );
            let (before, after) = (
                q.filter.as_ref().unwrap().size(),
                sq.filter.as_ref().unwrap().size(),
            );
            println!(
                "{:>20} {:>13} {n:>10} {:>10} -> {:<4} {before:>10} -> {after}",
                format!("{bug:?}"),
                format!("{oracle:?}"),
                rows_in(&db),
                rows_in(&sdb)
            );
            assert!(
                rows_in(&sdb) <= 2,
                "repro should need at most 2 rows: {sdb:?}"
            );
            assert!(after <= 4, "repro predicate should be tiny: {}", sq.sql());
            // 1-minimality: deleting any single remaining row makes the failure disappear
            for (name, _) in SCHEMA {
                for k in 0..sdb.tables[name].rows.len() {
                    let mut c = sdb.clone();
                    c.tables.get_mut(name).unwrap().rows.remove(k);
                    assert!(!failing(&c, &sq));
                }
            }
            if oracle == Oracle::Differential {
                let rows = |n: &str| {
                    sdb.tables[n]
                        .rows
                        .iter()
                        .map(|r| render(std::slice::from_ref(r), "nosort").join(","))
                        .collect::<Vec<_>>()
                };
                println!(
                    "      repro: t={:?} u={:?}  {}",
                    rows("t"),
                    rows("u"),
                    sq.sql()
                );
            }
        }
    }
}
/// A bug in the SHARED evaluator is invisible to differential testing. Shown with the data, not a flag: both engines call
/// the same `apply`, so their agreement says nothing about whether `apply` implements SQL.
fn check_oracle_blind_spot() {
    assert_eq!(
        apply(Op::And, None, Some(0)),
        Some(0),
        "NULL AND FALSE = FALSE"
    );
    assert_eq!(apply(Op::Or, None, Some(1)), Some(1), "NULL OR TRUE = TRUE");
    assert_eq!(apply(Op::And, None, Some(1)), None);
    assert_eq!(apply(Op::Or, None, Some(0)), None);
    println!(
        "shared evaluator pinned by explicit truth-table checks (differential testing cannot test code both sides share)"
    );
}
fn check_coverage_guidance() {
    let (mut g_total, mut u_total) = (0, 0);
    for seed in 1..=5u64 {
        let (g, corpus) = campaign(seed * 7919, 3000, true);
        let (u, _) = campaign(seed * 7919, 3000, false);
        assert!(
            corpus > 20 && corpus <= g.len(),
            "every corpus entry added at least one feature"
        );
        g_total += g.len();
        u_total += u.len();
    }
    println!(
        "coverage after 3000 queries, summed over 5 seeds: guided mutation {g_total} features, shallow random generator {u_total}"
    );
    assert!(
        g_total > u_total,
        "mutating coverage-increasing inputs should reach shapes the shallow generator cannot build"
    );
}

fn main() {
    check_oracle_blind_spot();
    check_parser();
    check_scripts();
    check_correct_engine();
    check_find_and_reduce();
    check_coverage_guidance();
    println!("all checks passed");
}
