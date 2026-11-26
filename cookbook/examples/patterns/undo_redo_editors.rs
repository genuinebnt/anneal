//! RECIPE 238 (patterns): undo/redo for a text editor: command, memento, persistent rope, piece table and undo tree, tested against a model
//! LEVEL: advanced
//! Use in: editors, drawing tools, form builders, spreadsheets, any app with history; also persistent data structures and version control ideas.   Run: cargo run --release --example undo_redo_editors
#![allow(dead_code)]
//!
//! Five ways to remember the past, all behind one `Editor` trait and all checked against a deliberately naive MODEL
//! (a vector of full document states plus a cursor into it), so any disagreement points at the design, not at the test.
//! (a) COMMAND: each edit stores its inverse (insert <-> delete with the removed text). Tiny entries, but every operation
//!     needs a correct inverse, and undo replays work. Consecutive typing is COALESCED into one undo step (a burst of
//!     adjacent inserts, capped at 16 chars), as users expect; any undo/redo or non-adjacent edit ends the burst.
//! (b) MEMENTO: copy the whole document per step. Trivial and obviously correct, but cost is O(document) per entry.
//! (c) PERSISTENT: the document is an immutable rope of `Arc` nodes. An edit rebuilds only the path to the change and
//!     SHARES everything else, so a snapshot is one pointer copy and entries cost O(log n) new nodes (sharing is asserted
//!     with `Arc::ptr_eq` and strong counts). This is the idea behind persistent vectors and git trees.
//! (d) PIECE TABLE: the original text and an append-only add buffer never change; the document is a list of (buffer, start,
//!     len) pieces. Edits touch O(pieces) and undo restores an older piece list; text is never copied or moved.
//! (e) UNDO TREE: history is a tree. Undo then a new edit creates a BRANCH instead of discarding the redo stack, and any
//!     state can be reached (goto = undo to the common ancestor, then redo down the other branch); nothing is ever lost.
//! Properties (20k seeded random sequences): undo followed by redo is the identity; any edits followed by undoing everything
//! returns the initial text and redoing everything returns the final text; every design equals the model after every step.
//! COMMON USES: text and code editors (VS Code uses a piece-tree buffer; Vim has an undo tree), Photoshop-style history,
//! CAVEAT: the piece-table design here snapshots by CLONING the whole piece vector, so its per-history-entry memory is larger than the memento in the printed table. A production piece table shares structure (persistent tree) or stores deltas, which is what makes it cheap; read the table as "naive snapshots", not as a verdict on piece tables.
//! collaborative editing (operations have inverses), Emacs undo, database transactions (savepoints), game save states.
//! Read: https://refactoring.guru/design-patterns/command , https://en.wikipedia.org/wiki/Piece_table ,
//! https://www.finseth.com/craft/ and https://docs.rs/im (persistent collections in Rust)
use std::collections::HashSet;
use std::mem::size_of;
use std::sync::Arc;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, pct: usize) -> bool {
        self.below(100) < pct
    }
}

// ---------------------------------------------------------------- operations, actions, shared helpers

/// all text is ASCII, so byte positions are character positions
#[derive(Clone, Debug, PartialEq)]
enum Op {
    Insert { pos: usize, text: String },
    Delete { pos: usize, len: usize },
}

#[derive(Clone, Debug)]
enum Action {
    Edit(Op),
    Undo,
    Redo,
    Probe, // undo immediately followed by redo: must be the identity
}

fn apply_op(text: &mut String, op: &Op) {
    match op {
        Op::Insert { pos, text: t } => text.insert_str(*pos, t),
        Op::Delete { pos, len } => text.replace_range(*pos..*pos + *len, ""),
    }
}

/// the op that cancels `op` when applied to the document AFTER it (needs the pre-edit text to restore deleted characters)
fn inverse(before: &str, op: &Op) -> Op {
    match op {
        Op::Insert { pos, text } => Op::Delete {
            pos: *pos,
            len: text.len(),
        },
        Op::Delete { pos, len } => Op::Insert {
            pos: *pos,
            text: before[*pos..*pos + *len].to_owned(),
        },
    }
}

/// Decides whether an edit continues the current typing burst. Every design shares this rule so they stay comparable.
#[derive(Default, Clone)]
struct Coalescer {
    open: Option<(usize, usize)>, // (position where the next typed char must land, burst length)
}

