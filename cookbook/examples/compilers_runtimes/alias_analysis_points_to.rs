//! RECIPE 822 (compilers): pointer / alias analysis: Andersen (inclusion), Steensgaard (unification), field, flow and context sensitivity.
//! LEVEL: advanced
//! Use in: optimisers that must know whether two memory operations may touch the same cell, escape analysis, call graph construction, bug finders.   Run: cargo run --release --example alias_analysis_points_to
#![allow(dead_code)]
//!
//! QUESTION: "can `*p` and `*q` name the same memory?" Everything that reorders, hoists or removes a load/store needs it.
//! IR: a heap language with `x = new` (allocation SITE), `x = null`, `x = y`, `x = y.f`, `x.f = y`, `x = call f(y)` and a
//! non-deterministic `if`. Objects have NF=2 fields that hold pointers. A tiny builder + line parser creates the demos.
//! ANALYSES (all start from the same list of four constraint kinds, `Addr`, `Copy`, `Load`, `Store`):
//!  * ANDERSEN (inclusion based, O(n^3) worst case): `x = y` means pts(x) CONTAINS pts(y). Solved on a constraint graph with a
//!    worklist. DIFFERENCE PROPAGATION: a node pushes only the objects that are new since its last visit. CYCLE ELIMINATION: nodes
//!    on a copy-edge cycle have equal sets, so a Tarjan SCC pass collapses them into one union-find representative (we run it before
//!    solving and whenever the worklist drains, "periodic" collapse; production solvers detect cycles lazily).
//!  * STEENSGAARD (unification based, almost linear): `x = y` UNIFIES pts(x) with pts(y) in a union-find; the two classes become
//!    one, recursively merging what they point to. Fast, coarser: `p = &a; q = &b; r = p; r = q;` makes a and b one class.
//!  * FIELD SENSITIVITY on/off: with it off every field index is folded to 0 (one summary cell per object).
//!  * FLOW SENSITIVITY: a per-statement state (var -> set, (object, field) -> set). A store through a pointer that MUST target a
//!    single non-null object is a STRONG UPDATE (overwrite); otherwise WEAK (union). The special element NULL is tracked precisely
//!    so we know when the singleton is really non-null (storing through null is a no-op in the concrete semantics, so a strong
//!    update through a maybe-null pointer would be unsound). `if` joins by union. No loops here, so each site runs at most once and
//!    one abstract object per (site, calling context) really is one concrete object.
//!  * HEAP ABSTRACTION by allocation site: every object created by the same `new` is one abstract object.
//!  * CONTEXT SENSITIVITY (k-CFA, k=0 or 1): with k=1 a function body is analysed once per call site (locals AND heap objects it
//!    allocates are cloned per call site, "heap cloning"), so `id(a)` and `id(b)` stop contaminating each other.
//! VERIFICATION: a concrete interpreter runs 1500 random programs (random branch choices). At EVERY main-level statement and at the
//! end: every concrete pointer value is in the abstract set, every equal concrete pair is reported may-alias, every concrete
//! field value is in the abstract field set (flow-sensitive: also null-ness). Precision lattice asserted on every program:
//! flow-sensitive(end) <= Andersen k=1 <= Andersen k=0 <= Steensgaard (as sets of may-alias pairs), field-sens <= field-insens.
//! Then a timing table on large generated programs: difference propagation and cycle elimination on/off vs Steensgaard.
//! RUST: `&mut T` is guaranteed unique and `&T` guarantees no mutation behind it (except UnsafeCell). Rustc therefore emits LLVM
//! `noalias` on such parameters: the optimiser gets, for free and from the TYPE SYSTEM, the answer a points-to analysis has to
//! compute expensively (and conservatively across function boundaries). C needs `restrict` and trust. Result: Rust can keep values
//! in registers across stores, vectorise `a[i] = b[i] + 1` without runtime overlap checks, and reorder loads (see main()).
//! COMMON USES: LLVM's alias analysis stack (BasicAA, TBAA, plus pointer-analysis research tools), Doop and Soot/SPARK (Java
//! call-graph and taint analyses use Andersen-style points-to), escape analysis in JIT compilers, Rust's noalias metadata.
//! Read: Andersen, "Program Analysis and Specialization for the C Programming Language" (PhD thesis 1994); Steensgaard,
//! "Points-to Analysis in Almost Linear Time" (POPL 1996), cited by title only (not opened); Smaragdakis and Balatsouras, "Pointer
//! Analysis" https://yanniss.github.io/points-to-tutorial15.pdf ; LLVM LangRef (noalias) https://llvm.org/docs/LangRef.html ;
//! https://doc.rust-lang.org/nomicon/aliasing.html ; https://en.wikipedia.org/wiki/Alias_analysis
//! TRY NEXT:
//!   - [S] `demo_field` runs only Andersen. Predict whether field-sensitive and field-insensitive Steensgaard report x3/x4
//!     as aliases, then add the two `run_steens` calls and check.
//!   - [S] In `Andersen::run` in `timing_table`, switch off only `diff` (keep cycle elimination) for the 900-statement program: predict `moved`,
//!     then compare it with both table columns.
//!   - [M] Make `collapse` lazy: detect a cycle only when `add_edge` closes one (search from `b` back to `a`); count how many SCC passes you save.
//!   - [M] Extend `fs_exec` with a `while` loop: iterate to a fixpoint, and note why strong updates now need a "single concrete object" check.
//!   - [A] PROJECT: feed the may-alias pairs into `loop_optimizations` so its LICM refuses to hoist a `Load` past a store that may alias, then measure hoists lost per analysis.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::time::Instant;

const NF: usize = 2;
const NULL: i64 = -1;

// ---------------------------------------------------------------- rng
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

// ---------------------------------------------------------------- IR
type Var = usize;
#[derive(Clone, Debug, PartialEq)]
enum Op {
    New { x: Var, site: u32 },
    Null { x: Var },
    Copy { x: Var, y: Var },
    Load { x: Var, y: Var, f: usize },
    Store { x: Var, f: usize, y: Var },
    Call { x: Var, func: usize, arg: Var },
    If(Vec<Stmt>, Vec<Stmt>),
}
#[derive(Clone, Debug, PartialEq)]
struct Stmt {
    id: usize,
    op: Op,
}
/// Locals: 0 = parameter, 1 = return value, the rest are temporaries. Bodies contain no calls and no `if`.
struct Func {
    nlocals: usize,
    body: Vec<Stmt>,
}
struct Prog {
    nvars: usize,
    funcs: Vec<Func>,
    body: Vec<Stmt>,
}

