//! RECIPE 854 (database): query compilation: data-centric produce/consume codegen (closures, register bytecode, Rust source) vs Volcano and vectorized
//! LEVEL: advanced
//! Use in: query engines, rule / filter / expression engines, any interpreter that evaluates the same user-defined logic over many rows.   Run: cargo run --release --example query_compilation_data_centric_codegen
#![allow(dead_code)]
//!
//! `volcano_vectorized_push` (recipe 211) compares execution models on ONE hard-coded query. Here the plan is DATA (random at
//! run time), so a "compiled" engine really has to be a COMPILER: plan in, program out.
//! PRODUCE / CONSUME (Neumann, HyPer): every operator implements two things. `produce` asks the operator to start generating
//! tuples; `consume` is what its parent does with ONE tuple that is already in registers. The compiler walks the plan once:
//! a filter produces from its child and, in the consume step, wraps the parent's code in an `if`; a hash join first produces its
//! BUILD child with a consumer that inserts into a hash table (that ends a PIPELINE: a "pipeline breaker"), then produces its
//! PROBE child with a consumer that probes and runs the parent's code inside the match loop. Operator boundaries disappear from
//! the generated program: one tight loop per pipeline, and a tuple stays in registers from the scan to the next breaker.
//! There is no JIT in std, so the same walk targets THREE backends:
//!  (a) CLOSURE COMPILATION (`closure_*`): plan -> one tree of boxed closures. Decisions (which operator, which column) are made
//!      once, at compile time. What REMAINS is an indirect call per closure per tuple; `col <op> const` is fused into one
//!      closure (a "superinstruction") and the saving is measured.
//!  (b) REGISTER BYTECODE (`bytecode_*`): plan -> a flat `Vec<Insn>` with jumps, run by one `match` loop. `continue` of the
//!      innermost loop is a backward jump; AND / OR become conditional jumps.
//!  (c) RUST SOURCE (`codegen_*`): plan -> the text a native compiler would receive. It is printed and structurally checked
//!      (balanced scopes, every register declared in an enclosing scope before use, hash tables built before they are probed,
//!      one loop per scan and per probe). It is NOT compiled by this program.
//! Baselines: VOLCANO (`next()` per tuple per operator, expression tree walked per tuple) and VECTORIZED (`next_batch`, one
//! dispatch per operator / expression node per BATCH). An independent ORACLE (materialising, nested-loop join, unoptimised
//! expressions) defines the right answer.
//! EXPRESSIONS AND SQL NULL: values are `Option<i64>`, logic is three-valued. Division by zero yields NULL here (real SQL raises
//! an error, which also restricts reordering: an engine must not let a moved predicate raise an error the query would not).
//!  * CONSTANT FOLDING: evaluate constant subtrees once. Only NULL-safe identities: `x AND FALSE -> FALSE`, `x OR TRUE -> TRUE`,
//!    `x AND TRUE -> x`, `x OR FALSE -> x`, `NOT NOT x -> x`. `x * 0 -> 0` is WRONG (NULL * 0 is NULL); the unsound rule is
//!    available as an injected bug and the differential test must catch it.
//!  * PREDICATE ORDER: a WHERE clause keeps a row only if the predicate is TRUE, so every conjunct can reject on FALSE *or* NULL
//!    (a value-context AND may only short-circuit on FALSE). Conjuncts are ordered by rank = cost / (1 - selectivity), with
//!    selectivity estimated on a sample: cheap and very selective tests first.
//! COUNTING: `indirect` = calls through a vtable or boxed closure; `switch` = interpreter `match` dispatches (expression node or
//! bytecode instruction). Both are counted by a `const COUNT: bool` instantiation, so the timed runs carry no counters. They
//! measure the DESIGN, not what LLVM or the branch predictor does with it. Generated Rust has neither, by construction.
//! COMMON USES: HyPer / Umbra (data-centric compilation), PostgreSQL's LLVM JIT for expressions, Spark whole-stage codegen
//! (emits Java source, the shape of backend (c)), SQLite's VDBE (bytecode, backend (b)), closure compilation in many rule
//! engines; vectorized: DuckDB, ClickHouse, Velox.
//! Read: https://www.vldb.org/pvldb/vol4/p539-neumann.pdf (Neumann, "Efficiently Compiling Efficient Query Plans for Modern
//! Hardware") and https://www.vldb.org/pvldb/vol11/p2209-kersten.pdf (Kersten et al., compiled vs vectorized).
//!
//! TRY NEXT:
//!   - [S] Disable the `col <op> const` superinstruction and predict the change in indirect calls per tuple before running.
//!   - [S] Reverse the rank order in `order_conjuncts` and measure `predicate evaluations`; when would the two orders tie?
//!   - [M] Add column pruning: let `consume` request the slots it needs so scans load only those. Which backend gains most?
//!   - [M] Add a LEFT OUTER JOIN to all engines; in produce/consume, where does the "no match" code go in each backend?
//!   - [A] PROJECT: compile the text of backend (c) with `rustc` from a build script, load it as a `cdylib` (see the ffi recipes) and find the table size at which compile time pays off.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::hint::black_box;
use std::rc::Rc;
use std::time::Instant;

type V = Option<i64>; // NULL is None; booleans are Some(0) / Some(1)
const TRUE: V = Some(1);
const FALSE: V = Some(0);

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for _ in 0..4 {
            r.next();
        }
        r
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
        self.next() % n
    }
    fn pm(&mut self, p: u64) -> bool {
        self.below(1000) < p
    }
}

// ---------------------------------------------------------------- data, expressions, plans

struct Table {
    cols: Vec<Vec<V>>, // columnar
    rows: usize,
}
type Db = Vec<Rc<Table>>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Col(usize),
    Lit(V),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    IsNull(Box<Expr>),
}

fn and3(a: V, b: V) -> V {
    match (a, b) {
        (Some(0), _) | (_, Some(0)) => FALSE,
        (Some(_), Some(_)) => TRUE,
        _ => None,
    }
}
fn or3(a: V, b: V) -> V {
    match (a, b) {
        (Some(x), _) if x != 0 => TRUE,
        (_, Some(y)) if y != 0 => TRUE,
        (Some(_), Some(_)) => FALSE,
        _ => None,
    }
}
fn not3(a: V) -> V {
    a.map(|x| (x == 0) as i64)
}
#[inline]
fn arith(a: V, b: V, f: impl Fn(i64, i64) -> V) -> V {
    match (a, b) {
        (Some(x), Some(y)) => f(x, y),
        _ => None,
    }
}
fn f_add(x: i64, y: i64) -> V {
    Some(x.wrapping_add(y))
}
fn f_sub(x: i64, y: i64) -> V {
    Some(x.wrapping_sub(y))
}
fn f_mul(x: i64, y: i64) -> V {
    Some(x.wrapping_mul(y))
}
fn f_mod(x: i64, y: i64) -> V {
    x.checked_rem(y) // None (NULL) for y == 0
}
fn apply(op: BinOp, a: V, b: V) -> V {
    match op {
        BinOp::Add => arith(a, b, f_add),
        BinOp::Sub => arith(a, b, f_sub),
        BinOp::Mul => arith(a, b, f_mul),
        BinOp::Mod => arith(a, b, f_mod),
        BinOp::Eq => arith(a, b, |x, y| Some((x == y) as i64)),
        BinOp::Ne => arith(a, b, |x, y| Some((x != y) as i64)),
        BinOp::Lt => arith(a, b, |x, y| Some((x < y) as i64)),
        BinOp::Le => arith(a, b, |x, y| Some((x <= y) as i64)),
        BinOp::Gt => arith(a, b, |x, y| Some((x > y) as i64)),
        BinOp::Ge => arith(a, b, |x, y| Some((x >= y) as i64)),
        BinOp::And => and3(a, b),
        BinOp::Or => or3(a, b),
    }
}
fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Bin(op, Box::new(a), Box::new(b))
}
/// The reference semantics: no short-circuit, no folding. Used by the oracle and by the folder (on constants only).
fn eval_ref(e: &Expr, row: &[V]) -> V {
    match e {
        Expr::Col(i) => row[*i],
        Expr::Lit(v) => *v,
        Expr::Bin(op, a, b) => apply(*op, eval_ref(a, row), eval_ref(b, row)),
        Expr::Not(a) => not3(eval_ref(a, row)),
        Expr::IsNull(a) => Some(eval_ref(a, row).is_none() as i64),
    }
}

