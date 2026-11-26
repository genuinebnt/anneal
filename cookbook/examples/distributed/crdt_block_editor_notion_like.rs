//! RECIPE 802 (distributed): a Notion/Outline-like block editor as a CRDT: a movable block TREE, Fugue text per block, Peritext-style marks, split/merge, undo and Markdown/HTML rendering.
//! LEVEL: advanced
//! Use in: the data model behind block editors (pages made of nested paragraphs, headings, lists, todos, code) that sync without a central lock.   Run: cargo run --release --example crdt_block_editor_notion_like
//! COMMON USES: Notion/Outline/AppFlowy-style documents, collaborative outliners, local-first note apps, design tools with layer trees.
//! Course: Kleppmann et al., "A highly-available move operation for replicated trees"; Litt et al., "Peritext".
#![allow(dead_code)]
//!
//! WHAT: a document is a TREE of blocks. Each block has a kind (paragraph, heading, bullet, numbered, todo, quote, code, divider), LWW attributes, and
//! a text that is a Fugue sequence of characters plus inline MARKS (bold, italic, code, link).
//! HOW (tree): creating, moving and deleting a block are all one op: `Move{child, parent, key}` (delete = move into `Parent::Trash`). Two users can
//! move blocks concurrently so that applying both naively would create a cycle (A moves X under Y while B moves Y under X). The Kleppmann algorithm:
//! every move has a Lamport timestamp and every replica applies moves in TIMESTAMP order. When an op arrives that is older than ops already applied, the
//! newer ones are UNDONE (the log remembers each move's old parent), the old op is applied, and the newer ones are REDONE. A move that would create a
//! cycle is skipped at apply time (same decision on every replica because they all replay the same total order). Sibling order is a fractional key
//! (`between`) plus a per-op suffix, so two replicas inserting at the same spot never mint equal keys (equal keys leave no room for a later insert).
//! HOW (text): `Text` is Fugue (see crdt_fugue_text_editor) with tombstones. Marks use the Peritext idea: a mark is `(start gap, end gap, kind, value)` where a gap is
//! "just after char c" or "just before char c". Where the gap anchors decide whether text typed at the boundary joins the mark: marks START with a
//! "before first char" anchor (no growth at the front; except that bold/italic from the first char of a paragraph start at the paragraph start) and bold/italic
//! END with a "before the NEXT char" anchor (grows at the end), links/code end "after the last char" (no growth). Per char and mark kind the op with the newest stamp wins, so bold-vs-unbold conflicts resolve identically everywhere. Anchors point at char
//! ids, so deleting boundary characters (tombstones) never loses a mark.
//! HOW (split / merge): Enter in the middle of a block is a composite of ordinary ops: create a sibling after it, copy the tail text and its marks into it, delete
//! the tail from the original. Backspace at the start of a block appends its text and marks to the previous sibling and trashes the block. Documented
//! conflict behaviour: text typed concurrently INSIDE a tail that another user moved stays in the old block; two users pressing Enter at the same spot get two new blocks
//! (each with a copy of the tail); two users merging the same block duplicate its text. Backspace moves the block's nested children under the surviving block.
//! All of that converges; none of it loses data (the trashed block keeps its text). Not covered: a child that someone else nests under a block while it is
//! being merged away goes to the Trash with it (the data stays in the replica, but is not rendered).
//! UNDO: block-level undo/redo records the inverse structural op (old parent/key, old kind) and applies it as a NEW op, so it merges like any other edit.
//! Split and merge are an undo barrier (they clear both stacks).
//! Op delivery is causal (version vector + dot per op). No database: the document is its op log plus derived state.
//! Honest limits: marks are checked on the scenarios below (not proved), undo covers structure and kind (not text, attributes, split or merge), key growth for repeated inserts at one spot
//! is not rebalanced, sibling runs inserted concurrently at one position may interleave (block keys are not Fugue), text position lookup is O(n) per char,
//! Markdown output does not escape Markdown syntax, and link URLs are escaped but not checked (a real editor must reject `javascript:`).
//! Read: https://martin.kleppmann.com/papers/move-op.pdf ; https://www.inkandswitch.com/peritext/ (the "bold grows at the end, links do not" rule is stated there) ; Fugue https://arxiv.org/abs/2305.00583
//! TRY NEXT:
//!   - [S] Add a `Quote` -> `Callout` kind and render it in `render_md`/`render_html`; which functions must change and which must not?
//!   - [S] Change `expands()` so Link grows at the end like bold; which of the Peritext scenarios flips, and which assertion tells you?
//!   - [M] Predict, then measure: how many moves does `MoveState::integrate` undo/redo on average when the delivery order is reversed (`schedule`)? `MoveState::redone` counts them: print it per replica and per move.
//!   - [M] Make double merges idempotent: give copied chars a `copy_of` id and hide duplicates in `Text::visible`; update the documented conflict list.
//!   - [A] PROJECT: store `Doc` snapshots and the op log with the page/WAL ideas of `crdt_todo_app_no_db` (save/load) and feed the tail of the log to a second replica over the `swim_gossip` membership view.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

type Rep = u32;
type Id = (Rep, u64);
const ROOT: Id = (u32::MAX, 0);
type Vv = BTreeMap<Rep, u64>;
type Stamp = (u64, Rep);

