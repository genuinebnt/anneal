//! RECIPE 150 (distributed): state-based CRDTs (counters, registers, sets, map) plus an RGA sequence CRDT, verified by law tests.
//! LEVEL: advanced
//! Use in: replicated data that must accept writes everywhere and converge without coordination.   Run: cargo run --release --example crdts
#![allow(dead_code)]
//!
//! WHAT: a CRDT (conflict-free replicated data type) is a value whose `merge` is a join in a semilattice: merge must be
//! COMMUTATIVE (a+b = b+a), ASSOCIATIVE ((a+b)+c = a+(b+c)) and IDEMPOTENT (a+a = a). If it is, replicas converge no matter how
//! states are ordered, duplicated, delayed or re-sent: the network may be lossy, as long as everything is eventually re-gossiped.
//! WHY: no locks, no leader, no consensus; every replica stays writable while partitioned.
//! HOW: each type keeps enough METADATA to make merge a pure join: per-replica slots (GCounter), (time, replica) stamps for
//! last-writer-wins with a deterministic tie-break, unique add-tags so a remove only kills what it OBSERVED (OR-Set, add wins).
//! The RGA list is the sequence sketch: every character has a unique (lamport, replica) id and names the element it was
//! inserted after; concurrent inserts at one spot are ordered by id so every replica picks the same order. It is op-based, so
//! ops are idempotent and buffered until their causal dependency (the `after` element) exists.
//! Compare operational_transform (operational transform): OT needs an ordering server and is hard to get right; CRDTs pay in metadata instead.
//! COMMON USES: collaborative editors and note apps, offline-first sync, shopping carts / presence / like counters in
//! eventually consistent stores (Riak ships counters, sets and maps as data types), multi-region configuration and feature flags.
//! Read: https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type and https://crdt.tech
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

type Replica = u32;
type Stamp = (u64, Replica); // (logical time, replica id): the replica id is the deterministic tie-break

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

/// `value()` is the observable result; two replicas are "converged" when their values are equal.
trait Crdt {
    type Value: PartialEq + Debug;
    fn merge(&mut self, other: &Self);
    fn value(&self) -> Self::Value;
}

// ---- GCounter: one monotone slot per replica; merge = per-slot max ----
#[derive(Clone, Debug, Default)]
struct GCounter(BTreeMap<Replica, u64>);
impl GCounter {
    fn inc(&mut self, r: Replica, by: u64) {
        *self.0.entry(r).or_default() += by;
    }
    fn total(&self) -> u64 {
        self.0.values().sum()
    }
}
impl Crdt for GCounter {
    type Value = BTreeMap<Replica, u64>;
    fn merge(&mut self, o: &Self) {
        for (&r, &n) in &o.0 {
            let slot = self.0.entry(r).or_default();
            *slot = (*slot).max(n); // max, not add: re-delivering a state must change nothing
        }
    }
    fn value(&self) -> Self::Value {
        self.0.clone()
    }
}

// ---- PNCounter: two GCounters; decrements are increments of the negative side ----
#[derive(Clone, Debug, Default)]
struct PNCounter {
    p: GCounter,
    n: GCounter,
}
impl PNCounter {
    fn add(&mut self, r: Replica, delta: i64) {
        if delta >= 0 {
            self.p.inc(r, delta as u64)
        } else {
            self.n.inc(r, delta.unsigned_abs())
        }
    }
    fn total(&self) -> i64 {
        self.p.total() as i64 - self.n.total() as i64
    }
}
impl Crdt for PNCounter {
    type Value = (BTreeMap<Replica, u64>, BTreeMap<Replica, u64>);
    fn merge(&mut self, o: &Self) {
        self.p.merge(&o.p);
        self.n.merge(&o.n);
    }
    fn value(&self) -> Self::Value {
        (self.p.value(), self.n.value())
    }
}

