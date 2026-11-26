//! RECIPE 743 (distributed): Spanner external consistency: TrueTime intervals, commit wait, 2PL + 2PC over replicated groups, snapshot reads, safe time.
//! LEVEL: advanced
//! Use in: globally distributed SQL databases, any system that wants real-time-ordered transactions without a central timestamp oracle.   Run: cargo run --release --example spanner_truetime_commit_wait
//! COMMON USES: Google Spanner (TrueTime from GPS and atomic clocks), CockroachDB (hybrid logical clocks with an uncertainty interval, optional commit wait),
//! YugabyteDB, FaunaDB/Calvin-style comparisons, Amazon Time Sync / clock-bound libraries, and the "wait out the uncertainty" trick in any clock-based ordering.
//! Course: CMU 15-721 Advanced Database Systems (Spring), lecture on distributed OLTP / Spanner (global clocks, commit wait).
#![allow(dead_code)]
//!
//! WHAT: a simulation of Spanner's transaction layer. Each shard is a PAXOS-GROUP-LIKE replicated state machine: a log of entries
//! (Write, Prepare, Commit, Heartbeat) applied by 3 replicas into an MVCC store (key -> versions sorted by timestamp). Read-write
//! transactions take exclusive locks (2PL, wait-die to avoid deadlock), and commit with TWO-PHASE COMMIT across the groups touched,
//! the coordinator group being just another group. Read-only transactions take no locks: they pick a timestamp and read versions.
//!
//! TRUETIME: `TT.now()` returns an INTERVAL [earliest, latest] guaranteed to contain absolute time, `TT.after(t)` = earliest > t.
//! The simulated clock of every node drifts (a triangle wave bounded by the true error E) and the advertised epsilon must be >= E.
//! COMMIT TIMESTAMP rules: a coordinator picks s >= TT.now().latest at commit request, s >= every participant's prepare timestamp,
//! and s > every timestamp the leader assigned before (monotonic). COMMIT WAIT: do not acknowledge the client or release locks until
//! TT.after(s). Then s is in the past for every clock in the world, so any transaction that starts after the acknowledgement picks a
//! timestamp > s. Latency cost is about 2*epsilon, which is why Google pushed epsilon down to a few milliseconds.
//!
//! SAFE TIME: a replica may serve a read at timestamp t only if t <= t_safe = min(t_paxos_safe, min prepared timestamp - 1):
//! it has applied every write with ts <= t (Paxos part), and no prepared-but-undecided transaction could still commit at or below t
//! (transaction-manager part). Idle groups advance t_safe with heartbeat entries. That makes lock-free reads at ANY replica correct.
//!
//! LINEARIZABILITY vs STRICT SERIALIZABILITY: linearizability is a single-object real-time guarantee (each operation takes effect
//! between invocation and response). Serializability orders whole multi-object transactions as if serial but allows any order,
//! even one contradicting real time. STRICT SERIALIZABILITY (Spanner calls it EXTERNAL CONSISTENCY) is both: the serial order is
//! consistent with real time, i.e. if T1 commits before T2 starts then ts(T1) < ts(T2). Snapshot isolation or plain serializability
//! can show stale reads to a client that heard about a commit out of band; strict serializability cannot.
//!
//! CHECKS: over 2000 random schedules (random clock phases, 1 or 2 groups per transaction, chains of transactions each starting right
//! after the previous acknowledgement) external consistency holds with epsilon >= E and commit wait, and is VIOLATED (counted) with no
//! commit wait or with epsilon below the true error; concurrent 2PL+2PC transfers equal a serial replay in commit-timestamp order and
//! conserve money; follower reads that are not blocked equal the leader's MVCC answer (2000 random reads against lagging replicas);
//! t_safe never decreases; commit-wait latency grows with epsilon.
//!
//! Read: Spanner: Google's Globally-Distributed Database (OSDI 2012) https://www.usenix.org/system/files/conference/osdi12/osdi12-final-16.pdf
//!       Spanner paper page https://research.google/pubs/spanner-googles-globally-distributed-database-2/
//!       CMU 15-721 course schedule https://15721.courses.cs.cmu.edu/spring2024/

