//! RECIPE 801 (distributed): a collaborative plain-text engine: Fugue (and a YATA/Yjs-style integrate to compare) with an O(log n) order-statistic treap, run-length ops, stable cursors, undo/redo and a diff adapter.
//! LEVEL: advanced
//! Use in: the core of a Google-Docs-like or code-editor collaboration feature.   Run: cargo run --release --example crdt_fugue_text_editor
//! COMMON USES: collaborative text editing (Yjs/Automerge style engines), shared notes, in-browser IDE pair programming, local-first apps.
//! Course: Weidner & Kleppmann, "The Art of the Fugue: Minimizing Interleaving in Collaborative Text Editing"; Yjs internals (YATA, Nicolaescu et al.).
#![allow(dead_code)]
//!
//! WHAT: every character is a node with a unique id (replica, counter). Two list CRDTs decide where a concurrent insert lands:
//!  * FUGUE: the node names a PARENT and a SIDE (left/right child). Rule for a local insert after visible char L: if L has no right child, be
//!    L's right child; otherwise be a LEFT child of the next node in the full traversal (tombstones included). Siblings sort by id.
//!    The document is the in-order traversal (left kids, node, right kids). That construction is what makes two authors' runs stay contiguous.
//!  * YATA (Yjs): the node names a LEFT and a RIGHT origin; `Yata::integrate` scans the gap between them with the conflict rules of Yjs.
//! WHY a treap: an editor speaks "index 17", a CRDT speaks "after id (3,9)". `Treap` is an arena (Vec) of nodes with subtree counts
//! (total and visible) and parent pointers, so index -> node (`by_visible`) and node -> index (`rank_*`) are both O(log n), and so is insertion
//! at a position (rotations restore the heap property). Remote Fugue inserts find their slot from the sibling set: after the LAST node of the
//! previous sibling's subtree (`FugueDoc::last`), or before the first node of the next one. Local inserts use the shortcut and assert it agrees.
//! HOW (the editor pains):
//!  * runs: consecutive typing is one `Run{id, parent, text}` on the wire instead of one op per char (`batch_runs`), deletes compact into id ranges
//!    (`compact_deletes`); `overhead_report` prints bytes per character for both encodings,
//!  * cursors are `Anchor`s (an id plus left/right stickiness), not integers, so remote edits move them correctly (`index_of`),
//!  * undo/redo are per user and use INVERSE ops (undo of an insert deletes those ids; undo of a delete re-inserts the text at an anchor),
//!  * `Peer::set_text` takes any new string, computes an LCS diff with prefix/suffix trimming, and emits minimal inserts/deletes.
//! Delivery is causal-by-dependency: an op waits in `pending` until every id it references exists; duplicates are ignored.
//! Honest limits: no garbage collection. Local editing and cursors are O(log n) (checked: no full-document walk), but placing a node that has
//! earlier siblings calls `FugueDoc::last`, which walks a right spine (O(length of a typed run); run-based storage like Yjs's items avoids that),
//! and `first` does the same on a left spine. `set_text` (O(old * new) LCS table on the differing middle), the YATA engine and the causal buffer's
//! retry loop are not O(log n). Tombstones stay one treap node each: run-length compaction is applied to the WIRE form only.
//! This is basic Fugue, not FugueMax. FugueMax orders right-side siblings by their right origins (reverse order, ids break ties) and is the variant
//! the paper proves "maximally non-interleaving"; basic Fugue may interleave more than necessary where some interleaving is unavoidable.
//! Read: Fugue paper https://arxiv.org/abs/2305.00583 ; Yjs https://github.com/yjs/yjs ; YATA paper "Near Real-Time Peer-to-Peer Shared Editing on Extensible Data Types" (title only, not fetched).
//! TRY NEXT:
//!   - [S] `Treap::insert_at` uses random priorities from `Treap::rng`. Feed it sorted positions (always append) and print the tree height with `Treap::height`: why does it stay O(log n)?
//!   - [S] In `Yata::integrate`, delete the `before.contains(&other.lo)` branch and run `convergence_suite::<Yata>`: which seed diverges first, and what does that teach about the rule?
//!   - [M] Predict, then measure: change `batch_runs` to also merge a run when `side == 0` chains (backward typing). Does `overhead_report` improve, and why can Fugue not store backward typing as one run?
//!   - [M] Make `FugueDoc::last` O(1) by caching the last node per subtree root and invalidating it in `apply_ins`; keep `treap_vs_vec_oracle` and the convergence suite green.
//!   - [A] PROJECT: ship the ops of a `Peer` over the length-prefixed protocol from `record_batch_codec` and store the treap state with the snapshot idea of `crdt_todo_app_no_db`, so a restarted editor resumes without a database.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

type Rep = u32;
type Id = (Rep, u64);
const ROOT: Id = (u32::MAX, 0); // "start of document" / "no parent"
const END: Id = (u32::MAX, 1); // "end of document" (right origin)

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

