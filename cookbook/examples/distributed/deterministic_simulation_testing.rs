//! RECIPE 853 (distributed): deterministic simulation testing (FoundationDB / TigerBeetle style): a Raft KV store in one thread with simulated time, network and disk
//! LEVEL: advanced
//! Use in: testing any distributed or concurrent system (databases, consensus, queues, schedulers) by replaying whole clusters from one seed; debugging "it failed once in CI".   Run: cargo run --release --example deterministic_simulation_testing
#![allow(dead_code)]
//!
//! THE IDEA: real distributed bugs need rare interleavings (a crash between an ack and an fsync, a partition that heals at the
//! wrong moment). Testing on real machines finds them rarely and cannot reproduce them. Instead run the WHOLE cluster inside ONE
//! thread: no real clock, no real network, no real disk. Time is a counter, the network is an event queue, the disk is a struct.
//! Every source of nondeterminism comes from ONE seeded RNG, so `seed -> exactly the same execution`, bit for bit. A failure is
//! then a number you can replay under a debugger, and you can run millions of seeds overnight (compressed time: a simulated
//! hour costs milliseconds). FoundationDB and TigerBeetle (the VOPR) test this way; Antithesis sells it as a hypervisor.
//! THE SYSTEM UNDER TEST: a compact Raft (3 nodes): terms, randomized election timeouts, RequestVote with the log up-to-date
//! rule, AppendEntries with consistency check and conflict truncation, commit only for current-term entries, a leader no-op,
//! client sessions deduplicated by op id, and a linearizable KV where every read goes THROUGH the log.
//! THE SIMULATED WORLD: network messages are delayed 1-4 ticks (so reordered), dropped, duplicated, and cut by PARTITIONS;
//! the disk accepts a write that completes later (fsync takes time and can FAIL, which the node treats as fatal: crash and
//! restart from what is durable), a crash loses unsynced writes and may leave a TORN write: a valid prefix of an in-flight
//! append. Nodes only answer after their own state is durable ("persist, then reply" via a deferred outbox keyed by fsync ticket).
//! BUGGIFY: code sprinkled with `if buggify(p)` hooks that, only in simulation, widen race windows (slow fsync, delay spikes,
//! crash right after sending an ack). Real systems do this too (FoundationDB's BUGGIFY macro).
//! FAULT PLAN = EXTERNAL EVENTS (crashes, partitions, heals, client operations) generated from the seed as an explicit list, so a
//! failing plan can be SHRUNK by delta debugging (drop chunks of events while it still fails) to a handful of events.
//! CHECKS (all online, plus a post-hoc one): election safety (one leader per term), state-machine safety (no two different
//! entries committed at an index), LEADER COMPLETENESS (a new leader holds every entry ever committed: "no committed entry
//! lost"), LOG MATCHING (same term at an index implies identical prefixes), a WGL-style LINEARIZABILITY CHECKER over the recorded
//! client history (ops that timed out are 'maybe happened'), and liveness after healing (a final probe write completes and
//! all replicas converge).
//! A LIVENESS CHECK NEEDS A FAULT-FREE PHASE AND NO CLIENT POLICY INSIDE IT: after T_END every injected fault (drops, fsync
//! failures, buggify) is off, and the probe retries until T_STOP. A bounded-retry probe measures the client's patience, not the
//! cluster; the only deadline is T_STOP.
//! LIMITS: log matching is checked every 20 ticks, not after every event. Passing seeds are evidence, not proof. Removing
//! a plan event also shifts later random draws, because one generator drives the whole world.
//! INJECTED BUGS: `AckBeforeFsync` (a follower acknowledges AppendEntries before its disk write is durable: a majority "commits"
//! an entry that evaporates when the follower crashes) and `StaleLeaderRead` (a leader answers GETs from local state without
//! going through the log, so a partitioned old leader serves stale values). The simulator must find each within a seed budget,
//! name the smallest failing seed, replay it identically, and shrink its schedule until no single event can be removed. The correct system must pass 2000 seeds.
//! COMMON USES: FoundationDB (the original, Will Wilson's talk), TigerBeetle VOPR, Antithesis, Dropbox Nucleus (CanopyCheck),
//! Rust: `turmoil`, `madsim`, `shuttle` (thread schedules), loom (memory model), Jepsen for the black-box counterpart.
//! Read: "Testing Distributed Systems w/ Deterministic Simulation" (Will Wilson, Strange Loop 2014) and the FoundationDB paper
//! "FoundationDB: A Distributed Unbundled Transactional Key Value Store" (SIGMOD 2021) both cited by title only (URLs not
//! checked); the Raft paper "In Search of an Understandable Consensus Algorithm" (Ongaro, Ousterhout); "Testing Linearizability" (Wing & Gong).
//!
//! TRY NEXT:
//!   - [S] Change `AckBeforeFsync` so the follower acks before fsync only for the FIRST entry of a batch: how many more seeds are needed to find the bug? Predict, then measure.
//!   - [S] Lower the fsync delay buggify from +15 to +1 ticks (`persist`) and see how the detection rate of `AckBeforeFsync` drops: which ingredient (window width vs crash timing) matters more?
//!   - [M] Add a third injected bug: do not persist `voted_for` before replying to RequestVote. Which invariant catches it and how many seeds does it need?
//!   - [M] Make the shrinker also reduce numeric parameters (partition duration, crash `down_for`), not just drop events, and compare final plan sizes.
//!   - [A] PROJECT: port the sim loop to run `linearizability_checker`'s model against a different system: take `raft_consensus` or `quorum_replication_dynamo` as the system under test and reuse this fault plan generator.

use std::collections::{BTreeMap, BTreeSet, HashSet};

const N: usize = 3;
const T_END: u64 = 600; // the plan (crashes, partitions, client ops) ends before this; at T_END all random fault injection stops
const T_STOP: u64 = 1400;
const NONE: usize = usize::MAX;
const PROBE: usize = 9999; // op id of the post-heal liveness write
const PROBE_AT: u64 = T_END + 150;
const CLIENT_ATTEMPTS: u32 = 14; // ordinary clients give up after this; the probe never does

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

