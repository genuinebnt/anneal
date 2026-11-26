//! RECIPE 856 (database): transactions without a classic 2PC coordinator: PERCOLATOR (client-driven snapshot isolation) and CALVIN (deterministic ordering)
//! LEVEL: advanced
//! Use in: cross-row / cross-shard transactions on a store that only has single-row atomicity, or replicated databases that want no commit protocol.   Run: cargo run --release --example hybrid_logical_clock_snapshots_percolator
#![allow(dead_code)]
//!
//! `two_phase_commit` shows 2PC's weakness: the coordinator's decision lives in ITS log, and participants block while it is
//! down. Both designs here remove that blocking party in opposite ways.
//! PERCOLATOR (Google; the model behind TiDB / TiKV transactions). The store offers only atomic SINGLE-ROW read-modify-write.
//! Each key has three columns: DATA[start_ts] = value, LOCK = {start_ts, primary key, expiry}, WRITE[commit_ts] = pointer to
//! a start_ts. A TIMESTAMP ORACLE hands out increasing numbers. The CLIENT drives:
//!   1. start_ts from the oracle; read at that snapshot: the newest WRITE record with commit_ts < start_ts names the DATA version.
//!      A lock with lock.start_ts < start_ts may belong to a transaction that committed before we started: it must be RESOLVED.
//!   2. PREWRITE, one row at a time, the PRIMARY first: abort if there is a WRITE record at commit_ts >= start_ts (somebody
//!      committed after our snapshot: first committer wins) or any lock; else put DATA[start_ts] and the lock.
//!   3. commit_ts from the oracle; COMMIT THE PRIMARY: only if our lock is still there, write WRITE[commit_ts] and drop the lock.
//!      That one row transaction is THE commit point of the whole distributed transaction.
//!   4. Commit the secondaries (could be asynchronous: the outcome is already decided).
//! CRASH RECOVERY is lazy and needs no coordinator: whoever meets a stale lock looks at the PRIMARY. Still locked and expired:
//! roll the primary back first (that decides ABORT), then clean up. A WRITE record for this start_ts: committed, ROLL FORWARD.
//! Anything else: rolled back. A rollback leaves a ROLLBACK record on the primary (as TiKV does; the paper does not need it
//! with its assumptions) so the fate stays readable and a late prewrite of the primary cannot bring the transaction back.
//! The lock expiry (TTL) is only a LIVENESS guess: a client that is merely paused gets rolled back, then finds its primary lock
//! gone and must abort. Safety never depends on the guess being right.
//! SNAPSHOT ISOLATION is not serializability: two transactions that each read both rows and write DIFFERENT rows both commit
//! (write skew). It is demonstrated below.
//! CALVIN (Thomson et al.). Agree on the ORDER of transaction requests before executing them: a SEQUENCER cuts epochs and
//! logs each batch (replicated; that is the only agreement). Every replica runs a DETERMINISTIC LOCK MANAGER: lock requests are
//! queued strictly in sequence order, so a transaction runs once nothing earlier conflicts with it. Threads may then run ready
//! transactions in any order and every replica still reaches the same state, with no vote and no deadlock. A replica that
//! crashes replays the input log from a checkpoint. Price: read/write sets must be declared up front. For DEPENDENT
//! transactions ("pay whoever this index row points to") OLLP does a reconnaissance read, declares the predicted set, and
//! deterministically aborts and resubmits if the index changed before execution.
//! TIMESTAMPS: Percolator uses a central oracle. Replacing it with HYBRID LOGICAL CLOCKS (see `vector_clocks_hlc`) removes the
//! round trip but creates an UNCERTAINTY window between nodes' clocks that reads must wait out or retry (not modelled here).
//! VERIFICATION: 2000 random schedules per system. Percolator: interleaved transfer clients that may CRASH or PAUSE between any
//! two RPCs, plus read-only audits; a systematic sweep crashes one transfer after every step; a post-hoc oracle checks every
//! read against the committed history, checks first-committer-wins, conservation of money and that recovery leaves no lock
//! and no orphaned data. Three injected protocol bugs must be caught. Calvin: three replicas with different thread schedules
//! (one crashes and replays) must match each other and a serial execution in sequence order.
//! COMMON USES: TiDB/TiKV (Percolator-style, with additions), Google Percolator's incremental indexing; FaunaDB described a
//! Calvin-inspired design, and deterministic execution reappears in many replicated state machines and blockchains.
//! Read: https://research.google/pubs/large-scale-incremental-processing-using-distributed-transactions-and-notifications/
//! (Peng, Dabek, "Large-scale Incremental Processing Using Distributed Transactions and Notifications"),
//! https://cs.yale.edu/homes/thomson/publications/calvin-sigmod12.pdf (Thomson et al., "Calvin: Fast Distributed Transactions
//! for Partitioned Database Systems").
//!
//! TRY NEXT:
//!   - [S] Set TTL to 1 and then to 500: predict the abort rate and the number of rollbacks of live (paused) clients, then measure.
//!   - [S] Remove the Rollback record in `resolve` and find out which check (if any) fails. What assumption about message delays does that hide?
//!   - [M] Make secondaries commit asynchronously: queue them and let other readers roll forward. Does any invariant change?
//!   - [M] Add `select for update` (lock without write) to Percolator and show it prevents the write skew demo.
//!   - [A] PROJECT: run the Calvin sequencer through `raft_consensus` and the replicas under the `deterministic_simulation_testing` fault plan.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type Ts = u64;
type Key = u32;

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

// =============================================================================================== PERCOLATOR

const TTL: u64 = 40; // lock time-to-live in scheduler ticks
const INITIAL: i64 = 100;

