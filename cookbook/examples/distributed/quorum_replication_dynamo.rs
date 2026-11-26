//! RECIPE 403 (distributed): Dynamo-style quorum replication, hinted handoff, read repair, version vectors
//! LEVEL: advanced
//! Use in: replicated key-value stores, shopping-cart style "always writable" services, tuning R/W/N.   Run: cargo run --release --example quorum_replication_dynamo
#![allow(dead_code)]
//!
//! WHAT: a deterministic discrete-event simulation of Dynamo's replication layer (the paper behind Riak,
//! Cassandra, Voldemort). Keys sit on a consistent-hash ring; the first N distinct nodes clockwise are the
//! PREFERENCE LIST. A write goes to all N and succeeds after W acks; a read asks all N and succeeds after R.
//! WHY: R + W > N makes every read quorum intersect every write quorum, so a read starting after a write
//! completed must reach a replica that has it. If R + W <= N the quorums can be disjoint and reads go stale:
//! a deliberate latency/availability trade (R=W=1 is fastest).
//! HOW:
//!  * Version vectors (node -> counter) order versions; CONCURRENT ones are kept as SIBLINGS and returned to
//!    the client, whose merge function (set union of a cart) resolves them; the next put carries the merged
//!    context and supersedes both.
//!  * SLOPPY QUORUM: an unreachable home replica is replaced by the next healthy ring node, which stores the
//!    write with a HINT naming the owner and hands it off when the owner returns.
//!  * READ REPAIR pushes the merged answer to replicas that answered older data; ANTI-ENTROPY compares a
//!    digest (real systems: Merkle trees) and syncs only on mismatch.
//!  * Failure injection: node down, slow (latency x10), partition. Time is virtual with seeded random message
//!    latencies, so every schedule is reproducible by seed.
//!
//! COMMON USES: tunable consistency in Cassandra (QUORUM / ONE / ALL consistency levels), Riak
//! (n_val/r/w, siblings, hinted handoff), Voldemort; the same intersection argument underlies
//! Paxos/Raft majority quorums (there W = R = majority is fixed and writes are ordered by a leader).
//! Read: Dynamo: Amazon's Highly Available Key-value Store (DeCandia et al., SOSP 2007)
//!   https://www.allthingsdistributed.com/files/amazon-dynamo-sosp2007.pdf
//! Read: Time, Clocks, and the Ordering of Events in a Distributed System (Lamport, 1978)
//!   https://lamport.azurewebsites.net/pubs/time-clocks.pdf

use std::collections::BTreeMap;

// ---------------------------------------------------------------- rng

/// xorshift64*: tiny, seedable, good enough for simulation schedules.
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in lo..=hi.
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// splitmix64 finaliser: spreads sequential ids evenly around the ring.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

// ---------------------------------------------------------------- version vectors