#[derive(Clone, Copy, PartialEq, Debug)]
enum Variant {
    Correct,
    AckBeforeFsync,
    StaleLeaderRead,
}

// ------------------------------------------------------------------------------------------------ plan (external events)

#[derive(Clone, Debug)]
struct OpSpec {
    id: usize,
    key: u32,
    put: Option<i64>, // None = GET
}
#[derive(Clone, Debug)]
enum Ext {
    Crash { node: usize, down_for: u64 },
    Partition { group: [u8; N] },
    Heal,
    Op(OpSpec),
}
#[derive(Clone, Debug)]
struct Item {
    at: u64,
    ext: Ext,
}
type Plan = Vec<Item>;

fn gen_plan(seed: u64) -> Plan {
    let mut rng = Rng::new(seed ^ 0xA5A5_5A5A_1234_5678);
    let mut plan = vec![];
    let n_ops = 30 + rng.below(25) as usize;
    for id in 0..n_ops {
        let key = rng.below(3) as u32;
        let put = if rng.pm(500) {
            Some(id as i64 + 1)
        } else {
            None
        }; // unique values make linearizability checking easy
        plan.push(Item {
            at: 10 + rng.below(T_END - 40),
            ext: Ext::Op(OpSpec { id, key, put }),
        });
    }
    for _ in 0..(2 + rng.below(5)) {
        plan.push(Item {
            at: 20 + rng.below(T_END - 120),
            ext: Ext::Crash {
                node: rng.below(N as u64) as usize,
                down_for: 5 + rng.below(60),
            },
        });
    }
    for _ in 0..(1 + rng.below(3)) {
        let at = 20 + rng.below(T_END - 160);
        let lone = rng.below(N as u64) as usize;
        let mut group = [0u8; N];
        group[lone] = 1;
        plan.push(Item {
            at,
            ext: Ext::Partition { group },
        });
        plan.push(Item {
            at: at + 20 + rng.below(100),
            ext: Ext::Heal,
        });
    }
    plan.sort_by_key(|i| i.at);
    plan
}

