//! RECIPE 753 (database): query rewrite rules over a Query Graph Model (view merging, pushdown, join/DISTINCT elimination, outer-join simplification, unnesting) and magic sets
//! LEVEL: advanced
//! Use in: SQL planners' logical-rewrite phase, view expansion, ORM/BI query generators that emit nested SELECTs, Datalog/recursive-query engines.   Run: cargo run --release --example query_rewrite_rules_magic_sets
//! COMMON USES: Starburst/DB2 "query rewrite" (QGM boxes, rule classes, the model for most later rewriters), PostgreSQL's planner (subquery pull-up, outer-join reduction, join removal, DISTINCT elimination via unique keys),
//! Apache Calcite and Spark Catalyst rule batches run to fixpoint, Datalog engines (magic-sets / demand transformation), SQL Server and Oracle view merging and subquery unnesting.
//! Course: CMU 15-799 Query Optimization (Spring 2025), lecture 03 (Starburst, query rewrites)
#![allow(dead_code)]
//!
//! WHAT: a miniature of Starburst's rewrite phase. A query is a tree of BOXES. A `Base` box is a stored table. A `Select` box owns QUANTIFIERS (range variables whose
//! type says how they feed the box: FOREACH = join, LEFT OUTER = null-supplying side, EXISTS / NOT EXISTS = subquery predicates), PREDICATES, a HEAD (output
//! expressions) and a DISTINCT flag. Every quantifier is ranged over another box, so views, derived tables and subqueries are all the same thing.
//!
//! RULES (each is `fn(&mut Select) -> bool`, true = it changed something), grouped into RULE CLASSES that the scheduler runs to a fixpoint, class by class:
//!   class 0 simplify    constant folding; CONTRADICTION detection (a=1 AND a=2, a=1 AND a>5 -> WHERE FALSE); empty-box collapse (a FALSE box drops its inputs)
//!   class 1 restructure VIEW MERGING (flatten a FOREACH over a Select box: substitute its head expressions, splice its quantifiers); EXISTS -> JOIN (valid when the
//!                       subquery matches at most one row per outer row, i.e. an equality on a key, or when the outer box is DISTINCT so duplicates vanish);
//!                       OUTER-JOIN SIMPLIFICATION (a null-rejecting WHERE predicate on the null-supplying side makes the padded rows die, so LEFT JOIN = inner join)
//!   class 2 predicates  PREDICATE TRANSITIVE CLOSURE (union-find over equalities: a=b AND b=5 => a=5, a=b AND b=c => a=c); PULL-UP (a view's `col = const` is
//!                       exported to its consumer as a derived fact so closure can use it); PUSHDOWN (a consumer predicate on one view column moves into the view)
//!   class 3 eliminate   JOIN ELIMINATION: a self-join on the key is one scan; a FOREIGN-KEY join to a parent table that nothing else reads is dropped (the FK is
//!                       NOT NULL and referentially valid, so the join neither filters nor multiplies); DISTINCT ELIMINATION when the head covers a key of every quantifier
//! SCHEDULER: sweeps the classes; inside a class repeats all rules until none fires; repeats the whole sequence until a full sweep is quiet. A TERMINATION GUARD
//! (sweep cap) turns an oscillating rule set into an error instead of a hang; a deliberately bad "flip equalities" rule demonstrates it.
//! WHY classes: simplification first exposes contradictions cheaply; merging creates the flat shape that predicate rules need; predicate rules create the
//! equalities (and the unreferenced quantifiers) that elimination needs. Rules stay local and independently testable, which is Starburst's point.
//! WHY the guards matter: pull-up and pushdown would ping-pong forever if each re-created what the other removed, so PUSHDOWN skips a predicate the view already
//! holds, and PULL-UP skips one the consumer already holds; both are monotone, so the fixpoint exists.
//! SEMANTICS: three-valued comparison (NULL = i64::MIN, any comparison with NULL is unknown and filters the row out). That is exactly why OUTER-JOIN
//! SIMPLIFICATION and FK elimination are sound only with null-rejecting predicates and NOT NULL foreign keys.
//!
//! MAGIC SETS: for recursive (and correlated/aggregate-view) queries, evaluating the whole view then filtering wastes work. The rewrite first computes the MAGIC set of
//! relevant bindings (sideways information passing): for `anc(s, Y)` with `anc(X,Y) :- edge(X,Y). anc(X,Y) :- edge(X,Z), anc(Z,Y).` the magic set is
//! `magic(s). magic(Z) :- magic(X), edge(X,Z).` and the rewritten rules are guarded by `magic(X)`, so only ancestors of reachable nodes are derived. The same trick on a
//! GROUP-BY view computes the aggregate only for the groups the outer query selects (the decorrelation form), including the COUNT-bug case of empty groups.
//!
//! VERIFICATION (asserts): 500 random databases x random query graphs (nested views, DISTINCT, left outer/exists/not exists quantifiers, contradictions) must return
//! the same row multiset before and after rewriting; every rule must fire somewhere; the scheduler must reach a fixpoint under the sweep cap and trip the guard on
//! the bad rule; magic-set answers must equal full evaluation and a BFS oracle on 500 random graphs with fewer derived facts on the sparse demo; magic group counts
//! must equal full group counts. Work (rows visited by the executor) is printed before and after; timings are machine dependent and not used.
//! Read: Extensible/Rule Based Query Rewrite Optimization in Starburst (Pirahesh, Hellerstein, Hasan, SIGMOD 1992)   Magic Sets and Other Strange Ways to Implement Logic Programs (Bancilhon, Maier, Sagiv, Ullman, PODS 1986)
//!       Magic is Relevant (Mumick, Finkelstein, Pirahesh, Ramakrishnan, SIGMOD 1990)   CMU 15-799 schedule https://15799.courses.cs.cmu.edu/spring2025/schedule.html   Datalog https://en.wikipedia.org/wiki/Datalog

use std::collections::{BTreeMap, HashMap, HashSet};

const NULL: i64 = i64::MIN;
type Row = Vec<i64>;
type Env = HashMap<usize, Row>;

// ---------------------------------------------------------------- utilities
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        ((self.next() >> 16) % n.max(1) as u64) as usize
    }
    fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- catalog and data
/// Three tables, each (key, fk-or-attr, attr). D(k,region,size); E(k,d,sal) with E.d -> D.k; P(k,e,amt) with P.e -> E.k.
/// Column 0 is a NOT NULL unique key; the FK columns are NOT NULL and reference existing parents.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Table {
    D,
    E,
    P,
}
const TABLES: [Table; 3] = [Table::D, Table::E, Table::P];
impl Table {
    fn fk(self, col: usize) -> Option<Table> {
        match (self, col) {
            (Table::E, 1) => Some(Table::D),
            (Table::P, 1) => Some(Table::E),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Table::D => "D",
            Table::E => "E",
            Table::P => "P",
        }
    }
}

