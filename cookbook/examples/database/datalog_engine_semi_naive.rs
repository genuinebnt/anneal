//! RECIPE 842 (database): a Datalog engine: parser, safety checks, stratified negation/aggregation, naive vs semi-naive, indexes, magic sets.
//! LEVEL: advanced
//! Use in: program analysis, access control/policy engines, graph reachability, knowledge graphs, query optimisers (magic sets).   Run: cargo run --release --example datalog_engine_semi_naive
//! COMMON USES: Soufflé (points-to and security analyses over large codebases), Doop (Java pointer analysis), CodeQL and
//! Semmle QL descend from Datalog, Datomic queries, LogicBlox, Differential Datalog, Cozo, Google's Zanzibar-style
//! permission checks as recursive group membership, and magic-set rewriting inside SQL engines for recursive CTEs.
//! Course: CMU 15-721 (recursive queries) and Abiteboul-Hull-Vianu "Foundations of Databases" (the "Alice book").
#![allow(dead_code)]
//!
//! WHAT. Datalog is SQL's recursive cousin: a program is FACTS (`edge(1,2).`) and RULES (`path(X,Z) :- path(X,Y), edge(Y,Z).`);
//! lower-case terms are constants, Capitalised terms are variables; the answer is the least set of facts closed under the rules.
//! This file builds the whole pipeline:
//!   1. PARSER: a tiny lexer plus recursive-descent parser for facts, rules, `not p(..)`, `X != Y`, aggregate heads
//!      `n(X, count(Y))`, and a goal `?- path(1, X).`
//!   2. SAFETY: every head variable and every variable under negation or `!=` must also occur in a POSITIVE body atom, otherwise the
//!      result could be infinite or ill-defined (`p(X) :- not q(X).` is rejected).
//!   3. STRATIFICATION: build the predicate dependency graph; an edge is STRICT when it goes through `not` or an aggregate. If a
//!      strict edge lies on a cycle the program has no clean meaning (`win(X) :- move(X,Y), not win(Y).`) and is rejected.
//!      Otherwise stratum(h) = max over body predicates of stratum(b) + (1 if strict) is computed by relaxation, and strata are
//!      evaluated bottom-up so a negated or aggregated relation is COMPLETE before anyone looks at it.
//!   4. NAIVE evaluation re-applies every rule to the full database until nothing new appears: round k recomputes every fact found
//!      in rounds < k. SEMI-NAIVE keeps a DELTA (the tuples new in the previous round) and, for a rule with recursive body atoms
//!      b_1..b_m, evaluates m variants: variant i joins DELTA at b_i, OLD (full minus delta) at b_j for j<i and FULL for j>i. Every
//!      new derivation uses at least one new tuple, and the OLD/FULL split makes each derivation appear in exactly one variant.
//!      Storage trick: a relation is an append-only Vec, so "delta" and "old" are just index ranges into it.
//!   5. INDEXES: the join plan for each rule variant is chosen greedily (start from the delta atom, then the atom with the most
//!      already-bound columns; filters run as soon as their variables are bound). The bound-column sets of the chosen plan ARE the
//!      indexes the engine builds (hash maps from key columns to ascending tuple positions), so no index is built that no rule uses.
//!   6. AGGREGATION (count/min/max/sum over the DISTINCT values of the aggregate variable per group) is evaluated in a stratum above
//!      its input, once, because aggregates are non-monotonic.
//!   7. MAGIC SETS (lite): for a goal like `path(1,X)` (binding pattern `bf`) rewrite the program so bottom-up evaluation only derives
//!      facts relevant to the goal: each IDB atom gets an ADORNMENT (b/f per argument, propagated left to right through the body),
//!      a `m_p__bf(bound args)` "magic" predicate collects the bindings that are ever asked for, every rule is guarded by its magic
//!      atom, and each IDB body atom spawns a rule that passes bindings sideways to it. Only positive programs are supported.
//!
//! CHECKS. (1) Worked programs with asserted answers: transitive closure with a cycle, same-generation, an Andersen-style points-to
//! analysis, group-inheritance access control with deny rules and `count`, `min` plus a join on the min. (2) Rejection of
//! unstratifiable, unsafe and arity-inconsistent programs, plus 100 random rings that are rejected iff they contain a negative edge.
//! (3) 500 random programs (recursion, negation, `!=`, constants, aggregates) where NAIVE == SEMI-NAIVE == a brute-force oracle that
//! enumerates every variable assignment over the active domain, and magic-set answers equal the filtered full answers for the goals
//! that rewrite. (4) Work counters (tuples fetched through indexes) show the semi-naive saving, asserted on a long chain.
//! Read: Ceri, Gottlob, Tanca, "What you always wanted to know about Datalog (and never dared to ask)" (cited by title; no URL checked)
//!       Bancilhon, Maier, Sagiv, Ullman, "Magic Sets and Other Strange Ways to Implement Logic Programs" (cited by title)
//!       Soufflé documentation https://souffle-lang.github.io/
//!
//! TRY NEXT:
//!   - [S] Make `Step`s for negated atoms use an index too (`StepKind::Neg` currently probes the hash set); show the work counter does not change and say why.
//!   - [S] Predict which of `path(X,Z) :- path(X,Y), edge(Y,Z)` (linear) or `path(X,Z) :- path(X,Y), path(Y,Z)` (non-linear) needs fewer
//!     rounds on the 80-node chain, then measure `Stats::iterations` and `Stats::work` for both modes.
//!   - [M] Add arithmetic builtins (`Z = X + Y`, `X < Y`) as new `Lit` variants with the same safety rule, then write shortest-path with `min`.
//!   - [M] Port: write a Zanzibar-style authorisation check (`check(User, Object, Relation)` with userset rewrites) and query it with
//!     `magic_rewrite` for one user; compare derived-fact counts with and without the rewrite.
//!   - [A] Replace the Jacobi-style round (insert at end of round) with Gauss-Seidel (insert immediately) and prove by test it still equals the oracle in fewer rounds.
//!   - [A] Project: combine with `incremental_view_maintenance_dbsp` (its `IncTc` delta idea) and `query_rewrite_rules_magic_sets` to build an
//!     incremental Datalog that applies fact deletions without recomputing the stratum.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

// ---------------- values, terms, programs ----------------
const SYM: i64 = 1 << 40; // symbols are interned above this offset; plain integers stay themselves

