//! RECIPE 411 (database): a lock manager: multi-granularity intention locks, FIFO queues, upgrades, deadlock detection and prevention.
//! LEVEL: advanced
//! Use in: lock-based DBMS concurrency control (strict 2PL), filesystems and resource schedulers, any "wait for a resource" system.   Run: cargo run --release --example lock_manager_deadlock
#![allow(dead_code)]
//!
//! A LOCK MANAGER maps resources to queues of requests. Modes: S (shared read), X (exclusive write) and the INTENTION modes
//! for a hierarchy database > table > row: IS ("I will read-lock something below"), IX ("I will write-lock something below")
//! and SIX (S on the whole table plus intent to X-lock rows). To lock a row you first take intention locks on every ancestor,
//! root to leaf, so a table-level S/X request only has to look at ONE lock to know whether any row below is in use. The
//! COMPATIBILITY MATRIX decides who may share a resource; modes form a lattice (IS < IX,S < SIX < X) and a request by a
//! transaction that already holds the resource becomes an UPGRADE to the join of the two modes (S + IX = SIX).
//! * FIFO FAIRNESS: a new request is granted only if it is compatible with every granted AND every already-waiting request.
//!   Otherwise a steady stream of readers would starve a waiting writer forever. Upgrades go to the FRONT of the wait queue.
//! * STRICT TWO-PHASE LOCKING: locks are only taken while running and ALL are released at commit/abort. This guarantees
//!   conflict-serializable and recoverable (no cascading abort) schedules; the demo verifies it with a precedence graph.
//! * DEADLOCK: waiting creates a WAIT-FOR GRAPH (edge T -> U: T waits for a lock U holds, or for an incompatible request queued
//!   ahead of it). A cycle is a deadlock; DETECTION searches the graph after every block and aborts a VICTIM (youngest, or
//!   the one holding the fewest locks = least work lost). Classic upgrade deadlock: two readers both ask to upgrade S -> X.
//! * PREVENTION needs no graph, only a fixed timestamp per transaction (kept across restarts so the oldest eventually wins):
//!   WAIT-DIE (old waits for young; a young requester DIES = aborts itself) and WOUND-WAIT (an old requester WOUNDS = aborts
//!   the young holder; a young one waits). Waits always point one way along age, so no cycle can form.
//! * The scheduler is deterministic (seeded xorshift) and single threaded: a blocked transaction simply is not scheduled.
//!
//! COMMON USES: PostgreSQL, MySQL InnoDB and SQL Server row/table locking, Oracle; Spanner-style wound-wait for distributed
//! transactions; file locks (flock/fcntl), Java/OS monitors with deadlock detectors; any ordered-resource allocator.
//! Read: "Database System Concepts" ch. on concurrency control, "Transaction Processing" (Gray, Reuter), Mohan et al. (ARIES, locking),
//! https://www.postgresql.org/docs/current/explicit-locking.html , https://dev.mysql.com/doc/refman/8.4/en/innodb-locking.html
use std::collections::{BTreeMap, BTreeSet, HashMap};

type TxId = usize; // also the timestamp: smaller = older

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    IS,
    IX,
    S,
    SIX,
    X,
}
use Mode::*;
const MODES: [Mode; 5] = [IS, IX, S, SIX, X];

fn compatible(a: Mode, b: Mode) -> bool {
    // rows/columns in the order IS IX S SIX X
    const M: [[bool; 5]; 5] = [
        [true, true, true, true, false],
        [true, true, false, false, false],
        [true, false, true, false, false],
        [true, false, false, false, false],
        [false, false, false, false, false],
    ];
    M[a as usize][b as usize]
}

/// least upper bound in the lattice IS < {IX, S} < SIX < X
fn join(a: Mode, b: Mode) -> Mode {
    match (a, b) {
        _ if a == b => a,
        (X, _) | (_, X) => X,
        (SIX, _) | (_, SIX) => SIX,
        (IS, m) | (m, IS) => m,
        _ => SIX, // IX + S
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Res {
    Db,
    Table(usize),
    Row(usize),
}

const ROWS: usize = 6;
const ROWS_PER_TABLE: usize = 3;
fn table_of(row: usize) -> usize {
    row / ROWS_PER_TABLE
}

impl std::fmt::Display for Res {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Res::Db => write!(f, "db"),
            Res::Table(t) => write!(f, "table{t}"),
            Res::Row(r) => write!(f, "row{r}"),
        }
    }
}