struct Db {
    d: Vec<Row>,
    e: Vec<Row>,
    p: Vec<Row>,
}
impl Db {
    fn table(&self, t: Table) -> &Vec<Row> {
        match t {
            Table::D => &self.d,
            Table::E => &self.e,
            Table::P => &self.p,
        }
    }
}

fn attr(rng: &mut Rng) -> i64 {
    if rng.chance(10) {
        NULL
    } else {
        rng.below(6) as i64
    }
}

/// Small value domains (0..5) make random equalities and constants actually match; duplicates and NULLs appear in attribute columns.
fn gen_db(rng: &mut Rng) -> Db {
    let nd = 1 + rng.below(6);
    let ne = rng.below(10);
    let np = if ne > 0 { rng.below(10) } else { 0 };
    let d = (0..nd)
        .map(|i| vec![i as i64, rng.below(6) as i64, attr(rng)])
        .collect();
    let e = (0..ne)
        .map(|i| vec![i as i64, rng.below(nd) as i64, attr(rng)])
        .collect();
    let p = (0..np)
        .map(|i| vec![i as i64, rng.below(ne) as i64, attr(rng)])
        .collect();
    Db { d, e, p }
}

// ---------------------------------------------------------------- the query graph model
#[derive(Clone, PartialEq, Debug)]
enum Expr {
    Col(usize, usize), // (quantifier id, column)
    Const(i64),
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Op {
    Eq,
    Ne,
    Lt,
}
#[derive(Clone, PartialEq, Debug)]
enum Pred {
    True,
    False,
    Cmp(Op, Expr, Expr),
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    ForEach,
    LeftOuter,
    Exists,
    NotExists,
}
#[derive(Clone, Debug)]
struct Quant {
    id: usize, // unique across the whole query, so merged boxes never clash
    kind: Kind,
    on: Vec<Pred>, // join condition for LEFT OUTER, correlation condition for EXISTS / NOT EXISTS
    input: QBox,
}
#[derive(Clone, Debug)]
struct Select {
    quants: Vec<Quant>,
    preds: Vec<Pred>,
    head: Vec<Expr>,
    distinct: bool,
}
#[derive(Clone, Debug)]
enum QBox {
    Base(Table),
    Select(Select),
}
impl QBox {
    fn arity(&self) -> usize {
        match self {
            QBox::Base(_) => 3,
            QBox::Select(s) => s.head.len(),
        }
    }
}

impl Expr {
    fn mentions(&self, id: usize) -> bool {
        matches!(self, Expr::Col(q, _) if *q == id)
    }
    fn subst(&self, f: &dyn Fn(usize, usize) -> Option<Expr>) -> Expr {
        match self {
            Expr::Col(q, c) => f(*q, *c).unwrap_or_else(|| self.clone()),
            k => k.clone(),
        }
    }
}
impl Pred {
    fn subst(&self, f: &dyn Fn(usize, usize) -> Option<Expr>) -> Pred {
        match self {
            Pred::Cmp(op, a, b) => Pred::Cmp(*op, a.subst(f), b.subst(f)),
            p => p.clone(),
        }
    }
    fn mentions(&self, id: usize) -> bool {
        matches!(self, Pred::Cmp(_, a, b) if a.mentions(id) || b.mentions(id))
    }
    fn quants(&self) -> Vec<usize> {
        let mut v = vec![];
        if let Pred::Cmp(_, a, b) = self {
            for e in [a, b] {
                if let Expr::Col(q, _) = e {
                    if !v.contains(q) {
                        v.push(*q);
                    }
                }
            }
        }
        v
    }
    /// Equality is symmetric, so `a = b` and `b = a` are the same fact.
    fn same(&self, o: &Pred) -> bool {
        match (self, o) {
            (Pred::Cmp(Op::Eq, a, b), Pred::Cmp(Op::Eq, c, d)) => {
                (a == c && b == d) || (a == d && b == c)
            }
            _ => self == o,
        }
    }
}

fn subst_select(s: &mut Select, f: &dyn Fn(usize, usize) -> Option<Expr>) {
    s.preds = s.preds.iter().map(|p| p.subst(f)).collect();
    s.head = s.head.iter().map(|e| e.subst(f)).collect();
    for q in &mut s.quants {
        q.on = q.on.iter().map(|p| p.subst(f)).collect();
    }
}

// ---------------------------------------------------------------- builders and printing
fn col(q: usize, c: usize) -> Expr {
    Expr::Col(q, c)
}
fn kc(v: i64) -> Expr {
    Expr::Const(v)
}
fn cmp(op: Op, a: Expr, b: Expr) -> Pred {
    Pred::Cmp(op, a, b)
}
fn eq(a: Expr, b: Expr) -> Pred {
    Pred::Cmp(Op::Eq, a, b)
}
fn base(t: Table) -> QBox {
    QBox::Base(t)
}
fn quant(id: usize, kind: Kind, on: Vec<Pred>, input: QBox) -> Quant {
    Quant {
        id,
        kind,
        on,
        input,
    }
}
fn select(quants: Vec<Quant>, preds: Vec<Pred>, head: Vec<Expr>, distinct: bool) -> QBox {
    QBox::Select(Select {
        quants,
        preds,
        head,
        distinct,
    })
}

fn fe(e: &Expr) -> String {
    match e {
        Expr::Col(q, c) => format!("q{q}.{c}"),
        Expr::Const(k) => k.to_string(),
    }
}
fn fp(p: &Pred) -> String {
    match p {
        Pred::True => "TRUE".into(),
        Pred::False => "FALSE".into(),
        Pred::Cmp(op, a, b) => {
            let o = match op {
                Op::Eq => "=",
                Op::Ne => "<>",
                Op::Lt => "<",
            };
            format!("{} {o} {}", fe(a), fe(b))
        }
    }
}
fn join_preds(ps: &[Pred]) -> String {
    ps.iter().map(fp).collect::<Vec<_>>().join(" AND ")
}

/// EXPLAIN-style rendering of a query graph.
fn show(b: &QBox, ind: usize, out: &mut String) {
    let pad = " ".repeat(ind);
    match b {
        QBox::Base(t) => out.push_str(&format!("{pad}BASE {}\n", t.name())),
        QBox::Select(s) => {
            let head: Vec<_> = s.head.iter().map(fe).collect();
            let d = if s.distinct { "DISTINCT " } else { "" };
            out.push_str(&format!("{pad}SELECT {d}[{}]\n", head.join(", ")));
            if !s.preds.is_empty() {
                out.push_str(&format!("{pad}  WHERE {}\n", join_preds(&s.preds)));
            }
            for q in &s.quants {
                let on = if q.on.is_empty() {
                    String::new()
                } else {
                    format!(" ON {}", join_preds(&q.on))
                };
                out.push_str(&format!("{pad}  q{} {:?}{on}\n", q.id, q.kind));
                show(&q.input, ind + 6, out);
            }
        }
    }
}
fn render(b: &QBox) -> String {
    let mut s = String::new();
    show(b, 2, &mut s);
    s
}

// ---------------------------------------------------------------- executor (defines the semantics rewrites must preserve)
fn eval(e: &Expr, env: &Env) -> i64 {
    match e {
        Expr::Col(q, c) => env[q][*c],
        Expr::Const(k) => *k,
    }
}
fn holds(p: &Pred, env: &Env) -> bool {
    match p {
        Pred::True => true,
        Pred::False => false,
        Pred::Cmp(op, a, b) => {
            let (x, y) = (eval(a, env), eval(b, env));
            if x == NULL || y == NULL {
                return false; // unknown filters the row out
            }
            match op {
                Op::Eq => x == y,
                Op::Ne => x != y,
                Op::Lt => x < y,
            }
        }
    }
}

struct Exec<'a> {
    db: &'a Db,
    work: u64, // candidate rows visited: a machine-independent cost
}
impl Exec<'_> {
    fn run(&mut self, b: &QBox) -> Vec<Row> {
        match b {
            QBox::Base(t) => self.db.table(*t).clone(),
            QBox::Select(s) => self.run_select(s),
        }
    }
    fn run_select(&mut self, s: &Select) -> Vec<Row> {
        let inputs: Vec<Vec<Row>> = s.quants.iter().map(|q| self.run(&q.input)).collect();
        // Check each WHERE predicate as soon as its last quantifier is bound (stage 0 = constants only), so pushed-down predicates really save work.
        let mut stages: Vec<Vec<&Pred>> = vec![vec![]; s.quants.len() + 1];
        for p in &s.preds {
            let last = p
                .quants()
                .iter()
                .filter_map(|id| s.quants.iter().position(|q| q.id == *id))
                .max();
            stages[last.map_or(0, |m| m + 1)].push(p);
        }
        let mut env = Env::new();
        let mut out = vec![];
        if stages[0].iter().all(|p| holds(p, &env)) {
            self.walk(s, &inputs, &stages, 0, &mut env, &mut out);
        }
        if s.distinct {
            out.sort();
            out.dedup();
        }
        out
    }
    fn walk(
        &mut self,
        s: &Select,
        inputs: &[Vec<Row>],
        stages: &[Vec<&Pred>],
        i: usize,
        env: &mut Env,
        out: &mut Vec<Row>,
    ) {
        if i == s.quants.len() {
            out.push(s.head.iter().map(|e| eval(e, env)).collect());
            return;
        }
        let q = &s.quants[i];
        let on_ok = |env: &Env| q.on.iter().all(|p| holds(p, env));
        let stage_ok = |env: &Env| stages[i + 1].iter().all(|p| holds(p, env));
        match q.kind {
            Kind::ForEach => {
                for row in &inputs[i] {
                    self.work += 1;
                    env.insert(q.id, row.clone());
                    if on_ok(env) && stage_ok(env) {
                        self.walk(s, inputs, stages, i + 1, env, out);
                    }
                }
            }
            Kind::LeftOuter => {
                let mut matched = false;
                for row in &inputs[i] {
                    self.work += 1;
                    env.insert(q.id, row.clone());
                    if !on_ok(env) {
                        continue;
                    }
                    matched = true;
                    if stage_ok(env) {
                        self.walk(s, inputs, stages, i + 1, env, out);
                    }
                }
                if !matched {
                    env.insert(q.id, vec![NULL; q.input.arity()]); // padded row
                    if stage_ok(env) {
                        self.walk(s, inputs, stages, i + 1, env, out);
                    }
                }
            }
            Kind::Exists | Kind::NotExists => {
                let mut found = false;
                for row in &inputs[i] {
                    self.work += 1;
                    env.insert(q.id, row.clone());
                    if on_ok(env) {
                        found = true;
                        break;
                    }
                }
                if found == (q.kind == Kind::Exists) {
                    self.walk(s, inputs, stages, i + 1, env, out);
                }
            }
        }
        env.remove(&q.id);
    }
}