type NodeId = usize;
type Key = u32;
type Vc = BTreeMap<NodeId, u64>;
/// The stored value is a shopping cart (sorted item ids) so that a union merge is meaningful.
type Value = Vec<u32>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Version {
    vc: Vc,
    value: Value,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Order {
    Equal,
    Before,
    After,
    Concurrent,
}

/// Causal comparison of two version vectors (missing entries count as 0).
fn compare(a: &Vc, b: &Vc) -> Order {
    let (mut a_gt, mut b_gt) = (false, false);
    for k in a.keys().chain(b.keys()) {
        let x = a.get(k).copied().unwrap_or(0);
        let y = b.get(k).copied().unwrap_or(0);
        a_gt |= x > y;
        b_gt |= y > x;
    }
    match (a_gt, b_gt) {
        (false, false) => Order::Equal,
        (true, false) => Order::After,
        (false, true) => Order::Before,
        (true, true) => Order::Concurrent,
    }
}

fn dominates_or_equal(a: &Vc, b: &Vc) -> bool {
    matches!(compare(a, b), Order::After | Order::Equal)
}

/// Element-wise maximum: the context a client sends back after merging siblings.
fn join(a: &Vc, b: &Vc) -> Vc {
    let mut out = a.clone();
    for (k, v) in b {
        let e = out.entry(*k).or_insert(0);
        *e = (*e).max(*v);
    }
    out
}

/// Drop every version that is strictly older than another; what remains are the siblings.
fn reconcile(vs: Vec<Version>) -> Vec<Version> {
    let mut out: Vec<Version> = Vec::new();
    for v in vs {
        let covered = out.iter().any(|o| match compare(&o.vc, &v.vc) {
            Order::After => true,
            Order::Equal => o.value == v.value,
            _ => false,
        });
        if covered {
            continue;
        }
        out.retain(|o| compare(&v.vc, &o.vc) != Order::After);
        out.push(v);
    }
    out.sort_by(|a, b| a.vc.cmp(&b.vc));
    out
}

fn context_of(vs: &[Version]) -> Vc {
    vs.iter().fold(Vc::new(), |acc, v| join(&acc, &v.vc))
}

/// Client-side sibling resolution for a cart: union of all items.
fn merge_carts(vs: &[Version]) -> Value {
    let mut items: Vec<u32> = vs.iter().flat_map(|v| v.value.iter().copied()).collect();
    items.sort_unstable();
    items.dedup();
    items
}

fn same_set(a: &[Version], b: &[Version]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(x))
}

// ---------------------------------------------------------------- replica

struct Hint {
    target: NodeId,
    key: Key,
    version: Version,
}

/// A message in flight: becomes visible at the replica at virtual time `at`.
struct Pending {
    at: u64,
    key: Key,
    versions: Vec<Version>,
    hint_for: Option<NodeId>,
}

struct Replica {
    store: BTreeMap<Key, Vec<Version>>,
    hints: Vec<Hint>,
    pending: Vec<Pending>,
    up: bool,
    slow: bool,
    side: u8,
}

impl Replica {
    fn new() -> Self {
        Replica {
            store: BTreeMap::new(),
            hints: Vec::new(),
            pending: Vec::new(),
            up: true,
            slow: false,
            side: 0,
        }
    }

    /// Apply every in-flight message that has arrived by virtual time `t`.
    fn advance(&mut self, t: u64) {
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|p| p.at <= t);
        self.pending = later;
        for p in due {
            match p.hint_for {
                Some(owner) => {
                    for v in p.versions {
                        let dup = self
                            .hints
                            .iter()
                            .any(|h| h.target == owner && h.key == p.key && h.version == v);
                        if !dup {
                            self.hints.push(Hint {
                                target: owner,
                                key: p.key,
                                version: v,
                            });
                        }
                    }
                }
                None => self.merge_store(p.key, p.versions),
            }
        }
    }

    fn merge_store(&mut self, key: Key, versions: Vec<Version>) {
        let cur = self.store.remove(&key).unwrap_or_default();
        let all: Vec<Version> = cur.into_iter().chain(versions).collect();
        self.store.insert(key, reconcile(all));
    }

    /// What this replica answers to a read: its own data plus any data it holds on behalf of others.
    fn read(&self, key: Key) -> Vec<Version> {
        let mut all = self.store.get(&key).cloned().unwrap_or_default();
        all.extend(
            self.hints
                .iter()
                .filter(|h| h.key == key)
                .map(|h| h.version.clone()),
        );
        reconcile(all)
    }

    /// FNV-1a digest of the whole store; stands in for a Merkle-tree root.
    fn digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |x: u64| {
            for b in x.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        for (k, vs) in &self.store {
            eat(*k as u64);
            for v in vs {
                for (n, c) in &v.vc {
                    eat(*n as u64);
                    eat(*c);
                }
                for x in &v.value {
                    eat(*x as u64);
                }
            }
        }
        h
    }
}

// ---------------------------------------------------------------- cluster

