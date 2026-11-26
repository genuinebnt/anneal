//! RECIPE 410 (database): MVCC snapshot isolation: version chains, first-committer-wins, GC, write skew, and SSI.
//! LEVEL: advanced
//! Use in: PostgreSQL / Oracle / MySQL-InnoDB style engines, CockroachDB-style stores, any "readers never block writers" system.   Run: cargo run --release --example mvcc_snapshot_isolation
#![allow(dead_code)]
//!
//! MULTI-VERSION CONCURRENCY CONTROL keeps every committed value of a key as a VERSION `[begin_ts, end_ts)` in a chain instead
//! of overwriting it. A TIMESTAMP ORACLE hands out commit timestamps (here: a counter). A transaction remembers the last commit
//! timestamp it saw at `begin` (its SNAPSHOT) and reads, for every key, the version whose interval contains that timestamp.
//! Consequences: readers never block or abort, a read-only transaction sees one frozen database (no dirty reads, no
//! non-repeatable reads, no phantoms), and writers only fight writers.
//! * WRITES are buffered in the transaction (so nobody can read them dirty) and installed atomically at commit with one new
//!   commit timestamp. A read first looks at the own buffer: READ-YOUR-WRITES.
//! * FIRST-COMMITTER-WINS: at commit, if any key we wrote has a version committed after our snapshot, a concurrent
//!   transaction already won; we abort. This is what prevents LOST UPDATE (read-modify-write races). Real engines often
//!   do first-UPDATER-wins (lock at write time) but the outcome is the same.
//! * GARBAGE COLLECTION: a version whose `end_ts` is <= the oldest active snapshot is invisible to everyone, now and later,
//!   so it can be dropped. One long-running transaction therefore pins every version created after it started (bloat).
//! * SNAPSHOT ISOLATION (SI) is NOT serializable: two concurrent transactions that read overlapping data and write DISJOINT
//!   keys both pass first-committer-wins. WRITE SKEW (two on-call doctors each see the other on duty and both go off duty)
//!   is the classic result. In dependency-graph terms (Adya) SI forbids cycles of wr/ww edges but allows a cycle containing
//!   two consecutive RW-ANTIDEPENDENCY edges `A -rw-> P -rw-> B` between concurrent transactions (Fekete et al.).
//! * SERIALIZABLE SNAPSHOT ISOLATION (SSI, Cahill; PostgreSQL's SERIALIZABLE) runs SI and additionally tracks rw edges
//!   (T1 read something a concurrent T2 wrote). A transaction that would become a PIVOT (an incoming and an outgoing rw edge,
//!   with the outgoing target already committed) is aborted. This is conservative (some false aborts) but never misses a cycle.
//!   Here the read sets double as SIREAD locks, and a scan reads every key of its range (the key universe is a small integer
//!   domain), which makes phantoms visible as ordinary rw edges. A real engine uses predicate / gap / page-level SIREAD locks.
//! * The SERIALIZABILITY CHECKER builds the dependency graph of the committed history (wr, ww, rw edges from the version
//!   order) and searches for a cycle. Acyclic <=> conflict-serializable. 2000 random interleavings are run per isolation level.
//!
//! COMMON USES: PostgreSQL, Oracle, SQL Server (snapshot mode), MySQL InnoDB (repeatable read), CockroachDB and TiDB
//! transactions, FoundationDB; in-memory STMs (software transactional memory); long analytical reads on live data.
//! Read: "A Critique of ANSI SQL Isolation Levels" (Berenson et al.), "Serializable Snapshot Isolation in PostgreSQL" (Ports,
//! Grittner), "Making Snapshot Isolation Serializable" (Fekete et al.). https://www.postgresql.org/docs/current/transaction-iso.html ,
//! https://www.vldb.org/pvldb/vol5/p1850_michaelcahill_vldb2012.pdf
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Instant;

