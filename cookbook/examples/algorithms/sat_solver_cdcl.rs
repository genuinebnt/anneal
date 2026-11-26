//! RECIPE 272 (algorithms): a SAT solver, from DPLL to CDCL (watched literals, 1UIP learning, VSIDS, restarts), plus problem encodings.
//! LEVEL: advanced
//! Use in: solving constraint problems by encoding them as boolean CNF; dependency resolution, verification, planning.   Run: cargo run --release --example sat_solver_cdcl
#![allow(dead_code)]
//!
//! Pipeline (the lineage in TAOCP 7.2.2.2, Satisfiability):
//! 1. DIMACS CNF parser. Literals are non-zero ints (-3 = NOT x3); a clause ends with 0.
//! 2. DPLL: choose a variable, try both values, after every choice run UNIT PROPAGATION (a clause with one
//!    unassigned literal and the rest false forces that literal). Chronological backtracking only.
//! 3. CDCL (conflict-driven clause learning): on a conflict, walk the implication trail backwards resolving
//!    antecedent clauses until exactly ONE literal of the current decision level remains (the first UIP). That
//!    clause is LEARNT (it is implied by the formula, so it prunes the future), and the solver BACKJUMPS to the second
//!    highest level in it (non-chronological), where the learnt clause is unit and flips the search direction.
//!    Supporting machinery: two WATCHED LITERALS per clause (a clause is only visited when a watched literal becomes
//!    false, and no work is undone on backtrack), VSIDS-style activity (variables in recent conflicts get bumped,
//!    all scores decay by growing the increment), PHASE SAVING (re-try the last value), and Luby RESTARTS.
//!    Omitted for brevity: clause deletion, learnt-clause minimisation, a heap for variable selection (linear scan).
//! 4. Models are verified clause by clause; CDCL is cross-checked against brute force on 500 random 3-SAT instances,
//!    and a clause/variable ratio sweep shows the phase transition near 4.26 (where instances are hardest).
//! 5. Encodings: N-queens, sudoku, graph colouring, pigeonhole (UNSAT, exponentially hard for resolution), XOR cycle.
//!
//! 2-SAT detail: if every clause has at most 2 literals, SAT is in linear time. Each clause (a OR b) gives implications
//! (NOT a -> b) and (NOT b -> a). Build the implication graph on 2n literal nodes; the formula is UNSAT exactly when some
//! x and NOT x lie in the same strongly connected component (see tarjan_scc Tarjan SCC), and a model is read off by taking x true
//! when comp(x) comes later in topological order than comp(NOT x). General 3-SAT is NP-complete, hence CDCL.
//!
//! COMMON USES: hardware and software verification (bounded model checking, equivalence checking), AI planning,
//! package dependency solving (libsolv in zypper; PubGrub-style resolvers in Dart's pub and uv add conflict learning
//! on version constraints, the same idea as CDCL), test generation, cryptanalysis experiments, scheduling and
//! configuration. Industrial solvers of this family include MiniSat, CaDiCaL, Kissat; SMT solvers such as Z3 use a CDCL core.
//! Read: Knuth, The Art of Computer Programming Vol 4B, section 7.2.2.2 (Satisfiability); Skiena, The Algorithm
//! Design Manual (Satisfiability in the problem catalog); Een and Sorensson, "An Extensible SAT-solver" (MiniSat).
//! https://en.wikipedia.org/wiki/Conflict-driven_clause_learning  https://www-cs-faculty.stanford.edu/~knuth/taocp.html
//! https://www.algorist.com/

use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

type Cnf = Vec<Vec<i32>>;

// ------------------------------------------------------------------ DIMACS
fn parse_dimacs(text: &str) -> (usize, Cnf) {
    let mut nvars = 0;
    let mut clauses = vec![];
    let mut cur = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('c') {
            continue;
        }
        if line.starts_with('p') {
            let f: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(f[1], "cnf");
            nvars = f[2].parse().unwrap();
            continue;
        }
        for tok in line.split_whitespace() {
            let x: i32 = tok.parse().unwrap();
            if x == 0 {
                clauses.push(std::mem::take(&mut cur));
            } else {
                cur.push(x);
            }
        }
    }
    (nvars, clauses)
}