#[derive(Clone, Copy)]
struct Config {
    nodes: usize,
    n: usize,
    r: usize,
    w: usize,
    sloppy: bool,
    read_repair: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum KvError {
    NoCoordinator,
    Unavailable { needed: usize, got: usize },
}

#[derive(Default, Debug)]
struct Stats {
    repairs: u64,
    hints_created: u64,
    handoffs: u64,
}

#[derive(Clone, Copy, Debug)]
struct Target {
    node: NodeId,
    hint_for: Option<NodeId>,
}

struct PutOk {
    vc: Vc,
    done_at: u64,
    coord: NodeId,
}

struct GetOk {
    versions: Vec<Version>,
    done_at: u64,
}

struct Cluster {
    cfg: Config,
    ring: Vec<(u64, NodeId)>,
    replicas: Vec<Replica>,
    now: u64,
    rng: Rng,
    stats: Stats,
}

const VNODES: u64 = 16;
const SLOW_MIN: u64 = 40;
const SLOW_MAX: u64 = 60;

impl Cluster {
    fn new(cfg: Config, seed: u64) -> Self {
        let mut ring: Vec<(u64, NodeId)> = (0..cfg.nodes)
            .flat_map(|n| (0..VNODES).map(move |v| (mix((n as u64) << 32 | v), n)))
            .collect();
        ring.sort();
        Cluster {
            cfg,
            ring,
            replicas: (0..cfg.nodes).map(|_| Replica::new()).collect(),
            now: 0,
            rng: Rng::new(seed),
            stats: Stats::default(),
        }
    }

    /// Distinct nodes in ring order starting at the key's position (the full preference walk).
    fn walk(&self, key: Key) -> Vec<NodeId> {
        let pos = mix(key as u64 ^ 0xABCD_EF01);
        let start = self.ring.partition_point(|p| p.0 < pos);
        let mut out = Vec::new();
        for i in 0..self.ring.len() {
            let node = self.ring[(start + i) % self.ring.len()].1;
            if !out.contains(&node) {
                out.push(node);
            }
        }
        out
    }

    fn home(&self, key: Key) -> Vec<NodeId> {
        self.walk(key)[..self.cfg.n].to_vec()
    }

    fn reachable(&self, from: NodeId, to: NodeId) -> bool {
        self.replicas[to].up && self.replicas[from].side == self.replicas[to].side
    }

    fn pick_coord(&mut self, key: Key, want: Option<NodeId>) -> Result<NodeId, KvError> {
        let up: Vec<NodeId> = self
            .home(key)
            .into_iter()
            .filter(|&h| self.replicas[h].up)
            .collect();
        match want {
            Some(c) if up.contains(&c) => Ok(c),
            Some(_) => Err(KvError::NoCoordinator),
            None if up.is_empty() => Err(KvError::NoCoordinator),
            None => Ok(up[self.rng.next() as usize % up.len()]),
        }
    }

    /// Nodes this operation talks to. Strict: reachable home nodes only. Sloppy: unreachable home
    /// nodes are replaced by the next reachable nodes further along the ring, carrying a hint.
    fn targets(&self, key: Key, coord: NodeId) -> Vec<Target> {
        let walk = self.walk(key);
        let (home, rest) = walk.split_at(self.cfg.n);
        let mut subs = rest.iter().copied().filter(|&s| self.reachable(coord, s));
        let mut out = Vec::new();
        for &h in home {
            if self.reachable(coord, h) {
                out.push(Target {
                    node: h,
                    hint_for: None,
                });
            } else if self.cfg.sloppy {
                if let Some(s) = subs.next() {
                    out.push(Target {
                        node: s,
                        hint_for: Some(h),
                    });
                }
            }
        }
        out
    }

    /// One-way latency to a node; slow nodes are ~10x slower.
    fn lat(&mut self, node: NodeId) -> u64 {
        if self.replicas[node].slow {
            self.rng.range(SLOW_MIN, SLOW_MAX)
        } else {
            self.rng.range(1, 10)
        }
    }