// ------------------------------------------------------------------------------------------------ simulated state

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    term: u64,
    op: Option<usize>, // None = leader no-op
}
#[derive(Clone, Debug, Default)]
struct Durable {
    term: u64,
    voted: Option<usize>,
    log: Vec<Entry>,
}
#[derive(Clone, Debug)]
enum Msg {
    ReqVote {
        term: u64,
        cand: usize,
        last_idx: usize,
        last_term: u64,
    },
    VoteResp {
        term: u64,
        granted: bool,
    },
    Append {
        term: u64,
        leader: usize,
        prev_idx: usize,
        prev_term: u64,
        entries: Vec<Entry>,
        commit: usize,
    },
    AppendResp {
        term: u64,
        success: bool,
        match_idx: usize,
    },
    Client {
        op: usize,
    },
}
#[derive(Clone, Debug)]
enum Res {
    Ok,
    Val(Option<i64>),
    NotLeader(usize),
}
enum Ev {
    Deliver {
        to: usize,
        from: usize,
        msg: Msg,
    },
    Tick {
        node: usize,
        epoch: u64,
    },
    FsyncDone {
        node: usize,
        epoch: u64,
        ticket: u64,
    },
    Ext(Ext),
    Restart {
        node: usize,
    },
    ClientStart {
        op: usize,
    },
    ClientTimeout {
        op: usize,
        attempt: u32,
    },
    ClientReply {
        op: usize,
        res: Res,
    },
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Role {
    Follower,
    Candidate,
    Leader,
}

struct Node {
    up: bool,
    epoch: u64,
    role: Role,
    term: u64,
    voted: Option<usize>,
    log: Vec<Entry>,
    commit: usize,
    applied: usize,
    kv: BTreeMap<u32, i64>,
    applied_ops: BTreeSet<usize>,
    votes: BTreeSet<usize>,
    next_idx: [usize; N],
    match_idx: [usize; N],
    election_deadline: u64,
    next_hb: u64,
    leader_hint: usize,
    pending: BTreeMap<usize, usize>, // log index -> op id (leader only)
    deferred: Vec<(u64, usize, Msg)>, // (fsync ticket, to, message)
    last_ticket: u64,
    durable_ticket: u64,
    // the disk
    disk: Durable,
    inflight: Vec<(u64, Durable)>,
}
impl Node {
    fn new() -> Node {
        Node {
            up: true,
            epoch: 0,
            role: Role::Follower,
            term: 0,
            voted: None,
            log: vec![],
            commit: 0,
            applied: 0,
            kv: BTreeMap::new(),
            applied_ops: BTreeSet::new(),
            votes: BTreeSet::new(),
            next_idx: [1; N],
            match_idx: [0; N],
            election_deadline: 0,
            next_hb: 0,
            leader_hint: NONE,
            pending: BTreeMap::new(),
            deferred: vec![],
            last_ticket: 0,
            durable_ticket: 0,
            disk: Durable::default(),
            inflight: vec![],
        }
    }
    fn last_idx(&self) -> usize {
        self.log.len()
    }
    fn last_term(&self) -> u64 {
        self.log.last().map_or(0, |e| e.term)
    }
}

#[derive(Clone, Debug)]
struct HOp {
    key: u32,
    put: Option<i64>,
    started: u64, // tick, only for reporting latency
    /// Event numbers, not ticks: two events in one tick still happen one after the other, and the checker needs that order.
    invoke: u64,
    ret: Option<u64>,
    done_at: u64,             // tick of the reply
    got: Option<Option<i64>>, // completed GET result
}

impl Client {
    /// An ordinary client has a finite retry budget. The liveness probe has none: liveness means "the write EVENTUALLY
    /// completes" and its only deadline is T_STOP. A retry budget is client policy, and legitimate split votes can outlast it.
    fn exhausted(&self, op: usize) -> bool {
        op != PROBE && self.attempt >= CLIENT_ATTEMPTS
    }
}

struct Client {
    hint: usize,
    attempt: u32,
    done: bool,
    gave_up: bool,
}

struct Report {
    violation: Option<String>,
    lin_ok: bool,
    liveness_ok: bool,
    hash: u64,
    trace: Vec<String>,
    completed: usize,
    indeterminate: usize,
    committed_entries: usize,
    buggify_hits: u64,
    probe_latency: Option<u64>,
}
impl Report {
    fn failed(&self) -> bool {
        self.violation.is_some() || !self.lin_ok
    }
}

struct Sim {
    now: u64,
    handled: u64, // events handled so far: the clock of the client history
    rng: Rng,
    seq: u64,
    queue: BTreeMap<(u64, u64), Ev>,
    nodes: Vec<Node>,
    variant: Variant,
    group: [u8; N],
    reliable: bool,
    next_ticket: u64,
    // oracle
    committed: BTreeMap<usize, Entry>,
    leaders: BTreeMap<u64, usize>,
    violation: Option<String>,
    // clients
    specs: BTreeMap<usize, OpSpec>,
    clients: BTreeMap<usize, Client>,
    history: BTreeMap<usize, HOp>,
    // trace
    hash: u64,
    text: bool,
    trace: Vec<String>,
    buggify_hits: u64,
    buggify_on: bool,
}

impl Sim {
    fn new(seed: u64, variant: Variant, plan: &Plan, text: bool) -> Sim {
        let mut s = Sim {
            now: 0,
            handled: 0,
            rng: Rng::new(seed),
            seq: 0,
            queue: BTreeMap::new(),
            nodes: (0..N).map(|_| Node::new()).collect(),
            variant,
            group: [0; N],
            reliable: false,
            next_ticket: 0,
            committed: BTreeMap::new(),
            leaders: BTreeMap::new(),
            violation: None,
            specs: BTreeMap::new(),
            clients: BTreeMap::new(),
            history: BTreeMap::new(),
            hash: 0xcbf2_9ce4_8422_2325,
            text,
            trace: vec![],
            buggify_hits: 0,
            buggify_on: true,
        };
        for n in 0..N {
            s.nodes[n].election_deadline = 20 + s.rng.below(40);
            s.at(1, Ev::Tick { node: n, epoch: 0 });
        }
        for item in plan {
            if let Ext::Op(o) = &item.ext {
                s.specs.insert(o.id, o.clone());
                s.at(item.at, Ev::ClientStart { op: o.id });
            } else {
                s.at(item.at, Ev::Ext(item.ext.clone()));
            }
        }
        // after T_END the world heals: reliable network, nobody left down, and a probe write to prove liveness
        s.at(T_END, Ev::Ext(Ext::Heal));
        let probe = OpSpec {
            id: PROBE,
            key: 0,
            put: Some(PROBE as i64),
        };
        s.specs.insert(PROBE, probe);
        s.at(PROBE_AT, Ev::ClientStart { op: PROBE });
        s
    }
    fn at(&mut self, t: u64, ev: Ev) {
        self.seq += 1;
        self.queue.insert((t.max(self.now), self.seq), ev);
    }
    fn after(&mut self, d: u64, ev: Ev) {
        self.at(self.now + d, ev);
    }
    fn tr(&mut self, tag: &str, a: u64, b: u64, c: u64) {
        for x in [a, b, c, tag.len() as u64] {
            self.hash ^= x;
            self.hash = self.hash.wrapping_mul(0x100_0000_01b3);
        }
        if self.text {
            let line = format!("{:>5} {tag} {a} {b} {c}", self.now);
            self.trace.push(line);
        }
    }
    fn buggify(&mut self, p: u64) -> bool {
        let hit = self.buggify_on && self.rng.pm(p);
        if hit {
            self.buggify_hits += 1;
        }
        hit
    }
    fn violate(&mut self, what: String) {
        if self.violation.is_none() {
            self.violation = Some(format!("t={} {what}", self.now));
        }
    }

    // ---- network and disk