fn run_sorted(db: &Db, q: &QBox) -> (Vec<Row>, u64) {
    let mut ex = Exec { db, work: 0 };
    let mut rows = ex.run(q);
    rows.sort();
    (rows, ex.work)
}

// ---------------------------------------------------------------- the rules
fn count_mentions(ps: &[Pred], id: usize) -> usize {
    ps.iter().filter(|p| p.mentions(id)).count()
}
/// How many places (WHERE, head, other quantifiers' ON) read quantifier `id`.
fn refs(s: &Select, id: usize) -> usize {
    count_mentions(&s.preds, id)
        + s.head.iter().filter(|e| e.mentions(id)).count()
        + s.quants
            .iter()
            .map(|q| count_mentions(&q.on, id))
            .sum::<usize>()
}
fn has_pred(ps: &[Pred], p: &Pred) -> bool {
    ps.iter().any(|x| x.same(p))
}

fn fold(p: &Pred) -> Pred {
    let Pred::Cmp(op, a, b) = p else {
        return p.clone();
    };
    if let (Expr::Const(x), Expr::Const(y)) = (a, b) {
        let t = match op {
            Op::Eq => x == y,
            Op::Ne => x != y,
            Op::Lt => x < y,
        };
        return if t { Pred::True } else { Pred::False };
    }
    // x <> x and x < x are never true (NULL gives unknown), but x = x is NOT folded: it is false for NULL.
    if a == b && matches!(op, Op::Ne | Op::Lt) {
        return Pred::False;
    }
    p.clone()
}

fn rule_const_fold(s: &mut Select) -> bool {
    let mut out: Vec<Pred> = vec![];
    for p in &s.preds {
        let f = fold(p);
        if f != Pred::True && !out.contains(&f) {
            out.push(f);
        }
    }
    if out.contains(&Pred::False) {
        out = vec![Pred::False];
    }
    let changed = out != s.preds;
    s.preds = out;
    changed
}

