//! RECIPE 401 (distributed): Paxos: single-decree safety under chaos, dueling-proposer livelock, and Multi-Paxos replicated log
//! LEVEL: advanced
//! Use in: understanding the consensus core behind Chubby, Spanner, Cassandra lightweight transactions, Raft and most replicated logs.   Run: cargo run --release --example paxos_basic_and_multi
#![allow(dead_code)]
//!
//! WHAT: PART 1, single-decree Paxos: proposers, acceptors (persistent `promised` and `accepted`) and learners exchanging
//! Prepare/Promise/Accept/Accepted over a simulated network that drops, duplicates, delays and reorders messages.
//! PART 2, Multi-Paxos: one stable leader runs phase 1 ONCE for every future log slot, then each client command costs a
//! single round trip (phase 2). A new leader recovers whatever may have been chosen, fills holes with NO-OPs, and
//! replicas end with identical logs. Both parts run inside one deterministic event queue with a seeded xorshift RNG.
//! WHY: Paxos is the proof that agreement is possible with crash faults, lost messages and no timing assumptions for SAFETY.
//! HOW (single decree): a ballot is (round, proposer id), totally ordered and unique per proposer. Phase 1: the proposer
//! asks a majority to PROMISE not to accept lower ballots and to report what they already accepted. If any acceptor reports
//! an accepted value, the proposer MUST propose the value with the highest accepted ballot (its own value is only used if
//! nobody reports one): this single rule is the heart of Paxos. Phase 2: it asks the acceptors to accept; a value is CHOSEN
//! when a majority accepted the same (ballot, value). Why safe: two majorities intersect, so a later ballot's phase 1 always
//! sees the chosen value and re-proposes it. The acceptor's state must be on stable storage before replying: an acceptor
//! with amnesia can promise twice or forget an accepted value, and then two values can be chosen (demonstrated below). A proposer's
//! round counter is persisted too, so a ballot is never reused.
//! LIVENESS: safety never depends on timing, progress does. Two proposers can duel forever: A prepares at b1, B prepares at
//! b2 > b1 (A's accept is rejected), A prepares at b3 (B's accept is rejected), and so on. Fixes: randomised exponential
//! back-off, or a distinguished proposer (leader) so duels are rare.
//! FLP: Fischer, Lynch and Paterson proved (1985) that no deterministic protocol can guarantee consensus in an asynchronous
//! system if even one process may crash, because a slow process cannot be told apart from a dead one, so an adversarial
//! schedule can keep the system undecided forever. Paxos does not contradict this: it sacrifices guaranteed termination, never
//! agreement. Dueling proposers are the schedule FLP says must exist; timeouts and randomness buy LIVENESS only.
//! PAXOS vs RAFT: Raft is Multi-Paxos with extra constraints that make it easier to reason about. Raft logs have no holes
//! (an entry is accepted only after its predecessor matches) and the leader must have the most up-to-date log, so data
//! flows only leader to follower and there is no gap filling. Paxos accepts slots independently and out of order and any
//! replica can win phase 1, then must RECOVER values from the promises and plug holes with no-ops. Paxos leaders may commit
//! older-ballot values by re-proposing them with their own ballot; Raft instead commits older entries indirectly.
//! COMMON USES: Google Chubby and Spanner (Multi-Paxos per tablet), Apache Cassandra lightweight transactions (single-decree
//! Paxos per key), Microsoft Azure Storage and Autopilot, Amazon DynamoDB leader election, Apache ZooKeeper's ZAB (a close
//! cousin), Raft (etcd, Consul) as the simplified descendant, and Mencius/EPaxos as variants.
//! See also examples/distributed/raft_consensus.rs (Multi-Paxos without holes) and saga_orchestrator.rs (availability over agreement).
//! Read: Lamport, "Paxos Made Simple" https://lamport.azurewebsites.net/pubs/paxos-simple.pdf ; Fischer, Lynch and Paterson,
//! "Impossibility of Distributed Consensus with One Faulty Process" https://groups.csail.mit.edu/tds/papers/Lynch/jacm85.pdf ;
//! Van Renesse and Altinbuken, "Paxos Made Moderately Complex" (ACM Computing Surveys 2015)
use std::collections::{BTreeMap, BTreeSet};

type Ballot = u64;
type Val = u32;
const NOOP: Val = 0;
const BALLOT_BITS: u32 = 4;
fn ballot(round: u64, id: usize) -> Ballot {
    (round << BALLOT_BITS) | id as u64
}
fn round_of(b: Ballot) -> u64 {
    b >> BALLOT_BITS
}

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
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---- shared simulated network and event queue
#[derive(Clone, Copy)]
struct Net {
    drop: u64,
    dup: u64,
    lo: u64,
    hi: u64,
    spike: u64,
}
const LOSSY: Net = Net {
    drop: 15,
    dup: 10,
    lo: 1,
    hi: 30,
    spike: 10,
};
const CALM: Net = Net {
    drop: 0,
    dup: 0,
    lo: 5,
    hi: 20,
    spike: 0,
};

/// A time-ordered event queue plus a lossy wire. Reordering comes for free from random per-copy delays.
struct Sched<E> {
    now: u64,
    seq: u64,
    q: BTreeMap<(u64, u64), E>,
    rng: Rng,
    net: Net,
    /// Directed links that silently drop everything (used to script exact partial-failure scenarios).
    blocked: BTreeSet<(usize, usize)>,
    /// Extra one-way latency per endpoint (an asymmetric topology; empty = none).
    extra: Vec<u64>,
    sent: u64,
}

