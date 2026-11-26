//! RECIPE 826 (compilers): unification, term rewriting + Knuth-Bendix completion, a Prolog engine, miniKanren and Datalog.
//! LEVEL: advanced
//! Use in: type checkers, theorem provers, rule engines, query languages and program synthesis: everything that solves equations between terms.   Run: cargo run --release --example term_rewriting_unification_logic_programming
//! COMMON USES: Hindley-Milner type inference (unification is its core, see hindley_milner_inference), Rust's trait solver (chalk and the next-gen
//! solver are Prolog-style resolution over trait goals), SWI-Prolog and Scryer, core.logic / miniKanren (relational programming and program synthesis),
//! Datalog engines (Soufflé, Datomic queries, the Rust-borrow-checker prototype Polonius, CodeQL-style code analysis), theorem provers such as
//! Vampire/E (completion and superposition), and compiler rewrite systems (peephole rules, Cranelift ISLE, egg's e-graphs; see egraph_equality_saturation).
//! Course: Baader and Nipkow "Term Rewriting and All That"; Friedman, Byrd, Kiselyov "The Reasoned Schemer".
#![allow(dead_code)]
//!
//! WHAT IS IN HERE.
//!  1. UNIFICATION three ways: Robinson (recursive, with occurs check), Martelli-Montanari (a set of equations transformed by delete / decompose /
//!     conflict / swap / eliminate / occurs-check rules) and the near-linear union-find version (merge the classes BEFORE recursing into children, so
//!     shared subterms are visited once; cyclic structures are rejected by a final acyclicity check). Verified: all three agree on solvability and give
//!     alpha-equivalent unified terms, and the result is a MOST GENERAL unifier: sigma is idempotent, sigma(s) = sigma(t), and for every ground unifier theta
//!     (brute force over all assignments of 8 small ground terms) theta(sigma(x)) = theta(x) for every variable x, which is exactly "theta = theta o sigma".
//!  2. TERM REWRITING: matching, innermost normalisation, a lexicographic path order (LPO) to PROVE termination of an oriented rule, critical pairs (unify a
//!     left-hand side with a non-variable subterm of another), and KNUTH-BENDIX COMPLETION: orient equations with the order, add critical pairs, interreduce.
//!     Fed the three group axioms (e*x = x, i(x)*x = e, (x*y)*z = x*(y*z)) it produces the famous ten-rule convergent system (checked against the known list).
//!     The result decides the equational theory: normal forms are checked against a concrete group (S3) on random terms, and commutativity, which cannot be
//!     oriented by any term order, is reported as a failure of completion (that is why AC-completion exists).
//!  3. A PROLOG ENGINE: Horn clauses, SLD resolution with backtracking over an explicit binding trail, cut (a cut barrier id travels up as a return value),
//!     `\+`, `is`, comparisons. Verified against brute-force oracles on 500 random programs (path counting on random DAGs, append/member/reverse/permutation/max on random lists).
//!  4. miniKANREN (microKanren core): goals are functions from states to streams, `disj` INTERLEAVES (fair search), relations run in any direction.
//!     `appendo` forwards and backwards, Peano `addo`/`mulo` (factoring 12 by running multiplication backwards), cross-checked against the Prolog engine, plus a
//!     small program-synthesis demo: find an expression with a relational evaluator from input/output examples.
//!  5. DATALOG: bottom-up naive vs semi-naive evaluation, which TERMINATES on cyclic graphs where top-down SLD loops, verified against Floyd-Warshall.
//! Top-down (Prolog) answers "what is X for this goal?" and can diverge; bottom-up (Datalog) computes the whole least model and always terminates (no function symbols).
//!
//! Read: Baader and Nipkow, Term Rewriting and All That (title only); Knuth and Bendix, "Simple Word Problems in Universal Algebras" (1970, title only);
//! Martelli and Montanari, "An Efficient Unification Algorithm" (1982, title only); Byrd, "Relational programming in miniKanren" (title only);
//! Friedman et al., "microKanren: A Minimal Functional Core for Relational Programming" (title only); miniKanren http://minikanren.org/ ;
//! https://en.wikipedia.org/wiki/Unification_(computer_science) ; https://en.wikipedia.org/wiki/Knuth%E2%80%93Bendix_completion_algorithm ;
//! https://en.wikipedia.org/wiki/Datalog ; https://en.wikipedia.org/wiki/SLD_resolution
//! TRY NEXT:
//!   - [S] Remove the occurs check from `unify_r` and run the `x = f(x)` case through `apply_full`: predict what happens, then guard `resolve` against cycles.
//!   - [S] Add `mul(X,e) = X` as a fourth input equation to `complete` and check that the final rule set is unchanged (it should be redundant); then try the order `e > mul > i` and report whether completion still terminates.
//!   - [M] Experiment: in `semi_naive` count `work` versus `naive` as the chain graph grows from 10 to 200 nodes; predict the asymptotic ratio before running.
//!   - [M] Port: encode a tiny Rust trait solver (`impl Display for Vec<T> where T: Display`) as Horn clauses and query "which types implement Display?" with `run_query`.
//!   - [A] Project: use the union-find unifier from section 1 to build the type inferencer of `hindley_milner_inference`, or feed critical-pair completion results into `egraph_equality_saturation` as rewrite rules and compare how far each gets on the group axioms.
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- terms, parser, printing

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum T {
    V(usize),
    F(String, Vec<T>),
}
use T::{F, V};
fn atom(s: &str) -> T {
    F(s.into(), vec![])
}
fn app(s: &str, a: Vec<T>) -> T {
    F(s.into(), a)
}
fn nil() -> T {
    atom("[]")
}
fn cons(h: T, t: T) -> T {
    app(".", vec![h, t])
}
fn list(items: Vec<T>) -> T {
    items.into_iter().rev().fold(nil(), |t, h| cons(h, t))
}
fn num(n: i64) -> T {
    atom(&n.to_string())
}
fn vars_of(t: &T, out: &mut Vec<usize>) {
    match t {
        V(v) => {
            if !out.contains(v) {
                out.push(*v)
            }
        }
        F(_, a) => a.iter().for_each(|x| vars_of(x, out)),
    }
}
fn shift(t: &T, off: usize) -> T {
    match t {
        V(v) => V(v + off),
        F(f, a) => F(f.clone(), a.iter().map(|x| shift(x, off)).collect()),
    }
}
fn show(t: &T) -> String {
    match t {
        V(v) => format!("_{v}"),
        F(f, a) if f == "." && a.len() == 2 => {
            let mut s = String::from("[");
            let mut cur = &a[1];
            s += &show(&a[0]);
            loop {
                match cur {
                    F(g, b) if g == "." && b.len() == 2 => {
                        s += &format!(",{}", show(&b[0]));
                        cur = &b[1];
                    }
                    F(g, _) if g == "[]" => break,
                    other => {
                        s += &format!("|{}", show(other));
                        break;
                    }
                }
            }
            s + "]"
        }
        F(f, a) if a.is_empty() => f.clone(),
        F(f, a) => format!("{f}({})", a.iter().map(show).collect::<Vec<_>>().join(",")),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Atom(String),
    Var(String),
    Punct(String),
}
fn tokenize(src: &str) -> Vec<Tok> {
    let cs: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '%' {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
        } else if c.is_alphanumeric() || c == '_' {
            let st = i;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            let w: String = cs[st..i].iter().collect();
            out.push(if c.is_uppercase() || c == '_' {
                Tok::Var(w)
            } else {
                Tok::Atom(w)
            });
        } else {
            let three: String = cs[i..(i + 3).min(cs.len())].iter().collect();
            let two: String = cs[i..(i + 2).min(cs.len())].iter().collect();
            let p = if ["=:=", "=\\="].contains(&three.as_str()) {
                three
            } else if [":-", "\\+", "\\=", "=<", ">="].contains(&two.as_str()) {
                two
            } else {
                c.to_string()
            };
            i += p.len();
            out.push(Tok::Punct(p));
        }
    }
    out
}
struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    names: HashMap<String, usize>,
    order: Vec<String>,
    next_anon: usize,
}
const INFIX: [(&str, u8); 14] = [
    ("=", 7),
    ("\\=", 7),
    ("<", 7),
    (">", 7),
    ("=<", 7),
    (">=", 7),
    ("=:=", 7),
    ("=\\=", 7),
    ("is", 7),
    ("+", 5),
    ("-", 5),
    ("*", 4),
    ("/", 4),
    ("mod", 4),
];
impl Parser {
    fn new(src: &str) -> Parser {
        Parser {
            toks: tokenize(src),
            pos: 0,
            names: HashMap::new(),
            order: vec![],
            next_anon: 0,
        }
    }
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn eat(&mut self, p: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Punct(q)) if q == p) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, p: &str) {
        assert!(
            self.eat(p),
            "expected `{p}` at token {} ({:?})",
            self.pos,
            self.peek()
        );
    }
    fn var(&mut self, n: &str) -> T {
        if n == "_" {
            self.next_anon += 1;
            let id = self.names.len() + 10_000 + self.next_anon; // renumbered by `renumber`
            return V(id);
        }
        let len = self.names.len();
        let id = *self.names.entry(n.to_string()).or_insert(len);
        if id == len {
            self.order.push(n.to_string());
        }
        V(id)
    }
    fn primary(&mut self) -> T {
        let t = self.peek().cloned().expect("unexpected end of input");
        self.pos += 1;
        match t {
            Tok::Var(n) => self.var(&n),
            Tok::Atom(a) => {
                if self.eat("(") {
                    let mut args = vec![self.term(9)];
                    while self.eat(",") {
                        args.push(self.term(9));
                    }
                    self.expect(")");
                    F(a, args)
                } else {
                    F(a, vec![])
                }
            }
            Tok::Punct(p) if p == "(" => {
                let t = self.term(10);
                self.expect(")");
                t
            }
            Tok::Punct(p) if p == "[" => {
                if self.eat("]") {
                    return nil();
                }
                let mut items = vec![self.term(9)];
                while self.eat(",") {
                    items.push(self.term(9));
                }
                let tail = if self.eat("|") { self.term(9) } else { nil() };
                self.expect("]");
                items.into_iter().rev().fold(tail, |t, h| cons(h, t))
            }
            Tok::Punct(p) if p == "!" => atom("!"),
            Tok::Punct(p) if p == "\\+" => app("\\+", vec![self.term(7)]),
            Tok::Punct(p) if p == "-" => match self.peek().cloned() {
                Some(Tok::Atom(d)) if d.chars().all(|c| c.is_ascii_digit()) => {
                    self.pos += 1;
                    atom(&format!("-{d}"))
                }
                _ => panic!("bad prefix minus"),
            },
            other => panic!("unexpected token {other:?}"),
        }
    }
    /// Precedence climbing; `maxp` 9 = argument level (no comma operator), 10 = parenthesised.
    fn term(&mut self, maxp: u8) -> T {
        let mut lhs = self.primary();
        loop {
            let op = match self.peek() {
                Some(Tok::Punct(p)) => p.clone(),
                Some(Tok::Atom(a)) if a == "is" || a == "mod" => a.clone(),
                _ => break,
            };
            let Some(&(_, p)) = INFIX.iter().find(|(o, _)| *o == op) else {
                break;
            };
            if p > maxp.min(8) && !(maxp == 10) && p > maxp {
                break;
            }
            self.pos += 1;
            let rhs = if p == 7 {
                self.term(6)
            } else {
                self.term(p - 1)
            };
            lhs = F(op, vec![lhs, rhs]);
        }
        lhs
    }
    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }
}
#[derive(Clone, Debug)]
struct Clause {
    head: T,
    body: Vec<T>,
}
/// Compact the variable ids of a clause to 0..n (anonymous `_` get fresh ids).
fn renumber(c: Clause) -> (Clause, usize) {
    let mut vs = vec![];
    vars_of(&c.head, &mut vs);
    c.body.iter().for_each(|g| vars_of(g, &mut vs));
    let map: HashMap<usize, usize> = vs.iter().enumerate().map(|(i, v)| (*v, i)).collect();
    fn re(t: &T, m: &HashMap<usize, usize>) -> T {
        match t {
            V(v) => V(m[v]),
            F(f, a) => F(f.clone(), a.iter().map(|x| re(x, m)).collect()),
        }
    }
    (
        Clause {
            head: re(&c.head, &map),
            body: c.body.iter().map(|g| re(g, &map)).collect(),
        },
        vs.len(),
    )
}
fn parse_program(src: &str) -> Vec<(Clause, usize)> {
    let mut p = Parser::new(src);
    let mut out = vec![];
    while !p.at_end() {
        p.names.clear();
        p.order.clear();
        let head = p.term(9);
        let mut body = vec![];
        if p.eat(":-") {
            body.push(p.term(9));
            while p.eat(",") {
                body.push(p.term(9));
            }
        }
        p.expect(".");
        out.push(renumber(Clause { head, body }));
    }
    out
}
/// Parse `t1, t2, ... .` as a query; returns the goals and the named variables in order of appearance.
fn parse_query(src: &str) -> (Vec<T>, Vec<String>, usize) {
    let mut p = Parser::new(src);
    let mut goals = vec![p.term(9)];
    while p.eat(",") {
        goals.push(p.term(9));
    }
    p.expect(".");
    let n = p.names.len() + p.next_anon;
    // anonymous variables get ids >= 10000: remap them above the named ones
    fn re(t: &T, base: usize) -> T {
        match t {
            V(v) if *v >= 10_000 => V(base + (v - 10_000)),
            V(v) => V(*v),
            F(f, a) => F(f.clone(), a.iter().map(|x| re(x, base)).collect()),
        }
    }
    let base = p.names.len();
    let goals = goals.iter().map(|g| re(g, base)).collect();
    (goals, p.order.clone(), n + 1)
}
/// Parse both sides of an equation with ONE parser, so that a name means the same variable on both sides (and different names differ).
fn parse_pair(a: &str, b: &str) -> (T, T) {
    let mut p = Parser::new(&format!("pair({a},{b})"));
    let F(_, mut args) = p.term(9) else {
        panic!("pair expected")
    };
    let right = args.pop().expect("two sides");
    (args.pop().expect("two sides"), right)
}
fn parse_one(src: &str) -> T {
    let mut p = Parser::new(src);
    p.term(9)
}