/// Hands out unique statement ids and allocation sites.
#[derive(Default)]
struct B {
    id: usize,
    site: u32,
}
impl B {
    fn st(&mut self, op: Op) -> Stmt {
        self.id += 1;
        Stmt { id: self.id, op }
    }
    fn new_obj(&mut self, x: Var) -> Stmt {
        self.site += 1;
        let site = self.site;
        self.st(Op::New { x, site })
    }
}

fn var(s: &str) -> Var {
    s.trim().trim_start_matches('x').parse().expect("var")
}
/// Line parser: `x0 = new`, `x0 = null`, `x1 = x0`, `x1 = x0.1`, `x0.1 = x1`, `x2 = call f0(x1)`.
fn parse(src: &str, b: &mut B) -> Vec<Stmt> {
    let mut out = vec![];
    for line in src.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (lhs, rhs) = line.split_once(" = ").expect("assignment");
        let op = if let Some((x, f)) = lhs.split_once('.') {
            Op::Store {
                x: var(x),
                f: f.parse().unwrap(),
                y: var(rhs),
            }
        } else if rhs == "new" {
            out.push(b.new_obj(var(lhs)));
            continue;
        } else if rhs == "null" {
            Op::Null { x: var(lhs) }
        } else if let Some(call) = rhs.strip_prefix("call f") {
            let (f, arg) = call.split_once('(').unwrap();
            Op::Call {
                x: var(lhs),
                func: f.parse().unwrap(),
                arg: var(arg.trim_end_matches(')')),
            }
        } else if let Some((y, f)) = rhs.split_once('.') {
            Op::Load {
                x: var(lhs),
                y: var(y),
                f: f.parse().unwrap(),
            }
        } else {
            Op::Copy {
                x: var(lhs),
                y: var(rhs),
            }
        };
        out.push(b.st(op));
    }
    out
}

fn show(stmts: &[Stmt], ind: usize, out: &mut String) {
    let pad = " ".repeat(ind);
    for s in stmts {
        match &s.op {
            Op::New { x, site } => out.push_str(&format!("{pad}x{x} = new o{site}\n")),
            Op::Null { x } => out.push_str(&format!("{pad}x{x} = null\n")),
            Op::Copy { x, y } => out.push_str(&format!("{pad}x{x} = x{y}\n")),
            Op::Load { x, y, f } => out.push_str(&format!("{pad}x{x} = x{y}.{f}\n")),
            Op::Store { x, f, y } => out.push_str(&format!("{pad}x{x}.{f} = x{y}\n")),
            Op::Call { x, func, arg } => {
                out.push_str(&format!("{pad}x{x} = call f{func}(x{arg})\n"))
            }
            Op::If(a, b) => {
                out.push_str(&format!("{pad}if ? {{\n"));
                show(a, ind + 2, out);
                out.push_str(&format!("{pad}}} else {{\n"));
                show(b, ind + 2, out);
                out.push_str(&format!("{pad}}}\n"));
            }
        }
    }
}

fn make_func(kind: usize, b: &mut B) -> Func {
    match kind {
        0 => Func {
            nlocals: 2,
            body: vec![b.st(Op::Copy { x: 1, y: 0 })],
        }, // identity
        1 => Func {
            // wrap: returns a fresh box holding the argument
            nlocals: 3,
            body: vec![
                b.new_obj(2),
                b.st(Op::Store { x: 2, f: 0, y: 0 }),
                b.st(Op::Copy { x: 1, y: 2 }),
            ],
        },
        2 => Func {
            nlocals: 2,
            body: vec![b.st(Op::Load { x: 1, y: 0, f: 0 })],
        }, // getter
        _ => Func {
            nlocals: 2,
            body: vec![b.new_obj(1)],
        }, // factory
    }
}

fn gen_stmts(
    r: &mut Rng,
    b: &mut B,
    nvars: usize,
    n: usize,
    nfuncs: usize,
    depth: usize,
    call_pct: u64,
) -> Vec<Stmt> {
    let mut v = vec![];
    for _ in 0..n {
        let x = r.below(nvars as u64) as usize;
        let y = r.below(nvars as u64) as usize;
        let f = r.below(NF as u64) as usize;
        let roll = r.below(100);
        let st = match roll {
            0..=17 => b.new_obj(x),
            18..=19 => b.st(Op::Null { x }),
            20..=39 => b.st(Op::Copy { x, y }),
            40..=59 => b.st(Op::Load { x, y, f }),
            60..=81 => b.st(Op::Store { x, f, y }),
            82..=93 if nfuncs > 0 && r.chance(call_pct * 100 / 12 + 1) => {
                let func = r.below(nfuncs as u64) as usize;
                b.st(Op::Call { x, func, arg: y })
            }
            94..=99 if depth == 0 && call_pct > 0 => {
                let na = 1 + r.below(3) as usize;
                let a = gen_stmts(r, b, nvars, na, nfuncs, 1, call_pct);
                let nc = 1 + r.below(3) as usize;
                let c = gen_stmts(r, b, nvars, nc, nfuncs, 1, call_pct);
                b.st(Op::If(a, c))
            }
            _ => b.st(Op::Copy { x, y }),
        };
        v.push(st);
    }
    v
}

fn gen_prog(r: &mut Rng, nvars: usize, nstmts: usize, call_pct: u64) -> Prog {
    let mut b = B::default();
    let funcs: Vec<Func> = (0..3)
        .map(|_| make_func(r.below(4) as usize, &mut b))
        .collect();
    let body = gen_stmts(r, &mut b, nvars, nstmts, funcs.len(), 0, call_pct);
    Prog { nvars, funcs, body }
}

// ---------------------------------------------------------------- concrete semantics (the oracle)
#[derive(Clone)]
struct CObj {
    site: u32,
    ctx: u32,
    f: [Option<usize>; NF],
}
type Obs<'a> = &'a mut dyn FnMut(usize, &[Option<usize>], &[CObj]);