/// `col op const` (or the mirrored form for symmetric operators) as (column, op, const).
fn col_const(p: &Pred) -> Option<(Expr, Op, i64)> {
    match p {
        Pred::Cmp(op, c @ Expr::Col(..), Expr::Const(k)) => Some((c.clone(), *op, *k)),
        Pred::Cmp(op, Expr::Const(k), c @ Expr::Col(..)) if *op != Op::Lt => {
            Some((c.clone(), *op, *k))
        }
        _ => None,
    }
}
fn rule_contradiction(s: &mut Select) -> bool {
    if s.preds == [Pred::False] {
        return false;
    }
    let mut known: Vec<(Expr, i64)> = vec![];
    for p in &s.preds {
        if let Some((c, Op::Eq, k)) = col_const(p) {
            known.push((c, k));
        }
    }
    let dead = s.preds.iter().any(|p| match col_const(p) {
        Some((c, op, m)) => known.iter().any(|(kc_, k)| {
            *kc_ == c && !holds(&Pred::Cmp(op, Expr::Const(*k), Expr::Const(m)), &Env::new())
        }),
        None => false,
    });
    if dead {
        s.preds = vec![Pred::False];
    }
    dead
}

/// A box that can never produce a row drops its inputs (keeping its arity), so the executor never scans them.
fn rule_empty_collapse(s: &mut Select) -> bool {
    if s.preds == [Pred::False] && !s.quants.is_empty() {
        s.quants.clear();
        s.head = vec![Expr::Const(0); s.head.len()];
        return true;
    }
    false
}

fn rule_view_merge(s: &mut Select) -> bool {
    let target = s.quants.iter().position(|q| {
        q.kind == Kind::ForEach
            && matches!(&q.input, QBox::Select(v) if v.quants.iter().all(|x| x.kind == Kind::ForEach) && (!v.distinct || s.distinct))
    });
    let Some(i) = target else { return false };
    let on = std::mem::take(&mut s.quants[i].on);
    s.preds.extend(on);
    let q = s.quants.remove(i);
    let QBox::Select(v) = q.input else {
        unreachable!()
    };
    let head = v.head.clone();
    // Consumers of the view column c now read the view's defining expression instead.
    subst_select(s, &|qq, c| (qq == q.id).then(|| head[c].clone()));
    s.preds.extend(v.preds);
    for (k, vq) in v.quants.into_iter().enumerate() {
        s.quants.insert(i + k, vq);
    }
    true
}

fn rule_exists_to_join(s: &mut Select) -> bool {
    for i in 0..s.quants.len() {
        let q = &s.quants[i];
        if q.kind != Kind::Exists {
            continue;
        }
        // Equality on the key of a base table: at most one match per outer row, so the join cannot multiply rows.
        let key_eq = |a: &Expr, b: &Expr| *a == Expr::Col(q.id, 0) && !b.mentions(q.id);
        let unique = matches!(q.input, QBox::Base(_))
            && q.on
                .iter()
                .any(|p| matches!(p, Pred::Cmp(Op::Eq, a, b) if key_eq(a, b) || key_eq(b, a)));
        if unique || s.distinct {
            let on = std::mem::take(&mut s.quants[i].on);
            s.quants[i].kind = Kind::ForEach;
            s.preds.extend(on);
            return true;
        }
    }
    false
}

fn rule_oj_simplify(s: &mut Select) -> bool {
    for i in 0..s.quants.len() {
        let id = s.quants[i].id;
        // A comparison involving a column of the null-supplying side is unknown on padded rows, so the WHERE clause throws those rows away.
        if s.quants[i].kind == Kind::LeftOuter && s.preds.iter().any(|p| p.mentions(id)) {
            let on = std::mem::take(&mut s.quants[i].on);
            s.quants[i].kind = Kind::ForEach;
            s.preds.extend(on);
            return true;
        }
    }
    false
}

fn node_id(nodes: &mut Vec<Expr>, parent: &mut Vec<usize>, e: &Expr) -> usize {
    nodes.iter().position(|n| n == e).unwrap_or_else(|| {
        nodes.push(e.clone());
        parent.push(nodes.len() - 1);
        nodes.len() - 1
    })
}
fn find(parent: &[usize], mut x: usize) -> usize {
    while parent[x] != x {
        x = parent[x];
    }
    x
}
fn rule_transitive(s: &mut Select) -> bool {
    let (mut nodes, mut parent) = (vec![], vec![]);
    for p in s.preds.clone() {
        if let Pred::Cmp(Op::Eq, a, b) = p {
            let (ia, ib) = (
                node_id(&mut nodes, &mut parent, &a),
                node_id(&mut nodes, &mut parent, &b),
            );
            let (ra, rb) = (find(&parent, ia), find(&parent, ib));
            parent[ra] = rb;
        }
    }
    let mut classes: HashMap<usize, Vec<Expr>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        classes.entry(find(&parent, i)).or_default().push(n.clone());
    }
    let mut added = vec![];
    for members in classes.values() {
        let cols: Vec<&Expr> = members
            .iter()
            .filter(|m| matches!(m, Expr::Col(..)))
            .collect();
        if let Some(k) = members.iter().find(|m| matches!(m, Expr::Const(_))) {
            // The class is pinned to a constant: every column in it equals that constant.
            added.extend(cols.iter().map(|c| eq((*c).clone(), k.clone())));
        } else {
            for (i, a) in cols.iter().enumerate() {
                added.extend(cols[i + 1..].iter().map(|b| eq((*a).clone(), (*b).clone())));
            }
        }
    }
    let mut changed = false;
    for p in added {
        if !has_pred(&s.preds, &p) {
            s.preds.push(p);
            changed = true;
        }
    }
    changed
}

fn rule_pull_up(s: &mut Select) -> bool {
    let mut adds = vec![];
    for q in s.quants.iter().filter(|q| q.kind == Kind::ForEach) {
        let QBox::Select(v) = &q.input else { continue };
        for p in &v.preds {
            if let Pred::Cmp(Op::Eq, a @ Expr::Col(..), Expr::Const(k)) = p {
                if let Some(j) = v.head.iter().position(|h| h == a) {
                    adds.push(eq(col(q.id, j), kc(*k)));
                }
            }
        }
    }
    let mut changed = false;
    for p in adds {
        if !has_pred(&s.preds, &p) {
            s.preds.push(p);
            changed = true;
        }
    }
    changed
}