impl<E> Sched<E> {
    fn new(seed: u64, net: Net) -> Self {
        Sched {
            now: 0,
            seq: 0,
            q: BTreeMap::new(),
            rng: Rng::new(seed),
            net,
            blocked: BTreeSet::new(),
            extra: vec![],
            sent: 0,
        }
    }
    fn at(&mut self, t: u64, e: E) {
        self.seq += 1;
        self.q.insert((t, self.seq), e);
    }
    fn pop_until(&mut self, limit: u64) -> Option<E> {
        if self.q.first_key_value()?.0.0 > limit {
            return None;
        }
        let ((t, _), e) = self.q.pop_first()?;
        self.now = t;
        Some(e)
    }
    fn transmit(&mut self, from: usize, to: usize, mk: impl Fn() -> E) {
        self.sent += 1;
        if self.blocked.contains(&(from, to)) || self.rng.chance(self.net.drop) {
            return;
        }
        let copies = 1 + self.rng.chance(self.net.dup) as usize;
        for _ in 0..copies {
            let n = self.net;
            let mut d = if n.hi > n.lo {
                self.rng.range(n.lo, n.hi)
            } else {
                n.lo
            };
            d += self.extra.get(from).unwrap_or(&0) + self.extra.get(to).unwrap_or(&0);
            if self.rng.chance(n.spike) {
                d += self.rng.range(50, 200);
            }
            self.at(self.now + d, mk());
        }
    }
}

// ================================================================ PART 1: single-decree Paxos

const NA: usize = 5; // acceptors are addresses 0..NA
const NP: usize = 3; // proposers NA..NA+NP
const NL: usize = 2; // learners after that
const MAJ: usize = NA / 2 + 1;
fn prop_addr(p: usize) -> usize {
    NA + p
}
fn learner_addr(l: usize) -> usize {
    NA + NP + l
}

#[derive(Clone, Debug)]
enum Msg {
    Prepare(Ballot),
    Promise {
        b: Ballot,
        acc: Option<(Ballot, Val)>,
    },
    Nack {
        b: Ballot,
        promised: Ballot,
    },
    Accept(Ballot, Val),
    Accepted {
        b: Ballot,
        v: Val,
    },
}

#[derive(Default)]
struct Acceptor {
    up: bool,
    promised: Ballot,
    accepted: Option<(Ballot, Val)>,
}

impl Acceptor {
    fn on_prepare(&mut self, b: Ballot) -> Msg {
        if b > self.promised {
            self.promised = b;
            Msg::Promise {
                b,
                acc: self.accepted,
            }
        } else {
            Msg::Nack {
                b,
                promised: self.promised,
            }
        }
    }
    fn on_accept(&mut self, b: Ballot, v: Val) -> Msg {
        if b >= self.promised {
            self.promised = b;
            self.accepted = Some((b, v));
            Msg::Accepted { b, v }
        } else {
            Msg::Nack {
                b,
                promised: self.promised,
            }
        }
    }
}

/// Learns a value once a majority of acceptors reported the same ballot. Used by learners and proposers alike.
#[derive(Default)]
struct Learner {
    tally: BTreeMap<Ballot, (Val, BTreeSet<usize>)>,
    chosen: Option<Val>,
}
impl Learner {
    fn on_accepted(&mut self, acc: usize, b: Ballot, v: Val) {
        let e = self.tally.entry(b).or_insert((v, BTreeSet::new()));
        e.1.insert(acc);
        self.chosen = if e.1.len() >= MAJ {
            Some(e.0)
        } else {
            self.chosen
        };
    }
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Policy {
    Eager,
    Backoff,
    Leader,
}
#[derive(PartialEq, Clone, Copy, Debug)]
enum Phase {
    Idle,
    Preparing,
    Proposing,
}

struct Proposer {
    up: bool,
    own: Val,
    /// PERSISTENT: highest round used or seen, so a restarted proposer never reuses a ballot.
    round_hi: u64,
    ballot: Ballot,
    phase: Phase,
    promises: BTreeMap<usize, Option<(Ballot, Val)>>,
    epoch: u64,
    tries: u32,
    learner: Learner,
}

enum Ev {
    Msg { from: usize, to: usize, m: Msg },
    Timer { p: usize, epoch: u64 },
    Start(usize),
    Crash(usize),
    Restart(usize),
    Calm,
}

/// Global observer: every accept ever done by any acceptor, so it can tell when a value became CHOSEN.
struct Check1 {
    tally: BTreeMap<Ballot, BTreeMap<usize, Val>>,
    chosen: Option<Val>,
    seen: usize,
}

struct Single {
    s: Sched<Ev>,
    acc: Vec<Acceptor>,
    props: Vec<Proposer>,
    learners: Vec<Learner>,
    policy: Policy,
    amnesia: bool,
    accept_log: Vec<(usize, Ballot, Val)>,
    chk: Check1,
    ballots_started: u64,
    decided_at: u64,
}

impl Single {
    fn new(seed: u64, net: Net, policy: Policy) -> Single {
        let acc = (0..NA)
            .map(|_| Acceptor {
                up: true,
                ..Default::default()
            })
            .collect();
        let props = (0..NP)
            .map(|p| Proposer {
                up: true,
                own: 100 + p as Val,
                round_hi: 0,
                ballot: 0,
                phase: Phase::Idle,
                promises: BTreeMap::new(),
                epoch: 0,
                tries: 0,
                learner: Learner::default(),
            })
            .collect();
        let learners = (0..NL).map(|_| Learner::default()).collect();
        let chk = Check1 {
            tally: BTreeMap::new(),
            chosen: None,
            seen: 0,
        };
        Single {
            s: Sched::new(seed, net),
            acc,
            props,
            learners,
            policy,
            amnesia: false,
            accept_log: vec![],
            chk,
            ballots_started: 0,
            decided_at: 0,
        }
    }