#[derive(Clone, Debug)]
enum Plan {
    Scan(usize),
    Filter(Box<Plan>, Expr),
    Project(Box<Plan>, Vec<Expr>),
    /// Inner equi-join. Output columns = probe columns followed by build columns. NULL keys never match.
    Join {
        build: Box<Plan>,
        probe: Box<Plan>,
        build_key: usize,
        probe_key: usize,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum AggFn {
    CountStar,
    Count,
    Sum,
    Min,
    Max,
}
#[derive(Clone, Debug)]
struct Agg {
    group: Vec<usize>,
    aggs: Vec<(AggFn, Expr)>,
}
#[derive(Clone, Debug)]
struct Query {
    plan: Plan,
    agg: Option<Agg>,
}

fn width(p: &Plan, db: &Db) -> usize {
    match p {
        Plan::Scan(t) => db[*t].cols.len(),
        Plan::Filter(c, _) => width(c, db),
        Plan::Project(_, es) => es.len(),
        Plan::Join { build, probe, .. } => width(build, db) + width(probe, db),
    }
}
fn count_ops(p: &Plan) -> (usize, usize) {
    // (scans, joins)
    match p {
        Plan::Scan(_) => (1, 0),
        Plan::Filter(c, _) | Plan::Project(c, _) => count_ops(c),
        Plan::Join { build, probe, .. } => {
            let (a, b) = (count_ops(build), count_ops(probe));
            (a.0 + b.0, a.1 + b.1 + 1)
        }
    }
}
fn conjuncts(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::Bin(BinOp::And, a, b) => {
            conjuncts(a, out);
            conjuncts(b, out);
        }
        other => out.push(other.clone()),
    }
}
fn split(e: &Expr) -> Vec<Expr> {
    let mut v = vec![];
    conjuncts(e, &mut v);
    v
}

// ---------------------------------------------------------------- aggregation (shared by every engine except the oracle)

#[derive(Clone, Copy, Default)]
struct Acc {
    n: i64,
    v: V,
}
struct AggState {
    fns: Vec<AggFn>,
    grouped: bool,
    groups: HashMap<Vec<V>, Vec<Acc>>, // NULL is an ordinary group key (SQL GROUP BY puts NULLs together)
}
impl AggState {
    fn new(a: &Agg) -> AggState {
        AggState {
            fns: a.aggs.iter().map(|(f, _)| *f).collect(),
            grouped: !a.group.is_empty(),
            groups: HashMap::new(),
        }
    }
    #[inline]
    fn update(&mut self, key: &[V], args: &[V]) {
        if !self.groups.contains_key(key) {
            self.groups
                .insert(key.to_vec(), vec![Acc::default(); self.fns.len()]);
        }
        let accs = self.groups.get_mut(key).unwrap();
        for ((acc, f), &a) in accs.iter_mut().zip(&self.fns).zip(args) {
            match (f, a) {
                (AggFn::CountStar, _) => acc.n += 1,
                (_, None) => {} // every other aggregate ignores NULL inputs
                (AggFn::Count, Some(_)) => acc.n += 1,
                (AggFn::Sum, Some(x)) => acc.v = Some(acc.v.unwrap_or(0).wrapping_add(x)),
                (AggFn::Min, Some(x)) => acc.v = Some(acc.v.map_or(x, |m| m.min(x))),
                (AggFn::Max, Some(x)) => acc.v = Some(acc.v.map_or(x, |m| m.max(x))),
            }
        }
    }
    fn finish(mut self) -> Vec<Vec<V>> {
        if !self.grouped && self.groups.is_empty() {
            self.groups
                .insert(vec![], vec![Acc::default(); self.fns.len()]); // global aggregate of nothing: one row
        }
        let fns = self.fns;
        self.groups
            .into_iter()
            .map(|(mut k, accs)| {
                k.extend(accs.iter().zip(&fns).map(|(a, f)| match f {
                    AggFn::CountStar | AggFn::Count => Some(a.n),
                    _ => a.v, // SUM / MIN / MAX of no non-NULL value is NULL
                }));
                k
            })
            .collect()
    }
}
fn sorted(mut rows: Vec<Vec<V>>) -> Vec<Vec<V>> {
    rows.sort();
    rows
}

// ---------------------------------------------------------------- oracle: as naive and as independent as possible

fn oracle_rows(p: &Plan, db: &Db, limit_spine: usize) -> Vec<Vec<V>> {
    match p {
        Plan::Scan(t) => {
            let t = &db[*t];
            (0..t.rows.min(limit_spine))
                .map(|i| t.cols.iter().map(|c| c[i]).collect())
                .collect()
        }
        Plan::Filter(c, e) => oracle_rows(c, db, limit_spine)
            .into_iter()
            .filter(|r| eval_ref(e, r) == TRUE)
            .collect(),
        Plan::Project(c, es) => oracle_rows(c, db, limit_spine)
            .iter()
            .map(|r| es.iter().map(|e| eval_ref(e, r)).collect())
            .collect(),
        Plan::Join {
            build,
            probe,
            build_key,
            probe_key,
        } => {
            let b = oracle_rows(build, db, usize::MAX);
            let mut out = vec![];
            for pr in oracle_rows(probe, db, limit_spine) {
                for br in &b {
                    if pr[*probe_key].is_some() && pr[*probe_key] == br[*build_key] {
                        let mut r = pr.clone();
                        r.extend_from_slice(br);
                        out.push(r);
                    }
                }
            }
            out
        }
    }
}
fn oracle(q: &Query, db: &Db) -> Vec<Vec<V>> {
    let rows = oracle_rows(&q.plan, db, usize::MAX);
    let Some(a) = &q.agg else { return sorted(rows) };
    let mut groups: BTreeMap<Vec<V>, Vec<Vec<V>>> = BTreeMap::new();
    if a.group.is_empty() {
        groups.insert(vec![], vec![]);
    }
    for r in rows {
        groups
            .entry(a.group.iter().map(|&g| r[g]).collect())
            .or_default()
            .push(r);
    }
    sorted(
        groups
            .into_iter()
            .map(|(mut k, members)| {
                for (f, e) in &a.aggs {
                    let vals: Vec<i64> = members.iter().filter_map(|r| eval_ref(e, r)).collect();
                    k.push(match f {
                        AggFn::CountStar => Some(members.len() as i64),
                        AggFn::Count => Some(vals.len() as i64),
                        AggFn::Sum => vals.iter().copied().reduce(i64::wrapping_add),
                        AggFn::Min => vals.iter().copied().min(),
                        AggFn::Max => vals.iter().copied().max(),
                    });
                }
                k
            })
            .collect(),
    )
}

// ---------------------------------------------------------------- optimiser: constant folding + predicate ordering

fn fold(e: &Expr, unsound: bool) -> Expr {
    use Expr::*;
    match e {
        Col(_) | Lit(_) => e.clone(),
        Not(a) => match fold(a, unsound) {
            Lit(v) => Lit(not3(v)),
            Not(inner) => *inner,
            a => Not(Box::new(a)),
        },
        IsNull(a) => match fold(a, unsound) {
            Lit(v) => Lit(Some(v.is_none() as i64)),
            a => IsNull(Box::new(a)),
        },
        Bin(op, a, b) => {
            let (a, b) = (fold(a, unsound), fold(b, unsound));
            match (*op, &a, &b) {
                (_, Lit(x), Lit(y)) => Lit(apply(*op, *x, *y)),
                (BinOp::And, Lit(Some(0)), _) | (BinOp::And, _, Lit(Some(0))) => Lit(FALSE),
                (BinOp::Or, Lit(Some(1)), _) | (BinOp::Or, _, Lit(Some(1))) => Lit(TRUE),
                // sound only because the generator (like a type checker) puts boolean operands under AND / OR
                (BinOp::And, Lit(Some(1)), _) | (BinOp::Or, Lit(Some(0)), _) => b,
                (BinOp::And, _, Lit(Some(1))) | (BinOp::Or, _, Lit(Some(0))) => a,
                (BinOp::Mul, Lit(Some(0)), _) | (BinOp::Mul, _, Lit(Some(0))) if unsound => {
                    Lit(Some(0))
                } // BUG: NULL*0 is NULL
                _ => bin(*op, a, b),
            }
        }
    }
}
fn cost(e: &Expr) -> usize {
    match e {
        Expr::Col(_) | Expr::Lit(_) => 1,
        Expr::Bin(_, a, b) => 1 + cost(a) + cost(b),
        Expr::Not(a) | Expr::IsNull(a) => 1 + cost(a),
    }
}
/// Order conjuncts by cost / (fraction rejected), measured on `sample`. For independent predicates this minimises expected cost.
fn order_conjuncts(mut cs: Vec<Expr>, sample: &[Vec<V>]) -> Vec<Expr> {
    let rank = |c: &Expr| {
        let pass = sample.iter().filter(|r| eval_ref(c, r) == TRUE).count();
        let sel = (pass as f64 + 0.5) / (sample.len() as f64 + 1.0);
        cost(c) as f64 / (1.0 - sel).max(1e-6)
    };
    cs.sort_by(|a, b| rank(a).total_cmp(&rank(b)));
    cs
}
#[derive(Clone, Copy)]
struct OptFlags {
    reorder: bool,
    unsound_fold: bool,
}
const OPT: OptFlags = OptFlags {
    reorder: true,
    unsound_fold: false,
};

fn optimize_plan(p: &Plan, db: &Db, o: OptFlags) -> Plan {
    match p {
        Plan::Scan(t) => Plan::Scan(*t),
        Plan::Project(c, es) => Plan::Project(
            Box::new(optimize_plan(c, db, o)),
            es.iter().map(|e| fold(e, o.unsound_fold)).collect(),
        ),
        Plan::Filter(c, e) => {
            let child = optimize_plan(c, db, o);
            // fold each conjunct, drop the ones that became TRUE, then flatten again (folding can expose new ANDs)
            let mut cs: Vec<Expr> = split(&fold(e, o.unsound_fold))
                .into_iter()
                .filter(|c| *c != Expr::Lit(TRUE))
                .collect();
            if o.reorder && cs.len() > 1 {
                let sample = oracle_rows(&child, db, 128); // sample the probe spine only; build sides stay complete
                cs = order_conjuncts(cs, &sample);
            }
            match cs.into_iter().reduce(|a, b| bin(BinOp::And, a, b)) {
                None => child,
                Some(pred) => Plan::Filter(Box::new(child), pred),
            }
        }
        Plan::Join {
            build,
            probe,
            build_key,
            probe_key,
        } => Plan::Join {
            build: Box::new(optimize_plan(build, db, o)),
            probe: Box::new(optimize_plan(probe, db, o)),
            build_key: *build_key,
            probe_key: *probe_key,
        },
    }
}
fn optimize(q: &Query, db: &Db, o: OptFlags) -> Query {
    let agg = q.agg.as_ref().map(|a| Agg {
        group: a.group.clone(),
        aggs: a
            .aggs
            .iter()
            .map(|(f, e)| (*f, fold(e, o.unsound_fold)))
            .collect(),
    });
    Query {
        plan: optimize_plan(&q.plan, db, o),
        agg,
    }
}

// ---------------------------------------------------------------- dispatch counters

#[derive(Default, Clone, Copy)]
struct Stats {
    indirect: u64,
    switch: u64,
    pred_evals: u64, // conjuncts evaluated
}
#[inline(always)]
fn ind<const C: bool>(s: &mut Stats) {
    if C {
        s.indirect += 1;
    }
}
#[inline(always)]
fn sw<const C: bool>(s: &mut Stats) {
    if C {
        s.switch += 1;
    }
}

// ---------------------------------------------------------------- engine 1: Volcano

fn interp<const C: bool>(e: &Expr, row: &[V], s: &mut Stats) -> V {
    sw::<C>(s);
    match e {
        Expr::Col(i) => row[*i],
        Expr::Lit(v) => *v,
        Expr::Bin(BinOp::And, a, b) => {
            let x = interp::<C>(a, row, s);
            if x == FALSE {
                FALSE
            } else {
                and3(x, interp::<C>(b, row, s))
            }
        }
        Expr::Bin(BinOp::Or, a, b) => {
            let x = interp::<C>(a, row, s);
            if x == TRUE {
                TRUE
            } else {
                or3(x, interp::<C>(b, row, s))
            }
        }
        Expr::Bin(op, a, b) => {
            let x = interp::<C>(a, row, s);
            let y = interp::<C>(b, row, s);
            apply(*op, x, y)
        }
        Expr::Not(a) => not3(interp::<C>(a, row, s)),
        Expr::IsNull(a) => Some(interp::<C>(a, row, s).is_none() as i64),
    }
}

trait Operator<const C: bool> {
    fn next(&mut self, s: &mut Stats) -> Option<Vec<V>>;
}
struct VScan {
    t: Rc<Table>,
    i: usize,
}
impl<const C: bool> Operator<C> for VScan {
    fn next(&mut self, _s: &mut Stats) -> Option<Vec<V>> {
        if self.i == self.t.rows {
            return None;
        }
        self.i += 1;
        Some(self.t.cols.iter().map(|c| c[self.i - 1]).collect())
    }
}
struct VFilter<const C: bool> {
    child: Box<dyn Operator<C>>,
    preds: Vec<Expr>,
}
impl<const C: bool> Operator<C> for VFilter<C> {
    fn next(&mut self, s: &mut Stats) -> Option<Vec<V>> {
        'rows: loop {
            ind::<C>(s);
            let row = self.child.next(s)?;
            for p in &self.preds {
                if C {
                    s.pred_evals += 1;
                }
                if interp::<C>(p, &row, s) != TRUE {
                    continue 'rows;
                }
            }
            return Some(row);
        }
    }
}
struct VProject<const C: bool> {
    child: Box<dyn Operator<C>>,
    exprs: Vec<Expr>,
}
impl<const C: bool> Operator<C> for VProject<C> {
    fn next(&mut self, s: &mut Stats) -> Option<Vec<V>> {
        ind::<C>(s);
        let row = self.child.next(s)?;
        Some(self.exprs.iter().map(|e| interp::<C>(e, &row, s)).collect())
    }
}
struct VJoin<const C: bool> {
    build: Box<dyn Operator<C>>,
    probe: Box<dyn Operator<C>>,
    build_key: usize,
    probe_key: usize,
    table: Option<HashMap<i64, Vec<Vec<V>>>>,
    current: Option<(Vec<V>, usize)>, // probe row, next match
}
impl<const C: bool> Operator<C> for VJoin<C> {
    fn next(&mut self, s: &mut Stats) -> Option<Vec<V>> {
        if self.table.is_none() {
            let mut m: HashMap<i64, Vec<Vec<V>>> = HashMap::new();
            loop {
                ind::<C>(s);
                let Some(r) = self.build.next(s) else { break };
                if let Some(k) = r[self.build_key] {
                    m.entry(k).or_default().push(r);
                }
            }
            self.table = Some(m);
        }
        loop {
            if let Some((pr, i)) = &mut self.current {
                let hit = pr[self.probe_key]
                    .and_then(|k| self.table.as_ref().unwrap().get(&k))
                    .and_then(|v| v.get(*i));
                if let Some(br) = hit {
                    *i += 1;
                    let mut out = pr.clone();
                    out.extend_from_slice(br);
                    return Some(out);
                }
            }
            ind::<C>(s);
            self.current = Some((self.probe.next(s)?, 0));
        }
    }
}
fn volcano_tree<const C: bool>(p: &Plan, db: &Db) -> Box<dyn Operator<C>> {
    match p {
        Plan::Scan(t) => Box::new(VScan {
            t: db[*t].clone(),
            i: 0,
        }),
        Plan::Filter(c, e) => Box::new(VFilter {
            child: volcano_tree::<C>(c, db),
            preds: split(e),
        }),
        Plan::Project(c, es) => Box::new(VProject {
            child: volcano_tree::<C>(c, db),
            exprs: es.clone(),
        }),
        Plan::Join {
            build,
            probe,
            build_key,
            probe_key,
        } => Box::new(VJoin {
            build: volcano_tree::<C>(build, db),
            probe: volcano_tree::<C>(probe, db),
            build_key: *build_key,
            probe_key: *probe_key,
            table: None,
            current: None,
        }),
    }
}
fn run_volcano<const C: bool>(q: &Query, db: &Db, s: &mut Stats) -> Vec<Vec<V>> {
    let mut root = volcano_tree::<C>(&q.plan, db);
    let mut out = vec![];
    let mut agg = q.agg.as_ref().map(AggState::new);
    loop {
        ind::<C>(s);
        let Some(row) = root.next(s) else { break };
        match (&mut agg, &q.agg) {
            (Some(st), Some(a)) => {
                let key: Vec<V> = a.group.iter().map(|&g| row[g]).collect();
                let args: Vec<V> = a
                    .aggs
                    .iter()
                    .map(|(_, e)| interp::<C>(e, &row, s))
                    .collect();
                st.update(&key, &args);
            }
            _ => out.push(row),
        }
    }
    agg.map_or(out, AggState::finish)
}