const BURST_LIMIT: usize = 16;

impl Coalescer {
    fn merges(&mut self, op: &Op) -> bool {
        match op {
            Op::Insert { pos, text } => {
                if let Some((end, total)) = self.open {
                    if *pos == end && total + text.len() <= BURST_LIMIT {
                        self.open = Some((end + text.len(), total + text.len()));
                        return true;
                    }
                }
                self.open = Some((pos + text.len(), text.len()));
                false
            }
            Op::Delete { .. } => {
                self.open = None;
                false
            }
        }
    }
    fn reset(&mut self) {
        self.open = None;
    }
}

trait Editor {
    fn name(&self) -> &'static str;
    fn edit(&mut self, op: &Op);
    fn undo(&mut self) -> bool;
    fn redo(&mut self) -> bool;
    fn text(&self) -> String;
    fn entries(&self) -> usize;
    fn history_bytes(&self) -> usize;
}

// ---------------------------------------------------------------- the MODEL (oracle)

/// every state ever reached on the current line of history, and where we are. Obviously correct, wasteful.
struct Model {
    states: Vec<String>,
    cur: usize,
    co: Coalescer,
}

impl Model {
    fn new(initial: &str) -> Model {
        Model {
            states: vec![initial.to_owned()],
            cur: 0,
            co: Coalescer::default(),
        }
    }
    fn text(&self) -> &str {
        &self.states[self.cur]
    }
    fn edit(&mut self, op: &Op) {
        let mut next = self.text().to_owned();
        apply_op(&mut next, op);
        if self.co.merges(op) {
            self.states[self.cur] = next; // joins the burst: the state before the burst stays at cur-1
        } else {
            self.states.truncate(self.cur + 1); // a new edit kills the redo line
            self.states.push(next);
            self.cur += 1;
        }
    }
    fn undo(&mut self) -> bool {
        self.co.reset();
        if self.cur == 0 {
            return false;
        }
        self.cur -= 1;
        true
    }
    fn redo(&mut self) -> bool {
        self.co.reset();
        if self.cur + 1 >= self.states.len() {
            return false;
        }
        self.cur += 1;
        true
    }
}

// ---------------------------------------------------------------- (a) command pattern with inverses

struct Cmd {
    op: Op,
    inv: Op,
}

struct CommandEditor {
    text: String,
    undo: Vec<Cmd>,
    redo: Vec<Cmd>,
    co: Coalescer,
}

impl CommandEditor {
    fn new(initial: &str) -> Self {
        CommandEditor {
            text: initial.into(),
            undo: vec![],
            redo: vec![],
            co: Coalescer::default(),
        }
    }
}

impl Editor for CommandEditor {
    fn name(&self) -> &'static str {
        "command"
    }
    fn edit(&mut self, op: &Op) {
        let inv = inverse(&self.text, op);
        if self.co.merges(op) {
            // extend the top command instead of pushing a new one: one undo now removes the whole burst
            let top = self
                .undo
                .last_mut()
                .expect("a burst continues an existing command");
            if let (
                Op::Insert { text: t, .. },
                Op::Delete { len, .. },
                Op::Insert { text: add, .. },
            ) = (&mut top.op, &mut top.inv, op)
            {
                t.push_str(add);
                *len += add.len();
            }
        } else {
            self.undo.push(Cmd {
                op: op.clone(),
                inv,
            });
        }
        self.redo.clear();
        apply_op(&mut self.text, op);
    }
    fn undo(&mut self) -> bool {
        self.co.reset();
        let Some(c) = self.undo.pop() else {
            return false;
        };
        apply_op(&mut self.text, &c.inv);
        self.redo.push(c);
        true
    }
    fn redo(&mut self) -> bool {
        self.co.reset();
        let Some(c) = self.redo.pop() else {
            return false;
        };
        apply_op(&mut self.text, &c.op);
        self.undo.push(c);
        true
    }
    fn text(&self) -> String {
        self.text.clone()
    }
    fn entries(&self) -> usize {
        self.undo.len() + self.redo.len()
    }
    fn history_bytes(&self) -> usize {
        let op_bytes = |o: &Op| {
            size_of::<Op>()
                + if let Op::Insert { text, .. } = o {
                    text.len()
                } else {
                    0
                }
        };
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|c| op_bytes(&c.op) + op_bytes(&c.inv))
            .sum()
    }
}