type Key = u32;
type Ts = u64;
/// transaction id; 0 means "the initial (empty) database" as a version writer
type TxId = usize;
const INF: Ts = u64::MAX;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// every read sees the latest committed value; no write-write check
    ReadCommitted,
    /// snapshot reads + first-committer-wins
    Snapshot,
    /// snapshot isolation + rw-antidependency tracking
    Serializable,
}

#[derive(Clone, Debug)]
struct Version {
    begin: Ts,
    end: Ts,
    value: Option<i64>, // None = tombstone (deleted)
    writer: TxId,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Status {
    Active,
    Committed(Ts),
    Aborted,
}

struct Txn {
    start: Ts,
    status: Status,
    writes: BTreeMap<Key, Option<i64>>, // buffered until commit
    reads: Vec<(Key, TxId)>,            // key and the writer of the version seen
}

#[derive(PartialEq, Debug)]
enum Abort {
    WriteConflict(Key),
    Pivot,
}

struct Db {
    mode: Mode,
    chains: BTreeMap<Key, Vec<Version>>, // oldest first
    last_commit: Ts,                     // the timestamp oracle
    txns: Vec<Txn>,                      // TxId = index + 1
}

impl Db {
    fn new(mode: Mode) -> Db {
        Db {
            mode,
            chains: BTreeMap::new(),
            last_commit: 0,
            txns: Vec::new(),
        }
    }

    /// a database preloaded by one committed transaction (commit ts 1)
    fn seeded(mode: Mode, rows: &[(Key, i64)]) -> Db {
        let mut db = Db::new(mode);
        let t = db.begin();
        for &(k, v) in rows {
            db.write(t, k, Some(v));
        }
        db.commit(t).unwrap();
        db
    }

    fn tx(&self, id: TxId) -> &Txn {
        &self.txns[id - 1]
    }

    fn begin(&mut self) -> TxId {
        self.txns.push(Txn {
            start: self.last_commit,
            status: Status::Active,
            writes: BTreeMap::new(),
            reads: vec![],
        });
        self.txns.len()
    }

    /// the version of `key` visible at `ts`: begin <= ts < end
    fn visible(&self, key: Key, ts: Ts) -> (Option<i64>, TxId) {
        let found = self
            .chains
            .get(&key)
            .and_then(|c| c.iter().rev().find(|v| v.begin <= ts && ts < v.end));
        found.map_or((None, 0), |v| (v.value, v.writer))
    }

    fn read(&mut self, t: TxId, key: Key) -> Option<i64> {
        if let Some(v) = self.tx(t).writes.get(&key) {
            return *v; // read-your-writes
        }
        // READ COMMITTED re-takes its snapshot at every statement; SI keeps the one from `begin`
        let ts = if self.mode == Mode::ReadCommitted {
            self.last_commit
        } else {
            self.tx(t).start
        };
        let (value, writer) = self.visible(key, ts);
        self.txns[t - 1].reads.push((key, writer));
        value
    }

    /// range scan over the integer key domain; reads every key (absent keys too) so inserts into the range are tracked
    fn scan(&mut self, t: TxId, lo: Key, hi: Key) -> Vec<(Key, i64)> {
        (lo..hi)
            .filter_map(|k| self.read(t, k).map(|v| (k, v)))
            .collect()
    }

    fn write(&mut self, t: TxId, key: Key, value: Option<i64>) {
        self.txns[t - 1].writes.insert(key, value);
    }

    fn abort(&mut self, t: TxId) {
        let tx = &mut self.txns[t - 1];
        tx.status = Status::Aborted;
        tx.writes.clear();
    }

    fn fail(&mut self, t: TxId, why: Abort) -> Result<Ts, Abort> {
        self.abort(t);
        Err(why)
    }