#[derive(Default, Clone)]
struct Interner {
    ids: HashMap<String, i64>,
    names: Vec<String>,
}
impl Interner {
    fn sym(&mut self, s: &str) -> i64 {
        if let Some(&v) = self.ids.get(s) {
            return v;
        }
        let v = SYM + self.names.len() as i64;
        self.names.push(s.to_string());
        self.ids.insert(s.to_string(), v);
        v
    }
    fn show(&self, v: i64) -> String {
        if v >= SYM {
            self.names[(v - SYM) as usize].clone()
        } else {
            v.to_string()
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Term {
    Var(usize),
    Const(i64),
}
#[derive(Clone, Debug)]
struct Atom {
    pred: String,
    args: Vec<Term>,
}
#[derive(Clone, Debug)]
enum Lit {
    Pos(Atom),
    Neg(Atom),
    Neq(Term, Term),
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum AggKind {
    Count,
    Min,
    Max,
    Sum,
}
#[derive(Clone, Debug)]
struct Rule {
    head: Atom,
    agg: Option<(usize, AggKind)>, // head argument position holding the aggregate; that arg's Var is the aggregated variable
    body: Vec<Lit>,
    nvars: usize,
}
#[derive(Clone)]
struct Program {
    rules: Vec<Rule>,
    facts: Vec<(String, Vec<i64>)>,
    query: Option<Atom>,
    names: Interner,
}

// ---------------- lexer and parser ----------------
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Id(String),
    Int(i64),
    LP,
    RP,
    Comma,
    Dot,
    Imp,
    Bang,
    Neq,
    Query,
}
fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let cs: Vec<char> = src.chars().collect();
    let (mut i, mut out) = (0, vec![]);
    while i < cs.len() {
        let c = cs[i];
        match c {
            _ if c.is_whitespace() => i += 1,
            '%' => {
                while i < cs.len() && cs[i] != '\n' {
                    i += 1;
                }
            }
            '(' => (out.push(Tok::LP), i += 1).1,
            ')' => (out.push(Tok::RP), i += 1).1,
            ',' => (out.push(Tok::Comma), i += 1).1,
            '.' => (out.push(Tok::Dot), i += 1).1,
            ':' if cs.get(i + 1) == Some(&'-') => (out.push(Tok::Imp), i += 2).1,
            '?' if cs.get(i + 1) == Some(&'-') => (out.push(Tok::Query), i += 2).1,
            '!' if cs.get(i + 1) == Some(&'=') => (out.push(Tok::Neq), i += 2).1,
            '!' => (out.push(Tok::Bang), i += 1).1,
            _ if c.is_ascii_digit()
                || (c == '-' && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit())) =>
            {
                let st = i;
                i += 1;
                while i < cs.len() && cs[i].is_ascii_digit() {
                    i += 1;
                }
                let t: String = cs[st..i].iter().collect();
                out.push(Tok::Int(t.parse().map_err(|_| format!("bad integer {t}"))?));
            }
            _ if c.is_alphabetic() || c == '_' => {
                let st = i;
                while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                    i += 1;
                }
                out.push(Tok::Id(cs[st..i].iter().collect()));
            }
            _ => return Err(format!("unexpected character `{c}`")),
        }
    }
    Ok(out)
}
struct Parser<'a> {
    toks: Vec<Tok>,
    i: usize,
    vars: HashMap<String, usize>,
    names: &'a mut Interner,
}
impl Parser<'_> {
    fn peek(&self, k: usize) -> Option<&Tok> {
        self.toks.get(self.i + k)
    }
    fn eat(&mut self, t: Tok) -> Result<(), String> {
        if self.peek(0) == Some(&t) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected {t:?}, found {:?}", self.peek(0)))
        }
    }
    fn term(&mut self) -> Result<Term, String> {
        match self.toks.get(self.i).cloned() {
            Some(Tok::Int(n)) => (self.i += 1, Ok(Term::Const(n))).1,
            Some(Tok::Id(s)) => {
                self.i += 1;
                if s.chars().next().is_some_and(|c| c.is_uppercase()) {
                    let n = self.vars.len();
                    Ok(Term::Var(*self.vars.entry(s).or_insert(n)))
                } else {
                    Ok(Term::Const(self.names.sym(&s)))
                }
            }
            t => Err(format!("expected a term, found {t:?}")),
        }
    }
    fn atom(&mut self) -> Result<Atom, String> {
        let Some(Tok::Id(pred)) = self.toks.get(self.i).cloned() else {
            return Err(format!("expected predicate, found {:?}", self.peek(0)));
        };
        self.i += 1;
        self.eat(Tok::LP)?;
        let mut args = vec![];
        if self.peek(0) != Some(&Tok::RP) {
            loop {
                args.push(self.term()?);
                if self.peek(0) == Some(&Tok::Comma) {
                    self.i += 1;
                } else {
                    break;
                }
            }
        }
        self.eat(Tok::RP)?;
        Ok(Atom { pred, args })
    }
    /// Head atom, allowing one aggregate argument like `count(Y)`.
    fn head(&mut self) -> Result<(Atom, Option<(usize, AggKind)>), String> {
        let Some(Tok::Id(pred)) = self.toks.get(self.i).cloned() else {
            return Err("expected head predicate".into());
        };
        self.i += 1;
        self.eat(Tok::LP)?;
        let (mut args, mut agg) = (vec![], None);
        loop {
            let kind = match (self.peek(0), self.peek(1)) {
                (Some(Tok::Id(s)), Some(Tok::LP)) => match s.as_str() {
                    "count" => Some(AggKind::Count),
                    "min" => Some(AggKind::Min),
                    "max" => Some(AggKind::Max),
                    "sum" => Some(AggKind::Sum),
                    _ => return Err(format!("unknown aggregate `{s}`")),
                },
                _ => None,
            };
            if let Some(k) = kind {
                if agg.is_some() {
                    return Err("at most one aggregate per rule head".into());
                }
                self.i += 2;
                let t = self.term()?;
                if !matches!(t, Term::Var(_)) {
                    return Err("aggregate argument must be a variable".into());
                }
                self.eat(Tok::RP)?;
                agg = Some((args.len(), k));
                args.push(t);
            } else {
                args.push(self.term()?);
            }
            if self.peek(0) == Some(&Tok::Comma) {
                self.i += 1;
            } else {
                break;
            }
        }
        self.eat(Tok::RP)?;
        Ok((Atom { pred, args }, agg))
    }
    fn lit(&mut self) -> Result<Lit, String> {
        let is_not = matches!((self.peek(0), self.peek(1)), (Some(Tok::Id(s)), Some(Tok::Id(_))) if s == "not");
        if is_not || self.peek(0) == Some(&Tok::Bang) {
            self.i += 1;
            return Ok(Lit::Neg(self.atom()?));
        }
        if matches!(
            (self.peek(0), self.peek(1)),
            (Some(Tok::Id(_)), Some(Tok::LP))
        ) {
            return Ok(Lit::Pos(self.atom()?));
        }
        let a = self.term()?;
        self.eat(Tok::Neq)?;
        Ok(Lit::Neq(a, self.term()?))
    }
}
fn parse(src: &str) -> Result<Program, String> {
    let toks = lex(src)?;
    let mut names = Interner::default();
    let mut p = Parser {
        toks,
        i: 0,
        vars: HashMap::new(),
        names: &mut names,
    };
    let (mut rules, mut facts, mut query) = (vec![], vec![], None);
    let mut arities: HashMap<String, usize> = HashMap::new();
    let note = |a: &Atom, ar: &mut HashMap<String, usize>| -> Result<(), String> {
        match ar.insert(a.pred.clone(), a.args.len()) {
            Some(old) if old != a.args.len() => Err(format!(
                "predicate `{}` used with arities {old} and {}",
                a.pred,
                a.args.len()
            )),
            _ => Ok(()),
        }
    };
    while p.i < p.toks.len() {
        p.vars.clear();
        if p.peek(0) == Some(&Tok::Query) {
            p.i += 1;
            let a = p.atom()?;
            p.eat(Tok::Dot)?;
            note(&a, &mut arities)?;
            query = Some(a);
            continue;
        }
        let (head, agg) = p.head()?;
        note(&head, &mut arities)?;
        if p.peek(0) == Some(&Tok::Dot) {
            p.i += 1;
            let mut vals = vec![];
            for t in &head.args {
                match t {
                    Term::Const(c) if agg.is_none() => vals.push(*c),
                    _ => return Err(format!("fact `{}` must be ground", head.pred)),
                }
            }
            facts.push((head.pred.clone(), vals));
            continue;
        }
        p.eat(Tok::Imp)?;
        let mut body = vec![];
        loop {
            let l = p.lit()?;
            if let Lit::Pos(a) | Lit::Neg(a) = &l {
                note(a, &mut arities)?;
            }
            body.push(l);
            if p.peek(0) == Some(&Tok::Comma) {
                p.i += 1;
            } else {
                break;
            }
        }
        p.eat(Tok::Dot)?;
        let rule = Rule {
            head,
            agg,
            body,
            nvars: p.vars.len(),
        };
        check_safe(&rule)?;
        rules.push(rule);
    }
    Ok(Program {
        rules,
        facts,
        query,
        names,
    })
}
fn vars_of(ts: &[Term]) -> impl Iterator<Item = usize> + '_ {
    ts.iter()
        .filter_map(|t| if let Term::Var(v) = t { Some(*v) } else { None })
}
fn check_safe(r: &Rule) -> Result<(), String> {
    let pos: HashSet<usize> = r
        .body
        .iter()
        .filter_map(|l| if let Lit::Pos(a) = l { Some(a) } else { None })
        .flat_map(|a| vars_of(&a.args))
        .collect();
    let bad = |what: &str, v: usize| {
        Err(format!(
            "unsafe rule for `{}`: variable #{v} in {what} is not bound by a positive body atom",
            r.head.pred
        ))
    };
    for v in vars_of(&r.head.args) {
        if !pos.contains(&v) {
            return bad("the head", v);
        }
    }
    for l in &r.body {
        match l {
            Lit::Neg(a) => {
                for v in vars_of(&a.args) {
                    if !pos.contains(&v) {
                        return bad("a negated atom", v);
                    }
                }
            }
            Lit::Neq(x, y) => {
                for v in vars_of(&[*x, *y]) {
                    if !pos.contains(&v) {
                        return bad("a `!=` test", v);
                    }
                }
            }
            Lit::Pos(_) => {}
        }
    }
    Ok(())
}