    fn retry_delay(&mut self, p: usize) -> u64 {
        match self.policy {
            Policy::Backoff => 60 + self.s.rng.below(40 << self.props[p].tries.min(4)),
            _ => 80,
        }
    }

    fn begin_round(&mut self, p: usize) {
        let delay = self.retry_delay(p);
        let pr = &mut self.props[p];
        pr.round_hi += 1;
        pr.ballot = ballot(pr.round_hi, p);
        pr.phase = Phase::Preparing;
        pr.promises.clear();
        pr.tries += 1;
        pr.epoch += 1;
        let (b, epoch) = (pr.ballot, pr.epoch);
        self.ballots_started += 1;
        self.s.at(self.s.now + delay, Ev::Timer { p, epoch });
        for a in 0..NA {
            self.send(prop_addr(p), a, Msg::Prepare(b));
        }
    }

    fn send(&mut self, from: usize, to: usize, m: Msg) {
        self.s.transmit(from, to, || Ev::Msg {
            from,
            to,
            m: m.clone(),
        });
    }

    fn step(&mut self, limit: u64) -> Result<bool, String> {
        let Some(ev) = self.s.pop_until(limit) else {
            return Ok(false);
        };
        match ev {
            Ev::Msg { from, to, m } => self.deliver(from, to, m),
            Ev::Timer { p, epoch } => {
                let pr = &self.props[p];
                if pr.up && pr.epoch == epoch && pr.learner.chosen.is_none() {
                    self.begin_round(p);
                }
            }
            Ev::Start(p) => {
                let pr = &self.props[p];
                if pr.up && pr.phase == Phase::Idle && pr.learner.chosen.is_none() {
                    self.begin_round(p);
                }
            }
            Ev::Crash(a) if a < NA => {
                self.acc[a].up = false;
                if self.amnesia {
                    (self.acc[a].promised, self.acc[a].accepted) = (0, None);
                }
            }
            Ev::Crash(a) => {
                let pr = &mut self.props[a - NA];
                pr.up = false;
                pr.phase = Phase::Idle;
                pr.learner = Learner::default(); // volatile: relearned when it next runs a round
            }
            Ev::Restart(a) if a < NA => self.acc[a].up = true,
            Ev::Restart(a) => {
                self.props[a - NA].up = true;
                self.s.at(self.s.now + 1, Ev::Start(a - NA));
            }
            Ev::Calm => {
                self.s.net = CALM;
                for a in 0..NA + NP {
                    self.s.at(self.s.now + 1, Ev::Restart(a));
                }
            }
        }
        self.check()?;
        Ok(true)
    }

    fn deliver(&mut self, from: usize, to: usize, m: Msg) {
        if to < NA {
            if !self.acc[to].up {
                return;
            }
            match m {
                Msg::Prepare(b) => {
                    let r = self.acc[to].on_prepare(b);
                    self.send(to, from, r);
                }
                Msg::Accept(b, v) => match self.acc[to].on_accept(b, v) {
                    Msg::Accepted { b, v } => {
                        self.accept_log.push((to, b, v));
                        // Broadcast to every learner and proposer: they all count acceptances themselves.
                        for t in NA..NA + NP + NL {
                            self.send(to, t, Msg::Accepted { b, v });
                        }
                    }
                    nack => self.send(to, from, nack),
                },
                _ => {}
            }
        } else if to < learner_addr(0) {
            self.proposer_msg(to - NA, from, m);
        } else if let Msg::Accepted { b, v } = m {
            self.learners[to - learner_addr(0)].on_accepted(from, b, v);
        }
    }

    fn proposer_msg(&mut self, p: usize, from: usize, m: Msg) {
        if !self.props[p].up {
            return;
        }
        let pr = &mut self.props[p];
        match m {
            Msg::Promise { b, acc } if b == pr.ballot && pr.phase == Phase::Preparing => {
                pr.promises.insert(from, acc);
                if pr.promises.len() >= MAJ {
                    // THE Paxos rule: adopt the value of the highest-ballot accepted proposal, else use our own.
                    let v = pr
                        .promises
                        .values()
                        .flatten()
                        .max_by_key(|(b, _)| *b)
                        .map_or(pr.own, |(_, v)| *v);
                    pr.phase = Phase::Proposing;
                    let b = pr.ballot;
                    for a in 0..NA {
                        self.send(prop_addr(p), a, Msg::Accept(b, v));
                    }
                }
            }
            Msg::Nack { b, promised } if b == pr.ballot && pr.learner.chosen.is_none() => {
                pr.round_hi = pr.round_hi.max(round_of(promised));
                pr.phase = Phase::Idle;
                if self.policy == Policy::Eager {
                    self.begin_round(p);
                } // no back-off: this is what duels
            }
            Msg::Accepted { b, v } => pr.learner.on_accepted(from, b, v),
            _ => {}
        }
    }