// ---------------------------------------------------------------- (b) memento: whole-document snapshots

struct MementoEditor {
    text: String,
    undo: Vec<String>,
    redo: Vec<String>,
    co: Coalescer,
}

impl Editor for MementoEditor {
    fn name(&self) -> &'static str {
        "memento"
    }
    fn edit(&mut self, op: &Op) {
        if !self.co.merges(op) {
            self.undo.push(self.text.clone()); // O(document) per history entry
        }
        self.redo.clear();
        apply_op(&mut self.text, op);
    }
    fn undo(&mut self) -> bool {
        self.co.reset();
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.text, prev));
        true
    }
    fn redo(&mut self) -> bool {
        self.co.reset();
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.text, next));
        true
    }
    fn text(&self) -> String {
        self.text.clone()
    }
    fn entries(&self) -> usize {
        self.undo.len() + self.redo.len()
    }
    fn history_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|s| size_of::<String>() + s.len())
            .sum()
    }
}

// ---------------------------------------------------------------- (c) persistent rope with structural sharing

enum Node {
    Leaf(String),
    Branch {
        left: Arc<Node>,
        right: Arc<Node>,
        len: usize,
        depth: u32,
    },
}

type Rope = Option<Arc<Node>>; // None = empty document

fn rlen(r: &Rope) -> usize {
    match r.as_deref() {
        None => 0,
        Some(Node::Leaf(s)) => s.len(),
        Some(Node::Branch { len, .. }) => *len,
    }
}

fn depth(n: &Node) -> u32 {
    match n {
        Node::Leaf(_) => 0,
        Node::Branch { depth, .. } => *depth,
    }
}

fn leaf(s: &str) -> Rope {
    if s.is_empty() {
        None
    } else {
        Some(Arc::new(Node::Leaf(s.to_owned())))
    }
}

fn join(a: Rope, b: Rope) -> Rope {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(l), Some(r)) => {
            let (len, d) = (
                rlen(&Some(l.clone())) + rlen(&Some(r.clone())),
                1 + depth(&l).max(depth(&r)),
            );
            Some(Arc::new(Node::Branch {
                left: l,
                right: r,
                len,
                depth: d,
            }))
        }
    }
}

/// split at byte `at`; untouched subtrees are reused by cloning their Arc (a pointer copy), only the spine is rebuilt
fn split(n: &Arc<Node>, at: usize) -> (Rope, Rope) {
    match &**n {
        Node::Leaf(s) => match at {
            0 => (None, Some(n.clone())),
            a if a >= s.len() => (Some(n.clone()), None),
            a => (leaf(&s[..a]), leaf(&s[a..])),
        },
        Node::Branch { left, right, .. } => {
            let ll = rlen(&Some(left.clone()));
            if at < ll {
                let (a, b) = split(left, at);
                (a, join(b, Some(right.clone())))
            } else if at == ll {
                (Some(left.clone()), Some(right.clone()))
            } else {
                let (a, b) = split(right, at - ll);
                (join(Some(left.clone()), a), b)
            }
        }
    }
}

fn leaves(n: &Arc<Node>, out: &mut Vec<Arc<Node>>) {
    match &**n {
        Node::Leaf(_) => out.push(n.clone()),
        Node::Branch { left, right, .. } => {
            leaves(left, out);
            leaves(right, out);
        }
    }
}

fn build(ls: &[Arc<Node>]) -> Rope {
    match ls.len() {
        0 => None,
        1 => Some(ls[0].clone()),
        n => join(build(&ls[..n / 2]), build(&ls[n / 2..])),
    }
}

/// repeated splits make the tree lopsided; rebuild the spine over the SAME leaves when it gets deep
fn rebalance(r: Rope) -> Rope {
    match &r {
        Some(n) if depth(n) > 40 => {
            let mut ls = vec![];
            leaves(n, &mut ls);
            build(&ls)
        }
        _ => r,
    }
}

fn rope_text(r: &Rope, out: &mut String) {
    match r.as_deref() {
        None => {}
        Some(Node::Leaf(s)) => out.push_str(s),
        Some(Node::Branch { left, right, .. }) => {
            rope_text(&Some(left.clone()), out);
            rope_text(&Some(right.clone()), out);
        }
    }
}