fn rule_push_down(s: &mut Select) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < s.preds.len() {
        let p = s.preds[i].clone();
        let qs = p.quants();
        let target = (qs.len() == 1)
            .then(|| {
                s.quants.iter().position(|q| {
                    q.id == qs[0] && q.kind == Kind::ForEach && matches!(q.input, QBox::Select(_))
                })
            })
            .flatten();
        if let Some(t) = target {
            let qid = qs[0];
            let QBox::Select(v) = &mut s.quants[t].input else {
                unreachable!()
            };
            let moved = fold(&p.subst(&|qq, c| (qq == qid).then(|| v.head[c].clone())));
            // A TRUE result (the view column is that very constant) is left in place: dropping it would let transitive closure re-derive it forever.
            if moved != Pred::True && !has_pred(&v.preds, &moved) {
                v.preds.push(moved);
                s.preds.remove(i);
                changed = true;
                continue;
            }
        }
        i += 1;
    }
    changed
}

fn rule_self_join_elim(s: &mut Select) -> bool {
    for j in 0..s.quants.len() {
        for i in 0..j {
            let (qi, qj) = (&s.quants[i], &s.quants[j]);
            let same_base =
                matches!((&qi.input, &qj.input), (QBox::Base(a), QBox::Base(b)) if a == b);
            if !(same_base && qi.kind == Kind::ForEach && qj.kind == Kind::ForEach) {
                continue;
            }
            let key_pred = eq(col(qi.id, 0), col(qj.id, 0));
            let Some(pi) = s.preds.iter().position(|p| p.same(&key_pred)) else {
                continue;
            };
            // Same key on the same table is the same row: the second quantifier is a duplicate of the first.
            let (keep, drop) = (qi.id, qj.id);
            s.preds.remove(pi);
            s.quants.remove(j);
            subst_select(s, &|q, c| (q == drop).then(|| col(keep, c)));
            return true;
        }
    }
    false
}

fn rule_fk_join_elim(s: &mut Select) -> bool {
    for pi in 0..s.preds.len() {
        let Pred::Cmp(Op::Eq, a, b) = s.preds[pi].clone() else {
            continue;
        };
        for (x, y) in [(&a, &b), (&b, &a)] {
            // x = child.fk, y = parent.key
            let (Expr::Col(ca, fc), Expr::Col(pb, 0)) = (x, y) else {
                continue;
            };
            let find_q = |id: usize| {
                s.quants
                    .iter()
                    .position(|q| q.id == id && q.kind == Kind::ForEach)
            };
            let (Some(ia), Some(ib)) = (find_q(*ca), find_q(*pb)) else {
                continue;
            };
            let (QBox::Base(ta), QBox::Base(tb)) = (&s.quants[ia].input, &s.quants[ib].input)
            else {
                continue;
            };
            // Exactly one parent row exists for each child row (NOT NULL FK + referential integrity), and nothing else reads the parent.
            if ta.fk(*fc) == Some(*tb) && refs(s, *pb) == 1 && s.quants[ib].on.is_empty() {
                s.preds.remove(pi);
                s.quants.remove(ib);
                return true;
            }
        }
    }
    false
}

fn rule_distinct_elim(s: &mut Select) -> bool {
    if !s.distinct {
        return false;
    }
    let pinned = |id: usize| {
        s.preds.iter().any(|p| matches!(p, Pred::Cmp(Op::Eq, a, b)
            if (*a == Expr::Col(id, 0) && matches!(b, Expr::Const(_))) || (*b == Expr::Col(id, 0) && matches!(a, Expr::Const(_)))))
    };
    // A quantifier is "identified" when the head determines its row: it exports its key (or it is pinned to one key by a constant), or it is a DISTINCT view whose
    // every column is exported. If all quantifiers are identified, equal head rows imply the same row combination, so no duplicates can exist.
    let identified = |q: &Quant| match &q.input {
        QBox::Base(_) => s.head.contains(&Expr::Col(q.id, 0)) || pinned(q.id),
        QBox::Select(v) => {
            v.distinct && (0..q.input.arity()).all(|c| s.head.contains(&Expr::Col(q.id, c)))
        }
    };
    let unique = s
        .quants
        .iter()
        .filter(|q| matches!(q.kind, Kind::ForEach | Kind::LeftOuter))
        .all(identified);
    if unique {
        s.distinct = false;
    }
    unique
}

/// A deliberately broken rule that swaps the sides of every equality and always claims progress: the scheduler's guard must catch it.
fn rule_bad_flip(s: &mut Select) -> bool {
    let mut any = false;
    for p in &mut s.preds {
        if let Pred::Cmp(Op::Eq, a, b) = p {
            std::mem::swap(a, b);
            any = true;
        }
    }
    any
}

// ---------------------------------------------------------------- rule scheduler
struct Rule {
    name: &'static str,
    class: usize,
    f: fn(&mut Select) -> bool,
}
const RULES: [Rule; 12] = [
    Rule {
        name: "const_fold",
        class: 0,
        f: rule_const_fold,
    },
    Rule {
        name: "contradiction",
        class: 0,
        f: rule_contradiction,
    },
    Rule {
        name: "empty_collapse",
        class: 0,
        f: rule_empty_collapse,
    },
    Rule {
        name: "view_merge",
        class: 1,
        f: rule_view_merge,
    },
    Rule {
        name: "exists_to_join",
        class: 1,
        f: rule_exists_to_join,
    },
    Rule {
        name: "oj_simplify",
        class: 1,
        f: rule_oj_simplify,
    },
    Rule {
        name: "transitive",
        class: 2,
        f: rule_transitive,
    },
    Rule {
        name: "pull_up",
        class: 2,
        f: rule_pull_up,
    },
    Rule {
        name: "push_down",
        class: 2,
        f: rule_push_down,
    },
    Rule {
        name: "self_join_elim",
        class: 3,
        f: rule_self_join_elim,
    },
    Rule {
        name: "fk_join_elim",
        class: 3,
        f: rule_fk_join_elim,
    },
    Rule {
        name: "distinct_elim",
        class: 3,
        f: rule_distinct_elim,
    },
];
const BAD_RULES: [Rule; 1] = [Rule {
    name: "bad_flip",
    class: 0,
    f: rule_bad_flip,
}];

/// Apply one rule to every box, children first (a view is simplified before its consumer looks at it). Returns how many boxes changed.
fn apply_rule(b: &mut QBox, r: &Rule) -> usize {
    match b {
        QBox::Base(_) => 0,
        QBox::Select(s) => {
            let below: usize = s
                .quants
                .iter_mut()
                .map(|q| apply_rule(&mut q.input, r))
                .sum();
            below + usize::from((r.f)(s))
        }
    }
}