    fn commit(&mut self, t: TxId) -> Result<Ts, Abort> {
        assert_eq!(self.tx(t).status, Status::Active);
        if self.mode != Mode::ReadCommitted {
            let start = self.tx(t).start;
            let lost = self.tx(t).writes.keys().find(|k| {
                self.chains
                    .get(k)
                    .and_then(|c| c.last())
                    .is_some_and(|v| v.begin > start)
            });
            if let Some(&k) = lost {
                return self.fail(t, Abort::WriteConflict(k));
            }
        }
        if self.mode == Mode::Serializable && self.is_dangerous(t) {
            return self.fail(t, Abort::Pivot);
        }
        self.last_commit += 1;
        let c = self.last_commit;
        for (&k, &v) in &self.txns[t - 1].writes {
            let chain = self.chains.entry(k).or_default();
            if let Some(prev) = chain.last_mut() {
                prev.end = c; // the old version stops being current at exactly our commit ts
            }
            chain.push(Version {
                begin: c,
                end: INF,
                value: v,
                writer: t,
            });
        }
        self.txns[t - 1].status = Status::Committed(c);
        Ok(c)
    }

    // ---- SSI -------------------------------------------------------------------------------------------------------
    /// (start, end) of a transaction's lifetime; an active one has not ended
    fn span(&self, u: TxId) -> (Ts, Ts) {
        match self.tx(u).status {
            Status::Committed(c) => (self.tx(u).start, c),
            _ => (self.tx(u).start, INF),
        }
    }

    fn concurrent(&self, a: TxId, b: TxId) -> bool {
        let (sa, ea) = self.span(a);
        let (sb, eb) = self.span(b);
        sa < eb && sb < ea
    }

    /// rw-antidependency `r -> w`: r read a key that w writes (so r's snapshot cannot contain w's version)
    fn rw(&self, r: TxId, w: TxId) -> bool {
        self.tx(r)
            .reads
            .iter()
            .any(|(k, _)| self.tx(w).writes.contains_key(k))
    }

    fn committed_at(&self, u: TxId) -> Option<Ts> {
        match self.tx(u).status {
            Status::Committed(c) => Some(c),
            _ => None,
        }
    }

    /// would committing `t` complete a dangerous structure `A -rw-> P -rw-> B` where B committed first?
    fn is_dangerous(&self, t: TxId) -> bool {
        let live: Vec<TxId> = (1..=self.txns.len())
            .filter(|&u| u != t && self.tx(u).status != Status::Aborted && self.concurrent(t, u))
            .collect();
        let out_committed: Vec<TxId> = live
            .iter()
            .copied()
            .filter(|&u| self.committed_at(u).is_some() && self.rw(t, u))
            .collect();
        let has_in = live.iter().any(|&u| self.rw(u, t));
        if !out_committed.is_empty() && has_in {
            return true; // t is the pivot
        }
        // t is the "A": it adds the incoming edge to an already committed pivot P that has an earlier-committed out target B
        out_committed.iter().any(|&p| {
            let pc = self.committed_at(p).unwrap();
            (1..=self.txns.len()).any(|b| {
                b != p
                    && self.committed_at(b).is_some_and(|bc| bc < pc)
                    && self.concurrent(p, b)
                    && self.rw(p, b)
            })
        })
    }

    // ---- GC and inspection -------------------------------------------------------------------------------------------
    fn horizon(&self) -> Ts {
        let snaps = self
            .txns
            .iter()
            .filter(|t| t.status == Status::Active && self.mode != Mode::ReadCommitted)
            .map(|t| t.start);
        snaps.min().unwrap_or(self.last_commit)
    }

    /// drop versions no snapshot can ever see again; returns how many were removed
    fn gc(&mut self) -> usize {
        let h = self.horizon();
        let before = self.total_versions();
        for chain in self.chains.values_mut() {
            chain.retain(|v| v.end > h);
            // a lone tombstone older than every snapshot is the same as "never existed"
            if chain.len() == 1 && chain[0].value.is_none() && chain[0].begin <= h {
                chain.clear();
            }
        }
        self.chains.retain(|_, c| !c.is_empty());
        before - self.total_versions()
    }

    fn total_versions(&self) -> usize {
        self.chains.values().map(Vec::len).sum()
    }