// ------------------------------------------------------------------ order-statistic treap in an arena
const NIL: u32 = u32::MAX;
#[derive(Clone, Debug)]
struct TN {
    l: u32,
    r: u32,
    p: u32,
    pri: u32,
    tot: u32,
    vis: u32,
    del: bool,
    id: Id,
    ch: char,
}
struct Treap {
    n: Vec<TN>,
    root: u32,
    seed: u64,
}
impl Default for Treap {
    fn default() -> Self {
        Treap::new()
    }
}
impl Treap {
    fn new() -> Treap {
        Treap {
            n: vec![],
            root: NIL,
            seed: 0x1234_5678_9ABC_DEF1,
        }
    }
    fn rng(&mut self) -> u32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 16) as u32
    }
    fn tot(&self, i: u32) -> u32 {
        if i == NIL { 0 } else { self.n[i as usize].tot }
    }
    fn vis(&self, i: u32) -> u32 {
        if i == NIL { 0 } else { self.n[i as usize].vis }
    }
    fn pull(&mut self, i: u32) {
        let (l, r) = (self.n[i as usize].l, self.n[i as usize].r);
        let (t, v) = (
            1 + self.tot(l) + self.tot(r),
            (!self.n[i as usize].del) as u32 + self.vis(l) + self.vis(r),
        );
        let x = &mut self.n[i as usize];
        x.tot = t;
        x.vis = v;
    }
    fn len_total(&self) -> usize {
        self.tot(self.root) as usize
    }
    fn len_visible(&self) -> usize {
        self.vis(self.root) as usize
    }
    /// Insert a new node so that it ends up at total index `pos` (0 = first). Returns its arena handle.
    fn insert_at(&mut self, pos: usize, id: Id, ch: char) -> u32 {
        assert!(pos <= self.len_total());
        let new = self.n.len() as u32;
        let pri = self.rng();
        self.n.push(TN {
            l: NIL,
            r: NIL,
            p: NIL,
            pri,
            tot: 1,
            vis: 1,
            del: false,
            id,
            ch,
        });
        if self.root == NIL {
            self.root = new;
            return new;
        }
        let (mut x, mut pos) = (self.root, pos as u32);
        loop {
            let tl = self.tot(self.n[x as usize].l);
            self.n[x as usize].tot += 1;
            self.n[x as usize].vis += 1;
            let go_left = pos <= tl;
            if !go_left {
                pos -= tl + 1;
            }
            let child = if go_left {
                self.n[x as usize].l
            } else {
                self.n[x as usize].r
            };
            if child == NIL {
                if go_left {
                    self.n[x as usize].l = new
                } else {
                    self.n[x as usize].r = new
                }
                self.n[new as usize].p = x;
                break;
            }
            x = child;
        }
        while self.n[new as usize].p != NIL
            && self.n[new as usize].pri > self.n[self.n[new as usize].p as usize].pri
        {
            self.rotate_up(new);
        }
        new
    }
    fn rotate_up(&mut self, x: u32) {
        let p = self.n[x as usize].p;
        let g = self.n[p as usize].p;
        if self.n[p as usize].l == x {
            let b = self.n[x as usize].r;
            self.n[p as usize].l = b;
            if b != NIL {
                self.n[b as usize].p = p
            }
            self.n[x as usize].r = p;
        } else {
            let b = self.n[x as usize].l;
            self.n[p as usize].r = b;
            if b != NIL {
                self.n[b as usize].p = p
            }
            self.n[x as usize].l = p;
        }
        self.n[p as usize].p = x;
        self.n[x as usize].p = g;
        if g == NIL {
            self.root = x;
        } else if self.n[g as usize].l == p {
            self.n[g as usize].l = x;
        } else {
            self.n[g as usize].r = x;
        }
        self.pull(p);
        self.pull(x);
    }
    fn by_total(&self, mut pos: usize) -> Option<u32> {
        let mut x = self.root;
        while x != NIL {
            let tl = self.tot(self.n[x as usize].l) as usize;
            if pos < tl {
                x = self.n[x as usize].l;
            } else if pos == tl {
                return Some(x);
            } else {
                pos -= tl + 1;
                x = self.n[x as usize].r;
            }
        }
        None
    }
    fn by_visible(&self, mut k: usize) -> Option<u32> {
        let mut x = self.root;
        while x != NIL {
            let nx = &self.n[x as usize];
            let vl = self.vis(nx.l) as usize;
            if k < vl {
                x = nx.l;
            } else if !nx.del && k == vl {
                return Some(x);
            } else {
                k -= vl + (!nx.del) as usize;
                x = nx.r;
            }
        }
        None
    }
    fn rank_total(&self, i: u32) -> usize {
        self.rank(i, |t, n| t.tot(n) as usize, |_| 1)
    }
    /// Number of visible nodes strictly before `i`.
    fn rank_visible(&self, i: u32) -> usize {
        self.rank(i, |t, n| t.vis(n) as usize, |x| (!x.del) as usize)
    }
    fn rank(
        &self,
        i: u32,
        cnt: impl Fn(&Treap, u32) -> usize,
        own: impl Fn(&TN) -> usize,
    ) -> usize {
        let mut pos = cnt(self, self.n[i as usize].l);
        let mut c = i;
        while self.n[c as usize].p != NIL {
            let p = self.n[c as usize].p;
            if self.n[p as usize].r == c {
                pos += cnt(self, self.n[p as usize].l) + own(&self.n[p as usize]);
            }
            c = p;
        }
        pos
    }
    fn set_deleted(&mut self, i: u32) {
        if self.n[i as usize].del {
            return;
        }
        self.n[i as usize].del = true;
        let mut c = i;
        while c != NIL {
            self.pull(c);
            c = self.n[c as usize].p;
        }
    }
    fn in_order(&self) -> Vec<u32> {
        let (mut out, mut st, mut x) = (vec![], vec![], self.root);
        while x != NIL || !st.is_empty() {
            while x != NIL {
                st.push(x);
                x = self.n[x as usize].l;
            }
            let y = st.pop().unwrap();
            out.push(y);
            x = self.n[y as usize].r;
        }
        out
    }
    fn height(&self) -> usize {
        fn h(t: &Treap, x: u32) -> usize {
            if x == NIL {
                0
            } else {
                1 + h(t, t.n[x as usize].l).max(h(t, t.n[x as usize].r))
            }
        }
        h(self, self.root)
    }
}