/// Run rule classes to a fixpoint. `cap` bounds the number of productive passes so a non-terminating rule set is reported, not hung.
fn rewrite(
    root: &mut QBox,
    rules: &[Rule],
    fired: &mut BTreeMap<&'static str, usize>,
    cap: usize,
) -> Result<usize, String> {
    let classes = rules.iter().map(|r| r.class).max().unwrap_or(0);
    let mut passes = 0;
    loop {
        let mut any = false;
        for class in 0..=classes {
            loop {
                let mut n = 0;
                for r in rules.iter().filter(|r| r.class == class) {
                    let k = apply_rule(root, r);
                    *fired.entry(r.name).or_default() += k;
                    n += k;
                }
                if n == 0 {
                    break;
                }
                any = true;
                passes += 1;
                if passes > cap {
                    return Err(format!("no fixpoint after {cap} productive passes"));
                }
            }
        }
        if !any {
            return Ok(passes);
        }
    }
}

// ---------------------------------------------------------------- random query graphs
type Visible = (usize, usize, Option<Table>); // (quantifier id, arity, base table if any)

fn gen_link(
    rng: &mut Rng,
    earlier: &[Visible],
    id: usize,
    arity: usize,
    tbl: Option<Table>,
) -> Pred {
    let (pid, parity, ptbl) = earlier[rng.below(earlier.len())];
    if let (Some(t), Some(pt)) = (tbl, ptbl) {
        for c in 1..3 {
            if t.fk(c) == Some(pt) && rng.chance(70) {
                return eq(col(id, c), col(pid, 0)); // child.fk = parent.key
            }
            if pt.fk(c) == Some(t) && rng.chance(70) {
                return eq(col(pid, c), col(id, 0));
            }
        }
        if t == pt && rng.chance(40) {
            return eq(col(id, 0), col(pid, 0)); // self-join on the key
        }
    }
    let op = if rng.chance(10) { Op::Lt } else { Op::Eq };
    let c = if rng.chance(30) { 0 } else { rng.below(arity) };
    cmp(op, col(id, c), col(pid, rng.below(parity)))
}

fn gen_select(rng: &mut Rng, ids: &mut usize, depth: u32) -> Select {
    let nq = 1 + rng.below(3);
    let mut quants: Vec<Quant> = vec![];
    let mut visible: Vec<Visible> = vec![];
    let mut preds = vec![];
    for i in 0..nq {
        let (input, tbl) = if depth > 0 && rng.chance(35) {
            (QBox::Select(gen_select(rng, ids, depth - 1)), None)
        } else {
            let t = TABLES[rng.below(3)];
            (QBox::Base(t), Some(t))
        };
        let id = *ids;
        *ids += 1;
        let kind = match (i, rng.below(10)) {
            (0, _) | (_, 0..=4) => Kind::ForEach,
            (_, 5 | 6) => Kind::LeftOuter,
            (_, 7 | 8) => Kind::Exists,
            _ => Kind::NotExists,
        };
        let arity = input.arity();
        let mut link = vec![];
        if i > 0 {
            link.push(gen_link(rng, &visible, id, arity, tbl));
            if rng.chance(15) {
                link.push(cmp(
                    Op::Lt,
                    col(id, rng.below(arity)),
                    kc(rng.below(6) as i64),
                ));
            }
        }
        let on = if kind == Kind::ForEach {
            preds.extend(link);
            vec![]
        } else {
            link
        };
        if matches!(kind, Kind::ForEach | Kind::LeftOuter) {
            visible.push((id, arity, tbl));
        }
        quants.push(quant(id, kind, on, input));
    }
    for _ in 0..rng.below(3) {
        let (id, ar, _) = visible[rng.below(visible.len())];
        let op = [Op::Eq, Op::Eq, Op::Lt, Op::Ne][rng.below(4)];
        preds.push(cmp(op, col(id, rng.below(ar)), kc(rng.below(6) as i64)));
    }
    if rng.chance(6) {
        let (id, ar, _) = visible[rng.below(visible.len())];
        let c = rng.below(ar);
        preds.push(eq(col(id, c), kc(1)));
        preds.push(eq(col(id, c), kc(2)));
    }
    let head = (0..1 + rng.below(3))
        .map(|_| {
            if rng.chance(8) {
                kc(rng.below(3) as i64)
            } else {
                let (id, ar, _) = visible[rng.below(visible.len())];
                col(id, rng.below(ar))
            }
        })
        .collect();
    Select {
        quants,
        preds,
        head,
        distinct: rng.chance(30),
    }
}

// ---------------------------------------------------------------- hand-written demo queries
fn demos() -> Vec<(&'static str, QBox)> {
    use Kind::*;
    use Table::*;
    let view_merge = {
        let v = select(
            vec![quant(2, ForEach, vec![], base(E))],
            vec![cmp(Op::Lt, col(2, 2), kc(4))],
            vec![col(2, 0), col(2, 1)],
            false,
        );
        select(
            vec![
                quant(1, ForEach, vec![], v),
                quant(3, ForEach, vec![], base(D)),
            ],
            vec![
                eq(col(1, 1), col(3, 0)),
                eq(col(3, 0), kc(2)),
                cmp(Op::Lt, kc(1), kc(2)),
            ],
            vec![col(1, 0)],
            false,
        )
    };
    let contradiction = select(
        vec![
            quant(1, ForEach, vec![], base(E)),
            quant(2, ForEach, vec![], base(D)),
        ],
        vec![
            eq(col(1, 1), kc(1)),
            eq(col(1, 1), col(2, 0)),
            eq(col(2, 0), kc(2)),
        ],
        vec![col(1, 0)],
        false,
    );
    let outer = select(
        vec![
            quant(1, ForEach, vec![], base(E)),
            quant(2, LeftOuter, vec![eq(col(1, 1), col(2, 0))], base(D)),
        ],
        vec![cmp(Op::Lt, col(2, 2), kc(3))],
        vec![col(1, 0), col(2, 2)],
        false,
    );
    let fk = select(
        vec![
            quant(1, ForEach, vec![], base(E)),
            quant(2, ForEach, vec![], base(D)),
        ],
        vec![eq(col(1, 1), col(2, 0))],
        vec![col(1, 0)],
        false,
    );
    let exists = select(
        vec![
            quant(1, ForEach, vec![], base(E)),
            quant(2, Exists, vec![eq(col(2, 0), col(1, 1))], base(D)),
        ],
        vec![],
        vec![col(1, 0), col(1, 2)],
        false,
    );
    let selfjoin = select(
        vec![
            quant(1, ForEach, vec![], base(E)),
            quant(2, ForEach, vec![], base(E)),
        ],
        vec![eq(col(1, 0), col(2, 0)), cmp(Op::Lt, col(1, 2), kc(3))],
        vec![col(1, 0), col(2, 2)],
        false,
    );
    let distinct = select(
        vec![quant(1, ForEach, vec![], base(E))],
        vec![],
        vec![col(1, 0), col(1, 2)],
        true,
    );
    let pushpull = {
        let v = select(
            vec![quant(2, ForEach, vec![], base(E))],
            vec![eq(col(2, 1), kc(2))],
            vec![col(2, 1)],
            true,
        );
        select(
            vec![
                quant(1, ForEach, vec![], v),
                quant(3, ForEach, vec![], base(D)),
            ],
            vec![eq(col(1, 0), col(3, 0)), cmp(Op::Lt, col(1, 0), kc(4))],
            vec![col(3, 1)],
            false,
        )
    };
    let notexists = select(
        vec![
            quant(1, ForEach, vec![], base(D)),
            quant(2, NotExists, vec![eq(col(2, 1), col(1, 0))], base(E)),
        ],
        vec![],
        vec![col(1, 0)],
        false,
    );
    vec![
        (
            "view merging + constant folding + transitive closure",
            view_merge,
        ),
        (
            "contradiction detection (e.d=1 AND e.d=d.k AND d.k=2)",
            contradiction,
        ),
        (
            "outer-join simplification (null-rejecting WHERE on the D side)",
            outer,
        ),
        ("foreign-key join elimination (D is never read)", fk),
        ("EXISTS -> join -> FK join elimination", exists),
        ("self-join elimination on the key", selfjoin),
        ("DISTINCT elimination (head contains the key)", distinct),
        (
            "pull-up + transitive closure + pushdown through a DISTINCT view",
            pushpull,
        ),
        (
            "NOT EXISTS stays an anti-semi-join (no unsound rewrite)",
            notexists,
        ),
    ]
}