fn vget(v: &Vv, r: Rep) -> u64 {
    v.get(&r).copied().unwrap_or(0)
}
fn vle(a: &Vv, b: &Vv) -> bool {
    a.iter().all(|(r, s)| *s <= vget(b, *r))
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

// ------------------------------------------------------------------ block kinds, marks
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Kind {
    #[default]
    Paragraph,
    Heading(u8),
    Bullet,
    Numbered,
    Todo,
    Quote,
    Code,
    Divider,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MarkKind {
    Link,
    Bold,
    Italic,
    Code,
}
/// Does text typed exactly at the END of the mark join it, as in rich-text editors and the Peritext essay? Text typed just before a mark never
/// joins it, with one exception from the essay: at the start of a paragraph there is no preceding character, so a growing mark covers it too.
fn expands(k: MarkKind) -> bool {
    match k {
        MarkKind::Bold | MarkKind::Italic => true,
        MarkKind::Link | MarkKind::Code => false,
    }
}
/// A boundary gap: right AFTER `id` or right BEFORE it. `id == None` means the start (for a start anchor) or end (for an end anchor) of the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Gap {
    id: Option<Id>,
    after: bool,
}
#[derive(Clone, Debug, PartialEq)]
struct MarkSpec {
    start: Gap,
    end: Gap,
    kind: MarkKind,
    value: Option<String>,
    remove: bool,
}
#[derive(Clone, Debug, PartialEq, Default)]
struct Lww<T> {
    stamp: Stamp,
    val: T,
}
impl<T: Clone> Lww<T> {
    fn set(&mut self, s: Stamp, v: &T) {
        if s > self.stamp {
            self.stamp = s;
            self.val = v.clone();
        }
    }
}

// ------------------------------------------------------------------ Fugue text of one block
#[derive(Clone, Debug, PartialEq)]
struct CNode {
    parent: Id,
    side: u8,
    ch: char,
    del: bool,
}
#[derive(Clone, Debug, PartialEq, Default)]
struct Text {
    nodes: BTreeMap<Id, CNode>,
    kids: BTreeMap<(Id, u8), BTreeSet<Id>>,
}
enum Fr {
    Kids(Id, u8),
    Node(Id),
    Emit(Id),
}
impl Text {
    fn full_order(&self) -> Vec<Id> {
        let (mut out, mut st) = (vec![], vec![Fr::Kids(ROOT, 1)]);
        while let Some(f) = st.pop() {
            match f {
                Fr::Emit(id) => out.push(id),
                Fr::Node(id) => st.extend([Fr::Kids(id, 1), Fr::Emit(id), Fr::Kids(id, 0)]),
                Fr::Kids(p, s) => {
                    if let Some(set) = self.kids.get(&(p, s)) {
                        st.extend(set.iter().rev().map(|k| Fr::Node(*k)));
                    }
                }
            }
        }
        out
    }
    fn visible(&self) -> Vec<Id> {
        self.full_order()
            .into_iter()
            .filter(|i| !self.nodes[i].del)
            .collect()
    }
    fn string(&self) -> String {
        self.visible().iter().map(|i| self.nodes[i].ch).collect()
    }
    /// Fugue's insert rule for a new char at visible index `idx`: (parent, side).
    fn place(&self, idx: usize) -> (Id, u8) {
        let vis = self.visible();
        let left = if idx == 0 { ROOT } else { vis[idx - 1] };
        if !self.kids.contains_key(&(left, 1)) {
            return (left, 1);
        }
        let full = self.full_order();
        let next = if left == ROOT {
            full[0]
        } else {
            full[full.iter().position(|x| *x == left).unwrap() + 1]
        };
        (next, 0)
    }
    fn integrate(&mut self, id: Id, n: CNode) {
        if self.nodes.contains_key(&id) {
            return;
        }
        self.kids.entry((n.parent, n.side)).or_default().insert(id);
        self.nodes.insert(id, n);
    }
}

// ------------------------------------------------------------------ the movable tree (Kleppmann et al.)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Parent {
    Root,
    Trash,
    Block(Id),
}
type Key = Vec<u32>;
#[derive(Clone, Debug, PartialEq)]
struct MoveOp {
    stamp: Stamp,
    child: Id,
    parent: Parent,
    key: Key,
}
#[derive(Clone, Debug, PartialEq)]
struct LogEntry {
    op: MoveOp,
    old: Option<(Parent, Key)>,
    applied: bool,
}
#[derive(Clone, Debug, PartialEq, Default)]
struct MoveState {
    tree: BTreeMap<Id, (Parent, Key)>,
    log: Vec<LogEntry>, // sorted by stamp
    redone: usize, // how many log entries were undone/redone by out-of-order arrivals (a statistic)
}
impl MoveState {
    fn is_ancestor(&self, anc: Id, mut p: Parent) -> bool {
        // walks up from `p`; bounded because the tree never contains cycles (that is the invariant)
        for _ in 0..=self.tree.len() {
            match p {
                Parent::Block(b) if b == anc => return true,
                Parent::Block(b) => match self.tree.get(&b) {
                    Some((pp, _)) => p = *pp,
                    None => return false,
                },
                _ => return false,
            }
        }
        panic!("cycle in tree")
    }
    fn do_move(&mut self, op: MoveOp) -> LogEntry {
        let old = self.tree.get(&op.child).cloned();
        let cyc = self.is_ancestor(op.child, op.parent);
        if !cyc {
            self.tree.insert(op.child, (op.parent, op.key.clone()));
        }
        LogEntry {
            op,
            old,
            applied: !cyc,
        }
    }
    fn undo(&mut self, e: &LogEntry) {
        if e.applied {
            match &e.old {
                Some(o) => self.tree.insert(e.op.child, o.clone()),
                None => self.tree.remove(&e.op.child),
            };
        }
    }
    /// Undo every newer entry, apply `op` at its place in timestamp order, redo the newer ones (they may now decide differently).
    fn integrate(&mut self, op: MoveOp) {
        if self.log.iter().any(|e| e.op.stamp == op.stamp) {
            return;
        }
        let at = self.log.partition_point(|e| e.op.stamp < op.stamp);
        let tail: Vec<LogEntry> = self.log.split_off(at);
        tail.iter().rev().for_each(|e| self.undo(e));
        self.redone += tail.len();
        let e = self.do_move(op);
        self.log.push(e);
        for t in tail {
            let e = self.do_move(t.op);
            self.log.push(e);
        }
    }
    fn children(&self, p: Parent) -> Vec<Id> {
        let mut v: Vec<(&Key, Id)> = self
            .tree
            .iter()
            .filter(|(_, (pp, _))| *pp == p)
            .map(|(c, (_, k))| (k, *c))
            .collect();
        v.sort();
        v.into_iter().map(|x| x.1).collect()
    }
}
const BASE: u32 = 1 << 16;
/// A key strictly between `lo` and `hi` (None = unbounded), lexicographic order, shorter prefix sorts first.
/// Needs `lo < hi` and keys that never end in digit 0: nothing fits between two EQUAL keys (or between `[5]` and `[5, 0]`).
fn between(lo: &[u32], hi: Option<&[u32]>) -> Key {
    assert!(
        hi.is_none_or(|h| lo < h),
        "between() needs lo < hi, got {lo:?} and {hi:?}"
    );
    let (mut out, mut tight) = (vec![], hi.is_some());
    for i in 0.. {
        let l = lo.get(i).copied().unwrap_or(0);
        let h = if tight {
            hi.unwrap().get(i).copied().unwrap_or(0)
        } else {
            BASE
        };
        if h > l + 1 {
            out.push((l + h) / 2);
            return out;
        }
        out.push(l);
        tight = tight && h == l;
    }
    unreachable!()
}

/// Two replicas inserting at the same spot compute the same `between` result, so every minted key gets a suffix derived from the op's dot.
/// `between` always diverges from `hi` BEFORE its last digit, so appending digits keeps the key inside the bounds. The last digit is never 0.
fn unique(mut key: Key, dot: Id) -> Key {
    assert!(
        dot.0 < BASE && dot.1 < 1 << 30,
        "dot does not fit the key suffix"
    );
    key.extend([dot.0, (dot.1 >> 15) as u32, (dot.1 & 0x7fff) as u32 + 1]);
    key
}