    /// Safety: at most one value is ever chosen, and a chosen value never changes. Also every learner agrees with it.
    fn check(&mut self) -> Result<(), String> {
        while self.chk.seen < self.accept_log.len() {
            let (a, b, v) = self.accept_log[self.chk.seen];
            self.chk.seen += 1;
            let m = self.chk.tally.entry(b).or_default();
            if m.values().any(|&x| x != v) {
                return Err(format!("ballot {b} carries two different values"));
            }
            m.insert(a, v);
            if m.len() >= MAJ {
                match self.chk.chosen {
                    Some(c) if c != v => {
                        return Err(format!(
                            "SAFETY: value {c} was chosen, then {v} was chosen at ballot {b}"
                        ));
                    }
                    Some(_) => {}
                    None => {
                        self.chk.chosen = Some(v);
                        self.decided_at = self.s.now;
                    }
                }
            }
        }
        let claims = self
            .props
            .iter()
            .filter_map(|p| p.learner.chosen)
            .chain(self.learners.iter().filter_map(|l| l.chosen));
        for c in claims {
            if self.chk.chosen != Some(c) {
                return Err(format!(
                    "a learner decided {c} but the chosen value is {:?}",
                    self.chk.chosen
                ));
            }
        }
        Ok(())
    }

    fn run(&mut self, limit: u64) -> Result<(), String> {
        while self.step(limit)? {}
        self.s.now = self.s.now.max(limit);
        Ok(())
    }
}

/// Random schedule: proposers start at random times, nodes crash and restart, then the network calms down.
fn chaos_single(seed: u64, policy: Policy, amnesia: bool) -> Result<Single, String> {
    let mut sim = Single::new(seed, LOSSY, policy);
    sim.amnesia = amnesia;
    let mut plan = Rng::new(seed ^ 0x5151);
    for p in 0..NP {
        let t = if policy == Policy::Leader && p > 0 {
            1500 + plan.range(0, 100)
        } else {
            plan.range(0, 200)
        };
        sim.s.at(t, Ev::Start(p));
    }
    for _ in 0..plan.range(2, 9) {
        let (who, t) = (plan.below((NA + NP) as u64) as usize, plan.range(5, 1500));
        sim.s.at(t, Ev::Crash(who));
        sim.s.at(t + plan.range(10, 300), Ev::Restart(who));
    }
    sim.s.at(2500, Ev::Calm);
    sim.run(6000)?;
    Ok(sim)
}

/// The classic livelock as an exact script on the acceptor state machines (no network, no randomness).
fn scripted_livelock() {
    let mut acc: Vec<Acceptor> = (0..3)
        .map(|_| Acceptor {
            up: true,
            ..Default::default()
        })
        .collect();
    let mut round = [0u64; 2];
    let mut cur = [0 as Ballot; 2];
    let prepare =
        |acc: &mut Vec<Acceptor>, p: usize, round: &mut [u64; 2], cur: &mut [Ballot; 2]| {
            round[p] = round[0].max(round[1]) + 1;
            cur[p] = ballot(round[p], p);
            for a in acc.iter_mut() {
                assert!(matches!(a.on_prepare(cur[p]), Msg::Promise { .. }));
            }
        };
    prepare(&mut acc, 0, &mut round, &mut cur);
    prepare(&mut acc, 1, &mut round, &mut cur);
    const DUELS: usize = 1000;
    for i in 0..DUELS {
        let p = i % 2; // p's accept arrives after the rival's higher prepare: rejected everywhere
        for a in acc.iter_mut() {
            assert!(matches!(
                a.on_accept(cur[p], 100 + p as Val),
                Msg::Nack { .. }
            ));
        }
        prepare(&mut acc, p, &mut round, &mut cur);
    }
    assert!(
        acc.iter().all(|a| a.accepted.is_none()),
        "no acceptor ever accepted anything: nothing can be chosen"
    );
    println!(
        "  scripted duel: {DUELS} alternating prepare/accept rounds, ballots up to round {}, acceptors that accepted anything: 0",
        round[0].max(round[1])
    );
}

/// With real jitter a nack-driven duel usually desynchronises by itself; this measures how much churn each policy causes.
fn contention_stats() {
    println!(
        "  simulated contention: 3 proposers all start at t=0, jittery calm network, 300 seeds each"
    );
    for policy in [Policy::Eager, Policy::Backoff, Policy::Leader] {
        let (mut ballots, mut worst, mut time) = (0, 0, 0);
        for seed in 0..300u64 {
            let mut sim = Single::new(seed, CALM, policy);
            sim.s.at(0, Ev::Start(0));
            for p in 1..NP {
                sim.s.at(
                    if policy == Policy::Leader { 2000 } else { 0 },
                    Ev::Start(p),
                );
            }
            sim.run(5000).expect("safety");
            assert!(sim.chk.chosen.is_some(), "{policy:?} seed {seed} undecided");
            ballots += sim.ballots_started;
            worst = worst.max(sim.ballots_started);
            time += sim.decided_at;
            if policy == Policy::Leader {
                assert_eq!(
                    sim.ballots_started, 1,
                    "a lone proposer needs exactly one ballot"
                );
            }
        }
        println!(
            "    {policy:?}: avg {:.1} ballots started (worst {worst}), avg decision time {:.0}ms",
            ballots as f64 / 300.0,
            time as f64 / 300.0
        );
    }
}

fn part1() {
    println!("== PART 1: single-decree Paxos ==");
    println!(
        "-- 5000 random schedules (15% loss, 10% duplication, stragglers, proposer and acceptor crashes with persistent state)"
    );
    let (mut decided, mut chosen_vals) = ([0u32; 3], BTreeSet::new());
    for seed in 0..5000u64 {
        let policy = [Policy::Eager, Policy::Backoff, Policy::Leader][(seed % 3) as usize];
        let sim = chaos_single(seed, policy, false)
            .unwrap_or_else(|e| panic!("seed {seed} {policy:?}: {e}"));
        if let Some(v) = sim.chk.chosen {
            decided[(seed % 3) as usize] += 1;
            chosen_vals.insert(v);
        }
        // After a calm period, back-off and the distinguished proposer must have chosen a value for everyone.
        if policy != Policy::Eager {
            assert!(
                sim.chk.chosen.is_some()
                    && sim.props.iter().all(|p| p.learner.chosen == sim.chk.chosen),
                "seed {seed}: liveness"
            );
        }
    }
    println!(
        "  5000/5000 schedules safe (never two chosen values, a chosen value never changed, all learners agreed)"
    );
    println!(
        "  decided within 6s: eager {}/1667, back-off {}/1667 (all), leader {}/1666 (all); values chosen across runs: {:?}",
        decided[0], decided[1], decided[2], chosen_vals
    );

    println!("-- teeth: acceptors WITHOUT stable storage (amnesia on crash) must be caught");
    let errs: Vec<String> = (0..3000u64)
        .filter_map(|s| chaos_single(s, Policy::Backoff, true).err())
        .collect();
    let (caught, first) = (errs.len(), errs.first().cloned().unwrap_or_default());
    println!("  {caught}/3000 schedules violated safety; first: {first}");
    assert!(caught > 0, "forgetting accepted state must be detectable");

    println!("-- dueling proposers");
    scripted_livelock();
    contention_stats();
}

// ================================================================ PART 2: Multi-Paxos

const NR: usize = 5;
const MAJR: usize = NR / 2 + 1;
const HB: u64 = 60;

#[derive(Clone, Debug)]
enum M2 {
    Prepare(Ballot),
    Promise {
        b: Ballot,
        accepted: BTreeMap<usize, (Ballot, Val)>,
    },
    Nack {
        promised: Ballot,
    },
    Accept {
        b: Ballot,
        slot: usize,
        v: Val,
    },
    Accepted {
        b: Ballot,
        slot: usize,
    },
    /// Leader heartbeat that also carries the decided prefix (a real system sends only the range a follower lacks).
    Hb {
        b: Ballot,
        decided: Vec<Val>,
    },
}
#[derive(PartialEq, Clone, Copy, Debug)]
enum Role {
    Follower,
    Candidate,
    Leader,
}

struct Replica {
    up: bool,
    // ---- persistent
    promised: Ballot,
    accepted: BTreeMap<usize, (Ballot, Val)>,
    // ---- volatile
    role: Role,
    ballot: Ballot,
    promises: BTreeMap<usize, BTreeMap<usize, (Ballot, Val)>>,
    next_slot: usize,
    proposed: BTreeMap<usize, Val>,
    acks: BTreeMap<usize, BTreeSet<usize>>,
    decided: BTreeMap<usize, Val>,
    epoch: u64,
}

enum Ev2 {
    Msg { from: usize, to: usize, m: M2 },
    Election { r: usize, epoch: u64 },
    Tick { r: usize, epoch: u64 },
    Submit(Val),
    Crash(usize),
    Restart(usize),
    Calm,
}

struct Multi {
    s: Sched<Ev2>,
    reps: Vec<Replica>,
    trace: bool,
    accept_log: Vec<(usize, usize, Ballot, Val)>,
    learn_log: Vec<(usize, Val)>,
    seen: (usize, usize),
    tally: BTreeMap<(usize, Ballot), BTreeMap<usize, Val>>,
    chosen: BTreeMap<usize, Val>,
    prepares: u64,
    accepts: u64,
    retransmit: bool,
    acked: Vec<Val>,
}

macro_rules! say {
    ($m:expr, $($a:tt)*) => { if $m.trace { println!("  [{:>5}ms] {}", $m.s.now, format!($($a)*)); } };
}

impl Multi {
    fn new(seed: u64, net: Net) -> Multi {
        let reps = (0..NR)
            .map(|_| Replica {
                up: true,
                promised: 0,
                accepted: BTreeMap::new(),
                role: Role::Follower,
                ballot: 0,
                promises: BTreeMap::new(),
                next_slot: 0,
                proposed: BTreeMap::new(),
                acks: BTreeMap::new(),
                decided: BTreeMap::new(),
                epoch: 0,
            })
            .collect();
        let mut m = Multi {
            s: Sched::new(seed, net),
            reps,
            trace: false,
            accept_log: vec![],
            learn_log: vec![],
            seen: (0, 0),
            tally: BTreeMap::new(),
            chosen: BTreeMap::new(),
            prepares: 0,
            accepts: 0,
            retransmit: true,
            acked: vec![],
        };
        for r in 0..NR {
            m.reset_timer(r, if r == 0 { 100 } else { 300 });
        }
        m
    }