// ---------------- stratification ----------------
fn strict_edges(r: &Rule) -> Vec<(String, bool)> {
    r.body
        .iter()
        .filter_map(|l| match l {
            Lit::Pos(a) => Some((a.pred.clone(), r.agg.is_some())),
            Lit::Neg(a) => Some((a.pred.clone(), true)),
            Lit::Neq(..) => None,
        })
        .collect()
}
fn stratify(p: &Program) -> Result<HashMap<String, usize>, String> {
    let mut preds: Vec<String> = vec![];
    let id = |s: &str, preds: &mut Vec<String>| -> usize {
        preds.iter().position(|x| x == s).unwrap_or_else(|| {
            preds.push(s.to_string());
            preds.len() - 1
        })
    };
    let mut edges = vec![]; // (body, head, strict)
    for r in &p.rules {
        let h = id(&r.head.pred, &mut preds);
        for (b, strict) in strict_edges(r) {
            let b = id(&b, &mut preds);
            edges.push((b, h, strict));
        }
    }
    for (f, _) in &p.facts {
        id(f, &mut preds);
    }
    let n = preds.len();
    let mut reach = vec![vec![false; n]; n]; // reach[x][y]: x depends (transitively) on y
    for &(b, h, _) in &edges {
        reach[h][b] = true;
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
    for &(b, h, strict) in &edges {
        if strict && (b == h || reach[b][h]) {
            return Err(format!(
                "not stratifiable: `{}` depends on `{}` through negation/aggregation inside a recursive cycle",
                preds[h], preds[b]
            ));
        }
    }
    let mut s = vec![0usize; n];
    for _ in 0..=n + 1 {
        let mut changed = false;
        for &(b, h, strict) in &edges {
            let want = s[b] + strict as usize;
            if s[h] < want {
                s[h] = want;
                changed = true;
            }
        }
        if !changed {
            return Ok(preds.into_iter().zip(s).collect());
        }
    }
    Err("stratum relaxation did not converge".into())
}

// ---------------- relations with append-only storage and hash indexes ----------------
#[derive(Default, Clone)]
struct Rel {
    tuples: Vec<Vec<i64>>,
    set: HashSet<Vec<i64>>,
    idx: HashMap<Vec<usize>, HashMap<Vec<i64>, Vec<u32>>>, // key columns -> key -> ascending positions
}
impl Rel {
    fn insert(&mut self, t: Vec<i64>) -> bool {
        if !self.set.insert(t.clone()) {
            return false;
        }
        let pos = self.tuples.len() as u32;
        for (cols, m) in self.idx.iter_mut() {
            m.entry(cols.iter().map(|&c| t[c]).collect())
                .or_default()
                .push(pos);
        }
        self.tuples.push(t);
        true
    }
    fn ensure_index(&mut self, cols: &[usize]) {
        if self.idx.contains_key(cols) {
            return;
        }
        let mut m: HashMap<Vec<i64>, Vec<u32>> = HashMap::new();
        for (p, t) in self.tuples.iter().enumerate() {
            m.entry(cols.iter().map(|&c| t[c]).collect())
                .or_default()
                .push(p as u32);
        }
        self.idx.insert(cols.to_vec(), m);
    }
    /// Tuples at positions [lo,hi) whose `cols` equal `key`.
    fn candidates(&self, cols: &[usize], key: &[i64], lo: usize, hi: usize) -> Vec<&Vec<i64>> {
        if lo >= hi {
            return vec![];
        }
        if cols.is_empty() {
            return self.tuples[lo..hi].iter().collect();
        }
        let Some(pos) = self.idx[cols].get(key) else {
            return vec![];
        };
        let a = pos.partition_point(|&p| (p as usize) < lo);
        let b = pos.partition_point(|&p| (p as usize) < hi);
        pos[a..b]
            .iter()
            .map(|&p| &self.tuples[p as usize])
            .collect()
    }
}

// ---------------- join plans ----------------
#[derive(Clone, Copy, PartialEq, Debug)]
enum Src {
    Full,
    Delta,
    Old,
}
#[derive(Clone, Debug)]
enum StepKind {
    Pos(Src, Vec<usize>), // source range and the BOUND columns (these become the index key)
    Neg,
    Neq,
}
#[derive(Clone, Debug)]
struct Step {
    lit: usize,
    kind: StepKind,
}
fn lit_vars(l: &Lit) -> Vec<usize> {
    match l {
        Lit::Pos(a) | Lit::Neg(a) => vars_of(&a.args).collect(),
        Lit::Neq(x, y) => vars_of(&[*x, *y]).collect(),
    }
}
fn make_plan(rule: &Rule, delta_lit: Option<usize>) -> Vec<Step> {
    let src_of = |j: usize| match delta_lit {
        None => Src::Full,
        Some(d) if j == d => Src::Delta,
        Some(d) if j < d => Src::Old,
        Some(_) => Src::Full,
    };
    let mut bound: HashSet<usize> = HashSet::new();
    let mut done = vec![false; rule.body.len()];
    let mut steps = vec![];
    loop {
        // filters first: negation and != fire as soon as all their variables are bound
        for j in 0..rule.body.len() {
            if !done[j]
                && !matches!(rule.body[j], Lit::Pos(_))
                && lit_vars(&rule.body[j]).iter().all(|v| bound.contains(v))
            {
                done[j] = true;
                steps.push(Step {
                    lit: j,
                    kind: if matches!(rule.body[j], Lit::Neg(_)) {
                        StepKind::Neg
                    } else {
                        StepKind::Neq
                    },
                });
            }
        }
        let score = |j: usize| -> usize {
            let Lit::Pos(a) = &rule.body[j] else { return 0 };
            a.args
                .iter()
                .filter(|t| {
                    matches!(t, Term::Const(_)) || matches!(t, Term::Var(v) if bound.contains(v))
                })
                .count()
        };
        let next = match delta_lit {
            Some(d) if !done[d] => Some(d),
            _ => (0..rule.body.len())
                .filter(|&j| !done[j] && matches!(rule.body[j], Lit::Pos(_)))
                .max_by_key(|&j| (score(j), usize::MAX - j)),
        };
        let Some(j) = next else { break };
        let Lit::Pos(a) = &rule.body[j] else {
            unreachable!()
        };
        let cols: Vec<usize> = a
            .args
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                matches!(t, Term::Const(_)) || matches!(t, Term::Var(v) if bound.contains(v))
            })
            .map(|(c, _)| c)
            .collect();
        bound.extend(vars_of(&a.args));
        done[j] = true;
        steps.push(Step {
            lit: j,
            kind: StepKind::Pos(src_of(j), cols),
        });
    }
    assert!(
        done.iter().all(|&d| d),
        "unsafe rule slipped through: a filter never became ground"
    );
    steps
}