    fn linked(&self, a: usize, b: usize) -> bool {
        self.group[a] == self.group[b]
    }
    fn send(&mut self, from: usize, to: usize, msg: Msg) {
        if !self.linked(from, to) || (!self.reliable && self.rng.pm(60)) {
            self.tr("drop", from as u64, to as u64, 0);
            return;
        }
        let mut d = 1 + self.rng.below(4);
        if self.buggify(10) {
            d += 25; // delay spike: reorders against later messages
        }
        self.after(
            d,
            Ev::Deliver {
                to,
                from,
                msg: msg.clone(),
            },
        );
        if !self.reliable && self.rng.pm(40) {
            let d2 = 1 + self.rng.below(6);
            self.after(d2, Ev::Deliver { to, from, msg });
        }
    }
    /// Start writing the node's persistent state; it becomes durable when FsyncDone fires. Returns the ticket.
    fn persist(&mut self, n: usize) -> u64 {
        self.next_ticket += 1;
        let t = self.next_ticket;
        let snap = Durable {
            term: self.nodes[n].term,
            voted: self.nodes[n].voted,
            log: self.nodes[n].log.clone(),
        };
        self.nodes[n].inflight.push((t, snap));
        self.nodes[n].last_ticket = t;
        let mut d = 1 + self.rng.below(3);
        if self.buggify(50) {
            d += 15; // a slow disk widens the window between "written" and "durable"
        }
        let epoch = self.nodes[n].epoch;
        self.after(
            d,
            Ev::FsyncDone {
                node: n,
                epoch,
                ticket: t,
            },
        );
        t
    }
    /// Send `msg` once the state it depends on (`ticket`) is durable.
    fn reply_after(&mut self, n: usize, ticket: u64, to: usize, msg: Msg) {
        if ticket <= self.nodes[n].durable_ticket {
            self.send(n, to, msg);
        } else {
            self.nodes[n].deferred.push((ticket, to, msg));
        }
    }
    fn fsync_done(&mut self, n: usize, ticket: u64) {
        if self.nodes[n].durable_ticket >= ticket {
            return;
        }
        // `reliable` is set by the T_END heal. Every injected fault must respect it, or the liveness phase is not fault free.
        if !self.reliable && self.rng.pm(8) {
            self.tr("fsync-fail", n as u64, ticket, 0);
            let down = 10 + self.rng.below(20);
            self.crash(n, down); // fsyncgate: a failed fsync must be fatal, never retried as if nothing happened
            return;
        }
        let nd = &mut self.nodes[n];
        let pos = nd.inflight.iter().position(|(t, _)| *t == ticket).unwrap();
        nd.disk = nd.inflight[pos].1.clone();
        nd.inflight.drain(..=pos);
        nd.durable_ticket = ticket;
        let (ready, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut nd.deferred)
            .into_iter()
            .partition(|(t, _, _)| *t <= ticket);
        nd.deferred = rest;
        self.tr("fsync", n as u64, ticket, 0);
        let acked = ready
            .iter()
            .any(|(_, _, m)| matches!(m, Msg::AppendResp { success: true, .. }));
        for (_, to, m) in ready {
            self.send(n, to, m);
        }
        if acked && self.buggify(30) {
            // die right after a DURABLE ack: harmless when the order is right (compare AckBeforeFsync in `on_append`)
            let down = 5 + self.rng.below(30);
            self.crash(n, down);
            return;
        }
        if self.nodes[n].role == Role::Leader {
            self.leader_try_commit(n); // our own durable log grew: it may complete a quorum
        }
    }
    fn crash(&mut self, n: usize, down_for: u64) {
        if !self.nodes[n].up {
            return;
        }
        self.tr("crash", n as u64, down_for, 0);
        // torn writes: an in-flight append may have reached the disk as a valid prefix
        let inflight = std::mem::take(&mut self.nodes[n].inflight);
        for (_, snap) in inflight {
            if self.rng.pm(400) {
                let common = self.nodes[n]
                    .disk
                    .log
                    .iter()
                    .zip(&snap.log)
                    .take_while(|(a, b)| a == b)
                    .count();
                let keep = common + self.rng.below((snap.log.len() - common) as u64 + 1) as usize;
                let mut torn = snap.clone();
                torn.log.truncate(keep);
                if self.rng.pm(500) {
                    torn.term = self.nodes[n].disk.term;
                    torn.voted = self.nodes[n].disk.voted;
                }
                if torn.term >= self.nodes[n].disk.term {
                    self.nodes[n].disk = torn;
                }
            }
        }
        let nd = &mut self.nodes[n];
        nd.up = false;
        nd.epoch += 1;
        nd.deferred.clear();
        self.after(down_for, Ev::Restart { node: n });
    }
    fn restart(&mut self, n: usize) {
        let d = self.nodes[n].disk.clone();
        let ed = self.now + 30 + self.rng.below(40);
        let nd = &mut self.nodes[n];
        let epoch = nd.epoch;
        *nd = Node {
            epoch,
            disk: d.clone(),
            ..Node::new()
        };
        nd.term = d.term;
        nd.voted = d.voted;
        nd.log = d.log;
        nd.election_deadline = ed;
        self.tr("restart", n as u64, self.nodes[n].log.len() as u64, 0);
        self.after(1, Ev::Tick { node: n, epoch });
    }

    // ---- raft