fn exec(
    prog: &Prog,
    stmts: &[Stmt],
    vars: &mut Vec<Option<usize>>,
    heap: &mut Vec<CObj>,
    ctx: u32,
    rng: &mut Rng,
    obs: Obs,
) {
    for s in stmts {
        if ctx == 0 {
            obs(s.id, vars, heap); // state BEFORE the statement, main frame only
        }
        match &s.op {
            Op::New { x, site } => {
                heap.push(CObj {
                    site: *site,
                    ctx,
                    f: [None; NF],
                });
                vars[*x] = Some(heap.len() - 1);
            }
            Op::Null { x } => vars[*x] = None,
            Op::Copy { x, y } => vars[*x] = vars[*y],
            Op::Load { x, y, f } => vars[*x] = vars[*y].and_then(|o| heap[o].f[*f]),
            Op::Store { x, f, y } => {
                if let Some(o) = vars[*x] {
                    heap[o].f[*f] = vars[*y]; // storing through null is a no-op (keeps the oracle total)
                }
            }
            Op::Call { x, func, arg } => {
                let fu = &prog.funcs[*func];
                let mut loc = vec![None; fu.nlocals];
                loc[0] = vars[*arg];
                exec(prog, &fu.body, &mut loc, heap, s.id as u32 + 1, rng, obs);
                vars[*x] = loc[1];
            }
            Op::If(a, b) => {
                let arm = if rng.chance(50) { a } else { b };
                exec(prog, arm, vars, heap, ctx, rng, obs);
            }
        }
    }
}

// ---------------------------------------------------------------- constraint generation
#[derive(Clone, Copy, Debug)]
enum Con {
    Addr(u32, u32),
    Copy(u32, u32),
    Load(u32, u32, usize),
    Store(u32, usize, u32),
}
#[derive(Default)]
struct Cons {
    cons: Vec<Con>,
    vars: HashMap<(usize, u32, usize), u32>, // (function+1 or 0 for main, ctx, local) -> node
    objs: HashMap<(u32, u32), u32>,          // (site, ctx) -> object
    nvars: u32,
    fs: bool,
    k: usize,
    seen: HashSet<(usize, u32)>,
}
impl Cons {
    fn node(&mut self, fid: usize, ctx: u32, v: usize) -> u32 {
        let n = self.nvars;
        let id = *self.vars.entry((fid, ctx, v)).or_insert(n);
        if id == n {
            self.nvars += 1;
        }
        id
    }
    fn obj(&mut self, site: u32, ctx: u32) -> u32 {
        let c = if self.k == 0 { 0 } else { ctx };
        let n = self.objs.len() as u32;
        *self.objs.entry((site, c)).or_insert(n)
    }
    fn fld(&self, f: usize) -> usize {
        if self.fs { f } else { 0 }
    }
    fn walk(&mut self, prog: &Prog, stmts: &[Stmt], fid: usize, ctx: u32) {
        for s in stmts {
            match &s.op {
                Op::New { x, site } => {
                    let (n, o) = (self.node(fid, ctx, *x), self.obj(*site, ctx));
                    self.cons.push(Con::Addr(n, o));
                }
                Op::Null { .. } => {}
                Op::Copy { x, y } => {
                    let (a, b) = (self.node(fid, ctx, *x), self.node(fid, ctx, *y));
                    self.cons.push(Con::Copy(a, b));
                }
                Op::Load { x, y, f } => {
                    let (a, b, f) = (
                        self.node(fid, ctx, *x),
                        self.node(fid, ctx, *y),
                        self.fld(*f),
                    );
                    self.cons.push(Con::Load(a, b, f));
                }
                Op::Store { x, f, y } => {
                    let (a, b, f) = (
                        self.node(fid, ctx, *x),
                        self.node(fid, ctx, *y),
                        self.fld(*f),
                    );
                    self.cons.push(Con::Store(a, f, b));
                }
                Op::Call { x, func, arg } => {
                    let cctx = if self.k == 0 { 0 } else { s.id as u32 + 1 };
                    let (callee, a, xr) =
                        (func + 1, self.node(fid, ctx, *arg), self.node(fid, ctx, *x));
                    let (param, ret) = (self.node(callee, cctx, 0), self.node(callee, cctx, 1));
                    self.cons.push(Con::Copy(param, a));
                    self.cons.push(Con::Copy(xr, ret));
                    if self.seen.insert((callee, cctx)) {
                        self.walk(prog, &prog.funcs[*func].body, callee, cctx);
                    }
                }
                Op::If(a, b) => {
                    self.walk(prog, a, fid, ctx);
                    self.walk(prog, b, fid, ctx);
                }
            }
        }
    }
}
fn gen_cons(prog: &Prog, k: usize, fs: bool) -> Cons {
    let mut c = Cons {
        fs,
        k,
        ..Default::default()
    };
    for v in 0..prog.nvars {
        c.node(0, 0, v); // main variables get nodes even if unused, so queries never miss
    }
    c.walk(prog, &prog.body, 0, 0);
    c
}