#[derive(Clone, Debug)]
struct Lock {
    start: Ts,
    primary: Key,
    expires: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum WriteRec {
    Put { start: Ts },
    Rollback { start: Ts },
}
/// One row. Every method of `Store` that touches a single row models one atomic single-row transaction.
#[derive(Default, Clone)]
struct Row {
    data: BTreeMap<Ts, i64>,
    lock: Option<Lock>,
    write: BTreeMap<Ts, WriteRec>, // keyed by commit_ts (a rollback record is keyed by its start_ts)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Bug {
    None,
    /// Recovery rolls a stale secondary back from its TTL alone, without asking the primary.
    IgnorePrimary,
    /// Prewrite forgets the write-write conflict check.
    NoConflictCheck,
    /// Commit of the primary does not check that our lock is still there.
    BlindPrimaryCommit,
}

#[derive(Default, Clone, Debug)]
struct PStats {
    started: u64,
    committed: u64,
    audits: u64,
    conflict_write: u64,
    conflict_lock: u64,
    read_blocked: u64,
    aborted_rolled_back: u64,
    rolled_back: u64,
    rolled_forward: u64,
    waits: u64,
}

struct Store {
    rows: Vec<Row>,
    tso: Ts,
    now: u64,
    bug: Bug,
    st: PStats,
    /// A WRITE record that points at DATA which does not exist (only a broken protocol can produce one).
    dangling: Cell<Option<(Key, Ts)>>,
}

enum Resolved {
    Cleared,
    /// The owner may still be alive: wait.
    Alive,
}

impl Store {
    fn new(accounts: u32, bug: Bug) -> Store {
        let mut s = Store {
            rows: vec![Row::default(); accounts as usize],
            tso: 1,
            now: 0,
            bug,
            st: PStats::default(),
            dangling: Cell::new(None),
        };
        for r in &mut s.rows {
            r.data.insert(1, INITIAL);
            r.write.insert(2, WriteRec::Put { start: 1 });
        }
        s.tso = 2;
        s
    }
    fn timestamp(&mut self) -> Ts {
        self.tso += 1;
        self.tso
    }
    /// The newest committed value with commit_ts < ts.
    fn snapshot_value(&self, k: Key, ts: Ts) -> i64 {
        let row = &self.rows[k as usize];
        for (_, w) in row.write.range(..ts).rev() {
            if let WriteRec::Put { start } = w {
                return row.data.get(start).copied().unwrap_or_else(|| {
                    self.dangling.set(Some((k, *start))); // reported by the oracle
                    0
                });
            }
        }
        panic!("key {k} has no version before {ts}")
    }
    /// Snapshot read. `Err(lock)` means a lock that might hide an earlier commit is in the way.
    fn read(&self, k: Key, ts: Ts) -> Result<i64, Lock> {
        match &self.rows[k as usize].lock {
            Some(l) if l.start < ts => Err(l.clone()),
            _ => Ok(self.snapshot_value(k, ts)),
        }
    }
    fn prewrite(
        &mut self,
        k: Key,
        value: i64,
        start: Ts,
        primary: Key,
    ) -> Result<(), Option<Lock>> {
        let expires = self.now + TTL;
        let row = &mut self.rows[k as usize];
        if self.bug != Bug::NoConflictCheck && row.write.range(start..).next().is_some() {
            self.st.conflict_write += 1;
            return Err(None);
        }
        if let Some(l) = &row.lock {
            self.st.conflict_lock += 1;
            return Err(Some(l.clone()));
        }
        row.data.insert(start, value);
        row.lock = Some(Lock {
            start,
            primary,
            expires,
        });
        Ok(())
    }
    fn holds_lock(&self, k: Key, start: Ts) -> bool {
        self.rows[k as usize]
            .lock
            .as_ref()
            .is_some_and(|l| l.start == start)
    }
    /// Turn a lock into a WRITE record.
    fn commit_row(&mut self, k: Key, start: Ts, commit: Ts) {
        let row = &mut self.rows[k as usize];
        row.write.insert(commit, WriteRec::Put { start });
        if row.lock.as_ref().is_some_and(|l| l.start == start) {
            row.lock = None;
        }
    }
    /// THE commit point.
    fn commit_primary(&mut self, k: Key, start: Ts, commit: Ts) -> bool {
        if self.bug != Bug::BlindPrimaryCommit && !self.holds_lock(k, start) {
            return false; // somebody decided we were dead and rolled us back
        }
        self.commit_row(k, start, commit);
        true
    }
    fn commit_secondary(&mut self, k: Key, start: Ts, commit: Ts) {
        // if the lock is gone, a reader already rolled this row forward (the primary is committed, so it cannot be a rollback)
        if self.holds_lock(k, start) {
            self.commit_row(k, start, commit);
        }
    }
    fn rollback_row(&mut self, k: Key, start: Ts, leave_record: bool) {
        let row = &mut self.rows[k as usize];
        if row.lock.as_ref().is_some_and(|l| l.start == start) {
            row.lock = None;
            row.data.remove(&start);
        }
        if leave_record {
            row.write
                .entry(start)
                .or_insert(WriteRec::Rollback { start });
        }
    }
    /// Somebody found `lock` on key `k`. Decide its fate from the PRIMARY row, which is the single source of truth.
    fn resolve(&mut self, k: Key, lock: &Lock) -> Resolved {
        if !self.holds_lock(k, lock.start) {
            return Resolved::Cleared; // already handled
        }
        if self.bug == Bug::IgnorePrimary {
            if self.now < lock.expires {
                return Resolved::Alive;
            }
            self.st.rolled_back += 1;
            self.rollback_row(k, lock.start, k == lock.primary);
            return Resolved::Cleared;
        }
        let p = lock.primary;
        if let Some(pl) = self.rows[p as usize]
            .lock
            .clone()
            .filter(|pl| pl.start == lock.start)
        {
            if self.now < pl.expires {
                return Resolved::Alive;
            }
            self.st.rolled_back += 1;
            self.rollback_row(p, lock.start, true); // decide ABORT on the primary first ...
            self.rollback_row(k, lock.start, k == p); // ... then clean up
            return Resolved::Cleared;
        }
        let commit = self.rows[p as usize]
            .write
            .range(lock.start..)
            .find(|(_, w)| **w == WriteRec::Put { start: lock.start })
            .map(|(c, _)| *c);
        match commit {
            Some(c) => {
                self.st.rolled_forward += 1;
                self.commit_row(k, lock.start, c);
            }
            None => {
                self.st.rolled_back += 1;
                self.rollback_row(k, lock.start, false);
            }
        }
        Resolved::Cleared
    }
}

// ---- clients as state machines: each `step` is one RPC, so the scheduler can interleave, pause or kill between any two

#[derive(Clone, Debug)]
struct Txn {
    start: Ts,
    commit: Ts,
    reads: Vec<(Key, i64)>,
    writes: Vec<(Key, i64)>, // writes[0] is the primary
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    /// Move `amount` from keys[0] into the others (nothing if funds are short).
    Transfer { amount: i64 },
    /// Read every account.
    Audit,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum PState {
    Begin,
    Reading(usize),
    Prewriting(usize),
    GetCommitTs,
    CommitPrimary,
    CommitSecondary(usize),
    Finished(bool),
}
struct Client {
    kind: Kind,
    keys: Vec<Key>,
    state: PState,
    txn: Txn,
    waited: u32,
}
impl Client {
    fn new(kind: Kind, keys: Vec<Key>) -> Client {
        Client {
            kind,
            keys,
            state: PState::Begin,
            txn: Txn {
                start: 0,
                commit: 0,
                reads: vec![],
                writes: vec![],
            },
            waited: 0,
        }
    }
    /// Label of the boundary we are at, for crash accounting.
    fn boundary(&self) -> &'static str {
        match self.state {
            PState::Begin => "before start_ts",
            PState::Reading(0) => "after start_ts",
            PState::Reading(_) => "between reads",
            PState::Prewriting(0) => "after reads, nothing written",
            PState::Prewriting(1) => "after primary prewrite",
            PState::Prewriting(_) => "between secondary prewrites",
            PState::GetCommitTs => "all prewritten, no commit_ts",
            PState::CommitPrimary => "has commit_ts, primary not committed",
            PState::CommitSecondary(1) => "primary committed, no secondary",
            PState::CommitSecondary(_) => "between secondary commits",
            PState::Finished(_) => "finished",
        }
    }
    /// Clean abort by a LIVE client: remove our own locks, primary first.
    fn abort(&mut self, s: &mut Store) {
        for (i, &(k, _)) in self.txn.writes.iter().enumerate() {
            if s.holds_lock(k, self.txn.start) {
                s.rollback_row(k, self.txn.start, i == 0);
            }
        }
        self.state = PState::Finished(false);
    }
    /// Meet a foreign lock: resolve it, or wait a bounded number of times and then give up.
    fn blocked(&mut self, s: &mut Store, k: Key, lock: &Lock) -> bool {
        match s.resolve(k, lock) {
            Resolved::Cleared => false,
            Resolved::Alive => {
                self.waited += 1;
                s.st.waits += 1;
                self.waited > 12
            }
        }
    }
    fn step(&mut self, s: &mut Store) {
        match self.state {
            PState::Begin => {
                self.txn.start = s.timestamp();
                s.st.started += 1;
                self.state = PState::Reading(0);
            }
            PState::Reading(i) => {
                let k = self.keys[i];
                match s.read(k, self.txn.start) {
                    Ok(v) => {
                        self.txn.reads.push((k, v));
                        self.state = if i + 1 < self.keys.len() {
                            PState::Reading(i + 1)
                        } else {
                            self.after_reads()
                        };
                    }
                    Err(lock) => {
                        if self.blocked(s, k, &lock) {
                            s.st.read_blocked += 1;
                            self.abort(s);
                        }
                    }
                }
            }
            PState::Prewriting(i) => {
                let (k, v) = self.txn.writes[i];
                match s.prewrite(k, v, self.txn.start, self.txn.writes[0].0) {
                    Ok(()) => {
                        self.state = if i + 1 < self.txn.writes.len() {
                            PState::Prewriting(i + 1)
                        } else {
                            PState::GetCommitTs
                        }
                    }
                    Err(Some(lock)) => {
                        // clear it if it is stale so that a later transaction can succeed; we ourselves give up either way
                        s.resolve(k, &lock);
                        self.abort(s);
                    }
                    Err(None) => self.abort(s),
                }
            }
            PState::GetCommitTs => {
                self.txn.commit = s.timestamp();
                self.state = PState::CommitPrimary;
            }
            PState::CommitPrimary => {
                if s.commit_primary(self.txn.writes[0].0, self.txn.start, self.txn.commit) {
                    self.state = if self.txn.writes.len() > 1 {
                        PState::CommitSecondary(1)
                    } else {
                        PState::Finished(true)
                    };
                } else {
                    s.st.aborted_rolled_back += 1;
                    self.abort(s);
                }
            }
            PState::CommitSecondary(i) => {
                s.commit_secondary(self.txn.writes[i].0, self.txn.start, self.txn.commit);
                self.state = if i + 1 < self.txn.writes.len() {
                    PState::CommitSecondary(i + 1)
                } else {
                    PState::Finished(true)
                };
            }
            PState::Finished(_) => {}
        }
    }
    fn after_reads(&mut self) -> PState {
        let Kind::Transfer { amount } = self.kind else {
            return PState::Finished(true);
        };
        let others = self.txn.reads.len() as i64 - 1;
        if self.txn.reads[0].1 < amount * others {
            return PState::Finished(false); // insufficient funds: a business abort, nothing written
        }
        self.txn.writes = self
            .txn
            .reads
            .iter()
            .enumerate()
            .map(|(i, &(k, v))| {
                (
                    k,
                    if i == 0 {
                        v - amount * others
                    } else {
                        v + amount
                    },
                )
            })
            .collect();
        PState::Prewriting(0)
    }
}

fn pick_keys(rng: &mut Rng, accounts: u32, n: usize) -> Vec<Key> {
    let mut v: Vec<Key> = (0..accounts).collect();
    for i in 0..n {
        let j = i + rng.below((v.len() - i) as u64) as usize;
        v.swap(i, j);
    }
    v.truncate(n);
    v
}

struct PRun {
    store: Store,
    /// every client that ever got a start_ts, including crashed ones
    history: Vec<(Kind, Txn, Option<bool>)>,
    crashes: BTreeMap<&'static str, u64>,
    pauses: u64,
}

fn run_percolator(seed: u64, accounts: u32, bug: Bug) -> PRun {
    let mut rng = Rng::new(seed);
    let mut store = Store::new(accounts, bug);
    let mut crashes = BTreeMap::new();
    let mut history = vec![];
    let mut pauses = 0;
    let mut slots: Vec<Option<(Client, u64)>> = (0..5).map(|_| None).collect(); // (client, paused until)
    let mut to_start = 40;
    while to_start > 0 || slots.iter().any(Option::is_some) {
        store.now += 1;
        let i = rng.below(slots.len() as u64) as usize;
        let Some((c, until)) = &mut slots[i] else {
            if to_start > 0 {
                to_start -= 1;
                let c = if rng.pm(200) {
                    Client::new(
                        Kind::Audit,
                        pick_keys(&mut rng, accounts, accounts as usize),
                    )
                } else {
                    let n = 2 + rng.below(3.min(accounts as u64 - 1)) as usize;
                    Client::new(
                        Kind::Transfer {
                            amount: 1 + rng.below(40) as i64,
                        },
                        pick_keys(&mut rng, accounts, n),
                    )
                };
                slots[i] = Some((c, 0));
            }
            continue;
        };
        if store.now < *until {
            continue;
        }
        if c.state != PState::Begin && rng.pm(25) {
            *crashes.entry(c.boundary()).or_default() += 1; // the process is gone: no cleanup whatsoever
            let (c, _) = slots[i].take().unwrap();
            history.push((c.kind, c.txn, None));
            continue;
        }
        if rng.pm(8) {
            *until = store.now + TTL / 2 + rng.below(3 * TTL); // GC pause: may outlive our locks, and we will come back
            pauses += 1;
            continue;
        }
        c.step(&mut store);
        if let PState::Finished(ok) = c.state {
            let (c, _) = slots[i].take().unwrap();
            history.push((c.kind, c.txn, Some(ok)));
        }
    }
    PRun {
        store,
        history,
        crashes,
        pauses,
    }
}

/// Recovery by ordinary readers: after every TTL has passed, reading each key resolves whatever is left.
fn recover(s: &mut Store) {
    s.now += 10 * TTL;
    for k in 0..s.rows.len() as u32 {
        let ts = s.timestamp();
        while let Err(lock) = s.read(k, ts) {
            assert!(
                matches!(s.resolve(k, &lock), Resolved::Cleared),
                "an expired lock must always be resolvable"
            );
        }
    }
}

/// The post-hoc oracle. It decides "committed" from the primary row alone and re-derives everything else.
fn verify_percolator(run: &mut PRun) -> Result<(), String> {
    recover(&mut run.store);
    let s = &run.store;
    if let Some((k, start)) = s.dangling.get() {
        return Err(format!(
            "a reader followed a WRITE record of key {k} to data of start_ts {start}, which was rolled back"
        ));
    }
    for (k, row) in s.rows.iter().enumerate() {
        if let Some(l) = &row.lock {
            return Err(format!("stranded lock on {k} from ts {}", l.start));
        }
        for start in row.data.keys() {
            if !row
                .write
                .values()
                .any(|w| *w == WriteRec::Put { start: *start })
            {
                return Err(format!("orphaned data at key {k} start_ts {start}"));
            }
        }
    }
    // committed transactions according to their primaries
    let mut committed: Vec<(Ts, &Txn)> = vec![];
    for (_, t, reported) in &run.history {
        let Some(&(primary, _)) = t.writes.first() else {
            continue;
        };
        let commit = s.rows[primary as usize]
            .write
            .iter()
            .find(|(_, w)| **w == WriteRec::Put { start: t.start })
            .map(|(c, _)| *c);
        if reported.is_some_and(|ok| ok != commit.is_some()) {
            return Err(format!(
                "client of ts {} was told committed={reported:?} but the primary says {}",
                t.start,
                commit.is_some()
            ));
        }
        // ATOMICITY: all writes visible at the same commit_ts, or none at all
        for &(k, v) in &t.writes {
            let row = &s.rows[k as usize];
            let here = row
                .write
                .iter()
                .find(|(_, w)| **w == WriteRec::Put { start: t.start })
                .map(|(c, _)| *c);
            if here != commit || (commit.is_some() && row.data.get(&t.start) != Some(&v)) {
                return Err(format!(
                    "atomicity: txn {} has commit {commit:?} on its primary but {here:?} on key {k}",
                    t.start
                ));
            }
        }
        if let Some(c) = commit {
            committed.push((c, t));
        }
    }
    committed.sort_by_key(|(c, _)| *c);
    // SNAPSHOT READS: every read by every transaction (committed or not) equals the last commit before its start_ts
    for (kind, t, _) in &run.history {
        for &(k, got) in &t.reads {
            let want = committed
                .iter()
                .filter(|(c, _)| *c < t.start)
                .filter_map(|(_, w)| w.writes.iter().find(|(wk, _)| *wk == k))
                .next_back()
                .map_or(INITIAL, |&(_, v)| v);
            if got != want {
                return Err(format!(
                    "snapshot read: ts {} read {got} from key {k}, history says {want}",
                    t.start
                ));
            }
        }
        if *kind == Kind::Audit && t.reads.len() == s.rows.len() {
            let total: i64 = t.reads.iter().map(|r| r.1).sum();
            if total != INITIAL * s.rows.len() as i64 {
                return Err(format!("audit at ts {} saw total {total}", t.start));
            }
        }
    }
    // FIRST COMMITTER WINS: overlapping committed transactions never wrote the same key
    for (i, (ca, a)) in committed.iter().enumerate() {
        for (cb, b) in &committed[i + 1..] {
            let overlap = a.start < *cb && b.start < *ca;
            if overlap
                && a.writes
                    .iter()
                    .any(|(k, _)| b.writes.iter().any(|(k2, _)| k == k2))
            {
                return Err(format!(
                    "lost update: txns {} and {} overlap and wrote a common key",
                    a.start, b.start
                ));
            }
        }
    }
    let end = s.tso + 1;
    let total: i64 = (0..s.rows.len() as u32)
        .map(|k| s.snapshot_value(k, end))
        .sum();
    if total != INITIAL * s.rows.len() as i64 {
        return Err(format!("money not conserved: {total}"));
    }
    run.store.st.committed = committed.len() as u64;
    run.store.st.audits = run
        .history
        .iter()
        .filter(|(k, _, r)| *k == Kind::Audit && *r == Some(true))
        .count() as u64;
    Ok(())
}

/// Crash one 3-key transfer after exactly `steps` RPCs and recover: committed iff the primary commit was among them.
fn percolator_crash_sweep() {
    // start_ts, 3 reads, 3 prewrites, commit_ts, primary commit, 2 secondary commits = 11 steps
    const COMMIT_POINT: usize = 9;
    let mut seen = vec![];
    for steps in 0..=11 {
        let mut s = Store::new(3, Bug::None);
        let mut c = Client::new(Kind::Transfer { amount: 10 }, vec![0, 1, 2]);
        for _ in 0..steps {
            c.step(&mut s);
        }
        let at = c.boundary();
        // before the TTL expires another reader must WAIT if there is an undecided lock
        let reader_ts = s.timestamp();
        let blocked = matches!(s.read(1, reader_ts), Err(ref l) if matches!(s.resolve(1, &l.clone()), Resolved::Alive));
        assert_eq!(
            blocked,
            (6..COMMIT_POINT).contains(&steps),
            "step {steps}: key 1 is locked and undecided only between its prewrite and the commit point"
        );
        recover(&mut s);
        let end = s.timestamp();
        let values: Vec<i64> = (0..3).map(|k| s.snapshot_value(k, end)).collect();
        let committed = steps >= COMMIT_POINT;
        assert_eq!(
            values,
            if committed {
                vec![80, 110, 110]
            } else {
                vec![100, 100, 100]
            },
            "crash after {steps} steps ({at})"
        );
        assert!(s.rows.iter().all(|r| r.lock.is_none()));
        if !committed && c.state != PState::Begin {
            // the "crashed" client was only slow: it wakes up and runs on. It must not commit part of a rolled-back transaction.
            for _ in 0..20 {
                c.step(&mut s);
            }
            if steps >= 5 {
                assert_eq!(
                    c.state,
                    PState::Finished(false),
                    "a client that had prewritten its primary and was rolled back must abort"
                );
                let end = s.timestamp();
                assert_eq!(
                    (0..3).map(|k| s.snapshot_value(k, end)).collect::<Vec<_>>(),
                    vec![100, 100, 100],
                    "step {steps}: the woken client changed data"
                );
                assert!(
                    s.rows.iter().all(|r| r.lock.is_none() && r.data.len() == 1),
                    "step {steps}: the woken client left a lock or data behind"
                );
            }
        }
        seen.push(format!(
            "{steps}:{}",
            if committed { "forward" } else { "back" }
        ));
    }
    println!(
        "percolator crash sweep (steps executed before the crash -> recovery outcome): {}",
        seen.join(" ")
    );
}

/// Two on-call doctors each see two doctors on call and each signs off a DIFFERENT row. SI lets both commit.
fn write_skew_demo() {
    let mut s = Store::new(2, Bug::None);
    let (t1, t2) = (s.timestamp(), s.timestamp());
    let sees = |s: &Store, ts| s.read(0, ts).unwrap() + s.read(1, ts).unwrap();
    assert!(sees(&s, t1) >= 200 && sees(&s, t2) >= 200); // both: "the invariant sum >= 100 survives if I zero one row"
    s.prewrite(0, 0, t1, 0).unwrap();
    s.prewrite(1, 0, t2, 1).unwrap(); // disjoint write sets: no write-write conflict
    let (c1, c2) = (s.timestamp(), s.timestamp());
    assert!(s.commit_primary(0, t1, c1) && s.commit_primary(1, t2, c2));
    let end = s.timestamp();
    assert_eq!(s.snapshot_value(0, end) + s.snapshot_value(1, end), 0);
    println!(
        "write skew: both transactions commit under snapshot isolation and the invariant 'sum >= 100' is broken (sum = 0)"
    );
}

fn percolator_suite() {
    percolator_crash_sweep();
    write_skew_demo();
    let mut crash_total: BTreeMap<&'static str, u64> = BTreeMap::new();
    println!("\npercolator: 2000 random schedules (5 concurrent clients, 40 transactions each)");
    println!(
        "  accounts  schedules  started  committed(writes)  audits  ww-conflict  lock-conflict  gave-up-reading  found-own-rollback  abort%  recovery fwd/back"
    );
    for (accounts, seeds) in [(4u32, 1..=700u64), (12, 701..=1400), (64, 1401..=2000)] {
        let mut t = PStats::default();
        let n = seeds.clone().count();
        let (mut crashed, mut pauses) = (0, 0);
        for seed in seeds {
            let mut r = run_percolator(seed, accounts, Bug::None);
            verify_percolator(&mut r).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            for (k, v) in &r.crashes {
                *crash_total.entry(k).or_default() += v;
                crashed += v;
            }
            pauses += r.pauses;
            let s = &r.store.st;
            for (dst, src) in [
                (&mut t.started, s.started),
                (&mut t.committed, s.committed),
                (&mut t.audits, s.audits),
                (&mut t.conflict_write, s.conflict_write),
                (&mut t.conflict_lock, s.conflict_lock),
                (&mut t.read_blocked, s.read_blocked),
                (&mut t.aborted_rolled_back, s.aborted_rolled_back),
                (&mut t.rolled_back, s.rolled_back),
                (&mut t.rolled_forward, s.rolled_forward),
            ] {
                *dst += src;
            }
        }
        let protocol_aborts =
            t.conflict_write + t.conflict_lock + t.read_blocked + t.aborted_rolled_back;
        let abort_rate = 100.0 * protocol_aborts as f64 / t.started as f64;
        println!(
            "  {accounts:>8}  {n:>9}  {:>7}  {:>17}  {:>6}  {:>11}  {:>13}  {:>15}  {:>23}  {abort_rate:>5.1}%  {}/{}",
            t.started,
            t.committed,
            t.audits,
            t.conflict_write,
            t.conflict_lock,
            t.read_blocked,
            t.aborted_rolled_back,
            t.rolled_forward,
            t.rolled_back
        );
        println!("            ({crashed} client crashes, {pauses} pauses)");
        assert!(
            t.committed > 0
                && t.audits > 0
                && t.rolled_back > 0
                && t.rolled_forward > 0
                && t.aborted_rolled_back > 0
        );
    }
    println!("  crashes by protocol boundary: {crash_total:?}");
    for b in [
        "after start_ts",
        "between reads",
        "after reads, nothing written",
        "after primary prewrite",
        "between secondary prewrites",
        "all prewritten, no commit_ts",
        "has commit_ts, primary not committed",
        "primary committed, no secondary",
        "between secondary commits",
    ] {
        assert!(
            crash_total.get(b).copied().unwrap_or(0) > 0,
            "no crash ever happened at boundary '{b}'"
        );
    }
    for bug in [
        Bug::IgnorePrimary,
        Bug::NoConflictCheck,
        Bug::BlindPrimaryCommit,
    ] {
        let (mut caught, mut first) = (0, None);
        for seed in 1..=500 {
            if let Err(e) = verify_percolator(&mut run_percolator(seed, 4, bug)) {
                caught += 1;
                first.get_or_insert((seed, e));
            }
        }
        let (seed, why) = first.unwrap_or_else(|| panic!("{bug:?} was never caught"));
        println!("  injected {bug:?}: caught in {caught}/500 schedules; first seed {seed}: {why}");
    }
}

// =============================================================================================== CALVIN

#[derive(Clone, Debug, PartialEq)]
enum Request {
    Transfer {
        from: Key,
        to: Key,
        amount: i64,
    },
    /// Dependent transaction: pay whatever account index row `index` points to. `predicted` is the OLLP reconnaissance result.
    PayViaIndex {
        from: Key,
        index: Key,
        amount: i64,
        predicted: Key,
    },
    /// Change the index row.
    Repoint {
        index: Key,
        to: Key,
    },
    Sum {
        keys: Vec<Key>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    Done(i64),
    InsufficientFunds,
    /// OLLP: the prediction was stale. Every replica decides this identically.
    Restart,
}
const ACCOUNTS: u32 = 16;
const INDEXES: u32 = 3;
const INDEX_BASE: Key = 1000;

type Db = BTreeMap<Key, i64>;
fn initial_db() -> Db {
    (0..ACCOUNTS)
        .map(|k| (k, INITIAL))
        .chain((0..INDEXES).map(|i| (INDEX_BASE + i, i as i64)))
        .collect()
}
/// The declared lock set: (key, exclusive).
fn lock_set(r: &Request) -> BTreeMap<Key, bool> {
    let mut m = BTreeMap::new();
    let mut add = |k: Key, x: bool| *m.entry(k).or_insert(false) |= x;
    match r {
        Request::Transfer { from, to, .. } => {
            add(*from, true);
            add(*to, true);
        }
        Request::PayViaIndex {
            from,
            index,
            predicted,
            ..
        } => {
            add(*from, true);
            add(*predicted, true);
            add(INDEX_BASE + index, false);
        }
        Request::Repoint { index, .. } => add(INDEX_BASE + index, true),
        Request::Sum { keys } => keys.iter().for_each(|&k| add(k, false)),
    }
    m
}
/// Stored procedure: a pure function of the database values under its locks. It touches ONLY declared keys.
fn execute(db: &mut Db, r: &Request) -> Outcome {
    let locks = lock_set(r);
    let transfer = |db: &mut Db, from: Key, to: Key, amount: i64| {
        assert!(
            locks.get(&from) == Some(&true) && locks.get(&to) == Some(&true),
            "write without an exclusive lock"
        );
        if db[&from] < amount {
            return Outcome::InsufficientFunds;
        }
        *db.get_mut(&from).unwrap() -= amount;
        *db.get_mut(&to).unwrap() += amount;
        Outcome::Done(amount)
    };
    match r {
        Request::Transfer { from, to, amount } => transfer(db, *from, *to, *amount),
        Request::PayViaIndex {
            from,
            index,
            amount,
            predicted,
        } => {
            if db[&(INDEX_BASE + index)] != *predicted as i64 {
                return Outcome::Restart; // we hold the wrong account's lock: touch nothing
            }
            transfer(db, *from, *predicted, *amount)
        }
        Request::Repoint { index, to } => {
            db.insert(INDEX_BASE + index, *to as i64);
            Outcome::Done(0)
        }
        Request::Sum { keys } => Outcome::Done(keys.iter().map(|k| db[k]).sum()),
    }
}

/// Per key: FIFO of (sequence number, exclusive). A transaction holds a lock when no conflicting request is ahead of it.
#[derive(Default)]
struct LockManager {
    queues: BTreeMap<Key, VecDeque<(usize, bool)>>,
    waiting: BTreeMap<usize, usize>, // seq -> number of locks not yet granted
    granted: BTreeSet<(Key, usize)>,
    ready: Vec<usize>,
    had_to_wait: u64,
}
impl LockManager {
    /// MUST be called in sequence order, by one thread: this is the whole source of determinism and of deadlock freedom.
    fn request(&mut self, seq: usize, locks: &BTreeMap<Key, bool>) {
        self.waiting.insert(seq, locks.len());
        for (&k, &x) in locks {
            self.queues.entry(k).or_default().push_back((seq, x));
            self.grant(k);
        }
        if self.waiting.contains_key(&seq) {
            self.had_to_wait += 1;
        }
    }
    /// Grant the head of the queue; if it is shared, every consecutive shared request behind it too.
    fn grant(&mut self, k: Key) {
        let Some(q) = self.queues.get(&k) else { return };
        let mut now = vec![];
        for (i, &(seq, x)) in q.iter().enumerate() {
            if x && i > 0 {
                break;
            }
            now.push(seq);
            if x {
                break;
            }
        }
        for seq in now {
            if self.granted.insert((k, seq)) {
                let n = self.waiting.get_mut(&seq).unwrap();
                *n -= 1;
                if *n == 0 {
                    self.waiting.remove(&seq);
                    self.ready.push(seq);
                }
            }
        }
    }
    fn release(&mut self, seq: usize, locks: &BTreeMap<Key, bool>) {
        for &k in locks.keys() {
            let q = self.queues.get_mut(&k).unwrap();
            let pos = q.iter().position(|&(s, _)| s == seq).unwrap();
            q.remove(pos);
            self.granted.remove(&(k, seq));
            self.grant(k);
        }
    }
}

struct Executed {
    db: Db,
    outcomes: Vec<Outcome>,
    waited: u64,
    max_parallel: usize,
    completion_order: Vec<usize>,
}
/// Execute one batch on a replica. `rng` models the OS thread scheduler: any ready transaction may finish next.
/// `shuffle_requests` is the injected bug: lock requests are issued in a scheduler-dependent order.
fn run_batch(
    db: &mut Db,
    batch: &[Request],
    base: usize,
    rng: &mut Rng,
    shuffle_requests: bool,
    out: &mut Executed,
) {
    let mut lm = LockManager::default();
    let sets: Vec<BTreeMap<Key, bool>> = batch.iter().map(lock_set).collect();
    let mut order: Vec<usize> = (0..batch.len()).collect();
    if shuffle_requests {
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i as u64 + 1) as usize);
        }
    }
    let (mut next, mut done) = (0, 0);
    let mut results = vec![None; batch.len()];
    while done < batch.len() {
        // interleave lock acquisition (the single lock-manager thread) with execution (the workers)
        if next < batch.len() && (lm.ready.is_empty() || rng.pm(500)) {
            let i = order[next];
            lm.request(i, &sets[i]);
            next += 1;
            continue;
        }
        assert!(
            !lm.ready.is_empty(),
            "deadlock: impossible when requests are queued in one global order"
        );
        out.max_parallel = out.max_parallel.max(lm.ready.len());
        // everything simultaneously ready must be pairwise compatible
        for (a, &x) in lm.ready.iter().enumerate() {
            for &y in &lm.ready[a + 1..] {
                assert!(
                    sets[x]
                        .iter()
                        .all(|(k, &wx)| sets[y].get(k).is_none_or(|&wy| !wx && !wy)),
                    "conflicting transactions granted together"
                );
            }
        }
        let pick = rng.below(lm.ready.len() as u64) as usize;
        let i = lm.ready.swap_remove(pick);
        results[i] = Some(execute(db, &batch[i]));
        out.completion_order.push(base + i);
        lm.release(i, &sets[i]);
        done += 1;
    }
    out.outcomes.extend(results.into_iter().map(Option::unwrap));
    out.waited += lm.had_to_wait;
}

