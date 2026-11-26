//! RECIPE 905 (concurrency): a mini loom: exhaustive interleaving exploration with sleep sets and a store-buffer (TSO) memory model.
//! LEVEL: advanced
//! Use in: PROVING small lock/queue/counter designs correct (or finding the exact interleaving that breaks them), understanding what loom/shuttle/CHESS/TLA+ do.   Run: cargo run --release --example model_checker_for_concurrent_code
//! COMMON USES: `loom` (tokio, crossbeam, parking_lot test their atomics with it), `shuttle` (AWS, randomised scheduling), Microsoft CHESS, CDSChecker/GenMC for C11 atomics, TLA+/TLC and Jepsen-style reasoning at the protocol level.
//! Course: Godefroid, "Partial-Order Methods for the Verification of Concurrent Systems" (sleep sets); Flanagan & Godefroid, "Dynamic Partial-Order Reduction" (POPL 2005); Sewell et al., "x86-TSO" (CACM 2010).
#![allow(dead_code)]
//!
//! WHY: stress tests only sample interleavings; a bug that needs 4 specific preemptions can survive a billion runs. A model checker
//! turns "I ran it a lot" into "I tried EVERY schedule up to the bound". It works because each thread is a tiny program of ATOMIC STEPS and the
//! checker owns the scheduler.
//! THE MODEL
//!   - Threads run a mini instruction set (`Ins`): Load/Store, indexed Load/Store (for linked structures), Xchg and Cas (atomic RMW), ALU, jumps,
//!     `CsEnter`/`CsExit` (critical-section marker: two threads inside at once = VIOLATION), `Fence`. One instruction = one scheduling point.
//!   - MEMORY MODES. `SC` (sequential consistency): a store hits memory immediately. `TSO` (x86-like store buffer): a store goes into the thread's FIFO
//!     buffer, loads read the newest own buffered value first, and the scheduler may FLUSH the oldest buffered store at any time as a separate action.
//!     `Xchg`, `Cas` and `Fence` are enabled only on an empty buffer (they drain it, like a locked x86 instruction).
//!     This is enough to reproduce the store-buffering litmus test, and to show why Peterson's lock needs a fence on real hardware.
//!   - The checker explores ALL schedules depth-first. Spin loops would make schedules infinite, so each thread may take at most `LOOP_BOUND`
//!     backward jumps; longer executions are counted as `pruned` (a BOUNDED proof: "no violation within the bound", say so honestly).
//! PARTIAL-ORDER REDUCTION (sleep sets): if two actions are INDEPENDENT (different threads, no read/write or write/write overlap on a variable) then
//! doing a-then-b and b-then-a reach the same state, so exploring both is redundant. After exploring action `a` from a state, `a` is put to
//! sleep in the sibling branches: it need not be tried again until something DEPENDENT on it runs. This keeps every reachable state and verdict but
//! cuts redundant schedules, often by orders of magnitude. We check soundness EMPIRICALLY: the set of final states and the violation verdict
//! are identical with and without reduction, on every program here. (DPOR, as in loom, computes the dependencies lazily at run time and
//! prunes more; sleep sets are the simple static cousin.)
//! COUNTEREXAMPLES: a breadth-first search over distinct states finds the SHORTEST violating schedule and prints it as a readable trace.
//! PROGRAMS: a correct TAS spinlock (proved in SC and TSO), a broken load-then-store lock (bug found), Peterson's lock (proved under SC, FAILS under
//! TSO, proved again with a fence: the famous result), a lost-update counter (bug found) vs a CAS-loop counter (proved), and a lock-free stack pop with
//! ABA (bug found with plain pointers, proved with a tagged head).
//! Read: https://github.com/tokio-rs/loom and https://docs.rs/shuttle/latest/shuttle/ and https://www.cl.cam.ac.uk/~pes20/weakmemory/cacm.pdf (x86-TSO, Owens, Sarkar, Sewell)
//! Read: Flanagan & Godefroid, "Dynamic Partial-Order Reduction for Model Checking Software" (POPL 2005, title only)
//!
//! TRY NEXT:
//!   - [S] Add a third thread to the spinlock and predict the schedule count with and without sleep sets (ratio, not exact), then measure; raise `LOOP_BOUND` to 3 and explain the growth.
//!   - [S] Peterson under TSO needs the fence between `Store turn` and `Load flag`. Move the fence BEFORE `Store turn` and predict the verdict; run it.
//!   - [M] Add a `StoreRelease`/`LoadAcquire` pair with a third "relaxed" mode where unfenced stores from one thread may flush out of order (PSO-like). Does Peterson with a single fence still hold?
//!   - [M] Model `compare_exchange_weak` (spurious failure as an extra nondeterministic choice) and show the CAS counter still passes while a version without the retry loop fails.
//!   - [A] PROJECT: encode the two-thread push/pop of the Michael-Scott queue from `mpmc_queues_michael_scott_and_combining` (head, tail, 3 nodes) in `Ins` and prove "no value lost" within the bound; compare with `lockfree_stack_aba`'s hand-built ABA.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::time::Instant;