// ---------------------------------------------------------------- Andersen
#[derive(Default, Clone, Copy)]
struct AStats {
    moved: u64,
    merged: u32,
    pops: u64,
    nodes: usize,
}
struct Andersen {
    parent: Vec<u32>,
    pts: Vec<BTreeSet<u32>>,
    delta: Vec<BTreeSet<u32>>,
    succ: Vec<BTreeSet<u32>>,
    loads: Vec<Vec<(u32, usize)>>,
    stores: Vec<Vec<(usize, u32)>>,
    fields: HashMap<(u32, usize), u32>,
    work: VecDeque<u32>,
    queued: Vec<bool>,
    diff: bool,
    st: AStats,
}
impl Andersen {
    fn new(diff: bool) -> Self {
        Andersen {
            parent: vec![],
            pts: vec![],
            delta: vec![],
            succ: vec![],
            loads: vec![],
            stores: vec![],
            fields: HashMap::new(),
            work: VecDeque::new(),
            queued: vec![],
            diff,
            st: AStats::default(),
        }
    }
    fn new_node(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        self.pts.push(BTreeSet::new());
        self.delta.push(BTreeSet::new());
        self.succ.push(BTreeSet::new());
        self.loads.push(vec![]);
        self.stores.push(vec![]);
        self.queued.push(false);
        id
    }
    fn find(&mut self, mut n: u32) -> u32 {
        let mut r = n;
        while self.parent[r as usize] != r {
            r = self.parent[r as usize];
        }
        while self.parent[n as usize] != r {
            let nx = self.parent[n as usize];
            self.parent[n as usize] = r;
            n = nx;
        }
        r
    }
    fn enqueue(&mut self, n: u32) {
        if !self.queued[n as usize] {
            self.queued[n as usize] = true;
            self.work.push_back(n);
        }
    }
    fn add_pts(&mut self, n: u32, o: u32) {
        let n = self.find(n);
        if self.pts[n as usize].insert(o) {
            self.delta[n as usize].insert(o);
            self.enqueue(n);
        }
    }
    /// pts(b) >= pts(a). A NEW edge must carry everything `a` already knows (the delta only covers future news).
    fn add_edge(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b && self.succ[a as usize].insert(b) {
            let all: Vec<u32> = self.pts[a as usize].iter().copied().collect();
            for o in all {
                self.st.moved += 1;
                self.add_pts(b, o);
            }
        }
    }
    fn field_node(&mut self, o: u32, f: usize) -> u32 {
        if let Some(&n) = self.fields.get(&(o, f)) {
            return n;
        }
        let n = self.new_node();
        self.fields.insert((o, f), n);
        n
    }
    fn run(&mut self, cycle: bool) {
        if cycle {
            self.collapse();
        }
        loop {
            while let Some(n) = self.work.pop_front() {
                self.queued[n as usize] = false;
                if self.find(n) != n {
                    continue; // merged away; its rep was enqueued with the full set
                }
                self.st.pops += 1;
                let d: Vec<u32> = if self.diff {
                    std::mem::take(&mut self.delta[n as usize])
                        .into_iter()
                        .collect()
                } else {
                    self.delta[n as usize].clear();
                    self.pts[n as usize].iter().copied().collect()
                };
                if d.is_empty() {
                    continue;
                }
                for (x, f) in self.loads[n as usize].clone() {
                    for &o in &d {
                        let fnode = self.field_node(o, f);
                        self.add_edge(fnode, x);
                    }
                }
                for (f, y) in self.stores[n as usize].clone() {
                    for &o in &d {
                        let fnode = self.field_node(o, f);
                        self.add_edge(y, fnode);
                    }
                }
                for s in self.succ[n as usize].clone() {
                    let s = self.find(s);
                    if s == n {
                        continue;
                    }
                    self.st.moved += d.len() as u64;
                    for &o in &d {
                        self.add_pts(s, o);
                    }
                }
            }
            if !(cycle && self.collapse()) {
                break;
            }
        }
    }
    /// Tarjan SCC over the copy graph of representatives; every non-trivial SCC becomes one node.
    fn collapse(&mut self) -> bool {
        let n = self.parent.len();
        let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
        for a in 0..n {
            if self.parent[a] as usize != a {
                continue;
            }
            let ss: Vec<u32> = self.succ[a].iter().copied().collect();
            for s in ss {
                let s = self.find(s) as usize;
                if s != a {
                    adj[a].push(s);
                }
            }
        }
        let (mut index, mut low) = (vec![usize::MAX; n], vec![0usize; n]);
        let (mut on, mut stack, mut sccs, mut idx) =
            (vec![false; n], vec![], Vec::<Vec<usize>>::new(), 0usize);
        for root in 0..n {
            if self.parent[root] as usize != root || index[root] != usize::MAX {
                continue;
            }
            index[root] = idx;
            low[root] = idx;
            idx += 1;
            stack.push(root);
            on[root] = true;
            let mut call = vec![(root, 0usize)];
            while let Some(&(v, i)) = call.last() {
                if i < adj[v].len() {
                    call.last_mut().unwrap().1 += 1;
                    let w = adj[v][i];
                    if index[w] == usize::MAX {
                        index[w] = idx;
                        low[w] = idx;
                        idx += 1;
                        stack.push(w);
                        on[w] = true;
                        call.push((w, 0));
                    } else if on[w] {
                        low[v] = low[v].min(index[w]);
                    }
                } else {
                    call.pop();
                    if let Some(&(p, _)) = call.last() {
                        low[p] = low[p].min(low[v]);
                    }
                    if low[v] == index[v] {
                        let mut comp = vec![];
                        loop {
                            let w = stack.pop().unwrap();
                            on[w] = false;
                            comp.push(w);
                            if w == v {
                                break;
                            }
                        }
                        if comp.len() > 1 {
                            sccs.push(comp);
                        }
                    }
                }
            }
        }
        let any = !sccs.is_empty();
        for comp in sccs {
            let r = comp[0];
            for &m in &comp[1..] {
                self.parent[m] = r as u32;
                self.st.merged += 1;
                let (p, s) = (
                    std::mem::take(&mut self.pts[m]),
                    std::mem::take(&mut self.succ[m]),
                );
                let (l, t) = (
                    std::mem::take(&mut self.loads[m]),
                    std::mem::take(&mut self.stores[m]),
                );
                self.pts[r].extend(p);
                self.succ[r].extend(s);
                self.loads[r].extend(l);
                self.stores[r].extend(t);
            }
            for &m in &comp {
                self.succ[r].remove(&(m as u32));
            }
            self.delta[r] = self.pts[r].clone(); // new members must be re-run against all loads/stores
            self.enqueue(r as u32);
        }
        any
    }
}