    fn become_follower(&mut self, n: usize, term: u64) -> bool {
        let nd = &mut self.nodes[n];
        let changed = term > nd.term;
        if changed {
            nd.term = term;
            nd.voted = None;
        }
        if nd.role != Role::Follower {
            nd.pending.clear();
            nd.leader_hint = NONE; // a deposed leader must not keep pointing clients at itself
        }
        nd.role = Role::Follower;
        changed
    }
    fn reset_election(&mut self, n: usize) {
        let d = if self.buggify(50) {
            12
        } else {
            40 + self.rng.below(40)
        };
        self.nodes[n].election_deadline = self.now + d;
    }
    fn tick(&mut self, n: usize) {
        let epoch = self.nodes[n].epoch;
        self.after(2, Ev::Tick { node: n, epoch });
        match self.nodes[n].role {
            Role::Leader => {
                if self.now >= self.nodes[n].next_hb {
                    self.nodes[n].next_hb = self.now + 6;
                    for p in (0..N).filter(|&p| p != n) {
                        self.send_append(n, p);
                    }
                }
            }
            _ => {
                if self.now >= self.nodes[n].election_deadline {
                    self.start_election(n);
                }
            }
        }
    }
    fn start_election(&mut self, n: usize) {
        let nd = &mut self.nodes[n];
        nd.term += 1;
        nd.voted = Some(n);
        nd.role = Role::Candidate;
        nd.votes = BTreeSet::from([n]);
        let (term, li, lt) = (nd.term, nd.last_idx(), nd.last_term());
        self.reset_election(n);
        self.tr("election", n as u64, term, 0);
        let ticket = self.persist(n);
        for p in (0..N).filter(|&p| p != n) {
            self.reply_after(
                n,
                ticket,
                p,
                Msg::ReqVote {
                    term,
                    cand: n,
                    last_idx: li,
                    last_term: lt,
                },
            );
        }
    }
    fn become_leader(&mut self, n: usize) {
        let term = self.nodes[n].term;
        if let Some(&other) = self.leaders.get(&term) {
            if other != n {
                self.violate(format!(
                    "ELECTION SAFETY: two leaders in term {term}: {other} and {n}"
                ));
            }
        }
        self.leaders.insert(term, n);
        // LEADER COMPLETENESS: every entry ever committed must be in this log
        let missing: Vec<usize> = self
            .committed
            .iter()
            .filter(|(i, e)| self.nodes[n].log.get(**i - 1) != Some(*e))
            .map(|(i, _)| *i)
            .collect();
        if let Some(i) = missing.first() {
            self.violate(format!("LEADER COMPLETENESS: node {n} became leader in term {term} without committed entry #{i}"));
        }
        let nd = &mut self.nodes[n];
        nd.role = Role::Leader;
        nd.leader_hint = n;
        nd.next_idx = [nd.log.len() + 1; N];
        nd.match_idx = [0; N];
        nd.next_hb = 0;
        nd.log.push(Entry { term, op: None }); // no-op so entries of older terms can commit
        self.tr("leader", n as u64, term, 0);
        self.persist(n);
        for p in (0..N).filter(|&p| p != n) {
            self.send_append(n, p);
        }
    }
    fn send_append(&mut self, n: usize, p: usize) {
        let nd = &self.nodes[n];
        let next = nd.next_idx[p].clamp(1, nd.log.len() + 1);
        let prev_idx = next - 1;
        let prev_term = if prev_idx == 0 {
            0
        } else {
            nd.log[prev_idx - 1].term
        };
        let entries: Vec<Entry> = nd.log[prev_idx..].iter().take(6).cloned().collect();
        let msg = Msg::Append {
            term: nd.term,
            leader: n,
            prev_idx,
            prev_term,
            entries,
            commit: nd.commit,
        };
        self.send(n, p, msg);
    }
    fn leader_try_commit(&mut self, n: usize) {
        let nd = &self.nodes[n];
        let own = nd.disk.log.len().min(nd.log.len()); // only the DURABLE part of our own log counts toward the quorum
        let mut target = None;
        for idx in (nd.commit + 1..=nd.log.len()).rev() {
            if nd.log[idx - 1].term != nd.term {
                break;
            }
            let count = (own >= idx) as usize
                + (0..N).filter(|&p| p != n && nd.match_idx[p] >= idx).count();
            if count >= 2 {
                target = Some(idx);
                break;
            }
        }
        if let Some(idx) = target {
            self.advance_commit(n, idx);
            for p in (0..N).filter(|&p| p != n) {
                self.send_append(n, p); // tell followers promptly
            }
        }
    }
    fn advance_commit(&mut self, n: usize, new: usize) {
        for idx in self.nodes[n].commit + 1..=new {
            let e = self.nodes[n].log[idx - 1].clone();
            match self.committed.get(&idx) {
                Some(prev) if *prev != e => {
                    self.violate(format!(
                        "STATE MACHINE SAFETY: index {idx} committed as {prev:?} and as {e:?}"
                    ));
                }
                None => {
                    self.committed.insert(idx, e.clone());
                }
                _ => {}
            }
            self.apply(n, idx, &e);
        }
        self.nodes[n].commit = new;
        self.tr("commit", n as u64, new as u64, 0);
    }
    fn apply(&mut self, n: usize, idx: usize, e: &Entry) {
        self.nodes[n].applied = idx;
        let Some(op) = e.op else { return };
        let spec = self.specs[&op].clone();
        let res = match spec.put {
            Some(v) => {
                if self.nodes[n].applied_ops.insert(op) {
                    self.nodes[n].kv.insert(spec.key, v); // dedupe: a retried op is applied once
                }
                Res::Ok
            }
            None => Res::Val(self.nodes[n].kv.get(&spec.key).copied()),
        };
        if self.nodes[n].pending.remove(&idx) == Some(op) {
            self.client_reply(op, res);
        }
    }
    fn client_reply(&mut self, op: usize, res: Res) {
        if self.reliable || !self.rng.pm(50) {
            let d = 1 + self.rng.below(3);
            self.after(d, Ev::ClientReply { op, res });
        }
    }