fn rope_apply(r: &Rope, op: &Op) -> Rope {
    let out = match op {
        Op::Insert { pos, text } => {
            let (a, b) = r.as_ref().map_or((None, None), |n| split(n, *pos));
            join(join(a, leaf(text)), b)
        }
        Op::Delete { pos, len } => {
            let (a, rest) = r.as_ref().map_or((None, None), |n| split(n, *pos));
            let (_, c) = rest.as_ref().map_or((None, None), |n| split(n, *len));
            join(a, c)
        }
    };
    rebalance(out)
}

/// distinct nodes reachable from the given roots: shared nodes count once, which is the whole point
fn unique_node_bytes(roots: &[&Rope]) -> usize {
    fn walk(n: &Arc<Node>, seen: &mut HashSet<*const Node>, bytes: &mut usize) {
        if !seen.insert(Arc::as_ptr(n)) {
            return; // already counted via another snapshot
        }
        *bytes += size_of::<Node>();
        match &**n {
            Node::Leaf(s) => *bytes += s.len(),
            Node::Branch { left, right, .. } => {
                walk(left, seen, bytes);
                walk(right, seen, bytes);
            }
        }
    }
    let (mut seen, mut bytes) = (HashSet::new(), 0);
    for r in roots.iter().copied().flatten() {
        walk(r, &mut seen, &mut bytes);
    }
    bytes
}

struct PersistentEditor {
    doc: Rope,
    undo: Vec<Rope>, // each snapshot is ONE Arc clone
    redo: Vec<Rope>,
    co: Coalescer,
}

impl PersistentEditor {
    fn new(initial: &str) -> Self {
        PersistentEditor {
            doc: leaf(initial),
            undo: vec![],
            redo: vec![],
            co: Coalescer::default(),
        }
    }
}

impl Editor for PersistentEditor {
    fn name(&self) -> &'static str {
        "persistent"
    }
    fn edit(&mut self, op: &Op) {
        if !self.co.merges(op) {
            self.undo.push(self.doc.clone()); // O(1)
        }
        self.redo.clear();
        self.doc = rope_apply(&self.doc, op);
    }
    fn undo(&mut self) -> bool {
        self.co.reset();
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.doc, prev));
        true
    }
    fn redo(&mut self) -> bool {
        self.co.reset();
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.doc, next));
        true
    }
    fn text(&self) -> String {
        let mut s = String::with_capacity(rlen(&self.doc));
        rope_text(&self.doc, &mut s);
        s
    }
    fn entries(&self) -> usize {
        self.undo.len() + self.redo.len()
    }
    fn history_bytes(&self) -> usize {
        let roots: Vec<&Rope> = self.undo.iter().chain(&self.redo).collect();
        unique_node_bytes(&roots) + roots.len() * size_of::<Rope>()
    }
}

// ---------------------------------------------------------------- (d) piece table

#[derive(Clone, Copy, Debug, PartialEq)]
struct Piece {
    add: bool, // which buffer
    start: usize,
    len: usize,
}

struct PieceEditor {
    orig: String, // immutable
    add: String,  // append-only: old piece lists stay valid forever
    pieces: Vec<Piece>,
    undo: Vec<Vec<Piece>>,
    redo: Vec<Vec<Piece>>,
    co: Coalescer,
}

impl PieceEditor {
    fn new(initial: &str) -> Self {
        let pieces = if initial.is_empty() {
            vec![]
        } else {
            vec![Piece {
                add: false,
                start: 0,
                len: initial.len(),
            }]
        };
        PieceEditor {
            orig: initial.into(),
            add: String::new(),
            pieces,
            undo: vec![],
            redo: vec![],
            co: Coalescer::default(),
        }
    }

    fn insert(&mut self, pos: usize, text: &str) {
        let start = self.add.len();
        self.add.push_str(text);
        let new = Piece {
            add: true,
            start,
            len: text.len(),
        };
        let mut out = Vec::with_capacity(self.pieces.len() + 2);
        let mut at = 0;
        let mut placed = false;
        for &p in &self.pieces {
            if !placed && pos < at + p.len {
                let cut = pos - at; // split this piece around the insertion point
                if cut > 0 {
                    out.push(Piece { len: cut, ..p });
                }
                out.push(new);
                out.push(Piece {
                    start: p.start + cut,
                    len: p.len - cut,
                    ..p
                });
                placed = true;
            } else {
                out.push(p);
            }
            at += p.len;
        }
        if !placed {
            out.push(new);
        }
        self.pieces = out;
    }

