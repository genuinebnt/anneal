//! RECIPE 402 (distributed): atomic commit: two-phase commit with write-ahead logs, crash recovery, blocking, presumed abort/commit, and 3PC
//! LEVEL: advanced
//! Use in: understanding XA / distributed database transactions, and why modern systems layer 2PC over Paxos/Raft or use sagas instead.   Run: cargo run --release --example two_phase_commit
#![allow(dead_code)]
//!
//! WHAT: ATOMIC COMMIT means every participant of a transaction commits or every one aborts. Two-phase commit (2PC):
//! phase 1 the coordinator asks everyone to PREPARE; a participant that can commit force-writes a PREPARED record to its
//! write-ahead log (WAL) and votes YES, which is a PROMISE it can no longer abort on its own; any NO (or silence) means
//! abort. Phase 2: the coordinator force-writes the decision (the COMMIT POINT) and tells everyone; participants
//! force-write it, apply it, acknowledge. The sim runs coordinator + 3 participants on a deterministic event queue
//! (seeded xorshift, lossy/duplicating network) with CRASH POINTS between every protocol step.
//! WHY the logs: a crash must never change a promise. A participant that crashed after voting YES comes back IN DOUBT: it
//! must keep the locks and ask (coordinator first, then peers); it may not guess. A coordinator that crashed before logging
//! a decision may abort on restart; one that logged COMMIT must re-send it until acknowledged.
//! BLOCKING: if the coordinator dies after the participants voted YES and before they learn the outcome, they cannot decide
//! alone: committing may contradict a (lost) abort, aborting may contradict a commit that another participant already
//! applied. They sit holding locks until the coordinator returns. Cooperative termination (ask the other participants)
//! only helps if somebody already knows the outcome or never voted; the demo shows both cases.
//! OPTIMISATIONS (log forces are the expensive part; the sim counts real forces and messages):
//!  * PRESUMED ABORT: no information about a transaction means "aborted". So aborts need no forced record at the
//!    coordinator, no acks and no participant force; abort is nearly free. Commits cost the same as basic 2PC.
//!  * PRESUMED COMMIT: no information means "committed". The coordinator force-writes a COLLECTING record before sending
//!    PREPARE (so a crash can be told apart from "committed and forgotten"), but commits need no acks and no forced commit
//!    record at participants. Commit is cheaper, abort slightly dearer. Used when most transactions commit.
//! 3PC (Skeen): adds a PRE-COMMIT phase so no participant can be in a state adjacent to both commit and abort; with timeouts
//! (uncertain => abort, pre-committed => commit) and a termination protocol it does not block when processes crash-stop
//! and the network is reliable. But a PARTITION lets the two sides take opposite decisions: it trades blocking for
//! atomicity violations, which is why real systems prefer Paxos Commit (consensus on the decision) or Spanner-style 2PC
//! over Paxos groups, where the coordinator state itself is replicated.
//! SAGAS (examples/distributed/saga_orchestrator.rs) drop atomicity and isolation: each step commits locally at once and
//! failures are undone by compensating actions, so nothing is ever locked across services (no blocking) but intermediate
//! states are visible. Use 2PC inside one database/cluster, sagas across independent services.
//! VERIFICATION: crash EVERY point of EVERY role (each of the three flavours, all-YES and one-NO), then 3000 random
//! schedules with loss, duplication and multiple crashes: atomicity is checked after every event and, once the system is
//! calm and everyone restarted, every participant must have decided.
//! COMMON USES: XA transactions (Java JTA, PostgreSQL PREPARE TRANSACTION, MySQL XA), Microsoft DTC, Google Spanner
//! and CockroachDB (2PC over replicated groups), Percolator-style systems, Kafka transactions (a coordinator log).
//! Read: Bernstein, Hadzilacos and Goodman, "Concurrency Control and Recovery in Database Systems", chapter 7
//! https://www.microsoft.com/en-us/research/people/philbe/book/ ; Gray and Lamport, "Consensus on Transaction Commit"
//! https://www.microsoft.com/en-us/research/publication/consensus-on-transaction-commit/ ; Mohan, Lindsay and Obermarck,
//! "Transaction Management in the R* Distributed Database Management System" (presumed abort/commit, ACM TODS 1986).
use std::collections::{BTreeMap, BTreeSet};