// ------------------------------------------------------------------ the document
#[derive(Clone, Debug, PartialEq, Default)]
struct Block {
    kind: Lww<Kind>,
    attrs: BTreeMap<String, Lww<String>>,
    text: Text,
    marks: Vec<(Stamp, MarkSpec)>,
}
#[derive(Clone, Debug, PartialEq, Default)]
struct Doc {
    blocks: BTreeMap<Id, Block>,
    mv: MoveState,
}
#[derive(Clone, Debug)]
enum Body {
    Move {
        child: Id,
        parent: Parent,
        key: Key,
    },
    Ins {
        block: Id,
        ch: char,
        parent: Id,
        side: u8,
    }, // the char id is the op's dot
    Del {
        block: Id,
        target: Id,
    },
    Mark {
        block: Id,
        spec: MarkSpec,
    },
    SetKind {
        block: Id,
        kind: Kind,
    },
    SetAttr {
        block: Id,
        name: String,
        val: String,
    },
}
#[derive(Clone, Debug)]
struct Op {
    dot: Id,
    deps: Vv,
    lam: u64,
    body: Body,
}
impl Doc {
    fn apply(&mut self, op: &Op) {
        let stamp: Stamp = (op.lam, op.dot.0);
        match &op.body {
            Body::Move { child, parent, key } => self.mv.integrate(MoveOp {
                stamp,
                child: *child,
                parent: *parent,
                key: key.clone(),
            }),
            Body::Ins {
                block,
                ch,
                parent,
                side,
            } => self.blocks.entry(*block).or_default().text.integrate(
                op.dot,
                CNode {
                    parent: *parent,
                    side: *side,
                    ch: *ch,
                    del: false,
                },
            ),
            Body::Del { block, target } => {
                if let Some(n) = self
                    .blocks
                    .entry(*block)
                    .or_default()
                    .text
                    .nodes
                    .get_mut(target)
                {
                    n.del = true;
                }
            }
            Body::Mark { block, spec } => {
                let b = self.blocks.entry(*block).or_default();
                if !b.marks.iter().any(|m| m.0 == stamp) {
                    b.marks.push((stamp, spec.clone()));
                    b.marks.sort_by_key(|m| m.0);
                }
            }
            Body::SetKind { block, kind } => {
                self.blocks.entry(*block).or_default().kind.set(stamp, kind)
            }
            Body::SetAttr { block, name, val } => self
                .blocks
                .entry(*block)
                .or_default()
                .attrs
                .entry(name.clone())
                .or_default()
                .set(stamp, val),
        }
    }
    fn is_live(&self, id: Id) -> bool {
        let mut cur = id;
        for _ in 0..=self.mv.tree.len() {
            match self.mv.tree.get(&cur) {
                Some((Parent::Root, _)) => return true,
                Some((Parent::Block(p), _)) => cur = *p,
                _ => return false,
            }
        }
        false
    }
    fn gap_index(&self, order: &[Id], g: Gap, is_start: bool) -> usize {
        match g.id {
            None => {
                if is_start {
                    0
                } else {
                    order.len()
                }
            }
            Some(id) => {
                let p = order
                    .iter()
                    .position(|x| *x == id)
                    .expect("anchor char must exist (causal delivery)");
                p + g.after as usize
            }
        }
    }
    /// Effective marks per visible char: for each kind the newest-stamped mark op that covers the char decides (add or remove).
    fn char_marks(&self, b: &Block) -> Vec<(char, BTreeMap<MarkKind, String>)> {
        let order = b.text.full_order();
        let mut eff: Vec<BTreeMap<MarkKind, (Stamp, Option<String>)>> =
            vec![BTreeMap::new(); order.len()];
        for (stamp, m) in &b.marks {
            let (s, e) = (
                self.gap_index(&order, m.start, true),
                self.gap_index(&order, m.end, false),
            );
            for slot in eff.iter_mut().take(e).skip(s) {
                let cur = slot.get(&m.kind).map(|x| x.0);
                if cur.is_none_or(|c| *stamp > c) {
                    slot.insert(
                        m.kind,
                        (
                            *stamp,
                            if m.remove {
                                None
                            } else {
                                Some(m.value.clone().unwrap_or_default())
                            },
                        ),
                    );
                }
            }
        }
        order
            .iter()
            .zip(eff)
            .filter(|(id, _)| !b.text.nodes[id].del)
            .map(|(id, e)| {
                (
                    b.text.nodes[id].ch,
                    e.into_iter()
                        .filter_map(|(k, (_, v))| v.map(|v| (k, v)))
                        .collect(),
                )
            })
            .collect()
    }
    /// Maximal runs of chars with identical marks.
    fn spans(&self, id: Id) -> Vec<(String, BTreeMap<MarkKind, String>)> {
        let Some(b) = self.blocks.get(&id) else {
            return vec![];
        };
        let mut out: Vec<(String, BTreeMap<MarkKind, String>)> = vec![];
        for (ch, m) in self.char_marks(b) {
            match out.last_mut() {
                Some(l) if l.1 == m => l.0.push(ch),
                _ => out.push((ch.to_string(), m)),
            }
        }
        out
    }
    fn kind(&self, id: Id) -> Kind {
        self.blocks.get(&id).map_or(Kind::Paragraph, |b| b.kind.val)
    }
    fn attr(&self, id: Id, name: &str) -> Option<&str> {
        self.blocks
            .get(&id)?
            .attrs
            .get(name)
            .map(|a| a.val.as_str())
    }
    fn inline_md(&self, id: Id) -> String {
        let mut s = String::new();
        for (t, m) in self.spans(id) {
            let mut piece = t;
            if m.contains_key(&MarkKind::Code) {
                piece = format!("`{piece}`");
            }
            if m.contains_key(&MarkKind::Italic) {
                piece = format!("*{piece}*");
            }
            if m.contains_key(&MarkKind::Bold) {
                piece = format!("**{piece}**");
            }
            if let Some(url) = m.get(&MarkKind::Link) {
                piece = format!("[{piece}]({url})");
            }
            s += &piece;
        }
        s
    }
    fn inline_html(&self, id: Id) -> String {
        let mut s = String::new();
        for (t, m) in self.spans(id) {
            let mut piece = esc(&t);
            for (k, tag) in [
                (MarkKind::Code, "code"),
                (MarkKind::Italic, "em"),
                (MarkKind::Bold, "strong"),
            ] {
                if m.contains_key(&k) {
                    piece = format!("<{tag}>{piece}</{tag}>");
                }
            }
            if let Some(url) = m.get(&MarkKind::Link) {
                piece = format!("<a href=\"{}\">{piece}</a>", esc(url));
            }
            s += &piece;
        }
        s
    }
    fn render_md(&self) -> String {
        let mut out = String::new();
        self.md_children(Parent::Root, 0, &mut out);
        out
    }
    fn md_children(&self, p: Parent, depth: usize, out: &mut String) {
        let ind = "  ".repeat(depth);
        let mut n = 0;
        for c in self.mv.children(p) {
            let k = self.kind(c);
            n = if k == Kind::Numbered { n + 1 } else { 0 };
            let t = self.inline_md(c);
            match k {
                Kind::Paragraph => writeln!(out, "{ind}{t}"),
                Kind::Heading(l) => {
                    writeln!(out, "{ind}{} {t}", "#".repeat(l.clamp(1, 6) as usize))
                }
                Kind::Bullet => writeln!(out, "{ind}- {t}"),
                Kind::Numbered => writeln!(out, "{ind}{n}. {t}"),
                Kind::Todo => writeln!(
                    out,
                    "{ind}- [{}] {t}",
                    if self.attr(c, "checked") == Some("true") {
                        'x'
                    } else {
                        ' '
                    }
                ),
                Kind::Quote => writeln!(out, "{ind}> {t}"),
                Kind::Code => writeln!(
                    out,
                    "{ind}```{}\n{ind}{}\n{ind}```",
                    self.attr(c, "lang").unwrap_or(""),
                    self.blocks
                        .get(&c)
                        .map_or(String::new(), |b| b.text.string())
                ),
                Kind::Divider => writeln!(out, "{ind}---"),
            }
            .unwrap();
            self.md_children(Parent::Block(c), depth + 1, out);
        }
    }
    fn render_html(&self) -> String {
        let mut out = String::new();
        self.html_children(Parent::Root, &mut out);
        out
    }
    fn html_children(&self, p: Parent, out: &mut String) {
        let kids = self.mv.children(p);
        let mut i = 0;
        while i < kids.len() {
            let k = self.kind(kids[i]);
            if matches!(k, Kind::Bullet | Kind::Numbered | Kind::Todo) {
                let tag = if k == Kind::Numbered { "ol" } else { "ul" };
                let class = if k == Kind::Todo {
                    " class=\"todo\""
                } else {
                    ""
                };
                write!(out, "<{tag}{class}>").unwrap();
                while i < kids.len() && self.kind(kids[i]) == k {
                    let c = kids[i];
                    let box_ = if k == Kind::Todo {
                        format!(
                            "<input type=\"checkbox\"{} disabled> ",
                            if self.attr(c, "checked") == Some("true") {
                                " checked"
                            } else {
                                ""
                            }
                        )
                    } else {
                        String::new()
                    };
                    write!(out, "<li>{box_}{}", self.inline_html(c)).unwrap();
                    self.html_children(Parent::Block(c), out);
                    out.push_str("</li>");
                    i += 1;
                }
                write!(out, "</{tag}>").unwrap();
                continue;
            }
            let c = kids[i];
            let t = self.inline_html(c);
            match k {
                Kind::Heading(l) => write!(out, "<h{0}>{t}</h{0}>", l.clamp(1, 6)),
                Kind::Quote => write!(out, "<blockquote>{t}</blockquote>"),
                Kind::Code => write!(
                    out,
                    "<pre><code class=\"lang-{}\">{}</code></pre>",
                    esc(self.attr(c, "lang").unwrap_or("")),
                    esc(&self
                        .blocks
                        .get(&c)
                        .map_or(String::new(), |b| b.text.string()))
                ),
                Kind::Divider => write!(out, "<hr>"),
                _ => write!(out, "<p>{t}</p>"),
            }
            .unwrap();
            let before = out.len();
            self.html_children(Parent::Block(c), out);
            if out.len() > before {
                out.insert_str(before, "<div class=\"nest\">");
                out.push_str("</div>");
            }
            i += 1;
        }
    }
}
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ------------------------------------------------------------------ replica: causal delivery + local editing API
struct Replica {
    id: Rep,
    doc: Doc,
    vv: Vv,
    lam: u64,
    pending: Vec<Op>,
    log: Vec<Op>,
    undo: Vec<Vec<Body>>,
    redo: Vec<Vec<Body>>,
}
impl Replica {
    fn new(id: Rep) -> Replica {
        Replica {
            id,
            doc: Doc::default(),
            vv: Vv::new(),
            lam: 0,
            pending: vec![],
            log: vec![],
            undo: vec![],
            redo: vec![],
        }
    }
    fn next_dot(&self) -> Id {
        (self.id, vget(&self.vv, self.id) + 1)
    }
    fn emit(&mut self, body: Body) -> (Op, Id) {
        self.lam += 1;
        let op = Op {
            dot: self.next_dot(),
            deps: self.vv.clone(),
            lam: self.lam,
            body,
        };
        self.doc.apply(&op);
        self.vv.insert(self.id, op.dot.1);
        self.log.push(op.clone());
        let d = op.dot;
        (op, d)
    }
    fn receive(&mut self, op: &Op) {
        if vget(&self.vv, op.dot.0) < op.dot.1 && !self.pending.iter().any(|p| p.dot == op.dot) {
            self.pending.push(op.clone());
        }
        while let Some(i) = self
            .pending
            .iter()
            .position(|o| o.dot.1 == vget(&self.vv, o.dot.0) + 1 && vle(&o.deps, &self.vv))
        {
            let o = self.pending.swap_remove(i);
            self.lam = self.lam.max(o.lam);
            self.doc.apply(&o);
            self.vv.insert(o.dot.0, o.dot.1);
            self.log.push(o);
        }
    }
    // ---- structure
    /// The sibling key for the NEXT op this replica emits (it is made unique with that op's dot). Trash is unordered: its key is empty.
    fn key_for(&self, parent: Parent, index: usize, exclude: Option<Id>) -> Key {
        if parent == Parent::Trash {
            return vec![];
        }
        let sibs: Vec<Id> = self
            .doc
            .mv
            .children(parent)
            .into_iter()
            .filter(|c| Some(*c) != exclude)
            .collect();
        let index = index.min(sibs.len());
        let lo = if index == 0 {
            vec![]
        } else {
            self.doc.mv.tree[&sibs[index - 1]].1.clone()
        };
        let hi = sibs.get(index).map(|c| self.doc.mv.tree[c].1.clone());
        unique(between(&lo, hi.as_deref()), self.next_dot())
    }
    fn new_block(&mut self, parent: Parent, index: usize, kind: Kind) -> (Vec<Op>, Id) {
        let key = self.key_for(parent, index, None);
        let child = self.next_dot();
        let (o1, _) = self.emit(Body::Move { child, parent, key });
        let mut ops = vec![o1];
        if kind != Kind::Paragraph {
            ops.push(self.emit(Body::SetKind { block: child, kind }).0);
        }
        self.undo.push(vec![Body::Move {
            child,
            parent: Parent::Trash,
            key: vec![],
        }]);
        self.redo.clear();
        (ops, child)
    }
    fn move_block(&mut self, child: Id, parent: Parent, index: usize) -> Op {
        let old = self.doc.mv.tree.get(&child).cloned();
        let key = self.key_for(parent, index, Some(child));
        if let Some((p, k)) = old {
            self.undo.push(vec![Body::Move {
                child,
                parent: p,
                key: k,
            }]);
            self.redo.clear();
        }
        self.emit(Body::Move { child, parent, key }).0
    }
    fn delete_block(&mut self, child: Id) -> Op {
        self.move_block(child, Parent::Trash, 0)
    }
    fn set_kind(&mut self, block: Id, kind: Kind) -> Op {
        self.undo.push(vec![Body::SetKind {
            block,
            kind: self.doc.kind(block),
        }]);
        self.redo.clear();
        self.emit(Body::SetKind { block, kind }).0
    }
    fn set_attr(&mut self, block: Id, name: &str, val: &str) -> Op {
        self.emit(Body::SetAttr {
            block,
            name: name.into(),
            val: val.into(),
        })
        .0
    }
    fn invert_stack(&mut self, from_undo: bool) -> Vec<Op> {
        let Some(bodies) = (if from_undo {
            self.undo.pop()
        } else {
            self.redo.pop()
        }) else {
            return vec![];
        };
        let mut ops = vec![];
        let mut inverse = vec![];
        for b in bodies {
            // record how to undo THIS application before applying it
            match &b {
                Body::Move { child, .. } => {
                    if let Some((p, k)) = self.doc.mv.tree.get(child).cloned() {
                        inverse.push(Body::Move {
                            child: *child,
                            parent: p,
                            key: k,
                        });
                    }
                }
                Body::SetKind { block, .. } => inverse.push(Body::SetKind {
                    block: *block,
                    kind: self.doc.kind(*block),
                }),
                _ => {}
            }
            ops.push(self.emit(b).0);
        }
        if from_undo {
            self.redo.push(inverse)
        } else {
            self.undo.push(inverse)
        }
        ops
    }
    fn undo_block_op(&mut self) -> Vec<Op> {
        self.invert_stack(true)
    }
    fn redo_block_op(&mut self) -> Vec<Op> {
        self.invert_stack(false)
    }
    // ---- text
    fn insert_text(&mut self, block: Id, idx: usize, s: &str) -> Vec<Op> {
        let mut ops = vec![];
        for (k, ch) in s.chars().enumerate() {
            // reading must not create the block entry: state may only change through ops, or replicas differ
            let (parent, side) = self
                .doc
                .blocks
                .get(&block)
                .map_or((ROOT, 1), |b| b.text.place(idx + k));
            ops.push(
                self.emit(Body::Ins {
                    block,
                    ch,
                    parent,
                    side,
                })
                .0,
            );
        }
        ops
    }
    fn delete_text(&mut self, block: Id, idx: usize, len: usize) -> Vec<Op> {
        let ids: Vec<Id> = self
            .doc
            .blocks
            .get(&block)
            .map(|b| b.text.visible())
            .unwrap_or_default()
            .into_iter()
            .skip(idx)
            .take(len)
            .collect();
        ids.into_iter()
            .map(|target| self.emit(Body::Del { block, target }).0)
            .collect()
    }
    fn text_len(&self, block: Id) -> usize {
        self.doc
            .blocks
            .get(&block)
            .map_or(0, |b| b.text.visible().len())
    }
    /// Apply or remove a mark over visible chars [i, j) using the expand rules for `kind`.
    fn mark(
        &mut self,
        block: Id,
        i: usize,
        j: usize,
        kind: MarkKind,
        value: Option<&str>,
        remove: bool,
    ) -> Option<Op> {
        let vis = self.doc.blocks.get(&block)?.text.visible();
        if i >= j || j > vis.len() {
            return None;
        }
        let grow_end = expands(kind);
        let start = if grow_end && i == 0 {
            Gap {
                id: None,
                after: true,
            }
        } else {
            Gap {
                id: Some(vis[i]),
                after: false,
            }
        };
        let end = if grow_end {
            Gap {
                id: vis.get(j).copied(),
                after: false,
            }
        } else {
            Gap {
                id: Some(vis[j - 1]),
                after: true,
            }
        };
        Some(
            self.emit(Body::Mark {
                block,
                spec: MarkSpec {
                    start,
                    end,
                    kind,
                    value: value.map(String::from),
                    remove,
                },
            })
            .0,
        )
    }
    // ---- split / merge (composites of ordinary ops)
    /// Text edits have no inverse here, so undoing only the structural half of a split or merge would hide or duplicate text. Forget both stacks.
    fn undo_barrier(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
    fn split(&mut self, block: Id, at: usize) -> Vec<Op> {
        let Some(&(parent, _)) = self.doc.mv.tree.get(&block) else {
            return vec![];
        };
        let chars = self
            .doc
            .blocks
            .get(&block)
            .map(|b| self.doc.char_marks(b))
            .unwrap_or_default();
        if at > chars.len() {
            return vec![];
        }
        let idx = self
            .doc
            .mv
            .children(parent)
            .iter()
            .position(|c| *c == block)
            .unwrap_or(0)
            + 1;
        let kind = self.doc.kind(block);
        let (mut ops, nb) = self.new_block(
            parent,
            idx,
            if matches!(kind, Kind::Divider | Kind::Code) {
                Kind::Paragraph
            } else {
                kind
            },
        );
        ops.extend(self.copy_with_marks(&chars[at..], nb, 0));
        ops.extend(self.delete_text(block, at, chars.len() - at));
        self.undo_barrier();
        ops
    }
    fn copy_with_marks(
        &mut self,
        chars: &[(char, BTreeMap<MarkKind, String>)],
        dst: Id,
        at: usize,
    ) -> Vec<Op> {
        let text: String = chars.iter().map(|c| c.0).collect();
        let mut ops = self.insert_text(dst, at, &text);
        for kind in [
            MarkKind::Link,
            MarkKind::Bold,
            MarkKind::Italic,
            MarkKind::Code,
        ] {
            let mut i = 0;
            while i < chars.len() {
                if let Some(v) = chars[i].1.get(&kind) {
                    let mut j = i;
                    while j < chars.len() && chars[j].1.get(&kind) == Some(v) {
                        j += 1;
                    }
                    ops.extend(self.mark(dst, at + i, at + j, kind, Some(v), false));
                    i = j;
                } else {
                    i += 1;
                }
            }
        }
        ops
    }
    fn merge_into_prev(&mut self, block: Id) -> Vec<Op> {
        let Some(&(parent, _)) = self.doc.mv.tree.get(&block) else {
            return vec![];
        };
        let sibs = self.doc.mv.children(parent);
        let Some(pos) = sibs.iter().position(|c| *c == block) else {
            return vec![];
        };
        if pos == 0 {
            return vec![];
        }
        let prev = sibs[pos - 1];
        let chars = self
            .doc
            .blocks
            .get(&block)
            .map(|b| self.doc.char_marks(b))
            .unwrap_or_default();
        let at = self.text_len(prev);
        let mut ops = self.copy_with_marks(&chars, prev, at);
        // nested blocks survive a Backspace: move them under `prev`, after its own children, before the emptied block is trashed
        for child in self.doc.mv.children(Parent::Block(block)) {
            let n = self.doc.mv.children(Parent::Block(prev)).len();
            ops.push(self.move_block(child, Parent::Block(prev), n));
        }
        ops.push(self.delete_block(block));
        self.undo_barrier();
        ops
    }
}

// ------------------------------------------------------------------ simulation
struct Net {
    reps: Vec<Replica>,
    queue: Vec<(usize, Op)>,
    side: Option<Vec<bool>>,
    rng: Rng,
}
impl Net {
    fn new(n: usize, seed: u64) -> Net {
        Net {
            reps: (1..=n as u32).map(Replica::new).collect(),
            queue: vec![],
            side: None,
            rng: Rng::new(seed),
        }
    }
    fn broadcast(&mut self, from: usize, ops: Vec<Op>) {
        for op in ops {
            for to in (0..self.reps.len()).filter(|t| *t != from) {
                self.queue.push((to, op.clone()));
                if self.rng.chance(12) {
                    self.queue.push((to, op.clone())); // duplicate
                }
            }
        }
    }
    fn deliver_one(&mut self) {
        let ok: Vec<usize> = (0..self.queue.len())
            .filter(|i| {
                self.side
                    .as_ref()
                    .is_none_or(|s| s[self.queue[*i].0] == s[self.queue[*i].1.dot.0 as usize - 1])
            })
            .collect();
        if ok.is_empty() {
            return;
        }
        let i = ok[self.rng.below(ok.len())];
        let (to, op) = self.queue.swap_remove(i);
        self.reps[to].receive(&op);
    }
    fn flush(&mut self) {
        self.side = None;
        while !self.queue.is_empty() {
            let i = self.rng.below(self.queue.len());
            let (to, op) = self.queue.swap_remove(i);
            self.reps[to].receive(&op);
        }
    }
    fn random_action(&mut self, a: usize) {
        let rng = &mut self.rng;
        let r = &mut self.reps[a];
        let live: Vec<Id> = r
            .doc
            .mv
            .tree
            .keys()
            .copied()
            .filter(|b| r.doc.is_live(*b))
            .collect();
        let kinds = [
            Kind::Paragraph,
            Kind::Heading(1),
            Kind::Heading(2),
            Kind::Bullet,
            Kind::Numbered,
            Kind::Todo,
            Kind::Quote,
            Kind::Code,
            Kind::Divider,
        ];
        let pick = |rng: &mut Rng| live[rng.below(live.len())];
        let ops = if live.is_empty() || rng.chance(14) {
            let parent = if live.is_empty() || rng.chance(50) {
                Parent::Root
            } else {
                Parent::Block(pick(rng))
            };
            let n = r.doc.mv.children(parent).len();
            r.new_block(parent, rng.below(n + 1), kinds[rng.below(kinds.len())])
                .0
        } else {
            let b = pick(rng);
            match rng.below(100) {
                0..=9 => {
                    let parent = if rng.chance(30) {
                        Parent::Root
                    } else {
                        Parent::Block(pick(rng))
                    }; // may target a descendant: a cycle attempt
                    let n = r.doc.mv.children(parent).len();
                    vec![r.move_block(b, parent, rng.below(n + 1))]
                }
                10..=13 => vec![r.delete_block(b)],
                14..=18 => vec![r.set_kind(b, kinds[rng.below(kinds.len())])],
                19..=21 => {
                    vec![r.set_attr(b, "checked", if rng.chance(50) { "true" } else { "false" })]
                }
                22..=56 => {
                    let n = r.text_len(b);
                    let s = ["ab", "x", "hello ", "ç"][rng.below(4)];
                    r.insert_text(b, rng.below(n + 1), s)
                }
                57..=66 => {
                    let n = r.text_len(b);
                    if n == 0 {
                        vec![]
                    } else {
                        let i = rng.below(n);
                        r.delete_text(b, i, 1 + rng.below((n - i).min(3)))
                    }
                }
                67..=82 => {
                    let n = r.text_len(b);
                    if n < 2 {
                        vec![]
                    } else {
                        let (i, j) = (rng.below(n - 1), 0);
                        let j = (i + 1 + rng.below(n - i - 1 + j)).min(n);
                        let kind = [
                            MarkKind::Bold,
                            MarkKind::Italic,
                            MarkKind::Link,
                            MarkKind::Code,
                        ][rng.below(4)];
                        r.mark(b, i, j, kind, Some("u"), rng.chance(30))
                            .into_iter()
                            .collect()
                    }
                }
                83..=89 => {
                    let n = r.text_len(b);
                    let at = rng.below(n + 1);
                    r.split(b, at)
                }
                90..=93 => r.merge_into_prev(b),
                94..=96 => r.undo_block_op(),
                _ => r.redo_block_op(),
            }
        };
        self.broadcast(a, ops);
    }
}

fn check_tree_invariants(d: &Doc) {
    // sibling keys are distinct (outside the unordered Trash), so a later insert always finds room between two neighbours
    let mut keys = BTreeSet::new();
    for (parent, key) in d.mv.tree.values().filter(|v| v.0 != Parent::Trash) {
        assert!(
            keys.insert(key),
            "duplicate sibling key {key:?} under {parent:?}"
        );
    }
    // every block has exactly one parent (map) and following parents always reaches Root/Trash without a cycle
    for c in d.mv.tree.keys() {
        let (mut cur, mut steps) = (*c, 0);
        while let Some((Parent::Block(p), _)) = d.mv.tree.get(&cur) {
            cur = *p;
            steps += 1;
            assert!(
                steps <= d.mv.tree.len(),
                "cycle detected through block {c:?}"
            );
        }
    }
    let total: usize = [Parent::Root, Parent::Trash]
        .iter()
        .map(|p| d.mv.children(*p).len())
        .sum::<usize>()
        + d.mv
            .tree
            .keys()
            .map(|b| d.mv.children(Parent::Block(*b)).len())
            .sum::<usize>();
    assert_eq!(
        total,
        d.mv.tree.len(),
        "every block must appear under exactly one parent"
    );
}

fn schedule(seed: u64) -> usize {
    let mut rng = Rng::new(seed);
    let n = 3 + rng.below(2);
    let mut net = Net::new(n, seed ^ 0x55);
    for _ in 0..50 + rng.below(70) {
        let a = net.rng.below(n);
        match net.rng.below(100) {
            0..=62 => net.random_action(a),
            63..=87 => net.deliver_one(),
            _ => {
                net.side = if net.side.is_some() {
                    None
                } else {
                    Some((0..n).map(|_| net.rng.chance(50)).collect())
                }
            }
        }
        // the tree must be sound in EVERY intermediate state, not only after everything has arrived
        net.reps.iter().for_each(|r| check_tree_invariants(&r.doc));
    }
    net.flush();
    let r0 = &net.reps[0];
    for r in &net.reps {
        assert!(r.pending.is_empty(), "stuck ops (seed {seed})");
        check_tree_invariants(&r.doc);
        assert_eq!(
            r.doc.render_md(),
            r0.doc.render_md(),
            "markdown diverged (seed {seed})"
        );
        assert_eq!(
            r.doc.mv.tree, r0.doc.mv.tree,
            "trees diverged (seed {seed})"
        );
        assert_eq!(
            r.doc.mv.log, r0.doc.mv.log,
            "move logs diverged (seed {seed})"
        );
        // the whole replicated state, including tombstones, attributes and mark ops that the renderers might not show
        assert!(
            r.doc.blocks == r0.doc.blocks,
            "block content diverged (seed {seed})"
        );
        assert_eq!(
            r.doc.render_html(),
            r0.doc.render_html(),
            "html diverged (seed {seed})"
        );
    }
    net.reps.iter().map(|r| r.doc.mv.redone).sum()
}

// ------------------------------------------------------------------ Peritext-style scenarios + move scenarios
fn pair() -> (Replica, Replica) {
    (Replica::new(1), Replica::new(2))
}
fn sync(a: &mut Replica, b: &mut Replica) {
    let (la, lb) = (a.log.clone(), b.log.clone());
    la.iter().for_each(|o| b.receive(o));
    lb.iter().for_each(|o| a.receive(o));
    assert_eq!(
        a.doc.render_html(),
        b.doc.render_html(),
        "replicas must converge"
    );
}
fn base_doc(text: &str) -> (Replica, Replica, Id) {
    let (mut a, mut b) = pair();
    let (_, blk) = a.new_block(Parent::Root, 0, Kind::Paragraph);
    a.insert_text(blk, 0, text);
    sync(&mut a, &mut b);
    (a, b, blk)
}
fn html_of(r: &Replica) -> String {
    r.doc.render_html()
}
/// The numbered examples of the Peritext essay, with its sentences. (5 and 7 need colours and comments, which this model does not have.)
fn peritext_scenarios() {
    const S: &str = "The fox jumped.";
    let check = |a: &Replica, b: &Replica, want: &str, ex: &str| {
        assert_eq!(html_of(a), want, "Peritext {ex}");
        assert_eq!(html_of(b), want, "Peritext {ex} (second replica)");
    };
    // Example 1: concurrent plain-text insertions stay where they were typed.
    let (mut a, mut b, blk) = base_doc(S);
    a.insert_text(blk, 4, "quick ");
    b.insert_text(blk, 14, " over the dog");
    sync(&mut a, &mut b);
    check(
        &a,
        &b,
        "<p>The quick fox jumped over the dog.</p>",
        "example 1",
    );
    // Example 2: Alice bolds the sentence while Bob inserts a word: the new word is bold too.
    let (mut a, mut b, blk) = base_doc(S);
    a.mark(blk, 0, 15, MarkKind::Bold, None, false);
    b.insert_text(blk, 4, "brown ");
    sync(&mut a, &mut b);
    check(
        &a,
        &b,
        "<p><strong>The brown fox jumped.</strong></p>",
        "example 2",
    );
    // Example 3: overlapping bold: the union is bold.
    let (mut a, mut b, blk) = base_doc(S);
    a.mark(blk, 0, 7, MarkKind::Bold, None, false); // "The fox"
    b.mark(blk, 4, 15, MarkKind::Bold, None, false); // "fox jumped."
    sync(&mut a, &mut b);
    check(
        &a,
        &b,
        "<p><strong>The fox jumped.</strong></p>",
        "example 3",
    );
    // Example 4: overlapping bold and italic: the overlap has both.
    let (mut a, mut b, blk) = base_doc(S);
    a.mark(blk, 0, 7, MarkKind::Bold, None, false);
    b.mark(blk, 4, 15, MarkKind::Italic, None, false);
    sync(&mut a, &mut b);
    check(
        &a,
        &b,
        "<p><strong>The </strong><strong><em>fox</em></strong><em> jumped.</em></p>",
        "example 4",
    );
    // Example 6: Alice bolds all, then un-bolds "fox jumped."; Bob bolds "jumped.". "jumped." is a real conflict: either answer is
    // allowed but all replicas must pick the same one. Here Alice's un-bold has the newer stamp (it is her second op).
    let (mut a, mut b, blk) = base_doc(S);
    a.mark(blk, 0, 15, MarkKind::Bold, None, false);
    a.mark(blk, 4, 15, MarkKind::Bold, None, true);
    b.mark(blk, 8, 15, MarkKind::Bold, None, false);
    sync(&mut a, &mut b);
    check(
        &a,
        &b,
        "<p><strong>The </strong>fox jumped.</p>",
        "example 6",
    );
    // Examples 8 and 9: text typed before a span stays out; text typed after it joins bold but not a link.
    for (kind, value, want) in [
        (
            MarkKind::Bold,
            None,
            "<p>The quick <strong>fox jumped over the dog</strong>.</p>",
        ),
        (
            MarkKind::Link,
            Some("http://x"),
            "<p>The quick <a href=\"http://x\">fox jumped</a> over the dog.</p>",
        ),
    ] {
        let (mut a, mut b, blk) = base_doc(S);
        a.mark(blk, 4, 14, kind, value, false); // "fox jumped"
        sync(&mut a, &mut b);
        a.insert_text(blk, 4, "quick ");
        a.insert_text(blk, 20, " over the dog");
        sync(&mut a, &mut b);
        check(&a, &b, want, "examples 8/9");
    }
    // Essay, same section: a link and a bold span that end on the same character: later text is bold but not linked.
    // At the START of a paragraph there is no preceding character, so text typed there takes the following character's bold (never its link).
    let (mut a, mut b, blk) = base_doc("fox now");
    a.mark(blk, 0, 3, MarkKind::Bold, None, false);
    a.mark(blk, 0, 3, MarkKind::Link, Some("http://x"), false);
    sync(&mut a, &mut b);
    b.insert_text(blk, 3, "es"); // at the end boundary
    b.insert_text(blk, 0, ">"); // at the start of the paragraph
    sync(&mut a, &mut b);
    check(
        &a,
        &b,
        "<p><strong>&gt;</strong><a href=\"http://x\"><strong>fox</strong></a><strong>es</strong> now</p>",
        "shared boundary + paragraph start",
    );
    // Deleting the characters a mark is anchored to: the remaining marked text keeps its mark (anchors point at tombstones).
    let (mut a, mut b, blk) = base_doc("0123456789");
    a.mark(blk, 2, 8, MarkKind::Italic, None, false);
    sync(&mut a, &mut b);
    b.delete_text(blk, 1, 3); // removes '1','2','3' (including the first marked chars)
    a.insert_text(blk, 5, "X");
    sync(&mut a, &mut b);
    check(&a, &b, "<p>0<em>4X567</em>89</p>", "deleted anchors");
    println!(
        "  Peritext essay examples 1, 2, 3, 4, 6, 8, 9 + shared-boundary, paragraph-start and deleted-anchor cases OK"
    );
}

fn move_scenarios() {
    // Cycle: A moves X under Y, B moves Y under X, concurrently. Exactly one move is skipped, on every replica.
    let (mut a, mut b) = pair();
    let (_, x) = a.new_block(Parent::Root, 0, Kind::Bullet);
    let (_, y) = a.new_block(Parent::Root, 1, Kind::Bullet);
    sync(&mut a, &mut b);
    a.move_block(x, Parent::Block(y), 0);
    b.move_block(y, Parent::Block(x), 0);
    sync(&mut a, &mut b);
    check_tree_invariants(&a.doc);
    assert_eq!(a.doc.mv.tree, b.doc.mv.tree);
    let skipped = a.doc.mv.log.iter().filter(|e| !e.applied).count();
    assert_eq!(skipped, 1, "exactly one of the two cyclic moves is skipped");
    println!("  concurrent cyclic moves: converged, tree acyclic, {skipped} move skipped");
    // Duplicate-free: concurrent moves of the same block to different parents leave it under exactly ONE parent (the later stamp).
    let (mut a, mut b) = pair();
    let (_, p1) = a.new_block(Parent::Root, 0, Kind::Paragraph);
    let (_, p2) = a.new_block(Parent::Root, 1, Kind::Paragraph);
    let (_, c) = a.new_block(Parent::Root, 2, Kind::Paragraph);
    sync(&mut a, &mut b);
    a.move_block(c, Parent::Block(p1), 0);
    b.move_block(c, Parent::Block(p2), 0);
    sync(&mut a, &mut b);
    check_tree_invariants(&a.doc);
    let parents: Vec<Parent> = [Parent::Block(p1), Parent::Block(p2), Parent::Root]
        .into_iter()
        .filter(|p| a.doc.mv.children(*p).contains(&c))
        .collect();
    assert_eq!(
        parents.len(),
        1,
        "a moved block must have exactly one parent"
    );
    println!(
        "  concurrent moves of one block: exactly one parent wins ({:?})",
        parents[0]
    );
    // Concurrent inserts at the SAME sibling position must still leave room between the two new blocks.
    // (Without the per-op key suffix both replicas mint the same key and the third insert can never find a key between equal ones.)
    let (mut a, mut b) = pair();
    let (_, left) = a.new_block(Parent::Root, 0, Kind::Paragraph);
    let (_, right) = a.new_block(Parent::Root, 1, Kind::Paragraph);
    sync(&mut a, &mut b);
    let (_, from_a) = a.new_block(Parent::Root, 1, Kind::Paragraph);
    let (_, from_b) = b.new_block(Parent::Root, 1, Kind::Paragraph);
    sync(&mut a, &mut b);
    assert_ne!(
        a.doc.mv.tree[&from_a].1, a.doc.mv.tree[&from_b].1,
        "concurrently minted sibling keys must differ"
    );
    let (_, mid) = a.new_block(Parent::Root, 2, Kind::Paragraph);
    sync(&mut a, &mut b);
    let order = a.doc.mv.children(Parent::Root);
    assert_eq!(order.len(), 5);
    assert_eq!(
        (order[0], order[2], order[4]),
        (left, mid, right),
        "the third insert lands exactly between the two concurrent ones"
    );
    assert!(order[1] == from_a || order[1] == from_b);
    println!(
        "  concurrent inserts at one position: keys stay distinct, a later insert fits between them"
    );
}

fn undo_scenario() {
    let mut r = Replica::new(1);
    let (_, a) = r.new_block(Parent::Root, 0, Kind::Bullet);
    let (_, b) = r.new_block(Parent::Root, 1, Kind::Bullet);
    let (_, c) = r.new_block(Parent::Root, 2, Kind::Paragraph);
    r.undo.clear();
    let snap = r.doc.render_md();
    r.move_block(c, Parent::Block(a), 0);
    r.set_kind(b, Kind::Heading(2));
    r.delete_block(a);
    assert_ne!(r.doc.render_md(), snap);
    for _ in 0..3 {
        r.undo_block_op();
    }
    assert_eq!(
        r.doc.mv.children(Parent::Root),
        vec![a, b, c],
        "undo restores structure"
    );
    assert_eq!(r.doc.kind(b), Kind::Bullet);
    for _ in 0..3 {
        r.redo_block_op();
    }
    assert_eq!(r.doc.mv.children(Parent::Trash), vec![a]);
    assert_eq!(r.doc.kind(b), Kind::Heading(2));
    println!("  block-level undo/redo of move, kind change and delete OK");
}

fn split_merge_scenarios() {
    let (mut a, mut b, blk) = base_doc("Hello world");
    a.mark(blk, 6, 11, MarkKind::Bold, None, false);
    sync(&mut a, &mut b); // the split below depends causally on this mark: without it `b` must hold the split ops back
    let before = html_of(&a);
    let ops = a.split(blk, 6);
    ops.iter().for_each(|o| b.receive(o));
    assert!(b.pending.is_empty(), "split ops stuck in the causal buffer");
    assert_eq!(html_of(&a), html_of(&b));
    assert_eq!(a.doc.render_md(), "Hello \n**world**\n");
    // merge back: text and marks return to one block
    let nb = a.doc.mv.children(Parent::Root)[1];
    let ops = a.merge_into_prev(nb);
    ops.iter().for_each(|o| b.receive(o));
    assert_eq!(html_of(&a), before);
    assert_eq!(html_of(&b), before);
    // concurrent Enter at the same place: documented behaviour = two new blocks, no lost or reordered text, replicas agree
    let (mut a, mut b, blk) = base_doc("abcd");
    a.split(blk, 2);
    b.split(blk, 2);
    sync(&mut a, &mut b);
    println!(
        "  concurrent Enter at the same spot -> {:?}",
        a.doc.render_md()
    );
    assert_eq!(a.doc.render_md().lines().next(), Some("ab"));
    assert_eq!(a.doc.render_md(), "ab\ncd\ncd\n");
    // text typed INSIDE a tail that someone else is moving stays in the old block
    let (mut a, mut b, blk) = base_doc("abcd");
    a.split(blk, 2);
    b.insert_text(blk, 3, "X");
    sync(&mut a, &mut b);
    assert_eq!(a.doc.render_md(), "abX\ncd\n");
    // two users press Backspace on the same block: its text is appended twice
    let (mut a, mut b, blk) = base_doc("one");
    let (_, second) = a.new_block(Parent::Root, 1, Kind::Paragraph);
    a.insert_text(second, 0, "two");
    sync(&mut a, &mut b);
    a.merge_into_prev(second);
    b.merge_into_prev(second);
    sync(&mut a, &mut b);
    assert_eq!(
        a.doc
            .spans(blk)
            .iter()
            .map(|s| s.0.as_str())
            .collect::<String>(),
        "onetwotwo"
    );
    // Backspace on a block with nested children: the children move to the surviving block, after its own children
    let (mut a, mut b, first) = base_doc("first");
    let (_, second) = a.new_block(Parent::Root, 1, Kind::Paragraph);
    let (_, kid0) = a.new_block(Parent::Block(first), 0, Kind::Bullet);
    let (_, kid1) = a.new_block(Parent::Block(second), 0, Kind::Bullet);
    let (_, kid2) = a.new_block(Parent::Block(second), 1, Kind::Bullet);
    sync(&mut a, &mut b);
    a.merge_into_prev(second);
    sync(&mut a, &mut b);
    assert_eq!(
        a.doc.mv.children(Parent::Block(first)),
        vec![kid0, kid1, kid2],
        "merge must not hide nested blocks"
    );
    assert!([kid1, kid2].iter().all(|k| a.doc.is_live(*k)));
    // split and merge are an undo barrier: a structure-only undo of half a split would hide the moved text
    let mut r = Replica::new(1);
    let (_, blk) = r.new_block(Parent::Root, 0, Kind::Paragraph);
    r.insert_text(blk, 0, "abcd");
    r.split(blk, 2);
    assert!(r.undo_block_op().is_empty() && r.redo_block_op().is_empty());
    assert_eq!(r.doc.render_md(), "ab\ncd\n");
    println!(
        "  split/merge with marks and nested children OK; documented concurrent outcomes asserted"
    );
}

fn demo_session() {
    println!("Scripted session: Alice, Bob and Carol edit one page while Carol is offline");
    let mut net = Net::new(3, 1);
    let (ops, title) = net.reps[0].new_block(Parent::Root, 0, Kind::Heading(1));
    let mut all = ops;
    all.extend(net.reps[0].insert_text(title, 0, "Trip plan"));
    let (o, list1) = net.reps[0].new_block(Parent::Root, 1, Kind::Bullet);
    all.extend(o);
    all.extend(net.reps[0].insert_text(list1, 0, "Book flights"));
    let (o, list2) = net.reps[0].new_block(Parent::Root, 2, Kind::Todo);
    all.extend(o);
    all.extend(net.reps[0].insert_text(list2, 0, "Pack bags"));
    net.broadcast(0, all);
    net.flush();
    // Carol goes offline: her edits are queued (not delivered) until the end.
    net.side = Some(vec![false, false, true]);
    let o = net.reps[2].insert_text(list1, 12, " (cheap!)");
    net.broadcast(2, o);
    let o: Vec<Op> = net.reps[2]
        .mark(list1, 0, 4, MarkKind::Bold, None, false)
        .into_iter()
        .collect();
    net.broadcast(2, o);
    let (o, sub) = net.reps[2].new_block(Parent::Block(list1), 0, Kind::Bullet);
    net.broadcast(2, o);
    let o = net.reps[2].insert_text(sub, 0, "Check airline sites");
    net.broadcast(2, o);
    // Alice and Bob: Alice moves the todo under the heading's list, Bob links a word and checks the todo.
    let o = vec![net.reps[0].move_block(list2, Parent::Block(list1), 0)];
    net.broadcast(0, o);
    let o: Vec<Op> = net.reps[1]
        .mark(
            list1,
            5,
            12,
            MarkKind::Link,
            Some("https://example.com/flights"),
            false,
        )
        .into_iter()
        .collect();
    net.broadcast(1, o);
    let o = vec![net.reps[1].set_attr(list2, "checked", "true")];
    net.broadcast(1, o);
    for _ in 0..40 {
        net.deliver_one();
    }
    println!(
        "--- Alice's view while Carol is offline (Markdown):\n{}",
        net.reps[0].doc.render_md()
    );
    net.flush();
    assert_eq!(net.reps[0].doc.render_md(), net.reps[2].doc.render_md());
    println!(
        "--- after Carol reconnects (Markdown, identical on all three):\n{}",
        net.reps[2].doc.render_md()
    );
    println!("--- HTML:\n{}", net.reps[1].doc.render_html());
    assert!(net.reps[0].doc.render_md().contains("**Book**"));
    assert!(
        net.reps[0]
            .doc
            .render_md()
            .contains("[flights](https://example.com/flights)")
    );
}

fn main() {
    println!("Peritext-style inline marks");
    peritext_scenarios();
    println!("Movable tree");
    move_scenarios();
    println!("Undo / redo");
    undo_scenario();
    println!("Split / merge");
    split_merge_scenarios();
    let t = std::time::Instant::now();
    let mut redone = 0;
    for seed in 0..2000 {
        redone += schedule(seed * 13 + 1);
    }
    println!(
        "Convergence: 2000 random schedules (3-4 replicas, text/marks/blocks/moves/kinds/split/merge/undo, partitions, reordered + duplicated delivery) OK in {:?}",
        t.elapsed()
    );
    println!(
        "  tree acyclic and single-parent on every replica; {redone} moves undone/redone by out-of-order arrivals"
    );
    demo_session();
    println!("OK");
}