struct AndRes {
    pts: Vec<BTreeSet<u32>>, // per main variable
    pairs: BTreeSet<(usize, usize)>,
    fieldpts: HashMap<(u32, usize), BTreeSet<u32>>,
    st: AStats,
}
fn pairs_of(n: usize, mut f: impl FnMut(usize, usize) -> bool) -> BTreeSet<(usize, usize)> {
    let mut s = BTreeSet::new();
    for x in 0..n {
        for y in x + 1..n {
            if f(x, y) {
                s.insert((x, y));
            }
        }
    }
    s
}
fn run_andersen(c: &Cons, nmain: usize, diff: bool, cycle: bool) -> AndRes {
    let mut a = Andersen::new(diff);
    for _ in 0..c.nvars {
        a.new_node();
    }
    for con in &c.cons {
        match *con {
            Con::Addr(x, o) => a.add_pts(x, o),
            Con::Copy(x, y) => a.add_edge(y, x),
            Con::Load(x, y, f) => a.loads[y as usize].push((x, f)),
            Con::Store(x, f, y) => a.stores[x as usize].push((f, y)),
        }
    }
    a.run(cycle);
    let pts: Vec<BTreeSet<u32>> = (0..nmain)
        .map(|v| {
            let n = a.find(c.vars[&(0, 0, v)]);
            a.pts[n as usize].clone()
        })
        .collect();
    let fkeys: Vec<((u32, usize), u32)> = a.fields.iter().map(|(k, v)| (*k, *v)).collect();
    let mut fieldpts = HashMap::new();
    for (k, n) in fkeys {
        let r = a.find(n);
        fieldpts.insert(k, a.pts[r as usize].clone());
    }
    a.st.nodes = a.parent.len();
    let pairs = pairs_of(nmain, |x, y| pts[x].intersection(&pts[y]).next().is_some());
    AndRes {
        pts,
        pairs,
        fieldpts,
        st: a.st,
    }
}

// ---------------------------------------------------------------- Steensgaard
struct Steens {
    parent: Vec<u32>,
    pts: Vec<Option<u32>>,
    fields: Vec<BTreeMap<usize, u32>>,
}
impl Steens {
    fn fresh(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        self.pts.push(None);
        self.fields.push(BTreeMap::new());
        id
    }
    fn find(&mut self, mut n: u32) -> u32 {
        let mut r = n;
        while self.parent[r as usize] != r {
            r = self.parent[r as usize];
        }
        while self.parent[n as usize] != r {
            let nx = self.parent[n as usize];
            self.parent[n as usize] = r;
            n = nx;
        }
        r
    }
    /// Union two classes and, recursively, what they point to / their fields (that recursion is the whole algorithm).
    fn join(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        if a == b {
            return;
        }
        self.parent[b as usize] = a;
        let pb = self.pts[b as usize].take();
        let fb = std::mem::take(&mut self.fields[b as usize]);
        match (self.pts[a as usize], pb) {
            (Some(x), Some(y)) => self.join(x, y),
            (None, Some(y)) => self.pts[a as usize] = Some(y),
            _ => {}
        }
        for (f, c) in fb {
            let ra = self.find(a);
            let cur = self.fields[ra as usize].get(&f).copied();
            match cur {
                Some(x) => self.join(x, c),
                None => {
                    let r = self.find(a);
                    self.fields[r as usize].insert(f, c);
                }
            }
        }
    }
    fn ensure_pts(&mut self, c: u32) -> u32 {
        let c = self.find(c);
        if let Some(p) = self.pts[c as usize] {
            return p;
        }
        let p = self.fresh();
        let c = self.find(c);
        self.pts[c as usize] = Some(p);
        p
    }
    fn ensure_field(&mut self, c: u32, f: usize) -> u32 {
        let c = self.find(c);
        if let Some(&x) = self.fields[c as usize].get(&f) {
            return x;
        }
        let x = self.fresh();
        let c = self.find(c);
        self.fields[c as usize].insert(f, x);
        x
    }
}
/// Returns (may-alias pairs, number of equivalence classes among variables+objects).
fn run_steens(c: &Cons, nmain: usize) -> (BTreeSet<(usize, usize)>, usize) {
    let mut s = Steens {
        parent: vec![],
        pts: vec![],
        fields: vec![],
    };
    for _ in 0..c.nvars + c.objs.len() as u32 {
        s.fresh();
    }
    let obj_class = |o: u32| c.nvars + o;
    for con in &c.cons {
        match *con {
            Con::Addr(x, o) => {
                let p = s.ensure_pts(x);
                s.join(p, obj_class(o));
            }
            Con::Copy(x, y) => {
                let (p, q) = (s.ensure_pts(x), s.ensure_pts(y));
                s.join(p, q);
            }
            Con::Load(x, y, f) => {
                let yp = s.ensure_pts(y);
                let fc = s.ensure_field(yp, f);
                let (a, b) = (s.ensure_pts(x), s.ensure_pts(fc));
                s.join(a, b);
            }
            Con::Store(x, f, y) => {
                let xp = s.ensure_pts(x);
                let fc = s.ensure_field(xp, f);
                let (a, b) = (s.ensure_pts(fc), s.ensure_pts(y));
                s.join(a, b);
            }
        }
    }
    let mut cls: Vec<Option<u32>> = vec![];
    for v in 0..nmain {
        let n = c.vars[&(0, 0, v)];
        let rn = s.find(n);
        let p = s.pts[rn as usize].map(|p| s.find(p));
        cls.push(p);
    }
    let pairs = pairs_of(nmain, |x, y| cls[x].is_some() && cls[x] == cls[y]);
    let reps: HashSet<u32> = (0..c.nvars + c.objs.len() as u32)
        .map(|n| s.find(n))
        .collect();
    (pairs, reps.len())
}

