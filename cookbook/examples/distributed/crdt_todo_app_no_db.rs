//! RECIPE 800 (distributed): an offline-first collaborative TODO list with NO database: composed CRDTs + three list orderings (RGA, Logoot/LSEQ, Fugue).
//! LEVEL: intermediate
//! Use in: any app where every device keeps its own full copy and syncs peer to peer.   Run: cargo run --release --example crdt_todo_app_no_db
//! COMMON USES: offline-first note/todo/task apps, local-first software (Automerge, Yjs, Ink & Switch research), collaborative lists in design tools.
//! Course: Weidner & Kleppmann, "The Art of the Fugue: Minimizing Interleaving in Collaborative Text Editing"; Kleppmann "Designing Data-Intensive Applications" ch. 5 (leaderless replication).
#![allow(dead_code)]
//!
//! WHAT: replicas keep ALL state in memory and sync over a simulated network that loses, duplicates, reorders and partitions
//! messages. State is a composition of CRDTs:
//!  * items: an OR-Set style "alive" flag (add tags + observed-remove set) with an ADD-WINS or REMOVE-WINS policy (`Policy`),
//!  * per-field registers: `title`/`due` are LWW registers stamped with a hybrid logical clock (`Hlc`), `done` is a multi-value
//!    register (concurrent true/false both survive, read policy = "done wins"),
//!  * a PN-counter of completion events (increment on done, decrement on un-done); it counts EVENTS, so under concurrent toggles
//!    it can differ from "how many items are done": the demo prints both,
//!  * ORDER of items: a sequence CRDT behind one trait `Seq`, implemented three ways: RGA, Logoot/LSEQ position ids, and Fugue.
//! HOW (replication): every local change is an `Op` stamped with a dot (replica, seq) and its causal dependencies (a version
//! vector). Delivery goes through a CAUSAL BUFFER (an op waits until its origin's previous op and every dep were delivered; duplicates
//! are dropped by the version vector). Sync is delta-state: replica A sends B only the ops B's version vector lacks.
//! HOW (the three orders): RGA/Fugue store a tree (each node names its parent), Logoot stores a dense position id per element.
//! In every case the visible order is a pure function of the SET of nodes, so merge = set union and the three CRDT laws
//! (commutative, associative, idempotent) are tested directly on `Doc::merge`.
//!  * Fugue (Weidner et al.): to insert after visible element L: if L has no right child, become L's right child, otherwise become
//!    a LEFT child of the next node in the full (tombstones included) traversal. Siblings sort by id. This keeps one author's run contiguous.
//!    This is BASIC Fugue, not FugueMax: FugueMax orders right-side siblings by their right origins (reverse order, ids break ties) and is the
//!    variant the paper proves "maximally non-interleaving"; basic Fugue may interleave more than necessary where some interleaving is unavoidable.
//!  * RGA: insert after L; siblings with the same origin sort newest-first by a Lamport timestamp. Typing BACKWARDS interleaves.
//!  * Logoot/LSEQ: a position is a path of (digit, site, counter); allocate between neighbours with boundary strategies. Concurrent runs interleave.
//! HOW (garbage collection): a tombstone is removable once its delete is CAUSALLY STABLE (every replica reported a version vector covering it) AND
//! this replica has delivered everything those reports cover (otherwise a concurrent op that names the tombstone as origin could still be in flight).
//! RGA and Logoot satisfy this per tombstone. Fugue may name a tombstone as `rightOrigin` even after seeing its delete, so its tombstones are only
//! collected at a BARRIER (all replicas identical, nothing in flight); we do that explicitly. Tags of the OR-sets are collected the same way.
//! SNAPSHOT: `Replica::save`/`load` use a hand-written binary format (magic, LEB128 varints, length-prefixed strings, FNV checksum).
//! Honest limits: snapshots do not contain the op log (restart only when your local ops are already replicated); GC'd state must not be
//! state-merged with un-GC'd old states (tag removal assumes op-based delivery); only the happy path of Fugue's published proofs is exercised here.
//! Read: Fugue paper https://arxiv.org/abs/2305.00583 ; "A Conflict-Free Replicated JSON Datatype" https://arxiv.org/abs/1608.03960 ; https://crdt.tech
//! TRY NEXT:
//!   - [S] In `Policy::RemoveWins` a concurrent edit does not resurrect a deleted item. Add a third policy "last-writer-wins by Hlc" in `Item::alive` and extend `add_wins_vs_remove_wins` to show three outcomes.
//!   - [S] Predict, then measure: change `Rng::chance` drop rate in `Sim::send` from 10% to 60%. How many extra drain rounds does `Sim::drain` need and why does the final state not change?
//!   - [M] Replace `Logoot::alloc`'s boundary strategy with uniform random choice and rerun `interleave_stats`: does the interleaved fraction go up or down, and why?
//!   - [M] Make `Replica::gc` collect Fugue tombstones without the barrier by also recording `rightOrigin` in `TNode` and keeping tombstones that any live node references; which invariant do you need from `deliver_ready`?
//!   - [A] PROJECT: put this replica behind the framed protocol of `record_batch_codec` (ops as records, checksum per batch) and gossip membership with `swim_gossip` so `Replica::peers` is discovered rather than fixed.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

type Rep = u32;
/// A dot: (replica, per-replica sequence). Also used as item id and node id.
type Id = (Rep, u64);
const ROOT: Id = (u32::MAX, 0);
type Vv = BTreeMap<Rep, u64>;
type Res<T> = Result<T, String>;

fn vget(v: &Vv, r: Rep) -> u64 {
    v.get(&r).copied().unwrap_or(0)
}
fn vle(a: &Vv, b: &Vv) -> bool {
    a.iter().all(|(r, s)| *s <= vget(b, *r))
}
fn vmax(a: &mut Vv, b: &Vv) {
    for (r, s) in b {
        let e = a.entry(*r).or_default();
        *e = (*e).max(*s);
    }
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n.max(1)
    }
    fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- binary format
struct W(Vec<u8>);
impl W {
    fn u(&mut self, mut v: u64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.0.push(b);
                return;
            }
            self.0.push(b | 0x80);
        }
    }
    fn id(&mut self, i: Id) {
        self.u(i.0 as u64);
        self.u(i.1);
    }
    fn opt(&mut self, i: Option<Id>) {
        match i {
            Some(i) => {
                self.u(1);
                self.id(i)
            }
            None => self.u(0),
        }
    }
    fn s(&mut self, s: &str) {
        self.u(s.len() as u64);
        self.0.extend_from_slice(s.as_bytes());
    }
    fn hlc(&mut self, h: Hlc) {
        self.u(h.wall);
        self.u(h.logical as u64);
        self.u(h.rep as u64);
    }
}
struct R<'a> {
    b: &'a [u8],
    p: usize,
}
impl R<'_> {
    fn u(&mut self) -> Res<u64> {
        let (mut v, mut sh) = (0u64, 0);
        loop {
            let b = *self.b.get(self.p).ok_or("truncated varint")?;
            self.p += 1;
            if sh > 63 {
                return Err("varint overflow".into());
            }
            v |= ((b & 0x7f) as u64) << sh;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            sh += 7;
        }
    }
    fn id(&mut self) -> Res<Id> {
        Ok((self.u()? as u32, self.u()?))
    }
    fn opt(&mut self) -> Res<Option<Id>> {
        Ok(if self.u()? == 1 {
            Some(self.id()?)
        } else {
            None
        })
    }
    fn s(&mut self) -> Res<String> {
        let n = self.u()? as usize;
        let end = self
            .p
            .checked_add(n)
            .filter(|e| *e <= self.b.len())
            .ok_or("truncated string")?;
        let s = std::str::from_utf8(&self.b[self.p..end])
            .map_err(|e| e.to_string())?
            .to_string();
        self.p = end;
        Ok(s)
    }
    fn hlc(&mut self) -> Res<Hlc> {
        Ok(Hlc {
            wall: self.u()?,
            logical: self.u()? as u32,
            rep: self.u()? as u32,
        })
    }
}
fn fnv(b: &[u8]) -> u32 {
    b.iter().fold(0x811c_9dc5u32, |h, x| {
        (h ^ *x as u32).wrapping_mul(16_777_619)
    })
}