// ---------------------------------------------------------------- 1. unification

type Sub = HashMap<usize, T>;
fn walk(t: &T, s: &Sub) -> T {
    let mut t = t.clone();
    while let V(v) = &t {
        match s.get(v) {
            Some(n) => t = n.clone(),
            None => break,
        }
    }
    t
}
fn occurs(v: usize, t: &T, s: &Sub) -> bool {
    match walk(t, s) {
        V(w) => w == v,
        F(_, a) => a.iter().any(|x| occurs(v, x, s)),
    }
}
/// Resolve through a (possibly triangular) substitution all the way down.
fn apply_full(t: &T, s: &Sub) -> T {
    match walk(t, s) {
        V(v) => V(v),
        F(f, a) => F(f, a.iter().map(|x| apply_full(x, s)).collect()),
    }
}
/// One-pass simultaneous substitution (used when the codomain may mention the domain variables).
fn subst1(t: &T, s: &Sub) -> T {
    match t {
        V(v) => s.get(v).cloned().unwrap_or(V(*v)),
        F(f, a) => F(f.clone(), a.iter().map(|x| subst1(x, s)).collect()),
    }
}
fn unify_r(a: &T, b: &T, s: &mut Sub) -> bool {
    let (a, b) = (walk(a, s), walk(b, s));
    match (a, b) {
        (V(x), V(y)) if x == y => true,
        (V(x), t) | (t, V(x)) => {
            if occurs(x, &t, s) {
                false
            } else {
                s.insert(x, t);
                true
            }
        }
        (F(f, xs), F(g, ys)) => {
            f == g && xs.len() == ys.len() && xs.iter().zip(&ys).all(|(p, q)| unify_r(p, q, s))
        }
    }
}
/// Robinson, made idempotent at the end.
fn mgu_robinson(a: &T, b: &T) -> Option<Sub> {
    let mut s = Sub::new();
    if !unify_r(a, b, &mut s) {
        return None;
    }
    Some(s.keys().map(|k| (*k, apply_full(&V(*k), &s))).collect())
}
/// Martelli-Montanari: transform the equation set until it is in solved form.
fn mgu_mm(a: &T, b: &T) -> Option<Sub> {
    let mut eqs = vec![(a.clone(), b.clone())];
    let mut sol: Vec<(usize, T)> = vec![];
    while let Some((l, r)) = eqs.pop() {
        if l == r {
            continue; // delete
        }
        match (l, r) {
            (F(f, xs), F(g, ys)) => {
                if f != g || xs.len() != ys.len() {
                    return None; // conflict
                }
                eqs.extend(xs.into_iter().zip(ys)); // decompose
            }
            (t @ F(..), V(x)) => eqs.push((V(x), t)), // swap
            (V(x), t) => {
                let mut vs = vec![];
                vars_of(&t, &mut vs);
                if vs.contains(&x) {
                    return None; // occurs check
                }
                let one: Sub = [(x, t.clone())].into();
                for e in eqs.iter_mut() {
                    *e = (subst1(&e.0, &one), subst1(&e.1, &one)); // eliminate
                }
                for s in sol.iter_mut() {
                    s.1 = subst1(&s.1, &one);
                }
                sol.push((x, t));
            }
        }
    }
    Some(sol.into_iter().collect())
}