// ---------------------------------------------------------------- flow-sensitive (strong updates)
type PSet = BTreeSet<i64>;
type Heap = BTreeMap<(i64, usize), PSet>;
fn okey(site: u32, ctx: u32) -> i64 {
    site as i64 * 100_000 + ctx as i64
}
fn null_set() -> PSet {
    std::iter::once(NULL).collect()
}
fn join_state(a: (Vec<PSet>, Heap), b: (Vec<PSet>, Heap)) -> (Vec<PSet>, Heap) {
    let vars =
        a.0.into_iter()
            .zip(b.0)
            .map(|(mut x, y)| {
                x.extend(y);
                x
            })
            .collect();
    let mut heap = a.1;
    for (k, v) in b.1 {
        heap.entry(k).or_default().extend(v);
    }
    (vars, heap)
}
fn fs_exec(
    prog: &Prog,
    stmts: &[Stmt],
    vars: &mut Vec<PSet>,
    heap: &mut Heap,
    ctx: u32,
    snaps: &mut HashMap<usize, Vec<PSet>>,
) {
    for s in stmts {
        if ctx == 0 {
            snaps.insert(s.id, vars.clone());
        }
        match &s.op {
            Op::New { x, site } => {
                let k = okey(*site, ctx);
                for f in 0..NF {
                    heap.insert((k, f), null_set());
                }
                vars[*x] = std::iter::once(k).collect();
            }
            Op::Null { x } => vars[*x] = null_set(),
            Op::Copy { x, y } => vars[*x] = vars[*y].clone(),
            Op::Load { x, y, f } => {
                let mut r = PSet::new();
                for &o in &vars[*y] {
                    if o == NULL {
                        r.insert(NULL);
                    } else {
                        r.extend(heap.get(&(o, *f)).cloned().unwrap_or_else(null_set));
                    }
                }
                vars[*x] = r;
            }
            Op::Store { x, f, y } => {
                let targets = vars[*x].clone();
                let val = vars[*y].clone();
                let strong = targets.len() == 1 && !targets.contains(&NULL);
                for &o in targets.iter().filter(|&&o| o != NULL) {
                    if strong {
                        heap.insert((o, *f), val.clone()); // STRONG UPDATE: overwrite
                    } else {
                        heap.entry((o, *f)).or_default().extend(val.iter().copied()); // weak
                    }
                }
            }
            Op::Call { x, func, arg } => {
                let fu = &prog.funcs[*func];
                let mut loc = vec![null_set(); fu.nlocals];
                loc[0] = vars[*arg].clone();
                fs_exec(prog, &fu.body, &mut loc, heap, s.id as u32 + 1, snaps);
                vars[*x] = loc[1].clone();
            }
            Op::If(a, b) => {
                let (mut v1, mut h1) = (vars.clone(), heap.clone());
                let (mut v2, mut h2) = (vars.clone(), heap.clone());
                fs_exec(prog, a, &mut v1, &mut h1, ctx, snaps);
                fs_exec(prog, b, &mut v2, &mut h2, ctx, snaps);
                let (v, h) = join_state((v1, h1), (v2, h2));
                *vars = v;
                *heap = h;
            }
        }
    }
}
struct FsRes {
    snaps: HashMap<usize, Vec<PSet>>,
    end: Vec<PSet>,
    heap: Heap,
    pairs: BTreeSet<(usize, usize)>,
}
fn run_flow(prog: &Prog) -> FsRes {
    let mut vars = vec![null_set(); prog.nvars];
    let (mut heap, mut snaps) = (Heap::new(), HashMap::new());
    fs_exec(prog, &prog.body, &mut vars, &mut heap, 0, &mut snaps);
    let pairs = pairs_of(prog.nvars, |x, y| {
        vars[x].iter().any(|o| *o != NULL && vars[y].contains(o))
    });
    FsRes {
        snaps,
        end: vars,
        heap,
        pairs,
    }
}

// ---------------------------------------------------------------- verification
struct Suite {
    a_fs0: AndRes,
    a_nf0: AndRes,
    a_fs1: AndRes,
    c_fs0: Cons,
    c_nf0: Cons,
    c_fs1: Cons,
    st_fs: BTreeSet<(usize, usize)>,
    st_nf: BTreeSet<(usize, usize)>,
    fl: FsRes,
}
fn analyse(p: &Prog) -> Suite {
    let (c_fs0, c_nf0, c_fs1) = (
        gen_cons(p, 0, true),
        gen_cons(p, 0, false),
        gen_cons(p, 1, true),
    );
    Suite {
        a_fs0: run_andersen(&c_fs0, p.nvars, true, true),
        a_nf0: run_andersen(&c_nf0, p.nvars, true, true),
        a_fs1: run_andersen(&c_fs1, p.nvars, true, true),
        st_fs: run_steens(&c_fs0, p.nvars).0,
        st_nf: run_steens(&c_nf0, p.nvars).0,
        fl: run_flow(p),
        c_fs0,
        c_nf0,
        c_fs1,
    }
}
/// Is the concrete object (site, ctx) inside the Andersen set `s` under this config?
fn in_set(c: &Cons, s: &BTreeSet<u32>, o: &CObj) -> bool {
    let key = (o.site, if c.k == 0 { 0 } else { o.ctx });
    c.objs.get(&key).is_some_and(|id| s.contains(id))
}
fn check_andersen(
    c: &Cons,
    r: &AndRes,
    vars: &[Option<usize>],
    heap: &[CObj],
    fin: bool,
    tag: &str,
) {
    for (x, v) in vars.iter().enumerate() {
        if let Some(o) = v {
            assert!(
                in_set(c, &r.pts[x], &heap[*o]),
                "{tag}: x{x} points to o{} but set misses it",
                heap[*o].site
            );
        }
        for y in x + 1..vars.len() {
            if v.is_some() && *v == vars[y] {
                assert!(
                    r.pairs.contains(&(x, y)),
                    "{tag}: concrete alias (x{x},x{y}) not reported"
                );
            }
        }
    }
    if fin {
        for o in heap {
            let key = (o.site, if c.k == 0 { 0 } else { o.ctx });
            let Some(&oid) = c.objs.get(&key) else {
                panic!("{tag}: object never seen by analysis")
            };
            for f in 0..NF {
                if let Some(t) = o.f[f] {
                    let fk = if c.fs { f } else { 0 };
                    let set = r.fieldpts.get(&(oid, fk)).expect("field node exists");
                    assert!(
                        in_set(c, set, &heap[t]),
                        "{tag}: heap field missing a target"
                    );
                }
            }
        }
    }
}
fn check_flow(fl: &FsRes, id: usize, vars: &[Option<usize>], heap: &[CObj]) {
    let st = if id == usize::MAX {
        &fl.end
    } else {
        &fl.snaps[&id]
    };
    let key = |o: &CObj| okey(o.site, o.ctx);
    for (x, v) in vars.iter().enumerate() {
        match v {
            Some(o) => assert!(
                st[x].contains(&key(&heap[*o])),
                "flow: x{x} object missing at stmt {id}"
            ),
            None => assert!(
                st[x].contains(&NULL),
                "flow: x{x} is null but NULL not in set at stmt {id}"
            ),
        }
        for y in x + 1..vars.len() {
            if v.is_some() && *v == vars[y] {
                assert!(
                    st[x].intersection(&st[y]).any(|o| *o != NULL),
                    "flow: alias (x{x},x{y}) lost at stmt {id}"
                );
            }
        }
    }
}
fn check_program(p: &Prog, seed: u64) {
    let su = analyse(p);
    for run in 0..3u64 {
        let mut rng = Rng(seed * 7 + run + 1);
        let (mut vars, mut heap) = (vec![None; p.nvars], vec![]);
        let mut obs = |id: usize, v: &[Option<usize>], h: &[CObj]| {
            check_andersen(&su.c_fs0, &su.a_fs0, v, h, false, "andersen k0 fs");
            check_andersen(&su.c_nf0, &su.a_nf0, v, h, false, "andersen k0 nf");
            check_andersen(&su.c_fs1, &su.a_fs1, v, h, false, "andersen k1 fs");
            check_flow(&su.fl, id, v, h);
            for x in 0..v.len() {
                for y in x + 1..v.len() {
                    if v[x].is_some() && v[x] == v[y] {
                        assert!(
                            su.st_fs.contains(&(x, y)) && su.st_nf.contains(&(x, y)),
                            "steensgaard missed alias"
                        );
                    }
                }
            }
        };
        exec(p, &p.body, &mut vars, &mut heap, 0, &mut rng, &mut obs);
        obs(usize::MAX, &vars, &heap);
        check_andersen(&su.c_fs0, &su.a_fs0, &vars, &heap, true, "andersen k0 fs");
        check_andersen(&su.c_nf0, &su.a_nf0, &vars, &heap, true, "andersen k0 nf");
        check_andersen(&su.c_fs1, &su.a_fs1, &vars, &heap, true, "andersen k1 fs");
        for o in &heap {
            for f in 0..NF {
                let set = su
                    .fl
                    .heap
                    .get(&(okey(o.site, o.ctx), f))
                    .expect("flow heap cell");
                match o.f[f] {
                    Some(t) => assert!(
                        set.contains(&okey(heap[t].site, heap[t].ctx)),
                        "flow heap target missing"
                    ),
                    None => assert!(set.contains(&NULL), "flow heap null missing"),
                }
            }
        }
    }
    // precision lattice
    assert!(
        su.a_fs0.pairs.is_subset(&su.st_fs),
        "Andersen must refine Steensgaard"
    );
    assert!(
        su.a_nf0.pairs.is_subset(&su.st_nf),
        "Andersen (nf) must refine Steensgaard (nf)"
    );
    assert!(
        su.a_fs0.pairs.is_subset(&su.a_nf0.pairs),
        "field sensitivity must not lose precision"
    );
    assert!(
        su.a_fs1.pairs.is_subset(&su.a_fs0.pairs),
        "k=1 must refine k=0"
    );
    assert!(
        su.fl.pairs.is_subset(&su.a_fs1.pairs),
        "flow-sensitive end state must refine flow-insensitive"
    );
}

