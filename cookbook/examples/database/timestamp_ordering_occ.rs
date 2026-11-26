//! RECIPE 731 (database): Non-locking concurrency control: basic timestamp ordering, the Thomas write rule, optimistic CC (Kung-Robinson), and strict 2PL wait-die for comparison.
//! LEVEL: advanced
//! Use in: in-memory OLTP engines, main-memory stores, validation-based commit protocols, STMs.   Run: cargo run --release --example timestamp_ordering_occ
#![allow(dead_code)]
//!
//! Locking decides the serial order dynamically (whoever gets the lock first). The protocols here fix or discover the order
//! WITHOUT locks held across the transaction, and pay in aborts instead of waiting.
//! * BASIC TIMESTAMP ORDERING (T/O): each transaction gets a timestamp at start and must behave as if it ran at that instant.
//!   Each object keeps `rts` (largest reader ts) and `wts` (largest writer ts). A read with ts < wts arrives "too late" (a
//!   younger transaction already overwrote the value it should see): abort. A write with ts < rts (a younger reader already
//!   missed it) or ts < wts is late: abort. Every conflict edge then points from the smaller to the larger timestamp, so the
//!   precedence graph is acyclic by construction (the checker verifies exactly this).
//! * THOMAS WRITE RULE: a late write with rts <= ts < wts is merely OBSOLETE, nobody read what it would have written and a
//!   younger write already replaced it, so skip it instead of aborting. The result is view- but not conflict-serializable
//!   (blind writes), and it accepts strictly more schedules. We count that on random schedules.
//! * DIRTY DATA: writes here go in place with an undo record; a transaction that touches an object with an UNCOMMITTED writer
//!   WAITS for it (strict T/O). Waits only ever go toward smaller timestamps, so there are no deadlocks. (A late Thomas write
//!   behind an uncommitted writer aborts instead of waiting: waiting on a younger writer could deadlock.)
//!   MVTO (multi-version T/O) keeps old versions so that late READS succeed; it is covered by the MVCC recipes.
//! * OPTIMISTIC CONCURRENCY CONTROL (Kung and Robinson 1981), three phases. READ: run against the committed state, reads go to
//!   a read set, writes to a PRIVATE WORKSPACE. VALIDATION: BACKWARD validation checks our read set against the write sets of
//!   transactions that committed after we started (conflict -> abort ourselves). FORWARD validation instead checks our write
//!   set against the read sets of still-ACTIVE transactions, and may abort them (early) instead of us. WRITE: install the
//!   workspace. Validation + write run in one critical section here (serial validation). PARALLEL VALIDATION lets several
//!   transactions validate at once by also checking against the write sets of transactions that are validating but not yet
//!   written (the `finish_active` set): sketched in `parallel_validation_demo`.
//! * STRICT 2PL WITH WAIT-DIE: locks until commit; an OLDER requester waits for a younger holder, a YOUNGER requester dies and
//!   restarts KEEPING its original timestamp (so it eventually becomes the oldest: no starvation).
//! * Every committed history of every protocol (3000 random workloads x 5 protocols) goes through the precedence-graph checker,
//!   T/O must also respect timestamp order and OCC commit order, and the final store must equal the last committed write per
//!   key (so undo works). A table compares abort rates under low and high contention, and 8 real threads move money
//!   between accounts with OCC validation while the total balance must stay constant.
//!
//! COMMON USES: Silo / Hekaton / TicToc-style in-memory engines (OCC validation), Spanner-style timestamp ordering with
//! MVCC, software transactional memory, optimistic version checks in application code (`UPDATE .. WHERE version = ?`).
//! Course: CMU 15-445 Lecture 19 (timestamp ordering and optimistic concurrency control).
//! Read: "On Optimistic Methods for Concurrency Control" (Kung, Robinson, 1981) https://www.eecs.harvard.edu/~htk/publication/1981-tods-kung-robinson.pdf ,
//! "An Evaluation of Concurrency Control with One Thousand Cores" (Yu et al.), CMU 15-445 notes https://15445.courses.cs.cmu.edu/
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Proto {
    Basic,
    Thomas,
    OccBackward,
    OccForward,
    WaitDie,
}
const PROTOS: [Proto; 5] = [
    Proto::Basic,
    Proto::Thomas,
    Proto::OccBackward,
    Proto::OccForward,
    Proto::WaitDie,
];

