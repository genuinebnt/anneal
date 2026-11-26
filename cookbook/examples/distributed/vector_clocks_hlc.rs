//! RECIPE 152 (distributed): Lamport clocks, vector clocks / version vectors, hybrid logical clocks and causal delivery.
//! LEVEL: intermediate
//! Use in: ordering events without a global clock, detecting concurrent writes, causal broadcast.   Run: cargo run --release --example vector_clocks_hlc
#![allow(dead_code)]
//!
//! WHAT: "happened-before" (a -> b) is the only order a distributed system can observe without a shared clock.
//! * Lamport clock: one counter; a -> b implies L(a) < L(b), but NOT the converse, so it cannot detect concurrency.
//! * Vector clock: one counter per process; compare gives Before / After / Equal / Concurrent exactly. A VERSION VECTOR has
//!   the same shape but counts updates to a replicated object (per replica) instead of events of a process.
//! * Dotted version vectors (idea only): a plain version vector assigned by a coordinator can make two concurrent writes through
//!   the SAME coordinator look ordered (shown below: a lost sibling). A dotted VV keeps the new write's single event id (the
//!   "dot" = replica + counter) apart from the causal-context vector, so a write is never considered to dominate a sibling it did not read.
//! * Hybrid logical clock (HLC): (physical ms, logical counter). Stays close to wall time (useful for humans and TTLs) while
//!   keeping Lamport's guarantee even if the physical clock stalls or jumps backwards; the physical clock is injected so tests control it.
//! * Causal broadcast: tag each message with the sender's vector; the receiver holds a message back until every message it
//!   depends on has been delivered (sender's next-in-line, and nothing from others that we have not seen).
//! COMMON USES: Dynamo-style stores keep siblings for concurrent writes and let the application merge them (Riak documents this),
//! CRDT / replication layers need causal delivery, databases such as CockroachDB use hybrid logical clocks for timestamps.
//! Read: https://en.wikipedia.org/wiki/Vector_clock and https://en.wikipedia.org/wiki/Lamport_timestamp
use std::cell::Cell;
use std::collections::BTreeMap;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n
    }
}