// ---------------- the evaluator ----------------
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Mode {
    Naive,
    SemiNaive,
}
#[derive(Default, Clone, Debug)]
struct Stats {
    iterations: usize, // rounds executed over all strata (including each stratum's final fruitless round)
    work: u64,         // tuples fetched through scans/indexes
    derived: usize,    // distinct facts derived by rules
}
struct Out {
    rels: BTreeMap<String, BTreeSet<Vec<i64>>>,
    stats: Stats,
    indexes: Vec<(String, Vec<usize>)>,
}
struct Db {
    rels: HashMap<String, Rel>,
    delta: HashMap<String, (usize, usize)>,
}
impl Db {
    fn range(&self, pred: &str, src: Src) -> (usize, usize) {
        let len = self.rels[pred].tuples.len();
        let (dl, dh) = self.delta.get(pred).copied().unwrap_or((len, len));
        match src {
            Src::Full => (0, len),
            Src::Delta => (dl, dh),
            Src::Old => (0, dl),
        }
    }
    fn val(t: &Term, bind: &[Option<i64>]) -> i64 {
        match t {
            Term::Const(c) => *c,
            Term::Var(v) => bind[*v].expect("plan must bind variables before use"),
        }
    }
    fn run(
        &self,
        rule: &Rule,
        plan: &[Step],
        si: usize,
        bind: &mut Vec<Option<i64>>,
        out: &mut Vec<Vec<i64>>,
        work: &mut u64,
    ) {
        if si == plan.len() {
            out.push(rule.head.args.iter().map(|t| Self::val(t, bind)).collect());
            return;
        }
        let step = &plan[si];
        match (&step.kind, &rule.body[step.lit]) {
            (StepKind::Neq, Lit::Neq(a, b)) => {
                if Self::val(a, bind) != Self::val(b, bind) {
                    self.run(rule, plan, si + 1, bind, out, work);
                }
            }
            (StepKind::Neg, Lit::Neg(a)) => {
                let t: Vec<i64> = a.args.iter().map(|x| Self::val(x, bind)).collect();
                if !self.rels[&a.pred].set.contains(&t) {
                    self.run(rule, plan, si + 1, bind, out, work);
                }
            }
            (StepKind::Pos(src, cols), Lit::Pos(a)) => {
                let (lo, hi) = self.range(&a.pred, *src);
                let key: Vec<i64> = cols.iter().map(|&c| Self::val(&a.args[c], bind)).collect();
                for t in self.rels[&a.pred].candidates(cols, &key, lo, hi) {
                    *work += 1;
                    let (mut newly, mut ok) = (vec![], true);
                    for (c, arg) in a.args.iter().enumerate() {
                        match arg {
                            Term::Const(x) if t[c] != *x => ok = false,
                            Term::Var(v) => match bind[*v] {
                                Some(x) if x != t[c] => ok = false,
                                Some(_) => {}
                                None => {
                                    bind[*v] = Some(t[c]);
                                    newly.push(*v);
                                }
                            },
                            _ => {}
                        }
                        if !ok {
                            break;
                        }
                    }
                    if ok {
                        self.run(rule, plan, si + 1, bind, out, work);
                    }
                    for v in newly {
                        bind[v] = None;
                    }
                }
            }
            _ => unreachable!("plan step does not match literal"),
        }
    }
}
fn aggregate(rule: &Rule, rows: Vec<Vec<i64>>) -> Vec<Vec<i64>> {
    let (pos, kind) = rule.agg.unwrap();
    let mut groups: BTreeMap<Vec<i64>, BTreeSet<i64>> = BTreeMap::new();
    for mut r in rows {
        let v = r.remove(pos);
        groups.entry(r).or_default().insert(v); // DISTINCT values per group
    }
    groups
        .into_iter()
        .map(|(mut key, vals)| {
            let a = match kind {
                AggKind::Count => vals.len() as i64,
                AggKind::Min => *vals.iter().next().unwrap(),
                AggKind::Max => *vals.iter().next_back().unwrap(),
                AggKind::Sum => vals.iter().sum(),
            };
            key.insert(pos, a);
            key
        })
        .collect()
}
fn evaluate(prog: &Program, strata: &HashMap<String, usize>, mode: Mode) -> Out {
    let mut db = Db {
        rels: HashMap::new(),
        delta: HashMap::new(),
    };
    let note = |p: &str, db: &mut Db| {
        db.rels.entry(p.to_string()).or_default();
    };
    for r in &prog.rules {
        note(&r.head.pred, &mut db);
        for l in &r.body {
            if let Lit::Pos(a) | Lit::Neg(a) = l {
                note(&a.pred, &mut db);
            }
        }
    }
    for (p, t) in &prog.facts {
        note(p, &mut db);
        db.rels.get_mut(p).unwrap().insert(t.clone());
    }
    let st = |p: &str| strata.get(p).copied().unwrap_or(0);
    // plans for every rule, and for every recursive positive atom when semi-naive; they decide the indexes
    let mut plans: HashMap<(usize, Option<usize>), Vec<Step>> = HashMap::new();
    for (ri, r) in prog.rules.iter().enumerate() {
        plans.insert((ri, None), make_plan(r, None));
        if r.agg.is_none() {
            for (j, l) in r.body.iter().enumerate() {
                if matches!(l, Lit::Pos(a) if st(&a.pred) == st(&r.head.pred)) {
                    plans.insert((ri, Some(j)), make_plan(r, Some(j)));
                }
            }
        }
    }
    let mut indexes = BTreeSet::new();
    for ((ri, _), plan) in &plans {
        for s in plan {
            if let (StepKind::Pos(_, cols), Lit::Pos(a)) = (&s.kind, &prog.rules[*ri].body[s.lit]) {
                if !cols.is_empty() {
                    db.rels.get_mut(&a.pred).unwrap().ensure_index(cols);
                    indexes.insert((a.pred.clone(), cols.clone()));
                }
            }
        }
    }
    let mut stats = Stats::default();
    let max_s = prog
        .rules
        .iter()
        .map(|r| st(&r.head.pred))
        .max()
        .unwrap_or(0);
    for s in 0..=max_s {
        let rules: Vec<usize> = (0..prog.rules.len())
            .filter(|&i| st(&prog.rules[i].head.pred) == s)
            .collect();
        let stratum_preds: HashSet<String> = rules
            .iter()
            .map(|&i| prog.rules[i].head.pred.clone())
            .collect();
        let mut round = 0;
        loop {
            round += 1;
            stats.iterations += 1;
            let mut buf: Vec<(String, Vec<i64>)> = vec![];
            for &ri in &rules {
                let r = &prog.rules[ri];
                let variants: Vec<Option<usize>> =
                    if round == 1 || mode == Mode::Naive || r.agg.is_some() {
                        if round > 1 && mode == Mode::SemiNaive {
                            continue; // aggregates never re-fire: their inputs are in lower strata
                        }
                        vec![None]
                    } else {
                        r.body
                        .iter()
                        .enumerate()
                        .filter(
                            |(_, l)| matches!(l, Lit::Pos(a) if stratum_preds.contains(&a.pred)),
                        )
                        .map(|(j, _)| Some(j))
                        .collect()
                    };
                for v in variants {
                    let mut rows = vec![];
                    let mut bind = vec![None; r.nvars];
                    db.run(
                        r,
                        &plans[&(ri, v)],
                        0,
                        &mut bind,
                        &mut rows,
                        &mut stats.work,
                    );
                    let rows = if r.agg.is_some() {
                        aggregate(r, rows)
                    } else {
                        rows
                    };
                    buf.extend(rows.into_iter().map(|t| (r.head.pred.clone(), t)));
                }
            }
            let before: HashMap<String, usize> = stratum_preds
                .iter()
                .map(|p| (p.clone(), db.rels[p].tuples.len()))
                .collect();
            let mut new = 0;
            for (p, t) in buf {
                if db.rels.get_mut(&p).unwrap().insert(t) {
                    new += 1;
                }
            }
            stats.derived += new;
            for p in &stratum_preds {
                db.delta
                    .insert(p.clone(), (before[p], db.rels[p].tuples.len()));
            }
            if new == 0 {
                break;
            }
        }
        for p in &stratum_preds {
            db.delta.remove(p);
        }
    }
    Out {
        rels: db
            .rels
            .iter()
            .map(|(k, v)| (k.clone(), v.tuples.iter().cloned().collect()))
            .collect(),
        stats,
        indexes: indexes.into_iter().collect(),
    }
}
fn run_program(src: &str, mode: Mode) -> Result<(Program, Out), String> {
    let prog = parse(src)?;
    let strata = stratify(&prog)?;
    let out = evaluate(&prog, &strata, mode);
    Ok((prog, out))
}

