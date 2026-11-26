//! RECIPE 153 (distributed): SWIM failure detection and gossip membership, simulated.
//! LEVEL: advanced
//! Use in: cluster membership, failure detection, service discovery.   Run: cargo run --release --example swim_gossip
#![allow(dead_code)]
//!
//! WHAT: SWIM (Das, Gupta, Motivala 2002) detects failures with O(1) messages per node per period, no matter
//! how big the cluster is, and spreads membership changes without any broadcast.
//!
//! HOW (each piece is implemented below on a deterministic discrete-event simulator):
//!  1. Every protocol period a node pings ONE member, picked by shuffled round-robin (so every member is probed
//!     within a bounded number of periods, the paper's time-bounded completeness).
//!  2. No ack within the RTT timeout: ask k other members to ping the target for us (ping-req). This routes
//!     around a lossy or congested direct link, a major source of false positives.
//!  3. Still no ack by the end of the period: mark the target SUSPECT (not dead). After a suspicion timeout
//!     (~ mult * log n periods) it becomes DEAD, which is final.
//!  4. Updates (Alive/Suspect/Dead + incarnation) ride on pings/acks: PIGGYBACKING. Each update is sent
//!     ~ lambda*log2(n) times (enough for epidemic spread w.h.p.), least-sent first, in a bounded buffer.
//!  5. Incarnation numbers order updates: Alive(i) beats Suspect(j) iff i > j; Suspect(i) beats Alive(j) iff
//!     i >= j; Dead beats everything. A node that hears it is suspected bumps its OWN incarnation and
//!     gossips Alive(i+1): the refutation. A slow-but-alive node thus survives.
//!  The network drops messages with a seeded probability and delays them randomly, so runs are reproducible.
//!
//! COMMON USES: cluster membership and failure detection in HashiCorp memberlist (used by Serf and
//! Consul's gossip layer, with Lifeguard refinements), gossip-based membership in distributed databases,
//! and any system that wants decentralised liveness without an O(n) heartbeat fan-in.
//! Read: https://www.cs.cornell.edu/projects/Quicksilver/public_pdfs/SWIM.pdf
//! Read: https://github.com/hashicorp/memberlist
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

type NodeId = usize;

// ---------------------------------------------------------------- RNG

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo)
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.next() as usize % (i + 1));
        }
    }
}

// ---------------------------------------------------------------- protocol types

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Alive,
    Suspect,
    Dead,
}

#[derive(Clone, Copy, Debug)]
struct Update {
    node: NodeId,
    state: State,
    inc: u64,
}

#[derive(Clone, Copy, Debug)]
struct Member {
    state: State,
    inc: u64,
}

#[derive(Clone, Debug)]
enum Kind {
    Ping,
    PingReq { target: NodeId },
    Ack,
}

#[derive(Clone, Debug)]
struct Msg {
    kind: Kind,
    from: NodeId,
    to: NodeId,
    seq: u64,
    gossip: Vec<Update>, // piggybacked membership updates
}

enum Ev {
    Deliver(Msg),
    Period(NodeId),
    DirectTimeout(NodeId, u64),
    ProbeDeadline(NodeId, u64),
    SuspectTimeout { at: NodeId, node: NodeId, inc: u64 },
    Crash(NodeId),
}

struct Cfg {
    n: usize,
    loss: f64,
    period: u64, // ms between probes
    rtt: u64,    // ms to wait for a direct ack
    k: usize,    // indirect probers
    lambda: u32, // retransmit multiplier
    suspect_mult: u64,
}

impl Cfg {
    fn new(n: usize, loss: f64) -> Cfg {
        Cfg {
            n,
            loss,
            period: 1000,
            rtt: 200,
            k: 3,
            lambda: 3,
            suspect_mult: 3,
        }
    }
    fn log2n(&self) -> u64 {
        (self.n as f64 + 1.0).log2().ceil() as u64
    }
    fn retransmit_limit(&self) -> u32 {
        self.lambda * self.log2n() as u32
    }
    /// Suspicion must outlast gossip spread + refutation round trip, so it scales with log n.
    fn suspect_timeout(&self) -> u64 {
        self.suspect_mult * self.log2n() * self.period
    }
}

const MAX_PIGGYBACK: usize = 6; // keeps every packet small and O(1)
const MAX_BUFFER: usize = 64; // bounded gossip buffer

struct Probe {
    target: NodeId,
    seq: u64,
    acked: bool,
}

struct Node {
    crashed: bool,
    inc: u64,
    view: Vec<Member>,
    buf: Vec<(Update, u32)>, // (update, times sent)
    order: Vec<NodeId>,      // shuffled round-robin probe list
    seq: u64,
    probe: Option<Probe>,
    relays: HashMap<u64, (NodeId, u64)>, // our ping seq -> (requester, requester's seq)
    sent: u64,
}