fn check_model(cnf: &Cnf, model: &[bool]) -> bool {
    cnf.iter().all(|c| {
        c.iter()
            .any(|&l| model[l.unsigned_abs() as usize - 1] == (l > 0))
    })
}

// ------------------------------------------------------------------ DPLL
#[derive(Default, Debug, Clone, Copy)]
struct Stats {
    conflicts: u64,
    decisions: u64,
    propagations: u64,
    restarts: u64,
}

fn dpll(cnf: &Cnf, nvars: usize, stats: &mut Stats) -> Option<Vec<bool>> {
    fn go(cnf: &Cnf, a: &mut Vec<i8>, stats: &mut Stats) -> bool {
        let mut assigned_here = vec![];
        // unit propagation to fixpoint (naive full scans: clarity over speed)
        loop {
            let mut changed = false;
            for c in cnf {
                let mut unassigned = None;
                let mut n_un = 0;
                let mut sat = false;
                for &l in c {
                    let v = a[l.unsigned_abs() as usize - 1];
                    if v == 0 {
                        n_un += 1;
                        unassigned = Some(l);
                    } else if (v > 0) == (l > 0) {
                        sat = true;
                        break;
                    }
                }
                if sat {
                    continue;
                }
                if n_un == 0 {
                    stats.conflicts += 1;
                    for v in assigned_here {
                        a[v] = 0;
                    }
                    return false;
                }
                if n_un == 1 {
                    let l = unassigned.unwrap();
                    let v = l.unsigned_abs() as usize - 1;
                    a[v] = if l > 0 { 1 } else { -1 };
                    assigned_here.push(v);
                    stats.propagations += 1;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // branch on the first unassigned variable of a still-unsatisfied clause
        let pick = cnf.iter().find(|c| {
            !c.iter().any(|&l| {
                a[l.unsigned_abs() as usize - 1] != 0
                    && (a[l.unsigned_abs() as usize - 1] > 0) == (l > 0)
            })
        });
        let Some(c) = pick else { return true };
        let v = c
            .iter()
            .map(|l| l.unsigned_abs() as usize - 1)
            .find(|&v| a[v] == 0)
            .unwrap();
        stats.decisions += 1;
        for val in [1i8, -1] {
            a[v] = val;
            if go(cnf, a, stats) {
                return true;
            }
            a[v] = 0;
        }
        for v in assigned_here {
            a[v] = 0;
        }
        false
    }
    let mut a = vec![0i8; nvars];
    go(cnf, &mut a, stats).then(|| a.iter().map(|&x| x >= 0).collect())
}

// ------------------------------------------------------------------ CDCL
// literal encoding: var v -> positive 2v, negative 2v+1; negation is `^ 1`
fn lit_of(l: i32) -> u32 {
    let v = l.unsigned_abs() - 1;
    2 * v + (l < 0) as u32
}
fn lval(a: &[i8], l: u32) -> i8 {
    let v = a[(l >> 1) as usize];
    if l & 1 == 1 { -v } else { v }
}

struct Clause {
    lits: Vec<u32>,
}

struct Solver {
    n: usize,
    clauses: Vec<Clause>,
    watches: Vec<Vec<usize>>, // watches[l] = clauses watching l, visited when l becomes FALSE
    assigns: Vec<i8>,
    level: Vec<usize>,
    reason: Vec<Option<usize>>,
    trail: Vec<u32>,
    trail_lim: Vec<usize>,
    qhead: usize,
    activity: Vec<f64>,
    var_inc: f64,
    phase: Vec<bool>,
    seen: Vec<bool>,
    ok: bool,
    stats: Stats,
}

fn luby(mut i: u64) -> u64 {
    loop {
        let mut k = 1;
        while (1u64 << k) - 1 < i {
            k += 1;
        }
        if (1u64 << k) - 1 == i {
            return 1 << (k - 1);
        }
        i -= (1u64 << (k - 1)) - 1;
    }
}

impl Solver {
    fn new(n: usize) -> Self {
        Solver {
            n,
            clauses: vec![],
            watches: vec![vec![]; 2 * n],
            assigns: vec![0; n],
            level: vec![0; n],
            reason: vec![None; n],
            trail: vec![],
            trail_lim: vec![],
            qhead: 0,
            activity: vec![0.0; n],
            var_inc: 1.0,
            phase: vec![false; n],
            seen: vec![false; n],
            ok: true,
            stats: Stats::default(),
        }
    }

    fn from_cnf(n: usize, cnf: &Cnf) -> Self {
        let mut s = Solver::new(n);
        for c in cnf {
            s.add_clause(c);
        }
        s
    }

    fn decision_level(&self) -> usize {
        self.trail_lim.len()
    }

    fn assign(&mut self, l: u32, reason: Option<usize>) {
        let v = (l >> 1) as usize;
        self.assigns[v] = if l & 1 == 0 { 1 } else { -1 };
        self.level[v] = self.decision_level();
        self.reason[v] = reason;
        self.trail.push(l);
    }

    fn add_clause(&mut self, dimacs: &[i32]) {
        let mut lits: Vec<u32> = dimacs.iter().map(|&l| lit_of(l)).collect();
        lits.sort();
        lits.dedup();
        if lits.windows(2).any(|w| w[0] ^ 1 == w[1]) {
            return; // tautology
        }
        match lits.len() {
            0 => self.ok = false,
            1 => match lval(&self.assigns, lits[0]) {
                -1 => self.ok = false,
                0 => self.assign(lits[0], None),
                _ => {}
            },
            _ => {
                self.attach(lits);
            }
        }
    }

    fn attach(&mut self, lits: Vec<u32>) -> usize {
        let ci = self.clauses.len();
        self.watches[lits[0] as usize].push(ci);
        self.watches[lits[1] as usize].push(ci);
        self.clauses.push(Clause { lits });
        ci
    }

    /// Two-watched-literal unit propagation. Returns the conflicting clause, if any.
    fn propagate(&mut self) -> Option<usize> {
        while self.qhead < self.trail.len() {
            let p = self.trail[self.qhead];
            self.qhead += 1;
            self.stats.propagations += 1;
            let fl = p ^ 1; // this literal just became false
            let mut ws = std::mem::take(&mut self.watches[fl as usize]);
            let (mut i, mut j) = (0, 0);
            let mut confl = None;
            while i < ws.len() {
                let ci = ws[i];
                i += 1;
                let lits = &mut self.clauses[ci].lits;
                if lits[0] == fl {
                    lits.swap(0, 1);
                }
                let first = lits[0];
                if lval(&self.assigns, first) == 1 {
                    ws[j] = ci; // clause already satisfied by the other watch
                    j += 1;
                    continue;
                }
                let mut moved = false;
                for k in 2..lits.len() {
                    if lval(&self.assigns, lits[k]) != -1 {
                        lits.swap(1, k);
                        self.watches[lits[1] as usize].push(ci);
                        moved = true;
                        break;
                    }
                }
                if moved {
                    continue;
                }
                ws[j] = ci;
                j += 1;
                if lval(&self.assigns, first) == -1 {
                    confl = Some(ci);
                    while i < ws.len() {
                        ws[j] = ws[i];
                        j += 1;
                        i += 1;
                    }
                    break;
                }
                self.assign(first, Some(ci)); // clause is unit
            }
            ws.truncate(j);
            self.watches[fl as usize] = ws;
            if confl.is_some() {
                self.qhead = self.trail.len();
                return confl;
            }
        }
        None
    }

    fn bump(&mut self, v: usize) {
        self.activity[v] += self.var_inc;
        if self.activity[v] > 1e100 {
            for a in &mut self.activity {
                *a *= 1e-100;
            }
            self.var_inc *= 1e-100;
        }
    }

    /// First-UIP conflict analysis. Returns (learnt clause with the asserting literal first, backjump level).
    fn analyze(&mut self, mut confl: usize) -> (Vec<u32>, usize) {
        let cur = self.decision_level();
        let mut learnt = vec![0u32];
        let mut counter = 0;
        let mut idx = self.trail.len();
        let mut p: Option<u32> = None;
        loop {
            let lits = self.clauses[confl].lits.clone();
            for &q in &lits {
                if Some(q) == p {
                    continue;
                }
                let v = (q >> 1) as usize;
                if !self.seen[v] && self.level[v] > 0 {
                    self.bump(v);
                    self.seen[v] = true;
                    if self.level[v] == cur {
                        counter += 1;
                    } else {
                        learnt.push(q);
                    }
                }
            }
            while !self.seen[(self.trail[idx - 1] >> 1) as usize] {
                idx -= 1;
            }
            idx -= 1;
            let pl = self.trail[idx];
            self.seen[(pl >> 1) as usize] = false;
            counter -= 1;
            p = Some(pl);
            if counter == 0 {
                break;
            }
            confl = self.reason[(pl >> 1) as usize].expect("implied literal has a reason");
        }
        learnt[0] = p.unwrap() ^ 1;
        for &q in &learnt[1..] {
            self.seen[(q >> 1) as usize] = false;
        }
        let mut bt = 0;
        if learnt.len() > 1 {
            let mut best = 1;
            for i in 2..learnt.len() {
                if self.level[(learnt[i] >> 1) as usize] > self.level[(learnt[best] >> 1) as usize]
                {
                    best = i;
                }
            }
            learnt.swap(1, best);
            bt = self.level[(learnt[1] >> 1) as usize];
        }
        (learnt, bt)
    }

    fn cancel_until(&mut self, lvl: usize) {
        if self.decision_level() <= lvl {
            return;
        }
        let keep = self.trail_lim[lvl];
        for &l in &self.trail[keep..] {
            let v = (l >> 1) as usize;
            self.phase[v] = l & 1 == 0; // phase saving
            self.assigns[v] = 0;
            self.reason[v] = None;
        }
        self.trail.truncate(keep);
        self.trail_lim.truncate(lvl);
        self.qhead = keep;
    }

    fn pick_branch(&self) -> Option<usize> {
        (0..self.n)
            .filter(|&v| self.assigns[v] == 0)
            .max_by(|&a, &b| self.activity[a].partial_cmp(&self.activity[b]).unwrap())
    }

    fn solve(&mut self) -> Option<Vec<bool>> {
        if !self.ok || self.propagate().is_some() {
            return None;
        }
        let (mut restart_no, unit) = (1u64, 50u64);
        let mut limit = luby(restart_no) * unit;
        let mut since = 0;
        loop {
            if let Some(confl) = self.propagate() {
                self.stats.conflicts += 1;
                since += 1;
                if self.decision_level() == 0 {
                    return None;
                }
                let (learnt, bt) = self.analyze(confl);
                self.cancel_until(bt);
                if learnt.len() == 1 {
                    self.assign(learnt[0], None);
                } else {
                    let first = learnt[0];
                    let ci = self.attach(learnt);
                    self.assign(first, Some(ci));
                }
                self.var_inc /= 0.95; // VSIDS decay
            } else if since >= limit {
                self.stats.restarts += 1;
                self.cancel_until(0);
                restart_no += 1;
                limit = luby(restart_no) * unit;
                since = 0;
            } else if let Some(v) = self.pick_branch() {
                self.stats.decisions += 1;
                self.trail_lim.push(self.trail.len());
                let l = 2 * v as u32 + (!self.phase[v]) as u32;
                self.assign(l, None);
            } else {
                return Some(self.assigns.iter().map(|&a| a > 0).collect());
            }
        }
    }
}

fn cdcl(nvars: usize, cnf: &Cnf) -> (Option<Vec<bool>>, Stats) {
    let mut s = Solver::from_cnf(nvars, cnf);
    let r = s.solve();
    if let Some(m) = &r {
        assert!(check_model(cnf, m), "model must satisfy every clause");
    }
    (r, s.stats)
}

// ------------------------------------------------------------------ oracles and generators
fn brute_force(nvars: usize, cnf: &Cnf) -> bool {
    let masks: Vec<(u32, u32)> = cnf
        .iter()
        .map(|c| {
            let (mut pos, mut neg) = (0, 0);
            for &l in c {
                if l > 0 {
                    pos |= 1 << (l - 1);
                } else {
                    neg |= 1 << (-l - 1);
                }
            }
            (pos, neg)
        })
        .collect();
    (0u32..1 << nvars).any(|a| {
        masks
            .iter()
            .all(|&(pos, neg)| a & pos != 0 || !a & neg != 0)
    })
}

fn random_3sat(rng: &mut Rng, n: usize, m: usize) -> Cnf {
    (0..m)
        .map(|_| {
            let mut vs = vec![];
            while vs.len() < 3 {
                let v = 1 + rng.below(n as u64) as i32;
                if !vs.contains(&v) {
                    vs.push(v);
                }
            }
            vs.iter()
                .map(|&v| if rng.below(2) == 0 { v } else { -v })
                .collect()
        })
        .collect()
}

fn pigeonhole(holes: usize) -> (usize, Cnf) {
    let pigeons = holes + 1;
    let var = |p: usize, h: usize| (p * holes + h + 1) as i32;
    let mut cnf: Cnf = (0..pigeons)
        .map(|p| (0..holes).map(|h| var(p, h)).collect())
        .collect();
    for h in 0..holes {
        for p in 0..pigeons {
            for q in p + 1..pigeons {
                cnf.push(vec![-var(p, h), -var(q, h)]);
            }
        }
    }
    (pigeons * holes, cnf)
}

/// x1 = x2 = ... = xn but xn != x1: an odd XOR cycle, UNSAT, with no short resolution refutation by unit propagation alone.
fn xor_cycle(n: usize) -> (usize, Cnf) {
    let mut cnf = vec![];
    for i in 1..=n {
        let (a, b) = (i as i32, (i % n + 1) as i32);
        if i < n {
            cnf.push(vec![-a, b]);
            cnf.push(vec![a, -b]); // a XOR b = 0
        } else {
            cnf.push(vec![a, b]);
            cnf.push(vec![-a, -b]); // a XOR b = 1
        }
    }
    (n, cnf)
}

fn at_most_one(cnf: &mut Cnf, vars: &[i32]) {
    for i in 0..vars.len() {
        for j in i + 1..vars.len() {
            cnf.push(vec![-vars[i], -vars[j]]);
        }
    }
}

fn exactly_one(cnf: &mut Cnf, vars: &[i32]) {
    cnf.push(vars.to_vec());
    at_most_one(cnf, vars);
}

fn queens_cnf(n: usize) -> (usize, Cnf) {
    let v = |r: usize, c: usize| (r * n + c + 1) as i32;
    let mut cnf = vec![];
    for r in 0..n {
        exactly_one(&mut cnf, &(0..n).map(|c| v(r, c)).collect::<Vec<_>>());
    }
    for c in 0..n {
        at_most_one(&mut cnf, &(0..n).map(|r| v(r, c)).collect::<Vec<_>>());
    }
    for d in 0..2 * n - 1 {
        let diag: Vec<i32> = (0..n)
            .filter(|&r| d >= r && d - r < n)
            .map(|r| v(r, d - r))
            .collect();
        at_most_one(&mut cnf, &diag);
        let anti: Vec<i32> = (0..n)
            .filter(|&r| d >= r && d - r < n)
            .map(|r| v(r, n - 1 - (d - r)))
            .collect();
        at_most_one(&mut cnf, &anti);
    }
    (n * n, cnf)
}

fn colouring_cnf(n: usize, edges: &[(usize, usize)], k: usize) -> (usize, Cnf) {
    let v = |x: usize, c: usize| (x * k + c + 1) as i32;
    let mut cnf = vec![];
    for x in 0..n {
        cnf.push((0..k).map(|c| v(x, c)).collect());
    }
    for &(a, b) in edges {
        for c in 0..k {
            cnf.push(vec![-v(a, c), -v(b, c)]);
        }
    }
    (n * k, cnf)
}

fn sudoku_cnf(puzzle: &str) -> (usize, Cnf) {
    let v = |r: usize, c: usize, d: usize| (81 * r + 9 * c + d + 1) as i32;
    let mut cnf = vec![];
    for r in 0..9 {
        for c in 0..9 {
            exactly_one(&mut cnf, &(0..9).map(|d| v(r, c, d)).collect::<Vec<_>>());
        }
    }
    for d in 0..9 {
        for i in 0..9 {
            exactly_one(&mut cnf, &(0..9).map(|c| v(i, c, d)).collect::<Vec<_>>()); // row i
            exactly_one(&mut cnf, &(0..9).map(|r| v(r, i, d)).collect::<Vec<_>>()); // column i
            let (br, bc) = (i / 3 * 3, i % 3 * 3);
            exactly_one(
                &mut cnf,
                &(0..9)
                    .map(|k| v(br + k / 3, bc + k % 3, d))
                    .collect::<Vec<_>>(),
            ); // box i
        }
    }
    for (i, ch) in puzzle.chars().enumerate() {
        if let Some(d) = ch.to_digit(10).filter(|&d| d > 0) {
            cnf.push(vec![v(i / 9, i % 9, d as usize - 1)]);
        }
    }
    (729, cnf)
}

fn main() {
    // ---- DIMACS + the three solvers on a tiny formula
    let text = "c tiny example\np cnf 4 5\n1 -2 0\n2 3 0\n-1 -3 4 0\n-4 2\n0\n3 -2 0\n";
    let (n, cnf) = parse_dimacs(text);
    assert_eq!((n, cnf.len()), (4, 5));
    let (m, st) = cdcl(n, &cnf);
    println!(
        "[dimacs] 4 vars 5 clauses -> {:?} ({} decisions, {} conflicts)",
        m.map(|m| m.iter().map(|&b| b as u8).collect::<Vec<_>>()),
        st.decisions,
        st.conflicts
    );
    assert_eq!(
        (1..=15).map(luby).collect::<Vec<_>>(),
        [1, 1, 2, 1, 1, 2, 4, 1, 1, 2, 1, 1, 2, 4, 8]
    );

    // ---- UNSAT instances: pigeonhole and XOR cycle
    println!("[unsat] pigeonhole PHP(n+1,n): DPLL vs CDCL");
    for holes in 2..=7 {
        let (nv, cnf) = pigeonhole(holes);
        let (r, st) = cdcl(nv, &cnf);
        assert!(r.is_none());
        let mut d = Stats::default();
        let dpll_note = if holes <= 5 {
            assert!(dpll(&cnf, nv, &mut d).is_none());
            format!("DPLL {} decisions", d.decisions)
        } else {
            "DPLL skipped".to_string()
        };
        println!(
            "   n={holes}: CDCL {} conflicts, {} decisions, {} restarts | {dpll_note}",
            st.conflicts, st.decisions, st.restarts
        );
    }
    for n in [3, 10, 31] {
        let (nv, cnf) = xor_cycle(n);
        assert!(cdcl(nv, &cnf).0.is_none());
        assert!(dpll(&cnf, nv, &mut Stats::default()).is_none());
    }
    println!("[unsat] odd XOR cycles refuted by CDCL and DPLL");

    // ---- random 3-SAT cross-check against brute force
    let mut rng = Rng(0x5a7_5a7_dead_beef);
    let (mut sat_count, mut checked) = (0, 0);
    for i in 0..500 {
        let ratio = [3.0, 4.26, 5.0][i % 3];
        let nv = 10 + rng.below(8) as usize; // 10..=17
        let cnf = random_3sat(&mut rng, nv, (ratio * nv as f64).round() as usize);
        let oracle = brute_force(nv, &cnf);
        let (r, _) = cdcl(nv, &cnf);
        assert_eq!(r.is_some(), oracle, "CDCL verdict matches brute force");
        if i % 5 == 0 {
            assert_eq!(
                dpll(&cnf, nv, &mut Stats::default()).is_some(),
                oracle,
                "DPLL too"
            );
        }
        sat_count += oracle as u32;
        checked += 1;
    }
    println!(
        "[random 3-SAT] {checked} instances cross-checked with brute force ({sat_count} SAT, {} UNSAT)",
        checked - sat_count
    );

    // ---- phase transition
    println!("[phase transition] n=100 variables, 30 instances per ratio");
    println!("   ratio  %SAT  avg conflicts  avg decisions  total ms");
    let mut table = vec![];
    for &ratio in &[3.0, 3.5, 4.0, 4.26, 4.5, 5.0, 6.0] {
        let t0 = Instant::now();
        let (mut sat, mut conf, mut dec) = (0u32, 0u64, 0u64);
        for _ in 0..30 {
            let cnf = random_3sat(&mut rng, 100, (ratio * 100.0f64).round() as usize);
            let (r, st) = cdcl(100, &cnf);
            sat += r.is_some() as u32;
            conf += st.conflicts;
            dec += st.decisions;
        }
        println!(
            "   {ratio:5.2}  {:4.0}  {:13.1}  {:13.1}  {:8.1}",
            sat as f64 / 30.0 * 100.0,
            conf as f64 / 30.0,
            dec as f64 / 30.0,
            t0.elapsed().as_secs_f64() * 1e3
        );
        table.push((ratio, sat, conf));
    }
    assert!(table[0].1 >= 27, "ratio 3.0 almost always SAT");
    assert!(table[6].1 <= 3, "ratio 6.0 almost always UNSAT");
    let hardest = table.iter().max_by_key(|t| t.2).unwrap().0;
    println!("   hardest ratio in this sweep: {hardest}");
    assert!(
        (3.5..=5.0).contains(&hardest),
        "hardness peaks near the threshold"
    );

    // ---- encodings
    for n in 2..=8 {
        let (nv, cnf) = queens_cnf(n);
        let (r, _) = cdcl(nv, &cnf);
        assert_eq!(r.is_some(), !(n == 2 || n == 3), "queens n={n}");
        if let Some(m) = r {
            let pos: Vec<(usize, usize)> =
                (0..nv).filter(|&i| m[i]).map(|i| (i / n, i % n)).collect();
            assert_eq!(pos.len(), n);
            for (a, &(r1, c1)) in pos.iter().enumerate() {
                for &(r2, c2) in &pos[a + 1..] {
                    assert!(r1 != r2 && c1 != c2 && r1.abs_diff(r2) != c1.abs_diff(c2));
                }
            }
            if n == 8 {
                let cols: Vec<usize> = pos.iter().map(|p| p.1).collect();
                println!("[n-queens] n=8 SAT, columns per row {cols:?}; n=3 UNSAT");
            }
        }
    }

    let puzzle =
        "530070000600195000098000060800060003400803001700020006060000280000419005000080079";
    let (nv, cnf) = sudoku_cnf(puzzle);
    let (r, st) = cdcl(nv, &cnf);
    let m = r.expect("sudoku is solvable");
    let grid: Vec<usize> = (0..81)
        .map(|cell| (0..9).find(|&d| m[cell * 9 + d]).unwrap() + 1)
        .collect();
    for i in 0..9 {
        for set in [
            (0..9).map(|k| grid[i * 9 + k]).collect::<Vec<_>>(),
            (0..9).map(|k| grid[k * 9 + i]).collect(),
            (0..9)
                .map(|k| grid[(i / 3 * 3 + k / 3) * 9 + i % 3 * 3 + k % 3])
                .collect(),
        ] {
            let mut s = set;
            s.sort();
            assert_eq!(s, (1..=9).collect::<Vec<_>>());
        }
    }
    for (i, ch) in puzzle.chars().enumerate() {
        if ch != '0' {
            assert_eq!(
                grid[i],
                ch.to_digit(10).unwrap() as usize,
                "givens preserved"
            );
        }
    }
    assert_eq!(&grid[..9], &[5, 3, 4, 6, 7, 8, 9, 1, 2]);
    println!(
        "[sudoku] solved with {} clauses, {} decisions, {} conflicts; first row {:?}",
        cnf.len(),
        st.decisions,
        st.conflicts,
        &grid[..9]
    );

    let petersen: Vec<(usize, usize)> = (0..5)
        .flat_map(|i| [(i, (i + 1) % 5), (i, i + 5), (i + 5, (i + 2) % 5 + 5)])
        .collect();
    let k4: Vec<(usize, usize)> = (0..4)
        .flat_map(|i| (i + 1..4).map(move |j| (i, j)))
        .collect();
    let c5: Vec<(usize, usize)> = (0..5).map(|i| (i, (i + 1) % 5)).collect();
    for (name, nn, edges, k, expect) in [
        ("Petersen", 10, &petersen, 3, true),
        ("Petersen", 10, &petersen, 2, false),
        ("K4", 4, &k4, 3, false),
        ("K4", 4, &k4, 4, true),
        ("C5", 5, &c5, 2, false),
        ("C5", 5, &c5, 3, true),
    ] {
        let (nv, cnf) = colouring_cnf(nn, edges, k);
        assert_eq!(
            cdcl(nv, &cnf).0.is_some(),
            expect,
            "{name} with {k} colours"
        );
    }
    println!("[colouring] Petersen is 3- but not 2-colourable; K4 needs 4; C5 needs 3");
    println!("all SAT results verified");
}