/// The sequencer: turns client requests into logged epochs. Restarted OLLP transactions re-enter a later epoch with a fresh
/// reconnaissance read of the latest state the sequencer knows (it runs one reference replica serially for that).
struct Log {
    epochs: Vec<Vec<Request>>,
    serial_outcomes: Vec<Outcome>,
    serial_db: Db,
    restarts: u64,
}
fn sequence(seed: u64) -> Log {
    let mut rng = Rng::new(seed);
    let mut db = initial_db(); // serial execution in sequence order: THE oracle
    let mut log = Log {
        epochs: vec![],
        serial_outcomes: vec![],
        serial_db: Db::new(),
        restarts: 0,
    };
    let mut retry: Vec<Request> = vec![];
    let mut epoch = 0;
    while epoch < 12 || !retry.is_empty() {
        // reconnaissance happens BEFORE the epoch is ordered, so it can be stale by execution time
        let recon = db.clone();
        let mut batch = vec![];
        for r in retry.drain(..) {
            let Request::PayViaIndex {
                from,
                index,
                amount,
                ..
            } = r
            else {
                unreachable!()
            };
            batch.push(Request::PayViaIndex {
                from,
                index,
                amount,
                predicted: recon[&(INDEX_BASE + index)] as Key,
            });
        }
        if epoch < 12 {
            for _ in 0..1 + rng.below(14) {
                let from = rng.below(ACCOUNTS as u64) as Key;
                let amount = 1 + rng.below(80) as i64;
                let index = rng.below(INDEXES as u64) as Key;
                batch.push(match rng.below(10) {
                    0..=4 => Request::Transfer {
                        from,
                        to: (from + 1 + rng.below(ACCOUNTS as u64 - 1) as Key) % ACCOUNTS,
                        amount,
                    },
                    5 | 6 => Request::PayViaIndex {
                        from,
                        index,
                        amount,
                        predicted: recon[&(INDEX_BASE + index)] as Key,
                    },
                    7 => Request::Repoint {
                        index,
                        to: rng.below(ACCOUNTS as u64) as Key,
                    },
                    _ => Request::Sum {
                        keys: (0..ACCOUNTS).collect(),
                    },
                });
            }
        }
        for r in &batch {
            let o = execute(&mut db, r);
            if o == Outcome::Restart {
                log.restarts += 1;
                retry.push(r.clone());
            }
            if matches!(r, Request::Sum { .. }) {
                assert_eq!(o, Outcome::Done(INITIAL * ACCOUNTS as i64));
            }
            log.serial_outcomes.push(o);
        }
        log.epochs.push(batch);
        epoch += 1;
        assert!(epoch < 200, "OLLP restarts did not converge");
    }
    log.serial_db = db;
    log
}