/// Near-linear unification on a term DAG with union-find.
enum Kind {
    Var(usize),
    Fun(String, Vec<usize>),
}
struct Dag {
    parent: Vec<usize>,
    kind: Vec<Kind>,
}
impl Dag {
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path halving
            x = self.parent[x];
        }
        x
    }
    fn build(&mut self, t: &T, vars: &mut HashMap<usize, usize>) -> usize {
        match t {
            V(v) => {
                if let Some(n) = vars.get(v) {
                    return *n;
                }
                let n = self.kind.len();
                self.kind.push(Kind::Var(*v));
                self.parent.push(n);
                vars.insert(*v, n);
                n
            }
            F(f, a) => {
                let kids: Vec<usize> = a.iter().map(|x| self.build(x, vars)).collect();
                let n = self.kind.len();
                self.kind.push(Kind::Fun(f.clone(), kids));
                self.parent.push(n);
                n
            }
        }
    }
    fn unify(&mut self, a: usize, b: usize) -> bool {
        let mut work = vec![(a, b)];
        while let Some((a, b)) = work.pop() {
            let (ra, rb) = (self.find(a), self.find(b));
            if ra == rb {
                continue;
            }
            match (&self.kind[ra], &self.kind[rb]) {
                (Kind::Var(_), _) => self.parent[ra] = rb,
                (_, Kind::Var(_)) => self.parent[rb] = ra,
                (Kind::Fun(f, xs), Kind::Fun(g, ys)) => {
                    if f != g || xs.len() != ys.len() {
                        return false;
                    }
                    let pairs: Vec<(usize, usize)> =
                        xs.iter().copied().zip(ys.iter().copied()).collect();
                    self.parent[ra] = rb; // merge FIRST: revisiting this pair is now a no-op, which is what makes it near-linear
                    work.extend(pairs);
                }
            }
        }
        true
    }
    fn acyclic(&mut self) -> bool {
        let n = self.kind.len();
        let mut state = vec![0u8; n]; // 0 new, 1 on stack, 2 done
        fn dfs(d: &mut Dag, x: usize, st: &mut Vec<u8>) -> bool {
            let r = d.find(x);
            match st[r] {
                1 => return false,
                2 => return true,
                _ => {}
            }
            st[r] = 1;
            let kids = if let Kind::Fun(_, k) = &d.kind[r] {
                k.clone()
            } else {
                vec![]
            };
            for k in kids {
                if !dfs(d, k, st) {
                    return false;
                }
            }
            st[r] = 2;
            true
        }
        (0..n).all(|x| dfs(self, x, &mut state))
    }
    fn extract(&mut self, x: usize, memo: &mut HashMap<usize, T>) -> T {
        let r = self.find(x);
        if let Some(t) = memo.get(&r) {
            return t.clone();
        }
        let t = match &self.kind[r] {
            Kind::Var(v) => V(*v),
            Kind::Fun(f, k) => {
                let (f, k) = (f.clone(), k.clone());
                F(f, k.into_iter().map(|c| self.extract(c, memo)).collect())
            }
        };
        memo.insert(r, t.clone());
        t
    }
}
fn mgu_uf(a: &T, b: &T) -> Option<Sub> {
    let mut d = Dag {
        parent: vec![],
        kind: vec![],
    };
    let mut vars = HashMap::new();
    let (na, nb) = (d.build(a, &mut vars), d.build(b, &mut vars));
    if !d.unify(na, nb) || !d.acyclic() {
        return None;
    }
    let mut memo = HashMap::new();
    let mut s = Sub::new();
    for (v, n) in vars {
        let t = d.extract(n, &mut memo);
        if t != V(v) {
            s.insert(v, t);
        }
    }
    Some(s)
}
fn canon(t: &T) -> T {
    let mut vs = vec![];
    vars_of(t, &mut vs);
    let m: Sub = vs.iter().enumerate().map(|(i, v)| (*v, V(i))).collect();
    subst1(t, &m)
}
fn rand_term(rng: &mut Rng, depth: u32) -> T {
    if depth == 0 || rng.chance(25) {
        return if rng.chance(55) {
            V(rng.below(3) as usize)
        } else {
            atom(["a", "b"][rng.below(2) as usize])
        };
    }
    if rng.chance(45) {
        app("f", vec![rand_term(rng, depth - 1)])
    } else {
        app(
            "g",
            vec![rand_term(rng, depth - 1), rand_term(rng, depth - 1)],
        )
    }
}
/// Replace random subterms by random terms so that pairs are often (but not always) unifiable.
fn perturb(rng: &mut Rng, t: &T) -> T {
    match t {
        F(f, a) if !rng.chance(25) => F(f.clone(), a.iter().map(|x| perturb(rng, x)).collect()),
        _ => rand_term(rng, 2),
    }
}
fn ground_universe() -> Vec<T> {
    let d0 = [atom("a"), atom("b")];
    let mut u: Vec<T> = d0.to_vec();
    for x in &d0 {
        u.push(app("f", vec![x.clone()]));
    }
    for x in &d0 {
        for y in &d0 {
            u.push(app("g", vec![x.clone(), y.clone()]));
        }
    }
    u
}
fn check_unification(rng: &mut Rng) {
    let universe = ground_universe();
    let (mut unifiable, mut total, mut theta_checked) = (0, 0, 0u64);
    for _ in 0..1500 {
        let s = rand_term(rng, 3);
        let t = if rng.chance(55) {
            perturb(rng, &s)
        } else {
            rand_term(rng, 3)
        };
        total += 1;
        let results = [mgu_robinson(&s, &t), mgu_mm(&s, &t), mgu_uf(&s, &t)];
        assert!(
            results.iter().all(|r| r.is_some() == results[0].is_some()),
            "algorithms disagree on {} =? {}",
            show(&s),
            show(&t)
        );
        let mut vs = vec![];
        vars_of(&s, &mut vs);
        vars_of(&t, &mut vs);
        // enumerate every assignment of the (at most 3) variables to the 8 ground terms
        let assignments = universe.len().pow(vs.len() as u32);
        let theta_of = |code: usize| -> Sub {
            let mut c = code;
            vs.iter()
                .map(|v| {
                    let g = universe[c % universe.len()].clone();
                    c /= universe.len();
                    (*v, g)
                })
                .collect()
        };
        match &results[0] {
            None => {
                for code in 0..assignments {
                    let th = theta_of(code);
                    assert_ne!(
                        subst1(&s, &th),
                        subst1(&t, &th),
                        "no unifier reported but theta unifies {} and {}",
                        show(&s),
                        show(&t)
                    );
                    theta_checked += 1;
                }
            }
            Some(_) => {
                unifiable += 1;
                let canon_u: Vec<T> = results
                    .iter()
                    .map(|r| canon(&subst1(&s, r.as_ref().unwrap())))
                    .collect();
                assert!(
                    canon_u.iter().all(|u| *u == canon_u[0]),
                    "unified terms differ up to renaming"
                );
                for r in &results {
                    let sg = r.as_ref().unwrap();
                    let ap = |x: &T| apply_full(x, sg);
                    assert_eq!(ap(&s), ap(&t), "not a unifier");
                    assert!(
                        sg.iter().all(|(k, v)| apply_full(v, sg) == *v
                            && !{
                                let mut o = vec![];
                                vars_of(v, &mut o);
                                o.contains(k)
                            }),
                        "not idempotent"
                    );
                }
                let sg = results[0].as_ref().unwrap();
                for code in 0..assignments {
                    let th = theta_of(code);
                    theta_checked += 1;
                    if subst1(&s, &th) == subst1(&t, &th) {
                        for v in &vs {
                            assert_eq!(
                                subst1(&apply_full(&V(*v), sg), &th),
                                th[v],
                                "sigma is not most general for {} =? {}",
                                show(&s),
                                show(&t)
                            );
                        }
                    }
                }
            }
        }
    }
    println!(
        "  {total} random pairs ({unifiable} unifiable): 3 algorithms agree, mgu property checked against {theta_checked} ground substitutions"
    );
}

/// f(x1, ..., xn) =? f(g(x0,x0), ..., g(x(n-1),x(n-1))): the INPUT has O(n) nodes but sigma(xn) written as a tree has 2^(n+1)-1 nodes.
/// A shared graph plus union-find only ever touches the O(n) input nodes, as long as nobody asks for the answer to be written out as a tree.
fn exponential_family() {
    let family = |n: usize| {
        (
            app("f", (1..=n).map(V).collect()),
            app("f", (0..n).map(|i| app("g", vec![V(i), V(i)])).collect()),
        )
    };
    // small n: the extracted answer really is exponential, and all three algorithms agree on it
    for n in [1usize, 4, 10] {
        let (a, b) = family(n);
        for sg in [mgu_robinson(&a, &b), mgu_mm(&a, &b), mgu_uf(&a, &b)] {
            let sg = sg.expect("the family is unifiable");
            assert_eq!(term_size(&sg[&n]), (1 << (n + 1)) - 1);
        }
    }
    let n = 2000; // sigma(x2000) has about 2^2001 tree nodes: it could never be built
    let (a, b) = family(n);
    let mut d = Dag {
        parent: vec![],
        kind: vec![],
    };
    let mut vars = HashMap::new();
    let (na, nb) = (d.build(&a, &mut vars), d.build(&b, &mut vars));
    let t0 = std::time::Instant::now();
    assert!(d.unify(na, nb) && d.acyclic());
    assert!(
        d.kind.len() <= 5 * n + 4,
        "the graph stays linear in n: {}",
        d.kind.len()
    );
    println!(
        "  exponential family, n = {n}: unified on a {}-node graph in {:?}; written as a tree the answer would have ~2^{} nodes",
        d.kind.len(),
        t0.elapsed(),
        n + 1
    );
}

// ---------------------------------------------------------------- 2. term rewriting and completion