    fn delete(&mut self, pos: usize, len: usize) {
        let (end, mut at) = (pos + len, 0);
        let mut out = Vec::with_capacity(self.pieces.len() + 1);
        for &p in &self.pieces {
            let (a, b) = (at, at + p.len);
            at = b;
            if a < pos {
                out.push(Piece {
                    len: (pos.min(b)) - a,
                    ..p
                }); // left remainder
            }
            if b > end {
                let from = end.max(a);
                out.push(Piece {
                    start: p.start + (from - a),
                    len: b - from,
                    ..p
                }); // right remainder
            }
        }
        self.pieces = out;
    }
}

impl Editor for PieceEditor {
    fn name(&self) -> &'static str {
        "piece table"
    }
    fn edit(&mut self, op: &Op) {
        if !self.co.merges(op) {
            self.undo.push(self.pieces.clone()); // O(pieces), but never touches the text buffers
        }
        self.redo.clear();
        match op {
            Op::Insert { pos, text } => self.insert(*pos, text),
            Op::Delete { pos, len } => self.delete(*pos, *len),
        }
    }
    fn undo(&mut self) -> bool {
        self.co.reset();
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.pieces, prev));
        true
    }
    fn redo(&mut self) -> bool {
        self.co.reset();
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.pieces, next));
        true
    }
    fn text(&self) -> String {
        let mut s = String::new();
        for p in &self.pieces {
            let buf = if p.add { &self.add } else { &self.orig };
            s.push_str(&buf[p.start..p.start + p.len]);
        }
        s
    }
    fn entries(&self) -> usize {
        self.undo.len() + self.redo.len()
    }
    fn history_bytes(&self) -> usize {
        let lists: usize = self
            .undo
            .iter()
            .chain(&self.redo)
            .map(|l| size_of::<Vec<Piece>>() + l.len() * size_of::<Piece>())
            .sum();
        lists + self.add.len() // the append-only buffer is part of what history costs
    }
}

// ---------------------------------------------------------------- (e) undo tree

struct TNode {
    parent: usize,
    op: Op,
    inv: Op,
    children: Vec<usize>,
    redo: Option<usize>, // which child `redo` follows: the one most recently created or visited
}

struct TreeEditor {
    text: String,
    nodes: Vec<TNode>,
    cur: usize,
    co: Coalescer,
}

impl TreeEditor {
    fn new(initial: &str) -> Self {
        let root = TNode {
            parent: 0,
            op: Op::Delete { pos: 0, len: 0 },
            inv: Op::Delete { pos: 0, len: 0 },
            children: vec![],
            redo: None,
        };
        TreeEditor {
            text: initial.into(),
            nodes: vec![root],
            cur: 0,
            co: Coalescer::default(),
        }
    }

    fn step_up(&mut self) -> bool {
        if self.cur == 0 {
            return false;
        }
        let inv = self.nodes[self.cur].inv.clone();
        apply_op(&mut self.text, &inv);
        let p = self.nodes[self.cur].parent;
        self.nodes[p].redo = Some(self.cur);
        self.cur = p;
        true
    }

    fn step_down(&mut self, child: usize) {
        let op = self.nodes[child].op.clone();
        apply_op(&mut self.text, &op);
        self.nodes[self.cur].redo = Some(child);
        self.cur = child;
    }

    /// jump to ANY state: undo to the lowest common ancestor, then replay down the other branch
    fn goto(&mut self, target: usize) {
        self.co.reset();
        let mut path = vec![target];
        while *path.last().unwrap() != 0 {
            path.push(self.nodes[*path.last().unwrap()].parent);
        }
        while !path.contains(&self.cur) {
            self.step_up();
        }
        let lca = path.iter().position(|&n| n == self.cur).unwrap();
        for &n in path[..lca].iter().rev() {
            self.step_down(n);
        }
    }
}