use std::collections::{BTreeMap, BTreeSet, HashMap};

type Key = u64;
type Ts = i64;

const TRUE_ERR: i64 = 10; // real maximum clock error E of every node (microsecond-ish units)
const LAT: i64 = 3; // one replication round trip

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

// ---------------------------------------------------------------- clocks and TrueTime

/// A drifting local clock: error follows a triangle wave in [-E, E] (drifts for a while, then is corrected by the time master).
#[derive(Clone, Copy)]
struct Clock {
    phase: i64,
}

impl Clock {
    fn skew(&self, real: i64) -> i64 {
        let m = ((real + self.phase) / 8).rem_euclid(4 * TRUE_ERR);
        if m < 2 * TRUE_ERR {
            m - TRUE_ERR
        } else {
            3 * TRUE_ERR - m
        }
    }
}

/// TT.now() as seen by a node whose clock is `clock`, with advertised uncertainty `eps`.
fn tt_now(clock: Clock, eps: i64, real: i64) -> (Ts, Ts) {
    let local = real + clock.skew(real);
    (local - eps, local + eps)
}

fn tt_after(clock: Clock, eps: i64, ts: Ts, real: i64) -> bool {
    tt_now(clock, eps, real).0 > ts
}

// ---------------------------------------------------------------- replicated group (state machine)

#[derive(Clone, Debug)]
enum Entry {
    Write {
        ts: Ts,
        writes: Vec<(Key, i64)>,
    },
    Prepare {
        txn: u64,
        ts: Ts,
        writes: Vec<(Key, i64)>,
    },
    Commit {
        txn: u64,
        ts: Ts,
    },
    Heartbeat {
        ts: Ts,
    },
}

impl Entry {
    fn ts(&self) -> Ts {
        match self {
            Entry::Write { ts, .. }
            | Entry::Prepare { ts, .. }
            | Entry::Commit { ts, .. }
            | Entry::Heartbeat { ts } => *ts,
        }
    }
}

#[derive(Default, Clone)]
struct Replica {
    applied: usize,
    store: BTreeMap<Key, Vec<(Ts, i64)>>, // MVCC versions sorted by timestamp
    paxos_safe: Ts,
    prepared: BTreeMap<u64, (Ts, Vec<(Key, i64)>)>,
}

impl Replica {
    fn install(&mut self, writes: &[(Key, i64)], ts: Ts) {
        for &(k, v) in writes {
            let vs = self.store.entry(k).or_default();
            let pos = vs.partition_point(|x| x.0 <= ts);
            vs.insert(pos, (ts, v));
        }
    }
    fn apply(&mut self, e: &Entry) {
        match e {
            Entry::Write { ts, writes } => self.install(writes, *ts),
            Entry::Prepare { txn, ts, writes } => {
                self.prepared.insert(*txn, (*ts, writes.clone()));
            }
            Entry::Commit { txn, ts } => {
                if let Some((_, w)) = self.prepared.remove(txn) {
                    self.install(&w, *ts);
                }
            }
            Entry::Heartbeat { .. } => {}
        }
        self.paxos_safe = self.paxos_safe.max(e.ts());
        self.applied += 1;
    }
    /// t_safe = min(applied-prefix timestamp, earliest prepared timestamp - 1): no future commit can land at or below it.
    fn t_safe(&self) -> Ts {
        let tm = self
            .prepared
            .values()
            .map(|(p, _)| p - 1)
            .min()
            .unwrap_or(Ts::MAX);
        self.paxos_safe.min(tm)
    }
    fn read(&self, k: Key, ts: Ts) -> Option<i64> {
        self.store
            .get(&k)
            .and_then(|vs| vs.iter().rev().find(|(t, _)| *t <= ts).map(|x| x.1))
    }
}

enum ReadResult {
    Value(Option<i64>),
    Blocked { t_safe: Ts },
}

struct Group {
    log: Vec<Entry>,
    replicas: Vec<Replica>, // replicas[0] is the leader, applies synchronously
    last_ts: Ts,            // largest timestamp this leader ever assigned
}