// ---------------- magic sets (lite) ----------------
fn adornment(args: &[Term], bound: &HashSet<usize>) -> String {
    args.iter()
        .map(|t| {
            if matches!(t, Term::Const(_)) || matches!(t, Term::Var(v) if bound.contains(v)) {
                'b'
            } else {
                'f'
            }
        })
        .collect()
}
fn pick(args: &[Term], ad: &str) -> Vec<Term> {
    args.iter()
        .zip(ad.chars())
        .filter(|(_, c)| *c == 'b')
        .map(|(t, _)| *t)
        .collect()
}
fn magic_rewrite(p: &Program) -> Result<(Program, String, String), String> {
    let q = p.query.clone().ok_or("no query")?;
    let idb: HashSet<String> = p.rules.iter().map(|r| r.head.pred.clone()).collect();
    if !idb.contains(&q.pred) {
        return Err("query predicate is extensional".into());
    }
    let mut rules_for: HashMap<String, Vec<Rule>> = HashMap::new();
    for r in &p.rules {
        rules_for
            .entry(r.head.pred.clone())
            .or_default()
            .push(r.clone());
    }
    for (pred, t) in &p.facts {
        if idb.contains(pred) {
            let head = Atom {
                pred: pred.clone(),
                args: t.iter().map(|&c| Term::Const(c)).collect(),
            };
            rules_for.get_mut(pred).unwrap().push(Rule {
                head,
                agg: None,
                body: vec![],
                nvars: 0,
            });
        }
    }
    let q_ad = adornment(&q.args, &HashSet::new());
    let mut out_rules = vec![];
    let mut todo = vec![(q.pred.clone(), q_ad.clone())];
    let mut seen: HashSet<(String, String)> = todo.iter().cloned().collect();
    while let Some((pr, ad)) = todo.pop() {
        for r in &rules_for[&pr] {
            if r.agg.is_some() || r.body.iter().any(|l| matches!(l, Lit::Neg(_))) {
                return Err("magic sets (lite) only handles positive programs".into());
            }
            let mut bound: HashSet<usize> = vars_of(&pick(&r.head.args, &ad)).collect();
            let magic_p = Atom {
                pred: format!("m_{pr}__{ad}"),
                args: pick(&r.head.args, &ad),
            };
            let mut body = vec![Lit::Pos(magic_p)];
            for l in &r.body {
                match l {
                    Lit::Pos(a) if idb.contains(&a.pred) => {
                        let beta = adornment(&a.args, &bound);
                        out_rules.push(Rule {
                            head: Atom {
                                pred: format!("m_{}__{beta}", a.pred),
                                args: pick(&a.args, &beta),
                            },
                            agg: None,
                            body: body.clone(), // sideways information passing: everything to the left
                            nvars: r.nvars,
                        });
                        body.push(Lit::Pos(Atom {
                            pred: format!("{}__{beta}", a.pred),
                            args: a.args.clone(),
                        }));
                        if seen.insert((a.pred.clone(), beta.clone())) {
                            todo.push((a.pred.clone(), beta));
                        }
                        bound.extend(vars_of(&a.args));
                    }
                    Lit::Pos(a) => {
                        body.push(l.clone());
                        bound.extend(vars_of(&a.args));
                    }
                    _ => body.push(l.clone()),
                }
            }
            out_rules.push(Rule {
                head: Atom {
                    pred: format!("{pr}__{ad}"),
                    args: r.head.args.clone(),
                },
                agg: None,
                body,
                nvars: r.nvars,
            });
        }
    }
    let mut facts: Vec<_> = p
        .facts
        .iter()
        .filter(|(pr, _)| !idb.contains(pr))
        .cloned()
        .collect();
    let seed: Vec<i64> = pick(&q.args, &q_ad)
        .iter()
        .map(|t| {
            if let Term::Const(c) = t {
                *c
            } else {
                unreachable!()
            }
        })
        .collect();
    facts.push((format!("m_{}__{q_ad}", q.pred), seed));
    let prog = Program {
        rules: out_rules,
        facts,
        query: None,
        names: p.names.clone(),
    };
    Ok((prog, format!("{}__{q_ad}", q.pred), q_ad))
}
fn matches_goal(t: &[i64], q: &Atom) -> bool {
    q.args
        .iter()
        .zip(t)
        .all(|(a, v)| !matches!(a, Term::Const(c) if c != v))
}