// ---------------------------------------------------------------- magic sets
/// Semi-naive evaluation of `anc(X,Y) :- edge(X,Y). anc(X,Y) :- edge(X,Z), anc(Z,Y).` restricted to start nodes accepted by `allowed`.
/// Work counts every (edge, delta fact) join probe, the usual measure of Datalog evaluation effort.
fn ancestors(
    edges: &[(usize, usize)],
    n: usize,
    allowed: &dyn Fn(usize) -> bool,
) -> (HashSet<(usize, usize)>, u64) {
    let mut incoming: Vec<Vec<usize>> = vec![vec![]; n]; // incoming[z] = all x with edge x -> z (and x allowed)
    let mut anc: HashSet<(usize, usize)> = HashSet::new();
    let mut delta: Vec<(usize, usize)> = vec![];
    let mut work = 0u64;
    for &(x, z) in edges.iter().filter(|(x, _)| allowed(*x)) {
        incoming[z].push(x);
        work += 1;
        if anc.insert((x, z)) {
            delta.push((x, z));
        }
    }
    while !delta.is_empty() {
        let mut next = vec![];
        for (z, y) in delta {
            for &x in &incoming[z] {
                work += 1;
                if anc.insert((x, y)) {
                    next.push((x, y));
                }
            }
        }
        delta = next;
    }
    (anc, work)
}

/// `magic(s). magic(Z) :- magic(X), edge(X,Z).` plus the guarded ancestor rules; returns (anc_magic facts, magic set, total work).
fn magic_ancestors(
    edges: &[(usize, usize)],
    n: usize,
    s: usize,
) -> (HashSet<(usize, usize)>, HashSet<usize>, u64) {
    let mut out: Vec<Vec<usize>> = vec![vec![]; n];
    for &(x, z) in edges {
        out[x].push(z);
    }
    let mut magic: HashSet<usize> = HashSet::from([s]);
    let (mut stack, mut work) = (vec![s], 0u64);
    while let Some(x) = stack.pop() {
        for &z in &out[x] {
            work += 1;
            if magic.insert(z) {
                stack.push(z);
            }
        }
    }
    let (anc, w) = ancestors(edges, n, &|x| magic.contains(&x));
    (anc, magic, work + w)
}

/// Oracle: nodes reachable from `s` by one or more edges (plain DFS, shares no code with the Datalog evaluation).
fn reach_oracle(edges: &[(usize, usize)], n: usize, s: usize) -> HashSet<usize> {
    let mut seen = vec![false; n];
    let mut stack: Vec<usize> = edges.iter().filter(|e| e.0 == s).map(|e| e.1).collect();
    while let Some(x) = stack.pop() {
        if !std::mem::replace(&mut seen[x], true) {
            stack.extend(edges.iter().filter(|e| e.0 == x).map(|e| e.1));
        }
    }
    (0..n).filter(|&x| seen[x]).collect()
}

fn check_magic_on(edges: &[(usize, usize)], n: usize, s: usize) -> (u64, u64) {
    let (full, w_full) = ancestors(edges, n, &|_| true);
    let (am, magic, w_magic) = magic_ancestors(edges, n, s);
    let from_full: HashSet<usize> = full.iter().filter(|f| f.0 == s).map(|f| f.1).collect();
    let from_magic: HashSet<usize> = am.iter().filter(|f| f.0 == s).map(|f| f.1).collect();
    assert_eq!(
        from_full, from_magic,
        "magic answers differ from full evaluation"
    );
    assert_eq!(
        from_full,
        reach_oracle(edges, n, s),
        "evaluation differs from DFS oracle"
    );
    assert!(
        am.is_subset(&full),
        "magic must only derive facts the full program derives"
    );
    assert!(magic.contains(&s));
    (w_full, w_magic)
}

/// Correlated/aggregate-view form: `SELECT d.k, (SELECT COUNT(*) FROM E WHERE E.d = d.k) FROM D d WHERE d.region = r`.
/// Full: group the whole of E, then look up each selected dept (missing group -> 0, the COUNT bug). Magic: only count E rows of the selected depts (E.d is indexed).
fn dept_counts(db: &Db, region: i64, use_magic: bool) -> (Vec<(i64, i64)>, u64) {
    let selected: Vec<i64> =
        db.d.iter()
            .filter(|r| r[1] == region)
            .map(|r| r[0])
            .collect();
    let mut groups: HashMap<i64, i64> = HashMap::new();
    let mut work = 0u64;
    if use_magic {
        let magic: HashSet<i64> = selected.iter().copied().collect(); // the magic set of relevant group keys
        for r in db.e.iter().filter(|r| magic.contains(&r[1])) {
            *groups.entry(r[1]).or_default() += 1;
            work += 1; // an index on E.d would touch only these rows
        }
    } else {
        for r in &db.e {
            *groups.entry(r[1]).or_default() += 1;
            work += 1;
        }
    }
    let mut out: Vec<(i64, i64)> = selected
        .iter()
        .map(|k| (*k, groups.get(k).copied().unwrap_or(0)))
        .collect();
    out.sort();
    (out, work)
}