fn proto_name(p: Proto) -> &'static str {
    match p {
        Proto::Basic => "basic T/O",
        Proto::Thomas => "T/O + Thomas",
        Proto::OccBackward => "OCC backward",
        Proto::OccForward => "OCC forward",
        Proto::WaitDie => "2PL wait-die",
    }
}

#[derive(Clone, Copy, Debug)]
enum Step {
    R(usize),
    W(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum St {
    Run,
    /// waiting for (transaction index, its attempt number) to finish
    Wait(usize, usize),
    Done,
}

enum Res {
    Ok,
    Block(usize),
    Abort,
}

struct Tx {
    prog: Vec<Step>,
    pc: usize,
    ts: u64,
    attempt: usize,
    st: St,
    undo: BTreeMap<usize, (i64, u64)>, // key -> (old value, old wts): T/O and 2PL write in place
    held: BTreeSet<usize>,             // 2PL: keys we hold locks on
    rs: BTreeSet<usize>,               // OCC read set
    ws: BTreeMap<usize, i64>,          // OCC private workspace
    start_tn: u64,
    doomed: bool, // forward validation marked us
    restarts: usize,
}

#[derive(Default, Clone)]
struct Lock {
    shared: BTreeSet<usize>,
    excl: Option<usize>,
}

struct Eng {
    proto: Proto,
    txs: Vec<Tx>,
    val: Vec<i64>,
    rts: Vec<u64>,
    wts: Vec<u64>,
    writer: Vec<Option<usize>>, // uncommitted in-place writer (tx index)
    locks: Vec<Lock>,
    tnc: u64, // OCC transaction number counter (= number of OCC commits)
    committed_ws: Vec<(u64, BTreeSet<usize>)>,
    clock: u64,
    next_attempt: usize,
    log: Vec<(usize, bool, usize)>, // (attempt, is_write, key) in execution order
    committed: BTreeSet<usize>,
    ts_of: HashMap<usize, u64>,
    commit_pos: HashMap<usize, u64>,
    commits: u64,
    aborts: u64,
    steps: u64,
    ignored: u64,
}

impl Eng {
    fn new(proto: Proto, nkeys: usize, progs: Vec<Vec<Step>>) -> Eng {
        let mut e = Eng {
            proto,
            txs: Vec::new(),
            val: vec![0; nkeys],
            rts: vec![0; nkeys],
            wts: vec![0; nkeys],
            writer: vec![None; nkeys],
            locks: vec![Lock::default(); nkeys],
            tnc: 0,
            committed_ws: Vec::new(),
            clock: 0,
            next_attempt: 1,
            log: Vec::new(),
            committed: BTreeSet::new(),
            ts_of: HashMap::new(),
            commit_pos: HashMap::new(),
            commits: 0,
            aborts: 0,
            steps: 0,
            ignored: 0,
        };
        for prog in progs {
            e.clock += 1;
            let tx = Tx {
                prog,
                pc: 0,
                ts: e.clock,
                attempt: e.next_attempt,
                st: St::Run,
                undo: BTreeMap::new(),
                held: BTreeSet::new(),
                rs: BTreeSet::new(),
                ws: BTreeMap::new(),
                start_tn: 0,
                doomed: false,
                restarts: 0,
            };
            e.next_attempt += 1;
            e.txs.push(tx);
        }
        e
    }

    fn is_occ(&self) -> bool {
        matches!(self.proto, Proto::OccBackward | Proto::OccForward)
    }

    fn read(&mut self, i: usize, k: usize) -> Res {
        let (ts, attempt) = (self.txs[i].ts, self.txs[i].attempt);
        match self.proto {
            Proto::Basic | Proto::Thomas => {
                if self.writer[k] == Some(i) {
                    return Res::Ok; // own uncommitted write
                }
                if ts < self.wts[k] {
                    return Res::Abort; // a younger transaction already overwrote what we should see
                }
                if let Some(w) = self.writer[k] {
                    return Res::Block(w); // strict: do not read dirty data
                }
                self.rts[k] = self.rts[k].max(ts);
                self.log.push((attempt, false, k));
                Res::Ok
            }
            Proto::WaitDie => match self.lock(i, k, false) {
                Res::Ok => {
                    self.log.push((attempt, false, k));
                    Res::Ok
                }
                r => r,
            },
            Proto::OccBackward | Proto::OccForward => {
                if !self.txs[i].ws.contains_key(&k) {
                    self.txs[i].rs.insert(k);
                    self.log.push((attempt, false, k));
                }
                Res::Ok
            }
        }
    }

    fn write(&mut self, i: usize, k: usize) -> Res {
        let (ts, attempt) = (self.txs[i].ts, self.txs[i].attempt);
        match self.proto {
            Proto::Basic | Proto::Thomas => {
                if self.writer[k] == Some(i) {
                    self.val[k] = attempt as i64;
                    self.log.push((attempt, true, k));
                    return Res::Ok;
                }
                if ts < self.rts[k] {
                    return Res::Abort; // a younger transaction already read the old value
                }
                if ts < self.wts[k] {
                    if self.proto == Proto::Thomas && self.writer[k].is_none() {
                        self.ignored += 1; // THOMAS WRITE RULE: obsolete write, skip it
                        return Res::Ok;
                    }
                    return Res::Abort;
                }
                if let Some(w) = self.writer[k] {
                    return Res::Block(w);
                }
                let old = (self.val[k], self.wts[k]);
                self.txs[i].undo.entry(k).or_insert(old);
                self.val[k] = attempt as i64;
                self.wts[k] = ts;
                self.writer[k] = Some(i);
                self.log.push((attempt, true, k));
                Res::Ok
            }
            Proto::WaitDie => match self.lock(i, k, true) {
                Res::Ok => {
                    let old = (self.val[k], 0);
                    self.txs[i].undo.entry(k).or_insert(old);
                    self.val[k] = attempt as i64;
                    self.log.push((attempt, true, k));
                    Res::Ok
                }
                r => r,
            },
            Proto::OccBackward | Proto::OccForward => {
                self.txs[i].ws.insert(k, attempt as i64);
                Res::Ok
            }
        }
    }

    /// Strict 2PL lock request with wait-die: smaller timestamp = older.
    fn lock(&mut self, i: usize, k: usize, excl: bool) -> Res {
        let conflicting: Vec<usize> = {
            let l = &self.locks[k];
            let mut v: Vec<usize> = l
                .shared
                .iter()
                .copied()
                .filter(|&h| h != i && excl)
                .collect();
            v.extend(l.excl.filter(|&h| h != i));
            v
        };
        if conflicting.is_empty() {
            let l = &mut self.locks[k];
            if excl {
                l.excl = Some(i);
                l.shared.remove(&i);
            } else if l.excl != Some(i) {
                l.shared.insert(i);
            }
            self.txs[i].held.insert(k);
            return Res::Ok;
        }
        let me = self.txs[i].ts;
        if conflicting.iter().all(|&h| self.txs[h].ts > me) {
            Res::Block(conflicting[0])
        } else {
            Res::Abort
        }
    }

    fn commit(&mut self, i: usize) -> Res {
        let attempt = self.txs[i].attempt;
        match self.proto {
            Proto::Basic | Proto::Thomas => {
                for &k in self.txs[i].undo.keys() {
                    self.writer[k] = None;
                }
            }
            Proto::WaitDie => self.release(i),
            Proto::OccBackward | Proto::OccForward => {
                if self.txs[i].doomed {
                    return Res::Abort;
                }
                // VALIDATION (backward): anyone who committed since we began and wrote something we read invalidates us.
                if self.proto == Proto::OccBackward {
                    let (start, rs) = (self.txs[i].start_tn, &self.txs[i].rs);
                    if self
                        .committed_ws
                        .iter()
                        .any(|(tn, w)| *tn > start && !w.is_disjoint(rs))
                    {
                        return Res::Abort;
                    }
                }
                // WRITE phase: install the private workspace.
                self.tnc += 1;
                let ws = std::mem::take(&mut self.txs[i].ws);
                for (&k, &v) in &ws {
                    self.val[k] = v;
                    self.log.push((attempt, true, k));
                }
                let keys: BTreeSet<usize> = ws.keys().copied().collect();
                if self.proto == Proto::OccForward {
                    // FORWARD validation: any active transaction that already read what we wrote is doomed (early abort).
                    for j in 0..self.txs.len() {
                        if j != i
                            && self.txs[j].st != St::Done
                            && !keys.is_disjoint(&self.txs[j].rs)
                        {
                            self.txs[j].doomed = true;
                        }
                    }
                }
                self.committed_ws.push((self.tnc, keys));
            }
        }
        self.committed.insert(attempt);
        self.ts_of.insert(attempt, self.txs[i].ts);
        self.commit_pos.insert(attempt, self.commits);
        self.commits += 1;
        Res::Ok
    }

    fn release(&mut self, i: usize) {
        for k in std::mem::take(&mut self.txs[i].held) {
            let l = &mut self.locks[k];
            l.shared.remove(&i);
            if l.excl == Some(i) {
                l.excl = None;
            }
        }
    }

    /// Roll back and restart. T/O restarts with a NEW timestamp (the old one is obsolete by definition); wait-die keeps the
    /// OLD one so the transaction ages; OCC re-reads from a fresh start.
    fn abort(&mut self, i: usize) {
        self.aborts += 1;
        let undo = std::mem::take(&mut self.txs[i].undo);
        for (k, (v, w)) in undo {
            self.val[k] = v;
            if matches!(self.proto, Proto::Basic | Proto::Thomas) {
                self.wts[k] = w;
                self.writer[k] = None;
            }
        }
        self.release(i);
        self.clock += 1;
        let (clock, tnc, na) = (self.clock, self.tnc, self.next_attempt);
        self.next_attempt += 1;
        let keep_ts = self.proto == Proto::WaitDie;
        let t = &mut self.txs[i];
        t.pc = 0;
        t.attempt = na;
        t.restarts += 1;
        t.rs.clear();
        t.ws.clear();
        t.doomed = false;
        t.start_tn = tnc;
        t.st = St::Run;
        if !keep_ts {
            t.ts = clock;
        }
    }

    fn runnable(&self, i: usize) -> bool {
        match self.txs[i].st {
            St::Run => true,
            St::Done => false,
            St::Wait(h, a) => self.txs[h].st == St::Done || self.txs[h].attempt != a,
        }
    }

    fn run(&mut self, rng: &mut Rng) {
        let n = self.txs.len();
        while self.txs.iter().any(|t| t.st != St::Done) {
            let ready: Vec<usize> = (0..n).filter(|&i| self.runnable(i)).collect();
            assert!(!ready.is_empty(), "deadlock under {:?}", self.proto);
            assert!(self.steps < 2_000_000, "livelock under {:?}", self.proto);
            let i = ready[rng.below(ready.len())];
            self.txs[i].st = St::Run;
            self.steps += 1;
            let pc = self.txs[i].pc;
            let res = if self.txs[i].doomed && self.is_occ() {
                Res::Abort // early abort thanks to forward validation
            } else if pc == self.txs[i].prog.len() {
                match self.commit(i) {
                    Res::Ok => {
                        self.txs[i].st = St::Done;
                        continue;
                    }
                    r => r,
                }
            } else {
                match self.txs[i].prog[pc] {
                    Step::R(k) => self.read(i, k),
                    Step::W(k) => self.write(i, k),
                }
            };
            match res {
                Res::Ok => self.txs[i].pc += 1,
                Res::Block(h) => self.txs[i].st = St::Wait(h, self.txs[h].attempt),
                Res::Abort => self.abort(i),
            }
        }
    }

    /// Verify: precedence graph of the committed attempts is acyclic and agrees with the protocol's serialization order;
    /// final store equals the last committed write per key.
    fn verify(&self) {
        let mut edges: BTreeSet<(usize, usize)> = BTreeSet::new();
        let log: Vec<&(usize, bool, usize)> = self
            .log
            .iter()
            .filter(|(a, _, _)| self.committed.contains(a))
            .collect();
        for (x, a) in log.iter().enumerate() {
            for b in &log[x + 1..] {
                if a.2 == b.2 && a.0 != b.0 && (a.1 || b.1) {
                    edges.insert((a.0, b.0));
                }
            }
        }
        let nodes: BTreeSet<usize> = self.committed.clone();
        assert!(
            acyclic(&nodes, &edges),
            "{:?}: committed history not conflict-serializable",
            self.proto
        );
        for &(a, b) in &edges {
            match self.proto {
                Proto::Basic | Proto::Thomas => assert!(
                    self.ts_of[&a] < self.ts_of[&b],
                    "T/O edge against timestamp order"
                ),
                Proto::OccBackward | Proto::OccForward => assert!(
                    self.commit_pos[&a] < self.commit_pos[&b],
                    "OCC edge against commit order"
                ),
                Proto::WaitDie => {}
            }
        }
        let mut last: HashMap<usize, i64> = HashMap::new();
        for (a, w, k) in &log {
            if *w {
                last.insert(*k, *a as i64);
            }
        }
        for k in 0..self.val.len() {
            assert_eq!(
                self.val[k],
                last.get(&k).copied().unwrap_or(0),
                "{:?}: store differs from committed writes at key {k}",
                self.proto
            );
        }
        assert_eq!(
            self.commits as usize,
            self.txs.len(),
            "every transaction eventually commits"
        );
    }
}

fn acyclic(nodes: &BTreeSet<usize>, edges: &BTreeSet<(usize, usize)>) -> bool {
    let mut indeg: BTreeMap<usize, usize> = nodes.iter().map(|&n| (n, 0)).collect();
    for &(_, b) in edges {
        *indeg.entry(b).or_default() += 1;
    }
    let mut ready: Vec<usize> = indeg
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(n, _)| *n)
        .collect();
    let mut seen = 0;
    while let Some(n) = ready.pop() {
        seen += 1;
        for &(a, b) in edges {
            if a == n {
                let d = indeg.get_mut(&b).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.push(b);
                }
            }
        }
    }
    seen == indeg.len()
}