const MAXT: usize = 3;
const MAXR: usize = 8;
const MAXV: usize = 8;
const MAXB: usize = 4;
const CS_VAR: usize = MAXV - 1; // pseudo-variable: all critical-section markers conflict with each other
const LOOP_BOUND: u8 = 2;

#[derive(Clone, Copy, Debug)]
enum Opd {
    I(i64),
    R(usize),
}
use Opd::{I, R};

#[derive(Clone, Copy, Debug)]
enum Ins {
    Load(usize, usize),           // reg <- var
    LoadIdx(usize, usize, usize), // reg <- var[base + regs[idx]]
    Store(usize, Opd),
    StoreIdx(usize, usize, Opd),
    Xchg(usize, usize, Opd),     // reg <- old; var <- opd (atomic)
    Cas(usize, usize, Opd, Opd), // reg <- 1 if var==expected then var<-new else 0 (atomic)
    Mov(usize, Opd),
    Add(usize, Opd),
    Sub(usize, Opd),
    Mod(usize, i64),
    Jz(usize, usize),
    Jnz(usize, usize),
    CsEnter,
    CsExit,
    Fence,
}
use Ins::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Act {
    Step(usize),
    Flush(usize),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct St {
    pc: [u8; MAXT],
    regs: [[i64; MAXR]; MAXT],
    mem: [i64; MAXV],
    buf: [[(u8, i64); MAXB]; MAXT],
    bl: [u8; MAXT],
    bj: [u8; MAXT],
    inside: u8,
}

struct Prog {
    name: &'static str,
    code: Vec<Vec<Ins>>,
    init: [i64; MAXV],
    /// Checked on every terminal state (all threads done, buffers drained).
    check: fn(&St) -> Option<String>,
}
#[derive(Clone, Copy)]
struct Cfg {
    relaxed: bool,
    reduce: bool,
    node_cap: u64,
}

impl Prog {
    fn init_state(&self) -> St {
        St {
            pc: [0; MAXT],
            regs: [[0; MAXR]; MAXT],
            mem: self.init,
            buf: [[(0, 0); MAXB]; MAXT],
            bl: [0; MAXT],
            bj: [0; MAXT],
            inside: 0,
        }
    }
    fn n(&self) -> usize {
        self.code.len()
    }
    fn ins(&self, s: &St, t: usize) -> Option<Ins> {
        self.code[t].get(s.pc[t] as usize).copied()
    }
    fn enabled(&self, cfg: Cfg, s: &St) -> Vec<Act> {
        let mut v = Vec::new();
        for t in 0..self.n() {
            if let Some(i) = self.ins(s, t) {
                let drains = matches!(i, Xchg(..) | Cas(..) | Fence);
                if !(cfg.relaxed && drains && s.bl[t] > 0) {
                    v.push(Act::Step(t));
                }
            }
            if cfg.relaxed && s.bl[t] > 0 {
                v.push(Act::Flush(t));
            }
        }
        v
    }
}

fn opd(s: &St, t: usize, o: Opd) -> i64 {
    match o {
        I(x) => x,
        R(r) => s.regs[t][r],
    }
}
/// A load sees the thread's own newest buffered store to that variable before memory (store forwarding).
fn read(s: &St, t: usize, var: usize) -> i64 {
    for k in (0..s.bl[t] as usize).rev() {
        if s.buf[t][k].0 as usize == var {
            return s.buf[t][k].1;
        }
    }
    s.mem[var]
}
fn write(cfg: Cfg, s: &mut St, t: usize, var: usize, val: i64) {
    if cfg.relaxed {
        let k = s.bl[t] as usize;
        assert!(k < MAXB, "store buffer overflow: raise MAXB");
        s.buf[t][k] = (var as u8, val);
        s.bl[t] += 1;
    } else {
        s.mem[var] = val;
    }
}

/// Execute one action. Returns the new state and a violation message if a critical-section invariant broke.
fn step(p: &Prog, cfg: Cfg, s: &St, a: Act) -> (St, Option<String>) {
    let mut n = *s;
    match a {
        Act::Flush(t) => {
            let (v, val) = n.buf[t][0];
            n.mem[v as usize] = val;
            n.buf[t].copy_within(1.., 0);
            n.bl[t] -= 1;
            (n, None)
        }
        Act::Step(t) => {
            let ins = p.ins(s, t).unwrap();
            let mut next = n.pc[t] + 1;
            let mut viol = None;
            match ins {
                Load(r, v) => n.regs[t][r] = read(&n, t, v),
                LoadIdx(r, base, ri) => n.regs[t][r] = read(&n, t, base + n.regs[t][ri] as usize),
                Store(v, o) => {
                    let x = opd(&n, t, o);
                    write(cfg, &mut n, t, v, x)
                }
                StoreIdx(base, ri, o) => {
                    let x = opd(&n, t, o);
                    let v = base + n.regs[t][ri] as usize;
                    write(cfg, &mut n, t, v, x)
                }
                Xchg(r, v, o) => {
                    let new = opd(&n, t, o);
                    n.regs[t][r] = n.mem[v];
                    n.mem[v] = new;
                }
                Cas(r, v, e, w) => {
                    let (e, w) = (opd(&n, t, e), opd(&n, t, w));
                    if n.mem[v] == e {
                        n.mem[v] = w;
                        n.regs[t][r] = 1;
                    } else {
                        n.regs[t][r] = 0;
                    }
                }
                Mov(r, o) => n.regs[t][r] = opd(&n, t, o),
                Add(r, o) => n.regs[t][r] += opd(&n, t, o),
                Sub(r, o) => n.regs[t][r] -= opd(&n, t, o),
                Mod(r, m) => n.regs[t][r] %= m,
                Jz(r, tgt) | Jnz(r, tgt) => {
                    let zero = n.regs[t][r] == 0;
                    if zero == matches!(ins, Jz(..)) {
                        if tgt as u8 <= n.pc[t] {
                            n.bj[t] += 1;
                        }
                        next = tgt as u8;
                    }
                }
                CsEnter => {
                    if n.inside > 0 {
                        viol = Some(format!(
                            "MUTUAL EXCLUSION VIOLATED: thread {t} entered while another thread was inside"
                        ));
                    }
                    n.inside += 1;
                }
                CsExit => n.inside -= 1,
                Fence => {}
            }
            n.pc[t] = next;
            (n, viol)
        }
    }
}

/// (reads, writes) as variable bitmasks, for the independence relation.
fn footprint(p: &Prog, s: &St, a: Act) -> (u16, u16) {
    let bit = |v: usize| 1u16 << v;
    match a {
        Act::Flush(t) => (0, bit(s.buf[t][0].0 as usize)),
        Act::Step(t) => match p.ins(s, t).unwrap() {
            Load(_, v) => (bit(v), 0),
            LoadIdx(_, b, ri) => (bit(b + s.regs[t][ri] as usize), 0),
            Store(v, _) => (0, bit(v)),
            StoreIdx(b, ri, _) => (0, bit(b + s.regs[t][ri] as usize)),
            Xchg(_, v, _) | Cas(_, v, _, _) => (bit(v), bit(v)),
            CsEnter | CsExit => (0, bit(CS_VAR)),
            _ => (0, 0), // register-only and jump instructions are independent of everything in other threads
        },
    }
}
fn thread_of(a: Act) -> usize {
    match a {
        Act::Step(t) | Act::Flush(t) => t,
    }
}
fn independent(p: &Prog, s: &St, a: Act, b: Act) -> bool {
    if thread_of(a) == thread_of(b) {
        return false;
    }
    let ((ra, wa), (rb, wb)) = (footprint(p, s, a), footprint(p, s, b));
    (wa & (rb | wb)) == 0 && (wb & ra) == 0
}

#[derive(Default)]
struct Stats {
    schedules: u64,
    pruned: u64,
    nodes: u64,
    violations: u64,
    finals: BTreeSet<Vec<i64>>,
    capped: bool,
}

fn final_key(s: &St) -> Vec<i64> {
    let mut k = s.mem.to_vec();
    k.extend(s.regs.iter().flatten());
    k
}

fn dfs(p: &Prog, cfg: Cfg, s: &St, sleep: &[Act], st: &mut Stats) {
    st.nodes += 1;
    if st.nodes > cfg.node_cap {
        st.capped = true;
        return;
    }
    let en = p.enabled(cfg, s);
    if en.is_empty() {
        st.schedules += 1;
        if (p.check)(s).is_some() {
            st.violations += 1;
        }
        st.finals.insert(final_key(s));
        return;
    }
    let mut done: Vec<Act> = Vec::new();
    for &a in &en {
        if cfg.reduce && sleep.contains(&a) {
            continue;
        }
        let (ns, viol) = step(p, cfg, s, a);
        if viol.is_some() {
            st.schedules += 1;
            st.violations += 1;
        } else if (0..p.n()).any(|t| ns.bj[t] > LOOP_BOUND) {
            st.pruned += 1;
        } else {
            let child: Vec<Act> = if cfg.reduce {
                sleep
                    .iter()
                    .chain(done.iter())
                    .copied()
                    .filter(|&b| independent(p, s, a, b))
                    .collect()
            } else {
                vec![]
            };
            dfs(p, cfg, &ns, &child, st);
        }
        done.push(a);
    }
}

/// Shortest violating schedule by BFS over distinct states, or None if no violation exists within the bound.
fn shortest_counterexample(p: &Prog, cfg: Cfg) -> Option<(Vec<Act>, String)> {
    let init = p.init_state();
    let mut seen = HashSet::new();
    seen.insert(init);
    let mut q = VecDeque::from([(init, Vec::<Act>::new())]);
    while let Some((s, path)) = q.pop_front() {
        let en = p.enabled(cfg, &s);
        if en.is_empty() {
            if let Some(m) = (p.check)(&s) {
                return Some((path, m));
            }
        }
        for a in en {
            let (ns, viol) = step(p, cfg, &s, a);
            let mut np = path.clone();
            np.push(a);
            if let Some(m) = viol {
                return Some((np, m));
            }
            if (0..p.n()).any(|t| ns.bj[t] > LOOP_BOUND) || !seen.insert(ns) {
                continue;
            }
            q.push_back((ns, np));
        }
    }
    None
}

fn render(p: &Prog, cfg: Cfg, path: &[Act]) -> Vec<String> {
    let mut s = p.init_state();
    let mut out = Vec::new();
    for &a in path {
        let line = match a {
            Act::Flush(t) => format!("T{t}  [store buffer drains {:?} to memory]", s.buf[t][0]),
            Act::Step(t) => format!("T{t}  pc{} {:?}", s.pc[t], p.ins(&s, t).unwrap()),
        };
        let (ns, _) = step(p, cfg, &s, a);
        out.push(format!("{line:<48} mem={:?}", &ns.mem[..CS_VAR.min(6)]));
        s = ns;
    }
    out
}

// ---------------------------------------------------------------- the programs
fn no_check(_: &St) -> Option<String> {
    None
}
fn tas_spinlock() -> Prog {
    // lock = var 0. Xchg is ONE atomic step: that is the whole trick.
    let t = vec![Xchg(0, 0, I(1)), Jnz(0, 0), CsEnter, CsExit, Store(0, I(0))];
    Prog {
        name: "TAS spinlock (xchg)",
        code: vec![t.clone(), t],
        init: [0; MAXV],
        check: no_check,
    }
}
fn broken_lock() -> Prog {
    // test and set as two separate steps: both threads can see 0, both then store 1
    let t = vec![
        Load(0, 0),
        Jnz(0, 0),
        Store(0, I(1)),
        CsEnter,
        CsExit,
        Store(0, I(0)),
    ];
    Prog {
        name: "broken lock (load; store)",
        code: vec![t.clone(), t],
        init: [0; MAXV],
        check: no_check,
    }
}
fn peterson(fence: bool) -> Prog {
    // flag0 = var 0, flag1 = var 1, turn = var 2
    let thread = |me: usize| {
        let other = 1 - me;
        let mut c = vec![Store(me, I(1)), Store(2, I(other as i64))];
        if fence {
            c.push(Fence);
        }
        let l = c.len();
        c.extend([
            Load(0, other),
            Jz(0, l + 5),
            Load(1, 2),
            Sub(1, I(other as i64)),
            Jz(1, l),
            CsEnter,
            CsExit,
            Store(me, I(0)),
        ]);
        c
    };
    Prog {
        name: if fence {
            "Peterson + fence"
        } else {
            "Peterson (no fence)"
        },
        code: vec![thread(0), thread(1)],
        init: [0; MAXV],
        check: no_check,
    }
}
fn lost_update() -> Prog {
    let t = vec![Load(0, 0), Add(0, I(1)), Store(0, R(0))];
    fn chk(s: &St) -> Option<String> {
        (s.mem[0] != 2).then(|| format!("LOST UPDATE: counter = {}, expected 2", s.mem[0]))
    }
    Prog {
        name: "counter load/add/store",
        code: vec![t.clone(), t],
        init: [0; MAXV],
        check: chk,
    }
}
fn cas_counter() -> Prog {
    let t = vec![
        Load(0, 0),
        Mov(1, R(0)),
        Add(1, I(1)),
        Cas(2, 0, R(0), R(1)),
        Jz(2, 0),
    ];
    fn chk(s: &St) -> Option<String> {
        (s.mem[0] != 2).then(|| format!("LOST UPDATE: counter = {}", s.mem[0]))
    }
    Prog {
        name: "counter CAS loop",
        code: vec![t.clone(), t],
        init: [0; MAXV],
        check: chk,
    }
}
/// Stack A->B->C in vars: head = 0, next[idx] at var 1+idx, `bpopped` = var 5. Node ids 1,2,3 (0 = null).
fn aba(tagged: bool) -> Prog {
    let mut init = [0; MAXV];
    init[0] = 1; // head = A
    init[2] = 2; // next[A] = B
    init[3] = 3; // next[B] = C
    let (t1, t2);
    if !tagged {
        t1 = vec![Load(0, 0), LoadIdx(1, 1, 0), Cas(2, 0, R(0), R(1))];
        t2 = vec![
            Load(0, 0),
            LoadIdx(1, 1, 0),
            Cas(2, 0, R(0), R(1)),
            Jz(2, 11), // pop A
            Load(3, 0),
            LoadIdx(4, 1, 3),
            Cas(2, 0, R(3), R(4)),
            Jz(2, 11),             // pop B
            Store(5, I(1)),        // remember B is out of the stack
            StoreIdx(1, 0, R(4)),  // reuse node A: next[A] = C
            Cas(2, 0, R(4), R(0)), // push A back: head C -> A   (the "A" in ABA)
        ];
    } else {
        // head = tag*16 + idx. Every successful CAS bumps the tag, so a recycled idx never looks the same.
        let pop = |base: usize, ok: usize, end: usize| {
            let (h, i, nx, nh) = (base, base + 1, base + 2, 3usize);
            vec![
                Load(h, 0),
                Mov(i, R(h)),
                Mod(i, 16),
                LoadIdx(nx, 1, i),
                Mov(nh, R(h)),
                Sub(nh, R(i)),
                Add(nh, I(16)),
                Add(nh, R(nx)),
                Cas(ok, 0, R(h), R(nh)),
                Jz(ok, end),
            ]
        };
        t1 = pop(0, 4, 99);
        let mut c = pop(0, 4, 27);
        c.extend(pop(5, 4, 27));
        c.push(Store(5, I(1)));
        c.extend([
            StoreIdx(1, 1, R(7)),
            Mov(6, R(3)),
            Sub(6, R(7)),
            Add(6, I(16)),
            Add(6, R(1)),
            Cas(4, 0, R(3), R(6)),
        ]);
        t2 = c;
    }
    // ABA = T1 read head=A, T2 then popped A, popped B, pushed A back (flag in var 5), AND yet T1's stale CAS (head: A -> B) succeeded.
    fn chk_plain(s: &St) -> Option<String> {
        (s.mem[5] == 1 && s.regs[1][0] == 1 && s.regs[0][0] == 1 && s.regs[0][2] == 1).then(|| "ABA CORRUPTION: T1's CAS(head, A, B) succeeded after A was popped and re-pushed; B is back in the stack".to_string())
    }
    fn chk_tagged(s: &St) -> Option<String> {
        (s.mem[5] == 1 && s.regs[1][1] == 1 && s.regs[0][0] == 1 && s.regs[0][4] == 1)
            .then(|| "ABA CORRUPTION despite tags".to_string())
    }
    let chk: fn(&St) -> Option<String> = if tagged { chk_tagged } else { chk_plain };
    Prog {
        name: if tagged {
            "stack pop, tagged head"
        } else {
            "stack pop, plain head (ABA)"
        },
        code: vec![t1, t2],
        init,
        check: chk,
    }
}

// ---------------------------------------------------------------- driver
fn run(p: &Prog, relaxed: bool) -> (Stats, Stats, bool, Option<(Vec<Act>, String)>) {
    let cap = 3_000_000;
    let full_cfg = Cfg {
        relaxed,
        reduce: false,
        node_cap: cap,
    };
    let red_cfg = Cfg {
        relaxed,
        reduce: true,
        node_cap: cap,
    };
    let (mut full, mut red) = (Stats::default(), Stats::default());
    dfs(p, full_cfg, &p.init_state(), &[], &mut full);
    dfs(p, red_cfg, &p.init_state(), &[], &mut red);
    let cx = shortest_counterexample(p, red_cfg);
    // soundness: same verdict and (when the full run finished) identical set of final states
    if !full.capped {
        assert_eq!(
            full.violations > 0,
            red.violations > 0,
            "{}: reduction changed the verdict",
            p.name
        );
        assert_eq!(
            full.finals, red.finals,
            "{}: reduction lost a final state",
            p.name
        );
    }
    (full, red, relaxed, cx)
}

fn main() {
    let progs: Vec<(Prog, bool, bool)> = vec![
        // (program, relaxed?, expect violation?)
        (tas_spinlock(), false, false),
        (tas_spinlock(), true, false),
        (broken_lock(), false, true),
        (peterson(false), false, false),
        (peterson(false), true, true),
        (peterson(true), true, false),
        (lost_update(), false, true),
        (cas_counter(), false, false),
        (aba(false), false, true),
        (aba(true), false, false),
    ];
    println!(
        "loop bound = {LOOP_BOUND} backward jumps per thread (verdicts are BOUNDED proofs); schedules = complete executions explored"
    );
    println!(
        "{:<30}{:<5}{:>12}{:>12}{:>9}{:>9}  verdict",
        "program", "mem", "full", "sleep-sets", "factor", "pruned"
    );
    let mut first_traces = Vec::new();
    for (p, relaxed, expect_bug) in &progs {
        let t0 = Instant::now();
        let (full, red, _, cx) = run(p, *relaxed);
        let full_s = if full.capped {
            format!(">{}", full.nodes - 1)
        } else {
            full.schedules.to_string()
        };
        let factor = if full.capped {
            "n/a".to_string()
        } else {
            format!(
                "{:.1}x",
                full.schedules as f64 / red.schedules.max(1) as f64
            )
        };
        let found = cx.is_some();
        if found != *expect_bug {
            if let Some((path, msg)) = &cx {
                for l in render(
                    p,
                    Cfg {
                        relaxed: *relaxed,
                        reduce: false,
                        node_cap: 0,
                    },
                    path,
                ) {
                    println!("  {l}");
                }
                println!("  => {msg}");
            }
        }
        assert_eq!(found, *expect_bug, "{}: wrong verdict", p.name);
        println!(
            "{:<30}{:<5}{:>12}{:>12}{:>9}{:>9}  {} ({:.1}s)",
            p.name,
            if *relaxed { "TSO" } else { "SC" },
            full_s,
            red.schedules,
            factor,
            red.pruned,
            if found {
                "VIOLATION FOUND"
            } else {
                "safe within bound"
            },
            t0.elapsed().as_secs_f64()
        );
        if let Some((path, msg)) = cx {
            first_traces.push((p, *relaxed, path, msg));
        }
    }
    for (p, relaxed, path, msg) in first_traces {
        if !matches!(
            p.name,
            "broken lock (load; store)" | "Peterson (no fence)" | "stack pop, plain head (ABA)"
        ) {
            continue;
        }
        let cfg = Cfg {
            relaxed,
            reduce: false,
            node_cap: 0,
        };
        println!(
            "\n-- shortest counterexample: {} under {} ({} steps) --",
            p.name,
            if relaxed { "TSO" } else { "SC" },
            path.len()
        );
        for line in render(p, cfg, &path) {
            println!("  {line}");
        }
        println!("  => {msg}");
    }
    println!(
        "\nall model-checker verdicts match expectations; reduction preserved verdicts and final states"
    );
}