// ---- Lamport ----
#[derive(Default)]
struct Lamport(u64);
impl Lamport {
    fn tick(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
    fn recv(&mut self, t: u64) -> u64 {
        self.0 = self.0.max(t) + 1;
        self.0
    }
}

// ---- vector clock / version vector ----
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Causal {
    Before,
    After,
    Equal,
    Concurrent,
}

/// Absent entry == 0, and zeros are never stored, so derived equality is semantic equality.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct VClock(BTreeMap<u32, u64>);
impl VClock {
    fn get(&self, p: u32) -> u64 {
        self.0.get(&p).copied().unwrap_or(0)
    }
    fn increment(&mut self, p: u32) {
        *self.0.entry(p).or_default() += 1;
    }
    fn merge(&mut self, o: &VClock) {
        for (&p, &n) in &o.0 {
            let e = self.0.entry(p).or_default();
            *e = (*e).max(n);
        }
    }
    fn compare(&self, o: &VClock) -> Causal {
        let (mut le, mut ge) = (true, true); // self <= o everywhere? self >= o everywhere?
        for p in self.0.keys().chain(o.0.keys()) {
            le &= self.get(*p) <= o.get(*p);
            ge &= self.get(*p) >= o.get(*p);
        }
        match (le, ge) {
            (true, true) => Causal::Equal,
            (true, false) => Causal::Before,
            (false, true) => Causal::After,
            (false, false) => Causal::Concurrent,
        }
    }
}

// ---- Dynamo-style store with siblings ----
struct Version {
    clock: VClock,
    value: String,
}
struct Store {
    replica: u32,
    versions: Vec<Version>,
}
impl Store {
    /// Values plus the merged context a client must hand back on its next put (so the store knows what it has read).
    fn get(&self) -> (Vec<&str>, VClock) {
        let mut ctx = VClock::default();
        self.versions.iter().for_each(|v| ctx.merge(&v.clock));
        (
            self.versions.iter().map(|v| v.value.as_str()).collect(),
            ctx,
        )
    }
    fn put(&mut self, ctx: &VClock, value: &str) {
        let mut clock = ctx.clone();
        clock.increment(self.replica);
        self.insert(Version {
            clock,
            value: value.to_string(),
        });
    }
    /// Keep a version unless something already stored dominates it; drop stored versions it dominates.
    fn insert(&mut self, v: Version) {
        let covered = |c: Causal| matches!(c, Causal::Before | Causal::Equal);
        if self
            .versions
            .iter()
            .any(|e| covered(v.clock.compare(&e.clock)))
        {
            return;
        }
        self.versions
            .retain(|e| !covered(e.clock.compare(&v.clock)));
        self.versions.push(v);
    }
    fn sync_from(&mut self, o: &Store) {
        for v in &o.versions {
            self.insert(Version {
                clock: v.clock.clone(),
                value: v.value.clone(),
            });
        }
    }
}

// ---- hybrid logical clock with an injectable physical clock ----
trait PhysicalClock {
    fn now(&self) -> u64;
}
struct Manual(Cell<u64>); // tests drive it, including backwards
impl PhysicalClock for Manual {
    fn now(&self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
struct HlcTime {
    wall: u64,
    logical: u32,
}
struct Hlc<C: PhysicalClock> {
    clock: C,
    last: HlcTime,
}
impl<C: PhysicalClock> Hlc<C> {
    fn new(clock: C) -> Self {
        Hlc {
            clock,
            last: HlcTime::default(),
        }
    }
    /// Local event or send: wall = max(last wall, physical); the counter breaks ties while the wall part is stuck.
    fn send(&mut self) -> HlcTime {
        let wall = self.last.wall.max(self.clock.now());
        let logical = if wall == self.last.wall {
            self.last.logical + 1
        } else {
            0
        };
        self.last = HlcTime { wall, logical };
        self.last
    }
    /// Receive: result is strictly greater than both our last timestamp and the message's.
    fn recv(&mut self, m: HlcTime) -> HlcTime {
        let wall = self.last.wall.max(m.wall).max(self.clock.now());
        let logical = match (wall == self.last.wall, wall == m.wall) {
            (true, true) => self.last.logical.max(m.logical) + 1,
            (true, false) => self.last.logical + 1,
            (false, true) => m.logical + 1,
            (false, false) => 0,
        };
        self.last = HlcTime { wall, logical };
        self.last
    }
}

/// 3 nodes with skewed clocks (each within +-EPS of true time), occasionally stepping BACKWARDS, exchanging messages.
fn hlc_simulation(seed: u64) {
    const EPS: u64 = 50;
    let mut rng = Rng(seed * 6700417 + 5);
    let mut nodes: Vec<Hlc<Manual>> = (0..3).map(|_| Hlc::new(Manual(Cell::new(1_000)))).collect();
    let mut last_seen = [HlcTime::default(); 3];
    let mut true_time = 1_000u64;
    for _ in 0..2_000 {
        true_time += rng.below(3) as u64;
        for n in &nodes {
            let skew = rng.below(2 * EPS as usize + 1) as u64; // new reading anywhere in [t-EPS, t+EPS]: may go backwards
            n.clock.0.set(true_time + skew - EPS);
        }
        let (i, j) = (rng.below(3), rng.below(3));
        let ts = nodes[i].send();
        assert!(
            ts > last_seen[i],
            "send must be strictly monotonic even if the physical clock went backwards"
        );
        last_seen[i] = ts;
        assert!(
            ts.wall <= true_time + EPS,
            "HLC wall part must stay within the clock-error bound of real time"
        );
        if i != j {
            let got = nodes[j].recv(ts);
            assert!(
                got > ts && got > last_seen[j],
                "receive happens-after the send and after our own history"
            );
            assert!(got.wall <= true_time + EPS);
            last_seen[j] = got;
        }
    }
}

// ---- causal broadcast ----
#[derive(Clone)]
struct Msg {
    id: usize,
    from: u32,
    vc: VClock,
}
struct Process {
    id: u32,
    delivered: VClock, // how many messages from each process we have delivered (including our own)
    buffer: Vec<Msg>,
    log: Vec<usize>, // delivery order, by message id
}
impl Process {
    fn deliverable(&self, m: &Msg) -> bool {
        m.vc.0.iter().all(|(&p, &n)| {
            if p == m.from {
                n == self.delivered.get(p) + 1
            } else {
                n <= self.delivered.get(p)
            }
        })
    }
    fn broadcast(&mut self, id: usize) -> Msg {
        self.delivered.increment(self.id); // delivering to ourselves is immediate
        self.log.push(id);
        Msg {
            id,
            from: self.id,
            vc: self.delivered.clone(),
        }
    }
    fn receive(&mut self, m: Msg) {
        self.buffer.push(m);
        while let Some(i) = self.buffer.iter().position(|m| self.deliverable(m)) {
            let m = self.buffer.remove(i);
            self.delivered.increment(m.from);
            self.log.push(m.id);
        }
    }
}

fn causal_simulation(seed: u64) {
    let (n, mut rng) = (4, Rng(seed * 15485863 + 11));
    let mut procs: Vec<Process> = (0..n)
        .map(|id| Process {
            id: id as u32,
            delivered: VClock::default(),
            buffer: vec![],
            log: vec![],
        })
        .collect();
    let mut sent: Vec<Msg> = vec![];
    let mut net: Vec<(usize, Msg)> = vec![];
    for _ in 0..150 {
        if rng.below(2) == 0 {
            let p = rng.below(n);
            let m = procs[p].broadcast(sent.len());
            sent.push(m.clone());
            net.extend((0..n).filter(|&q| q != p).map(|q| (q, m.clone())));
        } else if !net.is_empty() {
            let (to, m) = net.swap_remove(rng.below(net.len())); // arbitrary reordering
            procs[to].receive(m);
        }
    }
    while let Some((to, m)) = net.pop() {
        procs[to].receive(m);
    }
    for p in &procs {
        assert!(
            p.buffer.is_empty() && p.log.len() == sent.len(),
            "everything is eventually delivered"
        );
        for (j, &later) in p.log.iter().enumerate() {
            // no message may be delivered after one that causally FOLLOWS it
            assert!(
                p.log[..j]
                    .iter()
                    .all(|&earlier| sent[earlier].vc.compare(&sent[later].vc) != Causal::After)
            );
        }
    }
}

fn main() {
    // Lamport: receiving jumps past the sender's time; concurrent events still get ordered numbers (no concurrency detection).
    let (mut a, mut b) = (Lamport::default(), Lamport::default());
    let t = a.tick();
    assert!(b.recv(t) > t);

    // Vector clocks distinguish causality from concurrency.
    let (mut p, mut q) = (VClock::default(), VClock::default());
    p.increment(1);
    let mut q2 = p.clone(); // q sees p's event, then acts
    q2.increment(2);
    q.increment(2); // q acts independently
    assert_eq!(p.compare(&q2), Causal::Before);
    assert_eq!(q2.compare(&p), Causal::After);
    assert_eq!(p.compare(&q), Causal::Concurrent);
    assert_eq!(p.compare(&p.clone()), Causal::Equal);
    let mut m = p.clone();
    m.merge(&q);
    assert_eq!(
        (m.compare(&p), m.compare(&q)),
        (Causal::After, Causal::After)
    );
    println!("vector clock compare/merge OK");

    // Dynamo-style siblings: two clients read the same version, write through different coordinators -> conflict kept.
    let mut sa = Store {
        replica: 1,
        versions: vec![],
    };
    let mut sb = Store {
        replica: 2,
        versions: vec![],
    };
    sa.put(&VClock::default(), "cart:[book]");
    sb.sync_from(&sa);
    let (_, ctx) = sa.get();
    sa.put(&ctx, "cart:[book,pen]"); // client 1 via replica 1
    sb.put(&ctx, "cart:[book,mug]"); // client 2 via replica 2, same context
    sa.sync_from(&sb);
    sb.sync_from(&sa);
    let (vals, merged_ctx) = sa.get();
    let vals: Vec<String> = vals.into_iter().map(String::from).collect();
    assert_eq!(
        vals.len(),
        2,
        "concurrent writes are siblings, not silently overwritten"
    );
    sa.put(&merged_ctx, "cart:[book,pen,mug]"); // application-level merge, written with the merged context
    assert_eq!(sa.get().0, ["cart:[book,pen,mug]"]);
    println!("siblings detected ({vals:?}) and resolved with the merged context");

    // The weakness dotted version vectors fix: both clients read {1:1} but write via the SAME coordinator.
    let mut sc = Store {
        replica: 1,
        versions: vec![],
    };
    sc.put(&VClock::default(), "v0");
    let (_, c0) = sc.get();
    sc.put(&c0, "from client A");
    sc.put(&c0, "from client B"); // clock {1:2} again: equals A's, so A's write is silently dropped
    assert_eq!(
        sc.versions.len(),
        1,
        "a lost sibling: plain per-replica counters cannot tell these apart"
    );
    println!(
        "same-coordinator concurrent writes collapse to {:?} (the case dotted version vectors solve)",
        sc.get().0
    );

    // HLC: stays monotonic when the physical clock goes backwards.
    let mut h = Hlc::new(Manual(Cell::new(100)));
    let t1 = h.send();
    h.clock.0.set(40);
    let t2 = h.send();
    assert!(t2 > t1 && t2.wall == 100);
    println!("HLC with backwards physical clock: {t1:?} then {t2:?}");
    (1..=25).for_each(hlc_simulation);
    println!("25 seeded 3-node HLC runs: monotonic, causal and within the skew bound");
    (1..=60).for_each(causal_simulation);
    println!(
        "60 seeded causal-broadcast runs under random reordering: causal order respected, nothing lost"
    );
}