/// Random transactions: `len` operations, ~40% writes, keys uniform over `nkeys`.
fn gen_workload(rng: &mut Rng, n_tx: usize, nkeys: usize) -> Vec<Vec<Step>> {
    (0..n_tx)
        .map(|_| {
            (0..3 + rng.below(4))
                .map(|_| {
                    let k = rng.below(nkeys);
                    if rng.below(10) < 4 {
                        Step::W(k)
                    } else {
                        Step::R(k)
                    }
                })
                .collect()
        })
        .collect()
}

fn run_one(proto: Proto, nkeys: usize, seed: u64) -> Eng {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let n_tx = 10 + rng.below(5);
    let progs = gen_workload(&mut rng, n_tx, nkeys);
    let mut e = Eng::new(proto, nkeys, progs);
    e.run(&mut rng);
    e.verify();
    e
}

fn demo_scripted() {
    println!(
        "-- scripted: the Thomas write rule on  T1(ts1): w(x)   T2(ts2): w(x)  with T2's write arriving first"
    );
    for proto in [Proto::Basic, Proto::Thomas] {
        let mut e = Eng::new(proto, 1, vec![vec![Step::W(0)], vec![Step::W(0)]]);
        assert!(matches!(e.write(1, 0), Res::Ok)); // T2 writes first
        assert!(matches!(e.commit(1), Res::Ok));
        e.txs[1].st = St::Done;
        let r = e.write(0, 0); // T1 (older) arrives late
        match proto {
            Proto::Basic => assert!(matches!(r, Res::Abort), "late write aborts under basic T/O"),
            _ => assert!(
                matches!(r, Res::Ok) && e.ignored == 1 && e.val[0] == 2,
                "Thomas skips it; T2's value stays"
            ),
        }
        println!(
            "   {:<14} late write by the older T1 -> {}",
            proto_name(proto),
            if matches!(r, Res::Abort) {
                "ABORT"
            } else {
                "ignored, T1 commits"
            }
        );
    }
    println!("-- scripted: OCC backward validation");
    let mut e = Eng::new(
        Proto::OccBackward,
        2,
        vec![vec![Step::R(0), Step::W(1)], vec![Step::W(0)]],
    );
    assert!(matches!(e.read(0, 0), Res::Ok)); // T1 reads x in its read phase
    assert!(matches!(e.write(1, 0), Res::Ok) && matches!(e.commit(1), Res::Ok)); // T2 writes x and commits meanwhile
    assert!(matches!(e.write(0, 1), Res::Ok));
    assert!(
        matches!(e.commit(0), Res::Abort),
        "T1's read of x is stale: validation fails"
    );
    println!(
        "   T1 read x, T2 committed a write of x during T1's read phase -> T1 fails validation, retries"
    );
}