// ---- LWW register: highest (stamp, value) wins; the value compare only matters for identical stamps ----
#[derive(Clone, Debug, Default)]
struct LwwRegister<T> {
    stamp: Stamp,
    val: T,
}
impl<T: Ord + Clone> LwwRegister<T> {
    fn set(&mut self, val: T, time: u64, r: Replica) {
        // time must grow locally, else a replica could not overwrite its own earlier write
        self.stamp = (time.max(self.stamp.0 + 1), r);
        self.val = val;
    }
}
impl<T: Ord + Clone + Debug> Crdt for LwwRegister<T> {
    type Value = T;
    fn merge(&mut self, o: &Self) {
        if (o.stamp, &o.val) > (self.stamp, &self.val) {
            *self = o.clone();
        }
    }
    fn value(&self) -> T {
        self.val.clone()
    }
}

// ---- G-Set: grow-only; merge = union ----
#[derive(Clone, Debug, Default)]
struct GSet<T: Ord>(BTreeSet<T>);
impl<T: Ord + Clone + Debug> Crdt for GSet<T> {
    type Value = BTreeSet<T>;
    fn merge(&mut self, o: &Self) {
        self.0.extend(o.0.iter().cloned());
    }
    fn value(&self) -> BTreeSet<T> {
        self.0.clone()
    }
}

// ---- OR-Set: every add makes a unique tag; remove tombstones only the tags it has SEEN, so a concurrent add survives ----
type Tag = (Replica, u64);
#[derive(Clone, Debug, Default)]
struct OrSet<T: Ord> {
    adds: BTreeSet<(T, Tag)>,
    removed: BTreeSet<(T, Tag)>,
}
impl<T: Ord + Clone + Debug> OrSet<T> {
    fn next_tag(&self, r: Replica) -> Tag {
        let seen = self
            .adds
            .iter()
            .chain(&self.removed)
            .filter(|(_, t)| t.0 == r)
            .map(|(_, t)| t.1)
            .max();
        (r, seen.map_or(1, |n| n + 1))
    }
    fn add(&mut self, x: T, r: Replica) {
        let tag = self.next_tag(r);
        self.adds.insert((x, tag));
    }
    fn remove(&mut self, x: &T) {
        let seen: Vec<_> = self.adds.iter().filter(|(e, _)| e == x).cloned().collect();
        self.removed.extend(seen);
    }
}
impl<T: Ord + Clone + Debug> Crdt for OrSet<T> {
    type Value = BTreeSet<T>;
    fn merge(&mut self, o: &Self) {
        self.adds.extend(o.adds.iter().cloned());
        self.removed.extend(o.removed.iter().cloned());
    }
    fn value(&self) -> BTreeSet<T> {
        self.adds
            .iter()
            .filter(|e| !self.removed.contains(*e))
            .map(|(x, _)| x.clone())
            .collect()
    }
}

// ---- LWW map: per-key LWW entry; a delete is a write of `None` (tombstone) so it also wins by timestamp ----
#[derive(Clone, Debug, Default)]
struct LwwMap<K: Ord, V> {
    entries: BTreeMap<K, (Stamp, Option<V>)>,
}
impl<K: Ord + Clone, V: Ord + Clone> LwwMap<K, V> {
    fn write(&mut self, k: K, v: Option<V>, time: u64, r: Replica) {
        let old = self.entries.get(&k).map_or(0, |e| e.0.0);
        self.entries.insert(k, ((time.max(old + 1), r), v));
    }
}
impl<K: Ord + Clone + Debug, V: Ord + Clone + Debug> Crdt for LwwMap<K, V> {
    type Value = BTreeMap<K, V>;
    fn merge(&mut self, o: &Self) {
        for (k, e) in &o.entries {
            match self.entries.get_mut(k) {
                Some(mine) if *e > *mine => *mine = e.clone(),
                Some(_) => {}
                None => {
                    self.entries.insert(k.clone(), e.clone());
                }
            }
        }
    }
    fn value(&self) -> BTreeMap<K, V> {
        self.entries
            .iter()
            .filter_map(|(k, (_, v))| v.clone().map(|v| (k.clone(), v)))
            .collect()
    }
}