    fn reset_timer(&mut self, r: usize, base: u64) {
        self.reps[r].epoch += 1;
        let (epoch, d) = (self.reps[r].epoch, base + self.s.rng.below(200));
        self.s.at(self.s.now + d, Ev2::Election { r, epoch });
    }

    fn send(&mut self, from: usize, to: usize, m: M2) {
        self.s.transmit(from, to, || Ev2::Msg {
            from,
            to,
            m: m.clone(),
        });
    }

    fn broadcast(&mut self, from: usize, m: M2) {
        for to in (0..NR).filter(|&t| t != from) {
            self.send(from, to, m.clone());
        }
    }

    fn learn(&mut self, r: usize, slot: usize, v: Val) {
        if self.reps[r].decided.insert(slot, v).is_none() {
            self.learn_log.push((slot, v));
        }
    }

    fn leader(&self) -> Option<usize> {
        (0..NR)
            .filter(|&r| self.reps[r].up && self.reps[r].role == Role::Leader)
            .max_by_key(|&r| self.reps[r].ballot)
    }

    fn step_down(&mut self, r: usize) {
        if self.reps[r].role != Role::Follower {
            say!(self, "replica {r} sees a higher ballot: steps down");
            self.reps[r].role = Role::Follower;
        }
        self.reset_timer(r, 300);
    }