/// Sketch of PARALLEL validation (Kung-Robinson section 3.3): T validates against (a) committed transactions since its start
/// (as before) and (b) every transaction currently in its validation/write phase (`finish_active`), for which it requires
/// both ws(other) intersect rs(T) and ws(other) intersect ws(T) to be empty, because the other's write may land before or
/// after ours and we cannot know which.
fn parallel_validation_demo() {
    println!("-- parallel validation sketch: T validates while others are validating");
    let set = |v: &[usize]| v.iter().copied().collect::<BTreeSet<usize>>();
    let ok = |rs: &BTreeSet<usize>,
              ws: &BTreeSet<usize>,
              finish_active: &[(BTreeSet<usize>, BTreeSet<usize>)]| {
        finish_active
            .iter()
            .all(|(_, other_ws)| other_ws.is_disjoint(rs) && other_ws.is_disjoint(ws))
    };
    let other = (set(&[1]), set(&[7])); // validating now: read {1}, will write {7}
    assert!(
        ok(&set(&[1, 2]), &set(&[3]), &[other.clone()]),
        "disjoint: both may proceed in parallel"
    );
    assert!(
        !ok(&set(&[7]), &set(&[3]), &[other.clone()]),
        "T read what the other will write"
    );
    assert!(
        !ok(&set(&[2]), &set(&[7]), &[other]),
        "write-write overlap: install order unknown, so abort"
    );
    println!(
        "   disjoint sets validate in parallel; read-write and write-write overlap with a validating peer abort"
    );
}