const NP: usize = 3; // participants are nodes 1..=NP, the coordinator is node 0
const VOTE_TIMEOUT: u64 = 400;
const RETRY: u64 = 100;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        (0..4).for_each(|_| {
            r.next();
        });
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
        (self.next() >> 11) % n
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Flavour {
    Basic,
    PresumedAbort,
    PresumedCommit,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dec {
    Commit,
    Abort,
}

impl Flavour {
    /// Does the decision need to be force-written at the COORDINATOR?
    fn coord_forces(self, d: Dec) -> bool {
        !(self == Flavour::PresumedAbort && d == Dec::Abort)
    }
    /// Does the PARTICIPANT force its decision record (the other case is a lazy write)?
    fn part_forces(self, d: Dec) -> bool {
        match self {
            Flavour::Basic => true,
            Flavour::PresumedAbort => d == Dec::Commit,
            Flavour::PresumedCommit => d == Dec::Abort,
        }
    }
    /// Is an acknowledgement needed before the coordinator may forget the transaction?
    fn needs_ack(self, d: Dec) -> bool {
        self.part_forces(d)
    }
    /// What the coordinator answers when it has no memory of the transaction.
    fn presumption(self) -> Dec {
        if self == Flavour::PresumedCommit {
            Dec::Commit
        } else {
            Dec::Abort
        }
    }
}

/// Durable write-ahead log. `force` is a synchronous disk write (the cost we count), `lazy` is piggy-backed.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Rec {
    Collecting,
    Prepared,
    Decision(Dec),
    End,
}
#[derive(Default)]
struct Log {
    recs: Vec<Rec>,
    forced: u32,
    lazy: u32,
}
impl Log {
    fn force(&mut self, r: Rec) {
        self.recs.push(r);
        self.forced += 1;
    }
    fn lazy(&mut self, r: Rec) {
        self.recs.push(r);
        self.lazy += 1;
    }
    fn decision(&self) -> Option<Dec> {
        self.recs.iter().find_map(|r| {
            if let Rec::Decision(d) = r {
                Some(*d)
            } else {
                None
            }
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum Msg {
    Prepare,
    Vote(bool),
    Decide(Dec),
    Ack,
    Inquiry,
    InquiryReply(Option<Dec>),
    PeerQuery,
    PeerReply(Option<Dec>),
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum CPhase {
    Idle,
    Collecting,
    Deciding(Dec),
    Done,
}
struct Coord {
    up: bool,
    log: Log,
    phase: CPhase,
    votes: BTreeMap<usize, bool>,
    acked: BTreeSet<usize>,
    epoch: u64,
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum PState {
    Idle,
    Prepared,
    Committed,
    Aborted,
}
struct Part {
    up: bool,
    log: Log,
    can_commit: bool,
    state: PState,
    epoch: u64,
    doubt_since: Option<u64>,
    in_doubt: u64,
}

enum Ev {
    Msg { from: usize, to: usize, m: Msg },
    Timer { node: usize, epoch: u64 },
    Restart(usize),
    Begin,
}

struct Net {
    drop: u64,
    dup: u64,
}

struct Sim {
    now: u64,
    seq: u64,
    q: BTreeMap<(u64, u64), Ev>,
    rng: Rng,
    net: Net,
    flavour: Flavour,
    coop: bool,
    c: Coord,
    ps: Vec<Part>,
    pass: [usize; NP + 1],
    plan_nth: Vec<(usize, usize)>,
    plan_named: Vec<(usize, &'static str)>,
    restart_after: Option<u64>,
    crashes: Vec<String>,
    sent: u64,
    trace: bool,
}

macro_rules! say {
    ($s:expr, $($a:tt)*) => { if $s.trace { println!("  [{:>5}ms] {}", $s.now, format!($($a)*)); } };
}

impl Sim {
    fn new(seed: u64, flavour: Flavour, votes: [bool; NP]) -> Sim {
        let c = Coord {
            up: true,
            log: Log::default(),
            phase: CPhase::Idle,
            votes: BTreeMap::new(),
            acked: BTreeSet::new(),
            epoch: 0,
        };
        let ps = votes
            .iter()
            .map(|&v| Part {
                up: true,
                log: Log::default(),
                can_commit: v,
                state: PState::Idle,
                epoch: 0,
                doubt_since: None,
                in_doubt: 0,
            })
            .collect();
        Sim {
            now: 0,
            seq: 0,
            q: BTreeMap::new(),
            rng: Rng::new(seed),
            net: Net { drop: 0, dup: 0 },
            flavour,
            coop: true,
            c,
            ps,
            pass: [0; NP + 1],
            plan_nth: vec![],
            plan_named: vec![],
            restart_after: Some(300),
            crashes: vec![],
            sent: 0,
            trace: false,
        }
    }

    fn at(&mut self, t: u64, e: Ev) {
        self.seq += 1;
        self.q.insert((t, self.seq), e);
    }

    fn send(&mut self, from: usize, to: usize, m: Msg) {
        self.sent += 1;
        if self.rng.chance(self.net.drop) {
            return;
        }
        for _ in 0..1 + self.rng.chance(self.net.dup) as usize {
            let d = 5 + self.rng.below(15);
            self.at(self.now + d, Ev::Msg { from, to, m });
        }
    }

    fn timer(&mut self, node: usize, after: u64) {
        let epoch = if node == 0 {
            self.c.epoch
        } else {
            self.ps[node - 1].epoch
        };
        self.at(self.now + after, Ev::Timer { node, epoch });
    }

    // ---- crash points: every protocol step passes through `fp`; a crash plan can kill the node exactly there

    fn fp(&mut self, node: usize, name: String) -> bool {
        let nth = self.pass[node];
        self.pass[node] += 1;
        let named = self
            .plan_named
            .iter()
            .position(|&(n, p)| n == node && name.starts_with(p));
        let hit = named.is_some() || self.plan_nth.contains(&(node, nth));
        if let Some(i) = named {
            self.plan_named.remove(i);
        }
        if hit {
            self.crash(node, name);
        }
        hit
    }

    fn crash(&mut self, node: usize, why: String) {
        say!(self, "*** node {node} CRASHES at: {why}");
        self.crashes.push(format!(
            "{} @ {why}",
            if node == 0 {
                "coordinator".to_string()
            } else {
                format!("participant {node}")
            }
        ));
        if node == 0 {
            self.c.up = false;
            self.c.epoch += 1;
            (self.c.votes.clear(), self.c.acked.clear());
        } else {
            let p = &mut self.ps[node - 1];
            p.up = false;
            p.epoch += 1;
            p.state = PState::Idle; // volatile state is lost; recovery rebuilds it from the log (doubt_since survives: locks are logged)
        }
        if let Some(d) = self.restart_after {
            self.at(self.now + d, Ev::Restart(node));
        }
    }

    fn is_up(&self, node: usize) -> bool {
        if node == 0 {
            self.c.up
        } else {
            self.ps[node - 1].up
        }
    }

    // ---- coordinator

    fn c_begin(&mut self) {
        if self.fp(0, "begin transaction".into()) {
            return;
        }
        if self.flavour == Flavour::PresumedCommit {
            self.c.log.force(Rec::Collecting); // so that a crash is distinguishable from "committed and forgotten"
            if self.fp(0, "COLLECTING record forced".into()) {
                return;
            }
        }
        self.c.phase = CPhase::Collecting;
        self.c.epoch += 1;
        for p in 1..=NP {
            self.send(0, p, Msg::Prepare);
            if self.fp(0, format!("PREPARE sent to {p} of {NP}")) {
                return;
            }
        }
        self.timer(0, VOTE_TIMEOUT);
    }

    fn c_on_vote(&mut self, from: usize, yes: bool) {
        if self.c.phase != CPhase::Collecting {
            return;
        }
        self.c.votes.insert(from, yes);
        if !yes {
            self.c_decide(Dec::Abort);
        } else if self.c.votes.len() == NP && self.c.votes.values().all(|&v| v) {
            self.c_decide(Dec::Commit);
        }
    }

    /// The decision becomes durable (the commit point) BEFORE anyone is told.
    fn c_decide(&mut self, d: Dec) {
        if self.fp(0, format!("all votes in, before logging {d:?}")) {
            return;
        }
        // Presumed abort: an abort is not logged at all; a crash simply forgets it and the presumption answers for it.
        if self.flavour.coord_forces(d) {
            self.c.log.force(Rec::Decision(d));
        }
        say!(self, "coordinator decides {d:?}");
        self.c.phase = CPhase::Deciding(d);
        self.c.acked.clear();
        if self.fp(0, format!("{d:?} logged, nothing sent")) {
            return;
        }
        self.c_send_decision(d);
    }

    fn c_targets(&self) -> Vec<usize> {
        (1..=NP)
            .filter(|p| self.c.votes.get(p) != Some(&false))
            .collect()
    }

    fn c_send_decision(&mut self, d: Dec) {
        let targets: Vec<usize> = self
            .c_targets()
            .into_iter()
            .filter(|p| !self.c.acked.contains(p))
            .collect();
        for (k, p) in targets.iter().enumerate() {
            self.send(0, *p, Msg::Decide(d));
            if self.fp(0, format!("{d:?} sent to {} of {}", k + 1, targets.len())) {
                return;
            }
        }
        if self.flavour.needs_ack(d) {
            self.c.epoch += 1;
            self.timer(0, RETRY);
        } else {
            self.c.log.lazy(Rec::End);
            self.c.phase = CPhase::Done; // presumption covers every future inquiry: forget immediately
        }
    }

    fn c_on_ack(&mut self, from: usize) {
        let CPhase::Deciding(_) = self.c.phase else {
            return;
        };
        self.c.acked.insert(from);
        if self.c_targets().iter().all(|p| self.c.acked.contains(p)) {
            if self.fp(0, "all acks in, before END record".into()) {
                return;
            }
            self.c.log.lazy(Rec::End);
            self.c.phase = CPhase::Done;
            say!(
                self,
                "coordinator: all acknowledged, END written, transaction forgotten"
            );
        }
    }

    fn c_on_inquiry(&mut self, from: usize) {
        let answer = match self.c.phase {
            CPhase::Collecting => None, // still undecided: "wait"
            CPhase::Deciding(d) => Some(d),
            CPhase::Done | CPhase::Idle => {
                Some(self.c.log.decision().unwrap_or(self.flavour.presumption()))
            }
        };
        self.send(0, from, Msg::InquiryReply(answer));
    }

    fn c_on_timer(&mut self) {
        match self.c.phase {
            CPhase::Collecting => self.c_decide(Dec::Abort), // a silent participant counts as NO
            CPhase::Deciding(d) => self.c_send_decision(d),
            _ => {}
        }
    }

    /// Recovery rules: logged decision => re-send it; PC collecting record without decision => abort; otherwise nothing.
    fn c_recover(&mut self) {
        self.c.up = true;
        self.c.phase = CPhase::Idle;
        let recs = self.c.log.recs.clone();
        let ended = recs.contains(&Rec::End);
        match (self.c.log.decision(), recs.contains(&Rec::Collecting)) {
            (Some(d), _) if !ended => {
                say!(
                    self,
                    "coordinator restarts: log holds {d:?} without END, re-sends it"
                );
                self.c.phase = CPhase::Deciding(d);
                self.c_send_decision(d);
            }
            (None, true) => {
                say!(
                    self,
                    "coordinator restarts: COLLECTING without decision, aborts"
                );
                self.c_decide(Dec::Abort);
            }
            _ => say!(
                self,
                "coordinator restarts: no undecided transaction in the log (presumption: {:?})",
                self.flavour.presumption()
            ),
        }
    }

    // ---- participants

    fn p_set(&mut self, p: usize, st: PState) {
        let now = self.now;
        let part = &mut self.ps[p - 1];
        match (st, part.doubt_since) {
            (PState::Prepared, None) => part.doubt_since = Some(now),
            (PState::Prepared, Some(_)) => {}
            (_, Some(t)) => {
                part.in_doubt += now - t;
                part.doubt_since = None;
            }
            (_, None) => {}
        }
        part.state = st;
    }

    fn p_on_prepare(&mut self, p: usize) {
        match self.ps[p - 1].state {
            PState::Prepared => return self.send(p, 0, Msg::Vote(true)),
            PState::Aborted | PState::Committed => return self.send(p, 0, Msg::Vote(false)),
            PState::Idle => {}
        }
        if self.fp(p, "PREPARE received, before logging".into()) {
            return;
        }
        if self.ps[p - 1].can_commit {
            self.ps[p - 1].log.force(Rec::Prepared); // the promise is durable BEFORE the YES leaves
            if self.fp(p, "PREPARED forced, vote not sent".into()) {
                return;
            }
            self.p_set(p, PState::Prepared);
            say!(
                self,
                "participant {p} votes YES and is now IN DOUBT (holds locks)"
            );
            self.send(p, 0, Msg::Vote(true));
            self.ps[p - 1].epoch += 1;
            self.timer(p, RETRY * 2);
            self.fp(p, "YES vote sent".into());
        } else {
            self.ps[p - 1].log.lazy(Rec::Decision(Dec::Abort)); // a NO needs no force: unilateral abort is always safe
            self.p_set(p, PState::Aborted);
            say!(self, "participant {p} votes NO and aborts unilaterally");
            self.send(p, 0, Msg::Vote(false));
        }
    }

    fn p_on_decision(&mut self, p: usize, d: Dec) {
        let st = self.ps[p - 1].state;
        let target = if d == Dec::Commit {
            PState::Committed
        } else {
            PState::Aborted
        };
        if st == PState::Prepared || (st == PState::Idle && d == Dec::Abort) {
            if self.fp(p, format!("{d:?} received, before logging")) {
                return;
            }
            if self.flavour.part_forces(d) {
                self.ps[p - 1].log.force(Rec::Decision(d));
            } else {
                self.ps[p - 1].log.lazy(Rec::Decision(d));
            }
            if self.fp(p, format!("{d:?} logged, ack not sent")) {
                return;
            }
            self.p_set(p, target);
            say!(self, "participant {p} applies {d:?} and releases its locks");
        } else if st != target {
            panic!("ATOMICITY: participant {p} in {st:?} told {d:?}");
        }
        if self.flavour.needs_ack(d) {
            self.send(p, 0, Msg::Ack);
        }
    }

    fn p_on_timer(&mut self, p: usize) {
        if self.ps[p - 1].state != PState::Prepared {
            return;
        }
        self.send(p, 0, Msg::Inquiry);
        if self.coop {
            for q in (1..=NP).filter(|&q| q != p) {
                self.send(p, q, Msg::PeerQuery);
            }
        }
        self.timer(p, RETRY);
    }

    /// Cooperative termination, answering side. A peer that never voted YES may safely abort and say so.
    fn p_on_peer_query(&mut self, p: usize, from: usize) {
        let ans = match self.ps[p - 1].state {
            PState::Committed => Some(Dec::Commit),
            PState::Aborted => Some(Dec::Abort),
            PState::Prepared => None,
            PState::Idle => {
                self.ps[p - 1].log.lazy(Rec::Decision(Dec::Abort));
                self.p_set(p, PState::Aborted);
                Some(Dec::Abort)
            }
        };
        self.send(p, from, Msg::PeerReply(ans));
    }

    fn p_recover(&mut self, p: usize) {
        let part = &mut self.ps[p - 1];
        part.up = true;
        let st = match (part.log.decision(), part.log.recs.contains(&Rec::Prepared)) {
            (Some(Dec::Commit), _) => PState::Committed,
            (Some(Dec::Abort), _) => PState::Aborted,
            (None, true) => PState::Prepared, // voted YES, outcome unknown: IN DOUBT, must ask
            (None, false) => PState::Idle,
        };
        say!(self, "participant {p} restarts from its log: {st:?}");
        self.p_set(p, st);
        if st == PState::Prepared {
            self.ps[p - 1].epoch += 1;
            self.timer(p, RETRY);
        }
    }

    // ---- driver

    fn atomic(&self) -> bool {
        let has = |d: Dec, st: PState| {
            self.ps
                .iter()
                .any(|p| p.state == st || p.log.decision() == Some(d))
        };
        !(has(Dec::Commit, PState::Committed) && has(Dec::Abort, PState::Aborted))
    }

    fn step(&mut self, limit: u64) -> bool {
        let Some(&(t, _)) = self.q.keys().next() else {
            return false;
        };
        if t > limit {
            return false;
        }
        let (_, ev) = self.q.pop_first().unwrap();
        self.now = t;
        match ev {
            Ev::Begin => self.c_begin(),
            Ev::Restart(0) => {
                if !self.c.up {
                    self.c_recover()
                }
            }
            Ev::Restart(p) => {
                if !self.ps[p - 1].up {
                    self.p_recover(p)
                }
            }
            Ev::Timer { node: 0, epoch } => {
                if self.c.up && epoch == self.c.epoch {
                    self.c_on_timer()
                }
            }
            Ev::Timer { node, epoch } => {
                if self.ps[node - 1].up && epoch == self.ps[node - 1].epoch {
                    self.p_on_timer(node)
                }
            }
            Ev::Msg { from, to, m } if self.is_up(to) => self.deliver(from, to, m),
            Ev::Msg { .. } => {}
        }
        assert!(
            self.atomic(),
            "ATOMICITY VIOLATED at {}ms: {:?}",
            self.now,
            self.ps.iter().map(|p| p.state).collect::<Vec<_>>()
        );
        true
    }

    fn deliver(&mut self, from: usize, to: usize, m: Msg) {
        if to == 0 {
            match m {
                Msg::Vote(y) => self.c_on_vote(from, y),
                Msg::Ack => self.c_on_ack(from),
                Msg::Inquiry => self.c_on_inquiry(from),
                _ => {}
            }
            return;
        }
        match m {
            Msg::Prepare => self.p_on_prepare(to),
            Msg::Decide(d) | Msg::InquiryReply(Some(d)) | Msg::PeerReply(Some(d)) => {
                self.p_on_decision(to, d)
            }
            Msg::PeerQuery => self.p_on_peer_query(to, from),
            _ => {}
        }
    }

    fn run(&mut self, limit: u64) {
        while self.step(limit) {}
        self.now = self.now.max(limit);
    }

    /// Run one transaction from t=0.
    fn go(&mut self, limit: u64) {
        self.at(0, Ev::Begin);
        self.run(limit);
    }

    fn in_doubt_ms(&self, p: usize) -> u64 {
        let part = &self.ps[p - 1];
        part.in_doubt + part.doubt_since.map_or(0, |t| self.now - t)
    }

    fn states(&self) -> Vec<PState> {
        self.ps.iter().map(|p| p.state).collect()
    }

    fn outcome(&self) -> &'static str {
        let s = self.states();
        if s.contains(&PState::Prepared) {
            "IN DOUBT"
        } else if s.contains(&PState::Committed) {
            "COMMIT"
        } else {
            "ABORT"
        }
    }

    fn forced_total(&self) -> (u32, u32, u32) {
        let f = self.c.log.forced + self.ps.iter().map(|p| p.log.forced).sum::<u32>();
        let l = self.c.log.lazy + self.ps.iter().map(|p| p.log.lazy).sum::<u32>();
        (f, self.c.log.forced, l)
    }

    /// Nobody is in doubt (a participant that never voted YES holds no locks and needs no outcome).
    fn decided(&self) -> bool {
        self.ps.iter().all(|p| p.state != PState::Prepared) && self.ps.iter().all(|p| p.up)
    }
}

// ---------------------------------------------------------------- experiments

fn cost_table() {
    println!(
        "-- failure-free cost per transaction ({NP} participants): forced log writes and messages"
    );
    println!(
        "   {:<15} {:<7} {:>12} {:>14} {:>9}",
        "protocol", "outcome", "forced(coord)", "forced(total)", "messages"
    );
    let expect = [
        (Flavour::Basic, false, 7, 1, 12),
        (Flavour::PresumedAbort, false, 7, 1, 12),
        (Flavour::PresumedCommit, false, 5, 2, 9),
        (Flavour::Basic, true, 5, 1, 10),
        (Flavour::PresumedAbort, true, 2, 0, 8),
        (Flavour::PresumedCommit, true, 6, 2, 10),
    ];
    for (fl, abort, total, coord, msgs) in expect {
        let votes = [true, true, !abort];
        let mut s = Sim::new(1, fl, votes);
        s.go(2000);
        let (f, fc, _) = s.forced_total();
        println!(
            "   {:<15} {:<7} {:>12} {:>14} {:>9}",
            format!("{fl:?}"),
            if abort { "ABORT" } else { "COMMIT" },
            fc,
            f,
            s.sent
        );
        assert_eq!(s.outcome(), if abort { "ABORT" } else { "COMMIT" });
        assert_eq!(
            (f, fc, s.sent),
            (total, coord, msgs),
            "{fl:?} abort={abort}"
        );
    }
    println!(
        "   (presumed abort makes aborts almost free; presumed commit saves 2 forces and {NP} acks on commit)"
    );
}

/// Crash every node at every protocol step, restart it 300ms later, and demand atomicity and termination.
fn crash_everywhere() {
    println!(
        "-- crash EVERY step: each node, each crash point, restart after 300ms (calm network)"
    );
    let (mut scenarios, mut commits, mut aborts) = (0, 0, 0);
    for fl in [
        Flavour::Basic,
        Flavour::PresumedAbort,
        Flavour::PresumedCommit,
    ] {
        for votes in [[true; NP], [true, false, true]] {
            for node in 0..=NP {
                for nth in 0.. {
                    let mut s = Sim::new(7, fl, votes);
                    s.plan_nth = vec![(node, nth)];
                    s.go(4000);
                    if s.crashes.is_empty() {
                        break;
                    } // the node never reached its nth step: all points covered
                    scenarios += 1;
                    assert!(
                        s.decided(),
                        "{fl:?} {votes:?} {:?}: participants stuck {:?}",
                        s.crashes,
                        s.states()
                    );
                    match s.outcome() {
                        "COMMIT" => commits += 1,
                        _ => aborts += 1,
                    }
                    if fl == Flavour::Basic && votes == [true; NP] {
                        println!(
                            "   {:<48} -> {:<6} (max in doubt {} ms)",
                            s.crashes[0],
                            s.outcome(),
                            (1..=NP).map(|p| s.in_doubt_ms(p)).max().unwrap()
                        );
                    }
                }
            }
        }
    }
    println!(
        "   {scenarios} crash scenarios over 3 protocols x 2 vote patterns: all atomic and all terminated ({commits} commit, {aborts} abort)"
    );
}

fn blocking_demo() {
    println!(
        "-- BLOCKING: coordinator crashes after all participants voted YES, before any decision, and stays down"
    );
    let mut s = Sim::new(3, Flavour::Basic, [true; NP]);
    s.trace = true;
    s.plan_named = vec![(0, "all votes in, before logging")];
    s.restart_after = None;
    s.go(5000);
    assert_eq!(s.outcome(), "IN DOUBT");
    assert!(
        s.states().iter().all(|&x| x == PState::Prepared),
        "all three are stuck holding locks"
    );
    let stuck: Vec<u64> = (1..=NP).map(|p| s.in_doubt_ms(p)).collect();
    println!(
        "   after 5000 ms: states {:?}, in doubt for {stuck:?} ms; cooperative termination cannot help (every peer is uncertain)",
        s.states()
    );
    assert!(stuck.iter().all(|&t| t > 4500));
    println!("   coordinator finally restarts:");
    s.at(s.now + 1, Ev::Restart(0));
    s.run(s.now + 1000);
    assert_eq!(s.outcome(), "ABORT");
    println!("   -> all participants resolved: {:?}", s.states());

    println!(
        "-- cooperative termination: coordinator dies after telling ONLY participant 1 to COMMIT (never restarts)"
    );
    for coop in [false, true] {
        let mut s = Sim::new(3, Flavour::Basic, [true; NP]);
        s.trace = coop;
        s.coop = coop;
        s.plan_named = vec![(0, "Commit sent to 1 of")];
        s.restart_after = None;
        s.go(3000);
        println!(
            "   cooperative termination {}: {:?} after 3000 ms",
            if coop { "ON " } else { "OFF" },
            s.states()
        );
        if coop {
            assert_eq!(s.outcome(), "COMMIT");
        } else {
            assert_eq!(s.outcome(), "IN DOUBT");
        }
    }
}

/// Random schedules: loss, duplication, several crashes at random protocol steps, then calm and full recovery.
fn random_schedules() {
    println!(
        "-- 3000 random schedules: 0-20% loss, 5% duplication, 0-3 crashes at random steps, random votes and protocol"
    );
    let (mut commit, mut abort, mut crashes, mut msgs, mut max_doubt) = (0, 0, 0, 0u64, 0u64);
    for seed in 0..3000u64 {
        let mut r = Rng::new(seed ^ 0xC0FFEE);
        let fl = [
            Flavour::Basic,
            Flavour::PresumedAbort,
            Flavour::PresumedCommit,
        ][(seed % 3) as usize];
        let votes: [bool; NP] = std::array::from_fn(|_| r.chance(85));
        let mut s = Sim::new(seed, fl, votes);
        s.net = Net {
            drop: r.below(21),
            dup: 5,
        };
        s.restart_after = Some(30 + r.below(500));
        s.plan_nth = (0..r.below(4))
            .map(|_| (r.below(NP as u64 + 1) as usize, r.below(7) as usize))
            .collect();
        s.at(3000, Ev::Restart(0)); // belt and braces: everyone is restarted and the network calms down by 3 s
        for p in 1..=NP {
            s.at(3000, Ev::Restart(p));
        }
        s.go(3000);
        s.net = Net { drop: 0, dup: 0 };
        s.run(15_000);
        assert!(
            s.decided(),
            "seed {seed} {fl:?} {votes:?} {:?}: stuck {:?}",
            s.crashes,
            s.states()
        );
        assert!(s.atomic());
        crashes += s.crashes.len();
        msgs += s.sent;
        max_doubt = max_doubt.max((1..=NP).map(|p| s.in_doubt_ms(p)).max().unwrap());
        if s.outcome() == "COMMIT" {
            commit += 1
        } else {
            abort += 1
        }
    }
    println!(
        "   3000/3000 atomic and terminated: {commit} committed, {abort} aborted, {crashes} crashes injected, {msgs} messages, longest in-doubt window {max_doubt} ms"
    );
}

// ================================================================ 3PC

#[derive(Clone, Copy, PartialEq, Debug)]
enum S3 {
    Init,
    Uncertain,
    PreCommitted,
    Committed,
    Aborted,
}

/// The termination protocol among the participants of ONE connected group that lost the coordinator:
/// any Committed => commit, any Aborted => abort, any PreCommitted => commit (after re-sending pre-commit), else abort.
fn terminate(group: &[usize], st: &mut [S3]) -> Dec {
    let d = if group.iter().any(|&i| st[i] == S3::Committed) {
        Dec::Commit
    } else if group.iter().any(|&i| st[i] == S3::Aborted) {
        Dec::Abort
    } else if group.iter().any(|&i| st[i] == S3::PreCommitted) {
        Dec::Commit
    } else {
        Dec::Abort
    };
    for &i in group {
        st[i] = if d == Dec::Commit {
            S3::Committed
        } else {
            S3::Aborted
        };
    }
    d
}

/// The coordinator's sends in order: [CanCommit x N] then ([PreCommit x N] [DoCommit x N]) or (Abort x N) after a NO.
/// It crash-stops after `crash_after` sends; each send moves one participant to its new state.
fn three_pc(votes: [bool; NP], crash_after: usize) -> [S3; NP] {
    let mut plan: Vec<(usize, S3)> = (0..NP)
        .map(|i| (i, if votes[i] { S3::Uncertain } else { S3::Aborted }))
        .collect();
    if votes.iter().all(|&v| v) {
        plan.extend((0..NP).map(|i| (i, S3::PreCommitted)));
        plan.extend((0..NP).map(|i| (i, S3::Committed)));
    } else {
        plan.extend((0..NP).map(|i| (i, S3::Aborted)));
    }
    let mut st = [S3::Init; NP];
    for &(i, new) in plan.iter().take(crash_after) {
        st[i] = new;
    }
    st
}

fn three_pc_part() {
    println!(
        "-- 3PC under CRASH-STOP of the coordinator (reliable network): crash after every possible send"
    );
    let mut checked = 0;
    for votes in [[true; NP], [true, false, true]] {
        for k in 0..=3 * NP {
            let mut st = three_pc(votes, k);
            let group: Vec<usize> = (0..NP).collect();
            terminate(&group, &mut st); // the live participants run the termination protocol: no waiting for the coordinator
            assert!(
                st.iter()
                    .all(|&x| x == st[0] && matches!(x, S3::Committed | S3::Aborted)),
                "votes {votes:?} crash {k}: {st:?}"
            );
            checked += 1;
        }
    }
    println!(
        "   {checked} crash points: participants always reached the same decision WITHOUT the coordinator (2PC blocked here)"
    );

    println!(
        "-- 3PC under a PARTITION: coordinator + participant 1 got PRE-COMMIT, participants 2 and 3 are cut off"
    );
    let mut st = [S3::PreCommitted, S3::Uncertain, S3::Uncertain];
    let a = terminate(&[0], &mut st); // side A: sees PreCommitted, commits
    let b = terminate(&[1, 2], &mut st); // side B: all Uncertain, aborts (it cannot know A exists)
    println!("   side A decides {a:?}, side B decides {b:?}: final states {st:?}");
    assert!(
        a == Dec::Commit && b == Dec::Abort,
        "the timeout rules split the decision"
    );
    println!(
        "   => ATOMICITY VIOLATED. 3PC needs a reliable failure detector; with partitions use consensus (Paxos Commit) instead."
    );
}

fn main() {
    println!("== 2PC: cost of each flavour ==");
    cost_table();
    println!("\n== 2PC: crash at every step ==");
    crash_everywhere();
    println!("\n== 2PC: blocking and cooperative termination ==");
    blocking_demo();
    println!("\n== 2PC: random failure schedules ==");
    random_schedules();
    println!("\n== 3PC ==");
    three_pc_part();
}