// ------------------------------------------------------------------ operations and the engine trait
#[derive(Clone, Debug, PartialEq)]
enum Op {
    /// `a` = Fugue parent / YATA left origin, `b` = YATA right origin (unused by Fugue), `side` = Fugue child side.
    Ins {
        id: Id,
        a: Id,
        b: Id,
        side: u8,
        ch: char,
    },
    Del {
        target: Id,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Stick {
    Left,  // cursor sits right AFTER `id` (None = document start)
    Right, // cursor sits right BEFORE `id` (None = document end)
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Anchor {
    id: Option<Id>,
    stick: Stick,
}

trait Engine: Default {
    const NAME: &'static str;
    fn has(&self, id: Id) -> bool;
    /// Apply an op; returns false if a referenced id is missing (the caller buffers it). Idempotent.
    fn apply(&mut self, op: &Op) -> bool;
    fn local_insert(&mut self, id: Id, idx: usize, ch: char) -> Op;
    fn local_delete_id(&mut self, id: Id) -> Option<Op>;
    fn visible_ids(&self) -> Vec<Id>;
    fn text(&self) -> String;
    fn index_of(&self, a: &Anchor) -> usize;
    fn len(&self) -> usize {
        self.visible_ids().len()
    }
    /// Ids and chars of the visible range [idx, idx + n).
    fn chars_at(&self, idx: usize, n: usize) -> Vec<(Id, char)>;
    /// Panics if the engine's index structure disagrees with the CRDT's own definition of the order.
    fn check_structure(&self) {}
    fn anchor_at(&self, idx: usize, stick: Stick) -> Anchor {
        let v = self.visible_ids();
        let id = match stick {
            Stick::Left => idx.checked_sub(1).map(|i| v[i]),
            Stick::Right => v.get(idx).copied(),
        };
        Anchor { id, stick }
    }
}

// ------------------------------------------------------------------ Fugue on the treap
#[derive(Default)]
struct FugueDoc {
    tp: Treap,
    h: HashMap<Id, u32>,
    kids: BTreeMap<(Id, u8), BTreeSet<Id>>,
    /// How many times the whole document was walked. Editing paths must not move it (`local_edits_do_not_scan`).
    full_scans: std::cell::Cell<u64>,
}
impl FugueDoc {
    fn handle(&self, id: Id) -> u32 {
        self.h[&id]
    }
    fn last(&self, mut x: Id) -> Id {
        while let Some(k) = self.kids.get(&(x, 1)).and_then(|s| s.iter().next_back()) {
            x = *k;
        }
        x
    }
    fn first(&self, mut x: Id) -> Id {
        while let Some(k) = self.kids.get(&(x, 0)).and_then(|s| s.iter().next()) {
            x = *k;
        }
        x
    }
    /// Total index where a node (parent, side, id) must be spliced in: siblings are ordered by id and each owns a contiguous subtree.
    fn slot(&self, parent: Id, side: u8, id: Id) -> usize {
        let prev = self
            .kids
            .get(&(parent, side))
            .and_then(|s| s.range(..id).next_back().copied());
        if let Some(p) = prev {
            return self.tp.rank_total(self.handle(self.last(p))) + 1;
        }
        if side == 1 {
            return if parent == ROOT {
                0
            } else {
                self.tp.rank_total(self.handle(parent)) + 1
            };
        }
        match self.kids.get(&(parent, 0)).and_then(|s| s.iter().next()) {
            Some(min) => self.tp.rank_total(self.handle(self.first(*min))),
            None => self.tp.rank_total(self.handle(parent)),
        }
    }
    fn has_right_kid(&self, id: Id) -> bool {
        self.kids.contains_key(&(id, 1))
    }
}
impl Engine for FugueDoc {
    const NAME: &'static str = "Fugue";
    fn has(&self, id: Id) -> bool {
        self.h.contains_key(&id)
    }
    fn apply(&mut self, op: &Op) -> bool {
        match op {
            Op::Del { target } => match self.h.get(target) {
                Some(&i) => {
                    self.tp.set_deleted(i);
                    true
                }
                None => false,
            },
            Op::Ins {
                id, a, side, ch, ..
            } => {
                if self.h.contains_key(id) {
                    return true;
                }
                if *a != ROOT && !self.h.contains_key(a) {
                    return false;
                }
                let pos = self.slot(*a, *side, *id);
                let i = self.tp.insert_at(pos, *id, *ch);
                self.h.insert(*id, i);
                self.kids.entry((*a, *side)).or_default().insert(*id);
                true
            }
        }
    }
    fn local_insert(&mut self, id: Id, idx: usize, ch: char) -> Op {
        let left = if idx == 0 {
            ROOT
        } else {
            self.tp.n[self.tp.by_visible(idx - 1).unwrap() as usize].id
        };
        let (parent, side, fast_pos) = if !self.has_right_kid(left) {
            (
                left,
                1,
                if left == ROOT {
                    0
                } else {
                    self.tp.rank_total(self.handle(left)) + 1
                },
            )
        } else {
            // rightOrigin = next node in the FULL order (tombstones count); it has no left kids, so we go to its left.
            let pos = if left == ROOT {
                0
            } else {
                self.tp.rank_total(self.handle(left)) + 1
            };
            let right = self.tp.n[self.tp.by_total(pos).unwrap() as usize].id;
            (right, 0, self.tp.rank_total(self.handle(right)))
        };
        let op = Op::Ins {
            id,
            a: parent,
            b: END,
            side,
            ch,
        };
        assert_eq!(
            self.slot(parent, side, id),
            fast_pos,
            "local shortcut and general slot rule must agree"
        );
        assert!(self.apply(&op));
        op
    }
    fn local_delete_id(&mut self, id: Id) -> Option<Op> {
        let i = *self.h.get(&id)?;
        if self.tp.n[i as usize].del {
            return None;
        }
        self.tp.set_deleted(i);
        Some(Op::Del { target: id })
    }
    fn visible_ids(&self) -> Vec<Id> {
        self.full_scans.set(self.full_scans.get() + 1);
        self.tp
            .in_order()
            .into_iter()
            .filter(|i| !self.tp.n[*i as usize].del)
            .map(|i| self.tp.n[i as usize].id)
            .collect()
    }
    fn text(&self) -> String {
        self.full_scans.set(self.full_scans.get() + 1);
        self.tp
            .in_order()
            .into_iter()
            .filter(|i| !self.tp.n[*i as usize].del)
            .map(|i| self.tp.n[i as usize].ch)
            .collect()
    }
    fn len(&self) -> usize {
        self.tp.len_visible()
    }
    fn chars_at(&self, idx: usize, n: usize) -> Vec<(Id, char)> {
        (idx..idx + n)
            .map(|k| &self.tp.n[self.tp.by_visible(k).expect("range inside the document") as usize])
            .map(|x| (x.id, x.ch))
            .collect()
    }
    fn anchor_at(&self, idx: usize, stick: Stick) -> Anchor {
        let k = match stick {
            Stick::Left => idx.checked_sub(1),
            Stick::Right => Some(idx),
        };
        Anchor {
            id: k
                .and_then(|k| self.tp.by_visible(k))
                .map(|h| self.tp.n[h as usize].id),
            stick,
        }
    }
    /// The treap is only an index: its order must equal the Fugue definition (left subtrees, node, right subtrees, siblings by id).
    fn check_structure(&self) {
        enum Fr {
            Kids(Id, u8),
            Node(Id),
            Emit(Id),
        }
        let (mut want, mut st) = (vec![], vec![Fr::Kids(ROOT, 1)]);
        while let Some(f) = st.pop() {
            match f {
                Fr::Emit(id) => want.push(id),
                Fr::Node(id) => st.extend([Fr::Kids(id, 1), Fr::Emit(id), Fr::Kids(id, 0)]),
                Fr::Kids(p, side) => st.extend(
                    self.kids
                        .get(&(p, side))
                        .into_iter()
                        .flatten()
                        .rev()
                        .map(|k| Fr::Node(*k)),
                ),
            }
        }
        let got: Vec<Id> = self
            .tp
            .in_order()
            .iter()
            .map(|h| self.tp.n[*h as usize].id)
            .collect();
        assert_eq!(
            got, want,
            "treap order differs from the Fugue tree traversal"
        );
        assert_eq!(self.tp.len_total(), want.len());
        assert_eq!(
            self.tp.len_visible(),
            self.tp.n.iter().filter(|x| !x.del).count(),
            "visible counts are stale"
        );
    }
    fn index_of(&self, a: &Anchor) -> usize {
        match (a.id, a.stick) {
            (None, Stick::Left) => 0,
            (None, Stick::Right) => self.len(),
            (Some(id), s) => {
                let i = self.handle(id);
                let before = self.tp.rank_visible(i);
                if s == Stick::Left {
                    before + (!self.tp.n[i as usize].del) as usize
                } else {
                    before
                }
            }
        }
    }
}

// ------------------------------------------------------------------ YATA / Yjs integrate on a flat Vec (O(n), for comparison)
#[derive(Clone, Debug)]
struct YItem {
    id: Id,
    lo: Id,
    ro: Id,
    ch: char,
    del: bool,
}
#[derive(Default)]
struct Yata {
    items: Vec<YItem>,
}
impl Yata {
    fn pos(&self, id: Id) -> Option<usize> {
        self.items.iter().position(|x| x.id == id)
    }
    /// The conflict scan of Yjs (`Item.integrate`): walk the items between the two origins and decide where we land among concurrent inserts.
    fn integrate(&mut self, it: YItem) {
        let mut left: isize = if it.lo == ROOT {
            -1
        } else {
            self.pos(it.lo).unwrap() as isize
        };
        let end = if it.ro == END {
            self.items.len()
        } else {
            self.pos(it.ro).unwrap()
        };
        let (mut before, mut confl): (HashSet<Id>, HashSet<Id>) = (HashSet::new(), HashSet::new());
        let mut o = (left + 1) as usize;
        while o < self.items.len() && o != end {
            let other = &self.items[o];
            before.insert(other.id);
            confl.insert(other.id);
            if other.lo == it.lo {
                if other.id.0 < it.id.0 {
                    left = o as isize;
                    confl.clear();
                } else if other.ro == it.ro {
                    break;
                }
            } else if other.lo != ROOT && before.contains(&other.lo) {
                if !confl.contains(&other.lo) {
                    left = o as isize;
                    confl.clear();
                }
            } else {
                break;
            }
            o += 1;
        }
        self.items.insert((left + 1) as usize, it);
    }
}
impl Engine for Yata {
    const NAME: &'static str = "YATA";
    fn has(&self, id: Id) -> bool {
        self.pos(id).is_some()
    }
    fn apply(&mut self, op: &Op) -> bool {
        match op {
            Op::Del { target } => match self.pos(*target) {
                Some(i) => {
                    self.items[i].del = true;
                    true
                }
                None => false,
            },
            Op::Ins { id, a, b, ch, .. } => {
                if self.has(*id) {
                    return true;
                }
                if (*a != ROOT && !self.has(*a)) || (*b != END && !self.has(*b)) {
                    return false;
                }
                self.integrate(YItem {
                    id: *id,
                    lo: *a,
                    ro: *b,
                    ch: *ch,
                    del: false,
                });
                true
            }
        }
    }
    fn local_insert(&mut self, id: Id, idx: usize, ch: char) -> Op {
        let vis: Vec<usize> = (0..self.items.len())
            .filter(|i| !self.items[*i].del)
            .collect();
        let (lo, next) = if idx == 0 {
            (ROOT, 0)
        } else {
            (self.items[vis[idx - 1]].id, vis[idx - 1] + 1)
        };
        let ro = self.items.get(next).map_or(END, |x| x.id);
        let op = Op::Ins {
            id,
            a: lo,
            b: ro,
            side: 1,
            ch,
        };
        assert!(self.apply(&op));
        op
    }
    fn local_delete_id(&mut self, id: Id) -> Option<Op> {
        let i = self.pos(id)?;
        if self.items[i].del {
            return None;
        }
        self.items[i].del = true;
        Some(Op::Del { target: id })
    }
    fn visible_ids(&self) -> Vec<Id> {
        self.items.iter().filter(|x| !x.del).map(|x| x.id).collect()
    }
    fn text(&self) -> String {
        self.items.iter().filter(|x| !x.del).map(|x| x.ch).collect()
    }
    fn chars_at(&self, idx: usize, n: usize) -> Vec<(Id, char)> {
        self.items
            .iter()
            .filter(|x| !x.del)
            .skip(idx)
            .take(n)
            .map(|x| (x.id, x.ch))
            .collect()
    }
    fn index_of(&self, a: &Anchor) -> usize {
        match (a.id, a.stick) {
            (None, Stick::Left) => 0,
            (None, Stick::Right) => self.len(),
            (Some(id), s) => {
                let i = self.pos(id).unwrap();
                let before = self.items[..i].iter().filter(|x| !x.del).count();
                if s == Stick::Left {
                    before + (!self.items[i].del) as usize
                } else {
                    before
                }
            }
        }
    }
}

// ------------------------------------------------------------------ peers: causal buffer, undo/redo, diff adapter
#[derive(Clone, Debug)]
struct Act {
    insert: bool, // true: this action inserted `ids`; false: it deleted them
    ids: Vec<Id>,
    text: String,
    anchor: Anchor, // Left-sticky anchor of the character BEFORE the affected span
}
struct Peer<E: Engine> {
    id: Rep,
    eng: E,
    ctr: u64,
    pending: Vec<Op>,
    undo: Vec<Act>,
    redo: Vec<Act>,
}
impl<E: Engine> Peer<E> {
    fn new(id: Rep) -> Self {
        Peer {
            id,
            eng: E::default(),
            ctr: 0,
            pending: vec![],
            undo: vec![],
            redo: vec![],
        }
    }
    fn fresh(&mut self) -> Id {
        self.ctr += 1;
        (self.id, self.ctr)
    }
    fn insert_raw(&mut self, idx: usize, text: &str, ops: &mut Vec<Op>) -> Vec<Id> {
        let mut ids = vec![];
        for (k, ch) in text.chars().enumerate() {
            let id = self.fresh();
            ops.push(self.eng.local_insert(id, idx + k, ch));
            ids.push(id);
        }
        ids
    }
    fn delete_ids(&mut self, ids: &[Id], ops: &mut Vec<Op>) -> Vec<Id> {
        ids.iter()
            .filter_map(|id| self.eng.local_delete_id(*id).map(|op| (*id, op)))
            .map(|(id, op)| {
                ops.push(op);
                id
            })
            .collect()
    }
    fn char_of(&self, id: Id, ids: &[Id], text: &str) -> char {
        text.chars()
            .nth(ids.iter().position(|x| *x == id).unwrap())
            .unwrap()
    }
    // user actions (recorded for undo)
    fn type_text(&mut self, idx: usize, text: &str) -> Vec<Op> {
        let anchor = self.eng.anchor_at(idx, Stick::Left);
        let mut ops = vec![];
        let ids = self.insert_raw(idx, text, &mut ops);
        self.undo.push(Act {
            insert: true,
            ids,
            text: text.into(),
            anchor,
        });
        self.redo.clear();
        ops
    }
    fn delete_range(&mut self, idx: usize, len: usize) -> Vec<Op> {
        let (ids, text): (Vec<Id>, String) = self.eng.chars_at(idx, len).into_iter().unzip();
        let anchor = self.eng.anchor_at(idx, Stick::Left);
        let mut ops = vec![];
        self.delete_ids(&ids, &mut ops);
        self.undo.push(Act {
            insert: false,
            ids,
            text,
            anchor,
        });
        self.redo.clear();
        ops
    }
    /// Apply the inverse of `a` and return the action that would undo THAT (so redo works the same way).
    fn invert(&mut self, a: Act, ops: &mut Vec<Op>) -> Act {
        if a.insert {
            let gone = self.delete_ids(&a.ids, ops);
            let text = gone
                .iter()
                .map(|g| self.char_of(*g, &a.ids, &a.text))
                .collect();
            Act {
                insert: false,
                ids: gone,
                text,
                anchor: a.anchor,
            }
        } else {
            let idx = self.eng.index_of(&a.anchor); // the anchor survived remote edits: this is where the text belonged
            let ids = self.insert_raw(idx, &a.text, ops);
            // The revived characters have NEW ids: older stack entries that mention the dead ids must follow them.
            let remap: HashMap<Id, Id> = a.ids.iter().copied().zip(ids.iter().copied()).collect();
            for act in self.undo.iter_mut().chain(self.redo.iter_mut()) {
                act.ids
                    .iter_mut()
                    .for_each(|i| *i = *remap.get(i).unwrap_or(i));
                act.anchor.id = act.anchor.id.map(|i| *remap.get(&i).unwrap_or(&i));
            }
            let anchor = Anchor {
                id: a.anchor.id.map(|i| *remap.get(&i).unwrap_or(&i)),
                ..a.anchor
            };
            Act {
                insert: true,
                ids,
                text: a.text,
                anchor,
            }
        }
    }
    fn undo_one(&mut self) -> Vec<Op> {
        let mut ops = vec![];
        if let Some(a) = self.undo.pop() {
            let inv = self.invert(a, &mut ops);
            self.redo.push(inv);
        }
        ops
    }
    fn redo_one(&mut self) -> Vec<Op> {
        let mut ops = vec![];
        if let Some(a) = self.redo.pop() {
            let inv = self.invert(a, &mut ops);
            self.undo.push(inv);
        }
        ops
    }
    /// Make the document equal `new` using a minimal LCS diff.
    fn set_text(&mut self, new: &str) -> Vec<Op> {
        let old: Vec<char> = self.eng.text().chars().collect();
        let new: Vec<char> = new.chars().collect();
        let (mut ops, ids) = (vec![], self.eng.visible_ids());
        for e in diff(&old, &new) {
            match e {
                Edit::Del(i) => {
                    self.delete_ids(&[ids[i]], &mut ops);
                }
                Edit::Ins(at_new, ch) => {
                    // earlier edits are already applied, so the current doc is new[..at_new] + the rest of old: index == at_new
                    let id = self.fresh();
                    ops.push(self.eng.local_insert(id, at_new, ch));
                }
            }
        }
        ops
    }
    fn receive(&mut self, op: &Op) {
        self.pending.push(op.clone());
        loop {
            let before = self.pending.len();
            let mut rest = vec![];
            for op in std::mem::take(&mut self.pending) {
                if !self.eng.apply(&op) {
                    rest.push(op);
                }
            }
            self.pending = rest;
            if self.pending.len() == before || self.pending.is_empty() {
                break;
            }
        }
    }
}

// ------------------------------------------------------------------ text diff -> edits
#[derive(Debug, PartialEq)]
enum Edit {
    Del(usize), // delete the OLD char at this old index (ids are resolved before any edit)
    Ins(usize, char), // insert at this index of the NEW text (== current index when edits are applied in order)
}
/// LCS diff with common prefix/suffix trimming. Edits are returned in document order, so applying them one by one is valid
/// as long as deletes address OLD indices (stable ids) and inserts address NEW indices (growing prefix is already correct).
fn diff(old: &[char], new: &[char]) -> Vec<Edit> {
    let pre = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suf = old[pre..]
        .iter()
        .rev()
        .zip(new[pre..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (o, n) = (&old[pre..old.len() - suf], &new[pre..new.len() - suf]);
    let mut t = vec![vec![0usize; n.len() + 1]; o.len() + 1];
    for i in (0..o.len()).rev() {
        for j in (0..n.len()).rev() {
            t[i][j] = if o[i] == n[j] {
                t[i + 1][j + 1] + 1
            } else {
                t[i + 1][j].max(t[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut edits) = (0, 0, vec![]);
    while i < o.len() || j < n.len() {
        if i < o.len() && j < n.len() && o[i] == n[j] {
            i += 1;
            j += 1;
        } else if i < o.len() && (j == n.len() || t[i + 1][j] >= t[i][j + 1]) {
            edits.push(Edit::Del(pre + i));
            i += 1;
        } else {
            edits.push(Edit::Ins(pre + j, n[j]));
            j += 1;
        }
    }
    edits
}
fn lcs_len(a: &[char], b: &[char]) -> usize {
    let mut row = vec![0usize; b.len() + 1];
    for x in a {
        let mut diag = 0;
        for (j, y) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = if x == y {
                diag + 1
            } else {
                row[j + 1].max(row[j])
            };
            diag = up;
        }
    }
    row[b.len()]
}

// ------------------------------------------------------------------ wire encoding: naive vs run-length
struct Wb(Vec<u8>);
impl Wb {
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
        self.u(if i.0 == u32::MAX { 0 } else { i.0 as u64 + 1 });
        self.u(i.1);
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Run {
    id: Id,
    a: Id,
    b: Id,
    side: u8,
    text: String,
}
/// Merge consecutive inserts by one author into runs: each char i+1 must be the right child of char i with the next counter.
fn batch_runs(ops: &[Op]) -> Vec<Run> {
    let mut runs: Vec<Run> = vec![];
    for op in ops {
        if let Op::Ins { id, a, b, side, ch } = op {
            if let Some(r) = runs.last_mut() {
                let prev = (r.id.0, r.id.1 + r.text.chars().count() as u64 - 1);
                if *id == (prev.0, prev.1 + 1) && *a == prev && *side == 1 && *b == r.b {
                    r.text.push(*ch);
                    continue;
                }
            }
            runs.push(Run {
                id: *id,
                a: *a,
                b: *b,
                side: *side,
                text: ch.to_string(),
            });
        }
    }
    runs
}
fn expand_run(r: &Run) -> Vec<Op> {
    r.text
        .chars()
        .enumerate()
        .map(|(k, ch)| {
            let id = (r.id.0, r.id.1 + k as u64);
            if k == 0 {
                Op::Ins {
                    id,
                    a: r.a,
                    b: r.b,
                    side: r.side,
                    ch,
                }
            } else {
                Op::Ins {
                    id,
                    a: (r.id.0, id.1 - 1),
                    b: r.b,
                    side: 1,
                    ch,
                }
            }
        })
        .collect()
}
/// Deleted ids -> (replica, first counter, length) ranges.
fn compact_deletes(ids: &[Id]) -> Vec<(Rep, u64, u64)> {
    let mut v = ids.to_vec();
    v.sort();
    let mut out: Vec<(Rep, u64, u64)> = vec![];
    for (r, c) in v {
        match out.last_mut() {
            Some(l) if l.0 == r && l.1 + l.2 == c => l.2 += 1,
            _ => out.push((r, c, 1)),
        }
    }
    out
}
fn naive_bytes(ops: &[Op]) -> usize {
    let mut w = Wb(vec![]);
    for op in ops {
        match op {
            Op::Ins {
                id, a, side, ch, ..
            } => {
                w.0.push(1);
                w.id(*id);
                w.id(*a);
                w.0.push(*side);
                w.0.extend_from_slice(ch.to_string().as_bytes());
            }
            Op::Del { target } => {
                w.0.push(2);
                w.id(*target);
            }
        }
    }
    w.0.len()
}
fn rle_bytes(ops: &[Op]) -> usize {
    let mut w = Wb(vec![]);
    // inserts and deletes are batched separately; deletes as ranges
    for r in batch_runs(ops) {
        w.0.push(1);
        w.id(r.id);
        w.id(r.a);
        w.0.push(r.side);
        w.u(r.text.len() as u64);
        w.0.extend_from_slice(r.text.as_bytes());
    }
    let dels: Vec<Id> = ops
        .iter()
        .filter_map(|o| {
            if let Op::Del { target } = o {
                Some(*target)
            } else {
                None
            }
        })
        .collect();
    for (r, c, l) in compact_deletes(&dels) {
        w.0.push(2);
        w.id((r, c));
        w.u(l);
    }
    w.0.len()
}

// ------------------------------------------------------------------ random schedules
const WORDS: [&str; 8] = ["alpha ", "beta ", "gamma ", "delta", "x", "yz", " ", "\n"];
fn random_action<E: Engine>(p: &mut Peer<E>, rng: &mut Rng) -> Vec<Op> {
    let len = p.eng.len();
    match rng.below(100) {
        0..=44 => {
            let idx = rng.below(len + 1);
            let t = WORDS[rng.below(WORDS.len())];
            p.type_text(idx, t)
        }
        45..=64 if len > 0 => {
            let idx = rng.below(len);
            let n = 1 + rng.below((len - idx).min(5));
            p.delete_range(idx, n)
        }
        65..=74 => p.undo_one(),
        75..=79 => p.redo_one(),
        80..=89 => {
            let mut cs: Vec<char> = p.eng.text().chars().collect();
            if !cs.is_empty() {
                let i = rng.below(cs.len());
                cs[i] = 'Q';
            }
            if rng.chance(50) {
                cs.push('!');
            }
            p.set_text(&cs.into_iter().collect::<String>())
        }
        _ => {
            let idx = rng.below(len + 1);
            p.type_text(idx, "ab")
        }
    }
}
/// n peers produce ops; every op is queued for every other peer (maybe twice); deliveries pick random queue entries.
fn schedule<E: Engine>(seed: u64) -> Vec<Op> {
    let mut rng = Rng::new(seed);
    let n = 3 + rng.below(3);
    let mut peers: Vec<Peer<E>> = (1..=n as u32).map(Peer::new).collect();
    let mut queue: Vec<(usize, Op)> = vec![];
    let mut all_ops = vec![];
    for _ in 0..40 + rng.below(60) {
        let a = rng.below(n);
        if rng.chance(65) {
            let ops = random_action(&mut peers[a], &mut rng);
            for op in &ops {
                for b in (0..n).filter(|b| *b != a) {
                    queue.push((b, op.clone()));
                    if rng.chance(15) {
                        queue.push((b, op.clone()));
                    }
                }
            }
            all_ops.extend(ops);
        } else if !queue.is_empty() {
            let (to, op) = queue.swap_remove(rng.below(queue.len()));
            peers[to].receive(&op);
        }
    }
    while !queue.is_empty() {
        let (to, op) = queue.swap_remove(rng.below(queue.len()));
        peers[to].receive(&op);
    }
    let t0 = peers[0].eng.text();
    for p in &peers {
        p.eng.check_structure();
        assert!(
            p.pending.is_empty(),
            "{}: ops stuck in the causal buffer (seed {seed})",
            E::NAME
        );
        assert_eq!(
            p.eng.text(),
            t0,
            "{}: replicas diverged (seed {seed})",
            E::NAME
        );
        assert_eq!(
            p.eng.visible_ids(),
            peers[0].eng.visible_ids(),
            "{}: id order diverged (seed {seed})",
            E::NAME
        );
    }
    all_ops
}
fn convergence_suite<E: Engine>(n: u64) -> usize {
    (0..n).map(|s| schedule::<E>(s * 31 + 7).len()).sum()
}

// ------------------------------------------------------------------ checks
fn treap_vs_vec_oracle() {
    let mut rng = Rng::new(99);
    let (mut t, mut oracle): (Treap, Vec<(Id, bool)>) = (Treap::new(), vec![]);
    let mut handles = vec![];
    for k in 0..4000u64 {
        match rng.below(10) {
            0..=6 => {
                let pos = rng.below(oracle.len() + 1);
                let h = t.insert_at(pos, (1, k), 'x');
                oracle.insert(pos, ((1, k), false));
                handles.push((h, (1, k)));
            }
            _ if !oracle.is_empty() => {
                let i = rng.below(oracle.len());
                let h = t.by_total(i).unwrap();
                t.set_deleted(h);
                oracle[i].1 = true;
            }
            _ => {}
        }
        if k % 97 == 0 {
            assert_eq!(t.len_total(), oracle.len());
            let vis: Vec<Id> = oracle.iter().filter(|x| !x.1).map(|x| x.0).collect();
            assert_eq!(t.len_visible(), vis.len());
            for (j, id) in vis.iter().enumerate().step_by(7) {
                let h = t.by_visible(j).unwrap();
                assert_eq!(t.n[h as usize].id, *id);
                assert_eq!(t.rank_visible(h), j);
            }
            for (h, id) in handles.iter().step_by(11) {
                let pos = oracle.iter().position(|x| x.0 == *id).unwrap();
                assert_eq!(t.rank_total(*h), pos);
            }
        }
    }
    let order: Vec<Id> = t.in_order().iter().map(|h| t.n[*h as usize].id).collect();
    assert_eq!(order, oracle.iter().map(|x| x.0).collect::<Vec<_>>());
    println!(
        "  treap vs Vec oracle: 4000 random ops OK, height {} for {} nodes (log2 = {:.1})",
        t.height(),
        oracle.len(),
        (oracle.len() as f64).log2()
    );
    assert!(t.height() < 60);
}

/// 2..=4 authors with random ids concurrently type a run each into the same random gap of a shared random base text, forward or backward
/// (every char at the same index). Returns true if any author's run does not end up contiguous.
fn interleave_trial<E: Engine>(seed: u64, backward: bool) -> bool {
    let mut rng = Rng::new(seed);
    let authors = 2 + rng.below(3);
    let mut ids: Vec<Rep> = (1..=9).collect();
    let mut peers: Vec<Peer<E>> = (0..authors)
        .map(|_| Peer::new(ids.swap_remove(rng.below(ids.len()))))
        .collect();
    // the base text is written by a tenth replica so that author ids stay free to be in any order
    let mut writer = Peer::<E>::new(10);
    let mut base = vec![];
    for _ in 0..1 + rng.below(6) {
        let at = rng.below(writer.eng.len() + 1);
        base.extend(writer.type_text(at, "."));
    }
    peers
        .iter_mut()
        .for_each(|p| base.iter().for_each(|o| p.receive(o)));
    let gap = rng.below(writer.eng.len() + 1);
    let mut all: Vec<Vec<Op>> = vec![];
    for p in &mut peers {
        let len = 2 + rng.below(8);
        all.push(
            (0..len)
                .map(|k| {
                    let id = p.fresh();
                    p.eng
                        .local_insert(id, gap + if backward { 0 } else { k }, 'a')
                })
                .collect(),
        );
    }
    for (i, p) in peers.iter_mut().enumerate() {
        let mut others: Vec<usize> = (0..authors).filter(|j| *j != i).collect();
        while !others.is_empty() {
            let j = others.swap_remove(rng.below(others.len()));
            all[j].iter().for_each(|o| p.receive(o));
        }
    }
    let order = peers[0].eng.visible_ids();
    assert!(
        peers.iter().all(|p| p.eng.visible_ids() == order),
        "{}: diverged in interleave trial {seed}",
        E::NAME
    );
    all.iter().any(|run| {
        let pos: Vec<usize> = run
            .iter()
            .filter_map(|o| {
                if let Op::Ins { id, .. } = o {
                    order.iter().position(|x| x == id)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(pos.len(), run.len(), "a typed char is missing");
        pos.iter().max().unwrap() - pos.iter().min().unwrap() + 1 != run.len()
    })
}
fn interleave_count<E: Engine>() -> (usize, usize) {
    (0..300u64).fold((0, 0), |r, k| {
        (
            r.0 + interleave_trial::<E>(k, false) as usize,
            r.1 + interleave_trial::<E>(k, true) as usize,
        )
    })
}

fn cursor_stability() {
    // Differential test of anchors: index_of must equal a linear scan over the full (tombstone-inclusive) order.
    let mut rng = Rng::new(5);
    for seed in 0..300u64 {
        let mut me = Peer::<FugueDoc>::new(1);
        let mut other = Peer::<FugueDoc>::new(2);
        let init = me.type_text(0, "the quick brown fox");
        init.iter().for_each(|o| other.receive(o));
        let at = rng.below(me.eng.len() + 1);
        let stick = if rng.chance(50) {
            Stick::Left
        } else {
            Stick::Right
        };
        let anchor = me.eng.anchor_at(at, stick);
        for _ in 0..8 {
            let ops = random_action(&mut other, &mut rng);
            ops.iter().for_each(|o| me.receive(o));
        }
        let idx = me.eng.index_of(&anchor);
        // oracle: walk the total order
        let order: Vec<(Id, bool)> = me
            .eng
            .tp
            .in_order()
            .iter()
            .map(|h| (me.eng.tp.n[*h as usize].id, me.eng.tp.n[*h as usize].del))
            .collect();
        let expect = match (anchor.id, stick) {
            (None, Stick::Left) => 0,
            (None, Stick::Right) => me.eng.len(),
            (Some(id), s) => {
                let p = order.iter().position(|x| x.0 == id).unwrap();
                order[..if s == Stick::Left { p + 1 } else { p }]
                    .iter()
                    .filter(|x| !x.1)
                    .count()
            }
        };
        assert_eq!(idx, expect, "seed {seed}");
        // intention: a live anchor character still sits on the cursor's side
        if let (Some(id), Stick::Left) = (anchor.id, stick) {
            let ids = me.eng.visible_ids();
            if let Some(p) = ids.iter().position(|x| *x == id) {
                assert_eq!(idx, p + 1);
            }
        }
    }
    // Stickiness decides which neighbour a cursor follows when remote text lands exactly on it.
    let (mut a, mut b) = (Peer::<FugueDoc>::new(1), Peer::<FugueDoc>::new(2));
    a.type_text(0, "ab").iter().for_each(|o| b.receive(o));
    let (left, right) = (
        a.eng.anchor_at(1, Stick::Left),
        a.eng.anchor_at(1, Stick::Right),
    );
    b.type_text(1, "ZZ").iter().for_each(|o| a.receive(o));
    println!(
        "  cursor between a|b after remote 'ZZ' typed there: left-sticky at {}, right-sticky at {} of {:?}",
        a.eng.index_of(&left),
        a.eng.index_of(&right),
        a.eng.text()
    );
    assert_eq!(
        (a.eng.index_of(&left), a.eng.index_of(&right)),
        (1, 3),
        "left sticks to 'a', right sticks to 'b'"
    );
    // A cursor whose anchor char was deleted collapses to the gap.
    a.delete_range(1, 2);
    assert_eq!(a.eng.index_of(&left), 1);
    assert_eq!(a.eng.index_of(&right), 1);
    // A selection is two anchors. Hugging the selected chars (start sticks right, end sticks left): text typed at either edge stays
    // outside, text typed inside joins, and edits elsewhere only shift it.
    let (mut a, mut b) = (Peer::<FugueDoc>::new(1), Peer::<FugueDoc>::new(2));
    a.type_text(0, "the quick fox")
        .iter()
        .for_each(|o| b.receive(o));
    let sel = (
        a.eng.anchor_at(4, Stick::Right),
        a.eng.anchor_at(9, Stick::Left),
    ); // "quick"
    for (at, text) in [(0, ">> "), (7, "very "), (17, "!"), (12, "??")] {
        b.type_text(at, text).iter().for_each(|o| a.receive(o));
    }
    let (from, to) = (a.eng.index_of(&sel.0), a.eng.index_of(&sel.1));
    let selected: String = a.eng.text().chars().skip(from).take(to - from).collect();
    println!(
        "  selection of \"quick\" after remote edits: {selected:?} inside {:?}",
        a.eng.text()
    );
    assert_eq!(a.eng.text(), ">> the very ??quick! fox");
    assert_eq!(selected, "quick");
    b.type_text(from + 2, "--")
        .iter()
        .for_each(|o| a.receive(o)); // inside the selection
    let (from, to) = (a.eng.index_of(&sel.0), a.eng.index_of(&sel.1));
    assert_eq!(
        a.eng
            .text()
            .chars()
            .skip(from)
            .take(to - from)
            .collect::<String>(),
        "qu--ick"
    );
    println!(
        "  300 random anchor checks vs linear oracle OK; deleted-range cursors collapse to the gap; selections hug their text"
    );
}

fn undo_redo_checks<E: Engine>() {
    let mut rng = Rng::new(17);
    for seed in 0..200u64 {
        let mut p = Peer::<E>::new(1);
        let mut rng2 = Rng::new(seed + 1000);
        for _ in 0..(5 + rng2.below(15)) {
            let len = p.eng.len();
            if len > 2 && rng2.chance(35) {
                p.delete_range(rng2.below(len - 1), 1);
            } else {
                p.type_text(rng2.below(len + 1), WORDS[rng2.below(WORDS.len())]);
            }
        }
        let final_text = p.eng.text();
        let steps = p.undo.len();
        for _ in 0..steps {
            p.undo_one();
        }
        assert_eq!(
            p.eng.text(),
            "",
            "{}: undoing everything must restore the empty doc (seed {seed})",
            E::NAME
        );
        for _ in 0..steps {
            p.redo_one();
        }
        assert_eq!(
            p.eng.text(),
            final_text,
            "{}: redo must restore the final text (seed {seed})",
            E::NAME
        );
    }
    // multi-user: A only inserts; B edits concurrently; A undoes everything it did; none of A's characters survive, B's untouched ones do.
    for seed in 0..200u64 {
        let mut rng = Rng::new(seed + 7);
        let (mut a, mut b) = (Peer::<E>::new(1), Peer::<E>::new(2));
        let base = b.type_text(0, "BASE TEXT");
        base.iter().for_each(|o| a.receive(o));
        let (mut to_a, mut to_b) = (vec![], vec![]);
        for _ in 0..6 {
            let i = rng.below(a.eng.len() + 1);
            to_b.extend(a.type_text(i, "aa"));
            let l = b.eng.len();
            if rng.chance(50) && l > 3 {
                to_a.extend(b.delete_range(rng.below(l - 2), 2));
            } else {
                to_a.extend(b.type_text(rng.below(l + 1), "bb"));
            }
        }
        to_a.iter().for_each(|o| a.receive(o));
        to_b.iter().for_each(|o| b.receive(o));
        assert_eq!(a.eng.text(), b.eng.text());
        let b_alive_before: Vec<Id> = a
            .eng
            .visible_ids()
            .into_iter()
            .filter(|i| i.0 == 2)
            .collect();
        let mut undo_ops = vec![];
        while !a.undo.is_empty() {
            undo_ops.extend(a.undo_one());
        }
        undo_ops.iter().for_each(|o| b.receive(o));
        assert_eq!(a.eng.text(), b.eng.text());
        let after = a.eng.visible_ids();
        assert!(
            after.iter().all(|i| i.0 == 2),
            "{}: one of A's chars survived its own undo",
            E::NAME
        );
        assert_eq!(
            after,
            b_alive_before,
            "{}: B's characters must be untouched by A's undo",
            E::NAME
        );
    }
    let _ = rng.below(2);
}

fn diff_checks<E: Engine>() {
    let mut rng = Rng::new(3);
    let alphabet = ['a', 'b', 'c', 'd'];
    let mut total_ops = 0;
    for _ in 0..500 {
        let mk = |rng: &mut Rng| -> String {
            (0..rng.below(14)).map(|_| alphabet[rng.below(4)]).collect()
        };
        let (old, new) = (mk(&mut rng), mk(&mut rng));
        let mut p = Peer::<E>::new(1);
        p.type_text(0, &old);
        let ops = p.set_text(&new);
        assert_eq!(
            p.eng.text(),
            new,
            "{}: diff adapter produced the wrong text",
            E::NAME
        );
        let (o, n): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
        assert_eq!(
            ops.len(),
            o.len() + n.len() - 2 * lcs_len(&o, &n),
            "diff must be minimal"
        );
        total_ops += ops.len();
    }
    println!(
        "  {}: 500 random string pairs -> text equal and op count == |old|+|new|-2*LCS ({total_ops} ops)",
        E::NAME
    );
}

/// Typing, deleting, undo/redo, cursors and applying remote ops must use the order-statistic tree, never a walk over the whole document.
fn local_edits_do_not_scan() {
    let (mut a, mut b) = (Peer::<FugueDoc>::new(1), Peer::<FugueDoc>::new(2));
    a.type_text(0, &"lorem ipsum ".repeat(2000))
        .iter()
        .for_each(|o| b.receive(o));
    let (sa, sb) = (a.eng.full_scans.get(), b.eng.full_scans.get());
    let mut rng = Rng::new(11);
    for _ in 0..300 {
        let at = rng.below(a.eng.len() - 5);
        let mut ops = a.type_text(at, "hi");
        let cursor = a.eng.anchor_at(at, Stick::Right);
        ops.extend(a.delete_range(at, 3));
        ops.extend(a.undo_one());
        ops.extend(a.redo_one());
        assert!(a.eng.index_of(&cursor) <= a.eng.len());
        ops.iter().for_each(|o| b.receive(o));
    }
    assert_eq!(
        (a.eng.full_scans.get() - sa, b.eng.full_scans.get() - sb),
        (0, 0),
        "an editing path walked the whole document"
    );
    assert_eq!(a.eng.text(), b.eng.text());
    println!(
        "  1200 local edits + their remote application on a {}-char document: 0 full-document walks",
        a.eng.len()
    );
}

fn overhead_report() {
    let mut p = Peer::<FugueDoc>::new(1);
    let mut rng = Rng::new(1);
    let mut ops = vec![];
    let sentence = "collaborative editing needs identifiers for every character ";
    for _ in 0..40 {
        let at = p.eng.len();
        ops.extend(p.type_text(at, sentence)); // a typing burst
        if rng.chance(40) && p.eng.len() > 10 {
            let i = rng.below(p.eng.len() - 5);
            ops.extend(p.delete_range(i, 4)); // a backspace-ish deletion
        }
    }
    let typed = ops.iter().filter(|o| matches!(o, Op::Ins { .. })).count();
    let runs = batch_runs(&ops);
    let back: Vec<Op> = runs.iter().flat_map(expand_run).collect();
    let ins_only: Vec<Op> = ops
        .iter()
        .filter(|o| matches!(o, Op::Ins { .. }))
        .cloned()
        .collect();
    assert_eq!(back, ins_only, "run batching must be lossless");
    let (nb, rb) = (naive_bytes(&ops), rle_bytes(&ops));
    println!(
        "  {typed} typed chars -> naive: {} ops, {nb} bytes ({:.2} B/char); run-length: {} runs, {rb} bytes ({:.2} B/char); ratio {:.1}x",
        ops.len(),
        nb as f64 / typed as f64,
        runs.len()
            + compact_deletes(
                &ops.iter()
                    .filter_map(|o| if let Op::Del { target } = o {
                        Some(*target)
                    } else {
                        None
                    })
                    .collect::<Vec<_>>()
            )
            .len(),
        rb as f64 / typed as f64,
        nb as f64 / rb as f64
    );
    assert!(
        rb * 3 < nb,
        "RLE must be several times smaller for typing bursts"
    );
    println!(
        "  in-memory: {} bytes per treap node (id, links, counts, char) before any run-based storage",
        std::mem::size_of::<TN>()
    );
    let dels = compact_deletes(&[(1, 5), (1, 6), (1, 7), (2, 1), (1, 9)]);
    assert_eq!(dels, vec![(1, 5, 3), (1, 9, 1), (2, 1, 1)]);
}

fn main() {
    println!("Treap (position <-> id)");
    treap_vs_vec_oracle();
    local_edits_do_not_scan();
    println!(
        "Convergence: 2000 random schedules per engine (3-5 peers; typing, deletes, undo/redo, diff edits; reordered + duplicated delivery)"
    );
    let t = std::time::Instant::now();
    let nf = convergence_suite::<FugueDoc>(2000);
    println!(
        "  Fugue converged on 2000 schedules ({nf} ops) in {:?}",
        t.elapsed()
    );
    let t = std::time::Instant::now();
    let ny = convergence_suite::<Yata>(2000);
    println!(
        "  YATA  converged on 2000 schedules ({ny} ops) in {:?}",
        t.elapsed()
    );
    println!(
        "Concurrent typing into one gap (300 seeded trials per direction, 2-4 authors, runs of 2-9 chars): trials with an interleaved run"
    );
    for (name, f) in [
        (
            "Fugue",
            interleave_count::<FugueDoc> as fn() -> (usize, usize),
        ),
        ("YATA", interleave_count::<Yata>),
    ] {
        let (fw, bw) = f();
        println!("  {name:<6} forward {fw:3}  backward {bw:3}");
        assert_eq!(
            (fw, bw),
            (0, 0),
            "{name} must keep each run contiguous when all runs start from the same state"
        );
    }
    println!(
        "  (all authors start from one shared state here; that is not enough to make YATA interleave. RGA and Logoot do: see crdt_todo_app_no_db)"
    );
    println!("Cursors");
    cursor_stability();
    println!("Undo / redo");
    undo_redo_checks::<FugueDoc>();
    undo_redo_checks::<Yata>();
    println!(
        "  single-user undo-all/redo-all and multi-user 'undo only my chars' OK on both engines"
    );
    println!("Diff adapter");
    diff_checks::<FugueDoc>();
    diff_checks::<Yata>();
    println!("Wire overhead");
    overhead_report();
    println!("OK");
}