impl Editor for TreeEditor {
    fn name(&self) -> &'static str {
        "undo tree"
    }
    fn edit(&mut self, op: &Op) {
        let inv = inverse(&self.text, op);
        if self.co.merges(op) {
            let top = &mut self.nodes[self.cur];
            if let (
                Op::Insert { text: t, .. },
                Op::Delete { len, .. },
                Op::Insert { text: add, .. },
            ) = (&mut top.op, &mut top.inv, op)
            {
                t.push_str(add);
                *len += add.len();
            }
        } else {
            // never truncate: the old redo line stays as a sibling branch
            let id = self.nodes.len();
            self.nodes.push(TNode {
                parent: self.cur,
                op: op.clone(),
                inv,
                children: vec![],
                redo: None,
            });
            self.nodes[self.cur].children.push(id);
            self.nodes[self.cur].redo = Some(id);
            self.cur = id;
        }
        apply_op(&mut self.text, op);
    }
    fn undo(&mut self) -> bool {
        self.co.reset();
        self.step_up()
    }
    fn redo(&mut self) -> bool {
        self.co.reset();
        match self.nodes[self.cur].redo {
            Some(c) => {
                self.step_down(c);
                true
            }
            None => false,
        }
    }
    fn text(&self) -> String {
        self.text.clone()
    }
    fn entries(&self) -> usize {
        self.nodes.len() - 1
    }
    fn history_bytes(&self) -> usize {
        let t = |o: &Op| {
            if let Op::Insert { text, .. } = o {
                text.len()
            } else {
                0
            }
        };
        self.nodes
            .iter()
            .map(|n| size_of::<TNode>() + t(&n.op) + t(&n.inv) + n.children.len() * 8)
            .sum()
    }
}

// ---------------------------------------------------------------- random action generation and the property tests

const INITIAL: &str = "the quick brown fox";

fn make(kind: usize, initial: &str) -> Box<dyn Editor> {
    match kind {
        0 => Box::new(CommandEditor::new(initial)),
        1 => Box::new(MementoEditor {
            text: initial.into(),
            undo: vec![],
            redo: vec![],
            co: Coalescer::default(),
        }),
        2 => Box::new(PersistentEditor::new(initial)),
        3 => Box::new(PieceEditor::new(initial)),
        _ => Box::new(TreeEditor::new(initial)),
    }
}
const KINDS: usize = 5;

fn random_edit(rng: &mut Rng, len: usize, typing_at: &mut usize) -> Op {
    if len > 0 && rng.chance(30) {
        let pos = rng.below(len);
        return Op::Delete {
            pos,
            len: 1 + rng.below(5.min(len - pos)),
        };
    }
    // half of the inserts continue where the last one ended, which produces realistic typing bursts
    let pos = if *typing_at <= len && rng.chance(60) {
        *typing_at
    } else {
        rng.below(len + 1)
    };
    let text: String = (0..1 + rng.below(3))
        .map(|_| (b'a' + rng.below(26) as u8) as char)
        .collect();
    *typing_at = pos + text.len();
    Op::Insert { pos, text }
}

/// generate actions by driving the model, remembering the expected text after each
fn gen_sequence(rng: &mut Rng, n: usize, initial: &str) -> (Vec<Action>, Vec<(String, bool)>) {
    let mut model = Model::new(initial);
    let (mut actions, mut expected) = (vec![], vec![]);
    let mut typing_at = initial.len();
    for _ in 0..n {
        let (a, ok) = match rng.below(100) {
            0..60 => {
                let op = random_edit(rng, model.text().len(), &mut typing_at);
                model.edit(&op);
                (Action::Edit(op), true)
            }
            60..80 => (Action::Undo, model.undo()),
            80..95 => (Action::Redo, model.redo()),
            _ => {
                let before = model.text().to_owned();
                let ok = model.undo();
                if ok {
                    model.redo();
                }
                assert_eq!(
                    model.text(),
                    before,
                    "model: undo then redo is the identity"
                );
                (Action::Probe, ok)
            }
        };
        actions.push(a);
        expected.push((model.text().to_owned(), ok));
    }
    (actions, expected)
}

fn run_actions(ed: &mut dyn Editor, actions: &[Action], expected: &[(String, bool)]) {
    for (i, (a, (want_text, want_ok))) in actions.iter().zip(expected).enumerate() {
        let ok = match a {
            Action::Edit(op) => {
                ed.edit(op);
                true
            }
            Action::Undo => ed.undo(),
            Action::Redo => ed.redo(),
            Action::Probe => {
                let before = ed.text();
                let ok = ed.undo();
                if ok {
                    assert!(ed.redo(), "{}: redo after undo must succeed", ed.name());
                }
                assert_eq!(
                    ed.text(),
                    before,
                    "{}: undo;redo must be the identity",
                    ed.name()
                );
                ok
            }
        };
        assert_eq!(
            (&ed.text(), ok),
            (want_text, *want_ok),
            "{} diverged from the model at step {i} ({a:?})",
            ed.name()
        );
    }
}