impl Node {
    fn new(n: usize) -> Node {
        Node {
            crashed: false,
            inc: 0,
            view: vec![
                Member {
                    state: State::Alive,
                    inc: 0
                };
                n
            ],
            buf: Vec::new(),
            order: Vec::new(),
            seq: 0,
            probe: None,
            relays: HashMap::new(),
            sent: 0,
        }
    }
    /// Least-sent updates first (fresh news spreads fastest); drop updates that hit the retransmit limit.
    fn take_gossip(&mut self, limit: u32) -> Vec<Update> {
        self.buf.sort_by_key(|(_, c)| *c);
        let out: Vec<Update> = self
            .buf
            .iter_mut()
            .take(MAX_PIGGYBACK)
            .map(|(u, c)| {
                *c += 1;
                *u
            })
            .collect();
        self.buf.retain(|(_, c)| *c < limit);
        out
    }
}

// ---------------------------------------------------------------- simulator

struct Sim {
    cfg: Cfg,
    now: u64,
    rng: Rng,
    nodes: Vec<Node>,
    heap: BinaryHeap<Reverse<(u64, u64)>>, // (time, event id): ties break deterministically
    events: Vec<Option<Ev>>,
    slow: Vec<Option<(u64, u64, u64)>>, // (from, until, extra delay) per node
    suspect_events: Vec<u32>,           // how often each node became suspected somewhere
    dead_at: Vec<Vec<Option<u64>>>,     // [observer][node] first time observer declared node dead
}