#[derive(Clone, Debug, PartialEq)]
struct Rule {
    l: T,
    r: T,
}
fn matches(p: &T, t: &T, s: &mut Sub) -> bool {
    match (p, t) {
        (V(x), _) => match s.get(x) {
            Some(b) => b == t,
            None => {
                s.insert(*x, t.clone());
                true
            }
        },
        (F(f, ps), F(g, ts)) => {
            f == g && ps.len() == ts.len() && ps.iter().zip(ts).all(|(a, b)| matches(a, b, s))
        }
        _ => false,
    }
}
fn rewrite_step(t: &T, rules: &[Rule]) -> Option<T> {
    if let F(f, args) = t {
        for (i, a) in args.iter().enumerate() {
            if let Some(n) = rewrite_step(a, rules) {
                let mut v = args.clone();
                v[i] = n;
                return Some(F(f.clone(), v)); // innermost first
            }
        }
    }
    for r in rules {
        let mut s = Sub::new();
        if matches(&r.l, t, &mut s) {
            return Some(subst1(&r.r, &s));
        }
    }
    None
}
fn nf(t: &T, rules: &[Rule]) -> T {
    let mut t = t.clone();
    for _ in 0..100_000 {
        match rewrite_step(&t, rules) {
            Some(n) => t = n,
            None => return t,
        }
    }
    panic!("normalisation did not terminate: {}", show(&t));
}
/// Lexicographic path order with a symbol precedence (higher number = bigger symbol).
fn lpo_gt(s: &T, t: &T, prec: &HashMap<String, usize>) -> bool {
    match (s, t) {
        (V(_), _) => false,
        (F(..), V(x)) => {
            let mut vs = vec![];
            vars_of(s, &mut vs);
            vs.contains(x)
        }
        (F(f, ss), F(g, ts)) => {
            if ss.iter().any(|si| si == t || lpo_gt(si, t, prec)) {
                return true;
            }
            let (pf, pg) = (prec[f], prec[g]);
            if pf > pg {
                return ts.iter().all(|tj| lpo_gt(s, tj, prec));
            }
            if pf == pg && ss.len() == ts.len() {
                for (a, b) in ss.iter().zip(ts) {
                    if a != b {
                        return lpo_gt(a, b, prec) && ts.iter().all(|tj| lpo_gt(s, tj, prec));
                    }
                }
            }
            false
        }
    }
}
fn positions(t: &T, here: &mut Vec<usize>, out: &mut Vec<(Vec<usize>, T)>) {
    if let F(_, args) = t {
        out.push((here.clone(), t.clone()));
        for (i, a) in args.iter().enumerate() {
            here.push(i);
            positions(a, here, out);
            here.pop();
        }
    }
}
fn replace_at(t: &T, pos: &[usize], new: &T) -> T {
    match (pos.split_first(), t) {
        (None, _) => new.clone(),
        (Some((i, rest)), F(f, a)) => {
            let mut v = a.clone();
            v[*i] = replace_at(&a[*i], rest, new);
            F(f.clone(), v)
        }
        _ => unreachable!(),
    }
}
fn rename_rule(r: &Rule, off: usize) -> Rule {
    Rule {
        l: shift(&r.l, off),
        r: shift(&r.r, off),
    }
}
/// Critical pairs of r1 overlapped by r2 (r2 renamed apart); the trivial root overlap of a rule with itself is skipped.
fn critical_pairs(r1: &Rule, r2: &Rule, same: bool) -> Vec<(T, T)> {
    let mut v1 = vec![];
    vars_of(&r1.l, &mut v1);
    assert!(
        v1.iter().all(|v| *v < 1000),
        "r1 must not use the ids reserved for the renamed copy of r2"
    );
    let r2 = rename_rule(r2, 1000);
    let mut ps = vec![];
    positions(&r1.l, &mut vec![], &mut ps);
    let mut out = vec![];
    for (pos, sub) in ps {
        if same && pos.is_empty() {
            continue;
        }
        if let Some(s) = mgu_robinson(&sub, &r2.l) {
            out.push((
                apply_full(&r1.r, &s),
                apply_full(&replace_at(&r1.l, &pos, &r2.r), &s),
            ));
        }
    }
    out
}
fn all_critical_pairs(rules: &[Rule]) -> Vec<(T, T)> {
    let mut out = vec![];
    for (i, a) in rules.iter().enumerate() {
        for (j, b) in rules.iter().enumerate() {
            out.extend(critical_pairs(a, b, i == j));
        }
    }
    out
}
fn confluent(rules: &[Rule]) -> bool {
    all_critical_pairs(rules)
        .iter()
        .all(|(a, b)| nf(a, rules) == nf(b, rules))
}
fn reducible_at_any(t: &T, rule: &Rule) -> bool {
    let mut ps = vec![];
    positions(t, &mut vec![], &mut ps);
    ps.iter()
        .any(|(_, sub)| matches(&rule.l, sub, &mut Sub::new()))
}
fn term_size(t: &T) -> usize {
    match t {
        V(_) => 1,
        F(_, a) => 1 + a.iter().map(term_size).sum::<usize>(),
    }
}
/// Knuth-Bendix completion. Err = an equation that the order cannot orient.
/// Two details matter. (1) The SMALLEST pending equation is processed first. This is fair (no equation waits forever, as only finitely many terms
/// are smaller). With a LIFO stack this program did not finish the group axioms within 40 s: it keeps expanding the newest critical pairs.
/// (2) Every stored rule has its variables renumbered to 0..n. `critical_pairs` renames apart with a fixed offset, which is only sound if no rule
/// already uses ids at or above that offset, and terms taken from critical pairs do until renumbered (the assertion there guards this).
fn complete(eqs: &[(T, T)], prec: &HashMap<String, usize>) -> Result<Vec<Rule>, String> {
    let mut rules: Vec<Rule> = vec![];
    let mut todo: Vec<(T, T)> = eqs.to_vec();
    let mut steps = 0;
    while let Some(next) =
        (0..todo.len()).min_by_key(|&i| term_size(&todo[i].0) + term_size(&todo[i].1))
    {
        let (a, b) = todo.swap_remove(next);
        steps += 1;
        if steps > 5000 {
            return Err("gave up after 5000 steps".into());
        }
        let (a, b) = (nf(&a, &rules), nf(&b, &rules));
        if a == b {
            continue;
        }
        let (a, b) = canon_pair(&a, &b);
        let new = if lpo_gt(&a, &b, prec) {
            Rule { l: a, r: b }
        } else if lpo_gt(&b, &a, prec) {
            Rule { l: b, r: a }
        } else {
            return Err(format!("cannot orient {} = {}", show(&a), show(&b)));
        };
        // interreduce: old rules whose left side the new rule can rewrite go back to the equations; others get a reduced right side
        let mut kept = vec![];
        for old in rules.drain(..) {
            if reducible_at_any(&old.l, &new) {
                todo.push((old.l, old.r));
            } else {
                let r = nf(&old.r, std::slice::from_ref(&new));
                kept.push(Rule { l: old.l, r });
            }
        }
        rules = kept;
        rules.push(new.clone());
        for r in rules.clone() {
            todo.extend(critical_pairs(&new, &r, r == new));
            if r != new {
                todo.extend(critical_pairs(&r, &new, false));
            }
        }
    }
    Ok(rules)
}
fn rule_text(r: &Rule) -> String {
    let (l, rhs) = canon_pair(&r.l, &r.r); // canonical naming over both sides jointly
    format!("{} -> {}", show(&l), show(&rhs))
}
fn group_prec() -> HashMap<String, usize> {
    [("i", 3), ("mul", 2), ("e", 1)]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect()
}
// concrete S3 as the semantic oracle
type Perm = [usize; 3];
fn compose(a: Perm, b: Perm) -> Perm {
    [a[b[0]], a[b[1]], a[b[2]]]
}
fn inverse(a: Perm) -> Perm {
    let mut r = [0; 3];
    for i in 0..3 {
        r[a[i]] = i;
    }
    r
}
fn eval_group(t: &T, env: &[Perm; 3]) -> Perm {
    match t {
        V(v) => env[*v],
        F(f, a) => match (f.as_str(), a.len()) {
            ("e", 0) => [0, 1, 2],
            ("i", 1) => inverse(eval_group(&a[0], env)),
            ("mul", 2) => compose(eval_group(&a[0], env), eval_group(&a[1], env)),
            _ => panic!("bad group term"),
        },
    }
}
fn rand_group_term(rng: &mut Rng, depth: u32) -> T {
    if depth == 0 || rng.chance(20) {
        return if rng.chance(75) {
            V(rng.below(3) as usize)
        } else {
            atom("e")
        };
    }
    if rng.chance(25) {
        app("i", vec![rand_group_term(rng, depth - 1)])
    } else {
        app(
            "mul",
            vec![
                rand_group_term(rng, depth - 1),
                rand_group_term(rng, depth - 1),
            ],
        )
    }
}
fn check_group_completion(rng: &mut Rng) {
    let eq = |s: &str| {
        let F(_, a) = parse_one(s) else {
            panic!("equation expected")
        };
        canon_pair(&a[0], &a[1])
    };
    let axioms = [
        eq("mul(e,X) = X"),
        eq("mul(i(X),X) = e"),
        eq("mul(mul(X,Y),Z) = mul(X,mul(Y,Z))"),
    ];
    let rules = complete(&axioms, &group_prec()).expect("group axioms must complete");
    let mut got: Vec<String> = rules.iter().map(rule_text).collect();
    got.sort();
    let known = [
        "mul(e,_0) -> _0",
        "mul(i(_0),_0) -> e",
        "mul(mul(_0,_1),_2) -> mul(_0,mul(_1,_2))",
        "mul(i(_0),mul(_0,_1)) -> _1",
        "i(e) -> e",
        "mul(_0,e) -> _0",
        "i(i(_0)) -> _0",
        "mul(_0,i(_0)) -> e",
        "mul(_0,mul(i(_0),_1)) -> _1",
        "i(mul(_0,_1)) -> mul(i(_1),i(_0))",
    ];
    let mut want: Vec<String> = known.iter().map(|s| rule_text(&parse_rule(s))).collect();
    want.sort();
    println!("  completed the 3 group axioms into {} rules:", rules.len());
    for r in &got {
        println!("    {r}");
    }
    assert_eq!(
        got, want,
        "completion must reproduce the classic ten-rule system"
    );
    assert!(
        rules.iter().all(|r| lpo_gt(&r.l, &r.r, &group_prec())),
        "every rule decreases in the LPO, so the system terminates"
    );
    assert!(
        confluent(&rules),
        "all critical pairs of the result are joinable"
    );
    // semantics: normal forms agree with S3 evaluation, and equal normal forms imply equality in S3
    let perms: [Perm; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let envs: Vec<[Perm; 3]> = (0..216)
        .map(|c| [perms[c % 6], perms[c / 6 % 6], perms[c / 36]])
        .collect();
    let (mut shrunk, mut equal_nf) = (0, 0);
    for _ in 0..400 {
        let (s, t) = (rand_group_term(rng, 4), rand_group_term(rng, 4));
        let (ns, nt) = (nf(&s, &rules), nf(&t, &rules));
        assert_eq!(nf(&ns, &rules), ns, "normal form is stable");
        for env in &envs {
            assert_eq!(
                eval_group(&s, env),
                eval_group(&ns, env),
                "rewriting must preserve meaning: {} vs {}",
                show(&s),
                show(&ns)
            );
        }
        if ns == nt {
            equal_nf += 1;
            assert!(envs.iter().all(|e| eval_group(&s, e) == eval_group(&t, e)));
        }
        if show(&ns).len() < show(&s).len() {
            shrunk += 1;
        }
    }
    // also: terms that are provably equal but syntactically different now have equal normal forms
    let (l, r) = (
        parse_one("i(mul(X,mul(Y,i(X))))"),
        parse_one("mul(X,mul(i(Y),i(X)))"),
    );
    assert_eq!(nf(&l, &rules), nf(&r, &rules));
    println!(
        "  400 random terms: normal forms preserve meaning in S3 for all 216 assignments ({shrunk} got smaller, {equal_nf} random pairs had equal normal forms)"
    );
    // commutativity cannot be oriented by any reduction order
    let comm = eq("mul(X,Y) = mul(Y,X)");
    let r = complete(&[comm], &group_prec());
    println!("  completion of commutativity: {:?}", r.as_ref().err());
    assert!(r.is_err());
    // a non-confluent system gains the rule that joins its critical pair
    let prec: HashMap<String, usize> = [("f", 2), ("a", 1), ("b", 1), ("c", 0)]
        .iter()
        .map(|(k, v)| (k.to_string(), *v))
        .collect();
    let start = [
        Rule {
            l: parse_one("f(a)"),
            r: parse_one("b"),
        },
        Rule {
            l: parse_one("a"),
            r: parse_one("c"),
        },
    ];
    assert!(!confluent(&start));
    let done = complete(
        &start
            .iter()
            .map(|r| (r.l.clone(), r.r.clone()))
            .collect::<Vec<_>>(),
        &prec,
    )
    .unwrap();
    println!(
        "  {{f(a)->b, a->c}} is not confluent (f(c) vs b); completion yields {:?}",
        done.iter().map(rule_text).collect::<Vec<_>>()
    );
    assert!(confluent(&done));
}
fn canon_pair(a: &T, b: &T) -> (T, T) {
    let mut vs = vec![];
    vars_of(a, &mut vs);
    vars_of(b, &mut vs);
    let m: Sub = vs.iter().enumerate().map(|(i, v)| (*v, V(i))).collect();
    (subst1(a, &m), subst1(b, &m))
}
fn parse_rule(s: &str) -> Rule {
    let (l, r) = s.split_once("->").unwrap();
    // variables are written _0,_1: parse them as Prolog variables named _0,_1 ... (anonymous would differ), so map names by hand
    let fix = |x: &str| {
        let mut x = x.trim().to_string();
        for i in (0..4).rev() {
            x = x.replace(&format!("_{i}"), &format!("V{i}"));
        }
        let mut p = Parser::new(&x);
        let t = p.term(9);
        // variables were numbered by first occurrence per parse; renumber by the digit in their name instead
        let names = p.order.clone();
        let m: Sub = names
            .iter()
            .map(|n| (p.names[n], V(n[1..].parse::<usize>().unwrap())))
            .collect();
        subst1(&t, &m)
    };
    Rule {
        l: fix(l),
        r: fix(r),
    }
}

// ---------------------------------------------------------------- 3. a Prolog engine (SLD resolution, cut, negation, arithmetic)

#[derive(PartialEq, Debug)]
enum Flow {
    Cont,
    Stop,
    Cut(usize),
}
struct Node {
    goal: T,
    cut: usize,
    next: GL,
}
type GL = Option<Rc<Node>>;
fn push_goal(g: T, cut: usize, next: GL) -> GL {
    Some(Rc::new(Node { goal: g, cut, next }))
}
struct Sink {
    vars: Vec<usize>,
    sols: Vec<Vec<T>>,
    limit: usize,
}
struct Eng {
    db: Vec<(Clause, usize)>,
    index: HashMap<(String, usize), Vec<usize>>,
    bind: Vec<Option<T>>,
    trail: Vec<usize>,
    cut_ids: usize,
    depth_exceeded: bool,
    max_depth: usize,
}
impl Eng {
    fn new(db: Vec<(Clause, usize)>) -> Eng {
        let mut index: HashMap<(String, usize), Vec<usize>> = HashMap::new();
        for (i, (c, _)) in db.iter().enumerate() {
            if let F(f, a) = &c.head {
                index.entry((f.clone(), a.len())).or_default().push(i);
            }
        }
        Eng {
            db,
            index,
            bind: vec![],
            trail: vec![],
            cut_ids: 0,
            depth_exceeded: false,
            max_depth: 3000,
        }
    }
    fn walk(&self, t: &T) -> T {
        let mut t = t.clone();
        while let V(v) = &t {
            match &self.bind[*v] {
                Some(n) => t = n.clone(),
                None => break,
            }
        }
        t
    }
    fn resolve(&self, t: &T) -> T {
        match self.walk(t) {
            V(v) => V(v),
            F(f, a) => F(f, a.iter().map(|x| self.resolve(x)).collect()),
        }
    }
    fn occurs(&self, v: usize, t: &T) -> bool {
        match self.walk(t) {
            V(w) => w == v,
            F(_, a) => a.iter().any(|x| self.occurs(v, x)),
        }
    }
    fn unify(&mut self, a: &T, b: &T) -> bool {
        let (a, b) = (self.walk(a), self.walk(b));
        match (a, b) {
            (V(x), V(y)) if x == y => true,
            (V(x), t) | (t, V(x)) => {
                if self.occurs(x, &t) {
                    return false;
                }
                self.bind[x] = Some(t);
                self.trail.push(x);
                true
            }
            (F(f, xs), F(g, ys)) => {
                f == g && xs.len() == ys.len() && xs.iter().zip(&ys).all(|(p, q)| self.unify(p, q))
            }
        }
    }
    fn undo(&mut self, mark: usize) {
        while self.trail.len() > mark {
            let v = self.trail.pop().unwrap();
            self.bind[v] = None;
        }
    }
    fn eval(&self, t: &T) -> Option<i64> {
        match self.walk(t) {
            F(f, a) if a.is_empty() => f.parse().ok(),
            F(f, a) if a.len() == 2 => {
                let (x, y) = (self.eval(&a[0])?, self.eval(&a[1])?);
                match f.as_str() {
                    "+" => x.checked_add(y),
                    "-" => x.checked_sub(y),
                    "*" => x.checked_mul(y),
                    "/" => x.checked_div(y),
                    "mod" => x.checked_rem(y),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    fn solve(&mut self, gl: &GL, depth: usize, sink: &mut Sink) -> Flow {
        let Some(n) = gl else {
            let sol = sink.vars.iter().map(|v| self.resolve(&V(*v))).collect();
            sink.sols.push(sol);
            return if sink.sols.len() >= sink.limit {
                Flow::Stop
            } else {
                Flow::Cont
            };
        };
        let goal = self.walk(&n.goal);
        let (rest, cut) = (n.next.clone(), n.cut);
        let F(name, args) = goal else {
            return Flow::Cont;
        }; // unbound goal: fail
        match (name.as_str(), args.len()) {
            ("true", 0) => self.solve(&rest, depth, sink),
            ("fail", 0) => Flow::Cont,
            ("!", 0) => {
                let r = self.solve(&rest, depth, sink);
                if r == Flow::Stop { r } else { Flow::Cut(cut) }
            }
            (",", 2) => {
                let g = push_goal(args[0].clone(), cut, push_goal(args[1].clone(), cut, rest));
                self.solve(&g, depth, sink)
            }
            ("=", 2) => {
                let mark = self.trail.len();
                let r = if self.unify(&args[0], &args[1]) {
                    self.solve(&rest, depth, sink)
                } else {
                    Flow::Cont
                };
                self.undo(mark);
                r
            }
            ("\\=", 2) => {
                let mark = self.trail.len();
                let ok = !self.unify(&args[0], &args[1]);
                self.undo(mark);
                if ok {
                    self.solve(&rest, depth, sink)
                } else {
                    Flow::Cont
                }
            }
            ("is", 2) => {
                let Some(v) = self.eval(&args[1]) else {
                    return Flow::Cont;
                };
                let mark = self.trail.len();
                let r = if self.unify(&args[0], &num(v)) {
                    self.solve(&rest, depth, sink)
                } else {
                    Flow::Cont
                };
                self.undo(mark);
                r
            }
            ("<" | ">" | "=<" | ">=" | "=:=" | "=\\=", 2) => {
                let (Some(x), Some(y)) = (self.eval(&args[0]), self.eval(&args[1])) else {
                    return Flow::Cont;
                };
                let ok = match name.as_str() {
                    "<" => x < y,
                    ">" => x > y,
                    "=<" => x <= y,
                    ">=" => x >= y,
                    "=:=" => x == y,
                    _ => x != y,
                };
                if ok {
                    self.solve(&rest, depth, sink)
                } else {
                    Flow::Cont
                }
            }
            ("\\+", 1) => {
                self.cut_ids += 1;
                let mut sub = Sink {
                    vars: vec![],
                    sols: vec![],
                    limit: 1,
                };
                let g = push_goal(args[0].clone(), self.cut_ids, None);
                let _ = self.solve(&g, depth + 1, &mut sub);
                if sub.sols.is_empty() {
                    self.solve(&rest, depth, sink)
                } else {
                    Flow::Cont
                }
            }
            _ => {
                if depth > self.max_depth {
                    self.depth_exceeded = true;
                    return Flow::Cont;
                }
                self.cut_ids += 1;
                let me = self.cut_ids;
                let Some(cands) = self.index.get(&(name.clone(), args.len())).cloned() else {
                    return Flow::Cont;
                };
                let goal = F(name, args);
                for ci in cands {
                    let (cl, nv) = self.db[ci].clone();
                    let off = self.bind.len();
                    self.bind.extend(std::iter::repeat_n(None, nv));
                    let (head, body) = (
                        shift(&cl.head, off),
                        cl.body.iter().map(|g| shift(g, off)).collect::<Vec<_>>(),
                    );
                    let mark = self.trail.len();
                    let mut r = Flow::Cont;
                    if self.unify(&head, &goal) {
                        let gl2 = body
                            .into_iter()
                            .rev()
                            .fold(rest.clone(), |nx, g| push_goal(g, me, nx));
                        r = self.solve(&gl2, depth + 1, sink);
                    }
                    self.undo(mark);
                    self.bind.truncate(off);
                    match r {
                        Flow::Cont => {}
                        Flow::Cut(id) if id == me => return Flow::Cont, // the clause committed: skip the remaining clauses
                        other => return other,
                    }
                }
                Flow::Cont
            }
        }
    }
}
struct QueryResult {
    sols: Vec<Vec<(String, T)>>,
    exceeded: bool,
}
fn run_query(program: &str, query: &str, limit: usize) -> QueryResult {
    let mut eng = Eng::new(parse_program(program));
    let (goals, names, nvars) = parse_query(query);
    eng.bind = vec![None; nvars];
    let mut sink = Sink {
        vars: (0..names.len()).collect(),
        sols: vec![],
        limit,
    };
    let gl = goals
        .into_iter()
        .rev()
        .fold(None, |nx, g| push_goal(g, 0, nx));
    eng.solve(&gl, 0, &mut sink);
    let sols = sink
        .sols
        .into_iter()
        .map(|s| names.iter().cloned().zip(s).collect())
        .collect();
    QueryResult {
        sols,
        exceeded: eng.depth_exceeded,
    }
}
fn sol_strings(r: &QueryResult) -> Vec<String> {
    r.sols
        .iter()
        .map(|s| {
            s.iter()
                .map(|(n, t)| format!("{n}={}", show(t)))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect()
}

const LISTS: &str = "
append([],L,L).
append([H|T],L,[H|R]) :- append(T,L,R).
member(X,[X|_]).
member(X,[_|T]) :- member(X,T).
reverse(L,R) :- rev(L,[],R).
rev([],A,A).
rev([H|T],A,R) :- rev(T,[H|A],R).
select(X,[X|T],T).
select(X,[H|T],[H|R]) :- select(X,T,R).
perm([],[]).
perm(L,[H|T]) :- select(H,L,R), perm(R,T).
len([],0).
len([_|T],N) :- len(T,M), N is M+1.
max(X,Y,X) :- X >= Y, !.
max(_,Y,Y).
absent(X,L) :- \\+ member(X,L).
";

fn list_text(v: &[i64]) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn perms_of(v: &[i64]) -> Vec<Vec<i64>> {
    if v.is_empty() {
        return vec![vec![]];
    }
    let mut out = vec![];
    for i in 0..v.len() {
        let mut rest = v.to_vec();
        let x = rest.remove(i);
        for mut p in perms_of(&rest) {
            p.insert(0, x);
            out.push(p);
        }
    }
    out
}
fn check_prolog(rng: &mut Rng) {
    let mut programs = 0;
    let mut solutions_checked = 0usize;
    // type A: path counting on random DAGs (SLD yields one solution per PATH, so counts must match a DP)
    for _ in 0..250 {
        let n = 4 + rng.below(5) as usize;
        let mut edges = vec![];
        let mut text = String::new();
        for i in 0..n {
            for j in i + 1..n {
                if rng.chance(35) {
                    edges.push((i, j));
                    text += &format!("edge(n{i},n{j}).\n");
                }
            }
        }
        text += "path(X,Y) :- edge(X,Y).\npath(X,Y) :- edge(X,Z), path(Z,Y).\n";
        let mut cnt = vec![vec![0usize; n]; n];
        for x in (0..n).rev() {
            for y in 0..n {
                cnt[x][y] = edges.iter().filter(|(a, b)| *a == x && *b == y).count()
                    + edges
                        .iter()
                        .filter(|(a, _)| *a == x)
                        .map(|(_, z)| cnt[*z][y])
                        .sum::<usize>();
            }
        }
        let a = rng.below(n as u64) as usize;
        let r = run_query(&text, &format!("path(n{a},Y)."), 100_000);
        assert!(!r.exceeded);
        let mut got = vec![0usize; n];
        for s in &r.sols {
            let F(name, _) = &s[0].1 else { panic!() };
            got[name[1..].parse::<usize>().unwrap()] += 1;
        }
        assert_eq!(
            got, cnt[a],
            "solution multiplicities must equal path counts (start n{a})\n{text}"
        );
        let b = rng.below(n as u64) as usize;
        let r = run_query(&text, &format!("path(n{a},n{b})."), 100_000);
        assert_eq!(r.sols.len(), cnt[a][b]);
        solutions_checked += r.sols.len() + got.iter().sum::<usize>();
        programs += 1;
    }
    // type B: list relations against brute force
    for _ in 0..250 {
        let n = rng.below(6) as usize;
        let mut v: Vec<i64> = (0..n).map(|_| rng.below(5) as i64).collect();
        if rng.chance(50) {
            v.sort();
            v.dedup();
        }
        let l = list_text(&v);
        match rng.below(6) {
            0 => {
                let r = run_query(LISTS, &format!("append(X,Y,{l})."), 1000);
                let want: Vec<String> = (0..=v.len())
                    .map(|k| format!("X={}, Y={}", list_text(&v[..k]), list_text(&v[k..])))
                    .collect();
                assert_eq!(sol_strings(&r), want);
                solutions_checked += want.len();
            }
            1 => {
                let r = run_query(LISTS, &format!("member(X,{l})."), 1000);
                let want: Vec<String> = v.iter().map(|x| format!("X={x}")).collect();
                assert_eq!(sol_strings(&r), want);
                solutions_checked += want.len();
            }
            2 => {
                let r = run_query(LISTS, &format!("reverse({l},R)."), 10);
                let mut rv = v.clone();
                rv.reverse();
                assert_eq!(sol_strings(&r), vec![format!("R={}", list_text(&rv))]);
                solutions_checked += 1;
            }
            3 => {
                let mut u = v.clone();
                u.sort();
                u.dedup();
                u.truncate(5);
                let r = run_query(LISTS, &format!("perm({},P).", list_text(&u)), 1000);
                let mut want: Vec<String> = perms_of(&u)
                    .iter()
                    .map(|p| format!("P={}", list_text(p)))
                    .collect();
                let mut got = sol_strings(&r);
                want.sort();
                got.sort();
                assert_eq!(got, want);
                solutions_checked += want.len();
            }
            4 => {
                let (x, y) = (rng.below(20) as i64, rng.below(20) as i64);
                let r = run_query(LISTS, &format!("max({x},{y},Z)."), 10);
                assert_eq!(
                    sol_strings(&r),
                    vec![format!("Z={}", x.max(y))],
                    "cut must commit to the first clause (exactly one answer)"
                );
                let z = rng.below(6) as i64;
                let r = run_query(LISTS, &format!("absent({z},{l})."), 10);
                assert_eq!(
                    r.sols.len(),
                    usize::from(!v.contains(&z)),
                    "negation as failure"
                );
                let r = run_query(LISTS, &format!("len({l},N)."), 10);
                assert_eq!(sol_strings(&r), vec![format!("N={}", v.len())]);
                solutions_checked += 3;
            }
            _ => {
                // select enumerates every way to remove one element
                let r = run_query(LISTS, &format!("select(X,{l},R)."), 1000);
                let want: Vec<String> = (0..v.len())
                    .map(|i| {
                        let mut rest = v.clone();
                        let x = rest.remove(i);
                        format!("X={x}, R={}", list_text(&rest))
                    })
                    .collect();
                assert_eq!(sol_strings(&r), want);
                solutions_checked += want.len();
            }
        }
        programs += 1;
    }
    println!(
        "  {programs} random program/query pairs agree with brute-force oracles ({solutions_checked} answers compared, order included where Prolog fixes it)"
    );
}

// ---------------------------------------------------------------- 4. miniKanren (microKanren core)

struct SNode {
    v: usize,
    t: T,
    next: KSub,
}
type KSub = Option<Rc<SNode>>;
#[derive(Clone)]
struct State {
    sub: KSub,
    next: usize,
}
enum Stream {
    Nil,
    Cons(State, Box<Stream>),
    Delay(Box<dyn FnOnce() -> Stream>),
}
type Goal = Rc<dyn Fn(State) -> Stream>;
fn kwalk(t: &T, s: &KSub) -> T {
    let mut t = t.clone();
    loop {
        let V(v) = &t else { return t };
        let v = *v;
        let mut cur = s;
        let mut found = None;
        while let Some(n) = cur {
            if n.v == v {
                found = Some(n.t.clone());
                break;
            }
            cur = &n.next;
        }
        match found {
            Some(x) => t = x,
            None => return t,
        }
    }
}
fn kocc(v: usize, t: &T, s: &KSub) -> bool {
    match kwalk(t, s) {
        V(w) => w == v,
        F(_, a) => a.iter().any(|x| kocc(v, x, s)),
    }
}
fn kunify(a: &T, b: &T, s: &KSub) -> Option<KSub> {
    let (a, b) = (kwalk(a, s), kwalk(b, s));
    match (a, b) {
        (V(x), V(y)) if x == y => Some(s.clone()),
        (V(x), t) | (t, V(x)) => {
            if kocc(x, &t, s) {
                None
            } else {
                Some(Some(Rc::new(SNode {
                    v: x,
                    t,
                    next: s.clone(),
                })))
            }
        }
        (F(f, xs), F(g, ys)) => {
            if f != g || xs.len() != ys.len() {
                return None;
            }
            let mut cur = s.clone();
            for (p, q) in xs.iter().zip(&ys) {
                cur = kunify(p, q, &cur)?;
            }
            Some(cur)
        }
    }
}
fn mplus(a: Stream, b: Stream) -> Stream {
    match a {
        Stream::Nil => b,
        Stream::Delay(f) => Stream::Delay(Box::new(move || mplus(b, f()))), // swap: this is the interleaving
        Stream::Cons(s, r) => Stream::Cons(s, Box::new(mplus(*r, b))),
    }
}
fn bind(a: Stream, g: Goal) -> Stream {
    match a {
        Stream::Nil => Stream::Nil,
        Stream::Delay(f) => Stream::Delay(Box::new(move || bind(f(), g))),
        Stream::Cons(s, r) => mplus(g(s), bind(*r, g)),
    }
}
fn eq(a: T, b: T) -> Goal {
    Rc::new(move |st: State| match kunify(&a, &b, &st.sub) {
        Some(s) => Stream::Cons(
            State {
                sub: s,
                next: st.next,
            },
            Box::new(Stream::Nil),
        ),
        None => Stream::Nil,
    })
}
fn conj(a: Goal, b: Goal) -> Goal {
    Rc::new(move |st| bind(a(st), b.clone()))
}
fn disj(a: Goal, b: Goal) -> Goal {
    Rc::new(move |st| mplus(a(st.clone()), b(st)))
}
fn all(gs: Vec<Goal>) -> Goal {
    gs.into_iter().rev().reduce(|acc, g| conj(g, acc)).unwrap()
}
fn fresh(f: impl Fn(T) -> Goal + 'static) -> Goal {
    Rc::new(move |st: State| {
        let v = V(st.next);
        f(v)(State {
            sub: st.sub,
            next: st.next + 1,
        })
    })
}
fn fresh2(f: impl Fn(T, T) -> Goal + 'static) -> Goal {
    let f = Rc::new(f);
    fresh(move |a| {
        let f = f.clone();
        fresh(move |b| f(a.clone(), b))
    })
}
fn fresh3(f: impl Fn(T, T, T) -> Goal + 'static) -> Goal {
    let f = Rc::new(f);
    fresh(move |a| {
        let f = f.clone();
        fresh2(move |b, c| f(a.clone(), b, c))
    })
}
/// Pause a recursive relation so that building the goal does not recurse forever ("snooze" in microKanren).
fn delay(f: impl Fn() -> Goal + 'static) -> Goal {
    Rc::new(move |st| {
        let g = f();
        Stream::Delay(Box::new(move || g(st)))
    })
}
fn walk_star(t: &T, s: &KSub) -> T {
    match kwalk(t, s) {
        F(f, a) => F(f, a.iter().map(|x| walk_star(x, s)).collect()),
        v => v,
    }
}
fn reify(q: &T, s: &KSub) -> T {
    let t = walk_star(q, s);
    let mut vs = vec![];
    vars_of(&t, &mut vs);
    let m: Sub = vs
        .iter()
        .enumerate()
        .map(|(i, v)| (*v, atom(&format!("_.{i}"))))
        .collect();
    subst1(&t, &m)
}
fn run(n: usize, f: impl Fn(T) -> Goal) -> Vec<T> {
    let q = V(0);
    let mut st = f(q.clone())(State { sub: None, next: 1 });
    let mut out = vec![];
    let mut pulls = 0u64;
    while out.len() < n {
        pulls += 1;
        assert!(pulls < 20_000_000, "search budget exceeded");
        match st {
            Stream::Nil => break,
            Stream::Delay(f) => st = f(),
            Stream::Cons(s, r) => {
                out.push(reify(&q, &s.sub));
                st = *r;
            }
        }
    }
    out
}
fn appendo(l: T, s: T, out: T) -> Goal {
    delay(move || {
        let (l1, s1, o1) = (l.clone(), s.clone(), out.clone());
        let base = conj(eq(l.clone(), nil()), eq(s.clone(), out.clone()));
        let step = fresh3(move |a, d, r| {
            all(vec![
                eq(l1.clone(), cons(a.clone(), d.clone())),
                eq(o1.clone(), cons(a, r.clone())),
                appendo(d, s1.clone(), r),
            ])
        });
        disj(base, step)
    })
}
fn z() -> T {
    atom("z")
}
fn sc(n: T) -> T {
    app("s", vec![n])
}
fn peano(n: usize) -> T {
    (0..n).fold(z(), |t, _| sc(t))
}
fn from_peano(t: &T) -> Option<usize> {
    match t {
        F(f, a) if f == "z" && a.is_empty() => Some(0),
        F(f, a) if f == "s" && a.len() == 1 => from_peano(&a[0]).map(|n| n + 1),
        _ => None,
    }
}
fn addo(n: T, m: T, k: T) -> Goal {
    delay(move || {
        let (n1, m1, k1) = (n.clone(), m.clone(), k.clone());
        let base = conj(eq(n.clone(), z()), eq(m.clone(), k.clone()));
        let step = fresh2(move |n2, k2| {
            all(vec![
                eq(n1.clone(), sc(n2.clone())),
                eq(k1.clone(), sc(k2.clone())),
                addo(n2, m1.clone(), k2),
            ])
        });
        disj(base, step)
    })
}
/// n * m = k. The `m = 0` case is its own clause so that running it backwards with k known terminates.
fn mulo(n: T, m: T, k: T) -> Goal {
    delay(move || {
        let (n1, m1, k1) = (n.clone(), m.clone(), k.clone());
        let (n2, m2, k2) = (n.clone(), m.clone(), k.clone());
        let c1 = conj(eq(n.clone(), z()), eq(k.clone(), z()));
        let c2 = fresh(move |n_| {
            all(vec![
                eq(n1.clone(), sc(n_)),
                eq(m1.clone(), z()),
                eq(k1.clone(), z()),
            ])
        });
        let c3 = fresh3(move |n_, m_, p| {
            all(vec![
                eq(n2.clone(), sc(n_.clone())),
                eq(m2.clone(), sc(m_)),
                addo(m2.clone(), p.clone(), k2.clone()),
                mulo(n_, m2.clone(), p),
            ])
        });
        disj(c1, disj(c2, c3))
    })
}
/// Relational evaluator for expressions over x: x | one | two | plus(a,b) | times(a,b); values are Peano numerals.
fn evalo(e: T, x: T, v: T) -> Goal {
    delay(move || {
        let (e1, x1, v1) = (e.clone(), x.clone(), v.clone());
        let (e2, x2, v2) = (e.clone(), x.clone(), v.clone());
        let leaf = disj(
            conj(eq(e.clone(), atom("x")), eq(v.clone(), x.clone())),
            disj(
                conj(eq(e.clone(), atom("one")), eq(v.clone(), peano(1))),
                conj(eq(e.clone(), atom("two")), eq(v.clone(), peano(2))),
            ),
        );
        let plus = fresh2(move |a, b| {
            let (x1, v1, e1) = (x1.clone(), v1.clone(), e1.clone());
            fresh2(move |va, vb| {
                all(vec![
                    eq(e1.clone(), app("plus", vec![a.clone(), b.clone()])),
                    evalo(a.clone(), x1.clone(), va.clone()),
                    evalo(b.clone(), x1.clone(), vb.clone()),
                    addo(va, vb, v1.clone()),
                ])
            })
        });
        let times = fresh2(move |a, b| {
            let (x2, v2, e2) = (x2.clone(), v2.clone(), e2.clone());
            fresh2(move |va, vb| {
                all(vec![
                    eq(e2.clone(), app("times", vec![a.clone(), b.clone()])),
                    evalo(a.clone(), x2.clone(), va.clone()),
                    evalo(b.clone(), x2.clone(), vb.clone()),
                    mulo(va, vb, v2.clone()),
                ])
            })
        });
        disj(leaf, disj(plus, times))
    })
}
fn eval_expr(t: &T, x: usize) -> usize {
    match t {
        F(f, a) if f == "x" && a.is_empty() => x,
        F(f, a) if f == "one" && a.is_empty() => 1,
        F(f, a) if f == "two" && a.is_empty() => 2,
        F(f, a) if f == "plus" => eval_expr(&a[0], x) + eval_expr(&a[1], x),
        F(f, a) if f == "times" => eval_expr(&a[0], x) * eval_expr(&a[1], x),
        other => panic!("not an expression: {}", show(other)),
    }
}
fn kanren_section(rng: &mut Rng) {
    // appendo forwards and backwards
    let fwd = run(5, |q| {
        appendo(list(vec![atom("a"), atom("b")]), list(vec![atom("c")]), q)
    });
    println!(
        "  appendo([a,b],[c],Q)        forwards : {}",
        fwd.iter().map(show).collect::<Vec<_>>().join(" ")
    );
    assert_eq!(fwd.iter().map(show).collect::<Vec<_>>(), ["[a,b,c]"]);
    let back = run(100, |q| {
        fresh2(move |x, y| {
            conj(
                eq(q.clone(), list(vec![x.clone(), y.clone()])),
                appendo(x, y, list(vec![atom("a"), atom("b"), atom("c")])),
            )
        })
    });
    println!(
        "  appendo(X,Y,[a,b,c])        backwards: {}",
        back.iter().map(show).collect::<Vec<_>>().join(" ")
    );
    assert_eq!(back.len(), 4);
    let mid = run(10, |q| {
        appendo(
            q,
            list(vec![atom("c")]),
            list(vec![atom("a"), atom("b"), atom("c")]),
        )
    });
    assert_eq!(mid.iter().map(show).collect::<Vec<_>>(), ["[a,b]"]);
    let open = run(3, |q| {
        appendo(list(vec![atom("a")]), q, list(vec![atom("a"), atom("b")]))
    });
    assert_eq!(open.iter().map(show).collect::<Vec<_>>(), ["[b]"]);
    // cross-check against the Prolog engine on random lists
    let mut agree = 0;
    for _ in 0..200 {
        let n = rng.below(6) as usize;
        let v: Vec<i64> = (0..n).map(|_| rng.below(4) as i64).collect();
        let l = list(v.iter().map(|x| num(*x)).collect());
        let k = run(1000, |q| {
            fresh2({
                let l = l.clone();
                move |x, y| {
                    conj(
                        eq(q.clone(), cons(x.clone(), y.clone())),
                        appendo(x, y, l.clone()),
                    )
                }
            })
        });
        let p = run_query(LISTS, &format!("append(X,Y,{}).", list_text(&v)), 1000);
        let mut a: Vec<String> = k.iter().map(show).collect();
        let mut b: Vec<String> = p
            .sols
            .iter()
            .map(|s| format!("{}", show(&cons(s[0].1.clone(), s[1].1.clone()))))
            .collect();
        a.sort();
        b.sort();
        assert_eq!(
            a, b,
            "miniKanren and the Prolog engine must agree on append"
        );
        agree += 1;
    }
    println!("  miniKanren appendo agrees with the Prolog engine on {agree} random lists");
    // relational arithmetic
    let sums = run(100, |q| {
        fresh2(move |x, y| {
            conj(
                eq(q.clone(), list(vec![x.clone(), y.clone()])),
                addo(x, y, peano(5)),
            )
        })
    });
    assert_eq!(sums.len(), 6);
    let prod = run(1, |q| mulo(peano(3), peano(4), q));
    assert_eq!(from_peano(&prod[0]), Some(12));
    let mut factors: Vec<(usize, usize)> = run(100, |q| {
        fresh2(move |x, y| {
            conj(
                eq(q.clone(), list(vec![x.clone(), y.clone()])),
                mulo(x, y, peano(12)),
            )
        })
    })
    .iter()
    .map(|t| {
        let F(_, a) = t else { panic!() };
        (
            from_peano(&a[0]).unwrap(),
            from_peano(&cons_tail(t)).unwrap(),
        )
    })
    .collect();
    factors.sort();
    println!(
        "  3 * 4 = {}; running mulo backwards, X * Y = 12 has solutions {factors:?}",
        from_peano(&prod[0]).unwrap()
    );
    assert_eq!(
        factors,
        vec![(1, 12), (2, 6), (3, 4), (4, 3), (6, 2), (12, 1)]
    );
    // program synthesis: find e with f(1)=3, f(2)=5, f(3)=7 by running the evaluator backwards
    let found = run(1, |q| {
        all(vec![
            evalo(q.clone(), peano(1), peano(3)),
            evalo(q.clone(), peano(2), peano(5)),
            evalo(q.clone(), peano(3), peano(7)),
        ])
    });
    let e = &found[0];
    println!(
        "  synthesised from examples f(1)=3, f(2)=5, f(3)=7: {}",
        show(e)
    );
    for x in 1..=3 {
        assert_eq!(eval_expr(e, x), 2 * x + 1);
    }
    assert_eq!(
        eval_expr(e, 10),
        21,
        "the synthesised program generalises: it is 2x+1"
    );
}
fn cons_tail(t: &T) -> T {
    // [X,Y] encoded as '.'(X,'.'(Y,[])) : the second element
    let F(_, a) = t else { panic!() };
    let F(_, b) = &a[1] else { panic!() };
    b[0].clone()
}

// ---------------------------------------------------------------- 5. Datalog

type Rel = BTreeSet<Vec<String>>;
struct Datalog {
    rules: Vec<Clause>,
    facts: HashMap<String, Rel>,
}
fn dl_key(t: &T) -> String {
    let F(f, a) = t else {
        panic!("datalog atom expected")
    };
    format!("{f}/{}", a.len())
}
fn dl_args(t: &T) -> &[T] {
    let F(_, a) = t else { panic!() };
    a
}
impl Datalog {
    fn parse(src: &str) -> Datalog {
        let mut d = Datalog {
            rules: vec![],
            facts: HashMap::new(),
        };
        for (c, _) in parse_program(src) {
            if c.body.is_empty() {
                let tuple: Vec<String> = dl_args(&c.head).iter().map(|a| show(a)).collect();
                d.facts.entry(dl_key(&c.head)).or_default().insert(tuple);
            } else {
                d.rules.push(c);
            }
        }
        d
    }
}
/// Join the body atoms left to right; atom `delta_at` (if any) reads from `delta`, the others from `full`.
fn dl_join(
    body: &[T],
    i: usize,
    bind: &mut HashMap<usize, String>,
    full: &HashMap<String, Rel>,
    delta: &HashMap<String, Rel>,
    delta_at: Option<usize>,
    work: &mut u64,
    emit: &mut dyn FnMut(&HashMap<usize, String>),
) {
    if i == body.len() {
        emit(bind);
        return;
    }
    let src = if delta_at == Some(i) { delta } else { full };
    let Some(rel) = src.get(&dl_key(&body[i])) else {
        return;
    };
    for tuple in rel {
        *work += 1;
        let mut added = vec![];
        let mut ok = true;
        for (a, c) in dl_args(&body[i]).iter().zip(tuple) {
            match a {
                V(v) => match bind.get(v) {
                    Some(b) if b != c => {
                        ok = false;
                        break;
                    }
                    Some(_) => {}
                    None => {
                        bind.insert(*v, c.clone());
                        added.push(*v);
                    }
                },
                other => {
                    if show(other) != *c {
                        ok = false;
                        break;
                    }
                }
            }
        }
        if ok {
            dl_join(body, i + 1, bind, full, delta, delta_at, work, emit);
        }
        for v in added {
            bind.remove(&v);
        }
    }
}
fn dl_head(head: &T, bind: &HashMap<usize, String>) -> Vec<String> {
    dl_args(head)
        .iter()
        .map(|a| {
            if let V(v) = a {
                bind[v].clone()
            } else {
                show(a)
            }
        })
        .collect()
}
/// Returns (relations, iterations, join work).
fn dl_eval(d: &Datalog, semi_naive: bool) -> (HashMap<String, Rel>, usize, u64) {
    let mut full = d.facts.clone();
    let mut delta = d.facts.clone();
    let (mut iters, mut work) = (0, 0u64);
    loop {
        iters += 1;
        let mut derived: HashMap<String, Rel> = HashMap::new();
        for r in &d.rules {
            let positions: Vec<Option<usize>> = if semi_naive {
                (0..r.body.len()).map(Some).collect()
            } else {
                vec![None]
            };
            for pos in positions {
                let mut emit = |b: &HashMap<usize, String>| {
                    derived
                        .entry(dl_key(&r.head))
                        .or_default()
                        .insert(dl_head(&r.head, b));
                };
                dl_join(
                    &r.body,
                    0,
                    &mut HashMap::new(),
                    &full,
                    &delta,
                    pos,
                    &mut work,
                    &mut emit,
                );
            }
        }
        let mut new_delta: HashMap<String, Rel> = HashMap::new();
        for (k, rel) in derived {
            for t in rel {
                if full.entry(k.clone()).or_default().insert(t.clone()) {
                    new_delta.entry(k.clone()).or_default().insert(t);
                }
            }
        }
        if new_delta.is_empty() {
            return (full, iters, work);
        }
        delta = new_delta;
    }
}
fn check_datalog(rng: &mut Rng) {
    let tc = "path(X,Y) :- edge(X,Y).\npath(X,Y) :- path(X,Z), edge(Z,Y).\n";
    let (mut cyclic, mut naive_work, mut semi_work) = (0, 0u64, 0u64);
    for _ in 0..300 {
        let n = 3 + rng.below(6) as usize;
        let mut reach = vec![vec![false; n]; n];
        let mut text = String::new();
        for i in 0..n {
            for j in 0..n {
                if rng.chance(22) {
                    reach[i][j] = true;
                    text += &format!("edge(n{i},n{j}).\n");
                }
            }
        }
        for k in 0..n {
            for i in 0..n {
                for j in 0..n {
                    reach[i][j] |= reach[i][k] && reach[k][j]; // Floyd-Warshall closure
                }
            }
        }
        if (0..n).any(|i| reach[i][i]) {
            cyclic += 1;
        }
        let d = Datalog::parse(&format!("{text}{tc}"));
        let (a, _, wn) = dl_eval(&d, false);
        let (b, _, ws) = dl_eval(&d, true);
        naive_work += wn;
        semi_work += ws;
        let reach = &reach; // a shared borrow is Copy, so the inner closures can each capture it
        let want: Rel = (0..n)
            .flat_map(|i| {
                (0..n)
                    .filter(move |&j| reach[i][j])
                    .map(move |j| vec![format!("n{i}"), format!("n{j}")])
            })
            .collect();
        assert_eq!(
            a.get("path/2").cloned().unwrap_or_default(),
            want,
            "naive Datalog = transitive closure"
        );
        assert_eq!(
            b.get("path/2").cloned().unwrap_or_default(),
            want,
            "semi-naive Datalog = transitive closure"
        );
    }
    println!(
        "  300 random graphs ({cyclic} with cycles): naive and semi-naive both equal Floyd-Warshall; join work naive {naive_work} vs semi-naive {semi_work}"
    );
    // chain scaling: the point of semi-naive evaluation
    for n in [20usize, 40, 80] {
        let text: String = (0..n - 1)
            .map(|i| format!("edge(n{i},n{}).\n", i + 1))
            .collect::<String>()
            + tc;
        let d = Datalog::parse(&text);
        let (a, ia, wa) = dl_eval(&d, false);
        let (b, ib, wb) = dl_eval(&d, true);
        assert_eq!(a["path/2"], b["path/2"]);
        println!(
            "    chain of {n:>2}: {} path facts; naive {ia} rounds / work {wa}; semi-naive {ib} rounds / work {wb}",
            a["path/2"].len()
        );
    }
    // contrast: top-down SLD on a cyclic graph never finishes, bottom-up does
    let cyc =
        "edge(a,b).\nedge(b,a).\npath(X,Y) :- edge(X,Y).\npath(X,Y) :- edge(X,Z), path(Z,Y).\n";
    let r = run_query(cyc, "path(a,Y).", 50);
    println!(
        "  SLD on the cycle a<->b: depth limit hit = {} (it enumerated {} answers and would continue forever); Datalog derives {} facts and stops",
        r.exceeded,
        r.sols.len(),
        dl_eval(&Datalog::parse(cyc), true).0["path/2"].len()
    );
    assert!(r.exceeded || r.sols.len() == 50);
    assert_eq!(dl_eval(&Datalog::parse(cyc), true).0["path/2"].len(), 4);
}

fn real_main() {
    let mut rng = Rng(0x0826_C0DE_0000_0001);
    println!("== 1. unification ==");
    let cases = [
        ("f(X,g(Y))", "f(a,Z)", true),
        ("f(X,X)", "f(a,b)", false),
        ("X", "f(X)", false),
        ("g(X,f(X))", "g(f(Y),Z)", true),
    ];
    for (a, b, unifiable) in cases {
        let (ta, tb) = parse_pair(a, b);
        let r = mgu_uf(&ta, &tb);
        assert_eq!(r.is_some(), unifiable, "{a} =? {b}");
        println!(
            "  {a} =? {b}  ->  {}",
            match &r {
                Some(s) => {
                    let mut v: Vec<String> = s
                        .iter()
                        .map(|(k, t)| format!("_{k}:={}", show(t)))
                        .collect();
                    v.sort();
                    v.join(", ")
                }
                None => "no unifier".into(),
            }
        );
    }
    let (x, fx) = parse_pair("X", "f(X)");
    assert!(mgu_robinson(&x, &fx).is_none(), "occurs check");
    exponential_family();
    check_unification(&mut rng);

    println!("\n== 2. term rewriting and Knuth-Bendix completion ==");
    check_group_completion(&mut rng);

    println!("\n== 3. Prolog engine ==");
    let r = run_query(LISTS, "append(X,Y,[1,2]).", 10);
    println!("  append(X,Y,[1,2]): {}", sol_strings(&r).join(" | "));
    let r = run_query(LISTS, "max(3,9,Z), perm([a,b,c],P).", 3);
    println!(
        "  max(3,9,Z), perm([a,b,c],P) first 3: {}",
        sol_strings(&r).join(" | ")
    );
    check_prolog(&mut rng);

    println!("\n== 4. miniKanren ==");
    kanren_section(&mut rng);

    println!("\n== 5. Datalog ==");
    check_datalog(&mut rng);
    println!("\nall assertions passed");
}
fn main() {
    // deep recursion in the SLD engine and the unifier: run on a thread with a big stack
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(real_main)
        .unwrap()
        .join()
        .unwrap();
}