    fn handle(&mut self, to: usize, from: usize, msg: Msg) {
        if !self.nodes[to].up || (from != NONE && !self.linked(from, to)) {
            return;
        }
        match msg {
            Msg::ReqVote {
                term,
                cand,
                last_idx,
                last_term,
            } => {
                let mut changed = false;
                if term > self.nodes[to].term {
                    changed = self.become_follower(to, term);
                }
                let nd = &self.nodes[to];
                let up_to_date = last_term > nd.last_term()
                    || (last_term == nd.last_term() && last_idx >= nd.last_idx());
                let grant = term == nd.term && up_to_date && nd.voted.is_none_or(|v| v == cand);
                if grant && nd.voted != Some(cand) {
                    self.nodes[to].voted = Some(cand);
                    changed = true;
                    self.reset_election(to);
                }
                let ticket = if changed {
                    self.persist(to)
                } else {
                    self.nodes[to].last_ticket
                };
                let t = self.nodes[to].term;
                self.reply_after(
                    to,
                    ticket,
                    from,
                    Msg::VoteResp {
                        term: t,
                        granted: grant,
                    },
                );
            }
            Msg::VoteResp { term, granted } => {
                if term > self.nodes[to].term {
                    self.become_follower(to, term);
                    self.persist(to);
                } else if self.nodes[to].role == Role::Candidate
                    && term == self.nodes[to].term
                    && granted
                {
                    self.nodes[to].votes.insert(from);
                    if self.nodes[to].votes.len() >= 2 {
                        self.become_leader(to);
                    }
                }
            }
            Msg::Append {
                term,
                leader,
                prev_idx,
                prev_term,
                entries,
                commit,
            } => self.on_append(to, from, term, leader, prev_idx, prev_term, entries, commit),
            Msg::AppendResp {
                term,
                success,
                match_idx,
            } => {
                if term > self.nodes[to].term {
                    self.become_follower(to, term);
                    self.persist(to);
                    return;
                }
                if self.nodes[to].role != Role::Leader || term != self.nodes[to].term {
                    return;
                }
                if success {
                    let nd = &mut self.nodes[to];
                    nd.match_idx[from] = nd.match_idx[from].max(match_idx);
                    nd.next_idx[from] = nd.match_idx[from] + 1;
                    let more = nd.next_idx[from] <= nd.log.len();
                    self.leader_try_commit(to);
                    if more {
                        self.send_append(to, from);
                    }
                } else {
                    let nd = &mut self.nodes[to];
                    nd.next_idx[from] = (nd.next_idx[from].saturating_sub(1))
                        .max(nd.match_idx[from] + 1)
                        .max(1);
                    self.send_append(to, from);
                }
            }
            Msg::Client { op } => self.on_client(to, op),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn on_append(
        &mut self,
        n: usize,
        from: usize,
        term: u64,
        leader: usize,
        prev_idx: usize,
        prev_term: u64,
        entries: Vec<Entry>,
        commit: usize,
    ) {
        if term < self.nodes[n].term {
            let t = self.nodes[n].term;
            self.send(
                n,
                from,
                Msg::AppendResp {
                    term: t,
                    success: false,
                    match_idx: 0,
                },
            );
            return;
        }
        let mut changed = self.become_follower(n, term);
        self.nodes[n].leader_hint = leader;
        self.reset_election(n);
        let nd = &mut self.nodes[n];
        let consistent =
            prev_idx == 0 || (nd.log.len() >= prev_idx && nd.log[prev_idx - 1].term == prev_term);
        if !consistent {
            let ticket = if changed {
                self.persist(n)
            } else {
                self.nodes[n].last_ticket
            };
            let t = self.nodes[n].term;
            self.reply_after(
                n,
                ticket,
                from,
                Msg::AppendResp {
                    term: t,
                    success: false,
                    match_idx: 0,
                },
            );
            return;
        }
        for (i, e) in entries.iter().enumerate() {
            let idx = prev_idx + 1 + i;
            if idx <= nd.log.len() {
                if nd.log[idx - 1].term != e.term {
                    nd.log.truncate(idx - 1); // conflict: the leader's log wins
                    nd.log.push(e.clone());
                    changed = true;
                }
            } else {
                nd.log.push(e.clone());
                changed = true;
            }
        }
        let last_new = prev_idx + entries.len();
        let new_commit = commit.min(last_new);
        let ticket = if changed {
            self.persist(n)
        } else {
            self.nodes[n].last_ticket
        };
        let t = self.nodes[n].term;
        let resp = Msg::AppendResp {
            term: t,
            success: true,
            match_idx: last_new,
        };
        if self.variant == Variant::AckBeforeFsync && !entries.is_empty() {
            self.send(n, from, resp); // BUG: acknowledge before the entries are durable
            if self.buggify(150) {
                let down = 5 + self.rng.below(30);
                self.crash(n, down); // ...and die right after, losing them
                return;
            }
        } else {
            self.reply_after(n, ticket, from, resp);
        }
        if new_commit > self.nodes[n].commit {
            self.advance_commit(n, new_commit);
        }
    }
    fn on_client(&mut self, n: usize, op: usize) {
        if self.nodes[n].role != Role::Leader {
            let hint = self.nodes[n].leader_hint;
            self.client_reply(op, Res::NotLeader(hint));
            return;
        }
        let spec = self.specs[&op].clone();
        if self.variant == Variant::StaleLeaderRead && spec.put.is_none() {
            let v = self.nodes[n].kv.get(&spec.key).copied();
            self.client_reply(op, Res::Val(v)); // BUG: local read, leadership never confirmed
            return;
        }
        let nd = &mut self.nodes[n];
        let idx = match nd.log.iter().position(|e| e.op == Some(op)) {
            Some(i) => i + 1, // a retry of an op we already hold
            None => {
                let term = nd.term;
                nd.log.push(Entry { term, op: Some(op) });
                nd.log.len()
            }
        };
        if idx <= nd.commit {
            // a retry of an op that already committed (the first reply was lost): answer from current state
            let res = match spec.put {
                Some(_) => Res::Ok,
                None => Res::Val(nd.kv.get(&spec.key).copied()),
            };
            self.client_reply(op, res);
            return;
        }
        nd.pending.insert(idx, op);
        if idx > nd.commit && nd.commit < nd.log.len() {
            self.persist(n);
        }
        for p in (0..N).filter(|&p| p != n) {
            self.send_append(n, p);
        }
    }

    // ---- clients

    fn client_send(&mut self, op: usize, target: usize) {
        let hint = self.clients[&op].hint;
        let to = if target != NONE {
            target
        } else if hint != NONE && self.rng.pm(800) {
            hint // follow the last leader hint most of the time, probe a random node otherwise
        } else {
            self.rng.below(N as u64) as usize
        };
        let (att, d) = (self.clients[&op].attempt, 1 + self.rng.below(3));
        self.after(
            d,
            Ev::Deliver {
                to,
                from: NONE,
                msg: Msg::Client { op },
            },
        );
        self.after(30, Ev::ClientTimeout { op, attempt: att });
    }
    fn client_event(&mut self, ev: Ev) {
        match ev {
            Ev::ClientStart { op } => {
                let s = self.specs[&op].clone();
                self.history.insert(
                    op,
                    HOp {
                        key: s.key,
                        put: s.put,
                        started: self.now,
                        invoke: self.handled,
                        ret: None,
                        done_at: 0,
                        got: None,
                    },
                );
                self.clients.insert(
                    op,
                    Client {
                        hint: NONE,
                        attempt: 0,
                        done: false,
                        gave_up: false,
                    },
                );
                self.client_send(op, NONE);
            }
            Ev::ClientTimeout { op, attempt } => {
                let c = &mut self.clients.get_mut(&op).unwrap();
                if c.done || c.attempt != attempt {
                    return;
                }
                c.attempt += 1;
                if c.exhausted(op) {
                    c.gave_up = true;
                    c.done = true;
                    return;
                }
                self.client_send(op, NONE);
            }
            Ev::ClientReply { op, res } => {
                let c = self.clients.get_mut(&op).unwrap();
                if c.done {
                    return;
                }
                match res {
                    Res::NotLeader(h) => {
                        c.hint = h;
                        // back off a little (a hint can be stale, and hammering a follower burns the retry budget)
                        c.attempt += 1;
                        if c.exhausted(op) {
                            c.gave_up = true;
                            c.done = true;
                            return;
                        }
                        let att = c.attempt;
                        self.after(6, Ev::ClientTimeout { op, attempt: att });
                    }
                    Res::Ok | Res::Val(_) => {
                        c.done = true;
                        let h = self.history.get_mut(&op).unwrap();
                        h.ret = Some(self.handled);
                        h.done_at = self.now;
                        if let Res::Val(v) = res {
                            h.got = Some(v);
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    // ---- invariants and main loop

    fn check_log_matching(&mut self) {
        for a in 0..N {
            for b in a + 1..N {
                let (la, lb) = (&self.nodes[a].log, &self.nodes[b].log);
                let m = la.len().min(lb.len());
                let mut agree_from_top = None;
                for i in (0..m).rev() {
                    if la[i].term == lb[i].term {
                        agree_from_top = Some(i);
                        break;
                    }
                }
                if let Some(top) = agree_from_top {
                    if la[..=top] != lb[..=top] {
                        self.violate(format!(
                            "LOG MATCHING: nodes {a} and {b} share term at #{} but differ before",
                            top + 1
                        ));
                    }
                }
            }
        }
    }
    fn ext(&mut self, e: Ext) {
        match e {
            Ext::Crash { node, down_for } => self.crash(node, down_for),
            Ext::Partition { group } => {
                self.group = group;
                self.tr(
                    "partition",
                    group[0] as u64,
                    group[1] as u64,
                    group[2] as u64,
                );
            }
            Ext::Heal => {
                self.group = [0; N];
                self.tr("heal", 0, 0, 0);
                if self.now >= T_END {
                    self.reliable = true;
                    self.buggify_on = false;
                    for n in 0..N {
                        if !self.nodes[n].up {
                            self.queue
                                .retain(|_, ev| !matches!(ev, Ev::Restart { node } if *node == n));
                            self.restart(n);
                        }
                    }
                }
            }
            Ext::Op(_) => unreachable!(),
        }
    }
    fn run(&mut self) {
        let mut next_check = 20;
        while let Some((&(t, k), _)) = self.queue.iter().next() {
            if t > T_STOP || self.violation.is_some() {
                break;
            }
            let ev = self.queue.remove(&(t, k)).unwrap();
            self.now = t;
            self.handled += 1;
            if self.now >= next_check {
                next_check += 20;
                self.check_log_matching();
            }
            match ev {
                Ev::Deliver { to, from, msg } => {
                    self.tr("deliver", from as u64, to as u64, 0);
                    self.handle(to, from, msg);
                }
                Ev::Tick { node, epoch } => {
                    if self.nodes[node].up && self.nodes[node].epoch == epoch {
                        self.tick(node);
                    }
                }
                Ev::FsyncDone {
                    node,
                    epoch,
                    ticket,
                } => {
                    if self.nodes[node].up && self.nodes[node].epoch == epoch {
                        self.fsync_done(node, ticket);
                    }
                }
                Ev::Ext(e) => self.ext(e),
                Ev::Restart { node } => self.restart(node), // Node::new() inside sets `up`
                other => self.client_event(other),
            }
        }
    }
    fn probe_latency(&self) -> Option<u64> {
        self.history
            .get(&PROBE)
            .and_then(|h| h.ret.map(|_| h.done_at - h.started))
    }
    fn liveness(&self) -> bool {
        let probe_ok = self.probe_latency().is_some();
        let up = self.nodes.iter().all(|n| n.up);
        let same = self
            .nodes
            .iter()
            .all(|n| n.kv == self.nodes[0].kv && n.commit == self.nodes[0].commit);
        probe_ok && up && same
    }
}

// ------------------------------------------------------------------------------------------------ linearizability checker

/// Wing & Gong / Lowe style search for one key: is there a total order consistent with real time under which every completed
/// GET returns the latest PUT? Timed-out PUTs may or may not have happened; incomplete GETs are dropped.
fn linearizable_key(ops: &[HOp]) -> bool {
    assert!(ops.len() <= 64);
    let all_completed: u64 = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| o.ret.is_some())
        .fold(0, |m, (i, _)| m | 1 << i);
    let mut memo: HashSet<(u64, i64)> = HashSet::new();
    fn dfs(ops: &[HOp], done: u64, state: i64, all: u64, memo: &mut HashSet<(u64, i64)>) -> bool {
        if done & all == all {
            return true;
        }
        if !memo.insert((done, state)) {
            return false;
        }
        let min_ret = ops
            .iter()
            .enumerate()
            .filter(|(i, o)| done >> i & 1 == 0 && o.ret.is_some())
            .map(|(_, o)| o.ret.unwrap())
            .min()
            .unwrap_or(u64::MAX);
        for (i, o) in ops.iter().enumerate() {
            if done >> i & 1 == 1 || o.invoke > min_ret {
                continue; // someone must finish before this op could even start: it cannot go first
            }
            let next = match o.put {
                Some(v) => v,
                None => {
                    if o.got.is_none() || o.got.unwrap().unwrap_or(-1) != state {
                        continue;
                    }
                    state
                }
            };
            if dfs(ops, done | 1 << i, next, all, memo) {
                return true;
            }
        }
        false
    }
    dfs(ops, 0, -1, all_completed, &mut memo)
}

fn linearizable(history: &BTreeMap<usize, HOp>) -> bool {
    let mut by_key: BTreeMap<u32, Vec<HOp>> = BTreeMap::new();
    for h in history.values() {
        if h.put.is_none() && h.ret.is_none() {
            continue; // an unfinished read constrains nothing
        }
        by_key.entry(h.key).or_default().push(h.clone());
    }
    by_key.values().all(|ops| linearizable_key(ops))
}

// ------------------------------------------------------------------------------------------------ driver

fn run(seed: u64, variant: Variant, plan: &Plan, text: bool) -> Report {
    let mut s = Sim::new(seed, variant, plan, text);
    s.run();
    let history_lin = linearizable(&s.history);
    let (completed, indeterminate) = (
        s.history.values().filter(|h| h.ret.is_some()).count(),
        s.history.values().filter(|h| h.ret.is_none()).count(),
    );
    Report {
        violation: s.violation.clone(),
        lin_ok: history_lin,
        liveness_ok: s.violation.is_some() || s.liveness(),
        hash: s.hash,
        trace: std::mem::take(&mut s.trace),
        completed,
        indeterminate,
        committed_entries: s.committed.len(),
        buggify_hits: s.buggify_hits,
        probe_latency: s.probe_latency(),
    }
}

/// Delta debugging: drop chunks of the plan while the run still fails (in any way, not only the way it first failed).
/// It ends only after a pass that removes no single event, so removing any one event from the result makes the run pass.
fn shrink(seed: u64, variant: Variant, mut plan: Plan) -> Plan {
    let mut chunk = plan.len() / 2;
    while chunk >= 1 {
        let mut i = 0;
        let mut progressed = false;
        while i < plan.len() {
            let mut cand = plan.clone();
            cand.drain(i..(i + chunk).min(plan.len()));
            if run(seed, variant, &cand, false).failed() {
                plan = cand;
                progressed = true;
            } else {
                i += chunk;
            }
        }
        // after a pass with progress, try the same size again: one removal can make another possible
        if !progressed {
            chunk /= 2;
        }
    }
    plan
}

fn describe(plan: &Plan) -> String {
    let ops = plan.iter().filter(|i| matches!(i.ext, Ext::Op(_))).count();
    let crashes = plan
        .iter()
        .filter(|i| matches!(i.ext, Ext::Crash { .. }))
        .count();
    let parts = plan
        .iter()
        .filter(|i| matches!(i.ext, Ext::Partition { .. }))
        .count();
    format!(
        "{} events ({ops} client ops, {crashes} crashes, {parts} partitions)",
        plan.len()
    )
}

fn hunt(variant: Variant, budget: u64) -> Option<u64> {
    let mut first = None;
    let mut failures = 0;
    for seed in 1..=budget {
        let plan = gen_plan(seed);
        let r = run(seed, variant, &plan, false);
        if r.failed() {
            failures += 1;
            first.get_or_insert(seed);
        }
    }
    println!("{variant:?}: {failures}/{budget} seeds fail");
    first
}

fn main() {
    // 1. determinism: same seed, same plan, byte-identical trace
    let plan = gen_plan(77);
    let (a, b) = (
        run(77, Variant::Correct, &plan, true),
        run(77, Variant::Correct, &plan, true),
    );
    assert_eq!(a.trace, b.trace, "same seed must replay byte for byte");
    assert_eq!(a.hash, b.hash);
    assert_ne!(run(78, Variant::Correct, &gen_plan(78), true).hash, a.hash);
    println!(
        "determinism: seed 77 replayed identically ({} trace lines, hash {:016x}); {} ops completed, {} indeterminate, {} entries committed",
        a.trace.len(),
        a.hash,
        a.completed,
        a.indeterminate,
        a.committed_entries
    );

    // 2. the correct system survives 2000 seeds of faults
    let (mut ops, mut ind, mut buggify, mut committed, mut worst_probe) = (0, 0, 0, 0, 0);
    for seed in 1..=2000 {
        let r = run(seed, Variant::Correct, &gen_plan(seed), false);
        assert!(
            !r.failed(),
            "CORRECT system failed at seed {seed}: {:?} lin_ok={}",
            r.violation,
            r.lin_ok
        );
        assert!(
            r.liveness_ok,
            "seed {seed}: cluster did not recover and converge after healing"
        );
        ops += r.completed;
        ind += r.indeterminate;
        buggify += r.buggify_hits;
        committed += r.committed_entries;
        worst_probe = worst_probe.max(
            r.probe_latency
                .expect("liveness_ok implies the probe completed"),
        );
    }
    println!(
        "correct Raft: 2000 seeds pass all invariants + linearizability + liveness ({ops} client ops completed, {ind} timed out, {committed} log entries committed, {buggify} buggify injections)"
    );
    println!(
        "  slowest post-heal probe: {worst_probe} ticks of the {} available before the simulation stops",
        T_STOP - PROBE_AT
    );
    assert!(ops > 0 && buggify > 0);

    // 2b. the history the checker reads must order every call and reply: many share a tick, none share an event number
    for seed in 1..=50 {
        let mut sim = Sim::new(seed, Variant::Correct, &gen_plan(seed), false);
        sim.run();
        let mut stamps = HashSet::new();
        for h in sim.history.values() {
            for stamp in [Some(h.invoke), h.ret].into_iter().flatten() {
                assert!(
                    stamps.insert(stamp),
                    "seed {seed}: two history events share stamp {stamp}"
                );
            }
        }
    }

    // 3. the injected bugs are found, replayed and shrunk
    for variant in [Variant::AckBeforeFsync, Variant::StaleLeaderRead] {
        let seed = hunt(variant, 2000).unwrap_or_else(|| panic!("{variant:?} was never caught"));
        let plan = gen_plan(seed);
        let r1 = run(seed, variant, &plan, true);
        let r2 = run(seed, variant, &plan, true);
        assert!(
            r1.failed() && r1.trace == r2.trace,
            "failing seed must replay identically"
        );
        let small = shrink(seed, variant, plan.clone());
        let rs = run(seed, variant, &small, false);
        assert!(rs.failed() && small.len() <= plan.len());
        for i in 0..small.len() {
            let mut without = small.clone();
            without.remove(i);
            assert!(
                !run(seed, variant, &without, false).failed(),
                "{variant:?} seed {seed}: the shrunk plan does not need event {i}"
            );
        }
        let what = r1
            .violation
            .clone()
            .unwrap_or_else(|| "history is NOT linearizable".into());
        println!("  minimal failing seed {seed}: {what}");
        println!(
            "  schedule shrunk from {} to {}; still fails: {}",
            describe(&plan),
            describe(&small),
            rs.violation
                .clone()
                .unwrap_or_else(|| "non-linearizable history".into())
        );
    }
}