/// edits, then undo EVERYTHING: initial text. Then redo everything: the text we had before.
fn check_undo_all(ed: &mut dyn Editor, initial: &str) {
    while ed.redo() {} // start from the newest state on the line (a pending redo stack may exist)
    let final_text = ed.text();
    let mut undone = 0;
    while ed.undo() {
        undone += 1;
    }
    assert_eq!(
        ed.text(),
        initial,
        "{}: undoing everything must restore the initial text",
        ed.name()
    );
    let mut redone = 0;
    while ed.redo() {
        redone += 1;
    }
    assert_eq!(
        redone,
        undone,
        "{}: redo count mirrors undo count",
        ed.name()
    );
    assert_eq!(
        ed.text(),
        final_text,
        "{}: redoing everything must restore the final text",
        ed.name()
    );
}

fn property_tests() {
    let (mut total_actions, mut coalesced_runs) = (0usize, 0usize);
    for seed in 1..=20_000u64 {
        let mut rng = Rng::new(seed);
        let (actions, expected) = gen_sequence(&mut rng, 40, INITIAL);
        total_actions += actions.len();
        for kind in 0..KINDS {
            let mut ed = make(kind, INITIAL);
            run_actions(ed.as_mut(), &actions, &expected);
            check_undo_all(ed.as_mut(), INITIAL);
        }
        // coalescing really happened: fewer undo steps than edits
        let edits = actions
            .iter()
            .filter(|a| matches!(a, Action::Edit(_)))
            .count();
        let mut cmd = CommandEditor::new(INITIAL);
        run_actions(&mut cmd, &actions, &expected);
        coalesced_runs += (cmd.undo.len() + cmd.redo.len() < edits) as usize;
    }
    println!(
        "20000 sequences x 5 designs ({total_actions} actions per design): all equal to the model, undo/redo identity holds"
    );
    assert!(
        coalesced_runs > 5000,
        "typing bursts must coalesce in many sequences"
    );
}

/// the tree keeps EVERY state: record the text of each node when created, then visit all of them in random order
fn tree_never_loses_states() {
    let (mut branches, mut checked) = (0, 0);
    for seed in 1..=2000u64 {
        let mut rng = Rng::new(seed ^ 0x7777);
        let (actions, _) = gen_sequence(&mut rng, 60, INITIAL);
        let mut t = TreeEditor::new(INITIAL);
        let mut recorded: Vec<String> = vec![INITIAL.to_owned()];
        for a in &actions {
            match a {
                Action::Edit(op) => t.edit(op),
                Action::Undo => {
                    t.undo();
                }
                Action::Redo => {
                    t.redo();
                }
                Action::Probe => {
                    if t.undo() {
                        t.redo();
                    }
                }
            }
            recorded.resize(t.nodes.len(), String::new());
            if matches!(a, Action::Edit(_)) {
                recorded[t.cur] = t.text(); // a coalesced burst keeps extending this node, so refresh it
            }
        }
        branches += t.nodes.iter().filter(|n| n.children.len() > 1).count();
        // visit every node, in a shuffled order, from wherever we currently are
        let mut order: Vec<usize> = (0..t.nodes.len()).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i + 1));
        }
        for n in order {
            t.goto(n);
            assert_eq!(
                t.text(),
                recorded[n],
                "state of node {n} was lost (seed {seed})"
            );
            checked += 1;
        }
    }
    println!(
        "undo tree: {checked} states revisited across 2000 histories ({branches} branch points): none lost"
    );
    assert!(branches > 500, "undo-then-edit must create branches");
}

// ---------------------------------------------------------------- sharing demonstration and memory report