// ---------------------------------------------------------------- demos
fn describe(prog: &Prog, k: usize, fs: bool) -> Vec<String> {
    let c = gen_cons(prog, k, fs);
    let r = run_andersen(&c, prog.nvars, true, true);
    let inv: HashMap<u32, (u32, u32)> = c.objs.iter().map(|(k, v)| (*v, *k)).collect();
    r.pts
        .iter()
        .enumerate()
        .map(|(x, s)| {
            let names: Vec<String> = s
                .iter()
                .map(|o| {
                    let (site, ctx) = inv[o];
                    if ctx == 0 {
                        format!("o{site}")
                    } else {
                        format!("o{site}@{ctx}")
                    }
                })
                .collect();
            format!("x{x}->{{{}}}", names.join(","))
        })
        .collect()
}
fn demo_program(src: &str, nvars: usize, funcs: Vec<usize>) -> Prog {
    let mut b = B::default();
    let funcs: Vec<Func> = funcs.into_iter().map(|k| make_func(k, &mut b)).collect();
    let body = parse(src, &mut b);
    Prog { nvars, funcs, body }
}
fn demo_unify() {
    println!("-- Andersen vs Steensgaard: p=&a; q=&b; r=p; r=q");
    let p = demo_program("x0 = new\nx1 = new\nx2 = x0\nx2 = x1", 3, vec![]);
    let c = gen_cons(&p, 0, true);
    let a = run_andersen(&c, 3, true, true);
    let (s, classes) = run_steens(&c, 3);
    println!(
        "   Andersen: {}   alias(x0,x1)={}",
        describe(&p, 0, true).join(" "),
        a.pairs.contains(&(0, 1))
    );
    println!(
        "   Steensgaard alias(x0,x1)={} (a and b were unified through r), {classes} classes",
        s.contains(&(0, 1))
    );
    assert!(!a.pairs.contains(&(0, 1)) && s.contains(&(0, 1)));
}
fn demo_field() {
    println!("-- field sensitivity: box.0 = a; box.1 = b; x3 = box.0; x4 = box.1");
    let p = demo_program(
        "x0 = new\nx1 = new\nx2 = new\nx2.0 = x0\nx2.1 = x1\nx3 = x2.0\nx4 = x2.1",
        5,
        vec![],
    );
    for fs in [true, false] {
        let r = run_andersen(&gen_cons(&p, 0, fs), 5, true, true);
        println!(
            "   field-sensitive={fs}: {}  alias(x3,x4)={}",
            describe(&p, 0, fs)[3..].join(" "),
            r.pairs.contains(&(3, 4))
        );
        assert_eq!(r.pairs.contains(&(3, 4)), !fs);
    }
}
fn demo_context() {
    println!("-- context sensitivity: x3 = id(x1); x4 = id(x2)");
    let p = demo_program(
        "x1 = new\nx2 = new\nx3 = call f0(x1)\nx4 = call f0(x2)",
        5,
        vec![0],
    );
    for k in [0usize, 1] {
        let r = run_andersen(&gen_cons(&p, k, true), 5, true, true);
        println!(
            "   k={k}: {}  alias(x3,x4)={}",
            describe(&p, k, true)[3..].join(" "),
            r.pairs.contains(&(3, 4))
        );
        assert_eq!(r.pairs.contains(&(3, 4)), k == 0);
    }
}
fn demo_strong() {
    println!("-- strong update: x2 = a; x2 = b;  box.0 = a; box.0 = b; x4 = box.0");
    let p = demo_program(
        "x0 = new\nx1 = new\nx2 = x0\nx2 = x1\nx3 = new\nx3.0 = x0\nx3.0 = x1\nx4 = x3.0",
        5,
        vec![],
    );
    let fi = run_andersen(&gen_cons(&p, 0, true), 5, true, true);
    let fl = run_flow(&p);
    println!("   flow-insensitive pairs: {:?}", fi.pairs);
    println!("   flow-sensitive   pairs: {:?}", fl.pairs);
    assert!(
        fi.pairs.contains(&(0, 2)) && !fl.pairs.contains(&(0, 2)) && fl.pairs.contains(&(1, 2))
    );
    assert!(
        fi.pairs.contains(&(0, 4)) && !fl.pairs.contains(&(0, 4)) && fl.pairs.contains(&(1, 4))
    );
    let mut s = String::new();
    show(&p.body, 3, &mut s);
    assert!(s.contains("x3.0 = x1"));
}