impl Group {
    fn new() -> Group {
        Group {
            log: vec![],
            replicas: vec![Replica::default(); 3],
            last_ts: 0,
        }
    }
    fn append(&mut self, e: Entry) {
        self.last_ts = self.last_ts.max(e.ts());
        self.replicas[0].apply(&e);
        self.log.push(e);
    }
    /// Timestamps handed out by one leader are strictly increasing and never below the caller's clock reading.
    fn next_ts(&self, latest: Ts) -> Ts {
        latest.max(self.last_ts + 1)
    }
    fn catch_up(&mut self, r: usize, n: usize) {
        while self.replicas[r].applied < self.log.len() && n > 0 {
            let i = self.replicas[r].applied;
            let e = self.log[i].clone();
            self.replicas[r].apply(&e);
            if self.replicas[r].applied - i >= n {
                break;
            }
        }
    }
    fn read_at(&self, r: usize, k: Key, ts: Ts) -> ReadResult {
        let rep = &self.replicas[r];
        if rep.t_safe() < ts {
            ReadResult::Blocked {
                t_safe: rep.t_safe(),
            }
        } else {
            ReadResult::Value(rep.read(k, ts))
        }
    }
    /// A leader that must serve a read at `ts` promises "no future write at or below ts" by logging a heartbeat.
    fn ensure_safe(&mut self, ts: Ts) {
        if ts > self.last_ts {
            self.append(Entry::Heartbeat { ts });
        }
    }
}

// ---------------------------------------------------------------- cluster: 2PC over groups, commit wait

struct Cluster {
    groups: Vec<Group>,
    clocks: Vec<Clock>, // the leader of each group has its own clock
    eps: i64,
    commit_wait: bool,
}

struct Committed {
    ts: Ts,
    ack: i64,
    wait_extra: i64, // latency added by commit wait
}

impl Cluster {
    fn new(n: usize, eps: i64, commit_wait: bool, rng: &mut Rng) -> Cluster {
        Cluster {
            groups: (0..n).map(|_| Group::new()).collect(),
            clocks: (0..n)
                .map(|_| Clock {
                    phase: rng.below(4 * 8 * TRUE_ERR as u64) as i64,
                })
                .collect(),
            eps,
            commit_wait,
        }
    }
    fn group_of(&self, k: Key) -> usize {
        (k % self.groups.len() as u64) as usize
    }
    fn latest(&self, g: usize, real: i64) -> Ts {
        tt_now(self.clocks[g], self.eps, real).1
    }

    /// Read-write transaction commit (locks assumed held by the caller). Returns the commit timestamp and the client ack time.
    fn commit_rw(&mut self, txn: u64, writes: &[(Key, i64)], real: i64) -> Committed {
        let mut by_group: BTreeMap<usize, Vec<(Key, i64)>> = BTreeMap::new();
        for &(k, v) in writes {
            by_group.entry(self.group_of(k)).or_default().push((k, v));
        }
        let coord = *by_group.keys().next().unwrap();
        let (ts, t_commit);
        if by_group.len() == 1 {
            // Single-group fast path: one Paxos write, no 2PC.
            let w = self.groups[coord].next_ts(self.latest(coord, real));
            self.groups[coord].append(Entry::Write {
                ts: w,
                writes: writes.to_vec(),
            });
            ts = w;
            t_commit = real;
        } else {
            // Phase 1: every participant picks a prepare timestamp and logs Prepare (this pins its t_safe below p).
            let mut prep_max = 0;
            for (&g, w) in &by_group {
                let p = self.groups[g].next_ts(self.latest(g, real));
                self.groups[g].append(Entry::Prepare {
                    txn,
                    ts: p,
                    writes: w.clone(),
                });
                prep_max = prep_max.max(p);
            }
            // Phase 2: coordinator picks s >= all prepares, >= TT.now().latest, > its own previous timestamps.
            let t_votes = real + LAT;
            let s = self.groups[coord].next_ts(self.latest(coord, t_votes).max(prep_max));
            for &g in by_group.keys() {
                self.groups[g].append(Entry::Commit { txn, ts: s });
            }
            ts = s;
            t_commit = t_votes;
        }
        // COMMIT WAIT: hold the acknowledgement (and the locks) until TT.after(ts) on the coordinator's clock.
        let replicated = t_commit + LAT;
        let mut ack = replicated;
        if self.commit_wait {
            while !tt_after(self.clocks[coord], self.eps, ts, ack) {
                ack += 1;
            }
        }
        Committed {
            ts,
            ack,
            wait_extra: ack - replicated,
        }
    }

