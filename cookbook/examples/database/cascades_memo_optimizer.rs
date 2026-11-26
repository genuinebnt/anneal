//! RECIPE 750 (database): a Cascades-style top-down optimiser: memo of groups, transformation + implementation rules, enforcers, branch-and-bound.
//! LEVEL: advanced
//! Use in: query optimisers that must stay extensible (new rules, new operators, new physical properties), SQL planners, EXPLAIN tooling.   Run: cargo run --release --example cascades_memo_optimizer
//! COMMON USES: Microsoft SQL Server's optimiser (Cascades descendant), Greenplum/Pivotal Orca, Apache Calcite's VolcanoPlanner (memo = RelSet/RelSubset, traits = physical
//! properties, converters = enforcers), CockroachDB's `opt` package (memo + normalisation and exploration rules), Apache Doris "Nereids" and StarRocks (Cascades-style planners).
//! Course: CMU 15-799 Query Optimization (Spring 2025), lectures 04-06 (Volcano, Cascades, transformations)
#![allow(dead_code)]
//!
//! WHAT: the heart of a Cascades optimiser in one std-only file, for select-filter-join queries with an optional ORDER BY.
//! * MEMO: a table of GROUPS. A group is a set of logically EQUIVALENT expressions (same result), identified here by
//!   (set of tables, set of applied filters). Its LOGICAL PROPERTIES (schema = the table set, a cardinality estimate) are computed ONCE per group,
//!   because every member must produce the same relation. A GROUP EXPRESSION is an operator whose inputs are GROUPS, not trees: Join(g3, g5).
//!   One memo entry therefore stands for exponentially many trees. DUPLICATE DETECTION: every expression is hashed (its fingerprint is
//!   the derived `Hash`/`Eq` of `LOp`); inserting an expression that already exists returns the existing group, so the rules cannot loop forever.
//! * TRANSFORMATION RULES (logical -> logical): join COMMUTATIVITY, join ASSOCIATIVITY (Join(Join(A,B),C) -> Join(A,Join(B,C)), only when B-C is
//!   connected so no cross product is invented), SELECTION PUSHDOWN through a join (creating new groups for the filtered inputs), SELECTION MERGE.
//!   Each rule has a PATTERN (operator plus the operator of an input group; binding enumerates the input group's expressions) and a PROMISE (a priority
//!   so the likeliest-useful rules fire first, which finds cheap plans early and makes pruning bite). Not implemented: projection pushdown and
//!   aggregate pushdown (the memo shape is the same; they add operators, not machinery).
//! * IMPLEMENTATION RULES (logical -> physical): SeqScan, IndexScan (index on column 0 of every table, delivers rows sorted by it, or a range scan when
//!   a filter hits column 0), Filter, NestedLoop/Hash/MergeJoin. Physical expressions are costed with one shared cost model (`cost` module).
//! * PHYSICAL PROPERTIES: the required SORT ORDER (a column id). A plan delivers an order or not: an IndexScan delivers col 0, a MergeJoin delivers its
//!   join columns (they are equal on matches), a Filter preserves its input's order, hash/NL joins deliver nothing. Each (group, required order) pair has
//!   its OWN winner. ENFORCER: when no candidate delivers the required order, a Sort over the same group with no requirement competes with the rest.
//!   Required properties also propagate to inputs: a MergeJoin asks its children for sorted inputs, which a child satisfies for free with an IndexScan
//!   (an INTERESTING ORDER) instead of paying a Sort. This is Selinger's insight embedded in the search rather than the DP table.
//! * SEARCH: an explicit TASK STACK (no recursion) with the five Cascades tasks: OptimizeGroup, ExploreGroup, ExploreExpr (OptimizeExpr in the paper),
//!   ApplyRule, OptimizeInputs. Exploration is demand-driven (only groups reachable from the root, only when a group is first optimised). OptimizeInputs
//!   costs a physical expression child by child, accumulating cost, and abandons it the moment cost-so-far exceeds the LIMIT.
//! * BRANCH-AND-BOUND: the limit for a candidate is min(caller's limit, cheapest complete plan found so far for this (group, order)). Children get
//!   limit - local cost - accumulated cost. A group that fails under limit L records Failed(L): asked again with limit <= L it answers instantly.
//!   A group that succeeds records its WINNER per (group, required order), so each combination is solved once (memoisation = dynamic programming).
//!
//! COMPARISON. VOLCANO generator (Graefe/McKenna 1993): the ancestor. It first expands ALL logical alternatives (exhaustive), then runs top-down
//! directed dynamic programming with memoisation and branch-and-bound. Rules were compiled from a model file into C. Cascades (Graefe 1995) turns
//! rules and tasks into objects: exploration becomes lazy, rules carry promise, the search can be guided and abandoned, and enforcers/properties are
//! first-class. ORCA keeps Cascades (a MEMO with groups, rules, enforcers, property requests), adds a multi-threaded job scheduler (the task stack
//! becomes parallel jobs) and a stateless interface to the host database. CALCITE's VolcanoPlanner keeps a memo (RelSet = group, RelSubset = group
//! under one trait set) but fires rules from a priority queue, bottom-up and cost-propagating, rather than driving a goal-oriented top-down search; its newer top-down
//! rule driver moves toward Cascades. Bottom-up DP (the `query_optimizer` capstone, PostgreSQL) is simpler and faster for plain join ordering; a memo
//! pays for generality: new rewrite rules and physical properties plug in without touching the search.
//!
//! CHECKS (asserts): (a) chosen cost == brute force over ALL bushy join trees (n <= 5) with the same cost model, and the tree counts match the closed
//! forms; (b) a clique query on n tables yields exactly 2^n - 1 groups, a chain n(n+1)/2 (connected subsets); (c) pruning explores strictly fewer physical
//! expressions in total and never changes the optimum; (d) every (expression, rule) pair fires at most once; (e) executing the chosen plan on random data
//! equals the naive reference result (multiset) on 400 queries + 40 six-table queries; (f) interesting orders: two index-scanned inputs feed a MergeJoin with NO
//! Sort when the order is satisfied, and a Sort enforcer appears exactly when no cheaper source of the order exists; no plan ever sorts an already-sorted input.
//! Read: "The Volcano Optimizer Generator: Extensibility and Efficient Search" (Graefe, McKenna, 1993)   "The Cascades Framework for Query Optimization" (Graefe, 1995)
//!       "Orca: A Modular Query Optimizer Architecture for Big Data" (Soliman et al., 2014)   CMU 15-799 schedule https://15799.courses.cs.cmu.edu/spring2025/schedule.html
//!       Apache Calcite https://calcite.apache.org/docs/   CMU 15-445 https://15445.courses.cs.cmu.edu/

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::time::Instant;

const NCOL: usize = 3; // columns per table: c0 (indexed), c1, c2
const MAXT: usize = 6; // tables an EXECUTED query may have (rows are fixed-size arrays)
const INF: f64 = f64::INFINITY;