// ---------------- oracle: enumerate every assignment over the active domain ----------------
fn oracle(prog: &Program, order: &[String]) -> BTreeMap<String, BTreeSet<Vec<i64>>> {
    let mut rels: BTreeMap<String, BTreeSet<Vec<i64>>> = BTreeMap::new();
    let mut domain: BTreeSet<i64> = BTreeSet::new();
    for (p, t) in &prog.facts {
        rels.entry(p.clone()).or_default().insert(t.clone());
        domain.extend(t);
    }
    for r in &prog.rules {
        rels.entry(r.head.pred.clone()).or_default();
        let mut consts: Vec<Term> = r.head.args.clone();
        for l in &r.body {
            match l {
                Lit::Pos(a) | Lit::Neg(a) => consts.extend(&a.args),
                Lit::Neq(x, y) => consts.extend([*x, *y]),
            }
        }
        domain.extend(consts.iter().filter_map(|t| {
            if let Term::Const(c) = t {
                Some(*c)
            } else {
                None
            }
        }));
    }
    let dom: Vec<i64> = domain.into_iter().collect();
    let ev = |t: &Term, a: &[i64]| match t {
        Term::Const(c) => *c,
        Term::Var(v) => a[*v],
    };
    for pred in order {
        loop {
            let mut changed = false;
            for r in prog.rules.iter().filter(|r| &r.head.pred == pred) {
                let mut rows = vec![];
                let mut a = vec![0usize; r.nvars];
                'assign: loop {
                    let vals: Vec<i64> = a.iter().map(|&i| dom[i]).collect();
                    let ok = r.body.iter().all(|l| match l {
                        Lit::Pos(at) => rels.get(&at.pred).is_some_and(|s| {
                            s.contains(&at.args.iter().map(|t| ev(t, &vals)).collect::<Vec<_>>())
                        }),
                        Lit::Neg(at) => !rels.get(&at.pred).is_some_and(|s| {
                            s.contains(&at.args.iter().map(|t| ev(t, &vals)).collect::<Vec<_>>())
                        }),
                        Lit::Neq(x, y) => ev(x, &vals) != ev(y, &vals),
                    });
                    if ok {
                        rows.push(r.head.args.iter().map(|t| ev(t, &vals)).collect::<Vec<_>>());
                    }
                    let mut k = 0;
                    loop {
                        if k == a.len() {
                            break 'assign;
                        }
                        a[k] += 1;
                        if a[k] < dom.len() {
                            break;
                        }
                        a[k] = 0;
                        k += 1;
                    }
                }
                let rows = if r.agg.is_some() {
                    aggregate(r, rows)
                } else {
                    rows
                };
                for t in rows {
                    changed |= rels.get_mut(pred).unwrap().insert(t);
                }
            }
            if !changed {
                break;
            }
        }
    }
    rels
}

// ---------------- random programs ----------------
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
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}
const DOM: u64 = 5;
fn gen_program(r: &mut Rng) -> (String, Vec<String>) {
    let mut s = String::new();
    for (e, arity) in [("e0", 2), ("e1", 2), ("e2", 1)] {
        for _ in 0..3 + r.below(8) {
            let args: Vec<String> = (0..arity).map(|_| r.below(DOM).to_string()).collect();
            s += &format!("{e}({}).\n", args.join(","));
        }
    }
    let nidb = 2 + r.below(3) as usize;
    let var = |r: &mut Rng| ["X", "Y", "Z"][r.below(3) as usize].to_string();
    for i in 0..nidb {
        if i == nidb - 1 && r.chance(30) {
            let src = if i > 0 && r.chance(50) {
                format!("q{}", r.below(i as u64))
            } else {
                "e0".to_string()
            };
            let kind = ["count", "min", "max", "sum"][r.below(4) as usize];
            s += &format!("q{i}(X, {kind}(Y)) :- {src}(X,Y).\n");
            continue;
        }
        for _ in 0..1 + r.below(2) {
            let mut body: Vec<String> = vec![];
            let mut pvars: Vec<String> = vec![];
            let mut used_self = false;
            for _ in 0..1 + r.below(3) {
                let mut cands: Vec<(String, usize)> =
                    vec![("e0".into(), 2), ("e1".into(), 2), ("e2".into(), 1)];
                cands.extend((0..i).map(|j| (format!("q{j}"), 2)));
                if !used_self && r.chance(45) {
                    cands = vec![(format!("q{i}"), 2)];
                    used_self = true;
                }
                let (p, ar) = cands[r.below(cands.len() as u64) as usize].clone();
                let args: Vec<String> = (0..ar)
                    .map(|_| {
                        if r.chance(12) {
                            r.below(DOM).to_string()
                        } else {
                            let v = var(r);
                            pvars.push(v.clone());
                            v
                        }
                    })
                    .collect();
                body.push(format!("{p}({})", args.join(",")));
            }
            let head: Vec<String> = (0..2).map(|_| var(r)).collect();
            for h in &head {
                if !pvars.contains(h) {
                    body.push(format!("e2({h})"));
                    pvars.push(h.clone());
                }
            }
            if r.chance(35) {
                let mut cands: Vec<(String, usize)> = vec![("e0".into(), 2), ("e2".into(), 1)];
                cands.extend((0..i).map(|j| (format!("q{j}"), 2)));
                let (p, ar) = cands[r.below(cands.len() as u64) as usize].clone();
                let args: Vec<String> = (0..ar)
                    .map(|_| pvars[r.below(pvars.len() as u64) as usize].clone())
                    .collect();
                body.push(format!("not {p}({})", args.join(",")));
            }
            if r.chance(30) {
                let (a, b) = (
                    pvars[r.below(pvars.len() as u64) as usize].clone(),
                    pvars[r.below(pvars.len() as u64) as usize].clone(),
                );
                body.push(format!("{a} != {b}"));
            }
            s += &format!("q{i}({}) :- {}.\n", head.join(","), body.join(", "));
        }
    }
    (s, (0..nidb).map(|i| format!("q{i}")).collect())
}