// ---- RGA: replicated growable array (op-based sequence CRDT) ----
type Id = (u64, Replica); // (lamport counter, replica): totally ordered, a child's counter exceeds its parent's
#[derive(Clone, Debug)]
enum Op {
    Ins { id: Id, after: Option<Id>, ch: char },
    Del { id: Id },
}
#[derive(Clone, Debug)]
struct Elem {
    id: Id,
    ch: char,
    dead: bool, // tombstone: later inserts may still name a deleted element as their anchor
}
#[derive(Clone, Debug, Default)]
struct Rga {
    elems: Vec<Elem>,
    clock: u64,
}
impl Rga {
    fn pos(&self, id: Id) -> Option<usize> {
        self.elems.iter().position(|e| e.id == id)
    }
    /// Idempotent. Returns false only when a dependency is missing (the caller must buffer and retry).
    fn apply(&mut self, op: &Op) -> bool {
        match *op {
            Op::Ins { id, after, ch } => {
                if self.pos(id).is_some() {
                    return true;
                }
                let mut i = match after {
                    None => 0,
                    Some(a) => match self.pos(a) {
                        Some(p) => p + 1,
                        None => return false,
                    },
                };
                // Skip siblings (and their subtrees) with a higher id: higher id sits closer to the anchor on EVERY replica.
                while i < self.elems.len() && self.elems[i].id > id {
                    i += 1;
                }
                self.elems.insert(
                    i,
                    Elem {
                        id,
                        ch,
                        dead: false,
                    },
                );
                self.clock = self.clock.max(id.0);
                true
            }
            Op::Del { id } => match self.pos(id) {
                Some(p) => {
                    self.elems[p].dead = true;
                    true
                }
                None => false,
            },
        }
    }
    fn visible(&self) -> Vec<Id> {
        self.elems
            .iter()
            .filter(|e| !e.dead)
            .map(|e| e.id)
            .collect()
    }
    fn local_insert(&mut self, r: Replica, index: usize, ch: char) -> Op {
        let after = if index == 0 {
            None
        } else {
            Some(self.visible()[index - 1])
        };
        let op = Op::Ins {
            id: (self.clock + 1, r),
            after,
            ch,
        };
        assert!(self.apply(&op));
        op
    }
    fn local_delete(&mut self, index: usize) -> Op {
        let op = Op::Del {
            id: self.visible()[index],
        };
        assert!(self.apply(&op));
        op
    }
    fn text(&self) -> String {
        self.elems
            .iter()
            .filter(|e| !e.dead)
            .map(|e| e.ch)
            .collect()
    }
}

/// Deliver an op, then retry every buffered op until nothing more applies (handles out-of-order arrival).
fn deliver(r: &mut Rga, pending: &mut Vec<Op>, op: Op) {
    pending.push(op);
    loop {
        let before = pending.len();
        pending.retain(|op| !r.apply(op));
        if pending.len() == before {
            break;
        }
    }
}

/// 4 replicas edit text; every op is sent to every other replica through a shuffled, duplicating network.
fn simulate_rga(seed: u64) -> Vec<String> {
    let (n, mut rng) = (4, Rng(seed * 7919 + 1));
    let mut reps = vec![Rga::default(); n];
    let mut pending: Vec<Vec<Op>> = vec![vec![]; n];
    let mut net: Vec<(usize, Op)> = vec![];
    for _ in 0..250 {
        let r = rng.below(n);
        if rng.below(10) < 5 {
            let len = reps[r].visible().len();
            let op = if len == 0 || rng.below(10) < 7 {
                let ch = (b'a' + rng.below(26) as u8) as char;
                reps[r].local_insert(r as u32, rng.below(len + 1), ch)
            } else {
                reps[r].local_delete(rng.below(len))
            };
            net.extend((0..n).filter(|&j| j != r).map(|j| (j, op.clone())));
        } else if !net.is_empty() {
            let (to, op) = net.swap_remove(rng.below(net.len())); // random pick = reorder + delay
            if rng.below(4) == 0 {
                net.push((to, op.clone())); // duplicate delivery later
            }
            deliver(&mut reps[to], &mut pending[to], op);
        }
    }
    while let Some((to, op)) = net.pop() {
        deliver(&mut reps[to], &mut pending[to], op);
    }
    assert!(
        pending.iter().all(|p| p.is_empty()),
        "all ops must eventually apply"
    );
    reps.iter().map(Rga::text).collect()
}