// ---------------------------------------------------------------- main: demo + verification
fn main() {
    println!("== 1. rewrite demos (before -> after, executed on one fixed database) ==");
    let demo_db = gen_db(&mut Rng::new(7));
    println!(
        "fixed database: |D|={} |E|={} |P|={}",
        demo_db.d.len(),
        demo_db.e.len(),
        demo_db.p.len()
    );
    let mut fired: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (name, q) in demos() {
        let mut after = q.clone();
        let mut local = BTreeMap::new();
        let passes = rewrite(&mut after, &RULES, &mut local, 100).expect("demo reaches a fixpoint");
        let (r0, w0) = run_sorted(&demo_db, &q);
        let (r1, w1) = run_sorted(&demo_db, &after);
        assert_eq!(r0, r1, "demo '{name}' changed the result");
        println!(
            "\n-- {name}\nBEFORE (work {w0}, {} rows)\n{}AFTER  (work {w1}, {passes} passes; fired: {})\n{}",
            r0.len(),
            render(&q),
            local
                .iter()
                .filter(|(_, n)| **n > 0)
                .map(|(k, n)| format!("{k} x{n}"))
                .collect::<Vec<_>>()
                .join(", "),
            render(&after)
        );
        for (k, n) in local {
            *fired.entry(k).or_default() += n;
        }
    }

    println!("\n== 2. 500 random databases x random query graphs: results must not change ==");
    let (mut total0, mut total1, mut nonempty, mut changed, mut worse) = (0u64, 0u64, 0, 0, 0);
    for seed in 0..500u64 {
        let mut rng = Rng::new(1000 + seed);
        let db = gen_db(&mut rng);
        for _ in 0..2 {
            let mut ids = 1;
            let q = QBox::Select(gen_select(&mut rng, &mut ids, 2));
            let mut after = q.clone();
            let mut tr = BTreeMap::new();
            if rewrite(&mut after, &RULES, &mut tr, 200).is_err() {
                println!("{}\n{}\n{:?}", render(&q), render(&after), tr);
                panic!();
            }
            for (k, n) in tr {
                *fired.entry(k).or_default() += n;
            }
            assert_eq!(q.arity(), after.arity(), "rewrite changed the output arity");
            let (r0, w0) = run_sorted(&db, &q);
            let (r1, w1) = run_sorted(&db, &after);
            assert_eq!(
                r0,
                r1,
                "seed {seed}: rewrite changed the result\nBEFORE\n{}AFTER\n{}",
                render(&q),
                render(&after)
            );
            total0 += w0;
            total1 += w1;
            nonempty += usize::from(!r0.is_empty());
            changed += usize::from(render(&q) != render(&after));
            worse += usize::from(w1 > w0);
        }
    }
    println!(
        "1000 queries: {nonempty} non-empty results, {changed} rewritten, executor work {total0} -> {total1} ({worse} queries did more work)"
    );
    assert!(nonempty >= 200, "generator is too often vacuous");
    assert!(
        changed >= 400,
        "rules should fire on most generated queries"
    );
    assert!(total1 <= total0, "rewriting must not increase total work");
    println!("rule firing counts (demos + random):");
    for r in &RULES {
        let n = fired.get(r.name).copied().unwrap_or(0);
        println!("  class {} {:<15} {n}", r.class, r.name);
        assert!(n > 0, "rule {} never fired", r.name);
    }

    println!("\n== 3. termination guard ==");
    let (name, mut q) = demos().into_iter().nth(3).unwrap();
    let verdict = rewrite(&mut q, &BAD_RULES, &mut BTreeMap::new(), 50);
    println!("bad oscillating rule on '{name}': {verdict:?}");
    assert!(
        verdict.is_err(),
        "a rule set that never converges must trip the guard"
    );

    println!("\n== 4. magic sets: recursive ancestor query anc(s, Y) ==");
    // Sparse demo: one long chain 0..39 (s = 30 sees only 9 descendants) plus a dense component 40..59.
    let mut edges: Vec<(usize, usize)> = (0..39).map(|i| (i, i + 1)).collect();
    let mut rng = Rng::new(99);
    for _ in 0..60 {
        edges.push((40 + rng.below(20), 40 + rng.below(20)));
    }
    let (w_full, w_magic) = check_magic_on(&edges, 60, 30);
    println!("source 30: full closure work {w_full} vs magic-set work {w_magic}");
    assert!(
        w_magic * 5 < w_full,
        "magic sets should avoid deriving facts unrelated to the source"
    );
    let (mut sum_full, mut sum_magic) = (0u64, 0u64);
    for seed in 0..500u64 {
        let mut rng = Rng::new(5000 + seed);
        let n = 3 + rng.below(25);
        let edges: Vec<(usize, usize)> = (0..rng.below(2 * n + 1))
            .map(|_| (rng.below(n), rng.below(n)))
            .collect();
        let (wf, wm) = check_magic_on(&edges, n, rng.below(n));
        sum_full += wf;
        sum_magic += wm;
    }
    println!(
        "500 random graphs (cycles, self-loops, isolated sources): answers equal full evaluation and DFS oracle; work {sum_full} -> {sum_magic}"
    );

    println!(
        "\n== 5. magic set for a correlated COUNT view (COUNT bug: empty groups must be 0) =="
    );
    let (mut zeros, mut wf_sum, mut wm_sum) = (0, 0u64, 0u64);
    for seed in 0..500u64 {
        let mut rng = Rng::new(9000 + seed);
        let db = gen_db(&mut rng);
        let region = rng.below(6) as i64;
        let (full, wf) = dept_counts(&db, region, false);
        let (magic, wm) = dept_counts(&db, region, true);
        assert_eq!(full, magic, "magic group counts differ");
        zeros += full.iter().filter(|(_, c)| *c == 0).count();
        wf_sum += wf;
        wm_sum += wm;
    }
    println!(
        "500 databases: counts equal; {zeros} departments with COUNT 0 kept; rows touched {wf_sum} -> {wm_sum}"
    );
    assert!(zeros > 0, "the COUNT-bug case never occurred");
    assert!(wm_sum <= wf_sum);
    println!("\nall checks passed");
}