// ---------------- checks ----------------
fn has(p: &Program, out: &Out, pred: &str, args: &[&str]) -> bool {
    let t: Vec<i64> = args
        .iter()
        .map(|a| {
            a.parse::<i64>().unwrap_or_else(|_| {
                *p.names
                    .ids
                    .get(*a)
                    .unwrap_or_else(|| panic!("unknown symbol {a}"))
            })
        })
        .collect();
    out.rels.get(pred).is_some_and(|s| s.contains(&t))
}
fn size(out: &Out, pred: &str) -> usize {
    out.rels.get(pred).map_or(0, |s| s.len())
}
const TC: &str = "edge(1,2). edge(2,3). edge(3,4). edge(4,2). edge(5,6).
path(X,Y) :- edge(X,Y).
path(X,Z) :- path(X,Y), edge(Y,Z).";
const SG: &str = "par(bob,alice). par(carol,alice). par(dave,bob). par(erin,carol). par(frank,dave). par(gina,erin).
person(X) :- par(X,Y).  person(Y) :- par(X,Y).
sg(X,X) :- person(X).
sg(X,Y) :- par(X,XP), sg(XP,YP), par(Y,YP).";
const PTS: &str =
    "new(a,o1). new(b,o2). new(c,o3). assign(d,a). assign(e,d). store(b,f,a). load(g,b,f).
pts(V,O) :- new(V,O).
pts(V,O) :- assign(V,W), pts(W,O).
hpts(O1,F,O2) :- store(V,F,W), pts(V,O1), pts(W,O2).
pts(V,O2) :- load(V,W,F), pts(W,O1), hpts(O1,F,O2).";
const ACL: &str = "member(alice,eng). member(bob,ops). member(carol,eng).
sub(eng,staff). sub(ops,staff). sub(staff,all).
grant(all,wiki_read). grant(eng,repo_write). grant(ops,deploy). grant(staff,vpn).
deny(carol,repo_write).
in_group(U,G) :- member(U,G).
in_group(U,H) :- in_group(U,G), sub(G,H).
can(U,R) :- in_group(U,G), grant(G,R).
allowed(U,R) :- can(U,R), not deny(U,R).
n_allowed(U, count(R)) :- allowed(U,R).";
const PRICES: &str =
    "price(widget,acme,30). price(widget,bolt,25). price(gadget,acme,70). price(gadget,bolt,70).
cheapest(I, min(P)) :- price(I,S,P).
nsup(I, count(S)) :- price(I,S,P).
best(I,S) :- cheapest(I,P), price(I,S,P).";

fn check_examples() {
    let (p, o) = run_program(TC, Mode::SemiNaive).unwrap();
    assert_eq!(size(&o, "path"), 13);
    assert!(
        has(&p, &o, "path", &["3", "3"])
            && !has(&p, &o, "path", &["1", "1"])
            && !has(&p, &o, "path", &["5", "1"])
    );
    println!(
        "transitive closure: {} path facts in {} rounds, indexes {:?}",
        size(&o, "path"),
        o.stats.iterations,
        o.indexes
    );

    let (p, o) = run_program(SG, Mode::SemiNaive).unwrap();
    assert!(
        has(&p, &o, "sg", &["frank", "gina"])
            && has(&p, &o, "sg", &["bob", "carol"])
            && !has(&p, &o, "sg", &["frank", "erin"])
    );
    println!(
        "same generation: sg has {} facts; (frank,gina) cousins: yes; (frank,erin): no",
        size(&o, "sg")
    );

    let (p, o) = run_program(PTS, Mode::SemiNaive).unwrap();
    assert!(
        has(&p, &o, "pts", &["e", "o1"])
            && has(&p, &o, "pts", &["g", "o1"])
            && !has(&p, &o, "pts", &["g", "o2"])
    );
    println!(
        "points-to: pts(g,o1) via the heap field f; e aliases a through d: {} pts facts",
        size(&o, "pts")
    );

    let (p, o) = run_program(ACL, Mode::SemiNaive).unwrap();
    assert!(
        has(&p, &o, "n_allowed", &["alice", "3"])
            && has(&p, &o, "n_allowed", &["carol", "2"])
            && has(&p, &o, "n_allowed", &["bob", "3"])
    );
    assert!(
        !has(&p, &o, "allowed", &["carol", "repo_write"])
            && has(&p, &o, "allowed", &["carol", "vpn"])
    );
    println!(
        "access control: carol inherits 3 permissions via groups, the deny removes one => count 2"
    );

    let (p, o) = run_program(PRICES, Mode::SemiNaive).unwrap();
    assert!(has(&p, &o, "cheapest", &["widget", "25"]) && has(&p, &o, "nsup", &["gadget", "2"]));
    assert!(
        has(&p, &o, "best", &["widget", "bolt"]) && size(&o, "best") == 3,
        "ties keep both suppliers"
    );
    println!(
        "aggregates: cheapest widget costs 25 from bolt; best/2 joins on the min (strata chain)"
    );
}

fn check_rejections() {
    let bad = [
        ("win(X) :- move(X,Y), not win(Y).", "not stratifiable"),
        ("p(X) :- q(X), not r(X). r(X) :- p(X).", "not stratifiable"),
        ("c(X, count(Y)) :- e(X,Y), c(Y,Z).", "not stratifiable"),
        ("p(X) :- not q(X).", "unsafe"),
        ("p(X,Y) :- q(X).", "unsafe"),
        ("p(X) :- q(X), X != Y.", "unsafe"),
        ("p(X). ", "ground"),
        ("p(1). p(1,2).", "arities"),
        ("p(X) :- q(X)", "expected"),
    ];
    for (src, why) in bad {
        let e = run_program(src, Mode::SemiNaive)
            .err()
            .unwrap_or_else(|| panic!("accepted bad program {src}"));
        assert!(
            e.contains(why),
            "`{src}` rejected for the wrong reason: {e}"
        );
    }
    let mut r = Rng(2024);
    for _ in 0..100 {
        let k = 2 + r.below(4) as usize;
        let neg: Vec<bool> = (0..k).map(|_| r.chance(35)).collect();
        let mut s = String::from("e2(1). e2(2).\n");
        for i in 0..k {
            s += &format!(
                "p{i}(X) :- e2(X), {}p{}(X).\n",
                if neg[i] { "not " } else { "" },
                (i + 1) % k
            );
        }
        let res = run_program(&s, Mode::SemiNaive);
        assert_eq!(
            res.is_err(),
            neg.iter().any(|&b| b),
            "ring {neg:?}: rejected iff a negative edge is on the cycle"
        );
    }
    println!(
        "rejected: unstratifiable, unsafe, non-ground fact, arity clash, syntax error; 100 random rings rejected iff negative edge"
    );
}