/// Count how many random schedules each rule accepts WITHOUT any abort. Timestamps = order of first operation.
fn thomas_vs_basic_acceptance() {
    println!(
        "-- schedule acceptance: 3000 random schedules (3-4 txns, 2-3 keys), timestamp = order of first operation"
    );
    let mut rng = Rng(0x00c0_ffee_1234_5678);
    let (mut basic_ok, mut thomas_ok, mut thomas_only_ignored) = (0, 0, 0);
    for _ in 0..3000 {
        let (n_tx, nk) = (3 + rng.below(2), 2 + rng.below(2));
        let progs = gen_workload(&mut rng, n_tx, nk);
        let mut cur = vec![0usize; n_tx];
        let mut sched: Vec<(usize, Step)> = Vec::new();
        loop {
            let live: Vec<usize> = (0..n_tx).filter(|&t| cur[t] < progs[t].len()).collect();
            if live.is_empty() {
                break;
            }
            let t = live[rng.below(live.len())];
            sched.push((t, progs[t][cur[t]]));
            cur[t] += 1;
        }
        let mut ts = vec![0u64; n_tx];
        for (pos, (t, _)) in sched.iter().enumerate() {
            if ts[*t] == 0 {
                ts[*t] = pos as u64 + 1;
            }
        }
        let accept = |thomas: bool| -> Option<u32> {
            let (mut rts, mut wts) = (vec![0u64; nk], vec![0u64; nk]);
            let mut ignored = 0;
            for &(t, step) in &sched {
                match step {
                    Step::R(k) => {
                        if ts[t] < wts[k] {
                            return None;
                        }
                        rts[k] = rts[k].max(ts[t]);
                    }
                    Step::W(k) => {
                        if ts[t] < rts[k] {
                            return None;
                        }
                        if ts[t] < wts[k] {
                            if !thomas {
                                return None;
                            }
                            ignored += 1;
                            continue;
                        }
                        wts[k] = ts[t];
                    }
                }
            }
            Some(ignored)
        };
        let (b, t) = (accept(false), accept(true));
        assert!(
            b.is_none() || t.is_some(),
            "Thomas accepts everything basic T/O accepts"
        );
        basic_ok += b.is_some() as i32;
        thomas_ok += t.is_some() as i32;
        thomas_only_ignored += (b.is_none() && t.is_some()) as i32;
    }
    println!(
        "   accepted without aborts: basic T/O {basic_ok}, Thomas {thomas_ok}  (accepted only thanks to a skipped write: {thomas_only_ignored})"
    );
    assert!(thomas_ok > basic_ok && thomas_only_ignored == thomas_ok - basic_ok);
}