impl Sim {
    fn new(cfg: Cfg, seed: u64) -> Sim {
        let n = cfg.n;
        let mut s = Sim {
            now: 0,
            rng: Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1),
            nodes: (0..n).map(|_| Node::new(n)).collect(),
            heap: BinaryHeap::new(),
            events: Vec::new(),
            slow: vec![None; n],
            suspect_events: vec![0; n],
            dead_at: vec![vec![None; n]; n],
            cfg,
        };
        for i in 0..n {
            let offset = s.rng.range(0, s.cfg.period); // desynchronise periods
            s.schedule(offset, Ev::Period(i));
        }
        s
    }

    fn schedule(&mut self, after: u64, ev: Ev) {
        self.events.push(Some(ev));
        self.heap
            .push(Reverse((self.now + after, self.events.len() as u64 - 1)));
    }

    fn run_until(&mut self, end: u64) {
        while let Some(&Reverse((t, id))) = self.heap.peek() {
            if t > end {
                break;
            }
            self.heap.pop();
            self.now = t;
            let ev = self.events[id as usize].take().unwrap();
            self.handle(ev);
        }
        self.now = end;
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::Crash(i) => self.nodes[i].crashed = true,
            Ev::Period(i) => self.on_period(i),
            Ev::DirectTimeout(i, seq) => self.on_direct_timeout(i, seq),
            Ev::ProbeDeadline(i, seq) => self.on_deadline(i, seq),
            Ev::SuspectTimeout { at, node, inc } => {
                let m = self.nodes[at].view[node];
                if !self.nodes[at].crashed && m.state == State::Suspect && m.inc == inc {
                    self.apply(
                        at,
                        Update {
                            node,
                            state: State::Dead,
                            inc,
                        },
                    );
                }
            }
            Ev::Deliver(m) => self.on_deliver(m),
        }
    }

    // ---- network

    fn send(&mut self, from: NodeId, to: NodeId, kind: Kind, seq: u64) {
        let limit = self.cfg.retransmit_limit();
        let gossip = self.nodes[from].take_gossip(limit);
        self.nodes[from].sent += 1;
        if self.rng.f64() < self.cfg.loss {
            return; // dropped by the lossy network
        }
        let mut delay = self.rng.range(5, 50);
        for who in [from, to] {
            if let Some((a, b, extra)) = self.slow[who] {
                if (a..b).contains(&self.now) {
                    delay += extra; // a GC pause / overloaded host: everything it touches is late
                }
            }
        }
        self.schedule(
            delay,
            Ev::Deliver(Msg {
                kind,
                from,
                to,
                seq,
                gossip,
            }),
        );
    }

    // ---- protocol

    fn next_target(&mut self, i: NodeId) -> Option<NodeId> {
        for _ in 0..2 {
            while let Some(t) = self.nodes[i].order.pop() {
                if self.nodes[i].view[t].state != State::Dead {
                    return Some(t);
                }
            }
            let mut fresh: Vec<NodeId> = (0..self.cfg.n)
                .filter(|&j| j != i && self.nodes[i].view[j].state != State::Dead)
                .collect();
            self.rng.shuffle(&mut fresh);
            self.nodes[i].order = fresh;
        }
        None
    }

    fn on_period(&mut self, i: NodeId) {
        if self.nodes[i].crashed {
            return;
        }
        self.schedule(self.cfg.period, Ev::Period(i));
        let Some(target) = self.next_target(i) else {
            return;
        };
        self.nodes[i].seq += 1;
        let seq = self.nodes[i].seq;
        self.nodes[i].probe = Some(Probe {
            target,
            seq,
            acked: false,
        });
        self.send(i, target, Kind::Ping, seq);
        self.schedule(self.cfg.rtt, Ev::DirectTimeout(i, seq));
        self.schedule(self.cfg.period * 9 / 10, Ev::ProbeDeadline(i, seq));
    }

    fn pending(&self, i: NodeId, seq: u64) -> Option<NodeId> {
        match &self.nodes[i].probe {
            Some(p) if p.seq == seq && !p.acked && !self.nodes[i].crashed => Some(p.target),
            _ => None,
        }
    }

    fn on_direct_timeout(&mut self, i: NodeId, seq: u64) {
        let Some(target) = self.pending(i, seq) else {
            return;
        };
        let mut helpers: Vec<NodeId> = (0..self.cfg.n)
            .filter(|&j| j != i && j != target && self.nodes[i].view[j].state != State::Dead)
            .collect();
        self.rng.shuffle(&mut helpers);
        for &h in helpers.iter().take(self.cfg.k) {
            self.send(i, h, Kind::PingReq { target }, seq);
        }
    }

    fn on_deadline(&mut self, i: NodeId, seq: u64) {
        let Some(target) = self.pending(i, seq) else {
            return;
        };
        let m = self.nodes[i].view[target];
        if m.state == State::Alive {
            self.apply(
                i,
                Update {
                    node: target,
                    state: State::Suspect,
                    inc: m.inc,
                },
            );
        }
        self.nodes[i].probe = None;
    }

    fn on_deliver(&mut self, m: Msg) {
        let at = m.to;
        if self.nodes[at].crashed {
            return;
        }
        for u in &m.gossip {
            self.apply(at, *u);
        }
        match m.kind {
            Kind::Ping => self.send(at, m.from, Kind::Ack, m.seq),
            Kind::PingReq { target } => {
                // Probe on behalf of the requester using our own sequence space.
                self.nodes[at].seq += 1;
                let s2 = self.nodes[at].seq;
                self.nodes[at].relays.insert(s2, (m.from, m.seq));
                self.send(at, target, Kind::Ping, s2);
            }
            Kind::Ack => {
                if let Some((origin, oseq)) = self.nodes[at].relays.remove(&m.seq) {
                    self.send(at, origin, Kind::Ack, oseq); // forward the good news
                } else if let Some(p) = self.nodes[at].probe.as_mut().filter(|p| p.seq == m.seq) {
                    p.acked = true;
                }
            }
        }
    }

    /// Merge one update into `at`'s view using SWIM's precedence rules; gossip it if it was news.
    fn apply(&mut self, at: NodeId, u: Update) {
        if u.node == at {
            let me = &mut self.nodes[at];
            if u.state == State::Suspect && u.inc >= me.inc {
                me.inc = u.inc + 1; // refute: only the node itself may increment its incarnation
                me.view[at] = Member {
                    state: State::Alive,
                    inc: me.inc,
                };
                let refute = Update {
                    node: at,
                    state: State::Alive,
                    inc: me.inc,
                };
                self.queue(at, refute);
            }
            return;
        }
        let cur = self.nodes[at].view[u.node];
        let wins = match (cur.state, u.state) {
            (State::Dead, _) => false,
            (_, State::Dead) => true,
            (State::Alive, State::Suspect) => u.inc >= cur.inc,
            _ => u.inc > cur.inc,
        };
        if !wins {
            return;
        }
        self.nodes[at].view[u.node] = Member {
            state: u.state,
            inc: u.inc,
        };
        self.queue(at, u);
        match u.state {
            State::Suspect => {
                self.suspect_events[u.node] += 1;
                let after = self.cfg.suspect_timeout();
                self.schedule(
                    after,
                    Ev::SuspectTimeout {
                        at,
                        node: u.node,
                        inc: u.inc,
                    },
                );
            }
            State::Dead => {
                self.dead_at[at][u.node].get_or_insert(self.now);
            }
            State::Alive => {}
        }
    }

    fn queue(&mut self, at: NodeId, u: Update) {
        let buf = &mut self.nodes[at].buf;
        buf.retain(|(x, _)| x.node != u.node); // newer news about a node replaces older
        if buf.len() >= MAX_BUFFER {
            let worst = (0..buf.len()).max_by_key(|&i| buf[i].1).unwrap();
            buf.swap_remove(worst); // bounded memory: evict the most-retransmitted entry
        }
        buf.push((u, 0));
    }

    // ---- observations

    fn live(&self) -> impl Iterator<Item = NodeId> + '_ {
        (0..self.cfg.n).filter(|&i| !self.nodes[i].crashed)
    }
    fn msgs_per_node_period(&self, periods: u64) -> f64 {
        let total: u64 = self.nodes.iter().map(|n| n.sent).sum();
        total as f64 / (self.cfg.n as f64 * periods as f64)
    }
}