    /// Strong read-only transaction: no locks, one timestamp, versions read at each group's leader.
    fn read_only(
        &mut self,
        keys: &[Key],
        real: i64,
        client_group: usize,
    ) -> (Ts, Vec<Option<i64>>) {
        let ts = self.latest(client_group, real);
        let vals = keys
            .iter()
            .map(|&k| {
                let g = self.group_of(k);
                self.groups[g].ensure_safe(ts);
                match self.groups[g].read_at(0, k, ts) {
                    ReadResult::Value(v) => v,
                    ReadResult::Blocked { .. } => unreachable!("leader made ts safe"),
                }
            })
            .collect();
        (ts, vals)
    }
    fn read_latest(&self, k: Key) -> Option<i64> {
        self.groups[self.group_of(k)].replicas[0].read(k, Ts::MAX)
    }
}

// ---------------------------------------------------------------- experiment 1: external consistency

#[derive(Default, Clone, Copy)]
struct Tally {
    schedules_violating: usize,
    pair_violations: usize,
    ro_misses: usize,
    wait_sum: i64,
    commits: usize,
}

/// 2000 random chains: T1 acks, T2 starts after the ack, ... then a read-only txn starting after the last ack.
fn external_consistency(eps: i64, commit_wait: bool, seed: u64) -> Tally {
    let mut rng = Rng(seed);
    let mut t = Tally::default();
    for _ in 0..2000 {
        let mut c = Cluster::new(3, eps, commit_wait, &mut rng);
        let mut real = 1000 + rng.below(500) as i64;
        let mut prev: Option<Ts> = None;
        let (mut violating, mut keys_written) = (false, vec![]);
        for txn in 1..=4u64 {
            // Keys are fresh per transaction; one or two groups, so both the 1PC fast path and 2PC are exercised.
            let k1 = txn * 10 + rng.below(3);
            let mut writes = vec![(k1, txn as i64)];
            if rng.below(2) == 0 {
                let k2 = txn * 10 + 3 + rng.below(3);
                if c.group_of(k2) != c.group_of(k1) {
                    writes.push((k2, txn as i64));
                }
            }
            let done = c.commit_rw(txn, &writes, real);
            t.wait_sum += done.wait_extra;
            t.commits += 1;
            if let Some(p) = prev {
                if done.ts <= p {
                    t.pair_violations += 1;
                    violating = true;
                }
            }
            prev = Some(done.ts);
            keys_written.extend(writes.iter().map(|w| w.0));
            real = done.ack + rng.below(4) as i64; // the next transaction starts after the ack, maybe immediately
        }
        let (_, vals) = c.read_only(&keys_written, real, rng.below(3) as usize);
        let misses = vals.iter().filter(|v| v.is_none()).count();
        t.ro_misses += misses;
        violating |= misses > 0;
        t.schedules_violating += violating as usize;
    }
    t
}

// ---------------------------------------------------------------- experiment 2: 2PL + wait-die + 2PC transfers

#[derive(Clone)]
enum Step {
    LockA,
    LockB,
    Commit,
    Release { at: i64, ts: Ts, ra: i64, rb: i64 },
    Done,
}

struct Client {
    prio: u64, // wait-die priority: smaller = older = allowed to wait
    a: Key,
    b: Key,
    amt: i64,
    step: Step,
    wake: i64,
}

struct Serial {
    ts: Ts,
    a: Key,
    b: Key,
    amt: i64,
    ra: i64,
    rb: i64,
}