fn random_correctness() {
    println!(
        "-- 3000 random workloads x 5 protocols (keys: 1000 / 12 / 4 by seed): every committed history verified"
    );
    let mut totals: HashMap<&str, (u64, u64, u64)> = HashMap::new();
    for seed in 1..=3000u64 {
        let nkeys = [1000, 12, 4][(seed % 3) as usize];
        for proto in PROTOS {
            let e = run_one(proto, nkeys, seed);
            let t = totals.entry(proto_name(proto)).or_default();
            *t = (t.0 + e.commits, t.1 + e.aborts, t.2 + e.ignored);
        }
    }
    for proto in PROTOS {
        let (c, a, ig) = totals[proto_name(proto)];
        println!(
            "   {:<14} commits {c:>6}  aborts {a:>6}  skipped Thomas writes {ig:>4}",
            proto_name(proto)
        );
    }
    assert!(totals["T/O + Thomas"].2 > 0, "the rule fires");
    assert_eq!(totals["basic T/O"].2, 0);
}

fn contention_table() {
    println!(
        "-- contention: 400 workloads of ~12 txns each (work = executed steps, aborted attempts waste theirs)"
    );
    println!(
        "   {:<14}{:>12}{:>16}{:>18}{:>14}",
        "protocol", "contention", "aborts/commit", "commits/1k steps", "max restarts"
    );
    let mut rate: HashMap<(&str, bool), f64> = HashMap::new();
    for proto in PROTOS {
        for (label, nkeys) in [("low (1000)", 1000usize), ("high (4)", 4)] {
            let (mut c, mut a, mut s, mut mx) = (0u64, 0u64, 0u64, 0usize);
            for seed in 1..=400u64 {
                let e = run_one(proto, nkeys, seed + 77);
                c += e.commits;
                a += e.aborts;
                s += e.steps;
                mx = mx.max(e.txs.iter().map(|t| t.restarts).max().unwrap());
            }
            let r = a as f64 / c as f64;
            rate.insert((proto_name(proto), nkeys == 4), r);
            println!(
                "   {:<14}{label:>12}{r:>16.3}{:>18.1}{mx:>14}",
                proto_name(proto),
                c as f64 * 1000.0 / s as f64
            );
        }
    }
    for proto in PROTOS {
        let n = proto_name(proto);
        assert!(
            rate[&(n, true)] > rate[&(n, false)],
            "{n}: contention must raise the abort rate"
        );
    }
    println!(
        "   hot keys hurt everyone: T/O and wait-die (younger dies at once, retries into the same holders) restart a lot; OCC forward dooms stale readers early and wastes least."
    );
}