// ---------------------------------------------------------------- experiments

fn exp_no_false_dead() {
    let mut suspicions = 0;
    for seed in 1..=6 {
        let mut s = Sim::new(Cfg::new(32, 0.05), seed);
        s.run_until(120 * 1000);
        suspicions += s.suspect_events.iter().sum::<u32>();
        for obs in s.live().collect::<Vec<_>>() {
            for j in 0..32 {
                assert_ne!(
                    s.nodes[obs].view[j].state,
                    State::Dead,
                    "seed {seed}: false DEAD {j} at {obs}"
                );
            }
        }
    }
    println!(
        "no failures, 5% loss, 6 seeds x 120 periods: 0 false DEAD ({suspicions} transient suspicions refuted/cleared)"
    );
}

fn exp_detection() {
    let (n, victim, crash_period) = (32usize, 7usize, 5u64);
    let mut lat = Vec::new();
    for seed in 1..=12 {
        let mut s = Sim::new(Cfg::new(n, 0.05), seed);
        s.schedule(crash_period * 1000, Ev::Crash(victim));
        s.run_until(90 * 1000);
        let worst = s
            .live()
            .map(|o| s.dead_at[o][victim].expect("every live node must learn of the death"))
            .max()
            .unwrap();
        let first = s
            .live()
            .map(|o| s.dead_at[o][victim].unwrap())
            .min()
            .unwrap();
        lat.push((worst - crash_period * 1000) as f64 / 1000.0);
        assert!(
            first >= crash_period * 1000,
            "DEAD declared before the crash"
        );
        for o in s.live().collect::<Vec<_>>() {
            for j in (0..n).filter(|&j| j != victim) {
                assert_ne!(
                    s.nodes[o].view[j].state,
                    State::Dead,
                    "false DEAD of a live node"
                );
            }
        }
    }
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "crash detection by ALL live nodes (n=32, suspicion timeout {} periods), periods after crash over 12 seeds: min {:.1} median {:.1} max {:.1}",
        Cfg::new(n, 0.0).suspect_timeout() / 1000,
        lat[0],
        lat[lat.len() / 2],
        lat[lat.len() - 1]
    );
    assert!(
        lat[lat.len() - 1] < 45.0,
        "bounded: probe (<= ~n periods) + suspicion timeout + gossip"
    );
}

fn exp_constant_load() {
    let mut loads = Vec::new();
    for &n in &[16usize, 128] {
        let mut s = Sim::new(Cfg::new(n, 0.05), 42);
        s.run_until(60 * 1000);
        let max = s.nodes.iter().map(|x| x.sent).max().unwrap() as f64 / 60.0;
        let mean = s.msgs_per_node_period(60);
        println!("n={n:>3}: {mean:.2} msgs sent per node per period (busiest node {max:.2})");
        assert!(
            mean > 1.8 && mean < 4.5,
            "2 (ping+ack) plus ~10% of probes fanning out to k indirect paths"
        );
        assert!(max < 8.0, "no hot spot: probes are spread randomly");
        loads.push(mean);
    }
    assert!(
        (loads[0] - loads[1]).abs() < 0.4,
        "load must not grow with cluster size (O(1), not O(n))"
    );
}

fn exp_slow_node_refutes() {
    let (n, slow) = (16usize, 3usize);
    let mut s = Sim::new(Cfg::new(n, 0.02), 9);
    s.slow[slow] = Some((5_000, 11_000, 1_500)); // 6 periods where it answers 1.5s late
    s.run_until(80 * 1000);
    let suspected = s.suspect_events[slow];
    assert!(suspected > 0, "a node that answers late must get suspected");
    assert!(
        s.nodes[slow].inc >= 1,
        "it must have refuted by bumping its incarnation"
    );
    for o in s.live().collect::<Vec<_>>() {
        let m = s.nodes[o].view[slow];
        assert_eq!(
            m.state,
            State::Alive,
            "observer {o} still doubts the slow node"
        );
        assert!(o == slow || m.inc >= 1);
        assert!(
            s.dead_at[o].iter().all(|d| d.is_none()),
            "nobody may be declared dead"
        );
    }
    println!(
        "slow-but-alive node {slow}: suspected {suspected} times, refuted with incarnation {}, never declared DEAD",
        s.nodes[slow].inc
    );
}

fn main() {
    exp_no_false_dead();
    exp_detection();
    exp_constant_load();
    exp_slow_node_refutes();
    println!("all SWIM checks passed");
}