struct Entry {
    txn: TxId,
    mode: Mode,
    granted: bool, // a waiting entry of a txn that also has a granted entry is an UPGRADE
}

#[derive(PartialEq, Debug)]
enum Outcome {
    Granted,
    Waiting,
}

#[derive(Default)]
struct LockManager {
    table: BTreeMap<Res, Vec<Entry>>,
}

impl LockManager {
    fn acquire(&mut self, txn: TxId, res: Res, mode: Mode) -> Outcome {
        let q = self.table.entry(res).or_default();
        assert!(
            !q.iter().any(|e| e.txn == txn && !e.granted),
            "a blocked transaction cannot issue requests"
        );
        if let Some(g) = q.iter().position(|e| e.txn == txn && e.granted) {
            let want = join(q[g].mode, mode);
            if want == q[g].mode {
                return Outcome::Granted; // already strong enough
            }
            if q.iter()
                .all(|e| e.txn == txn || !e.granted || compatible(e.mode, want))
            {
                q[g].mode = want; // in-place upgrade
                return Outcome::Granted;
            }
            let pos = q.iter().position(|e| !e.granted).unwrap_or(q.len()); // front of the waiters
            q.insert(
                pos,
                Entry {
                    txn,
                    mode: want,
                    granted: false,
                },
            );
            return Outcome::Waiting;
        }
        let ok = q.iter().all(|e| e.txn == txn || compatible(e.mode, mode)); // granted AND waiting: FIFO fairness
        q.push(Entry {
            txn,
            mode,
            granted: ok,
        });
        if ok {
            Outcome::Granted
        } else {
            Outcome::Waiting
        }
    }

    /// grant as many waiters as the FIFO rule allows; returns the transactions that were woken
    fn grant_pass(q: &mut Vec<Entry>) -> Vec<TxId> {
        let (mut woke, mut blocked_ahead, mut i) = (vec![], Vec::<(TxId, Mode)>::new(), 0);
        while i < q.len() {
            if q[i].granted {
                i += 1;
                continue;
            }
            let (t, m) = (q[i].txn, q[i].mode);
            let ok = q
                .iter()
                .all(|e| e.txn == t || !e.granted || compatible(e.mode, m))
                && blocked_ahead
                    .iter()
                    .all(|&(bt, bm)| bt == t || compatible(bm, m));
            if !ok {
                blocked_ahead.push((t, m)); // later requests may not overtake an incompatible waiter
                i += 1;
                continue;
            }
            match q.iter().position(|e| e.txn == t && e.granted) {
                Some(g) => {
                    q[g].mode = m; // finish the upgrade
                    q.remove(i);
                }
                None => {
                    q[i].granted = true;
                    i += 1;
                }
            }
            woke.push(t);
        }
        woke
    }

    /// strict 2PL release point: drop every lock and pending request of `txn`
    fn release_all(&mut self, txn: TxId) -> Vec<TxId> {
        let mut woke = vec![];
        for q in self.table.values_mut() {
            if q.iter().any(|e| e.txn == txn) {
                q.retain(|e| e.txn != txn);
                woke.extend(Self::grant_pass(q));
            }
        }
        self.table.retain(|_, q| !q.is_empty());
        woke
    }

    fn held_count(&self, txn: TxId) -> usize {
        self.table
            .values()
            .flatten()
            .filter(|e| e.txn == txn && e.granted)
            .count()
    }