fn concurrent_transfers(rng: &mut Rng) -> (usize, usize) {
    const ACCOUNTS: u64 = 40;
    let mut c = Cluster::new(3, TRUE_ERR, true, rng);
    let init: Vec<(Key, i64)> = (0..ACCOUNTS).map(|k| (k, 1000)).collect();
    let boot = c.commit_rw(0, &init, 1000);
    let mut now = boot.ack + 1;
    let mut clients: Vec<Client> = (0..300u64)
        .map(|i| {
            let a = rng.below(ACCOUNTS);
            let b = (a + 1 + rng.below(ACCOUNTS - 1)) % ACCOUNTS;
            Client {
                prio: i,
                a,
                b,
                amt: 1 + rng.below(50) as i64,
                step: Step::LockA,
                wake: now,
            }
        })
        .collect();
    let mut locks: HashMap<Key, usize> = HashMap::new();
    let mut serial = vec![];
    let (mut aborts, mut txn_id) = (0usize, 1u64);
    while clients.iter().any(|c| !matches!(c.step, Step::Done)) {
        // Only a few clients are active at a time (like worker threads); pick randomly among the ready ones.
        let ready: Vec<usize> = (0..clients.len())
            .filter(|&i| !matches!(clients[i].step, Step::Done) && clients[i].wake <= now)
            .take(8)
            .collect();
        now += 1;
        if ready.is_empty() {
            continue;
        }
        let i = ready[rng.below(ready.len() as u64) as usize];
        let step = clients[i].step.clone();
        match step {
            Step::LockA | Step::LockB => {
                let key = if matches!(step, Step::LockA) {
                    clients[i].a
                } else {
                    clients[i].b
                };
                match locks.get(&key).copied() {
                    None => {
                        locks.insert(key, i);
                        clients[i].step = if matches!(step, Step::LockA) {
                            Step::LockB
                        } else {
                            Step::Commit
                        };
                    }
                    Some(owner) if owner == i => unreachable!(),
                    Some(owner) => {
                        if clients[i].prio < clients[owner].prio {
                            // Older requester waits.
                        } else {
                            // WAIT-DIE: a younger requester aborts itself (keeps its priority, so it cannot starve), releases and retries.
                            locks.retain(|_, o| *o != i);
                            aborts += 1;
                            clients[i].step = Step::LockA;
                            clients[i].wake = now + 1 + rng.below(6) as i64;
                        }
                    }
                }
            }
            Step::Commit => {
                let (a, b, amt) = (clients[i].a, clients[i].b, clients[i].amt);
                let (ra, rb) = (c.read_latest(a).unwrap(), c.read_latest(b).unwrap());
                let done = c.commit_rw(txn_id, &[(a, ra - amt), (b, rb + amt)], now);
                txn_id += 1;
                clients[i].step = Step::Release {
                    at: done.ack,
                    ts: done.ts,
                    ra,
                    rb,
                };
                clients[i].wake = done.ack; // locks are held through commit wait
            }
            Step::Release { ts, ra, rb, .. } => {
                locks.retain(|_, o| *o != i);
                serial.push(Serial {
                    ts,
                    a: clients[i].a,
                    b: clients[i].b,
                    amt: clients[i].amt,
                    ra,
                    rb,
                });
                clients[i].step = Step::Done;
            }
            Step::Done => {}
        }
    }
    // Serial replay in commit-timestamp order must reproduce every value each transaction read, and the final state.
    serial.sort_by_key(|s| s.ts);
    let uniq: BTreeSet<Ts> = serial.iter().map(|s| s.ts).collect();
    let mut model: Vec<i64> = vec![1000; ACCOUNTS as usize];
    for s in &serial {
        assert_eq!(
            (s.ra, s.rb),
            (model[s.a as usize], model[s.b as usize]),
            "not equivalent to the serial order of commit timestamps"
        );
        model[s.a as usize] -= s.amt;
        model[s.b as usize] += s.amt;
    }
    for k in 0..ACCOUNTS {
        assert_eq!(c.read_latest(k), Some(model[k as usize]));
    }
    assert_eq!(
        model.iter().sum::<i64>(),
        1000 * ACCOUNTS as i64,
        "money is conserved"
    );
    assert_eq!(serial.len(), 300);
    assert!(uniq.len() > 1);
    // Snapshot read sees a consistent total at any timestamp (no lock taken).
    let mid = serial[serial.len() / 2].ts;
    let total: i64 = (0..ACCOUNTS)
        .map(|k| match c.groups[c.group_of(k)].read_at(0, k, mid) {
            ReadResult::Value(v) => v.unwrap(),
            ReadResult::Blocked { .. } => unreachable!(),
        })
        .sum();
    assert_eq!(
        total,
        1000 * ACCOUNTS as i64,
        "snapshot at a past timestamp is internally consistent"
    );
    (serial.len(), aborts)
}