    fn accept_local(&mut self, r: usize, b: Ballot, slot: usize, v: Val) {
        self.reps[r].accepted.insert(slot, (b, v));
        self.accept_log.push((r, slot, b, v));
    }

    fn start_election(&mut self, r: usize) {
        let rep = &mut self.reps[r];
        rep.ballot = ballot(round_of(rep.promised) + 1, r);
        rep.promised = rep.ballot;
        rep.role = Role::Candidate;
        rep.promises = BTreeMap::from([(r, rep.accepted.clone())]);
        let b = rep.ballot;
        say!(
            self,
            "replica {r} runs PHASE 1 once for ALL slots with ballot {b}"
        );
        self.prepares += (NR - 1) as u64;
        self.broadcast(r, M2::Prepare(b));
        self.reset_timer(r, 300);
    }

    /// Phase 1 done: re-propose every slot that may have been chosen (highest-ballot value) and plug holes with no-ops.
    fn become_leader(&mut self, r: usize) {
        let mut merged: BTreeMap<usize, (Ballot, Val)> = BTreeMap::new();
        for acc in self.reps[r].promises.values() {
            for (&slot, &(b, v)) in acc {
                if merged.get(&slot).is_none_or(|&(mb, _)| b > mb) {
                    merged.insert(slot, (b, v));
                }
            }
        }
        let top = merged.keys().next_back().map_or(0, |&s| s + 1);
        let rep = &mut self.reps[r];
        rep.role = Role::Leader;
        rep.epoch += 1;
        rep.next_slot = top.max(rep.decided.keys().next_back().map_or(0, |&s| s + 1));
        rep.proposed.clear();
        rep.acks.clear();
        let (b, epoch) = (rep.ballot, rep.epoch);
        let holes = (0..top).filter(|s| !merged.contains_key(s)).count();
        say!(
            self,
            "replica {r} is LEADER (ballot {b}): recovered {} slots from promises, filling {holes} hole(s) with no-ops, next slot {}",
            merged.len(),
            self.reps[r].next_slot
        );
        for slot in 0..top {
            let v = merged.get(&slot).map_or(NOOP, |&(_, v)| v);
            self.phase2(r, slot, v);
        }
        self.s.at(self.s.now + 1, Ev2::Tick { r, epoch });
    }

    /// Phase 2 only: ONE round trip per command once a leader is established.
    fn phase2(&mut self, r: usize, slot: usize, v: Val) {
        let b = self.reps[r].ballot;
        self.reps[r].proposed.insert(slot, v);
        self.accept_local(r, b, slot, v);
        self.reps[r].acks.entry(slot).or_default().insert(r);
        self.accepts += (NR - 1) as u64;
        self.broadcast(r, M2::Accept { b, slot, v });
    }

    fn prefix(&self, r: usize) -> Vec<Val> {
        let d = &self.reps[r].decided;
        (0..).map_while(|s| d.get(&s).copied()).collect()
    }

    fn step(&mut self, limit: u64) -> Result<bool, String> {
        let Some(ev) = self.s.pop_until(limit) else {
            return Ok(false);
        };
        match ev {
            Ev2::Msg { from, to, m } if self.reps[to].up => self.deliver(from, to, m),
            Ev2::Msg { .. } => {}
            Ev2::Election { r, epoch } => {
                if self.reps[r].up
                    && self.reps[r].epoch == epoch
                    && self.reps[r].role != Role::Leader
                {
                    self.start_election(r);
                }
            }
            Ev2::Tick { r, epoch } => {
                if self.reps[r].up
                    && self.reps[r].epoch == epoch
                    && self.reps[r].role == Role::Leader
                {
                    let (b, decided) = (self.reps[r].ballot, self.prefix(r));
                    self.broadcast(r, M2::Hb { b, decided });
                    // Lost Accepts are simply re-sent until a majority acknowledges (acceptors treat repeats idempotently).
                    let pending: Vec<(usize, Val)> = self.reps[r]
                        .proposed
                        .iter()
                        .filter(|(s, _)| !self.reps[r].decided.contains_key(s))
                        .map(|(&s, &v)| (s, v))
                        .collect();
                    let resend = self.retransmit;
                    for (slot, v) in pending.into_iter().filter(|_| resend) {
                        self.broadcast(r, M2::Accept { b, slot, v });
                    }
                    self.s.at(self.s.now + HB, Ev2::Tick { r, epoch });
                }
            }
            Ev2::Submit(cmd) => {
                if let Some(l) = self.leader() {
                    let slot = self.reps[l].next_slot;
                    self.reps[l].next_slot += 1;
                    say!(
                        self,
                        "leader {l} assigns command {cmd} to slot {slot} (no phase 1)"
                    );
                    self.phase2(l, slot, cmd);
                }
            }
            Ev2::Crash(r) => {
                say!(
                    self,
                    "replica {r} CRASHES (keeps promised and accepted on disk)"
                );
                let rep = &mut self.reps[r];
                (rep.up, rep.role, rep.decided) = (false, Role::Follower, BTreeMap::new());
            }
            Ev2::Restart(r) if !self.reps[r].up => {
                say!(self, "replica {r} RESTARTS");
                self.reps[r].up = true;
                self.reset_timer(r, 300);
            }
            Ev2::Restart(_) => {}
            Ev2::Calm => {
                self.s.net = CALM;
                self.s.blocked.clear();
                for r in 0..NR {
                    self.s.at(self.s.now + 1, Ev2::Restart(r));
                }
            }
        }
        self.check()?;
        Ok(true)
    }