    fn put(
        &mut self,
        key: Key,
        ctx: &Vc,
        value: Value,
        want: Option<NodeId>,
    ) -> Result<PutOk, KvError> {
        let coord = self.pick_coord(key, want)?;
        let targets = self.targets(key, coord);
        // The counter must exceed anything this coordinator already issued for the key, even if the
        // client's context is stale; otherwise two different writes could share one vector.
        self.replicas[coord].advance(self.now);
        let local_max = self.replicas[coord]
            .read(key)
            .iter()
            .map(|v| v.vc.get(&coord).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);
        let mut vc = ctx.clone();
        let counter = ctx.get(&coord).copied().unwrap_or(0).max(local_max) + 1;
        vc.insert(coord, counter);
        let version = Version {
            vc: vc.clone(),
            value,
        };

        let mut acks = Vec::new();
        for t in targets {
            let (req, resp) = if t.node == coord {
                (0, 0)
            } else {
                (self.lat(t.node), self.lat(t.node))
            };
            if t.hint_for.is_some() {
                self.stats.hints_created += 1;
            }
            self.replicas[t.node].pending.push(Pending {
                at: self.now + req,
                key,
                versions: vec![version.clone()],
                hint_for: t.hint_for,
            });
            acks.push(self.now + req + resp);
        }
        if acks.len() < self.cfg.w {
            return Err(KvError::Unavailable {
                needed: self.cfg.w,
                got: acks.len(),
            });
        }
        acks.sort_unstable();
        let done_at = acks[self.cfg.w - 1];
        self.now = done_at;
        Ok(PutOk { vc, done_at, coord })
    }

    fn get(&mut self, key: Key) -> Result<GetOk, KvError> {
        let coord = self.pick_coord(key, None)?;
        let targets = self.targets(key, coord);
        struct Resp {
            at: u64,
            node: NodeId,
            hint_for: Option<NodeId>,
            versions: Vec<Version>,
        }
        let mut resps = Vec::new();
        for t in targets {
            let (req, resp) = if t.node == coord {
                (0, 0)
            } else {
                (self.lat(t.node), self.lat(t.node))
            };
            let arrival = self.now + req;
            self.replicas[t.node].advance(arrival);
            resps.push(Resp {
                at: arrival + resp,
                node: t.node,
                hint_for: t.hint_for,
                versions: self.replicas[t.node].read(key),
            });
        }
        if resps.len() < self.cfg.r {
            return Err(KvError::Unavailable {
                needed: self.cfg.r,
                got: resps.len(),
            });
        }
        resps.sort_by_key(|r| r.at);
        let r = self.cfg.r;
        let merged = reconcile(resps[..r].iter().flat_map(|x| x.versions.clone()).collect());
        let done_at = resps[r - 1].at;
        if self.cfg.read_repair {
            // Late replies are still useful: anyone who answered with an older view gets the newest.
            let full = reconcile(resps.iter().flat_map(|x| x.versions.clone()).collect());
            for x in resps
                .iter()
                .filter(|x| x.hint_for.is_none() && !same_set(&x.versions, &full))
            {
                self.replicas[x.node].pending.push(Pending {
                    at: x.at,
                    key,
                    versions: full.clone(),
                    hint_for: None,
                });
                self.stats.repairs += 1;
            }
        }
        self.now = done_at;
        Ok(GetOk {
            versions: merged,
            done_at,
        })
    }

    fn set_up(&mut self, n: NodeId, up: bool) {
        self.replicas[n].up = up;
    }

    /// Deliver everything in flight (virtual "wait long enough").
    fn settle(&mut self) {
        for r in &mut self.replicas {
            r.advance(u64::MAX);
        }
    }