// ---------------------------------------------------------------- engine 2: vectorized

struct Batch {
    cols: Vec<Vec<V>>,
    len: usize,
}
/// Evaluate one expression node for all selected rows at once: ONE dispatch, then a tight loop.
/// AND / OR evaluate both sides here (no per-row branch); the FILTER short-circuits by shrinking the selection vector.
fn eval_vec<const C: bool>(e: &Expr, b: &Batch, sel: &[u32], s: &mut Stats) -> Vec<V> {
    sw::<C>(s);
    macro_rules! zip {
        ($x:expr, $y:expr, $f:expr) => {
            $x.iter().zip(&$y).map(|(&a, &c)| $f(a, c)).collect()
        };
    }
    match e {
        Expr::Col(i) => {
            let c = &b.cols[*i];
            sel.iter().map(|&r| c[r as usize]).collect()
        }
        Expr::Lit(v) => vec![*v; sel.len()],
        Expr::Not(a) => eval_vec::<C>(a, b, sel, s).into_iter().map(not3).collect(),
        Expr::IsNull(a) => eval_vec::<C>(a, b, sel, s)
            .into_iter()
            .map(|v| Some(v.is_none() as i64))
            .collect(),
        Expr::Bin(op, l, r) => {
            let x = eval_vec::<C>(l, b, sel, s);
            let y = eval_vec::<C>(r, b, sel, s);
            // the `match op` sits OUTSIDE the loops: each arm is its own monomorphised kernel
            match op {
                BinOp::Add => zip!(x, y, |a, c| arith(a, c, f_add)),
                BinOp::Sub => zip!(x, y, |a, c| arith(a, c, f_sub)),
                BinOp::Mul => zip!(x, y, |a, c| arith(a, c, f_mul)),
                BinOp::Mod => zip!(x, y, |a, c| arith(a, c, f_mod)),
                BinOp::Eq => zip!(x, y, |a, c| apply(BinOp::Eq, a, c)),
                BinOp::Ne => zip!(x, y, |a, c| apply(BinOp::Ne, a, c)),
                BinOp::Lt => zip!(x, y, |a, c| apply(BinOp::Lt, a, c)),
                BinOp::Le => zip!(x, y, |a, c| apply(BinOp::Le, a, c)),
                BinOp::Gt => zip!(x, y, |a, c| apply(BinOp::Gt, a, c)),
                BinOp::Ge => zip!(x, y, |a, c| apply(BinOp::Ge, a, c)),
                BinOp::And => zip!(x, y, and3),
                BinOp::Or => zip!(x, y, or3),
            }
        }
    }
}
trait VecOp<const C: bool> {
    /// A returned batch may be empty; `None` is end of stream.
    fn next_batch(&mut self, s: &mut Stats) -> Option<Batch>;
}
struct BScan {
    t: Rc<Table>,
    i: usize,
    size: usize,
}
impl<const C: bool> VecOp<C> for BScan {
    fn next_batch(&mut self, _s: &mut Stats) -> Option<Batch> {
        if self.i == self.t.rows {
            return None;
        }
        let end = (self.i + self.size).min(self.t.rows);
        let b = Batch {
            cols: self
                .t
                .cols
                .iter()
                .map(|c| c[self.i..end].to_vec())
                .collect(),
            len: end - self.i,
        };
        self.i = end;
        Some(b)
    }
}
struct BFilter<const C: bool> {
    child: Box<dyn VecOp<C>>,
    preds: Vec<Expr>,
}
impl<const C: bool> VecOp<C> for BFilter<C> {
    fn next_batch(&mut self, s: &mut Stats) -> Option<Batch> {
        ind::<C>(s);
        let b = self.child.next_batch(s)?;
        let mut sel: Vec<u32> = (0..b.len as u32).collect(); // SELECTION VECTOR
        for p in &self.preds {
            if sel.is_empty() {
                break;
            }
            if C {
                s.pred_evals += sel.len() as u64;
            }
            let v = eval_vec::<C>(p, &b, &sel, s);
            let mut k = 0;
            for i in 0..sel.len() {
                if v[i] == TRUE {
                    sel[k] = sel[i];
                    k += 1;
                }
            }
            sel.truncate(k);
        }
        Some(Batch {
            cols: b
                .cols
                .iter()
                .map(|c| sel.iter().map(|&r| c[r as usize]).collect())
                .collect(),
            len: sel.len(),
        })
    }
}
struct BProject<const C: bool> {
    child: Box<dyn VecOp<C>>,
    exprs: Vec<Expr>,
}
impl<const C: bool> VecOp<C> for BProject<C> {
    fn next_batch(&mut self, s: &mut Stats) -> Option<Batch> {
        ind::<C>(s);
        let b = self.child.next_batch(s)?;
        let sel: Vec<u32> = (0..b.len as u32).collect();
        Some(Batch {
            cols: self
                .exprs
                .iter()
                .map(|e| eval_vec::<C>(e, &b, &sel, s))
                .collect(),
            len: b.len,
        })
    }
}
struct BuildSide {
    cols: Vec<Vec<V>>,
    map: HashMap<i64, Vec<u32>>,
}
struct BJoin<const C: bool> {
    build: Box<dyn VecOp<C>>,
    probe: Box<dyn VecOp<C>>,
    build_key: usize,
    probe_key: usize,
    build_width: usize,
    side: Option<BuildSide>,
}
impl<const C: bool> VecOp<C> for BJoin<C> {
    fn next_batch(&mut self, s: &mut Stats) -> Option<Batch> {
        if self.side.is_none() {
            let mut side = BuildSide {
                cols: vec![vec![]; self.build_width],
                map: HashMap::new(),
            };
            let mut n = 0u32;
            loop {
                ind::<C>(s);
                let Some(b) = self.build.next_batch(s) else {
                    break;
                };
                for r in 0..b.len {
                    if let Some(k) = b.cols[self.build_key][r] {
                        side.map.entry(k).or_default().push(n);
                        for (dst, src) in side.cols.iter_mut().zip(&b.cols) {
                            dst.push(src[r]);
                        }
                        n += 1;
                    }
                }
            }
            self.side = Some(side);
        }
        ind::<C>(s);
        let p = self.probe.next_batch(s)?;
        let side = self.side.as_ref().unwrap();
        let (mut pi, mut bi) = (vec![], vec![]); // matching row pairs, then gather column by column
        for r in 0..p.len {
            if let Some(m) = p.cols[self.probe_key][r].and_then(|k| side.map.get(&k)) {
                for &x in m {
                    pi.push(r);
                    bi.push(x as usize);
                }
            }
        }
        let mut cols: Vec<Vec<V>> = p
            .cols
            .iter()
            .map(|c| pi.iter().map(|&r| c[r]).collect())
            .collect();
        cols.extend(
            side.cols
                .iter()
                .map(|c| bi.iter().map(|&r| c[r]).collect::<Vec<V>>()),
        );
        Some(Batch {
            cols,
            len: pi.len(),
        })
    }
}
fn vector_tree<const C: bool>(p: &Plan, db: &Db, size: usize) -> Box<dyn VecOp<C>> {
    match p {
        Plan::Scan(t) => Box::new(BScan {
            t: db[*t].clone(),
            i: 0,
            size,
        }),
        Plan::Filter(c, e) => Box::new(BFilter {
            child: vector_tree::<C>(c, db, size),
            preds: split(e),
        }),
        Plan::Project(c, es) => Box::new(BProject {
            child: vector_tree::<C>(c, db, size),
            exprs: es.clone(),
        }),
        Plan::Join {
            build,
            probe,
            build_key,
            probe_key,
        } => Box::new(BJoin {
            build_width: width(build, db),
            build: vector_tree::<C>(build, db, size),
            probe: vector_tree::<C>(probe, db, size),
            build_key: *build_key,
            probe_key: *probe_key,
            side: None,
        }),
    }
}
fn run_vectorized<const C: bool>(q: &Query, db: &Db, size: usize, s: &mut Stats) -> Vec<Vec<V>> {
    let mut root = vector_tree::<C>(&q.plan, db, size);
    let mut out = vec![];
    let mut agg = q.agg.as_ref().map(AggState::new);
    let (mut key, mut args) = (vec![], vec![]);
    loop {
        ind::<C>(s);
        let Some(b) = root.next_batch(s) else { break };
        match (&mut agg, &q.agg) {
            (Some(st), Some(a)) => {
                let sel: Vec<u32> = (0..b.len as u32).collect();
                let arg_cols: Vec<Vec<V>> = a
                    .aggs
                    .iter()
                    .map(|(_, e)| eval_vec::<C>(e, &b, &sel, s))
                    .collect();
                for r in 0..b.len {
                    key.clear();
                    key.extend(a.group.iter().map(|&g| b.cols[g][r]));
                    args.clear();
                    args.extend(arg_cols.iter().map(|c| c[r]));
                    st.update(&key, &args);
                }
            }
            _ => out.extend((0..b.len).map(|r| b.cols.iter().map(|c| c[r]).collect::<Vec<V>>())),
        }
    }
    agg.map_or(out, AggState::finish)
}