// ---------------------------------------------------------------- clocks and registers
/// Hybrid logical clock: wall time when it moves forward, a logical counter when it does not, replica id breaks ties.
/// Causally later events always get a bigger stamp even if a replica's wall clock is behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
struct Hlc {
    wall: u64,
    logical: u32,
    rep: Rep,
}
impl Hlc {
    fn tick(&mut self, pt: u64) {
        if pt > self.wall {
            self.wall = pt;
            self.logical = 0;
        } else {
            self.logical += 1;
        }
    }
    fn observe(&mut self, m: Hlc, pt: u64) {
        let w = self.wall.max(m.wall).max(pt);
        self.logical = if w == self.wall && w == m.wall {
            self.logical.max(m.logical) + 1
        } else if w == self.wall {
            self.logical + 1
        } else if w == m.wall {
            m.logical + 1
        } else {
            0
        };
        self.wall = w;
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
struct Lww<T> {
    hlc: Hlc,
    val: T,
}
impl<T: Clone> Lww<T> {
    fn set(&mut self, hlc: Hlc, val: &T) {
        if hlc > self.hlc {
            self.hlc = hlc;
            self.val = val.clone();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Policy {
    AddWins,
    RemoveWins,
}

/// One todo item: every field is its own small CRDT, so concurrent edits to different fields all survive.
#[derive(Clone, Debug, PartialEq, Default)]
struct Item {
    title: Lww<String>,
    due: Lww<Option<u64>>,
    done: BTreeMap<Id, bool>, // multi-value register: dot -> value
    done_rm: BTreeSet<Id>,    // dots overwritten by a later write that observed them
    tags: BTreeSet<Id>,       // OR-set add tags (creation and every edit is an "add")
    tags_rm: BTreeSet<Id>,    // observed-removed tags
    killed_by: Option<Id>,    // smallest delete dot (remove-wins flag + GC stability anchor)
}
impl Item {
    fn live_tags(&self) -> Vec<Id> {
        self.tags.difference(&self.tags_rm).copied().collect()
    }
    fn alive(&self, p: Policy) -> bool {
        !self.live_tags().is_empty() && (p == Policy::AddWins || self.killed_by.is_none())
    }
    fn done_dots(&self) -> Vec<Id> {
        self.done
            .keys()
            .filter(|d| !self.done_rm.contains(d))
            .copied()
            .collect()
    }
    fn is_done(&self) -> bool {
        self.done
            .iter()
            .any(|(d, v)| *v && !self.done_rm.contains(d))
    }
    fn merge(&mut self, o: &Item) {
        self.title.set(o.title.hlc, &o.title.val);
        self.due.set(o.due.hlc, &o.due.val);
        self.done.extend(o.done.iter().map(|(k, v)| (*k, *v)));
        self.done_rm.extend(&o.done_rm);
        self.tags.extend(&o.tags);
        self.tags_rm.extend(&o.tags_rm);
        self.killed_by = match (self.killed_by, o.killed_by) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }
}

// ---------------------------------------------------------------- the sequence trait + three implementations
trait Seq: Clone + Default + Debug + PartialEq {
    type Node: Clone + Debug;
    const NAME: &'static str;
    /// Local insert at a visible index; returns the node to broadcast.
    fn local_insert(&mut self, id: Id, idx: usize, rng: &mut Rng) -> Self::Node;
    /// Idempotent remote integrate (causal delivery guarantees the node's references exist).
    fn integrate(&mut self, n: &Self::Node);
    fn tombstone(&mut self, id: Id, dot: Id) -> bool;
    fn visible(&self) -> Vec<Id>;
    fn contains(&self, id: Id) -> bool;
    fn merge(&mut self, o: &Self);
    fn gc(&mut self, stable: &dyn Fn(Id) -> bool, barrier: bool) -> usize;
    fn total(&self) -> usize;
    fn encode(&self, w: &mut W);
    fn decode(r: &mut R) -> Res<Self>;
}

#[derive(Clone, Debug, PartialEq, Default)]
struct TNode {
    parent: Id,
    side: u8, // 0 = left child, 1 = right child
    ts: u64,  // Lamport stamp (only RGA sorts by it)
    del: Option<Id>,
}
/// Shared tree storage: `RGA = true` means only right children, newest first; `false` is Fugue (two sides, siblings by id).
#[derive(Clone, Debug, PartialEq, Default)]
struct Tree<const RGA: bool> {
    nodes: BTreeMap<Id, TNode>,
    kids: BTreeMap<(Id, u8), BTreeSet<(u64, Id)>>,
    max_ts: u64,
}
type Rga = Tree<true>;
type Fugue = Tree<false>;

enum Fr {
    Kids(Id, u8),
    Node(Id),
    Emit(Id),
}
impl<const RGA: bool> Tree<RGA> {
    fn key(n: &TNode, id: Id) -> (u64, Id) {
        (if RGA { n.ts } else { 0 }, id)
    }
    fn has_kids(&self, id: Id, side: u8) -> bool {
        self.kids.contains_key(&(id, side))
    }
    /// In-order traversal including tombstones: left kids, node, right kids.
    fn full_order(&self) -> Vec<Id> {
        let (mut out, mut st) = (
            Vec::with_capacity(self.nodes.len()),
            vec![Fr::Kids(ROOT, 1)],
        );
        while let Some(f) = st.pop() {
            match f {
                Fr::Emit(id) => out.push(id),
                Fr::Node(id) => {
                    st.extend([Fr::Kids(id, 1), Fr::Emit(id), Fr::Kids(id, 0)]);
                }
                Fr::Kids(p, s) => {
                    if let Some(set) = self.kids.get(&(p, s)) {
                        // stack pops last-pushed first, so push in reverse of the wanted visiting order
                        if RGA {
                            st.extend(set.iter().map(|k| Fr::Node(k.1)));
                        } else {
                            st.extend(set.iter().rev().map(|k| Fr::Node(k.1)));
                        }
                    }
                }
            }
        }
        out
    }
    fn unlink(&mut self, id: Id) {
        if let Some(n) = self.nodes.remove(&id) {
            let key = (n.parent, n.side);
            if let Some(set) = self.kids.get_mut(&key) {
                set.remove(&Self::key(&n, id));
                if set.is_empty() {
                    self.kids.remove(&key);
                }
            }
        }
    }
}
impl<const RGA: bool> Seq for Tree<RGA> {
    type Node = (Id, TNode);
    const NAME: &'static str = if RGA { "RGA" } else { "Fugue" };
    fn local_insert(&mut self, id: Id, idx: usize, _rng: &mut Rng) -> Self::Node {
        let vis = self.visible();
        assert!(idx <= vis.len());
        let left = if idx == 0 { ROOT } else { vis[idx - 1] };
        let (parent, side) = if RGA || !self.has_kids(left, 1) {
            (left, 1)
        } else {
            // Fugue: left has a right subtree, so go LEFT of the next node in the full traversal (it has no left kids).
            let full = self.full_order();
            let next = if left == ROOT {
                full[0]
            } else {
                full[full.iter().position(|x| *x == left).unwrap() + 1]
            };
            (next, 0)
        };
        let n = TNode {
            parent,
            side,
            ts: self.max_ts + 1,
            del: None,
        };
        self.integrate(&(id, n.clone()));
        (id, n)
    }
    fn integrate(&mut self, (id, n): &Self::Node) {
        if self.nodes.contains_key(id) {
            return;
        }
        self.kids
            .entry((n.parent, n.side))
            .or_default()
            .insert(Self::key(n, *id));
        self.max_ts = self.max_ts.max(n.ts);
        self.nodes.insert(*id, n.clone());
    }
    fn tombstone(&mut self, id: Id, dot: Id) -> bool {
        match self.nodes.get_mut(&id) {
            Some(n) => {
                n.del = Some(n.del.map_or(dot, |d| d.min(dot)));
                true
            }
            None => false,
        }
    }
    fn visible(&self) -> Vec<Id> {
        self.full_order()
            .into_iter()
            .filter(|i| self.nodes[i].del.is_none())
            .collect()
    }
    fn contains(&self, id: Id) -> bool {
        self.nodes.contains_key(&id)
    }
    fn merge(&mut self, o: &Self) {
        for (id, n) in &o.nodes {
            self.integrate(&(*id, n.clone()));
            if let Some(d) = n.del {
                self.tombstone(*id, d);
            }
        }
    }
    fn gc(&mut self, stable: &dyn Fn(Id) -> bool, barrier: bool) -> usize {
        if !RGA && !barrier {
            return 0; // Fugue may still be referenced as rightOrigin: see module docs
        }
        let mut removed = 0;
        loop {
            let dead: Vec<Id> = self
                .nodes
                .iter()
                .filter(|(id, n)| {
                    n.del.is_some_and(stable) && !self.has_kids(**id, 0) && !self.has_kids(**id, 1)
                })
                .map(|(id, _)| *id)
                .collect();
            if dead.is_empty() {
                return removed;
            }
            removed += dead.len();
            dead.into_iter().for_each(|d| self.unlink(d));
        }
    }
    fn total(&self) -> usize {
        self.nodes.len()
    }
    fn encode(&self, w: &mut W) {
        w.u(self.nodes.len() as u64);
        for (id, n) in &self.nodes {
            w.id(*id);
            w.id(n.parent);
            w.u(n.side as u64);
            w.u(n.ts);
            w.opt(n.del);
        }
    }
    fn decode(r: &mut R) -> Res<Self> {
        let mut t = Self::default();
        for _ in 0..r.u()? {
            let id = r.id()?;
            let n = TNode {
                parent: r.id()?,
                side: r.u()? as u8,
                ts: r.u()?,
                del: r.opt()?,
            };
            t.integrate(&(id, n));
        }
        Ok(t)
    }
}

type Pos = Vec<(u32, u32, u32)>; // (digit, site, counter) per level
#[derive(Clone, Debug, PartialEq, Default)]
struct Logoot {
    by_pos: BTreeMap<Pos, (Id, Option<Id>)>,
    pos_of: BTreeMap<Id, Pos>,
}
/// Allocate a position strictly between `p` and `q` (None = document edge). Levels get exponentially bigger digit
/// spaces and alternate "allocate near the left" / "near the right" (the LSEQ idea) so both typing directions stay shallow.
fn alloc(p: Option<&Pos>, q: Option<&Pos>, site: u32, ctr: u32, rng: &mut Rng) -> Pos {
    let mut out = Pos::new();
    let (mut pt, mut qt) = (p.is_some(), q.is_some());
    for level in 0.. {
        let base = 16u32 << level.min(20);
        let pc = if pt {
            p.unwrap().get(level).copied()
        } else {
            None
        };
        if pc.is_none() {
            pt = false;
        }
        let qc = if qt { Some(q.unwrap()[level]) } else { None };
        let (lo, hi) = (pc.map_or(0, |c| c.0), qc.map_or(base, |c| c.0));
        if hi > lo + 1 {
            let step = 1 + rng.below(((hi - lo - 1) as usize).min(10)) as u32;
            out.push((
                if level % 2 == 0 { lo + step } else { hi - step },
                site,
                ctr,
            ));
            return out;
        }
        let c = pc.unwrap_or((0, 0, 0)); // minimal padding component: never exceeds q's component
        out.push(c);
        qt = qt && qc == Some(c);
    }
    unreachable!()
}
impl Seq for Logoot {
    type Node = (Id, Pos);
    const NAME: &'static str = "Logoot";
    fn local_insert(&mut self, id: Id, idx: usize, rng: &mut Rng) -> Self::Node {
        let vis: Vec<&Pos> = self
            .by_pos
            .iter()
            .filter(|(_, v)| v.1.is_none())
            .map(|(p, _)| p)
            .collect();
        assert!(idx <= vis.len());
        let p = if idx == 0 { None } else { Some(vis[idx - 1]) };
        let q = vis.get(idx).copied();
        let pos = alloc(p, q, id.0, id.1 as u32, rng);
        let n = (id, pos);
        self.integrate(&n);
        n
    }
    fn integrate(&mut self, (id, pos): &Self::Node) {
        if self.pos_of.contains_key(id) {
            return;
        }
        self.by_pos.insert(pos.clone(), (*id, None));
        self.pos_of.insert(*id, pos.clone());
    }
    fn tombstone(&mut self, id: Id, dot: Id) -> bool {
        let Some(pos) = self.pos_of.get(&id) else {
            return false;
        };
        let e = self.by_pos.get_mut(pos).unwrap();
        e.1 = Some(e.1.map_or(dot, |d| d.min(dot)));
        true
    }
    fn visible(&self) -> Vec<Id> {
        self.by_pos
            .values()
            .filter(|v| v.1.is_none())
            .map(|v| v.0)
            .collect()
    }
    fn contains(&self, id: Id) -> bool {
        self.pos_of.contains_key(&id)
    }
    fn merge(&mut self, o: &Self) {
        for (pos, (id, del)) in &o.by_pos {
            self.integrate(&(*id, pos.clone()));
            if let Some(d) = del {
                self.tombstone(*id, *d);
            }
        }
    }
    fn gc(&mut self, stable: &dyn Fn(Id) -> bool, _barrier: bool) -> usize {
        let dead: Vec<Pos> = self
            .by_pos
            .iter()
            .filter(|(_, v)| v.1.is_some_and(stable))
            .map(|(p, _)| p.clone())
            .collect();
        for p in &dead {
            let (id, _) = self.by_pos.remove(p).unwrap();
            self.pos_of.remove(&id);
        }
        dead.len()
    }
    fn total(&self) -> usize {
        self.by_pos.len()
    }
    fn encode(&self, w: &mut W) {
        w.u(self.by_pos.len() as u64);
        for (pos, (id, del)) in &self.by_pos {
            w.id(*id);
            w.opt(*del);
            w.u(pos.len() as u64);
            for c in pos {
                w.u(c.0 as u64);
                w.u(c.1 as u64);
                w.u(c.2 as u64);
            }
        }
    }
    fn decode(r: &mut R) -> Res<Self> {
        let mut l = Self::default();
        for _ in 0..r.u()? {
            let (id, del) = (r.id()?, r.opt()?);
            let pos: Pos = (0..r.u()?)
                .map(|_| Ok((r.u()? as u32, r.u()? as u32, r.u()? as u32)))
                .collect::<Res<_>>()?;
            l.integrate(&(id, pos));
            if let Some(d) = del {
                l.tombstone(id, d);
            }
        }
        Ok(l)
    }
}

// ---------------------------------------------------------------- the todo document
#[derive(Clone, Debug)]
enum Body<N> {
    Create {
        node: N,
        title: String,
        hlc: Hlc,
    },
    Title {
        item: Id,
        val: String,
        hlc: Hlc,
    },
    Due {
        item: Id,
        val: Option<u64>,
        hlc: Hlc,
    },
    Done {
        item: Id,
        val: bool,
        observed: Vec<Id>,
        delta: i8,
    },
    Delete {
        item: Id,
        observed: Vec<Id>,
    },
    Purge {
        item: Id,
    },
}
#[derive(Clone, Debug)]
struct Op<N> {
    dot: Id,
    deps: Vv,
    body: Body<N>,
}
fn hlc_of<N>(b: &Body<N>) -> Option<Hlc> {
    match b {
        Body::Create { hlc, .. } | Body::Title { hlc, .. } | Body::Due { hlc, .. } => Some(*hlc),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
struct Doc<L: Seq> {
    items: BTreeMap<Id, Item>,
    list: L,
    plus: BTreeMap<Rep, u64>, // PN-counter halves: per-replica slots, merge = max
    minus: BTreeMap<Rep, u64>,
}
#[derive(Clone, Debug, PartialEq)]
struct Row {
    id: Id,
    title: String,
    done: bool,
    due: Option<u64>,
}
impl<L: Seq> Doc<L> {
    fn apply(&mut self, op: &Op<L::Node>) {
        let dot = op.dot;
        match &op.body {
            Body::Create { node, title, hlc } => {
                self.list.integrate(node);
                let it = self.items.entry(dot).or_default();
                it.title.set(*hlc, title);
                it.tags.insert(dot);
            }
            Body::Title { item, val, hlc } => self.edit(*item, dot, |it| it.title.set(*hlc, val)),
            Body::Due { item, val, hlc } => self.edit(*item, dot, |it| it.due.set(*hlc, val)),
            Body::Done {
                item,
                val,
                observed,
                delta,
            } => {
                self.edit(*item, dot, |it| {
                    it.done_rm.extend(observed);
                    it.done.insert(dot, *val);
                });
                match delta.signum() {
                    1 => *self.plus.entry(dot.0).or_default() += 1,
                    -1 => *self.minus.entry(dot.0).or_default() += 1,
                    _ => {}
                }
            }
            Body::Delete { item, observed } => {
                if let Some(it) = self.items.get_mut(item) {
                    it.tags_rm.extend(observed);
                    it.killed_by = Some(it.killed_by.map_or(dot, |d| d.min(dot)));
                }
            }
            Body::Purge { item } => {
                self.list.tombstone(*item, dot);
            }
        }
    }
    /// An edit also ADDS a fresh tag: this is what makes a concurrent edit beat a concurrent delete under add-wins.
    fn edit(&mut self, item: Id, dot: Id, f: impl FnOnce(&mut Item)) {
        if let Some(it) = self.items.get_mut(&item) {
            f(it);
            it.tags.insert(dot);
        }
    }
    fn merge(&mut self, o: &Self) {
        for (id, it) in &o.items {
            self.items.entry(*id).or_default().merge(it);
        }
        for (a, b) in [(&mut self.plus, &o.plus), (&mut self.minus, &o.minus)] {
            for (r, n) in b {
                let e = a.entry(*r).or_default();
                *e = (*e).max(*n);
            }
        }
        self.list.merge(&o.list);
    }
    fn view(&self, p: Policy) -> Vec<Row> {
        self.list
            .visible()
            .iter()
            .filter_map(|id| {
                self.items
                    .get(id)
                    .filter(|it| it.alive(p))
                    .map(|it| (id, it))
            })
            .map(|(id, it)| Row {
                id: *id,
                title: it.title.val.clone(),
                done: it.is_done(),
                due: it.due.val,
            })
            .collect()
    }
    fn completions(&self) -> i64 {
        self.plus.values().sum::<u64>() as i64 - self.minus.values().sum::<u64>() as i64
    }
    fn encode(&self, w: &mut W) {
        w.u(self.items.len() as u64);
        for (id, it) in &self.items {
            w.id(*id);
            w.hlc(it.title.hlc);
            w.s(&it.title.val);
            w.hlc(it.due.hlc);
            w.u(it.due.val.map_or(0, |d| d + 1));
            w.u(it.done.len() as u64);
            for (d, v) in &it.done {
                w.id(*d);
                w.u(*v as u64);
            }
            for set in [&it.done_rm, &it.tags, &it.tags_rm] {
                w.u(set.len() as u64);
                set.iter().for_each(|d| w.id(*d));
            }
            w.opt(it.killed_by);
        }
        for m in [&self.plus, &self.minus] {
            w.u(m.len() as u64);
            for (r, n) in m {
                w.u(*r as u64);
                w.u(*n);
            }
        }
        self.list.encode(w);
    }
    fn decode(r: &mut R) -> Res<Self> {
        let mut d = Doc::<L>::default();
        for _ in 0..r.u()? {
            let id = r.id()?;
            let mut it = Item {
                title: Lww {
                    hlc: r.hlc()?,
                    val: r.s()?,
                },
                ..Item::default()
            };
            let (h, v) = (r.hlc()?, r.u()?);
            it.due = Lww {
                hlc: h,
                val: v.checked_sub(1),
            };
            for _ in 0..r.u()? {
                let (k, v) = (r.id()?, r.u()? == 1);
                it.done.insert(k, v);
            }
            for which in 0..3 {
                let set: BTreeSet<Id> = (0..r.u()?).map(|_| r.id()).collect::<Res<_>>()?;
                match which {
                    0 => it.done_rm = set,
                    1 => it.tags = set,
                    _ => it.tags_rm = set,
                }
            }
            it.killed_by = r.opt()?;
            d.items.insert(id, it);
        }
        for which in 0..2 {
            let m: BTreeMap<Rep, u64> = (0..r.u()?)
                .map(|_| Ok((r.u()? as u32, r.u()?)))
                .collect::<Res<_>>()?;
            if which == 0 { d.plus = m } else { d.minus = m }
        }
        d.list = L::decode(r)?;
        Ok(d)
    }
}

// ---------------------------------------------------------------- the replica
#[derive(Clone, Debug)]
struct Msg<N> {
    from: Rep,
    to: Rep,
    ops: Vec<Op<N>>,
    vv: Vv,
}
struct Replica<L: Seq> {
    id: Rep,
    doc: Doc<L>,
    vv: Vv,
    hlc: Hlc,
    max_seen: Hlc,
    policy: Policy,
    log: BTreeMap<Rep, BTreeMap<u64, Op<L::Node>>>,
    pending: Vec<Op<L::Node>>,
    rows: BTreeMap<Rep, Vv>, // last version vector each peer reported
    w: Vv, // causal-stability watermark: everything <= w is delivered at every replica
    peers: Vec<Rep>,
    rng: Rng,
    now: u64,
    skew: u64,
    gc_removed: usize,
}
impl<L: Seq> Replica<L> {
    fn new(id: Rep, peers: Vec<Rep>, policy: Policy, seed: u64) -> Self {
        Replica {
            id,
            doc: Doc::default(),
            vv: Vv::new(),
            hlc: Hlc {
                rep: id,
                ..Hlc::default()
            },
            max_seen: Hlc::default(),
            policy,
            log: BTreeMap::new(),
            pending: vec![],
            rows: BTreeMap::new(),
            w: Vv::new(),
            peers,
            rng: Rng::new(seed ^ ((id as u64) << 32)),
            now: 1_000,
            skew: (id as u64 * 37) % 90, // each replica's wall clock is a little off
            gc_removed: 0,
        }
    }
    fn next_dot(&self) -> Id {
        (self.id, vget(&self.vv, self.id) + 1)
    }
    fn stamp(&mut self) -> Hlc {
        self.hlc.tick(self.now + self.skew);
        assert!(
            self.hlc > self.max_seen,
            "HLC must exceed every stamp this replica has seen (causality)"
        );
        self.max_seen = self.hlc;
        self.hlc
    }
    fn commit(&mut self, body: Body<L::Node>) -> Id {
        let op = Op {
            dot: self.next_dot(),
            deps: self.vv.clone(),
            body,
        };
        self.doc.apply(&op);
        self.vv.insert(self.id, op.dot.1);
        let dot = op.dot;
        self.log.entry(self.id).or_default().insert(dot.1, op);
        dot
    }
    // ---- local operations
    fn add(&mut self, idx: usize, title: &str) -> Id {
        let item = self.next_dot();
        let node = self.doc.list.local_insert(item, idx, &mut self.rng);
        let hlc = self.stamp();
        self.commit(Body::Create {
            node,
            title: title.into(),
            hlc,
        })
    }
    fn set_title(&mut self, item: Id, val: &str) {
        let hlc = self.stamp();
        self.commit(Body::Title {
            item,
            val: val.into(),
            hlc,
        });
    }
    fn set_due(&mut self, item: Id, val: Option<u64>) {
        let hlc = self.stamp();
        self.commit(Body::Due { item, val, hlc });
    }
    fn set_done(&mut self, item: Id, val: bool) {
        let it = &self.doc.items[&item];
        let delta = match (val, it.is_done()) {
            (true, false) => 1,
            (false, true) => -1,
            _ => 0,
        };
        let observed = it.done_dots();
        self.commit(Body::Done {
            item,
            val,
            observed,
            delta,
        });
    }
    fn delete(&mut self, item: Id) {
        let observed = self.doc.items[&item].live_tags();
        self.commit(Body::Delete { item, observed });
    }
    fn purge_stable_dead(&mut self) -> usize {
        let vis: BTreeSet<Id> = self.doc.list.visible().into_iter().collect();
        let w = self.w.clone();
        let dead: Vec<Id> = self
            .doc
            .items
            .iter()
            .filter(|(id, it)| {
                vis.contains(id)
                    && !it.alive(self.policy)
                    && it.killed_by.is_some_and(|d| vget(&w, d.0) >= d.1)
            })
            .map(|(id, _)| *id)
            .collect();
        for item in &dead {
            self.commit(Body::Purge { item: *item });
        }
        dead.len()
    }
    fn typing_run(&mut self, k: usize) -> Vec<Id> {
        let at = self.rng.below(self.doc.list.visible().len() + 1);
        (0..k).map(|j| self.add(at + j, "typed")).collect()
    }
    fn random_op(&mut self) {
        let rows = self.doc.view(self.policy);
        let k = self.rng.below(100);
        if rows.is_empty() || k < 35 {
            let idx = self.rng.below(self.doc.list.visible().len() + 1);
            let t = format!("t{}-{}", self.id, self.next_dot().1);
            self.add(idx, &t);
            return;
        }
        let item = rows[self.rng.below(rows.len())].id;
        match k {
            35..=59 => {
                let t = format!("e{}-{}", self.id, self.next_dot().1);
                self.set_title(item, &t)
            }
            60..=71 => {
                let d = if self.rng.chance(20) {
                    None
                } else {
                    Some(self.rng.below(30) as u64)
                };
                self.set_due(item, d)
            }
            72..=91 => self.set_done(item, !rows.iter().find(|r| r.id == item).unwrap().done),
            _ => self.delete(item),
        }
    }
    // ---- replication
    fn make_msg(&self, to: Rep) -> Msg<L::Node> {
        let empty = Vv::new();
        let belief = self.rows.get(&to).unwrap_or(&empty);
        let ops = self
            .log
            .iter()
            .flat_map(|(o, m)| m.range(vget(belief, *o) + 1..).map(|(_, op)| op.clone()))
            .collect();
        Msg {
            from: self.id,
            to,
            ops,
            vv: self.vv.clone(),
        }
    }
    fn receive(&mut self, m: &Msg<L::Node>) {
        vmax(self.rows.entry(m.from).or_default(), &m.vv);
        for op in &m.ops {
            let dup = vget(&self.vv, op.dot.0) >= op.dot.1
                || self.pending.iter().any(|p| p.dot == op.dot);
            if !dup {
                self.pending.push(op.clone());
            }
        }
        self.deliver_ready();
    }
    /// The causal buffer: deliver any pending op whose origin predecessor and dependencies are all delivered.
    fn deliver_ready(&mut self) {
        while let Some(i) = self
            .pending
            .iter()
            .position(|op| op.dot.1 == vget(&self.vv, op.dot.0) + 1 && vle(&op.deps, &self.vv))
        {
            let op = self.pending.swap_remove(i);
            if let Some(h) = hlc_of(&op.body) {
                self.hlc.observe(h, self.now + self.skew);
                self.max_seen = self.max_seen.max(h);
            }
            self.doc.apply(&op);
            self.vv.insert(op.dot.0, op.dot.1);
            self.log.entry(op.dot.0).or_default().insert(op.dot.1, op);
        }
    }
    /// Update the stability watermark, then drop whatever it allows. Returns how many list tombstones were removed.
    fn gc(&mut self, barrier: bool) -> usize {
        let everyone: Vec<Rep> = self.peers.iter().copied().chain([self.id]).collect();
        let rows_ok = everyone
            .iter()
            .all(|r| *r == self.id || self.rows.get(r).is_some_and(|v| vle(v, &self.vv)));
        if rows_ok {
            let mut w = Vv::new();
            for (origin, _) in self.vv.iter() {
                let m = everyone
                    .iter()
                    .map(|r| {
                        vget(
                            if *r == self.id {
                                &self.vv
                            } else {
                                &self.rows[r]
                            },
                            *origin,
                        )
                    })
                    .min()
                    .unwrap();
                w.insert(*origin, m);
            }
            vmax(&mut self.w, &w);
        }
        if barrier {
            self.w = self.vv.clone(); // quiescent cut: every replica holds exactly this state, so everything delivered is stable
        }
        let wm = self.w.clone();
        let stable = move |d: Id| vget(&wm, d.0) >= d.1;
        let removed = self.doc.list.gc(&stable, barrier);
        let list = &self.doc.list;
        let policy = self.policy;
        self.doc
            .items
            .retain(|id, it| it.alive(policy) || list.contains(*id));
        for it in self.doc.items.values_mut() {
            let gone: Vec<Id> = it.tags_rm.iter().filter(|t| stable(**t)).copied().collect();
            for t in gone {
                it.tags_rm.remove(&t);
                it.tags.remove(&t);
            }
            let gone: Vec<Id> = it.done_rm.iter().filter(|t| stable(**t)).copied().collect();
            for t in gone {
                it.done_rm.remove(&t);
                it.done.remove(&t);
            }
        }
        self.gc_removed += removed;
        removed
    }
    // ---- snapshot
    fn save(&self) -> Vec<u8> {
        let mut w = W(b"TODO\x01".to_vec());
        w.u(self.vv.len() as u64);
        for (r, s) in &self.vv {
            w.u(*r as u64);
            w.u(*s);
        }
        w.hlc(self.hlc);
        self.doc.encode(&mut w);
        let sum = fnv(&w.0);
        w.0.extend_from_slice(&sum.to_le_bytes());
        w.0
    }
    fn load(bytes: &[u8], id: Rep, peers: Vec<Rep>, policy: Policy, seed: u64) -> Res<Self> {
        if bytes.len() < 9 || &bytes[..5] != b"TODO\x01" {
            return Err("bad magic/version".into());
        }
        let (body, sum) = bytes.split_at(bytes.len() - 4);
        if fnv(body).to_le_bytes() != sum {
            return Err("checksum mismatch".into());
        }
        let mut r = R { b: body, p: 5 };
        let mut rep = Replica::new(id, peers, policy, seed);
        for _ in 0..r.u()? {
            let (a, b) = (r.u()? as u32, r.u()?);
            rep.vv.insert(a, b);
        }
        rep.hlc = r.hlc()?;
        rep.max_seen = rep.hlc;
        rep.doc = Doc::decode(&mut r)?;
        if r.p != body.len() {
            return Err("trailing bytes".into());
        }
        Ok(rep)
    }
}

// ---------------------------------------------------------------- simulated network
struct Sim<L: Seq> {
    reps: Vec<Replica<L>>,
    net: Vec<Msg<L::Node>>,
    side: Option<Vec<bool>>,
    rng: Rng,
    runs: Vec<Vec<Id>>,
    lost: usize,
    duplicated: usize,
}
impl<L: Seq> Sim<L> {
    fn new(seed: u64, n: usize, policy: Policy) -> Self {
        let ids: Vec<Rep> = (1..=n as u32).collect();
        let reps = ids
            .iter()
            .map(|i| {
                Replica::new(
                    *i,
                    ids.iter().copied().filter(|p| p != i).collect(),
                    policy,
                    seed,
                )
            })
            .collect();
        Sim {
            reps,
            net: vec![],
            side: None,
            rng: Rng::new(seed ^ 0xABCD),
            runs: vec![],
            lost: 0,
            duplicated: 0,
        }
    }
    fn blocked(&self, a: usize, b: usize) -> bool {
        self.side.as_ref().is_some_and(|s| s[a] != s[b])
    }
    fn send(&mut self, a: usize, b: usize) {
        let m = self.reps[a].make_msg(self.reps[b].id);
        if self.blocked(a, b) || self.rng.chance(10) {
            self.lost += 1;
            return;
        }
        if self.rng.chance(15) {
            self.net.push(m.clone());
            self.duplicated += 1;
        }
        self.net.push(m);
    }
    fn deliver_one(&mut self) {
        if self.net.is_empty() {
            return;
        }
        let i = self.rng.below(self.net.len()); // random pick = arbitrary reordering
        let m = self.net.swap_remove(i);
        let to = self.reps.iter().position(|r| r.id == m.to).unwrap();
        self.reps[to].receive(&m);
    }
    fn step(&mut self) {
        let n = self.reps.len();
        let a = self.rng.below(n);
        for r in &mut self.reps {
            r.now += 1 + (self.rng.next() % 3);
        }
        match self.rng.below(100) {
            0..=44 => self.reps[a].random_op(),
            45..=52 => {
                let k = 2 + self.rng.below(4);
                let run = self.reps[a].typing_run(k);
                self.runs.push(run);
            }
            53..=78 => {
                let b = (a + 1 + self.rng.below(n - 1)) % n;
                self.send(a, b);
            }
            79..=92 => self.deliver_one(),
            93..=95 => {
                self.side = if self.side.is_some() {
                    None
                } else {
                    Some((0..n).map(|_| self.rng.chance(50)).collect())
                };
            }
            96..=97 => {
                self.reps[a].gc(false);
            }
            _ => {
                self.reps[a].purge_stable_dead();
            }
        }
    }
    /// Heal the partition and run reliable all-pairs anti-entropy until every replica has everything.
    fn drain(&mut self) {
        self.side = None;
        self.net.clear();
        for round in 0..20 {
            for a in 0..self.reps.len() {
                for b in 0..self.reps.len() {
                    if a != b {
                        let m = self.reps[a].make_msg(self.reps[b].id);
                        self.reps[b].receive(&m);
                    }
                }
            }
            let v0 = &self.reps[0].vv;
            if round > 0
                && self
                    .reps
                    .iter()
                    .all(|r| &r.vv == v0 && r.pending.is_empty())
            {
                return;
            }
        }
        panic!("did not converge in 20 rounds");
    }
    fn assert_converged(&self, what: &str) {
        let (r0, p) = (&self.reps[0], self.reps[0].policy);
        for r in &self.reps[1..] {
            assert_eq!(
                r0.doc.view(p),
                r.doc.view(p),
                "{what}: views diverged between replica {} and {}",
                r0.id,
                r.id
            );
            assert_eq!(
                r0.doc.list.visible(),
                r.doc.list.visible(),
                "{what}: list order diverged"
            );
            assert_eq!(
                r0.doc.completions(),
                r.doc.completions(),
                "{what}: counter diverged"
            );
        }
        let vis = r0.doc.list.visible();
        assert_eq!(
            vis.len(),
            vis.iter().collect::<BTreeSet<_>>().len(),
            "{what}: duplicate element in list"
        );
    }
    /// Every replica's sequential typing run keeps its internal order in the final list (intention preservation).
    fn assert_runs_ordered(&self) {
        let vis = self.reps[0].doc.list.visible();
        for run in &self.runs {
            let pos: Vec<usize> = run
                .iter()
                .filter_map(|id| vis.iter().position(|x| x == id))
                .collect();
            assert!(
                pos.windows(2).all(|w| w[0] < w[1]),
                "typing run order violated: {run:?} at {pos:?}"
            );
        }
    }
    fn tombstones(&self) -> usize {
        self.reps
            .iter()
            .map(|r| r.doc.list.total() - r.doc.list.visible().len())
            .sum()
    }
}

fn scenario<L: Seq>(seed: u64) -> (usize, usize, usize) {
    let mut rng = Rng::new(seed);
    let n = 3 + rng.below(3);
    let policy = if rng.chance(50) {
        Policy::AddWins
    } else {
        Policy::RemoveWins
    };
    let mut s = Sim::<L>::new(seed, n, policy);
    for _ in 0..60 + rng.below(80) {
        s.step();
    }
    s.drain();
    s.assert_converged("phase 1");
    s.assert_runs_ordered();
    // Deleted items become list tombstones once their delete is causally stable (watermark needs fresh reports: drain twice).
    s.drain();
    s.reps.iter_mut().for_each(|r| {
        r.gc(false);
    });
    s.reps[0].purge_stable_dead();
    s.drain();
    s.assert_converged("after purge");
    s.drain();
    // Second phase: a barrier GC on identical replicas, then more concurrent work, must still converge.
    let before = s.tombstones();
    s.reps.iter_mut().for_each(|r| {
        r.gc(true);
    });
    let after = s.tombstones();
    // Replicas collected garbage at different times above; after a common barrier the WHOLE state (items, registers, tags, list nodes,
    // counter slots), not just the rendered view, must be identical.
    for r in &s.reps[1..] {
        assert!(
            r.doc == s.reps[0].doc,
            "full state diverged after barrier GC (seed {seed}, replica {})",
            r.id
        );
    }
    for _ in 0..40 {
        s.step();
    }
    s.drain();
    s.assert_converged("phase 2 (after GC)");
    s.assert_runs_ordered();
    // Idempotence of delivery: replaying every message again changes nothing.
    let snap = s.reps[0].doc.clone();
    for a in 1..n {
        let mut m = s.reps[a].make_msg(s.reps[0].id);
        m.ops = s.reps[a]
            .log
            .values()
            .flat_map(|l| l.values().cloned())
            .collect();
        s.reps[0].receive(&m);
    }
    assert_eq!(
        snap, s.reps[0].doc,
        "replaying the whole log must be a no-op"
    );
    (before, after, s.reps[0].doc.list.visible().len())
}

// ---------------------------------------------------------------- merge laws on whole documents
/// Returns how many list tombstones the three divergent documents held, so the caller can check the laws were not tested on delete-free lists.
fn law_trial<L: Seq>(seed: u64) -> usize {
    let mut s = Sim::<L>::new(seed, 3, Policy::AddWins);
    for _ in 0..8 {
        s.reps[0].random_op();
    }
    s.drain();
    // Make two deletes causally stable, so that the divergent replicas below can purge them (that is what creates list tombstones).
    for _ in 0..2 {
        let rows = s.reps[0].doc.view(Policy::AddWins);
        if let Some(row) = rows.get(s.rng.below(rows.len())) {
            s.reps[0].delete(row.id);
        }
    }
    s.drain();
    s.drain();
    s.reps.iter_mut().for_each(|r| {
        r.gc(false);
    });
    for r in &mut s.reps {
        for _ in 0..3 + (r.rng.below(6)) {
            r.random_op();
        }
        if r.rng.chance(50) {
            r.purge_stable_dead();
        }
    }
    let [a, b, c] = [0, 1, 2].map(|i| s.reps[i].doc.clone());
    let join = |x: &Doc<L>, y: &Doc<L>| {
        let mut z = x.clone();
        z.merge(y);
        z
    };
    assert_eq!(join(&a, &a), a, "idempotent");
    assert_eq!(join(&a, &b), join(&b, &a), "commutative");
    assert_eq!(
        join(&join(&a, &b), &c),
        join(&a, &join(&b, &c)),
        "associative"
    );
    // state-merge agrees with op delivery: merged state equals the converged replica state
    let merged = join(&join(&a, &b), &c);
    s.drain();
    assert_eq!(
        merged.view(Policy::AddWins),
        s.reps[0].doc.view(Policy::AddWins),
        "state join == op delivery"
    );
    assert_eq!(
        merged.list.visible(),
        s.reps[0].doc.list.visible(),
        "state join == op delivery (list order and tombstones)"
    );
    [a, b, c]
        .iter()
        .map(|d| d.list.total() - d.list.visible().len())
        .sum()
}

// ---------------------------------------------------------------- interleaving experiment
/// Two replicas concurrently type a run of `len` characters between the same two neighbours. Returns true if the final
/// order interleaves the runs (a run is not contiguous). `backward` = each new character is inserted at the SAME index.
fn interleave_trial<L: Seq>(seed: u64, backward: bool, len: usize) -> bool {
    let mut rng = Rng::new(seed);
    let (mut base, mut a, mut b) = (L::default(), L::default(), L::default());
    let (x, y) = ((0, 1), (0, 2));
    let nx = base.local_insert(x, 0, &mut rng);
    let ny = base.local_insert(y, 1, &mut rng);
    for l in [&mut a, &mut b] {
        l.integrate(&nx);
        l.integrate(&ny);
    }
    let run = |l: &mut L, rep: Rep, rng: &mut Rng| -> (Vec<Id>, Vec<L::Node>) {
        let ids: Vec<Id> = (0..len as u64).map(|k| (rep, k + 1)).collect();
        let nodes = ids
            .iter()
            .enumerate()
            .map(|(k, id)| l.local_insert(*id, if backward { 1 } else { 1 + k }, rng))
            .collect();
        (ids, nodes)
    };
    let (ra, na) = run(&mut a, 1, &mut rng);
    let (rb, nb) = run(&mut b, 2, &mut rng);
    na.iter().for_each(|n| b.integrate(n));
    nb.iter().for_each(|n| a.integrate(n));
    assert_eq!(a.visible(), b.visible(), "{}: replicas diverged", L::NAME);
    let vis = a.visible();
    let contiguous = |r: &Vec<Id>| {
        let p: Vec<usize> = r
            .iter()
            .map(|i| vis.iter().position(|x| x == i).unwrap())
            .collect();
        p.iter().max().unwrap() - p.iter().min().unwrap() == len - 1
    };
    !(contiguous(&ra) && contiguous(&rb))
}
fn interleave_stats<L: Seq>() -> (usize, usize) {
    let (mut fwd, mut bwd) = (0, 0);
    for seed in 0..300 {
        fwd += interleave_trial::<L>(seed, false, 3 + (seed as usize % 4)) as usize;
        bwd += interleave_trial::<L>(seed, true, 3 + (seed as usize % 4)) as usize;
    }
    (fwd, bwd)
}

// ---------------------------------------------------------------- demos with fixed expectations
fn add_wins_vs_remove_wins() {
    for (policy, expect_alive) in [(Policy::AddWins, true), (Policy::RemoveWins, false)] {
        let mut s = Sim::<Fugue>::new(1, 3, policy);
        let item = s.reps[0].add(0, "buy milk");
        s.drain();
        s.reps[1].delete(item); // replica 2 deletes ...
        s.reps[2].set_title(item, "buy oat milk"); // ... while replica 3 concurrently edits
        s.drain();
        s.assert_converged("add/remove");
        let alive = !s.reps[0].doc.view(policy).is_empty();
        println!(
            "  {policy:?}: concurrent delete + edit -> item {}",
            if alive {
                "SURVIVES (edit re-added it)"
            } else {
                "is DELETED"
            }
        );
        assert_eq!(alive, expect_alive);
    }
}
fn done_register_and_counter() {
    let mut s = Sim::<Rga>::new(2, 2, Policy::AddWins);
    let item = s.reps[0].add(0, "ship it");
    s.drain();
    s.reps[0].set_done(item, true);
    s.reps[1].set_done(item, true); // both mark done concurrently
    s.drain();
    let done = s.reps[0].doc.view(Policy::AddWins)[0].done;
    println!(
        "  two concurrent 'done' toggles: done={done}, completion-event counter={} (events, not items!)",
        s.reps[0].doc.completions()
    );
    assert!(done);
    assert_eq!(s.reps[0].doc.completions(), 2);
    s.reps[0].set_done(item, false);
    s.reps[1].set_done(item, true);
    s.drain();
    println!(
        "  concurrent un-done vs done: multi-value register keeps both, read policy says done={}",
        s.reps[0].doc.view(Policy::AddWins)[0].done
    );
    assert!(s.reps[0].doc.view(Policy::AddWins)[0].done);
    s.assert_converged("done");
}
fn hlc_demo() {
    let mut fast = Hlc {
        rep: 1,
        ..Hlc::default()
    };
    let mut slow = Hlc {
        rep: 2,
        ..Hlc::default()
    };
    fast.tick(10_000);
    let sent = fast;
    slow.observe(sent, 100); // slow's wall clock is 9900 behind
    slow.tick(101);
    assert!(
        slow > sent,
        "a causally later event must have a bigger HLC even with a lagging wall clock"
    );
    println!("  HLC: send={sent:?} -> later event on a lagging clock={slow:?} (still larger)");
}
/// The race that causal stability exists for: replica 1 purges X and collects garbage while replica 2, which has not seen the purge yet,
/// types right after X. If the tombstone were dropped early, the new node would arrive naming an origin that no longer exists.
fn gc_waits_for_stability<L: Seq>() {
    let mut s = Sim::<L>::new(4, 2, Policy::AddWins);
    let x = s.reps[0].add(0, "x");
    s.drain();
    s.reps[0].delete(x);
    s.drain();
    s.drain(); // second round: now every replica has REPORTED that it saw the delete
    s.reps[0].gc(false);
    assert_eq!(
        s.reps[0].purge_stable_dead(),
        1,
        "the delete is stable, so X can be purged from the list"
    );
    assert_eq!(
        s.reps[1].doc.list.visible(),
        vec![x],
        "replica 2 has not seen the purge"
    );
    let child = s.reps[1].add(1, "typed after x");
    assert_eq!(
        s.reps[0].gc(false),
        0,
        "{}: the purge is not stable yet, so the tombstone must be kept",
        L::NAME
    );
    s.drain();
    s.assert_converged("gc race");
    assert_eq!(
        s.reps[0].doc.list.visible(),
        vec![child],
        "{}: the concurrent insert must survive",
        L::NAME
    );
}

fn snapshot_demo() {
    let mut s = Sim::<Fugue>::new(9, 3, Policy::AddWins);
    for _ in 0..80 {
        s.step();
    }
    s.drain();
    let bytes = s.reps[0].save();
    let loaded = Replica::<Fugue>::load(&bytes, 1, vec![2, 3], Policy::AddWins, 9).unwrap();
    assert_eq!(loaded.doc, s.reps[0].doc);
    assert_eq!(loaded.vv, s.reps[0].vv);
    let mut bad = bytes.clone();
    let mid = bad.len() / 2;
    bad[mid] ^= 0x40;
    assert!(
        Replica::<Fugue>::load(&bad, 1, vec![2, 3], Policy::AddWins, 9).is_err(),
        "corruption must be detected"
    );
    assert!(
        Replica::<Fugue>::load(&bytes[..bytes.len() - 7], 1, vec![2, 3], Policy::AddWins, 9)
            .is_err(),
        "truncation must be detected"
    );
    // restart replica 1 from its snapshot (everything was replicated), keep working, converge.
    s.reps[0] = loaded;
    for _ in 0..60 {
        s.step();
    }
    s.drain();
    s.assert_converged("after restart from snapshot");
    println!(
        "  snapshot: {} bytes for {} items; round trip, corruption and truncation checks OK",
        bytes.len(),
        s.reps[0].doc.items.len()
    );
}

fn main() {
    println!("Todo CRDT without a database");
    println!("HLC");
    hlc_demo();
    println!("Add-wins vs remove-wins");
    add_wins_vs_remove_wins();
    println!("Multi-value register + PN-counter");
    done_register_and_counter();
    println!("Snapshot save/load");
    snapshot_demo();
    println!("Garbage collection vs an in-flight insert");
    gc_waits_for_stability::<Rga>();
    gc_waits_for_stability::<Logoot>();
    gc_waits_for_stability::<Fugue>();
    println!("  tombstone kept until its purge is causally stable (RGA, Logoot, Fugue)");

    fn run_all<L: Seq>() {
        let t = std::time::Instant::now();
        let (mut before, mut after, mut len) = (0, 0, 0);
        for seed in 0..1000 {
            let (b, a, l) = scenario::<L>(seed * 7919 + 13);
            before += b;
            after += a;
            len += l;
        }
        let law_tombstones: usize = (0..300).map(|seed| law_trial::<L>(seed + 5)).sum();
        assert!(
            law_tombstones > 100,
            "merge laws must be exercised on lists that contain deletes, got {law_tombstones} tombstones"
        );
        println!(
            "  {:<7} 1000 scenarios converged (3-5 replicas, loss/dup/reorder/partition); merge laws x300 OK; tombstones before/after barrier GC: {before}/{after}; avg list len {:.1}; {:?}",
            L::NAME,
            len as f64 / 1000.0,
            t.elapsed()
        );
        assert!(after <= before && before > 0, "GC must not add tombstones");
    }
    println!("Strong eventual consistency (3000 scenarios total)");
    run_all::<Rga>();
    run_all::<Logoot>();
    run_all::<Fugue>();

    println!(
        "Interleaving of two concurrent typing runs (300 trials each, forward / backward typing)"
    );
    let (rf, rb) = interleave_stats::<Rga>();
    let (lf, lb) = interleave_stats::<Logoot>();
    let (ff, fb) = interleave_stats::<Fugue>();
    println!("  RGA    interleaved: forward {rf:3}  backward {rb:3}");
    println!("  Logoot interleaved: forward {lf:3}  backward {lb:3}");
    println!("  Fugue  interleaved: forward {ff:3}  backward {fb:3}");
    assert_eq!((ff, fb), (0, 0), "Fugue must keep every run contiguous");
    assert!(rb > 0, "RGA is known to interleave backward typing");
    assert!(lf + lb > 0, "Logoot with random allocation interleaves");
    println!("OK");
}