    /// Hint holders push data to owners that are reachable again; returns how many hints moved.
    fn handoff(&mut self) -> usize {
        self.settle();
        let mut moved = 0;
        for holder in 0..self.replicas.len() {
            if !self.replicas[holder].up {
                continue;
            }
            let hints = std::mem::take(&mut self.replicas[holder].hints);
            let mut keep = Vec::new();
            for h in hints {
                if self.reachable(holder, h.target) {
                    self.replicas[h.target].merge_store(h.key, vec![h.version]);
                    moved += 1;
                } else {
                    keep.push(h);
                }
            }
            self.replicas[holder].hints = keep;
        }
        self.stats.handoffs += moved as u64;
        moved
    }

    /// Compare digests; if they differ, merge every key both ways. Returns whether a sync happened.
    fn anti_entropy(&mut self, a: NodeId, b: NodeId) -> bool {
        self.settle();
        if self.replicas[a].digest() == self.replicas[b].digest() {
            return false;
        }
        let keys: Vec<Key> = self.replicas[a]
            .store
            .keys()
            .chain(self.replicas[b].store.keys())
            .copied()
            .collect();
        for k in keys {
            let va = self.replicas[a].store.get(&k).cloned().unwrap_or_default();
            let vb = self.replicas[b].store.get(&k).cloned().unwrap_or_default();
            self.replicas[a].merge_store(k, vb);
            self.replicas[b].merge_store(k, va);
        }
        true
    }