// ---------------------------------------------------------------- experiment 3: safe time at lagging replicas

fn safe_time_reads(rng: &mut Rng) -> (usize, usize) {
    let mut c = Cluster::new(3, TRUE_ERR, true, rng);
    let mut now = 1000i64;
    let mut pending: Vec<(u64, Vec<usize>, Vec<(Key, i64)>)> = vec![];
    let mut txn = 1u64;
    let mut max_ts = 0;
    // Drive the groups with interleaved single-group writes and 2PC whose Commit entries arrive later than other writes.
    for _ in 0..400 {
        now += 3;
        match rng.below(4) {
            0 | 1 => {
                let k = rng.below(30);
                let g = c.group_of(k);
                let ts = c.groups[g].next_ts(c.latest(g, now));
                c.groups[g].append(Entry::Write {
                    ts,
                    writes: vec![(k, rng.below(1000) as i64)],
                });
                max_ts = max_ts.max(ts);
            }
            2 => {
                let (k1, k2) = (rng.below(30), rng.below(30));
                let (g1, g2) = (c.group_of(k1), c.group_of(k2));
                if g1 != g2 {
                    let mut gs = vec![];
                    let mut writes = vec![];
                    for (g, k) in [(g1, k1), (g2, k2)] {
                        let p = c.groups[g].next_ts(c.latest(g, now));
                        let w = vec![(k, rng.below(1000) as i64)];
                        c.groups[g].append(Entry::Prepare {
                            txn,
                            ts: p,
                            writes: w.clone(),
                        });
                        gs.push(g);
                        writes.extend(w);
                    }
                    pending.push((txn, gs, writes));
                    txn += 1;
                }
            }
            _ => {
                if !pending.is_empty() {
                    let (id, gs, _) = pending.remove(rng.below(pending.len() as u64) as usize);
                    let prep_max = gs
                        .iter()
                        .map(|&g| c.groups[g].replicas[0].prepared[&id].0)
                        .max()
                        .unwrap();
                    let s = gs
                        .iter()
                        .map(|&g| c.groups[g].next_ts(c.latest(g, now)))
                        .max()
                        .unwrap()
                        .max(prep_max);
                    for &g in &gs {
                        c.groups[g].append(Entry::Commit { txn: id, ts: s });
                    }
                    max_ts = max_ts.max(s);
                }
            }
        }
    }
    for (id, gs, _) in pending.drain(..).collect::<Vec<_>>() {
        let s = gs
            .iter()
            .map(|&g| c.groups[g].replicas[0].prepared[&id].0)
            .max()
            .unwrap()
            .max(c.groups.iter().map(|g| g.last_ts).max().unwrap() + 1);
        for &g in &gs {
            c.groups[g].append(Entry::Commit { txn: id, ts: s });
        }
        max_ts = max_ts.max(s);
    }
    // Followers apply random prefixes of the log; t_safe must be monotone along the way.
    for g in 0..3 {
        for r in 1..3 {
            let target = rng.below(c.groups[g].log.len() as u64 + 1) as usize;
            let mut last = Ts::MIN;
            while c.groups[g].replicas[r].applied < target {
                c.groups[g].catch_up(r, 1);
                let s = c.groups[g].replicas[r].t_safe();
                assert!(s >= last, "t_safe went backwards");
                last = s;
            }
        }
    }
    let (mut ok, mut blocked) = (0, 0);
    for _ in 0..2000 {
        let k = rng.below(30);
        let g = c.group_of(k);
        let r = 1 + rng.below(2) as usize;
        let ts = 1000 + rng.below((max_ts - 1000 + 20) as u64) as i64;
        let truth = c.groups[g].replicas[0].read(k, ts);
        match c.groups[g].read_at(r, k, ts) {
            ReadResult::Value(v) => {
                assert_eq!(v, truth, "a read below t_safe must match the final answer");
                ok += 1;
            }
            ReadResult::Blocked { t_safe } => {
                assert!(t_safe < ts);
                blocked += 1;
            }
        }
    }
    // After the replicas catch up every read is servable and equals the leader.
    for g in 0..3 {
        for r in 1..3 {
            let n = c.groups[g].log.len();
            c.groups[g].catch_up(r, n);
            assert_eq!(c.groups[g].replicas[r].store, c.groups[g].replicas[0].store);
        }
    }
    (ok, blocked)
}