/// Run the whole log. With `crash_in`, the replica dies part-way through that epoch and replays from the last checkpoint.
fn replica(log: &Log, seed: u64, crash_in: Option<usize>, shuffle_requests: bool) -> Executed {
    let mut rng = Rng::new(seed);
    let mut out = Executed {
        db: initial_db(),
        outcomes: vec![],
        waited: 0,
        max_parallel: 0,
        completion_order: vec![],
    };
    let mut checkpoint = (0usize, out.db.clone(), 0usize); // (epoch, state, outcomes so far)
    let mut crash_in = crash_in;
    let mut e = 0;
    while e < log.epochs.len() {
        if e % 4 == 0 {
            checkpoint = (e, out.db.clone(), out.outcomes.len());
        }
        let batch = &log.epochs[e];
        let mut db = std::mem::take(&mut out.db);
        if crash_in == Some(e) {
            // half-executed epoch, then the process dies: its memory is discarded
            let cut = rng.below(batch.len() as u64) as usize;
            for r in &batch[..cut] {
                execute(&mut db, r);
            }
            crash_in = None;
            e = checkpoint.0;
            out.db = checkpoint.1.clone();
            out.outcomes.truncate(checkpoint.2);
            continue;
        }
        let base = out.outcomes.len();
        run_batch(&mut db, batch, base, &mut rng, shuffle_requests, &mut out);
        out.db = db;
        e += 1;
    }
    out
}