// ---- generic verification: laws, then 5 replicas over a lossy/duplicating/reordering network ----
type OpFn<C> = dyn Fn(&mut C, Replica, u64, &mut Rng);

fn random_state<C: Clone + Default>(
    rng: &mut Rng,
    base: Option<&C>,
    r: Replica,
    op: &OpFn<C>,
) -> C {
    let mut c = base.cloned().unwrap_or_default(); // forking a shared history makes laws test non-trivial overlaps
    for t in 0..1 + rng.below(6) {
        op(&mut c, r, t as u64 / 2, rng);
    }
    c
}

fn joined<C: Crdt + Clone>(a: &C, b: &C) -> C {
    let mut x = a.clone();
    x.merge(b);
    x
}

fn check_laws<C: Crdt + Clone + Default>(name: &str, op: &OpFn<C>) {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    for _ in 0..400 {
        let a = random_state(&mut rng, None, 0, op);
        let fork_a = rng.below(2) == 0;
        let b = random_state(&mut rng, fork_a.then_some(&a), 1, op);
        let fork_b = rng.below(2) == 0;
        let c = random_state(&mut rng, fork_b.then_some(&b), 2, op);
        assert_eq!(
            joined(&a, &b).value(),
            joined(&b, &a).value(),
            "{name}: commutativity"
        );
        assert_eq!(
            joined(&joined(&a, &b), &c).value(),
            joined(&a, &joined(&b, &c)).value(),
            "{name}: associativity"
        );
        assert_eq!(joined(&a, &a).value(), a.value(), "{name}: idempotence");
    }
}

fn simulate<C: Crdt + Clone + Default>(seed: u64, op: &OpFn<C>) -> Vec<C> {
    let (n, mut rng) = (5, Rng(seed * 104729 + 3));
    let mut reps = vec![C::default(); n];
    let mut net: Vec<(usize, C)> = vec![];
    for step in 0..300u64 {
        let r = rng.below(n);
        match rng.below(10) {
            0..=4 => op(&mut reps[r], r as Replica, step / 3, &mut rng), // time/3 => frequent cross-replica timestamp ties
            5..=6 => net.push((rng.below(n), reps[r].clone())), // gossip a full state snapshot
            _ if !net.is_empty() => {
                let i = rng.below(net.len());
                match rng.below(4) {
                    0 => {} // dropped: the message vanishes
                    1 => {
                        let (to, m) = net[i].clone(); // duplicated: stays in flight, delivered again later
                        reps[to].merge(&m);
                    }
                    _ => {
                        let (to, m) = net.swap_remove(i);
                        reps[to].merge(&m);
                    }
                }
                if rng.below(4) == 0 && !net.is_empty() {
                    net.swap_remove(0); // extra drop
                }
            }
            _ => {}
        }
    }
    for i in 0..n {
        for j in 0..n {
            let other = reps[j].clone(); // final all-to-all anti-entropy round
            reps[i].merge(&other);
        }
    }
    reps
}

fn verify<C: Crdt + Clone + Default>(name: &str, op: &OpFn<C>) {
    check_laws::<C>(name, op);
    for seed in 1..=30 {
        let reps = simulate::<C>(seed, op);
        assert!(
            reps.iter().all(|r| r.value() == reps[0].value()),
            "{name}: diverged at seed {seed}"
        );
    }
    println!("  {name:<12} laws OK (400 triples), 30 seeded 5-replica runs converged");
}