type Row = [i64; MAXT * NCOL]; // column id = table * NCOL + col; cells of absent tables stay 0
type GroupId = usize;
type Req = Option<usize>; // required sort order: a column id, or no requirement

fn col_id(t: usize, c: usize) -> usize {
    t * NCOL + c
}
fn bits(mask: u32) -> impl Iterator<Item = usize> {
    (0..32).filter(move |i| mask >> i & 1 == 1)
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n.max(1)
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- the query and its statistics

/// equi-join predicate t_a.c_ca = t_b.c_cb with its estimated selectivity 1/max(ndv)
#[derive(Clone, Debug)]
struct Edge {
    a: usize,
    ca: usize,
    b: usize,
    cb: usize,
    sel: f64,
}
/// range filter lo <= t.col <= hi
#[derive(Clone, Debug)]
struct Filter {
    t: usize,
    col: usize,
    lo: i64,
    hi: i64,
    sel: f64,
}
#[derive(Clone, Debug)]
struct Query {
    n: usize,
    nrows: Vec<f64>,
    dom: Vec<[i64; NCOL]>, // values of t.c lie in 0..dom (uniform): the "statistics"
    edges: Vec<Edge>,
    filters: Vec<Filter>,
    order: Req,
}

impl Query {
    fn new(
        nrows: Vec<f64>,
        dom: Vec<[i64; NCOL]>,
        edges: &[(usize, usize, usize, usize)],
        filters: &[(usize, usize, i64, i64)],
        order: Req,
    ) -> Query {
        let edges = edges
            .iter()
            .map(|&(a, ca, b, cb)| Edge {
                a,
                ca,
                b,
                cb,
                sel: 1.0 / dom[a][ca].max(dom[b][cb]) as f64,
            })
            .collect();
        let filters = filters
            .iter()
            .map(|&(t, col, lo, hi)| Filter {
                t,
                col,
                lo,
                hi,
                sel: ((hi - lo + 1) as f64 / dom[t][col] as f64).min(1.0),
            })
            .collect();
        Query {
            n: nrows.len(),
            nrows,
            dom,
            edges,
            filters,
            order,
        }
    }
    /// clique on column 0 / chain on column 0, no filters: the shapes with closed-form group counts
    fn shaped(n: usize, clique: bool, rows: f64) -> Query {
        let mut e = vec![];
        for a in 0..n {
            for b in a + 1..n {
                if clique || b == a + 1 {
                    e.push((a, 0, b, 0));
                }
            }
        }
        Query::new(vec![rows; n], vec![[100; NCOL]; n], &e, &[], None)
    }
    fn all(&self) -> u32 {
        (1u32 << self.n) - 1
    }
    /// the filters that mention only tables of `tables` (what a group over those tables may have applied)
    fn filters_of(&self, tables: u32) -> u32 {
        (0..self.filters.len())
            .filter(|&i| tables >> self.filters[i].t & 1 == 1)
            .fold(0, |m, i| m | 1 << i)
    }
    /// LOGICAL PROPERTY: cardinality of the relation over `tables` with filters `fm` applied (independence assumption, floor 1)
    fn card(&self, tables: u32, fm: u32) -> f64 {
        let mut c: f64 = bits(tables).map(|t| self.nrows[t]).product();
        c *= bits(fm).map(|i| self.filters[i].sel).product::<f64>();
        for e in &self.edges {
            if tables >> e.a & 1 == 1 && tables >> e.b & 1 == 1 {
                c *= e.sel;
            }
        }
        c.max(1.0)
    }
    /// predicates linking `l` to `r`, as (column id in l, column id in r)
    fn edges_between(&self, l: u32, r: u32) -> Vec<(usize, usize)> {
        let mut out = vec![];
        for e in &self.edges {
            if l >> e.a & 1 == 1 && r >> e.b & 1 == 1 {
                out.push((col_id(e.a, e.ca), col_id(e.b, e.cb)));
            } else if l >> e.b & 1 == 1 && r >> e.a & 1 == 1 {
                out.push((col_id(e.b, e.cb), col_id(e.a, e.ca)));
            }
        }
        out
    }
    fn is_connected(&self, set: u32) -> bool {
        let Some(first) = bits(set).next() else {
            return true;
        };
        let mut seen = 1u32 << first;
        loop {
            let before = seen;
            for e in &self.edges {
                let (a, b) = (1u32 << e.a, 1u32 << e.b);
                if set & a != 0 && set & b != 0 && (seen & (a | b) != 0) {
                    seen |= a | b;
                }
            }
            if seen == before {
                return seen == set;
            }
        }
    }
    /// a left-deep join order in which every prefix is connected (BFS over the join graph)
    fn connected_order(&self) -> Vec<usize> {
        let mut order = vec![0usize];
        let mut seen = 1u32;
        while order.len() < self.n {
            let next = (0..self.n)
                .find(|&t| seen >> t & 1 == 0 && !self.edges_between(seen, 1 << t).is_empty())
                .expect("query graph must be connected");
            seen |= 1 << next;
            order.push(next);
        }
        order
    }
}

// ---------------------------------------------------------------- the cost model (shared by the memo and the brute force)

mod cost {
    use super::*;
    pub fn sort(n: f64) -> f64 {
        0.05 * n * (n + 2.0).log2() // the ENFORCER's price
    }
    pub fn seq_scan(q: &Query, t: usize) -> f64 {
        q.nrows[t] // filters on a scan are evaluated for free while reading
    }
    /// range scan when a filter hits the indexed column 0, else a (slightly dearer) full ordered scan
    pub fn index_scan(q: &Query, t: usize, fm: u32) -> f64 {
        let n = q.nrows[t];
        match bits(fm)
            .map(|i| &q.filters[i])
            .find(|f| f.t == t && f.col == 0)
        {
            Some(f) => (n + 2.0).log2() + 3.0 * n * f.sel,
            None => 1.2 * n,
        }
    }
    pub fn filter(input_rows: f64) -> f64 {
        0.1 * input_rows
    }
    pub fn hash_join(probe: f64, build: f64, out: f64) -> f64 {
        1.0 * probe + 1.5 * build + 0.1 * out
    }
    pub fn nested_loop(l: f64, r: f64, out: f64) -> f64 {
        0.02 * l * r + 0.1 * out
    }
    pub fn merge_join(l: f64, r: f64, out: f64) -> f64 {
        0.5 * (l + r) + 0.1 * out
    }
}

// ---------------------------------------------------------------- memo

/// LOGICAL group expression: inputs are groups. Derived Hash/Eq is the duplicate-detection fingerprint.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum LOp {
    Get(usize),
    Select(u32, GroupId), // filter mask, input
    Join(GroupId, GroupId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum POp {
    Seq { t: usize, fm: u32 },
    Index { t: usize, fm: u32 },
    Filter { fm: u32 },
    Hash,
    NestedLoop,
    Merge { lc: usize, rc: usize },
    Sort { col: usize },
}

/// PHYSICAL group expression with the property each input is REQUIRED to deliver
#[derive(Clone, Debug)]
struct PExpr {
    op: POp,
    kids: Vec<(GroupId, Req)>,
}

struct Group {
    key: (u32, u32), // (tables, applied filters) = the group's identity and schema
    card: f64,       // logical property
    exprs: Vec<LOp>,
    explored: bool,
}

#[derive(Clone)]
struct Winner {
    cost: f64,
    pe: PExpr,
}
enum Entry {
    Won(Winner),
    Failed(f64), // no plan with cost <= limit exists
}

struct Memo<'a> {
    q: &'a Query,
    groups: Vec<Group>,
    by_key: HashMap<(u32, u32), GroupId>,
    by_expr: HashMap<LOp, (GroupId, usize)>,
    winners: HashMap<(GroupId, Req), Entry>,
}

impl<'a> Memo<'a> {
    fn tables(&self, g: GroupId) -> u32 {
        self.groups[g].key.0
    }
    fn key_of(&self, op: &LOp) -> (u32, u32) {
        match *op {
            LOp::Get(t) => (1 << t, 0),
            LOp::Select(m, c) => (self.groups[c].key.0, self.groups[c].key.1 | m),
            LOp::Join(l, r) => (
                self.groups[l].key.0 | self.groups[r].key.0,
                self.groups[l].key.1 | self.groups[r].key.1,
            ),
        }
    }
    /// insert an expression: an existing fingerprint returns (its group, None); otherwise it joins the group with the same logical identity
    /// (created on demand) and returns its index there.
    fn insert(&mut self, op: LOp) -> (GroupId, Option<usize>) {
        if let Some(&(g, _)) = self.by_expr.get(&op) {
            return (g, None);
        }
        let key = self.key_of(&op);
        let g = match self.by_key.get(&key) {
            Some(&g) => g,
            None => {
                self.groups.push(Group {
                    key,
                    card: self.q.card(key.0, key.1),
                    exprs: vec![],
                    explored: false,
                });
                self.by_key.insert(key, self.groups.len() - 1);
                self.groups.len() - 1
            }
        };
        self.groups[g].exprs.push(op);
        let idx = self.groups[g].exprs.len() - 1;
        self.by_expr.insert(op, (g, idx));
        (g, Some(idx))
    }
    fn lop_text(&self, op: &LOp) -> String {
        match op {
            LOp::Get(t) => format!("Get(t{t})"),
            LOp::Select(m, c) => format!("Select({m:#b},G{c})"),
            LOp::Join(l, r) => format!("Join(G{l},G{r})"),
        }
    }
}

// ---------------------------------------------------------------- rules

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Rule {
    Commute,
    Assoc,
    Push,
    Merge,
}
const RULES: [Rule; 4] = [Rule::Assoc, Rule::Commute, Rule::Merge, Rule::Push]; // ascending promise

impl Rule {
    /// PROMISE: higher fires first. Pushdown shrinks inputs (cheap plans early), commutativity is cheap, associativity explodes the memo.
    fn promise(self) -> u32 {
        match self {
            Rule::Push => 4,
            Rule::Merge => 3,
            Rule::Commute => 2,
            Rule::Assoc => 1,
        }
    }
    /// top-level pattern: the operator the rule is rooted at (input patterns are bound while applying)
    fn matches(self, op: &LOp) -> bool {
        matches!(
            (self, op),
            (Rule::Commute | Rule::Assoc, LOp::Join(..))
                | (Rule::Push | Rule::Merge, LOp::Select(..))
        )
    }
}

// ---------------------------------------------------------------- the search

#[derive(Clone, Copy, Debug)]
enum Task {
    OptimizeGroup { g: GroupId, req: Req, limit: f64 },
    Finalize { g: GroupId, req: Req, limit: f64 },
    ExploreGroup(GroupId),
    ExploreExpr(GroupId, usize),
    ApplyRule { g: GroupId, i: usize, rule: Rule },
}

/// OptimizeInputs is kept apart from `Task` because it carries a physical expression (not Copy)
#[derive(Clone, Debug)]
struct Inputs {
    g: GroupId,
    req: Req,
    pe: PExpr,
    local: f64,
    idx: usize,
    acc: f64,
    limit: f64,
}
enum Work {
    Basic(Task),
    OptimizeInputs(Inputs),
}

#[derive(Clone, Debug, Default)]
struct Stats {
    tasks: usize,
    rule_fires: usize,
    candidates: usize, // physical expressions considered
    costed: usize,     // physical expressions whose inputs were all costed (not cut off)
    pruned: usize,     // abandoned by branch-and-bound
    explored_exprs: usize,
}

struct Optimizer<'a> {
    memo: Memo<'a>,
    stack: Vec<Work>,
    prune: bool,
    best: HashMap<(GroupId, Req), Winner>, // cheapest complete candidate of each in-progress OptimizeGroup
    seen: HashSet<(GroupId, usize)>,
    applied: HashMap<(GroupId, usize, Rule), u32>,
    stats: Stats,
    root: GroupId,
}