/// 8 real threads, 16 accounts. Read phase reads (balance, version) without holding any lock; validation + write phase locks
/// the two accounts in index order (no deadlock), re-checks both versions, then applies and bumps them.
fn threaded_bank() {
    const ACCTS: usize = 16;
    const THREADS: u64 = 8;
    const PER_THREAD: usize = 20_000;
    let accts: Vec<Mutex<(i64, u64)>> = (0..ACCTS).map(|_| Mutex::new((1000, 0))).collect();
    let (commits, aborts, skipped) = (AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0));
    let start = std::time::Instant::now();
    std::thread::scope(|s| {
        for t in 0..THREADS {
            let (accts, commits, aborts, skipped) = (&accts, &commits, &aborts, &skipped);
            s.spawn(move || {
                let mut rng =
                    Rng(0x1234_5678_9abc_def1 ^ (t + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                for _ in 0..PER_THREAD {
                    let a = rng.below(ACCTS);
                    let b = (a + 1 + rng.below(ACCTS - 1)) % ACCTS;
                    let amount = 1 + rng.below(300) as i64;
                    loop {
                        // READ phase (no locks held across both reads: the pair may be inconsistent, validation will notice)
                        let (ba, va) = *accts[a].lock().unwrap();
                        let (_, vb) = *accts[b].lock().unwrap();
                        if ba < amount {
                            skipped.fetch_add(1, Ordering::Relaxed); // application-level abort: insufficient funds
                            break;
                        }
                        // VALIDATION + WRITE phase
                        let (lo, hi) = (a.min(b), a.max(b));
                        let mut first = accts[lo].lock().unwrap();
                        let mut second = accts[hi].lock().unwrap();
                        let (from, to) = if a < b {
                            (&mut *first, &mut *second)
                        } else {
                            (&mut *second, &mut *first)
                        };
                        if from.1 == va && to.1 == vb {
                            from.0 -= amount;
                            to.0 += amount;
                            from.1 += 1;
                            to.1 += 1;
                            commits.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                        aborts.fetch_add(1, Ordering::Relaxed); // someone changed an account under us: retry
                    }
                }
            });
        }
    });
    let balances: Vec<i64> = accts.iter().map(|m| m.lock().unwrap().0).collect();
    let total: i64 = balances.iter().sum();
    let (c, a, sk) = (
        commits.load(Ordering::Relaxed),
        aborts.load(Ordering::Relaxed),
        skipped.load(Ordering::Relaxed),
    );
    println!(
        "-- threaded OCC bank: {THREADS} threads x {PER_THREAD} transfers over {ACCTS} accounts: commits {c}, validation aborts {a}, insufficient funds {sk}, {:?}",
        start.elapsed()
    );
    assert_eq!(total, ACCTS as i64 * 1000, "money is conserved");
    assert!(balances.iter().all(|&b| b >= 0), "no overdraft");
    assert_eq!(
        c + sk,
        THREADS * PER_THREAD as u64,
        "every transfer either committed or was rejected"
    );
}

fn main() {
    demo_scripted();
    parallel_validation_demo();
    thomas_vs_basic_acceptance();
    random_correctness();
    contention_table();
    threaded_bank();
    println!("all timestamp-ordering / OCC checks passed");
}