/// The anti-pattern: replicate a plain number and let the last arrival overwrite. Order of arrival decides the result.
fn naive_counter_counterexample() {
    let (a, b) = (1u64, 2u64); // replica A incremented once, replica B twice: the true total is 3
    let hear = |order: [u64; 2]| order.iter().fold(0u64, |_, &v| v); // receive = overwrite with what arrived
    let (c, d) = (hear([a, b]), hear([b, a])); // C hears A then B, D hears B then A
    assert_ne!(c, d, "naive replicas diverge");
    let (mut g1, mut g2) = (GCounter::default(), GCounter::default());
    let (mut ga, mut gb) = (GCounter::default(), GCounter::default());
    ga.inc(0, 1);
    gb.inc(1, 2);
    g1.merge(&ga);
    g1.merge(&gb);
    g2.merge(&gb);
    g2.merge(&ga);
    assert_eq!((g1.total(), g2.total()), (3, 3));
    println!(
        "  naive 'last arrival wins': C={c} D={d} (diverged, and neither is 3); GCounter: both {}",
        g1.total()
    );
}

fn main() {
    println!(
        "state-based CRDTs: laws + convergence under a lossy, duplicating, reordering network"
    );
    verify::<GCounter>("GCounter", &|c, r, _, rng| {
        c.inc(r, 1 + rng.below(3) as u64)
    });
    verify::<PNCounter>("PNCounter", &|c, r, _, rng| {
        c.add(r, rng.below(7) as i64 - 3)
    });
    verify::<LwwRegister<u8>>("LwwRegister", &|c, r, t, rng| {
        c.set(rng.below(5) as u8, t, r)
    });
    verify::<GSet<u8>>("GSet", &|c, _, _, rng| {
        c.0.insert(rng.below(12) as u8);
    });
    verify::<OrSet<u8>>("OrSet", &|c, r, _, rng| {
        let x = rng.below(5) as u8;
        if rng.below(3) == 0 {
            c.remove(&x)
        } else {
            c.add(x, r)
        }
    });
    verify::<LwwMap<u8, u8>>("LwwMap", &|c, r, t, rng| {
        let k = rng.below(4) as u8;
        let v = if rng.below(4) == 0 {
            None
        } else {
            Some(rng.below(6) as u8)
        };
        c.write(k, v, t, r);
    });

    // LWW tie-break: same logical time on two replicas; the higher replica id wins everywhere, in either merge order.
    let (mut x, mut y) = (
        LwwRegister::<&str>::default(),
        LwwRegister::<&str>::default(),
    );
    x.stamp = (5, 0);
    x.val = "from r0";
    y.stamp = (5, 1);
    y.val = "from r1";
    assert_eq!(joined(&x, &y).value(), "from r1");
    assert_eq!(joined(&y, &x).value(), "from r1");

    // OR-Set add-wins: r1 removes what it saw while r0 concurrently re-adds; the new tag survives.
    let (mut s0, mut s1) = (OrSet::<&str>::default(), OrSet::default());
    s0.add("milk", 0);
    s1.merge(&s0);
    s1.remove(&"milk");
    s0.add("milk", 0); // concurrent: s1 never saw this tag
    assert_eq!(joined(&s0, &s1).value(), BTreeSet::from(["milk"]));
    println!("  LWW tie-break and OR-Set add-wins demos OK");
    naive_counter_counterexample();

    println!("RGA sequence CRDT");
    let mut a = Rga::default();
    let base = ["h", "i"]
        .iter()
        .enumerate()
        .map(|(i, s)| a.local_insert(0, i, s.chars().next().unwrap()))
        .collect::<Vec<_>>();
    let mut b = Rga::default();
    base.iter().for_each(|o| assert!(b.apply(o)));
    let (oa, ob) = (a.local_insert(0, 1, 'X'), b.local_insert(1, 1, 'Y')); // both insert at index 1 concurrently
    assert!(a.apply(&ob) && b.apply(&oa));
    assert_eq!(a.text(), b.text());
    println!(
        "  concurrent inserts at the same index converge: {:?} == {:?}",
        a.text(),
        b.text()
    );
    for seed in 1..=40 {
        let texts = simulate_rga(seed);
        assert!(
            texts.iter().all(|t| *t == texts[0]),
            "RGA diverged at seed {seed}: {texts:?}"
        );
    }
    println!("  40 seeded 4-replica runs (shuffled + duplicated ops, buffered deps) converged");
}