fn main() {
    println!(
        "== TrueTime: interval [earliest, latest] with epsilon, drifting clocks bounded by E = {TRUE_ERR}"
    );
    let mut rng = Rng(7);
    for eps in [TRUE_ERR / 2, TRUE_ERR, 2 * TRUE_ERR] {
        let clock = Clock {
            phase: rng.below(320) as i64,
        };
        let mut inside = 0;
        for real in 0..2000 {
            let (e, l) = tt_now(clock, eps, real);
            inside += (e <= real && real <= l) as usize;
        }
        println!(
            "  eps = {eps:>2}: real time inside TT.now() on {inside}/2000 readings {}",
            if eps >= TRUE_ERR {
                "(bound holds)"
            } else {
                "(bound BROKEN: eps < true error)"
            }
        );
        assert_eq!(inside == 2000, eps >= TRUE_ERR);
    }

    println!(
        "== external consistency over 2000 random schedules (4 chained txns + a read-only txn each)"
    );
    println!(
        "  {:<34} {:>10} {:>10} {:>10}",
        "configuration", "bad sched", "ts inv.", "RO misses"
    );
    let cases = [
        ("commit wait, eps = E", TRUE_ERR, true),
        ("commit wait, eps = 2E", 2 * TRUE_ERR, true),
        ("NO commit wait, eps = E", TRUE_ERR, false),
        ("commit wait, eps = E/5 (too small)", TRUE_ERR / 5, true),
    ];
    let mut res = vec![];
    for (name, eps, wait) in cases {
        let t = external_consistency(eps, wait, 42);
        println!(
            "  {name:<34} {:>10} {:>10} {:>10}",
            t.schedules_violating, t.pair_violations, t.ro_misses
        );
        res.push(t);
    }
    assert_eq!(
        res[0].schedules_violating + res[1].schedules_violating,
        0,
        "commit wait with eps >= E must be externally consistent"
    );
    assert!(
        res[2].pair_violations > 0 && res[2].ro_misses > 0,
        "without commit wait the property must break"
    );
    assert!(
        res[3].pair_violations > 0,
        "too small an epsilon breaks it even with commit wait"
    );

    println!("== commit-wait latency vs epsilon (mean extra ack latency per commit, same units)");
    let mut prev = -1.0;
    for eps in [0, 1, 2, 5, 10, 20, 40] {
        let t = external_consistency(eps, true, 9);
        let mean = t.wait_sum as f64 / t.commits as f64;
        println!("  eps {eps:>2}: mean wait {mean:>6.2}   (about 2*eps when the clock is exact)");
        assert!(mean >= prev, "wait must not shrink as eps grows");
        prev = mean;
    }

    println!("== 2PL (wait-die) + 2PC transfers across 3 groups, 300 concurrent transactions");
    let (n, aborts) = concurrent_transfers(&mut Rng(2024));
    println!(
        "  {n} transfers committed, {aborts} wait-die aborts/retries; equal to serial replay in commit-ts order; money conserved"
    );

    println!("== safe time: follower reads against lagging replicas");
    let (ok, blocked) = safe_time_reads(&mut Rng(31337));
    println!(
        "  2000 random follower reads: {ok} served below t_safe (all equal the leader's MVCC answer), {blocked} would block"
    );
    assert!(ok > 0 && blocked > 0);

    println!("spanner_truetime_commit_wait: all checks passed");
}