fn check_random_programs() {
    let mut r = Rng(0xDA7A_1042);
    let (mut naive_work, mut semi_work, mut naive_rounds, mut semi_rounds) =
        (0u64, 0u64, 0usize, 0usize);
    let (mut magic_ok, mut magic_skipped, mut with_neg, mut with_agg, mut facts_out) =
        (0, 0, 0, 0, 0usize);
    for case in 0..500 {
        let (src, order) = gen_program(&mut r);
        let prog = parse(&src)
            .unwrap_or_else(|e| panic!("generator produced a bad program ({e}):\n{src}"));
        let strata = stratify(&prog)
            .unwrap_or_else(|e| panic!("generated program should stratify ({e}):\n{src}"));
        with_neg += prog
            .rules
            .iter()
            .any(|r| r.body.iter().any(|l| matches!(l, Lit::Neg(_)))) as usize;
        with_agg += prog.rules.iter().any(|r| r.agg.is_some()) as usize;
        let naive = evaluate(&prog, &strata, Mode::Naive);
        let semi = evaluate(&prog, &strata, Mode::SemiNaive);
        let want = oracle(&prog, &order);
        for p in &order {
            let w = want.get(p).cloned().unwrap_or_default();
            assert_eq!(
                naive.rels.get(p).cloned().unwrap_or_default(),
                w,
                "naive != oracle on {p} (case {case})\n{src}"
            );
            assert_eq!(
                semi.rels.get(p).cloned().unwrap_or_default(),
                w,
                "semi-naive != oracle on {p} (case {case})\n{src}"
            );
            facts_out += w.len();
        }
        naive_work += naive.stats.work;
        semi_work += semi.stats.work;
        naive_rounds += naive.stats.iterations;
        semi_rounds += semi.stats.iterations;
        // magic sets: goal on the last IDB predicate with a random binding pattern
        let last = order.last().unwrap().clone();
        if prog
            .rules
            .iter()
            .any(|r| r.head.pred == last && r.agg.is_some())
        {
            magic_skipped += 1;
            continue;
        }
        let args: Vec<Term> = (0..2)
            .map(|i| {
                if r.chance(50) {
                    Term::Const(r.below(DOM) as i64)
                } else {
                    Term::Var(i)
                }
            })
            .collect();
        let mut goal_prog = prog.clone();
        goal_prog.query = Some(Atom {
            pred: last.clone(),
            args,
        });
        match magic_rewrite(&goal_prog) {
            Err(_) => magic_skipped += 1,
            Ok((mp, adorned, _)) => {
                let ms = stratify(&mp).expect("magic program must stratify");
                let mo = evaluate(&mp, &ms, Mode::SemiNaive);
                let q = goal_prog.query.as_ref().unwrap();
                let got: BTreeSet<_> = mo
                    .rels
                    .get(&adorned)
                    .into_iter()
                    .flatten()
                    .filter(|t| matches_goal(t, q))
                    .cloned()
                    .collect();
                let want: BTreeSet<_> = semi.rels[&last]
                    .iter()
                    .filter(|t| matches_goal(t, q))
                    .cloned()
                    .collect();
                assert_eq!(
                    got, want,
                    "magic answers differ (case {case}), goal {q:?}\n{src}"
                );
                magic_ok += 1;
            }
        }
    }
    println!(
        "500 random programs ({with_neg} with negation, {with_agg} with aggregates, {facts_out} oracle facts): naive == semi-naive == brute-force oracle"
    );
    println!(
        "  rounds naive {naive_rounds} vs semi-naive {semi_rounds}; tuples fetched naive {naive_work} vs semi-naive {semi_work}"
    );
    println!(
        "  magic sets: {magic_ok} goals answered identically, {magic_skipped} not rewritable (negation/aggregate in the way)"
    );
    assert!(magic_ok > 50, "too few goals exercised the magic rewrite");
    assert!(
        semi_work <= naive_work,
        "semi-naive must never fetch more than naive"
    );
}

fn chain_program(n: usize, linear: bool) -> String {
    let mut s: String = (1..n).map(|i| format!("edge({i},{}). ", i + 1)).collect();
    s += "path(X,Y) :- edge(X,Y). ";
    s += if linear {
        "path(X,Z) :- path(X,Y), edge(Y,Z)."
    } else {
        "path(X,Z) :- path(X,Y), path(Y,Z)."
    };
    s
}
fn check_work_saved() {
    println!("\nchain of 80 nodes (3160 path facts):");
    println!(
        "{:>12} {:>8} {:>8} {:>12} {:>12}",
        "rule shape", "mode", "rounds", "tuples read", "derived"
    );
    let mut results = HashMap::new();
    for linear in [true, false] {
        for mode in [Mode::Naive, Mode::SemiNaive] {
            let (_, o) = run_program(&chain_program(80, linear), mode).unwrap();
            assert_eq!(size(&o, "path"), 80 * 79 / 2);
            println!(
                "{:>12} {:>8} {:>8} {:>12} {:>12}",
                if linear { "linear" } else { "non-linear" },
                format!("{mode:?}"),
                o.stats.iterations,
                o.stats.work,
                o.stats.derived
            );
            results.insert((linear, mode), o.stats);
        }
    }
    let (n, s) = (
        &results[&(true, Mode::Naive)],
        &results[&(true, Mode::SemiNaive)],
    );
    assert!(
        n.work > 10 * s.work,
        "semi-naive should read >10x fewer tuples on a long chain ({} vs {})",
        n.work,
        s.work
    );
    assert_eq!(n.derived, s.derived);
    assert!(
        results[&(false, Mode::SemiNaive)].iterations < s.iterations,
        "doubling (non-linear) needs fewer rounds: log n vs n"
    );

    // magic sets on two disjoint components: the goal only touches one
    let mut src = chain_program(80, true);
    src += " edge(101,102). edge(102,103). edge(103,104). edge(104,105). edge(105,106). edge(106,107). ?- path(70,X).";
    let prog = parse(&src).unwrap();
    let strata = stratify(&prog).unwrap();
    let full = evaluate(&prog, &strata, Mode::SemiNaive);
    let (mp, adorned, ad) = magic_rewrite(&prog).unwrap();
    let mo = evaluate(&mp, &stratify(&mp).unwrap(), Mode::SemiNaive);
    let q = prog.query.as_ref().unwrap();
    let want: BTreeSet<_> = full.rels["path"]
        .iter()
        .filter(|t| matches_goal(t, q))
        .collect();
    let got: BTreeSet<_> = mo.rels[&adorned]
        .iter()
        .filter(|t| matches_goal(t, q))
        .collect();
    assert_eq!(want, got);
    assert_eq!(got.len(), 10);
    assert!(
        mo.stats.derived * 10 < full.stats.derived,
        "magic must derive far fewer facts for a bound goal"
    );
    println!(
        "magic sets, goal ?- path(70,X) (adornment {ad}): {} answers; facts derived {} (magic) vs {} (full), tuples read {} vs {}",
        got.len(),
        mo.stats.derived,
        full.stats.derived,
        mo.stats.work,
        full.stats.work
    );
}

fn main() {
    check_examples();
    check_rejections();
    check_random_programs();
    check_work_saved();
    println!("all checks passed");
}