    fn deliver(&mut self, from: usize, to: usize, m: M2) {
        match m {
            M2::Prepare(b) if b > self.reps[to].promised => {
                self.reps[to].promised = b;
                if self.reps[to].role != Role::Follower {
                    self.step_down(to);
                }
                let accepted = self.reps[to].accepted.clone();
                self.send(to, from, M2::Promise { b, accepted });
            }
            M2::Prepare(_) | M2::Accept { .. } if !self.accept_ok(to, &m) => {
                let promised = self.reps[to].promised;
                self.send(to, from, M2::Nack { promised });
            }
            M2::Accept { b, slot, v } => {
                self.reps[to].promised = b;
                if self.reps[to].role != Role::Follower && b > self.reps[to].ballot {
                    self.step_down(to);
                }
                self.accept_local(to, b, slot, v);
                self.send(to, from, M2::Accepted { b, slot });
            }
            M2::Promise { b, accepted }
                if self.reps[to].role == Role::Candidate && b == self.reps[to].ballot =>
            {
                self.reps[to].promises.insert(from, accepted);
                if self.reps[to].promises.len() >= MAJR {
                    self.become_leader(to);
                }
            }
            M2::Accepted { b, slot }
                if self.reps[to].role == Role::Leader && b == self.reps[to].ballot =>
            {
                let rep = &mut self.reps[to];
                let acks = rep.acks.entry(slot).or_default();
                acks.insert(from);
                if acks.len() >= MAJR && !rep.decided.contains_key(&slot) {
                    let v = rep.proposed[&slot];
                    self.learn(to, slot, v);
                    if v != NOOP {
                        self.acked.push(v);
                    }
                    say!(self, "slot {slot} = {v} chosen by a majority at the leader");
                }
            }
            M2::Nack { promised }
                if promised > self.reps[to].ballot && self.reps[to].role != Role::Follower =>
            {
                self.step_down(to);
            }
            M2::Hb { b, decided } if b >= self.reps[to].promised => {
                if self.reps[to].role != Role::Follower && b > self.reps[to].ballot {
                    self.step_down(to);
                } else if self.reps[to].role == Role::Follower {
                    self.reset_timer(to, 300);
                }
                for (slot, v) in decided.into_iter().enumerate() {
                    self.learn(to, slot, v);
                }
            }
            _ => {}
        }
    }

    /// An Accept is acceptable iff its ballot is not below the promise.
    fn accept_ok(&self, to: usize, m: &M2) -> bool {
        matches!(m, M2::Accept { b, .. } if *b >= self.reps[to].promised)
    }

    /// Safety per slot: a value is chosen when a majority accepted the same (slot, ballot); it never changes, and every
    /// replica's decided entry equals it. Run after every event.
    fn check(&mut self) -> Result<(), String> {
        while self.seen.0 < self.accept_log.len() {
            let (r, slot, b, v) = self.accept_log[self.seen.0];
            self.seen.0 += 1;
            let m = self.tally.entry((slot, b)).or_default();
            if m.values().any(|&x| x != v) {
                return Err(format!("slot {slot} ballot {b} carries two values"));
            }
            m.insert(r, v);
            if m.len() >= MAJR {
                if let Some(&c) = self.chosen.get(&slot) {
                    if c != v {
                        return Err(format!(
                            "SAFETY: slot {slot} chosen as {c}, later chosen as {v}"
                        ));
                    }
                }
                self.chosen.insert(slot, v);
            }
        }
        while self.seen.1 < self.learn_log.len() {
            let (slot, v) = self.learn_log[self.seen.1];
            self.seen.1 += 1;
            if self.chosen.get(&slot) != Some(&v) {
                return Err(format!(
                    "a replica learned slot {slot} = {v}, but chosen is {:?}",
                    self.chosen.get(&slot)
                ));
            }
        }
        Ok(())
    }