// ---------------------------------------------------------------- engine 3a: closure compilation (produce / consume)

type ExprFn = Box<dyn Fn(&[V], &mut Stats) -> V>;
/// "Code that consumes one tuple": the tuple is already in the register file.
type Sink = Box<dyn FnMut(&mut [V], &mut Stats)>;
type Pipeline = Box<dyn FnMut(&mut [V], &mut Stats)>;
/// Given the register slots that hold the producer's output columns, build the consumer code.
type Consumer<'a> = Box<dyn FnOnce(&mut ClosureGen, &[usize]) -> Sink + 'a>;

struct ClosureGen {
    slots: usize,
    pipelines: Vec<Pipeline>, // in execution order: a build pipeline is pushed before the pipeline that probes it
    fuse: bool,
}
impl ClosureGen {
    fn alloc(&mut self, n: usize) -> Vec<usize> {
        let first = self.slots;
        self.slots += n;
        (first..self.slots).collect()
    }
}
fn bin_closure<const C: bool>(a: ExprFn, b: ExprFn, f: impl Fn(V, V) -> V + 'static) -> ExprFn {
    Box::new(move |r, s| {
        ind::<C>(s);
        let x = a(r, s);
        let y = b(r, s);
        f(x, y)
    })
}
fn col_const<const C: bool>(slot: usize, k: i64, f: impl Fn(i64, i64) -> V + 'static) -> ExprFn {
    Box::new(move |r, s| {
        ind::<C>(s);
        match r[slot] {
            Some(x) => f(x, k),
            None => None,
        }
    })
}
fn compile_expr<const C: bool>(e: &Expr, map: &[usize], fuse: bool) -> ExprFn {
    match e {
        Expr::Col(i) => {
            let slot = map[*i];
            Box::new(move |r, s| {
                ind::<C>(s);
                r[slot]
            })
        }
        Expr::Lit(v) => {
            let v = *v;
            Box::new(move |_, s| {
                ind::<C>(s);
                v
            })
        }
        Expr::Not(a) => {
            let a = compile_expr::<C>(a, map, fuse);
            Box::new(move |r, s| {
                ind::<C>(s);
                not3(a(r, s))
            })
        }
        Expr::IsNull(a) => {
            let a = compile_expr::<C>(a, map, fuse);
            Box::new(move |r, s| {
                ind::<C>(s);
                Some(a(r, s).is_none() as i64)
            })
        }
        Expr::Bin(op, l, r) => {
            if let (true, Expr::Col(i), Expr::Lit(Some(k))) = (fuse, &**l, &**r) {
                let (slot, k) = (map[*i], *k);
                match op {
                    BinOp::Lt => return col_const::<C>(slot, k, |x, y| Some((x < y) as i64)),
                    BinOp::Gt => return col_const::<C>(slot, k, |x, y| Some((x > y) as i64)),
                    BinOp::Le => return col_const::<C>(slot, k, |x, y| Some((x <= y) as i64)),
                    BinOp::Ge => return col_const::<C>(slot, k, |x, y| Some((x >= y) as i64)),
                    BinOp::Eq => return col_const::<C>(slot, k, |x, y| Some((x == y) as i64)),
                    BinOp::Ne => return col_const::<C>(slot, k, |x, y| Some((x != y) as i64)),
                    BinOp::Add => return col_const::<C>(slot, k, f_add),
                    BinOp::Sub => return col_const::<C>(slot, k, f_sub),
                    BinOp::Mul => return col_const::<C>(slot, k, f_mul),
                    BinOp::Mod => return col_const::<C>(slot, k, f_mod),
                    BinOp::And | BinOp::Or => {}
                }
            }
            let a = compile_expr::<C>(l, map, fuse);
            let b = compile_expr::<C>(r, map, fuse);
            // `match op` runs HERE, once. Each arm creates a closure type with the operator baked in.
            match op {
                BinOp::And => Box::new(move |r, s| {
                    ind::<C>(s);
                    let x = a(r, s);
                    if x == FALSE { FALSE } else { and3(x, b(r, s)) }
                }),
                BinOp::Or => Box::new(move |r, s| {
                    ind::<C>(s);
                    let x = a(r, s);
                    if x == TRUE { TRUE } else { or3(x, b(r, s)) }
                }),
                BinOp::Add => bin_closure::<C>(a, b, |x, y| arith(x, y, f_add)),
                BinOp::Sub => bin_closure::<C>(a, b, |x, y| arith(x, y, f_sub)),
                BinOp::Mul => bin_closure::<C>(a, b, |x, y| arith(x, y, f_mul)),
                BinOp::Mod => bin_closure::<C>(a, b, |x, y| arith(x, y, f_mod)),
                BinOp::Eq => bin_closure::<C>(a, b, |x, y| apply(BinOp::Eq, x, y)),
                BinOp::Ne => bin_closure::<C>(a, b, |x, y| apply(BinOp::Ne, x, y)),
                BinOp::Lt => bin_closure::<C>(a, b, |x, y| apply(BinOp::Lt, x, y)),
                BinOp::Le => bin_closure::<C>(a, b, |x, y| apply(BinOp::Le, x, y)),
                BinOp::Gt => bin_closure::<C>(a, b, |x, y| apply(BinOp::Gt, x, y)),
                BinOp::Ge => bin_closure::<C>(a, b, |x, y| apply(BinOp::Ge, x, y)),
            }
        }
    }
}
fn closure_produce<'a, const C: bool>(
    p: &'a Plan,
    db: &'a Db,
    g: &mut ClosureGen,
    consume: Consumer<'a>,
) {
    match p {
        Plan::Scan(t) => {
            let table = db[*t].clone();
            let slots = g.alloc(table.cols.len());
            let mut sink = consume(g, &slots);
            g.pipelines.push(Box::new(move |regs, s| {
                for i in 0..table.rows {
                    for (c, &slot) in table.cols.iter().zip(&slots) {
                        regs[slot] = c[i];
                    }
                    ind::<C>(s);
                    sink(regs, s);
                }
            }));
        }
        Plan::Filter(child, e) => closure_produce::<C>(
            child,
            db,
            g,
            Box::new(move |g, slots| {
                let preds: Vec<ExprFn> = split(e)
                    .iter()
                    .map(|c| compile_expr::<C>(c, slots, g.fuse))
                    .collect();
                let mut parent = consume(g, slots);
                Box::new(move |regs, s| {
                    for p in &preds {
                        if C {
                            s.pred_evals += 1;
                        }
                        if p(regs, s) != TRUE {
                            return; // "continue" of the scan loop
                        }
                    }
                    ind::<C>(s);
                    parent(regs, s);
                })
            }),
        ),
        Plan::Project(child, es) => closure_produce::<C>(
            child,
            db,
            g,
            Box::new(move |g, slots| {
                let fns: Vec<ExprFn> = es
                    .iter()
                    .map(|e| compile_expr::<C>(e, slots, g.fuse))
                    .collect();
                let out = g.alloc(es.len()); // fresh slots: an output may not clobber an input another output still reads
                let mut parent = consume(g, &out);
                Box::new(move |regs, s| {
                    for (f, &o) in fns.iter().zip(&out) {
                        regs[o] = f(regs, s);
                    }
                    ind::<C>(s);
                    parent(regs, s);
                })
            }),
        ),
        Plan::Join {
            build,
            probe,
            build_key,
            probe_key,
        } => {
            // flat row storage: key -> row numbers, row n lives at rows[n*w .. (n+1)*w]
            let table: Rc<RefCell<(HashMap<i64, Vec<u32>>, Vec<V>)>> = Rc::default();
            let build_width = width(build, db);
            let t2 = table.clone();
            let key_col = *build_key;
            closure_produce::<C>(
                build,
                db,
                g,
                Box::new(move |_, slots| {
                    let slots = slots.to_vec();
                    Box::new(move |regs, _| {
                        let Some(k) = regs[slots[key_col]] else {
                            return;
                        };
                        let (map, rows) = &mut *t2.borrow_mut();
                        map.entry(k)
                            .or_default()
                            .push((rows.len() / slots.len()) as u32);
                        rows.extend(slots.iter().map(|&i| regs[i]));
                    })
                }),
            ); // <- the build PIPELINE is complete and registered
            let probe_col = *probe_key;
            closure_produce::<C>(
                probe,
                db,
                g,
                Box::new(move |g, probe_slots| {
                    let key = probe_slots[probe_col];
                    let mine = g.alloc(build_width);
                    let mut all = probe_slots.to_vec();
                    all.extend(&mine);
                    let mut parent = consume(g, &all);
                    Box::new(move |regs, s| {
                        let Some(k) = regs[key] else { return };
                        let t = table.borrow();
                        let Some(matches) = t.0.get(&k) else { return };
                        for &m in matches {
                            let at = m as usize * build_width;
                            for (i, &slot) in mine.iter().enumerate() {
                                regs[slot] = t.1[at + i];
                            }
                            ind::<C>(s);
                            parent(regs, s);
                        }
                    })
                }),
            );
        }
    }
}
struct ClosureQuery {
    gen_: ClosureGen,
    agg: Option<Rc<RefCell<AggState>>>,
    out: Rc<RefCell<Vec<Vec<V>>>>,
}
fn closure_compile<const C: bool>(q: &Query, db: &Db, fuse: bool) -> ClosureQuery {
    let mut g = ClosureGen {
        slots: 0,
        pipelines: vec![],
        fuse,
    };
    let agg = q
        .agg
        .as_ref()
        .map(|a| Rc::new(RefCell::new(AggState::new(a))));
    let out: Rc<RefCell<Vec<Vec<V>>>> = Rc::default();
    let (agg2, out2) = (agg.clone(), out.clone());
    closure_produce::<C>(
        &q.plan,
        db,
        &mut g,
        Box::new(move |g, slots| {
            let slots = slots.to_vec();
            match (&q.agg, agg2) {
                (Some(a), Some(state)) => {
                    let keys: Vec<usize> = a.group.iter().map(|&k| slots[k]).collect();
                    let args: Vec<ExprFn> = a
                        .aggs
                        .iter()
                        .map(|(_, e)| compile_expr::<C>(e, &slots, g.fuse))
                        .collect();
                    let (mut kb, mut ab) = (vec![], vec![]);
                    Box::new(move |regs, s| {
                        kb.clear();
                        kb.extend(keys.iter().map(|&k| regs[k]));
                        ab.clear();
                        ab.extend(args.iter().map(|f| f(regs, s)));
                        state.borrow_mut().update(&kb, &ab);
                    })
                }
                _ => Box::new(move |regs, _| {
                    out2.borrow_mut()
                        .push(slots.iter().map(|&i| regs[i]).collect())
                }),
            }
        }),
    );
    ClosureQuery { gen_: g, agg, out }
}
fn run_closure<const C: bool>(q: &Query, db: &Db, fuse: bool, s: &mut Stats) -> Vec<Vec<V>> {
    let mut c = closure_compile::<C>(q, db, fuse);
    let mut regs = vec![None; c.gen_.slots];
    for p in &mut c.gen_.pipelines {
        ind::<C>(s);
        p(&mut regs, s);
    }
    c.gen_.pipelines.clear(); // drops the sinks and with them their Rc clones of the result state
    match c.agg {
        Some(a) => Rc::try_unwrap(a)
            .ok()
            .expect("sole owner")
            .into_inner()
            .finish(),
        None => Rc::try_unwrap(c.out).ok().expect("sole owner").into_inner(),
    }
}