fn demo_sharing() {
    // a 2000-leaf document: chunks of 32 chars joined into a balanced rope
    let chunks: Vec<Arc<Node>> = (0..2000)
        .map(|i| Arc::new(Node::Leaf(format!("{:032}", i))))
        .collect();
    let doc = build(&chunks);
    let snapshot = doc.clone(); // the "undo entry": one pointer copy
    assert_eq!(Arc::strong_count(doc.as_ref().unwrap()), 2);

    let edited = rope_apply(
        &doc,
        &Op::Insert {
            pos: 31_990,
            text: "XYZ".into(),
        },
    );
    let mut before = vec![];
    leaves(doc.as_ref().unwrap(), &mut before);
    let mut after = vec![];
    leaves(edited.as_ref().unwrap(), &mut after);
    let shared = after
        .iter()
        .filter(|a| before.iter().any(|b| Arc::ptr_eq(a, b)))
        .count();
    println!(
        "one insert into a {}-leaf rope: {} of {} leaves are shared with the snapshot",
        before.len(),
        shared,
        after.len()
    );
    assert!(
        shared >= before.len() - 2,
        "only the leaf that was split may differ"
    );
    assert!(
        Arc::ptr_eq(&before[0], &after[0]) && Arc::ptr_eq(&before[1999], after.last().unwrap())
    );
    // an untouched leaf is now held by both versions (plus our two vectors, hence the offset)
    assert!(Arc::strong_count(&before[1000]) >= 2 + 2);
    let both = unique_node_bytes(&[&snapshot, &edited]);
    let one = unique_node_bytes(&[&snapshot]);
    println!(
        "memory of snapshot alone: {one} B; snapshot + edited version: {both} B (+{} B, {:.2}%)",
        both - one,
        (both - one) as f64 * 100.0 / one as f64
    );
    assert!(
        both - one < one / 20,
        "a second version must cost a small fraction of the first"
    );
}

fn memory_report() {
    // a 4000-char document and 3000 random actions: the same history, five very different bills
    let initial: String = (0..4000).map(|i| (b'a' + (i % 26) as u8) as char).collect();
    let mut rng = Rng::new(99);
    let (actions, expected) = gen_sequence(&mut rng, 3000, &initial);
    println!(
        "\nmemory per history entry ({} actions on a {}-char document):",
        actions.len(),
        initial.len()
    );
    println!(
        "{:<12} {:>8} {:>12} {:>14}",
        "design", "entries", "total bytes", "bytes/entry"
    );
    let mut per_entry = vec![];
    for kind in 0..KINDS {
        let mut ed = make(kind, &initial);
        run_actions(ed.as_mut(), &actions, &expected);
        let (n, bytes) = (ed.entries().max(1), ed.history_bytes());
        println!("{:<12} {:>8} {:>12} {:>14}", ed.name(), n, bytes, bytes / n);
        per_entry.push(bytes / n);
    }
    assert!(
        per_entry[1] > 4000,
        "memento pays the whole document per entry"
    );
    assert!(per_entry[0] < 200, "command entries are tiny");
    assert!(
        per_entry[2] * 3 < per_entry[1],
        "structural sharing beats whole-document copies"
    );
}

fn demo_tree_branching() {
    let mut t = TreeEditor::new("ab");
    t.edit(&Op::Insert {
        pos: 2,
        text: "c".into(),
    }); // abc
    t.undo(); // ab
    t.edit(&Op::Delete { pos: 0, len: 1 }); // b   <- a branch; a linear editor would have dropped "abc"
    assert_eq!(t.text(), "b");
    assert!(!t.redo(), "the new branch has nothing to redo");
    t.goto(1); // jump back to the "abc" state on the other branch
    assert_eq!(t.text(), "abc");
    t.goto(2);
    assert_eq!(t.text(), "b");
    println!(
        "undo tree: after undo + new edit both 'abc' and 'b' stay reachable (nodes: {})",
        t.nodes.len()
    );
}

fn main() {
    demo_sharing();
    demo_tree_branching();

    // coalescing, concretely: typing "hello" is ONE undo step in every design
    for kind in 0..KINDS {
        let mut ed = make(kind, "");
        for (i, c) in "hello".chars().enumerate() {
            ed.edit(&Op::Insert {
                pos: i,
                text: c.to_string(),
            });
        }
        assert_eq!(ed.text(), "hello");
        assert!(
            ed.undo() && ed.text().is_empty(),
            "{}: one undo removes the whole burst",
            ed.name()
        );
        assert!(!ed.undo());
        assert!(ed.redo() && ed.text() == "hello");
    }
    println!("typing 'hello' char by char is one undo step in all five designs");

    property_tests();
    tree_never_loses_states();
    memory_report();
    println!("OK");
}