impl<'a> Optimizer<'a> {
    /// initial logical plan: left-deep joins in a connected order, then one Select per filter stacked on top (so merge and pushdown have work to do)
    fn new(q: &'a Query, prune: bool) -> Optimizer<'a> {
        let memo = Memo {
            q,
            groups: vec![],
            by_key: HashMap::new(),
            by_expr: HashMap::new(),
            winners: HashMap::new(),
        };
        let mut o = Optimizer {
            memo,
            stack: vec![],
            prune,
            best: HashMap::new(),
            seen: HashSet::new(),
            applied: HashMap::new(),
            stats: Stats::default(),
            root: 0,
        };
        let order = q.connected_order();
        let mut g = o.memo.insert(LOp::Get(order[0])).0;
        for &t in &order[1..] {
            let leaf = o.memo.insert(LOp::Get(t)).0;
            g = o.memo.insert(LOp::Join(g, leaf)).0;
        }
        for i in 0..q.filters.len() {
            g = o.memo.insert(LOp::Select(1 << i, g)).0;
        }
        o.root = g;
        o
    }

    /// optimise the root for the query's required order and return the cheapest plan
    fn run(&mut self) -> Plan {
        let req = self.memo.q.order;
        self.stack.push(Work::Basic(Task::OptimizeGroup {
            g: self.root,
            req,
            limit: INF,
        }));
        while let Some(w) = self.stack.pop() {
            self.stats.tasks += 1;
            match w {
                Work::Basic(t) => self.basic(t),
                Work::OptimizeInputs(i) => self.optimize_inputs(i),
            }
        }
        self.extract(self.root, req)
    }

    fn push(&mut self, t: Task) {
        self.stack.push(Work::Basic(t));
    }

    fn basic(&mut self, t: Task) {
        match t {
            Task::OptimizeGroup { g, req, limit } => self.optimize_group(g, req, limit),
            Task::Finalize { g, req, limit } => {
                let entry = match self.best.remove(&(g, req)) {
                    Some(w) if w.cost <= limit => Entry::Won(w),
                    _ => Entry::Failed(limit),
                };
                self.memo.winners.insert((g, req), entry);
            }
            Task::ExploreGroup(g) => {
                if self.memo.groups[g].explored {
                    return;
                }
                self.memo.groups[g].explored = true; // expressions added later are scheduled by whoever adds them
                for i in (0..self.memo.groups[g].exprs.len()).rev() {
                    self.push(Task::ExploreExpr(g, i));
                }
            }
            Task::ExploreExpr(g, i) => {
                if !self.seen.insert((g, i)) {
                    return;
                }
                self.stats.explored_exprs += 1;
                let op = self.memo.groups[g].exprs[i];
                for rule in RULES {
                    if rule.matches(&op) {
                        self.push(Task::ApplyRule { g, i, rule }); // lowest promise first, so the highest is popped first
                    }
                }
                // inputs are explored BEFORE the rules run: patterns bind to the input groups' expressions, which must be complete
                match op {
                    LOp::Join(l, r) => {
                        self.push(Task::ExploreGroup(r));
                        self.push(Task::ExploreGroup(l));
                    }
                    LOp::Select(_, c) => self.push(Task::ExploreGroup(c)),
                    LOp::Get(_) => {}
                }
            }
            Task::ApplyRule { g, i, rule } => self.apply_rule(g, i, rule),
        }
    }

    // ----- OptimizeGroup: pick the cheapest physical plan of (g, req) under `limit`

    fn optimize_group(&mut self, g: GroupId, req: Req, limit: f64) {
        let limit = if self.prune { limit } else { INF };
        match self.memo.winners.get(&(g, req)) {
            Some(Entry::Won(_)) => return,
            Some(Entry::Failed(l)) if limit <= *l => return,
            _ => {}
        }
        if !self.memo.groups[g].explored {
            self.push(Task::OptimizeGroup { g, req, limit }); // run again after exploration
            self.push(Task::ExploreGroup(g));
            return;
        }
        self.best.remove(&(g, req));
        let mut cands = self.implementations(g, req);
        if req.is_some() {
            // ENFORCER: Sort(G, no requirement) competes with every plan that delivers the order natively
            let local = cost::sort(self.memo.groups[g].card);
            cands.push((
                0,
                PExpr {
                    op: POp::Sort { col: req.unwrap() },
                    kids: vec![(g, None)],
                },
                local,
            ));
        }
        cands.sort_by_key(|c| c.0);
        self.push(Task::Finalize { g, req, limit });
        self.stats.candidates += cands.len();
        for (_, pe, local) in cands {
            self.stack.push(Work::OptimizeInputs(Inputs {
                g,
                req,
                pe,
                local,
                idx: 0,
                acc: 0.0,
                limit,
            }));
        }
    }

    /// IMPLEMENTATION RULES: logical expression -> physical candidates (promise, expression, local cost) delivering `req` natively
    fn implementations(&self, g: GroupId, req: Req) -> Vec<(u32, PExpr, f64)> {
        let m = &self.memo;
        let q = m.q;
        let mut out = vec![];
        let none = |op, kids| PExpr { op, kids };
        for op in &m.groups[g].exprs {
            match *op {
                LOp::Get(t) => self.scan_candidates(t, 0, req, &mut out),
                LOp::Select(fm, c) => {
                    // pattern Select(Get): bind the input group's expressions; fuse the filter into the scan
                    for inner in &m.groups[c].exprs {
                        if let LOp::Get(t) = *inner {
                            self.scan_candidates(t, fm, req, &mut out);
                        }
                    }
                    // general case: a Filter operator; the order requirement passes straight through to its input
                    let local = cost::filter(m.groups[c].card);
                    out.push((2, none(POp::Filter { fm }, vec![(c, req)]), local));
                }
                LOp::Join(l, r) => {
                    let edges = q.edges_between(m.tables(l), m.tables(r));
                    if edges.is_empty() {
                        continue; // never implement a cross product
                    }
                    let (cl, cr, co) = (m.groups[l].card, m.groups[r].card, m.groups[g].card);
                    if req.is_none() {
                        out.push((
                            5,
                            none(POp::Hash, vec![(l, None), (r, None)]),
                            cost::hash_join(cl, cr, co),
                        ));
                        out.push((
                            1,
                            none(POp::NestedLoop, vec![(l, None), (r, None)]),
                            cost::nested_loop(cl, cr, co),
                        ));
                    }
                    for (lc, rc) in edges {
                        if req.is_none() || req == Some(lc) || req == Some(rc) {
                            // MergeJoin REQUIRES both inputs sorted on its keys and DELIVERS that order
                            let pe =
                                none(POp::Merge { lc, rc }, vec![(l, Some(lc)), (r, Some(rc))]);
                            out.push((3, pe, cost::merge_join(cl, cr, co)));
                        }
                    }
                }
            }
        }
        out
    }

    fn scan_candidates(&self, t: usize, fm: u32, req: Req, out: &mut Vec<(u32, PExpr, f64)>) {
        let q = self.memo.q;
        if req.is_none() {
            out.push((
                4,
                PExpr {
                    op: POp::Seq { t, fm },
                    kids: vec![],
                },
                cost::seq_scan(q, t),
            ));
        }
        if req.is_none() || req == Some(col_id(t, 0)) {
            out.push((
                3,
                PExpr {
                    op: POp::Index { t, fm },
                    kids: vec![],
                },
                cost::index_scan(q, t, fm),
            ));
        }
    }

    // ----- OptimizeInputs: cost a physical expression child by child with a bound

    fn optimize_inputs(&mut self, mut w: Inputs) {
        let cur = self.best.get(&(w.g, w.req)).map_or(INF, |b| b.cost);
        let bound = if self.prune { w.limit.min(cur) } else { INF };
        if w.local > bound {
            self.stats.pruned += 1;
            return;
        }
        for i in w.idx..w.pe.kids.len() {
            let (cg, creq) = w.pe.kids[i];
            match self.memo.winners.get(&(cg, creq)) {
                Some(Entry::Won(win)) => {
                    w.acc += win.cost;
                    if w.local + w.acc > bound {
                        self.stats.pruned += 1; // cost so far already exceeds the best known plan: BRANCH-AND-BOUND cut
                        return;
                    }
                }
                other => {
                    let climit = bound - w.local - w.acc;
                    if matches!(other, Some(Entry::Failed(l)) if climit <= *l) {
                        self.stats.pruned += 1; // the child already failed under an equal or larger limit
                        return;
                    }
                    w.idx = i;
                    self.stack.push(Work::OptimizeInputs(w));
                    self.push(Task::OptimizeGroup {
                        g: cg,
                        req: creq,
                        limit: climit,
                    });
                    return;
                }
            }
        }
        self.stats.costed += 1;
        let total = w.local + w.acc;
        if self.best.get(&(w.g, w.req)).is_none_or(|b| total < b.cost) {
            self.best.insert(
                (w.g, w.req),
                Winner {
                    cost: total,
                    pe: w.pe,
                },
            );
        }
    }

    // ----- ApplyRule: transformation rules with pattern bindings

    fn add_expr(&mut self, op: LOp) -> GroupId {
        let (g, new) = self.memo.insert(op);
        if let Some(i) = new {
            if self.memo.groups[g].explored {
                self.push(Task::ExploreExpr(g, i));
            }
        }
        g
    }
    fn wrap(&mut self, mask: u32, g: GroupId) -> GroupId {
        if mask == 0 {
            g
        } else {
            self.add_expr(LOp::Select(mask, g))
        }
    }

    fn apply_rule(&mut self, g: GroupId, i: usize, rule: Rule) {
        *self.applied.entry((g, i, rule)).or_insert(0) += 1;
        self.stats.rule_fires += 1;
        let q = self.memo.q;
        let op = self.memo.groups[g].exprs[i];
        let mut out = vec![];
        match (rule, op) {
            (Rule::Commute, LOp::Join(l, r)) => out.push(LOp::Join(r, l)),
            // Join(Join(A,B),C) -> Join(A, Join(B,C)): bind every Join expression of the left input group
            (Rule::Assoc, LOp::Join(x, c)) => {
                for inner in self.memo.groups[x].exprs.clone() {
                    if let LOp::Join(a, b) = inner {
                        if q.edges_between(self.memo.tables(b), self.memo.tables(c))
                            .is_empty()
                        {
                            continue; // B-C would be a cross product
                        }
                        let bc = self.add_expr(LOp::Join(b, c));
                        out.push(LOp::Join(a, bc));
                    }
                }
            }
            // Select(m, Join(L,R)) -> Join(Select(m_L, L), Select(m_R, R))
            (Rule::Push, LOp::Select(m, c)) => {
                for inner in self.memo.groups[c].exprs.clone() {
                    if let LOp::Join(l, r) = inner {
                        let (ml, mr) = (
                            m & q.filters_of(self.memo.tables(l)),
                            m & q.filters_of(self.memo.tables(r)),
                        );
                        let (nl, nr) = (self.wrap(ml, l), self.wrap(mr, r));
                        out.push(LOp::Join(nl, nr));
                    }
                }
            }
            // Select(m1, Select(m2, X)) -> Select(m1|m2, X)
            (Rule::Merge, LOp::Select(m1, c)) => {
                for inner in self.memo.groups[c].exprs.clone() {
                    if let LOp::Select(m2, c2) = inner {
                        out.push(LOp::Select(m1 | m2, c2));
                    }
                }
            }
            _ => unreachable!("rule applied to an expression it does not match"),
        }
        for o in out {
            let (g2, new) = self.memo.insert(o);
            assert_eq!(
                g2, g,
                "a rule must produce an equivalent expression: same group"
            );
            if let Some(j) = new {
                self.push(Task::ExploreExpr(g, j));
            }
        }
    }

    // ----- plan extraction

    fn extract(&self, g: GroupId, req: Req) -> Plan {
        let Some(Entry::Won(w)) = self.memo.winners.get(&(g, req)) else {
            panic!("no winner for G{g} {req:?}")
        };
        Plan {
            op: w.pe.op,
            kids: w
                .pe
                .kids
                .iter()
                .map(|&(cg, cr)| self.extract(cg, cr))
                .collect(),
            rows: self.memo.groups[g].card,
            cost: w.cost,
        }
    }
}

// ---------------------------------------------------------------- plans: explain, properties

struct Plan {
    op: POp,
    kids: Vec<Plan>,
    rows: f64,
    cost: f64,
}

impl Plan {
    fn tables(&self) -> u32 {
        match self.op {
            POp::Seq { t, .. } | POp::Index { t, .. } => 1 << t,
            _ => self.kids.iter().fold(0, |m, k| m | k.tables()),
        }
    }
    /// does this plan deliver rows sorted by column `col`?
    fn delivers(&self, col: usize) -> bool {
        match self.op {
            POp::Index { t, .. } => col == col_id(t, 0),
            POp::Filter { .. } => self.kids[0].delivers(col),
            POp::Merge { lc, rc } => col == lc || col == rc,
            POp::Sort { col: c } => c == col,
            _ => false,
        }
    }
    fn count(&self, pred: &dyn Fn(&POp) -> bool) -> usize {
        usize::from(pred(&self.op)) + self.kids.iter().map(|k| k.count(pred)).sum::<usize>()
    }
    fn explain(&self, q: &Query, depth: usize, out: &mut String) {
        let fs = |fm: u32| {
            bits(fm)
                .map(|i| {
                    let f = &q.filters[i];
                    format!(" [t{}.c{} in {}..{}]", f.t, f.col, f.lo, f.hi)
                })
                .collect::<String>()
        };
        let name = match self.op {
            POp::Seq { t, fm } => format!("SeqScan t{t}{}", fs(fm)),
            POp::Index { t, fm } => format!("IndexScan t{t}.c0{}", fs(fm)),
            POp::Filter { fm } => format!("Filter{}", fs(fm)),
            POp::Hash => "HashJoin".into(),
            POp::NestedLoop => "NestedLoopJoin".into(),
            POp::Merge { lc, rc } => format!(
                "MergeJoin t{}.c{} = t{}.c{}",
                lc / NCOL,
                lc % NCOL,
                rc / NCOL,
                rc % NCOL
            ),
            POp::Sort { col } => format!("Sort t{}.c{}   <- enforcer", col / NCOL, col % NCOL),
        };
        let _ = writeln!(
            out,
            "{}{:<46} rows={:<9.1} cost={:.1}",
            "  ".repeat(depth),
            name,
            self.rows,
            self.cost
        );
        for k in &self.kids {
            k.explain(q, depth + 1, out);
        }
    }
}

// ---------------------------------------------------------------- brute force: every bushy tree, same cost model

#[derive(Clone)]
enum Tree {
    Leaf(usize),
    Join(Box<Tree>, Box<Tree>),
}

/// all ordered binary join trees over `set` without cross products
fn all_trees(q: &Query, set: u32, cache: &mut HashMap<u32, Vec<Tree>>) -> Vec<Tree> {
    if let Some(v) = cache.get(&set) {
        return v.clone();
    }
    let mut out = vec![];
    if set.count_ones() == 1 {
        out.push(Tree::Leaf(set.trailing_zeros() as usize));
    } else {
        let mut l = (set - 1) & set;
        while l != 0 {
            let r = set ^ l;
            if !q.edges_between(l, r).is_empty() {
                let (ls, rs) = (all_trees(q, l, cache), all_trees(q, r, cache));
                for a in &ls {
                    for b in &rs {
                        out.push(Tree::Join(Box::new(a.clone()), Box::new(b.clone())));
                    }
                }
            }
            l = (l - 1) & set;
        }
    }
    cache.insert(set, out.clone());
    out
}

/// best cost of one fixed tree for every possible required order (None, or any column of its tables)
struct Profile {
    tables: u32,
    by_req: HashMap<Req, f64>,
}

fn finish(q: &Query, tables: u32, rows: f64, opts: Vec<(f64, Vec<usize>)>) -> Profile {
    let mut by_req = HashMap::new();
    by_req.insert(None, opts.iter().map(|o| o.0).fold(INF, f64::min));
    for t in bits(tables) {
        for c in 0..NCOL {
            let col = col_id(t, c);
            let best = opts
                .iter()
                .map(|(cost, d)| {
                    cost + if d.contains(&col) {
                        0.0
                    } else {
                        cost::sort(rows)
                    }
                })
                .fold(INF, f64::min);
            by_req.insert(Some(col), best);
        }
    }
    let _ = q;
    Profile { tables, by_req }
}

fn profile(q: &Query, t: &Tree) -> Profile {
    match t {
        Tree::Leaf(x) => {
            let fm = q.filters_of(1 << x); // filters are applied at the leaves (pushing them down is never worse in this model)
            let rows = q.card(1 << x, fm);
            let opts = vec![
                (cost::seq_scan(q, *x), vec![]),
                (cost::index_scan(q, *x, fm), vec![col_id(*x, 0)]),
            ];
            finish(q, 1 << x, rows, opts)
        }
        Tree::Join(l, r) => {
            let (pl, pr) = (profile(q, l), profile(q, r));
            let tables = pl.tables | pr.tables;
            let (cl, cr) = (
                q.card(pl.tables, q.filters_of(pl.tables)),
                q.card(pr.tables, q.filters_of(pr.tables)),
            );
            let co = q.card(tables, q.filters_of(tables));
            let (nl, nr) = (pl.by_req[&None], pr.by_req[&None]);
            let mut opts = vec![
                (nl + nr + cost::hash_join(cl, cr, co), vec![]),
                (nl + nr + cost::nested_loop(cl, cr, co), vec![]),
            ];
            for (lc, rc) in q.edges_between(pl.tables, pr.tables) {
                opts.push((
                    pl.by_req[&Some(lc)] + pr.by_req[&Some(rc)] + cost::merge_join(cl, cr, co),
                    vec![lc, rc],
                ));
            }
            finish(q, tables, co, opts)
        }
    }
}

/// (optimal cost for the query's order, number of join trees examined)
fn brute_force(q: &Query) -> (f64, usize) {
    let trees = all_trees(q, q.all(), &mut HashMap::new());
    let best = trees
        .iter()
        .map(|t| profile(q, t).by_req[&q.order])
        .fold(INF, f64::min);
    (best, trees.len())
}

// ---------------------------------------------------------------- execution (to prove plans are CORRECT, not just cheap)

type Db = Vec<Vec<[i64; NCOL]>>;

fn gen_db(q: &Query, rng: &mut Rng) -> Db {
    (0..q.n)
        .map(|t| {
            (0..q.nrows[t] as usize)
                .map(|_| std::array::from_fn(|c| rng.range(0, q.dom[t][c] - 1)))
                .collect()
        })
        .collect()
}

fn passes(f: &Filter, r: &Row) -> bool {
    (f.lo..=f.hi).contains(&r[col_id(f.t, f.col)])
}

fn scan(q: &Query, db: &Db, t: usize, fm: u32) -> Vec<Row> {
    db[t]
        .iter()
        .map(|vals| {
            let mut r = [0; MAXT * NCOL];
            r[t * NCOL..t * NCOL + NCOL].copy_from_slice(vals);
            r
        })
        .filter(|r| bits(fm).all(|i| passes(&q.filters[i], r)))
        .collect()
}

fn merged(l: &Row, r: &Row, rtables: u32) -> Row {
    let mut o = *l;
    for t in bits(rtables) {
        o[t * NCOL..t * NCOL + NCOL].copy_from_slice(&r[t * NCOL..t * NCOL + NCOL]);
    }
    o
}

fn exec(p: &Plan, q: &Query, db: &Db) -> Vec<Row> {
    match p.op {
        POp::Seq { t, fm } => scan(q, db, t, fm),
        POp::Index { t, fm } => {
            let mut v = scan(q, db, t, fm);
            v.sort_by_key(|r| r[col_id(t, 0)]); // an index yields rows in key order
            v
        }
        POp::Filter { fm } => exec(&p.kids[0], q, db)
            .into_iter()
            .filter(|r| bits(fm).all(|i| passes(&q.filters[i], r)))
            .collect(),
        POp::Sort { col } => {
            let mut v = exec(&p.kids[0], q, db);
            v.sort_by_key(|r| r[col]);
            v
        }
        POp::Hash | POp::NestedLoop | POp::Merge { .. } => {
            let (l, r) = (exec(&p.kids[0], q, db), exec(&p.kids[1], q, db));
            let (tl, tr) = (p.kids[0].tables(), p.kids[1].tables());
            let edges = q.edges_between(tl, tr);
            let ok = |a: &Row, b: &Row| edges.iter().all(|&(lc, rc)| a[lc] == b[rc]);
            let mut out = vec![];
            match p.op {
                POp::NestedLoop => {
                    for a in &l {
                        out.extend(r.iter().filter(|b| ok(a, b)).map(|b| merged(a, b, tr)));
                    }
                }
                POp::Hash => {
                    let (lc, rc) = edges[0];
                    let mut table: HashMap<i64, Vec<&Row>> = HashMap::new();
                    for b in &r {
                        table.entry(b[rc]).or_default().push(b);
                    }
                    for a in &l {
                        for b in table.get(&a[lc]).into_iter().flatten().filter(|b| ok(a, b)) {
                            out.push(merged(a, b, tr));
                        }
                    }
                }
                POp::Merge { lc, rc } => {
                    // a REAL merge: it trusts its inputs to be sorted, so a missing enforcer would be caught right here
                    assert!(
                        l.windows(2).all(|w| w[0][lc] <= w[1][lc]),
                        "MergeJoin left input is not sorted"
                    );
                    assert!(
                        r.windows(2).all(|w| w[0][rc] <= w[1][rc]),
                        "MergeJoin right input is not sorted"
                    );
                    let (mut i, mut j) = (0, 0);
                    while i < l.len() && j < r.len() {
                        match l[i][lc].cmp(&r[j][rc]) {
                            std::cmp::Ordering::Less => i += 1,
                            std::cmp::Ordering::Greater => j += 1,
                            std::cmp::Ordering::Equal => {
                                let k = l[i][lc];
                                let ie = i + l[i..].iter().take_while(|x| x[lc] == k).count();
                                let je = j + r[j..].iter().take_while(|x| x[rc] == k).count();
                                for a in &l[i..ie] {
                                    out.extend(
                                        r[j..je]
                                            .iter()
                                            .filter(|b| ok(a, b))
                                            .map(|b| merged(a, b, tr)),
                                    );
                                }
                                (i, j) = (ie, je);
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
            out
        }
    }
}

/// REFERENCE: no optimiser at all. Tables in index order, filters at the scan, join predicates the moment both sides exist.
fn naive(q: &Query, db: &Db) -> Vec<Row> {
    let mut rows = vec![[0i64; MAXT * NCOL]];
    let mut present = 0u32;
    for t in 0..q.n {
        let base = scan(q, db, t, q.filters_of(1 << t));
        let edges = q.edges_between(present, 1 << t);
        let mut next = vec![];
        for a in &rows {
            for b in &base {
                if edges.iter().all(|&(lc, rc)| a[lc] == b[rc]) {
                    next.push(merged(a, b, 1 << t));
                }
            }
        }
        rows = next;
        present |= 1 << t;
    }
    rows
}

fn sorted(mut v: Vec<Row>) -> Vec<Row> {
    v.sort();
    v
}

// ---------------------------------------------------------------- random queries

fn random_query(rng: &mut Rng, n: usize) -> Query {
    let nrows: Vec<f64> = (0..n).map(|_| rng.range(3, 20) as f64).collect();
    let dom: Vec<[i64; NCOL]> = (0..n)
        .map(|_| std::array::from_fn(|_| rng.range(2, 9)))
        .collect();
    let mut edges = vec![];
    for t in 1..n {
        edges.push((
            rng.below(t as u64) as usize,
            rng.below(3) as usize,
            t,
            rng.below(3) as usize,
        )); // random spanning tree: connected
    }
    for a in 0..n {
        for b in a + 1..n {
            if rng.chance(25) {
                edges.push((a, rng.below(3) as usize, b, rng.below(3) as usize)); // extra edges, sometimes parallel (residual predicates)
            }
        }
    }
    let mut filters = vec![];
    for _ in 0..rng.below(4) {
        let (t, c) = (rng.below(n as u64) as usize, rng.below(3) as usize);
        let lo = rng.range(0, dom[t][c] - 1);
        let hi = (lo + rng.range(0, dom[t][c] / 2)).min(dom[t][c] - 1);
        filters.push((t, c, lo, hi));
    }
    let order = if rng.chance(45) {
        Some(col_id(rng.below(n as u64) as usize, rng.below(3) as usize))
    } else {
        None
    };
    Query::new(nrows, dom, &edges, &filters, order)
}

fn check_plan_properties(p: &Plan, req: Req) {
    if let Some(c) = req {
        assert!(p.delivers(c), "plan does not deliver the required order");
    }
    if let POp::Sort { col } = p.op {
        assert!(
            !p.kids[0].delivers(col),
            "redundant Sort over an input that is already sorted"
        );
    }
    if let POp::Merge { lc, rc } = p.op {
        assert!(
            p.kids[0].delivers(lc) && p.kids[1].delivers(rc),
            "MergeJoin input order not guaranteed by the plan"
        );
    }
    // each child's own requirement was set by its parent; recurse with what the node needs
    for (i, k) in p.kids.iter().enumerate() {
        let need = match p.op {
            POp::Merge { lc, rc } => Some(if i == 0 { lc } else { rc }),
            POp::Filter { .. } => req,
            _ => None,
        };
        check_plan_properties(k, need);
    }
}

fn group_dump(o: &Optimizer, limit: usize) -> String {
    let mut s = String::new();
    for (i, g) in o.memo.groups.iter().enumerate().take(limit) {
        let tabs = bits(g.key.0)
            .map(|t| format!("t{t}"))
            .collect::<Vec<_>>()
            .join(",");
        let exprs = g
            .exprs
            .iter()
            .map(|e| o.memo.lop_text(e))
            .collect::<Vec<_>>()
            .join(" | ");
        let _ = writeln!(
            s,
            "  G{i:<2} {{{tabs}}} filters={:#b} rows={:<8.1} {exprs}",
            g.key.1, g.card
        );
    }
    s
}

// ---------------------------------------------------------------- main: demo + checks

fn main() {
    let mut rng = Rng(0x00C0FFEE_CA5CADE5);
    demo();
    check_counts_and_closed_forms();

    // (a) (c) (d) (e): 400 random queries, every property in one pass
    let t0 = Instant::now();
    let (mut sum_pruned, mut sum_full, mut sum_tasks_p, mut sum_tasks_f) =
        (0usize, 0usize, 0usize, 0usize);
    let (mut trees_total, mut merges, mut sorts, mut ordered) = (0usize, 0usize, 0usize, 0usize);
    for qi in 0..440 {
        let n = if qi < 400 {
            rng.range(2, 5) as usize
        } else {
            6
        };
        let q = random_query(&mut rng, n);
        let db = gen_db(&q, &mut rng);

        let mut with = Optimizer::new(&q, true);
        let plan = with.run();
        let mut without = Optimizer::new(&q, false);
        let plan_full = without.run();

        if n <= 5 {
            let (bf, trees) = brute_force(&q); // (a) brute force over every bushy tree
            assert!(
                close(plan.cost, bf),
                "q{qi}: memo cost {} != brute force {bf}",
                plan.cost
            );
            trees_total += trees;
        }
        assert!(
            close(plan.cost, plan_full.cost),
            "q{qi}: pruning changed the optimum"
        ); // (c)
        assert!(with.stats.costed <= without.stats.costed);
        sum_pruned += with.stats.costed;
        sum_full += without.stats.costed;
        sum_tasks_p += with.stats.tasks;
        sum_tasks_f += without.stats.tasks;
        assert!(
            with.applied.values().all(|&c| c == 1),
            "a rule fired twice on one expression"
        ); // (d)
        assert_eq!(with.stats.rule_fires, with.applied.len());
        assert_eq!(
            with.memo.groups.len(),
            without.memo.groups.len(),
            "exploration must not depend on pruning"
        );

        check_plan_properties(&plan, q.order); // (f) no redundant sorts, orders delivered
        merges += plan.count(&|o| matches!(o, POp::Merge { .. }));
        sorts += plan.count(&|o| matches!(o, POp::Sort { .. }));
        ordered += usize::from(q.order.is_some());

        let reference = sorted(naive(&q, &db)); // (e) execute and compare with the unoptimised plan
        let got = exec(&plan, &q, &db);
        if let Some(c) = q.order {
            assert!(
                got.windows(2).all(|w| w[0][c] <= w[1][c]),
                "result not ordered"
            );
        }
        assert_eq!(
            sorted(got),
            reference,
            "q{qi}: optimised plan returns different rows"
        );
        assert_eq!(sorted(exec(&plan_full, &q, &db)), reference);
    }
    println!("\n== 440 random queries (400 with n in 2..=5 vs brute force, 40 with n = 6) ==");
    println!(
        "  optimum == brute force on 400 queries ({trees_total} join trees enumerated); executed results == naive reference on 440"
    );
    println!(
        "  physical expressions costed: pruned {sum_pruned} vs unpruned {sum_full} ({:.1}% fewer); tasks {sum_tasks_p} vs {sum_tasks_f}",
        100.0 * (1.0 - sum_pruned as f64 / sum_full as f64)
    );
    println!(
        "  chosen plans: {merges} MergeJoins, {sorts} Sort enforcers across {ordered} ordered queries ({:?})",
        t0.elapsed()
    );
    assert!(
        sum_pruned < sum_full,
        "branch-and-bound must prune something"
    );
    assert!(merges > 0 && sorts > 0);

    check_interesting_orders();
    println!("\nall cascades_memo_optimizer checks passed");
}

fn demo() {
    // four tables; filters on t0 (indexed column) and t3; ORDER BY t1.c0
    let q = Query::new(
        vec![2000.0, 500.0, 20_000.0, 80.0],
        vec![[1000, 50, 50], [500, 40, 40], [1000, 100, 10], [80, 8, 8]],
        &[(0, 1, 1, 1), (1, 0, 2, 0), (2, 1, 3, 1)],
        &[(0, 0, 10, 40), (3, 2, 1, 2)],
        Some(col_id(1, 0)),
    );
    println!(
        "== EXPLAIN: t0 JOIN t1 JOIN t2 JOIN t3, filters on t0.c0 and t3.c2, ORDER BY t1.c0 =="
    );
    let mut o = Optimizer::new(&q, true);
    let t0 = Instant::now();
    let plan = o.run();
    let mut s = String::new();
    plan.explain(&q, 1, &mut s);
    print!("{s}");
    println!(
        "  search: {} groups, {} logical exprs, {} tasks, {} rule firings, {} physical exprs costed, {} pruned ({:?})",
        o.memo.groups.len(),
        o.memo.groups.iter().map(|g| g.exprs.len()).sum::<usize>(),
        o.stats.tasks,
        o.stats.rule_fires,
        o.stats.costed,
        o.stats.pruned,
        t0.elapsed()
    );

    let q3 = Query::shaped(3, true, 100.0);
    let mut o3 = Optimizer::new(&q3, true);
    o3.run();
    println!(
        "\n== the memo of a 3-clique (7 groups, expressions are group references) ==\n{}",
        group_dump(&o3, 12)
    );
}

fn check_counts_and_closed_forms() {
    println!("\n== (b) memo size: groups of an n-way join, and brute-force tree counts ==");
    println!(
        "  {:>2} {:>9} {:>10} {:>9} {:>10} {:>14} {:>14}",
        "n", "clique", "2^n-1", "chain", "n(n+1)/2", "exprs (clique)", "costed pr/full"
    );
    for n in 2..=7usize {
        let (qc, qp) = (
            Query::shaped(n, true, 100.0),
            Query::shaped(n, false, 100.0),
        );
        let (mut oc, mut op) = (Optimizer::new(&qc, true), Optimizer::new(&qp, true));
        oc.run();
        op.run();
        let mut of = Optimizer::new(&qc, false);
        of.run();
        let exprs: usize = oc.memo.groups.iter().map(|g| g.exprs.len()).sum();
        println!(
            "  {n:>2} {:>9} {:>10} {:>9} {:>10} {exprs:>14} {:>6}/{:<7}",
            oc.memo.groups.len(),
            (1usize << n) - 1,
            op.memo.groups.len(),
            n * (n + 1) / 2,
            oc.stats.costed,
            of.stats.costed
        );
        assert_eq!(
            oc.memo.groups.len(),
            (1 << n) - 1,
            "clique: one group per non-empty subset"
        );
        assert_eq!(
            op.memo.groups.len(),
            n * (n + 1) / 2,
            "chain: one group per connected interval"
        );
        assert!(oc.stats.costed <= of.stats.costed);
        assert!(oc.applied.values().all(|&c| c == 1));
    }
    // tree counts: ordered bushy trees of a clique = (2n-2)!/(n-1)!, of a chain = 2^(n-1) * Catalan(n-1)
    for (n, clique, expect) in [
        (3usize, true, 12usize),
        (4, true, 120),
        (5, true, 1680),
        (4, false, 40),
        (5, false, 224),
    ] {
        let (_, trees) = brute_force(&Query::shaped(n, clique, 100.0));
        assert_eq!(trees, expect, "tree count n={n} clique={clique}");
    }
    println!(
        "  brute-force tree counts: clique 12/120/1680 (n=3,4,5), chain 40/224 (n=4,5): match the closed forms"
    );
}

fn check_interesting_orders() {
    println!("\n== (f) interesting orders vs the Sort enforcer ==");
    let mk = |order: Req| {
        Query::new(
            vec![10_000.0; 2],
            vec![[10_000; NCOL]; 2],
            &[(0, 0, 1, 0)],
            &[],
            order,
        )
    };
    for (label, order) in [
        ("no ORDER BY", None),
        ("ORDER BY t0.c0 (indexed)", Some(col_id(0, 0))),
        ("ORDER BY t0.c1 (not indexed)", Some(col_id(0, 1))),
    ] {
        let q = mk(order);
        let plan = Optimizer::new(&q, true).run();
        let mut s = String::new();
        plan.explain(&q, 1, &mut s);
        println!("  {label}:\n{s}");
        check_plan_properties(&plan, order);
        let (bf, _) = brute_force(&q);
        assert!(close(plan.cost, bf));
        let sorts = plan.count(&|o| matches!(o, POp::Sort { .. }));
        match order {
            Some(c) if c == col_id(0, 1) => {
                assert_eq!(sorts, 1, "no index delivers c1: the enforcer is required");
                assert!(matches!(plan.op, POp::Sort { .. }));
            }
            _ => {
                assert_eq!(
                    sorts, 0,
                    "both inputs come sorted from the indexes: the MergeJoin needs no Sort at all"
                );
                assert!(matches!(plan.op, POp::Merge { .. }));
                assert!(plan.kids.iter().all(|k| matches!(k.op, POp::Index { .. })));
            }
        }
    }
    // the enforcer's price is exactly the difference between the two requirements
    let none = Optimizer::new(&mk(None), true).run().cost;
    let c1 = Optimizer::new(&mk(Some(col_id(0, 1))), true).run().cost;
    let out_rows = 10_000.0 * 10_000.0 / 10_000.0;
    assert!(
        close(c1 - none, cost::sort(out_rows)),
        "enforcer cost mismatch"
    );
    println!(
        "  cost without order {none:.0}, with t0.c1 order {c1:.0}: difference {:.0} = sort({out_rows:.0})",
        c1 - none
    );
}