// ---------------------------------------------------------------- engine 3b: register bytecode

type Reg = u32;
#[derive(Debug, Clone)]
enum Insn {
    Rewind {
        cur: Reg,
    },
    /// Load the next row of `table` into consecutive registers, or jump to `done`.
    ScanNext {
        cur: Reg,
        table: usize,
        base: Reg,
        done: usize,
    },
    Const {
        dst: Reg,
        v: V,
    },
    Mov {
        dst: Reg,
        src: Reg,
    },
    Bin {
        op: BinOp,
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    Not {
        dst: Reg,
        a: Reg,
    },
    IsNull {
        dst: Reg,
        a: Reg,
    },
    JumpIfEq {
        r: Reg,
        v: V,
        to: usize,
    }, // value-level short circuit: AND leaves on FALSE, OR on TRUE
    SkipUnlessTrue {
        r: Reg,
        to: usize,
    }, // predicate context: FALSE and NULL both reject
    Insert {
        ht: usize,
        key: Reg,
        srcs: Vec<Reg>,
    },
    ProbeInit {
        ht: usize,
        key: Reg,
        it: usize,
    },
    ProbeNext {
        it: usize,
        ht: usize,
        base: Reg,
        done: usize,
    },
    Emit {
        srcs: Vec<Reg>,
    },
    Aggregate {
        keys: Vec<Reg>,
        args: Vec<Reg>,
    },
    Jump {
        to: usize,
    },
    Halt,
}
type BcConsumer<'a> = Box<dyn FnOnce(&mut BcGen, &[Reg], usize) + 'a>; // (output registers, `continue` label)

#[derive(Default)]
struct BcGen {
    code: Vec<Insn>,
    regs: Reg,
    cursors: Reg,
    tables: Vec<usize>, // row width of each hash table
    iters: usize,
}
impl BcGen {
    fn reg(&mut self) -> Reg {
        self.regs += 1;
        self.regs - 1
    }
    fn block(&mut self, n: usize) -> Vec<Reg> {
        (0..n).map(|_| self.reg()).collect()
    }
    fn patch_done(&mut self, at: usize, target: usize) {
        match &mut self.code[at] {
            Insn::ScanNext { done, .. } | Insn::ProbeNext { done, .. } => *done = target,
            other => panic!("not patchable: {other:?}"),
        }
    }
    fn expr(&mut self, e: &Expr, map: &[Reg]) -> Reg {
        match e {
            Expr::Col(i) => map[*i], // already in a register: no instruction at all
            Expr::Lit(v) => {
                let dst = self.reg();
                self.code.push(Insn::Const { dst, v: *v });
                dst
            }
            Expr::Not(a) => {
                let a = self.expr(a, map);
                let dst = self.reg();
                self.code.push(Insn::Not { dst, a });
                dst
            }
            Expr::IsNull(a) => {
                let a = self.expr(a, map);
                let dst = self.reg();
                self.code.push(Insn::IsNull { dst, a });
                dst
            }
            Expr::Bin(op @ (BinOp::And | BinOp::Or), a, b) => {
                let a = self.expr(a, map);
                let dst = self.reg();
                self.code.push(Insn::Mov { dst, src: a });
                let at = self.code.len();
                let short = if *op == BinOp::And { FALSE } else { TRUE };
                self.code.push(Insn::JumpIfEq {
                    r: dst,
                    v: short,
                    to: 0,
                });
                let b = self.expr(b, map);
                self.code.push(Insn::Bin {
                    op: *op,
                    dst,
                    a: dst,
                    b,
                });
                let end = self.code.len();
                self.code[at] = Insn::JumpIfEq {
                    r: dst,
                    v: short,
                    to: end,
                };
                dst
            }
            Expr::Bin(op, a, b) => {
                let a = self.expr(a, map);
                let b = self.expr(b, map);
                let dst = self.reg();
                self.code.push(Insn::Bin { op: *op, dst, a, b });
                dst
            }
        }
    }
}
/// `outer` is the label to jump to when this producer's loop is exhausted.
fn bc_produce<'a>(p: &'a Plan, db: &'a Db, g: &mut BcGen, consume: BcConsumer<'a>) {
    match p {
        Plan::Scan(t) => {
            let regs = g.block(db[*t].cols.len());
            let cur = g.cursors;
            g.cursors += 1;
            g.code.push(Insn::Rewind { cur });
            let top = g.code.len();
            g.code.push(Insn::ScanNext {
                cur,
                table: *t,
                base: regs[0],
                done: 0,
            });
            consume(g, &regs, top);
            g.code.push(Insn::Jump { to: top });
            let done = g.code.len();
            g.patch_done(top, done); // falls through to the next pipeline
        }
        Plan::Filter(child, e) => bc_produce(
            child,
            db,
            g,
            Box::new(move |g, regs, cont| {
                for c in split(e) {
                    let r = g.expr(&c, regs);
                    g.code.push(Insn::SkipUnlessTrue { r, to: cont });
                }
                consume(g, regs, cont);
            }),
        ),
        Plan::Project(child, es) => bc_produce(
            child,
            db,
            g,
            Box::new(move |g, regs, cont| {
                let out: Vec<Reg> = es.iter().map(|e| g.expr(e, regs)).collect();
                consume(g, &out, cont);
            }),
        ),
        Plan::Join {
            build,
            probe,
            build_key,
            probe_key,
        } => {
            let ht = g.tables.len();
            let bw = width(build, db);
            g.tables.push(bw);
            let (bk, pk) = (*build_key, *probe_key);
            bc_produce(
                build,
                db,
                g,
                Box::new(move |g, regs, _| {
                    g.code.push(Insn::Insert {
                        ht,
                        key: regs[bk],
                        srcs: regs.to_vec(),
                    })
                }),
            );
            bc_produce(
                probe,
                db,
                g,
                Box::new(move |g, probe_regs, outer| {
                    let it = g.iters;
                    g.iters += 1;
                    let mine = g.block(bw);
                    g.code.push(Insn::ProbeInit {
                        ht,
                        key: probe_regs[pk],
                        it,
                    });
                    let top = g.code.len();
                    g.code.push(Insn::ProbeNext {
                        it,
                        ht,
                        base: mine[0],
                        done: outer,
                    }); // exhausted: continue the OUTER loop
                    let mut all = probe_regs.to_vec();
                    all.extend(&mine);
                    consume(g, &all, top); // `continue` now means: next match
                    g.code.push(Insn::Jump { to: top });
                }),
            );
        }
    }
}
fn bc_compile(q: &Query, db: &Db) -> BcGen {
    let mut g = BcGen::default();
    bc_produce(
        &q.plan,
        db,
        &mut g,
        Box::new(move |g, regs, _| match &q.agg {
            Some(a) => {
                let args = a.aggs.iter().map(|(_, e)| g.expr(e, regs)).collect();
                g.code.push(Insn::Aggregate {
                    keys: a.group.iter().map(|&k| regs[k]).collect(),
                    args,
                });
            }
            None => g.code.push(Insn::Emit {
                srcs: regs.to_vec(),
            }),
        }),
    );
    g.code.push(Insn::Halt);
    g
}
fn bc_verify(g: &BcGen) {
    let n = g.code.len();
    for i in &g.code {
        match i {
            Insn::Jump { to } | Insn::JumpIfEq { to, .. } | Insn::SkipUnlessTrue { to, .. } => {
                assert!(*to < n)
            }
            Insn::ScanNext { done, .. } | Insn::ProbeNext { done, .. } => {
                assert!(*done > 0 && *done < n, "unpatched or wild exit label")
            }
            _ => {}
        }
    }
    assert!(matches!(g.code.last(), Some(Insn::Halt)));
}
fn bc_run<const C: bool>(g: &BcGen, q: &Query, db: &Db, s: &mut Stats) -> Vec<Vec<V>> {
    let mut r: Vec<V> = vec![None; g.regs as usize];
    let mut cur = vec![0usize; g.cursors as usize];
    let mut hts: Vec<(HashMap<i64, Vec<u32>>, Vec<V>)> =
        g.tables.iter().map(|_| Default::default()).collect();
    let mut iters: Vec<(Option<i64>, usize)> = vec![(None, 0); g.iters];
    let mut agg = q.agg.as_ref().map(AggState::new);
    let mut out = vec![];
    let (mut kb, mut ab): (Vec<V>, Vec<V>) = (vec![], vec![]);
    let mut pc = 0;
    loop {
        sw::<C>(s);
        let insn = &g.code[pc];
        pc += 1;
        match insn {
            Insn::Rewind { cur: c } => cur[*c as usize] = 0,
            Insn::ScanNext {
                cur: c,
                table,
                base,
                done,
            } => {
                let t = &db[*table];
                let i = cur[*c as usize];
                if i == t.rows {
                    pc = *done;
                } else {
                    for (k, col) in t.cols.iter().enumerate() {
                        r[*base as usize + k] = col[i];
                    }
                    cur[*c as usize] = i + 1;
                }
            }
            Insn::Const { dst, v } => r[*dst as usize] = *v,
            Insn::Mov { dst, src } => r[*dst as usize] = r[*src as usize],
            Insn::Bin { op, dst, a, b } => {
                r[*dst as usize] = apply(*op, r[*a as usize], r[*b as usize])
            }
            Insn::Not { dst, a } => r[*dst as usize] = not3(r[*a as usize]),
            Insn::IsNull { dst, a } => r[*dst as usize] = Some(r[*a as usize].is_none() as i64),
            Insn::JumpIfEq { r: x, v, to } => {
                if r[*x as usize] == *v {
                    pc = *to;
                }
            }
            Insn::SkipUnlessTrue { r: x, to } => {
                if C {
                    s.pred_evals += 1;
                }
                if r[*x as usize] != TRUE {
                    pc = *to;
                }
            }
            Insn::Insert { ht, key, srcs } => {
                if let Some(k) = r[*key as usize] {
                    let (map, rows) = &mut hts[*ht];
                    map.entry(k)
                        .or_default()
                        .push((rows.len() / srcs.len()) as u32);
                    rows.extend(srcs.iter().map(|&x| r[x as usize]));
                }
            }
            Insn::ProbeInit { key, it, .. } => iters[*it] = (r[*key as usize], 0),
            Insn::ProbeNext { it, ht, base, done } => {
                let (key, pos) = iters[*it];
                let (map, rows) = &hts[*ht];
                // the hash lookup is repeated per match to keep the iterator state a plain (key, position) pair
                match key.and_then(|k| map.get(&k)).and_then(|m| m.get(pos)) {
                    Some(&m) => {
                        let w = g.tables[*ht];
                        r[*base as usize..*base as usize + w]
                            .copy_from_slice(&rows[m as usize * w..(m as usize + 1) * w]);
                        iters[*it].1 = pos + 1;
                    }
                    None => pc = *done,
                }
            }
            Insn::Emit { srcs } => out.push(srcs.iter().map(|&x| r[x as usize]).collect()),
            Insn::Aggregate { keys, args } => {
                kb.clear();
                kb.extend(keys.iter().map(|&x| r[x as usize]));
                ab.clear();
                ab.extend(args.iter().map(|&x| r[x as usize]));
                agg.as_mut().unwrap().update(&kb, &ab);
            }
            Insn::Jump { to } => pc = *to,
            Insn::Halt => break,
        }
    }
    agg.map_or(out, AggState::finish)
}