    /// wait-for edges (waiter, blocker, resource)
    fn wait_edges(&self) -> Vec<(TxId, TxId, Res)> {
        let mut out = vec![];
        for (&res, q) in &self.table {
            for (i, w) in q.iter().enumerate().filter(|(_, e)| !e.granted) {
                for (j, e) in q.iter().enumerate() {
                    let blocks =
                        e.txn != w.txn && !compatible(e.mode, w.mode) && (e.granted || j < i);
                    if blocks {
                        out.push((w.txn, e.txn, res));
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    fn blockers(&self, txn: TxId) -> BTreeSet<TxId> {
        self.wait_edges()
            .into_iter()
            .filter(|e| e.0 == txn)
            .map(|e| e.1)
            .collect()
    }

    fn graph(&self) -> BTreeMap<TxId, BTreeSet<TxId>> {
        let mut g: BTreeMap<TxId, BTreeSet<TxId>> = BTreeMap::new();
        for (a, b, _) in self.wait_edges() {
            g.entry(a).or_default().insert(b);
        }
        g
    }

    /// every pair of granted modes held by different transactions on a resource must be compatible
    fn check_invariants(&self) {
        for (res, q) in &self.table {
            for (i, a) in q.iter().enumerate().filter(|(_, e)| e.granted) {
                for b in q[i + 1..].iter().filter(|e| e.granted && e.txn != a.txn) {
                    assert!(
                        compatible(a.mode, b.mode),
                        "{res}: {:?} and {:?} granted together",
                        a.mode,
                        b.mode
                    );
                }
            }
        }
    }
}

fn find_cycle(g: &BTreeMap<TxId, BTreeSet<TxId>>) -> Option<Vec<TxId>> {
    fn dfs(
        g: &BTreeMap<TxId, BTreeSet<TxId>>,
        u: TxId,
        color: &mut HashMap<TxId, u8>,
        path: &mut Vec<TxId>,
    ) -> Option<Vec<TxId>> {
        color.insert(u, 1);
        path.push(u);
        for &v in g.get(&u).into_iter().flatten() {
            match color.get(&v).copied().unwrap_or(0) {
                1 => return Some(path[path.iter().position(|&x| x == v).unwrap()..].to_vec()),
                0 => {
                    if let Some(c) = dfs(g, v, color, path) {
                        return Some(c);
                    }
                }
                _ => {}
            }
        }
        path.pop();
        color.insert(u, 2);
        None
    }
    let mut color = HashMap::new();
    g.keys().find_map(|&u| {
        if color.contains_key(&u) {
            None
        } else {
            dfs(g, u, &mut color, &mut vec![])
        }
    })
}

// ---- transactions and the scheduler -------------------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
enum Op {
    Read(usize),
    Write(usize),
    Scan(usize),              // read every row of a table under one table S lock
    ScanUpdate(usize, usize), // SIX on the table, X on one row
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Lock(Res, Mode),
    Data(bool, usize), // (is_write, row)
    Commit,
}

/// expand an operation into the lock path (root to leaf) followed by its data accesses
fn expand(op: Op) -> Vec<Step> {
    match op {
        Op::Read(r) => vec![
            Step::Lock(Res::Db, IS),
            Step::Lock(Res::Table(table_of(r)), IS),
            Step::Lock(Res::Row(r), S),
            Step::Data(false, r),
        ],
        Op::Write(r) => vec![
            Step::Lock(Res::Db, IX),
            Step::Lock(Res::Table(table_of(r)), IX),
            Step::Lock(Res::Row(r), X),
            Step::Data(true, r),
        ],
        Op::Scan(t) => {
            let mut v = vec![Step::Lock(Res::Db, IS), Step::Lock(Res::Table(t), S)];
            v.extend((t * ROWS_PER_TABLE..(t + 1) * ROWS_PER_TABLE).map(|r| Step::Data(false, r)));
            v
        }
        Op::ScanUpdate(t, r) => {
            assert_eq!(table_of(r), t);
            let mut v = vec![
                Step::Lock(Res::Db, IX),
                Step::Lock(Res::Table(t), SIX),
                Step::Lock(Res::Row(r), X),
            ];
            v.extend((t * ROWS_PER_TABLE..(t + 1) * ROWS_PER_TABLE).map(|r| Step::Data(false, r)));
            v.push(Step::Data(true, r));
            v
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Victim {
    Youngest,
    LeastWork,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Policy {
    Nothing,
    Detect(Victim),
    WaitDie,
    WoundWait,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Status {
    Running,
    Blocked,
    Committed,
}

struct Tx {
    script: Vec<Step>,
    pc: usize,
    status: Status,
    attempt: usize,
    restarts: usize,
}

#[derive(Default, Debug)]
struct Stats {
    deadlocks: usize,
    dies: usize,
    wounds: usize,
}

struct Sim {
    policy: Policy,
    lm: LockManager,
    tx: Vec<Tx>,
    log: Vec<(TxId, usize, bool, usize)>, // (txn, attempt, is_write, row)
    stats: Stats,
}

impl Sim {
    fn new(policy: Policy, scripts: &[Vec<Op>]) -> Sim {
        let tx = scripts
            .iter()
            .map(|ops| {
                let mut script: Vec<Step> = ops.iter().flat_map(|&o| expand(o)).collect();
                script.push(Step::Commit);
                Tx {
                    script,
                    pc: 0,
                    status: Status::Running,
                    attempt: 0,
                    restarts: 0,
                }
            })
            .collect();
        Sim {
            policy,
            lm: LockManager::default(),
            tx,
            log: vec![],
            stats: Stats::default(),
        }
    }

    /// abort + immediate restart with the SAME timestamp (so the oldest transaction is never starved by restarts)
    fn abort(&mut self, t: TxId) {
        let woke = self.lm.release_all(t);
        let tx = &mut self.tx[t];
        tx.pc = 0;
        tx.status = Status::Running;
        tx.attempt += 1;
        tx.restarts += 1;
        for w in woke {
            self.tx[w].status = Status::Running;
            self.tx[w].pc += 1; // its pending lock was granted
        }
    }

    fn pick_victim(&self, cycle: &[TxId], v: Victim) -> TxId {
        match v {
            Victim::Youngest => *cycle.iter().max().unwrap(),
            Victim::LeastWork => *cycle
                .iter()
                .min_by_key(|&&t| (self.lm.held_count(t), usize::MAX - t))
                .unwrap(),
        }
    }

    /// deadlock detection: abort victims until the wait-for graph is acyclic
    fn detect_and_resolve(&mut self, v: Victim) -> Vec<TxId> {
        let mut victims = vec![];
        while let Some(cycle) = find_cycle(&self.lm.graph()) {
            let victim = self.pick_victim(&cycle, v);
            self.abort(victim);
            self.stats.deadlocks += 1;
            victims.push(victim);
        }
        victims
    }

    /// apply the policy to the current wait-for relation. Re-checked after EVERY lock request, not just for the one that
    /// blocked: an in-place upgrade can make an already waiting transaction wait for a new, unchecked holder.
    fn enforce(&mut self) {
        match self.policy {
            Policy::Nothing => {}
            Policy::Detect(v) => {
                self.detect_and_resolve(v);
            }
            Policy::WaitDie => {
                // a waiter that is younger than any of its blockers dies; afterwards every wait edge points old -> young
                while let Some(t) = (0..self.tx.len()).find(|&t| {
                    self.tx[t].status == Status::Blocked
                        && self.lm.blockers(t).iter().any(|&b| b < t)
                }) {
                    self.abort(t);
                    self.stats.dies += 1;
                }
            }
            Policy::WoundWait => {
                // an older waiter wounds every younger blocker; afterwards every wait edge points young -> old
                while let Some(b) = (0..self.tx.len())
                    .filter(|&t| self.tx[t].status == Status::Blocked)
                    .find_map(|t| self.lm.blockers(t).into_iter().find(|&b| b > t))
                {
                    self.abort(b);
                    self.stats.wounds += 1;
                }
            }
        }
    }

    fn step(&mut self, t: TxId) {
        assert_eq!(self.tx[t].status, Status::Running);
        match self.tx[t].script[self.tx[t].pc] {
            Step::Lock(res, mode) => match self.lm.acquire(t, res, mode) {
                Outcome::Granted => self.tx[t].pc += 1,
                Outcome::Waiting => {
                    self.tx[t].status = Status::Blocked;
                }
            },
            Step::Data(w, row) => {
                self.log.push((t, self.tx[t].attempt, w, row));
                self.tx[t].pc += 1;
            }
            Step::Commit => {
                for w in self.lm.release_all(t) {
                    self.tx[w].status = Status::Running;
                    self.tx[w].pc += 1;
                }
                self.tx[t].status = Status::Committed;
            }
        }
        self.enforce();
        self.lm.check_invariants();
        if self.policy != Policy::Nothing {
            assert!(
                find_cycle(&self.lm.graph()).is_none(),
                "{:?} must never leave a deadlock",
                self.policy
            );
        }
    }

    /// run `n` steps of one transaction (scripted interleavings)
    fn steps(&mut self, t: TxId, n: usize) {
        for _ in 0..n {
            if self.tx[t].status == Status::Running {
                self.step(t);
            }
        }
    }

    /// random scheduling until everyone commits; false = stuck (everyone left is blocked)
    fn run(&mut self, rng: &mut Rng) -> bool {
        for _ in 0..50_000 {
            let runnable: Vec<TxId> = (0..self.tx.len())
                .filter(|&t| self.tx[t].status == Status::Running)
                .collect();
            if runnable.is_empty() {
                return self.tx.iter().all(|t| t.status == Status::Committed);
            }
            let t = runnable[rng.below(runnable.len())];
            self.step(t);
        }
        panic!("no progress after 50000 steps under {:?}", self.policy);
    }

    /// precedence graph over the committed (final-attempt) data accesses; acyclic <=> conflict-serializable
    fn precedence_cycle(&self) -> Option<Vec<TxId>> {
        let ops: Vec<_> = self
            .log
            .iter()
            .filter(|e| e.1 == self.tx[e.0].attempt)
            .collect();
        let mut g: BTreeMap<TxId, BTreeSet<TxId>> =
            (0..self.tx.len()).map(|t| (t, BTreeSet::new())).collect();
        for (i, a) in ops.iter().enumerate() {
            for b in &ops[i + 1..] {
                if a.0 != b.0 && a.3 == b.3 && (a.2 || b.2) {
                    g.get_mut(&a.0).unwrap().insert(b.0); // a happened first
                }
            }
        }
        find_cycle(&g)
    }
}

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

fn random_workload(seed: u64) -> Vec<Vec<Op>> {
    let mut r = Rng(seed * 2654435761 + 1);
    (0..5)
        .map(|_| {
            (0..2 + r.below(3))
                .map(|_| match r.below(10) {
                    0..=3 => Op::Read(r.below(ROWS)),
                    4..=7 => Op::Write(r.below(ROWS)),
                    8 => Op::Scan(r.below(2)),
                    _ => {
                        let t = r.below(2);
                        Op::ScanUpdate(t, t * ROWS_PER_TABLE + r.below(ROWS_PER_TABLE))
                    }
                })
                .collect()
        })
        .collect()
}

// ---- demos -------------------------------------------------------------------------------------------------------------
fn demo_matrix() {
    println!("-- compatibility matrix (Y = can be held together by different transactions)");
    print!("        ");
    for m in MODES {
        print!("{:>4}", format!("{m:?}"));
    }
    println!();
    for a in MODES {
        print!("   {:<5}", format!("{a:?}"));
        for b in MODES {
            print!("{:>4}", if compatible(a, b) { "Y" } else { "." });
            assert_eq!(compatible(a, b), compatible(b, a), "symmetric");
            let j = join(a, b);
            assert_eq!(j, join(b, a));
            // the join is at least as strong as both: anything incompatible with a or b is incompatible with j
            for c in MODES {
                if !compatible(a, c) || !compatible(b, c) {
                    assert!(
                        !compatible(j, c),
                        "join({a:?},{b:?}) must conflict with {c:?}"
                    );
                }
            }
        }
        println!();
    }
    assert!(MODES.iter().all(|&m| !compatible(X, m)));
    assert_eq!(join(IX, S), SIX);
    println!(
        "   join(IX, S) = SIX: one transaction reading a table and updating some rows needs exactly that."
    );
}

fn demo_hierarchy_and_fifo() {
    println!("-- multi-granularity locking and FIFO fairness");
    let mut lm = LockManager::default();
    // T0 reads row1, T1 writes row2 (same table): intention locks coexist, row locks differ
    for (t, mode_i, mode_r, row) in [(0, IS, S, 1), (1, IX, X, 2)] {
        assert_eq!(lm.acquire(t, Res::Db, mode_i), Outcome::Granted);
        assert_eq!(lm.acquire(t, Res::Table(0), mode_i), Outcome::Granted);
        assert_eq!(lm.acquire(t, Res::Row(row), mode_r), Outcome::Granted);
    }
    // T2 wants to read the whole table: S conflicts with T1's IX, one check at table level
    assert_eq!(lm.acquire(2, Res::Db, IS), Outcome::Granted);
    assert_eq!(lm.acquire(2, Res::Table(0), S), Outcome::Waiting);
    // T3 only wants IS on the same table: compatible with everything granted, but T2 is queued; IS vs S compatible so it passes
    assert_eq!(lm.acquire(3, Res::Table(0), IS), Outcome::Granted);
    // T4 wants IX: compatible with the granted locks but NOT with the waiting S -> it must queue behind T2 (no starvation)
    assert_eq!(lm.acquire(4, Res::Table(0), IX), Outcome::Waiting);
    println!(
        "   table0 queue: T0 IS, T1 IX granted; T2 S waits (blocked by T1's IX); T3 IS passes; T4 IX waits behind T2"
    );
    let edges = lm.wait_edges();
    assert!(edges.contains(&(2, 1, Res::Table(0))) && edges.contains(&(4, 2, Res::Table(0))));
    assert!(
        lm.release_all(1).contains(&2),
        "when the writer commits, the table scan proceeds"
    );
    assert_eq!(
        lm.table[&Res::Table(0)]
            .iter()
            .find(|e| e.txn == 4)
            .unwrap()
            .granted,
        false,
        "IX still waits for S"
    );
    lm.release_all(2);
    assert!(
        lm.table[&Res::Table(0)]
            .iter()
            .find(|e| e.txn == 4)
            .unwrap()
            .granted
    );
    // plain FIFO: readers share, a waiting writer blocks later readers
    let mut lm = LockManager::default();
    assert_eq!(lm.acquire(0, Res::Row(0), S), Outcome::Granted);
    assert_eq!(lm.acquire(1, Res::Row(0), X), Outcome::Waiting);
    assert_eq!(
        lm.acquire(2, Res::Row(0), S),
        Outcome::Waiting,
        "S is compatible with the holder, but must not overtake the waiting X"
    );
    lm.release_all(0);
    assert!(lm.table[&Res::Row(0)][0].granted && lm.table[&Res::Row(0)][0].mode == X);
    lm.release_all(1);
    assert!(lm.table[&Res::Row(0)][0].granted && lm.table[&Res::Row(0)][0].txn == 2);
    println!(
        "   row0: S holder, X waits, a later S queues behind X (FIFO); order of grants was T0, T1, T2"
    );
}

fn print_graph(lm: &LockManager) {
    for (a, b, res) in lm.wait_edges() {
        let held = lm.table[&res]
            .iter()
            .find(|e| e.txn == b && e.granted)
            .unwrap();
        println!(
            "      T{} --waits for {} ({:?} held by T{})--> T{}",
            a + 1,
            res,
            held.mode,
            b + 1,
            b + 1
        );
    }
}

fn demo_classic_deadlock() {
    println!("-- the classic deadlock: T1 locks row0 then row1, T2 locks row1 then row0");
    let scripts = vec![
        vec![Op::Write(0), Op::Write(1)],
        vec![Op::Write(1), Op::Write(0)],
    ];
    let mut sim = Sim::new(Policy::Nothing, &scripts);
    sim.steps(0, 4); // T1: db IX, table IX, row0 X, data
    sim.steps(1, 4); // T2: row1 X
    sim.steps(0, 3); // T1 asks row1 -> blocks
    sim.steps(1, 3); // T2 asks row0 -> blocks: cycle
    assert!(sim.tx.iter().all(|t| t.status == Status::Blocked));
    println!("   wait-for graph at the deadlock:");
    print_graph(&sim.lm);
    let cycle = find_cycle(&sim.lm.graph()).expect("deadlock");
    println!(
        "   cycle found by DFS: {:?}",
        cycle
            .iter()
            .map(|t| format!("T{}", t + 1))
            .collect::<Vec<_>>()
    );
    assert_eq!(cycle.len(), 2);
    let victims = sim.detect_and_resolve(Victim::Youngest);
    assert_eq!(victims, vec![1], "the youngest (T2) is the victim");
    assert_eq!(
        sim.tx[0].status,
        Status::Running,
        "T1 was woken: its row1 lock was granted"
    );
    let mut rng = Rng(5);
    assert!(sim.run(&mut rng));
    assert_eq!((sim.tx[0].restarts, sim.tx[1].restarts), (0, 1));
    assert!(sim.precedence_cycle().is_none());
    println!(
        "   victim T2 aborted and restarted; both committed (restarts: T1={}, T2={})",
        sim.tx[0].restarts, sim.tx[1].restarts
    );
}

fn demo_upgrade_deadlock() {
    println!(
        "-- lock upgrade deadlock: both transactions read row0 (S) and then want to write it (S -> X)"
    );
    let scripts = vec![
        vec![Op::Read(0), Op::Write(0)],
        vec![Op::Read(0), Op::Write(0)],
    ];
    for policy in [
        Policy::Nothing,
        Policy::Detect(Victim::Youngest),
        Policy::WaitDie,
        Policy::WoundWait,
    ] {
        let mut sim = Sim::new(policy, &scripts);
        sim.steps(0, 4); // read row0 completely
        sim.steps(1, 4);
        sim.steps(0, 3); // db IX (upgrade IS->IX ok), table IX (ok), row0 S -> X: waits for T2's S
        sim.steps(1, 3);
        if policy == Policy::Nothing {
            assert!(find_cycle(&sim.lm.graph()).is_some());
            println!(
                "   {:<22} stuck: T1 waits for T2's S and T2 waits for T1's S",
                format!("{policy:?}")
            );
            print_graph(&sim.lm);
            continue;
        }
        let ok = sim.run(&mut Rng(9));
        assert!(ok && sim.precedence_cycle().is_none());
        println!(
            "   {:<22} resolved: {:?}, restarts T1={} T2={}",
            format!("{policy:?}"),
            sim.stats,
            sim.tx[0].restarts,
            sim.tx[1].restarts
        );
        assert_eq!(
            sim.tx[0].restarts, 0,
            "the older transaction never restarts"
        );
    }
}

fn random_workloads() {
    println!(
        "-- 3000 random workloads (5 txns, 2 tables x 3 rows, reads/writes/scans/SIX) x 5 policies"
    );
    println!(
        "   {:<22}{:>9}{:>10}{:>7}{:>8}{:>10}{:>17}",
        "policy", "finished", "deadlocks", "dies", "wounds", "restarts", "oldest restarts"
    );
    let policies = [
        Policy::Nothing,
        Policy::Detect(Victim::Youngest),
        Policy::Detect(Victim::LeastWork),
        Policy::WaitDie,
        Policy::WoundWait,
    ];
    for policy in policies {
        let (mut finished, mut restarts, mut oldest, mut deadlocks, mut dies, mut wounds) =
            (0, 0, 0, 0, 0, 0);
        for seed in 1..=3000u64 {
            let mut sim = Sim::new(policy, &random_workload(seed));
            let done = sim.run(&mut Rng(seed ^ 0xABCDEF));
            if done {
                finished += 1;
                // strict 2PL => conflict-serializable
                assert!(
                    sim.precedence_cycle().is_none(),
                    "seed {seed}: strict 2PL history not serializable"
                );
            } else {
                assert_eq!(
                    policy,
                    Policy::Nothing,
                    "only the unprotected policy may deadlock (seed {seed})"
                );
                assert!(
                    find_cycle(&sim.lm.graph()).is_some(),
                    "stuck means a real wait-for cycle"
                );
            }
            restarts += sim.tx.iter().map(|t| t.restarts).sum::<usize>();
            oldest += sim.tx[0].restarts;
            deadlocks += sim.stats.deadlocks;
            dies += sim.stats.dies;
            wounds += sim.stats.wounds;
        }
        println!(
            "   {:<22}{:>9}{:>10}{:>7}{:>8}{:>10}{:>17}",
            format!("{policy:?}"),
            finished,
            deadlocks,
            dies,
            wounds,
            restarts,
            oldest
        );
        match policy {
            Policy::Nothing => {
                assert!(finished < 3000, "without handling, some workloads deadlock")
            }
            _ => assert_eq!(finished, 3000),
        }
        if policy != Policy::Detect(Victim::LeastWork) && policy != Policy::Nothing {
            assert_eq!(
                oldest, 0,
                "{policy:?}: the oldest transaction is never aborted, so it cannot starve"
            );
        }
    }
    println!(
        "   (LeastWork may abort the oldest transaction: it minimises lost work but gives up the no-starvation guarantee.)"
    );
}

fn main() {
    demo_matrix();
    demo_hierarchy_and_fifo();
    demo_classic_deadlock();
    demo_upgrade_deadlock();
    random_workloads();
    println!("all lock manager checks passed");
}