fn calvin_suite() {
    let (
        mut txns,
        mut waited,
        mut restarts,
        mut reordered,
        mut max_parallel,
        mut diverged_with_bug,
        mut dependent,
    ) = (0u64, 0u64, 0u64, 0u64, 0usize, 0u64, 0u64);
    for seed in 1..=2000u64 {
        let log = sequence(seed);
        let crash = Rng::new(seed ^ 77).below(log.epochs.len() as u64) as usize;
        let replicas = [
            replica(&log, seed * 3 + 1, None, false),
            replica(&log, seed * 3 + 2, None, false),
            replica(&log, seed * 3 + 3, Some(crash), false),
        ];
        for (i, r) in replicas.iter().enumerate() {
            assert_eq!(
                r.db, log.serial_db,
                "seed {seed}: replica {i} state differs from serial execution in sequence order"
            );
            assert_eq!(
                r.outcomes, log.serial_outcomes,
                "seed {seed}: replica {i} made different commit/abort decisions"
            );
        }
        let n: usize = log.epochs.iter().map(Vec::len).sum();
        txns += n as u64;
        waited += replicas[0].waited;
        restarts += log.restarts;
        dependent += log
            .epochs
            .iter()
            .flatten()
            .filter(|r| matches!(r, Request::PayViaIndex { .. }))
            .count() as u64;
        reordered += (replicas[0].completion_order != replicas[1].completion_order) as u64;
        max_parallel = max_parallel.max(replicas[0].max_parallel);
        // the bug: lock requests issued in scheduler order instead of sequence order
        let (a, b) = (
            replica(&log, seed * 3 + 1, None, true),
            replica(&log, seed * 3 + 2, None, true),
        );
        diverged_with_bug += (a.db != b.db || a.outcomes != b.outcomes) as u64;
    }
    println!(
        "\ncalvin: 2000 random logs, 3 replicas each (one crashes and replays from a checkpoint): all equal serial execution"
    );
    println!(
        "  {txns} sequenced transactions; lock-conflict rate (had to wait) {:.1}%; OLLP restarts {restarts} of {dependent} dependent attempts ({:.1}%); concurrency aborts: 0",
        100.0 * waited as f64 / txns as f64,
        100.0 * restarts as f64 / dependent as f64
    );
    println!(
        "  replicas completed transactions in a different order in {reordered} logs; up to {max_parallel} transactions were runnable at once"
    );
    println!(
        "  injected bug (lock requests in thread-arrival order): replicas diverged in {diverged_with_bug} of 2000 logs"
    );
    assert!(
        waited > 0 && restarts > 0 && reordered > 1000 && max_parallel > 1 && diverged_with_bug > 0
    );
}

fn main() {
    percolator_suite();
    calvin_suite();
}