// ---------------------------------------------------------------- engine 3c: Rust source text

type SrcConsumer<'a> = Box<dyn FnOnce(&mut SrcGen, &[String]) + 'a>;
#[derive(Default)]
struct SrcGen {
    lines: Vec<String>,
    depth: usize,
    vars: usize,
    hts: usize,
    pipelines: usize,
}
impl SrcGen {
    fn line(&mut self, s: impl AsRef<str>) {
        self.lines
            .push(format!("{}{}", "    ".repeat(self.depth), s.as_ref()));
    }
    fn open(&mut self, s: impl AsRef<str>) {
        self.line(s);
        self.depth += 1;
    }
    fn close(&mut self) {
        self.depth -= 1;
        self.line("}");
    }
    fn var(&mut self) -> String {
        self.vars += 1;
        format!("v{}", self.vars - 1)
    }
}
fn src_expr(e: &Expr, map: &[String]) -> String {
    match e {
        Expr::Col(i) => map[*i].clone(),
        Expr::Lit(None) => "None".into(),
        Expr::Lit(Some(x)) => format!("Some({x}i64)"),
        Expr::Not(a) => format!("not3({})", src_expr(a, map)),
        Expr::IsNull(a) => format!("is_null({})", src_expr(a, map)),
        Expr::Bin(op, a, b) => {
            let (x, y) = (src_expr(a, map), src_expr(b, map));
            match op {
                // the right side is a closure so that it is only evaluated when needed; rustc inlines it away
                BinOp::And => format!("and_then3({x}, || {y})"),
                BinOp::Or => format!("or_else3({x}, || {y})"),
                _ => format!(
                    "{}({x}, {y})",
                    format!("{op:?}").to_lowercase().replace("mod", "rem")
                ),
            }
        }
    }
}
fn src_produce<'a>(p: &'a Plan, db: &'a Db, g: &mut SrcGen, consume: SrcConsumer<'a>) {
    match p {
        Plan::Scan(t) => {
            let n = g.pipelines;
            g.pipelines += 1;
            g.line(format!("// pipeline {n}: scan of table {t}"));
            g.open(format!("for i{n} in 0..db[{t}].rows {{"));
            let vars: Vec<String> = (0..db[*t].cols.len()).map(|_| g.var()).collect();
            for (c, v) in vars.iter().enumerate() {
                g.line(format!("let {v} = db[{t}].cols[{c}][i{n}];"));
            }
            consume(g, &vars);
            g.close();
        }
        Plan::Filter(child, e) => src_produce(
            child,
            db,
            g,
            Box::new(move |g, vars| {
                for c in split(e) {
                    g.line(format!(
                        "if {} != Some(1i64) {{ continue; }}",
                        src_expr(&c, vars)
                    ));
                }
                consume(g, vars);
            }),
        ),
        Plan::Project(child, es) => src_produce(
            child,
            db,
            g,
            Box::new(move |g, vars| {
                let out: Vec<String> = es
                    .iter()
                    .map(|e| {
                        let rhs = src_expr(e, vars);
                        let v = g.var();
                        // the type is spelled out because `let v = None;` alone does not type-check
                        g.line(format!("let {v}: V = {rhs};"));
                        v
                    })
                    .collect();
                consume(g, &out);
            }),
        ),
        Plan::Join {
            build,
            probe,
            build_key,
            probe_key,
        } => {
            let ht = g.hts;
            g.hts += 1;
            let bw = width(build, db);
            let (bk, pk) = (*build_key, *probe_key);
            g.line(format!(
                "let mut ht{ht}: HashMap<i64, Vec<[V; {bw}]>> = HashMap::new();"
            ));
            src_produce(
                build,
                db,
                g,
                Box::new(move |g, vars| {
                    g.line(format!(
                        "if let Some(k) = {} {{ ht{ht}.entry(k).or_default().push([{}]); }}",
                        vars[bk],
                        vars.join(", ")
                    ))
                }),
            );
            src_produce(
                probe,
                db,
                g,
                Box::new(move |g, probe_vars| {
                    g.open(format!(
                        "for m{ht} in {}.and_then(|k| ht{ht}.get(&k)).into_iter().flatten() {{",
                        probe_vars[pk]
                    ));
                    let mut all = probe_vars.to_vec();
                    for c in 0..bw {
                        let v = g.var();
                        g.line(format!("let {v} = m{ht}[{c}];"));
                        all.push(v);
                    }
                    consume(g, &all);
                    g.close();
                }),
            );
        }
    }
}
fn codegen_rust(q: &Query, db: &Db) -> String {
    let mut g = SrcGen::default();
    g.open("fn query(db: &[Table], agg: &mut AggState, out: &mut Vec<Vec<V>>) {");
    src_produce(
        &q.plan,
        db,
        &mut g,
        Box::new(move |g, vars| match &q.agg {
            Some(a) => {
                let keys: Vec<String> = a.group.iter().map(|&k| vars[k].clone()).collect();
                let args: Vec<String> = a.aggs.iter().map(|(_, e)| src_expr(e, vars)).collect();
                g.line(format!(
                    "agg.update(&[{}], &[{}]);",
                    keys.join(", "),
                    args.join(", ")
                ));
            }
            None => g.line(format!("out.push(vec![{}]);", vars.join(", "))),
        }),
    );
    g.close();
    g.lines.join("\n") + "\n"
}
/// Register names (`v` followed by digits) that appear in a line of generated code.
fn idents(line: &str, prefix: &str) -> Vec<String> {
    line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|w| {
            w.strip_prefix(prefix)
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(str::to_string)
        .collect()
}
/// Structural check of the generated text with a real scope stack. Not a type check and not a substitute for rustc.
fn check_source(src: &str, q: &Query) -> Result<(), String> {
    let (scans, joins) = count_ops(&q.plan);
    let mut scopes: Vec<Vec<String>> = vec![vec![]];
    let (mut scan_loops, mut probe_loops, mut sinks) = (0, 0, 0);
    let mut filled: BTreeMap<String, bool> = BTreeMap::new();
    for (n, line) in src.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        let declared = t
            .strip_prefix("let ")
            .and_then(|r| r.split([' ', ':']).next())
            .map(|w| if w == "mut" { "" } else { w });
        for v in idents(t, "v") {
            if declared == Some(v.as_str())
                && (t.starts_with(&format!("let {v} ")) || t.starts_with(&format!("let {v}:")))
            {
                continue;
            }
            if !scopes.iter().any(|s| s.contains(&v)) {
                return Err(format!(
                    "line {n}: {v} used outside the scope that declares it"
                ));
            }
        }
        for h in idents(t, "ht") {
            if t.starts_with("let mut ") {
                filled.insert(h, false);
            } else if t.contains(".entry(") {
                *filled.get_mut(&h).ok_or("insert into undeclared table")? = true;
            } else if filled.get(&h) != Some(&true) {
                return Err(format!("line {n}: {h} probed before its build pipeline"));
            }
        }
        if let Some(v) = declared.filter(|v| v.starts_with('v')) {
            scopes.last_mut().unwrap().push(v.to_string());
        }
        scan_loops += t.starts_with("for i") as usize;
        probe_loops += t.starts_with("for m") as usize;
        sinks += (t.starts_with("agg.update") || t.starts_with("out.push")) as usize;
        for c in t.chars() {
            if c == '{' {
                scopes.push(vec![]);
            }
            if c == '}' {
                if scopes.len() == 1 {
                    return Err(format!("line {n}: unbalanced braces"));
                }
                scopes.pop();
            }
        }
    }
    let pipelines = src.matches("// pipeline ").count();
    if scopes.len() != 1 {
        return Err("unclosed scope".into());
    }
    if (scan_loops, probe_loops, pipelines, sinks) != (scans, joins, scans, 1) {
        return Err(format!(
            "shape: {scan_loops} scan loops, {probe_loops} probe loops, {pipelines} pipelines, {sinks} sinks; plan has {scans} scans, {joins} joins"
        ));
    }
    if src.contains("dyn ") || src.contains(".next()") {
        return Err(
            "generated code must not contain dynamic dispatch or iterator pulls between operators"
                .into(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- random data and queries

fn gen_table(rng: &mut Rng, rows: usize, domains: &[u64], null_pm: u64) -> Rc<Table> {
    let cols = domains
        .iter()
        .map(|&d| {
            (0..rows)
                .map(|_| {
                    if rng.pm(null_pm) {
                        None
                    } else {
                        Some(rng.below(d) as i64)
                    }
                })
                .collect()
        })
        .collect();
    Rc::new(Table { cols, rows })
}
fn gen_int(rng: &mut Rng, w: usize, depth: u32) -> Expr {
    if depth == 0 || rng.pm(450) {
        return match rng.below(10) {
            0 => Expr::Lit(None),
            1 => Expr::Lit(Some(0)),
            2 | 3 => Expr::Lit(Some(rng.below(30) as i64)),
            _ => Expr::Col(rng.below(w as u64) as usize),
        };
    }
    let op = [BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Mod][rng.below(4) as usize];
    bin(op, gen_int(rng, w, depth - 1), gen_int(rng, w, depth - 1))
}
fn gen_bool(rng: &mut Rng, w: usize, depth: u32) -> Expr {
    if depth > 0 && rng.pm(450) {
        return match rng.below(5) {
            0 => Expr::Not(Box::new(gen_bool(rng, w, depth - 1))),
            1 | 2 => bin(
                BinOp::And,
                gen_bool(rng, w, depth - 1),
                gen_bool(rng, w, depth - 1),
            ),
            _ => bin(
                BinOp::Or,
                gen_bool(rng, w, depth - 1),
                gen_bool(rng, w, depth - 1),
            ),
        };
    }
    match rng.below(12) {
        0 => Expr::IsNull(Box::new(gen_int(rng, w, 1))),
        1 => Expr::Lit([TRUE, FALSE, None][rng.below(3) as usize]),
        2..=6 => {
            let op = [
                BinOp::Eq,
                BinOp::Ne,
                BinOp::Lt,
                BinOp::Le,
                BinOp::Gt,
                BinOp::Ge,
            ][rng.below(6) as usize];
            bin(
                op,
                Expr::Col(rng.below(w as u64) as usize),
                Expr::Lit(Some(rng.below(40) as i64)),
            )
        }
        _ => {
            let op = [
                BinOp::Eq,
                BinOp::Ne,
                BinOp::Lt,
                BinOp::Le,
                BinOp::Gt,
                BinOp::Ge,
            ][rng.below(6) as usize];
            bin(op, gen_int(rng, w, 2), gen_int(rng, w, 2))
        }
    }
}
fn maybe_filter(rng: &mut Rng, p: Plan, db: &Db, pm: u64) -> Plan {
    if !rng.pm(pm) {
        return p;
    }
    let w = width(&p, db);
    let mut e = gen_bool(rng, w, 2);
    for _ in 0..rng.below(3) {
        e = bin(BinOp::And, e, gen_bool(rng, w, 1));
    }
    Plan::Filter(Box::new(p), e)
}
fn maybe_project(rng: &mut Rng, p: Plan, db: &Db, pm: u64) -> Plan {
    if !rng.pm(pm) {
        return p;
    }
    let w = width(&p, db);
    // keep column 0 first so that a join above still has a key with matches
    let mut es = vec![Expr::Col(0)];
    for _ in 0..1 + rng.below(3) {
        es.push(gen_int(rng, w, 2));
    }
    Plan::Project(Box::new(p), es)
}
fn join(build: Plan, probe: Plan, build_key: usize, probe_key: usize) -> Plan {
    Plan::Join {
        build: Box::new(build),
        probe: Box::new(probe),
        build_key,
        probe_key,
    }
}
/// Table 0 is the fact table; 1 and 2 are small dimensions. Column 0 of every table is a join key.
fn gen_case(seed: u64) -> (Db, Query) {
    let mut rng = Rng::new(seed);
    let nulls = 20 + rng.below(150);
    let fact_rows = rng.below(300) as usize; // includes empty and tiny inputs
    let fact = gen_table(&mut rng, fact_rows, &[20, 40, 40, 10], nulls);
    let d1_rows = rng.below(40) as usize;
    let d1 = gen_table(&mut rng, d1_rows, &[20, 40, 40], nulls);
    let d2_rows = rng.below(30) as usize;
    let d2 = gen_table(&mut rng, d2_rows, &[20, 40], nulls);
    let db: Db = vec![fact, d1, d2];
    let r = &mut rng;
    let mut p = maybe_filter(r, Plan::Scan(0), &db, 650);
    p = maybe_project(r, p, &db, 200);
    if r.pm(550) {
        let mut build = maybe_filter(r, Plan::Scan(1), &db, 400);
        if r.pm(150) {
            // bushy: the build side is itself a join
            build = join(Plan::Scan(2), build, 0, 0);
        }
        build = maybe_project(r, build, &db, 200);
        p = join(build, p, 0, 0);
        p = maybe_filter(r, p, &db, 350);
    }
    if r.pm(250) {
        let build = maybe_filter(r, Plan::Scan(2), &db, 400);
        let probe_key = r.below(width(&p, &db) as u64) as usize;
        p = join(build, p, 0, probe_key);
    }
    p = maybe_project(r, p, &db, 300);
    let w = width(&p, &db);
    let agg = r.pm(600).then(|| {
        let group = (0..r.below(3))
            .map(|_| r.below(w as u64) as usize)
            .collect();
        let fns = [
            AggFn::CountStar,
            AggFn::Count,
            AggFn::Sum,
            AggFn::Min,
            AggFn::Max,
        ];
        let aggs = (0..1 + r.below(3))
            .map(|_| (fns[r.below(5) as usize], gen_int(r, w, 2)))
            .collect();
        Agg { group, aggs }
    });
    (db, Query { plan: p, agg })
}

// ---------------------------------------------------------------- checks and benchmarks

fn semantics_checks() {
    let (t, f, n) = (TRUE, FALSE, None::<i64>);
    for (a, b, and, or) in [
        (t, t, t, t),
        (t, f, f, t),
        (t, n, n, t),
        (f, f, f, f),
        (f, n, f, n),
        (n, n, n, n),
    ] {
        assert_eq!(
            (and3(a, b), and3(b, a), or3(a, b), or3(b, a)),
            (and, and, or, or)
        );
    }
    assert_eq!((not3(t), not3(f), not3(n)), (f, t, n));
    assert_eq!(apply(BinOp::Eq, n, n), n, "NULL = NULL is not TRUE");
    assert_eq!(apply(BinOp::Mod, Some(7), Some(0)), n);
    // folding
    let x = Expr::Col(0);
    let times0 = bin(BinOp::Mul, x.clone(), Expr::Lit(Some(0)));
    assert_eq!(fold(&times0, false), times0, "x*0 must not fold");
    assert_eq!(fold(&times0, true), Expr::Lit(Some(0)));
    let k = bin(
        BinOp::Add,
        bin(BinOp::Mul, Expr::Lit(Some(6)), Expr::Lit(Some(7))),
        x.clone(),
    );
    assert_eq!(
        fold(&k, false),
        bin(BinOp::Add, Expr::Lit(Some(42)), x.clone())
    );
    let p = bin(BinOp::Lt, x.clone(), Expr::Lit(Some(3)));
    assert_eq!(
        fold(&bin(BinOp::And, p.clone(), Expr::Lit(f)), false),
        Expr::Lit(f)
    );
    assert_eq!(fold(&bin(BinOp::And, p.clone(), Expr::Lit(t)), false), p);
    assert_eq!(
        fold(&bin(BinOp::And, p.clone(), Expr::Lit(n)), false),
        bin(BinOp::And, p, Expr::Lit(n)),
        "x AND NULL is not NULL: it can be FALSE"
    );
}

fn differential(cases: u64) {
    let (mut with_join, mut with_agg, mut nonempty, mut folded) = (0, 0, 0, 0);
    let mut s = Stats::default();
    for seed in 1..=cases {
        let (db, q) = gen_case(seed);
        let want = oracle(&q, &db);
        let o = optimize(&q, &db, OPT);
        folded += (format!("{o:?}") != format!("{q:?}")) as u32;
        let batch = [1, 3, 64, 1024][(seed % 4) as usize];
        let bc = bc_compile(&o, &db);
        bc_verify(&bc);
        for (name, got) in [
            (
                "volcano, unoptimised plan",
                run_volcano::<false>(&q, &db, &mut s),
            ),
            ("volcano", run_volcano::<false>(&o, &db, &mut s)),
            (
                "vectorized",
                run_vectorized::<false>(&o, &db, batch, &mut s),
            ),
            ("closures", run_closure::<false>(&o, &db, true, &mut s)),
            (
                "closures, unfused",
                run_closure::<false>(&o, &db, false, &mut s),
            ),
            ("bytecode", bc_run::<false>(&bc, &o, &db, &mut s)),
            (
                "bytecode, unoptimised plan",
                bc_run::<false>(&bc_compile(&q, &db), &q, &db, &mut s),
            ),
        ] {
            assert_eq!(
                sorted(got),
                want,
                "seed {seed}: {name} differs from the oracle\n{q:#?}"
            );
        }
        for plan in [&q, &o] {
            let src = codegen_rust(plan, &db);
            check_source(&src, plan).unwrap_or_else(|e| panic!("seed {seed}: {e}\n{src}"));
        }
        with_join += (count_ops(&q.plan).1 > 0) as u32;
        with_agg += q.agg.is_some() as u32;
        nonempty += (!want.is_empty()) as u32;
    }
    println!(
        "differential: {cases} random queries ({with_join} with hash joins, {with_agg} with aggregation, {nonempty} with non-empty results, {folded} changed by the optimiser)"
    );
    println!(
        "  oracle == volcano == vectorized == closures == bytecode on every one; both generated sources pass the structural check"
    );
    assert!(
        with_join > cases as u32 / 4
            && with_agg > cases as u32 / 4
            && nonempty > cases as u32 / 2
            && folded > 0
    );
}

/// Non-vacuity: the harness must notice a wrong rewrite, and the source checker must notice broken code.
fn mutation_checks() {
    let mut caught = vec![];
    for seed in 1..=3000 {
        let (db, q) = gen_case(seed);
        let bad = optimize(
            &q,
            &db,
            OptFlags {
                reorder: true,
                unsound_fold: true,
            },
        );
        if sorted(run_closure::<false>(&bad, &db, true, &mut Stats::default())) != oracle(&q, &db) {
            caught.push(seed);
        }
    }
    assert!(
        !caught.is_empty(),
        "the unsound x*0 -> 0 fold was never detected"
    );
    println!(
        "injected unsound fold (x*0 -> 0): detected by {} of 3000 random queries, first at seed {}",
        caught.len(),
        caught[0]
    );

    let (db, q) = join_query(1000);
    let good = codegen_rust(&q, &db);
    check_source(&good, &q).unwrap();
    let probe_first = {
        let mut l: Vec<&str> = good.lines().collect();
        let ins = l.iter().position(|x| x.contains(".entry(")).unwrap();
        l.remove(ins);
        l.join("\n")
    };
    let broken = [
        good.replacen("let v0 ", "let v999 ", 1),
        good.trim_end().trim_end_matches('}').to_string(),
        probe_first,
    ];
    for (i, b) in broken.iter().enumerate() {
        assert!(
            check_source(b, &q).is_err(),
            "structural checker accepted broken source #{i}"
        );
    }
    println!(
        "structural checker rejects: an undeclared register, a missing brace, a probe of a table that was never built"
    );
}

fn bench_db(rows: usize) -> Db {
    let mut rng = Rng::new(99);
    vec![
        gen_table(&mut rng, rows, &[1000, 1000, 100, 16], 20),
        gen_table(&mut rng, 800, &[1000, 50, 8], 0),
    ]
}
/// SELECT c3, COUNT(*), SUM(c1*c2 + 3), MAX(c1) FROM t0 WHERE c1 < 900 AND c2 > 20 AND c0 % 7 <> 0 GROUP BY c3
fn filter_query(rows: usize) -> (Db, Query) {
    use BinOp::*;
    let (c, k) = (Expr::Col, |x| Expr::Lit(Some(x)));
    let pred = bin(
        And,
        bin(
            And,
            bin(Ne, bin(Mod, c(0), k(7)), k(0)),
            bin(Lt, c(1), bin(Mul, k(30), k(30))),
        ),
        bin(Gt, c(2), k(20)),
    );
    let agg = Agg {
        group: vec![3],
        aggs: vec![
            (AggFn::CountStar, k(0)),
            (AggFn::Sum, bin(Add, bin(Mul, c(1), c(2)), k(3))),
            (AggFn::Max, c(1)),
        ],
    };
    (
        bench_db(rows),
        Query {
            plan: Plan::Filter(Box::new(Plan::Scan(0)), pred),
            agg: Some(agg),
        },
    )
}
/// SELECT d.c2, COUNT(*), SUM(t.c1 + d.c1) FROM t0 t JOIN t1 d ON d.c0 = t.c0 WHERE t.c2 < 60 AND d.c1 < 40 GROUP BY d.c2
fn join_query(rows: usize) -> (Db, Query) {
    use BinOp::*;
    let (c, k) = (Expr::Col, |x| Expr::Lit(Some(x)));
    let probe = Plan::Filter(Box::new(Plan::Scan(0)), bin(Lt, c(2), k(60)));
    let build = Plan::Filter(Box::new(Plan::Scan(1)), bin(Lt, c(1), k(40)));
    let agg = Agg {
        group: vec![6],
        aggs: vec![(AggFn::CountStar, k(0)), (AggFn::Sum, bin(Add, c(1), c(5)))],
    };
    (
        bench_db(rows),
        Query {
            plan: join(build, probe, 0, 0),
            agg: Some(agg),
        },
    )
}
/// What a native compiler would make of the generated code for `filter_query`, written by hand: the baseline.
fn hand_filter(db: &Db) -> Vec<Vec<V>> {
    let t = &db[0];
    let mut acc = [(0i64, None::<i64>, None::<i64>, false); 17]; // 16 groups + NULL
    for i in 0..t.rows {
        let (c0, c1, c2) = (t.cols[0][i], t.cols[1][i], t.cols[2][i]);
        let (Some(a), Some(b), Some(c)) = (c0, c1, c2) else {
            continue;
        }; // every conjunct rejects NULL
        if a % 7 == 0 || b >= 900 || c <= 20 {
            continue;
        }
        let g = &mut acc[t.cols[3][i].map_or(16, |x| x as usize)];
        g.0 += 1;
        g.1 = Some(g.1.unwrap_or(0) + b * c + 3);
        g.2 = Some(g.2.map_or(b, |m| m.max(b)));
        g.3 = true;
    }
    acc.iter()
        .enumerate()
        .filter(|(_, g)| g.3)
        .map(|(k, g)| vec![(k < 16).then_some(k as i64), Some(g.0), g.1, g.2])
        .collect()
}

fn dispatch_table() {
    const ROWS: usize = 100_000;
    for (name, (db, q)) in [
        ("filter + aggregate", filter_query(ROWS)),
        ("hash join + aggregate", join_query(ROWS)),
    ] {
        let q = optimize(&q, &db, OPT);
        let count = |f: &dyn Fn(&mut Stats) -> Vec<Vec<V>>| {
            let mut s = Stats::default();
            black_box(f(&mut s));
            (
                s.indirect as f64 / ROWS as f64,
                s.switch as f64 / ROWS as f64,
            )
        };
        let bc = bc_compile(&q, &db);
        let rows = [
            ("volcano", count(&|s| run_volcano::<true>(&q, &db, s))),
            (
                "vectorized (1024)",
                count(&|s| run_vectorized::<true>(&q, &db, 1024, s)),
            ),
            (
                "closures, unfused",
                count(&|s| run_closure::<true>(&q, &db, false, s)),
            ),
            (
                "closures, fused col-op-const",
                count(&|s| run_closure::<true>(&q, &db, true, s)),
            ),
            (
                "register bytecode",
                count(&|s| bc_run::<true>(&bc, &q, &db, s)),
            ),
        ];
        println!(
            "\ndispatches per scanned fact row, {name}:        indirect calls   interpreter switches"
        );
        for (n, (i, s)) in rows {
            println!("  {n:<30} {i:>26.3} {s:>20.3}");
        }
        println!(
            "  {:<30} {:>26.3} {:>20.3}   (by construction, not measured)",
            "generated Rust (native)", 0.0, 0.0
        );
        let get = |k: usize| rows[k].1;
        assert!(
            get(1).0 + get(1).1 < 0.1,
            "vectorized dispatch is amortised over the batch"
        );
        assert!(get(3).0 < get(2).0, "superinstructions remove calls");
        assert!(get(0).0 + get(0).1 > 5.0 && get(4).0 == 0.0 && get(4).1 > 1.0);
    }
}

fn predicate_order_demo() {
    use BinOp::*;
    let db = bench_db(100_000);
    let (c, k) = (Expr::Col, |x| Expr::Lit(Some(x)));
    // written worst-first: an expensive test that passes nearly everything before a cheap one that rejects 97%
    let costly = bin(
        Ne,
        bin(Mod, bin(Add, bin(Mul, c(0), c(1)), c(2)), k(97)),
        k(5),
    );
    let selective = bin(Lt, c(1), k(30));
    let q = Query {
        plan: Plan::Filter(Box::new(Plan::Scan(0)), bin(And, costly, selective.clone())),
        agg: None,
    };
    let work = |q: &Query| {
        let mut s = Stats::default();
        let out = sorted(run_volcano::<true>(q, &db, &mut s));
        (out, s.pred_evals, s.switch)
    };
    let written = work(&optimize(
        &q,
        &db,
        OptFlags {
            reorder: false,
            unsound_fold: false,
        },
    ));
    let ordered = optimize(&q, &db, OPT);
    let Plan::Filter(_, e) = &ordered.plan else {
        unreachable!()
    };
    assert_eq!(
        split(e)[0],
        selective,
        "the cheap selective conjunct must run first"
    );
    let new = work(&ordered);
    assert_eq!(written.0, new.0);
    assert!(new.2 * 2 < written.2);
    println!(
        "\npredicate ordering by cost/(1-selectivity): conjunct evaluations {} -> {}, expression nodes {} -> {}",
        written.1, new.1, written.2, new.2
    );
}

fn timings() {
    const ROWS: usize = 1_000_000;
    for (name, (db, q), hand) in [
        ("filter + aggregate", filter_query(ROWS), true),
        ("hash join + aggregate", join_query(ROWS), false),
    ] {
        let q = optimize(&q, &db, OPT);
        let bc = bc_compile(&q, &db);
        let mut reference: Option<Vec<Vec<V>>> = None;
        let mut time = |label: &str, f: &dyn Fn() -> Vec<Vec<V>>| {
            let mut best = f64::MAX;
            for _ in 0..3 {
                let t = Instant::now();
                let r = sorted(black_box(f()));
                best = best.min(t.elapsed().as_secs_f64());
                assert_eq!(
                    &r,
                    reference.get_or_insert(r.clone()),
                    "{label} disagrees on the benchmark query"
                );
            }
            println!("  {label:<30} {:>8.1} M rows/s", ROWS as f64 / best / 1e6);
        };
        println!(
            "\nthroughput, {name}, {ROWS} rows, best of 3 (timings are printed, never asserted):"
        );
        let s = || Stats::default();
        time("volcano", &|| run_volcano::<false>(&q, &db, &mut s()));
        time("vectorized (1024)", &|| {
            run_vectorized::<false>(&q, &db, 1024, &mut s())
        });
        time("closures, unfused", &|| {
            run_closure::<false>(&q, &db, false, &mut s())
        });
        time("closures, fused", &|| {
            run_closure::<false>(&q, &db, true, &mut s())
        });
        time("register bytecode", &|| {
            bc_run::<false>(&bc, &q, &db, &mut s())
        });
        if hand {
            time("hand-written native loop", &|| hand_filter(&db));
        }
    }
}

fn main() {
    semantics_checks();

    let (db, q) = join_query(1000);
    let o = optimize(&q, &db, OPT);
    println!(
        "generated Rust for: SELECT d.c2, COUNT(*), SUM(t.c1+d.c1) FROM t JOIN d ON d.c0=t.c0 WHERE t.c2<60 AND d.c1<40 GROUP BY d.c2\n"
    );
    print!("{}", codegen_rust(&o, &db));
    let bc = bc_compile(&o, &db);
    println!(
        "\nthe same plan as register bytecode ({} instructions, {} registers):",
        bc.code.len(),
        bc.regs
    );
    for (i, insn) in bc.code.iter().enumerate() {
        println!("  {i:>3}  {insn:?}");
    }
    println!();

    differential(500);
    mutation_checks();
    predicate_order_demo();
    dispatch_table();
    timings();
}