    fn holds(&self, node: NodeId, key: Key, vc: &Vc) -> bool {
        self.replicas[node]
            .store
            .get(&key)
            .is_some_and(|vs| vs.iter().any(|v| dominates_or_equal(&v.vc, vc)))
    }
}

fn cfg(nodes: usize, n: usize, r: usize, w: usize, sloppy: bool) -> Config {
    Config {
        nodes,
        n,
        r,
        w,
        sloppy,
        read_repair: true,
    }
}

// ---------------------------------------------------------------- demos

fn demo_ring() {
    let c = Cluster::new(cfg(6, 3, 2, 2, false), 1);
    let mut load = [0usize; 6];
    for k in 0..6000u32 {
        let w = c.walk(k);
        assert_eq!(w.len(), 6, "walk visits every node exactly once");
        load[w[0]] += 1;
    }
    assert!(
        load.iter().all(|&x| (600..1500).contains(&x)),
        "virtual nodes balance load"
    );
    println!(
        "ring: primary-owner load over 6000 keys = {load:?}; preference list of key 42 = {:?}",
        c.home(42)
    );
}

fn demo_siblings() {
    // N=R=W-style strict quorum so both writes are visible to the read.
    let mut c = Cluster::new(cfg(5, 3, 2, 2, false), 7);
    let home = c.home(1);
    // Two clients both read an empty cart and add different items through different coordinators.
    c.put(1, &Vc::new(), vec![10], Some(home[0])).unwrap();
    c.put(1, &Vc::new(), vec![20], Some(home[1])).unwrap();
    let got = c.get(1).unwrap();
    println!(
        "siblings: read returned {} versions {:?}",
        got.versions.len(),
        got.versions.iter().map(|v| &v.value).collect::<Vec<_>>()
    );
    assert_eq!(
        got.versions.len(),
        2,
        "concurrent writes via different coordinators must surface as siblings"
    );
    // Client merges and writes back with the joined context: siblings collapse.
    let merged = merge_carts(&got.versions);
    assert_eq!(merged, vec![10, 20]);
    c.put(1, &context_of(&got.versions), merged.clone(), None)
        .unwrap();
    let after = c.get(1).unwrap();
    assert_eq!(after.versions.len(), 1);
    assert_eq!(after.versions[0].value, merged);
    println!("          merged cart {merged:?} written with joined context -> single version");

    // Randomised: distinct coordinators => always 2 siblings; same coordinator serialises (no siblings).
    let mut rng = Rng::new(99);
    for t in 0..300u64 {
        let mut c = Cluster::new(cfg(5, 3, 2, 2, false), t + 1000);
        let key = rng.range(0, 50) as u32;
        let h = c.home(key);
        let i = rng.next() as usize % 3;
        let j = (i + 1 + rng.next() as usize % 2) % 3;
        c.put(key, &Vc::new(), vec![1], Some(h[i])).unwrap();
        c.put(key, &Vc::new(), vec![2], Some(h[j])).unwrap();
        assert_eq!(c.get(key).unwrap().versions.len(), 2);
    }
    println!("          300 random pairs via distinct coordinators -> always 2 siblings");
}

fn demo_sloppy_and_handoff() {
    let key = 5;
    // Strict: one home replica down and W=3 means unavailable.
    let mut strict = Cluster::new(cfg(6, 3, 2, 3, false), 3);
    let home = strict.home(key);
    strict.set_up(home[2], false);
    let err = strict
        .put(key, &Vc::new(), vec![1], Some(home[0]))
        .err()
        .unwrap();
    assert_eq!(err, KvError::Unavailable { needed: 3, got: 2 });
    println!("strict quorum, 1 of 3 homes down, W=3 -> {err:?}");

    // Sloppy: a stand-in node takes the write plus a hint.
    let mut c = Cluster::new(cfg(6, 3, 2, 3, true), 3);
    c.set_up(home[2], false);
    let ok = c.put(key, &Vc::new(), vec![1], Some(home[0])).unwrap();
    c.settle();
    let holder = (0..6)
        .find(|&n| !c.replicas[n].hints.is_empty())
        .expect("someone holds a hint");
    assert!(
        !home.contains(&holder),
        "the hint holder is outside the preference list"
    );
    assert_eq!(c.replicas[holder].hints[0].target, home[2]);
    assert!(
        c.replicas[home[2]].store.is_empty(),
        "down replica has nothing yet"
    );
    let seen = c.get(key).unwrap();
    assert_eq!(
        seen.versions.len(),
        1,
        "read during the outage sees the write via the stand-in"
    );
    println!(
        "sloppy quorum: put ok (done t={}), hint for node {} parked on node {}",
        ok.done_at, home[2], holder
    );

    // Owner returns; hints drain.
    c.set_up(home[2], true);
    let moved = c.handoff();
    assert_eq!(moved, 1);
    assert!(
        c.holds(home[2], key, &ok.vc),
        "hinted handoff delivered the write"
    );
    assert!(c.replicas.iter().all(|r| r.hints.is_empty()));
    println!(
        "          owner back: handoff moved {moved} hint, owner now holds the write, hints empty"
    );

    // Randomised: any single down home node, any seed: handoff always converges.
    let mut rng = Rng::new(5);
    let mut handed = 0;
    for t in 0..300u64 {
        let mut c = Cluster::new(cfg(7, 3, 2, 3, true), t + 1);
        let key = rng.range(0, 1000) as u32;
        let h = c.home(key);
        let down = h[rng.next() as usize % 3];
        c.set_up(down, false);
        let coord = *h.iter().find(|&&x| x != down).unwrap();
        let ok = c.put(key, &Vc::new(), vec![t as u32], Some(coord)).unwrap();
        c.set_up(down, true);
        handed += c.handoff();
        assert!(c.holds(down, key, &ok.vc));
        assert!(c.replicas.iter().all(|r| r.hints.is_empty()));
    }
    assert_eq!(handed, 300);
    println!("          300 random outages: every hint eventually delivered ({handed} handoffs)");
}

fn demo_read_repair_and_anti_entropy() {
    let key = 9;
    let mut c = Cluster::new(cfg(5, 3, 3, 1, false), 11);
    let home = c.home(key);
    c.set_up(home[2], false);
    let ok = c.put(key, &Vc::new(), vec![7], Some(home[0])).unwrap();
    c.settle();
    c.set_up(home[2], true);
    let before = home.iter().filter(|&&h| c.holds(h, key, &ok.vc)).count();
    assert_eq!(
        before, 2,
        "W=1 write while one replica was down: only the reachable homes got it"
    );
    c.get(key).unwrap(); // R=3 asks everyone and repairs the laggard
    c.settle();
    let after = home.iter().filter(|&&h| c.holds(h, key, &ok.vc)).count();
    assert_eq!(after, 3);
    assert!(c.stats.repairs >= 1);
    println!(
        "read repair: homes holding the write {before} -> {after} after one read ({} repairs)",
        c.stats.repairs
    );

    // Same situation without read repair: anti-entropy heals it instead.
    let mut c = Cluster::new(
        Config {
            read_repair: false,
            ..cfg(5, 3, 3, 1, false)
        },
        11,
    );
    c.set_up(home[2], false);
    let ok = c.put(key, &Vc::new(), vec![7], Some(home[0])).unwrap();
    c.set_up(home[2], true);
    c.get(key).unwrap();
    c.settle();
    assert!(
        !c.holds(home[2], key, &ok.vc),
        "no read repair: replica still stale"
    );
    assert_ne!(c.replicas[home[0]].digest(), c.replicas[home[2]].digest());
    assert!(c.anti_entropy(home[0], home[2]));
    assert_eq!(c.replicas[home[0]].digest(), c.replicas[home[2]].digest());
    assert!(c.holds(home[2], key, &ok.vc));
    assert!(
        !c.anti_entropy(home[0], home[2]),
        "second sync is a no-op: digests already equal"
    );
    println!("anti-entropy: digests differed, one sync made them equal, second sync was a no-op");
}

fn demo_slow_and_partition() {
    let key = 3;
    // Slow replica: W=2 hides it, W=3 waits for it. Bounds follow from the latency model, not timing.
    for (w, expect_slow) in [(2, false), (3, true)] {
        let mut c = Cluster::new(cfg(5, 3, 1, w, false), 21);
        let home = c.home(key);
        c.replicas[home[2]].slow = true;
        let start = c.now;
        let ok = c.put(key, &Vc::new(), vec![1], Some(home[0])).unwrap();
        let took = ok.done_at - start;
        println!("slow replica: W={w} put took {took} virtual ticks");
        if expect_slow {
            assert!(took >= 2 * SLOW_MIN);
        } else {
            assert!(took <= 20);
        }
    }

    // Partition: coordinator isolated with 1 of 3 homes.
    let mut strict = Cluster::new(cfg(6, 3, 2, 2, false), 31);
    let home = strict.home(key);
    strict.replicas[home[1]].side = 1;
    strict.replicas[home[2]].side = 1;
    let err = strict
        .put(key, &Vc::new(), vec![1], Some(home[0]))
        .err()
        .unwrap();
    println!("partition, strict W=2: {err:?}");
    assert!(matches!(err, KvError::Unavailable { needed: 2, got: 1 }));

    let mut sloppy = Cluster::new(cfg(6, 3, 2, 2, true), 31);
    sloppy.replicas[home[1]].side = 1;
    sloppy.replicas[home[2]].side = 1;
    let ok = sloppy.put(key, &Vc::new(), vec![1], Some(home[0])).unwrap();
    assert_eq!(sloppy.stats.hints_created, 2);
    println!(
        "partition, sloppy W=2: write accepted with {} hints for the unreachable homes",
        sloppy.stats.hints_created
    );
    for n in 0..6 {
        sloppy.replicas[n].side = 0; // heal
    }
    sloppy.handoff();
    let got = sloppy.get(key).unwrap();
    assert!(
        got.versions
            .iter()
            .any(|v| dominates_or_equal(&v.vc, &ok.vc))
    );
    assert!(
        sloppy.holds(home[1], key, &ok.vc) && sloppy.holds(home[2], key, &ok.vc),
        "hints healed the partition"
    );
}

// ---------------------------------------------------------------- statistical experiments

/// One schedule: a single client does write-then-read on a few keys, chaining contexts so every write
/// supersedes the previous one. Returns (reads, stale reads, siblings seen). Stale = the read result
/// does not contain a version that descends from the write that just completed.
fn run_schedule(config: Config, seed: u64, slow_percent: u64) -> (u32, u32, u32) {
    let mut c = Cluster::new(config, seed);
    let mut rng = Rng::new(seed ^ 0xDEAD_BEEF);
    for n in 0..config.nodes {
        c.replicas[n].slow = rng.chance(slow_percent);
    }
    let mut ctx: BTreeMap<Key, Vc> = BTreeMap::new();
    let (mut reads, mut stale, mut siblings) = (0, 0, 0);
    for step in 0..6u32 {
        let key = rng.range(0, 2) as u32;
        let c0 = ctx.get(&key).cloned().unwrap_or_default();
        let ok = c
            .put(key, &c0, vec![step + 1], None)
            .expect("no failures injected");
        ctx.insert(key, ok.vc.clone());
        let got = c.get(key).expect("no failures injected");
        reads += 1;
        if got.versions.len() > 1 {
            siblings += 1;
        }
        if !got
            .versions
            .iter()
            .any(|v| dominates_or_equal(&v.vc, &ok.vc))
        {
            stale += 1;
        }
    }
    (reads, stale, siblings)
}

fn invariant_strict_quorums() {
    let configs = [
        (3, 2, 2),
        (3, 1, 3),
        (3, 3, 1),
        (5, 3, 3),
        (5, 2, 4),
        (5, 4, 2),
    ];
    let mut rng = Rng::new(2024);
    let mut total_reads = 0;
    for seed in 0..3000u64 {
        let (n, r, w) = configs[rng.next() as usize % configs.len()];
        assert!(r + w > n);
        let mut config = cfg(n + 2, n, r, w, rng.chance(50));
        config.read_repair = rng.chance(50);
        let (reads, stale, siblings) = run_schedule(config, seed + 1, 25);
        assert_eq!(
            stale, 0,
            "R+W>N, no failures: read-your-completed-write must hold (seed {seed}, N{n} R{r} W{w})"
        );
        assert_eq!(siblings, 0, "a single causal chain never produces siblings");
        total_reads += reads;
    }
    println!(
        "invariant: 3000 random schedules with R+W>N ({total_reads} reads, random latencies, 25% slow nodes) -> 0 stale reads"
    );
}

fn staleness_table() {
    println!(
        "\nstaleness of a read that starts right after a write completes (3000 schedules x 6 reads each)"
    );
    println!("   N  R  W  R+W>N   stale reads   P(stale)");
    let table = [
        (3, 1, 1),
        (3, 1, 2),
        (3, 2, 1),
        (3, 2, 2),
        (3, 1, 3),
        (3, 3, 1),
        (5, 1, 1),
        (5, 2, 2),
        (5, 2, 3),
        (5, 3, 3),
        (5, 1, 5),
        (5, 2, 4),
    ];
    for (n, r, w) in table {
        let (mut reads, mut stale) = (0u64, 0u64);
        for seed in 0..3000u64 {
            let (a, b, _) = run_schedule(cfg(n + 2, n, r, w, false), seed + 1, 0);
            reads += a as u64;
            stale += b as u64;
        }
        let strict = r + w > n;
        println!(
            "  {n:2} {r:2} {w:2}   {:5}   {stale:11}   {:7.4}",
            if strict { "yes" } else { "no" },
            stale as f64 / reads as f64
        );
        if strict {
            assert_eq!(stale, 0, "N{n} R{r} W{w} intersects: never stale");
        } else {
            assert!(
                stale > 0,
                "N{n} R{r} W{w} is non-intersecting: stale reads must occur"
            );
        }
    }
}

fn main() {
    demo_ring();
    demo_siblings();
    demo_sloppy_and_handoff();
    demo_read_repair_and_anti_entropy();
    demo_slow_and_partition();
    invariant_strict_quorums();
    staleness_table();
    println!("\nall quorum / hinted-handoff / read-repair / sibling assertions passed");
}