    fn dump(&self, key: Key) -> String {
        let fmt_ts = |t: Ts| {
            if t == INF {
                "inf".to_string()
            } else {
                t.to_string()
            }
        };
        let parts: Vec<String> = self
            .chains
            .get(&key)
            .into_iter()
            .flatten()
            .map(|v| {
                format!(
                    "[{},{})={}",
                    v.begin,
                    fmt_ts(v.end),
                    v.value.map_or("del".to_string(), |x| x.to_string())
                )
            })
            .collect();
        format!("k{key}: {}", parts.join(" "))
    }

    fn history(&self) -> Vec<Hist> {
        let mut h = vec![];
        for (i, t) in self.txns.iter().enumerate() {
            if let Status::Committed(c) = t.status {
                h.push(Hist {
                    id: i + 1,
                    commit: c,
                    reads: t.reads.clone(),
                    writes: t.writes.keys().copied().collect(),
                });
            }
        }
        h
    }
}

// ---- serializability checker ---------------------------------------------------------------------------------------
struct Hist {
    id: TxId,
    commit: Ts,
    reads: Vec<(Key, TxId)>,
    writes: Vec<Key>,
}

/// Adya's direct serialization graph: wr (read from), ww (next version), rw (overwrites the version I read)
fn dependency_graph(h: &[Hist]) -> BTreeMap<TxId, BTreeSet<TxId>> {
    let mut order: HashMap<Key, Vec<(Ts, TxId)>> = HashMap::new(); // version order = commit order
    for t in h {
        for &k in &t.writes {
            order.entry(k).or_default().push((t.commit, t.id));
        }
    }
    for v in order.values_mut() {
        v.sort();
    }
    let mut g: BTreeMap<TxId, BTreeSet<TxId>> = h.iter().map(|t| (t.id, BTreeSet::new())).collect();
    let mut edge = |a: TxId, b: TxId| {
        if a != b && a != 0 {
            g.get_mut(&a).unwrap().insert(b);
        }
    };
    for v in order.values() {
        for w in v.windows(2) {
            edge(w[0].1, w[1].1); // ww
        }
    }
    for t in h {
        for &(k, w) in &t.reads {
            edge(w, t.id); // wr
            let vers = order.get(&k).map(Vec::as_slice).unwrap_or(&[]);
            let next = if w == 0 {
                vers.first()
            } else {
                vers.iter()
                    .position(|x| x.1 == w)
                    .and_then(|p| vers.get(p + 1))
            };
            if let Some(&(_, n)) = next {
                edge(t.id, n); // rw: t read the version just before n's
            }
        }
    }
    g
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
        for &v in &g[&u] {
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

// ---- anomaly scenarios (scripted interleavings) --------------------------------------------------------------------
fn dirty_read(mode: Mode) -> bool {
    let mut db = Db::seeded(mode, &[(1, 10)]);
    let (t1, t2) = (db.begin(), db.begin());
    db.write(t1, 1, Some(99)); // uncommitted
    db.read(t2, 1) == Some(99)
}

fn non_repeatable_read(mode: Mode) -> bool {
    let mut db = Db::seeded(mode, &[(1, 10)]);
    let (t1, t2) = (db.begin(), db.begin());
    let first = db.read(t1, 1);
    db.write(t2, 1, Some(11));
    db.commit(t2).unwrap();
    first != db.read(t1, 1)
}

fn phantom(mode: Mode) -> bool {
    let mut db = Db::seeded(mode, &[(1, 10), (2, 20)]);
    let (t1, t2) = (db.begin(), db.begin());
    let before = db.scan(t1, 0, 10).len();
    db.write(t2, 5, Some(50)); // INSERT into the scanned range
    db.commit(t2).unwrap();
    before != db.scan(t1, 0, 10).len()
}

fn lost_update(mode: Mode) -> bool {
    let mut db = Db::seeded(mode, &[(1, 100)]);
    let (t1, t2) = (db.begin(), db.begin());
    let (a, b) = (db.read(t1, 1).unwrap(), db.read(t2, 1).unwrap());
    db.write(t1, 1, Some(a + 10));
    db.write(t2, 1, Some(b + 10));
    let r1 = db.commit(t1);
    let r2 = db.commit(t2);
    assert!(r1.is_ok());
    if mode != Mode::ReadCommitted {
        assert!(
            matches!(r2, Err(Abort::WriteConflict(1))),
            "first committer wins"
        );
    }
    let t3 = db.begin();
    // an update is LOST only if both commits succeeded yet the sum of increments is missing (an abort is a visible retry)
    r2.is_ok() && db.read(t3, 1) != Some(120)
}

/// the on-call doctors: invariant "at least one doctor on duty". Returns (commit results, doctors left on duty)
fn write_skew(mode: Mode) -> ([Result<Ts, Abort>; 2], i64) {
    let mut db = Db::seeded(mode, &[(1, 1), (2, 1)]); // 1 = on call
    let (t1, t2) = (db.begin(), db.begin());
    for (t, me) in [(t1, 1), (t2, 2)] {
        let on_duty: i64 = db.scan(t, 1, 3).iter().map(|x| x.1).sum();
        if on_duty >= 2 {
            db.write(t, me, Some(0)); // "someone else is on duty, I can leave"
        }
    }
    let r = [db.commit(t1), db.commit(t2)];
    let t3 = db.begin();
    let left = db.scan(t3, 1, 3).iter().map(|x| x.1).sum();
    (r, left)
}

// ---- random histories ------------------------------------------------------------------------------------------------
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

#[derive(Clone, Copy)]
enum Op {
    Read(Key),
    Scan(Key, Key),
    Write(Key, Option<i64>),
}

const N_TXNS: usize = 5;
const N_KEYS: usize = 4;

fn gen_script(r: &mut Rng) -> Vec<Op> {
    (0..2 + r.below(3))
        .map(|_| match r.below(20) {
            0..=7 => Op::Read(r.below(N_KEYS) as Key),
            8..=10 => {
                let lo = r.below(N_KEYS - 1);
                Op::Scan(lo as Key, (lo + 2 + r.below(2)).min(N_KEYS) as Key)
            }
            11..=12 => Op::Write(r.below(N_KEYS) as Key, None),
            _ => Op::Write(r.below(N_KEYS) as Key, Some(r.below(100) as i64)),
        })
        .collect()
}

struct Slot {
    ops: Vec<Op>,
    pc: usize,
    id: Option<TxId>,
    own: BTreeMap<Key, Option<i64>>,
    done: bool,
}

struct RunStats {
    committed: usize,
    aborted: usize,
    serializable: bool,
    cycle: Vec<TxId>,
}

/// one random interleaving of N_TXNS transactions; SI/SSI reads are checked against a model of every committed state
fn run_history(mode: Mode, seed: u64) -> RunStats {
    let mut plan = Rng(seed * 2 + 1);
    let mut sched = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut slots: Vec<Slot> = (0..N_TXNS)
        .map(|_| Slot {
            ops: gen_script(&mut plan),
            pc: 0,
            id: None,
            own: BTreeMap::new(),
            done: false,
        })
        .collect();
    let mut db = Db::new(mode);
    let mut states: Vec<BTreeMap<Key, i64>> = vec![BTreeMap::new()]; // states[c] = database after commit ts c (the oracle)
    let (mut committed, mut aborted) = (0, 0);
    loop {
        let live: Vec<usize> = (0..N_TXNS).filter(|&i| !slots[i].done).collect();
        if live.is_empty() {
            break;
        }
        let s = &mut slots[live[sched.below(live.len())]];
        let t = *s.id.get_or_insert_with(|| db.begin());
        let check = mode != Mode::ReadCommitted;
        if s.pc == s.ops.len() {
            match db.commit(t) {
                Ok(c) => {
                    assert_eq!(c as usize, states.len());
                    let mut st = states[c as usize - 1].clone();
                    for (&k, &v) in &s.own {
                        match v {
                            Some(x) => st.insert(k, x),
                            None => st.remove(&k),
                        };
                    }
                    states.push(st);
                    committed += 1;
                }
                Err(_) => aborted += 1,
            }
            s.done = true;
            continue;
        }
        let op = s.ops[s.pc];
        s.pc += 1;
        let snap = db.tx(t).start as usize;
        match op {
            Op::Read(k) => {
                let got = db.read(t, k);
                if check && !s.own.contains_key(&k) {
                    assert_eq!(
                        got,
                        states[snap].get(&k).copied(),
                        "snapshot read must match the model at ts {snap}"
                    );
                }
            }
            Op::Scan(lo, hi) => {
                let got = db.scan(t, lo, hi);
                if check && !s.own.keys().any(|k| (lo..hi).contains(k)) {
                    let want: Vec<(Key, i64)> =
                        states[snap].range(lo..hi).map(|(&k, &v)| (k, v)).collect();
                    assert_eq!(got, want);
                }
            }
            Op::Write(k, v) => {
                db.write(t, k, v);
                s.own.insert(k, v);
            }
        }
    }
    // final state and GC must agree with the model; with no active transaction exactly one version per live key remains
    let reader = db.begin();
    let finals: BTreeMap<Key, i64> = db.scan(reader, 0, N_KEYS as Key).into_iter().collect();
    assert_eq!(&finals, states.last().unwrap());
    db.abort(reader);
    db.gc();
    assert_eq!(db.total_versions(), finals.len());
    let g = dependency_graph(&db.history());
    let cycle = find_cycle(&g).unwrap_or_default();
    RunStats {
        committed,
        aborted,
        serializable: cycle.is_empty(),
        cycle,
    }
}

// ---- demos -------------------------------------------------------------------------------------------------------------
fn demo_versions_and_gc() {
    println!("-- version chains, read-your-writes, garbage collection");
    let mut db = Db::seeded(Mode::Snapshot, &[(1, 10)]);
    let old_reader = db.begin(); // snapshot = ts 1
    for v in [20, 30, 40] {
        let w = db.begin();
        assert_eq!(db.read(w, 1), Some(if v == 20 { 10 } else { v - 10 }));
        db.write(w, 1, Some(v));
        assert_eq!(db.read(w, 1), Some(v), "read-your-writes before commit");
        let other = db.begin();
        assert_ne!(
            db.read(other, 1),
            Some(v),
            "uncommitted write is invisible to others"
        );
        db.commit(w).unwrap();
    }
    println!("   {}", db.dump(1));
    assert_eq!(db.total_versions(), 4);
    let freed = db.gc();
    println!(
        "   gc with a snapshot at ts 1 still open freed {freed}: {}",
        db.dump(1)
    );
    assert_eq!(
        db.read(old_reader, 1),
        Some(10),
        "the old snapshot still sees its version"
    );
    db.abort(old_reader);
    for t in 1..=db.txns.len() {
        if db.tx(t).status == Status::Active {
            db.abort(t); // the `other` probe transactions also pinned old snapshots
        }
    }
    let freed = db.gc();
    println!("   after the readers end, gc freed {freed}: {}", db.dump(1));
    assert_eq!(db.total_versions(), 1);
    // deleting leaves a tombstone, which gc removes once no snapshot can see the value
    let d = db.begin();
    db.write(d, 1, None);
    db.commit(d).unwrap();
    assert_eq!(db.total_versions(), 2);
    db.gc();
    assert_eq!(db.total_versions(), 0);
}

fn anomaly_table() {
    println!(
        "-- anomalies by isolation level (yes = the anomaly HAPPENED in the scripted interleaving)"
    );
    let modes = [Mode::ReadCommitted, Mode::Snapshot, Mode::Serializable];
    let rows: [(&str, fn(Mode) -> bool, [bool; 3]); 4] = [
        ("dirty read", dirty_read, [false, false, false]),
        (
            "non-repeatable read",
            non_repeatable_read,
            [true, false, false],
        ),
        ("phantom (scan)", phantom, [true, false, false]),
        ("lost update", lost_update, [true, false, false]),
    ];
    println!(
        "   {:<22}{:>6}{:>6}{:>6}{:>10}",
        "anomaly", "RC", "SI", "SSI", "strict2PL"
    );
    let yn = |b: bool| if b { "yes" } else { "no" };
    for (name, f, want) in rows {
        let got: Vec<bool> = modes.iter().map(|&m| f(m)).collect();
        println!(
            "   {:<22}{:>6}{:>6}{:>6}{:>10}",
            name,
            yn(got[0]),
            yn(got[1]),
            yn(got[2]),
            "no"
        );
        assert_eq!(got, want, "{name}");
    }
    let skew: Vec<i64> = modes.iter().map(|&m| write_skew(m).1).collect();
    println!(
        "   {:<22}{:>6}{:>6}{:>6}{:>10}",
        "write skew",
        yn(skew[0] == 0),
        yn(skew[1] == 0),
        yn(skew[2] == 0),
        "no"
    );
    assert_eq!(
        skew,
        vec![0, 0, 1],
        "RC and SI end with nobody on duty; SSI keeps one doctor"
    );
    println!(
        "   (strict 2PL column is the textbook result: reads take locks, so it blocks instead of allowing these.)"
    );
    println!(
        "   {:<24}{:<26}{:<30}{}",
        "", "RC", "SI / SSI", "strict 2PL"
    );
    println!(
        "   {:<24}{:<26}{:<30}{}",
        "readers block writers?", "no", "no", "yes (S locks held to the end)"
    );
    println!(
        "   {:<24}{:<26}{:<30}{}",
        "writers block readers?", "no", "no", "yes"
    );
    println!(
        "   {:<24}{:<26}{:<30}{}",
        "failure mode",
        "silent anomalies",
        "abort on conflict / pivot",
        "deadlock (wait-for cycle)"
    );
    println!(
        "   {:<24}{:<26}{:<30}{}",
        "extra storage", "versions", "versions + SIREAD info", "lock table"
    );
}

fn doctors_trace() {
    println!(
        "-- write skew trace: Alice(k1) and Bob(k2) are both on call; each leaves if the other is on duty"
    );
    for mode in [Mode::Snapshot, Mode::Serializable] {
        let (r, left) = write_skew(mode);
        println!(
            "   {:?}: T1 commit = {:?}, T2 commit = {:?}, doctors left on duty = {left}",
            mode, r[0], r[1]
        );
        assert!(r[0].is_ok());
        match mode {
            Mode::Snapshot => assert!(
                r[1].is_ok() && left == 0,
                "SI lets both commit: invariant broken"
            ),
            _ => assert!(
                r[1] == Err(Abort::Pivot) && left == 1,
                "SSI aborts the pivot"
            ),
        }
    }
}

fn random_histories() {
    println!(
        "-- 2000 random interleavings ({N_TXNS} txns, {N_KEYS} keys, reads/scans/writes/deletes) checked with the dependency-graph cycle test"
    );
    let mut shown = false;
    for mode in [Mode::ReadCommitted, Mode::Snapshot, Mode::Serializable] {
        let start = Instant::now();
        let (mut nonser, mut commits, mut aborts) = (0, 0, 0);
        for seed in 1..=2000u64 {
            let st = run_history(mode, seed);
            commits += st.committed;
            aborts += st.aborted;
            if !st.serializable {
                nonser += 1;
                if mode == Mode::Snapshot && !shown {
                    shown = true;
                    println!("   example SI cycle (seed {seed}): {:?}", st.cycle);
                }
            }
        }
        println!(
            "   {:<14} non-serializable histories: {:>4}/2000   commits {commits:>5}  aborts {aborts:>5}   ({:?})",
            format!("{mode:?}"),
            nonser,
            start.elapsed()
        );
        match mode {
            Mode::Serializable => assert_eq!(nonser, 0, "SSI must never produce a cycle"),
            Mode::Snapshot => assert!(nonser > 0, "SI should admit write-skew style cycles"),
            Mode::ReadCommitted => assert!(nonser > 0),
        }
    }
}

fn main() {
    demo_versions_and_gc();
    anomaly_table();
    doctors_trace();
    random_histories();
    println!("all MVCC checks passed");
}