// ---------------------------------------------------------------- tables
fn precision_table() {
    println!(
        "-- soundness + precision over 1500 random programs (8 vars, 26 stmts, calls and ifs)"
    );
    let mut tot = [0usize; 6];
    let mut strict_st = 0;
    for seed in 0..1500u64 {
        let mut r = Rng(0x9E37_79B9 ^ (seed + 1) * 0x2545_F491);
        let p = gen_prog(&mut r, 8, 26, 12);
        check_program(&p, seed);
        let su = analyse(&p);
        let lens = [
            su.st_nf.len(),
            su.st_fs.len(),
            su.a_nf0.pairs.len(),
            su.a_fs0.pairs.len(),
            su.a_fs1.pairs.len(),
            su.fl.pairs.len(),
        ];
        for (t, l) in tot.iter_mut().zip(lens) {
            *t += l;
        }
        if su.st_fs.len() > su.a_fs0.pairs.len() {
            strict_st += 1;
        }
    }
    let names = [
        "Steensgaard field-insens",
        "Steensgaard field-sens",
        "Andersen k0 field-insens",
        "Andersen k0 field-sens",
        "Andersen k1 field-sens",
        "flow-sensitive (end)",
    ];
    for (n, t) in names.iter().zip(tot) {
        println!("   {n:<26} may-alias pairs (sum over programs): {t}");
    }
    println!("   Steensgaard strictly coarser than Andersen on {strict_st}/1500 programs");
    assert!(
        tot[0] >= tot[2]
            && tot[1] >= tot[3]
            && tot[3] >= tot[4]
            && tot[4] >= tot[5]
            && tot[2] >= tot[3]
    );
    assert!(
        strict_st > 0 && tot[3] > tot[4] && tot[4] > tot[5],
        "each refinement should win somewhere"
    );
}

fn timing_table() {
    println!(
        "-- scaling (large generated programs, k=0, field-sensitive; times are machine dependent)"
    );
    println!(
        "   {:>6} {:>6} | {:>9} {:>10} {:>8} | {:>9} {:>10} | {:>9} {:>7} | {:>8} {:>8}",
        "stmts",
        "vars",
        "and(ms)",
        "moved",
        "merged",
        "naive(ms)",
        "moved",
        "steens(ms)",
        "classes",
        "and prs",
        "st prs"
    );
    for &(n, nv) in &[(150usize, 20usize), (400, 60), (900, 130)] {
        let mut r = Rng(1234 + n as u64);
        let p = gen_prog(&mut r, nv, n, 4);
        let c = gen_cons(&p, 0, true);
        let nm = nv.min(150);
        let t = Instant::now();
        let a = run_andersen(&c, nm, true, true);
        let t_a = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let nv_ = run_andersen(&c, nm, false, false);
        let t_n = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let (sp, classes) = run_steens(&c, nm);
        let t_s = t.elapsed().as_secs_f64() * 1e3;
        assert_eq!(
            a.pairs, nv_.pairs,
            "optimisations must not change the solution"
        );
        assert!(a.pairs.is_subset(&sp));
        println!(
            "   {:>6} {:>6} | {:>9.2} {:>10} {:>8} | {:>9.2} {:>10} | {:>9.2} {:>7} | {:>8} {:>8}",
            n,
            nv,
            t_a,
            a.st.moved,
            a.st.merged,
            t_n,
            nv_.st.moved,
            t_s,
            classes,
            a.pairs.len(),
            sp.len()
        );
    }
}

/// How many (store, load) pairs on the same field could an optimiser reorder? That needs "the two bases do not alias".
fn reorder_table() {
    println!(
        "-- optimiser view: store/load pairs on the same field that may be reordered (bases provably distinct)"
    );
    let (mut total, mut ste, mut and, mut rust) = (0usize, 0usize, 0usize, 0usize);
    for seed in 0..300u64 {
        let mut r = Rng(77 + seed * 31);
        let p = gen_prog(&mut r, 8, 26, 0); // straight-line, no calls
        let su = analyse(&p);
        let mut ops = vec![]; // (base, field, is_store)
        for s in &p.body {
            match s.op {
                Op::Load { y, f, .. } => ops.push((y, f, false)),
                Op::Store { x, f, .. } => ops.push((x, f, true)),
                _ => {}
            }
        }
        for i in 0..ops.len() {
            for j in i + 1..ops.len() {
                let (a, b) = (ops[i], ops[j]);
                if a.1 != b.1 || !(a.2 || b.2) {
                    continue;
                }
                total += 1;
                let key = (a.0.min(b.0), a.0.max(b.0));
                let distinct = a.0 != b.0;
                if distinct && !su.st_fs.contains(&key) {
                    ste += 1;
                }
                if distinct && !su.a_fs0.pairs.contains(&key) {
                    and += 1;
                }
                if distinct {
                    rust += 1; // IF both bases were live `&mut` bindings, the borrow checker would rule out overlap
                }
            }
        }
    }
    println!(
        "   dependent-looking pairs: {total};  Steensgaard frees {ste}, Andersen frees {and}, all-`&mut` bases would free {rust}"
    );
    println!(
        "   (the last figure is a hypothetical UPPER bound: every distinct base would have to be a live `&mut`. These programs copy pointers"
    );
    println!(
        "   freely, so they do not satisfy that, and two `&T` may alias. The analyses must be sound for the program as written.)"
    );
    assert!(ste <= and && and <= rust);
}

fn main() {
    demo_unify();
    demo_field();
    demo_context();
    demo_strong();
    precision_table();
    timing_table();
    reorder_table();
    println!("alias_analysis_points_to: all checks passed");
}