    fn run(&mut self, limit: u64) -> Result<(), String> {
        while self.step(limit)? {}
        self.s.now = self.s.now.max(limit);
        Ok(())
    }
    fn run_for(&mut self, d: u64) -> Result<(), String> {
        self.run(self.s.now + d)
    }
}

fn scripted_multi() -> Result<(), String> {
    let mut m = Multi::new(5, CALM);
    m.trace = true;
    m.retransmit = false; // the script controls exactly which Accepts arrive
    println!("-- stable leader: phase 1 once, then one round trip per command");
    m.run(1000)?;
    let l = m.leader().ok_or("no leader")?;
    let (p0, a0) = (m.prepares, m.accepts);
    for c in 101..=105 {
        m.s.at(m.s.now + 1, Ev2::Submit(c));
        m.run_for(30)?;
    }
    m.run_for(300)?;
    let (dp, da) = (m.prepares - p0, m.accepts - a0);
    println!(
        "  5 commands cost {dp} Prepare and {da} Accept messages; per-slot Paxos would need {} + {da}",
        5 * (NR - 1)
    );
    assert_eq!((dp, da), (0, 5 * (NR as u64 - 1)));
    for r in 0..NR {
        assert_eq!(m.prefix(r), vec![101, 102, 103, 104, 105], "replica {r}");
    }

    println!(
        "-- leader {l} dies with PARTIALLY replicated slots: slot 5 on {{{l},a}}, slot 6 only on {{{l}}}, slot 7 on {{{l},b}}"
    );
    let o: Vec<usize> = (0..NR).filter(|&r| r != l).collect();
    let only = |keep: &[usize], blocked: &mut BTreeSet<(usize, usize)>| {
        blocked.clear();
        for &x in o.iter().filter(|x| !keep.contains(x)) {
            blocked.insert((l, x));
        }
    };
    only(&[o[0]], &mut m.s.blocked);
    m.s.at(m.s.now + 1, Ev2::Submit(106));
    m.run_for(60)?;
    only(&[], &mut m.s.blocked);
    m.s.at(m.s.now + 1, Ev2::Submit(107));
    m.run_for(60)?;
    only(&[o[1]], &mut m.s.blocked);
    m.s.at(m.s.now + 1, Ev2::Submit(108));
    m.run_for(60)?;
    m.s.at(m.s.now + 1, Ev2::Crash(l));
    m.run_for(5)?;
    // Force the successor to be o[0], reaching only o[1] and o[3] in phase 1 so it sees slot 5 (own) and slot 7 (o[1]).
    m.s.blocked = BTreeSet::from([(o[0], o[2]), (o[2], o[0])]);
    assert!(m.chosen.get(&6).is_none(), "slot 6 was never chosen");
    m.reps[o[0]].epoch += 1;
    m.start_election(o[0]);
    m.run_for(200)?;
    assert_eq!(m.leader(), Some(o[0]), "the forced candidate won");
    m.s.blocked.clear();
    m.s.at(m.s.now + 1, Ev2::Submit(109));
    m.s.at(m.s.now + 2, Ev2::Submit(110));
    m.s.at(m.s.now + 3, Ev2::Restart(l));
    m.run_for(1200)?;
    for r in 0..NR {
        assert_eq!(m.prefix(r), m.prefix(o[0]), "replica {r} log differs");
    }
    println!(
        "  final log on every replica (0 = no-op): {:?}",
        m.prefix(o[0])
    );
    let log = m.prefix(o[0]);
    assert_eq!(
        &log[5..10],
        &[106, NOOP, 108, 109, 110],
        "visible slots preserved, hole filled with a no-op, new commands appended"
    );
    Ok(())
}

fn chaos_multi(seed: u64) -> Result<Multi, String> {
    let (mut m, mut plan) = (Multi::new(seed, LOSSY), Rng::new(seed ^ 0x77));
    for k in 0..40u32 {
        m.s.at(150 + 60 * k as u64, Ev2::Submit(1000 + k));
    }
    for _ in 0..plan.range(2, 6) {
        let (who, t) = (plan.below(NR as u64) as usize, plan.range(120, 2800));
        m.s.at(t, Ev2::Crash(who));
        m.s.at(t + plan.range(50, 700), Ev2::Restart(who));
    }
    m.s.at(3200, Ev2::Calm);
    m.run(7000)?;
    // Liveness after calm: a leader exists and new commands commit everywhere.
    for k in 0..5u32 {
        // A command sent to a leader that is deposed a moment later is simply lost (the client retries with a new one).
        let landed = (0..10u32).any(|attempt| {
            let cmd = 9000 + 10 * k + attempt;
            m.s.at(m.s.now + 1, Ev2::Submit(cmd));
            m.run_for(500).is_ok() && (0..NR).all(|r| m.prefix(r).contains(&cmd))
        });
        if !landed {
            return Err(format!(
                "LIVENESS: post-calm command {k} never reached every replica"
            ));
        }
    }
    let want = m.prefix(m.leader().ok_or("lost leader")?);
    for r in 0..NR {
        if m.prefix(r) != want {
            return Err(format!(
                "replica {r} log {} != leader's {}",
                m.prefix(r).len(),
                want.len()
            ));
        }
    }
    if let Some(&lost) = m.acked.iter().find(|v| !want.contains(v)) {
        return Err(format!("acknowledged command {lost} vanished"));
    }
    Ok(m)
}

fn part2() {
    println!("\n== PART 2: Multi-Paxos ==");
    scripted_multi().expect("scripted multi-paxos scenario");
    println!(
        "-- 400 random schedules: lossy network, leader/follower crashes, 40 client commands, then calm"
    );
    let (mut slots, mut acked, mut noops, mut prepares, mut accepts) = (0, 0, 0, 0, 0);
    for seed in 0..400u64 {
        let m = chaos_multi(seed).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        let log = m.prefix(0);
        slots += log.len();
        noops += log.iter().filter(|&&v| v == NOOP).count();
        acked += m.acked.len();
        prepares += m.prepares;
        accepts += m.accepts;
    }
    println!(
        "  400/400 schedules: every slot agreed across replicas after every event, no acknowledged command lost, all logs identical after calm"
    );
    println!(
        "  totals: {slots} log slots ({noops} no-ops), {acked} leader acknowledgements, {prepares} Prepare vs {accepts} Accept messages (phase 1 is rare)"
    );
}

fn main() {
    part1();
    part2();
}
