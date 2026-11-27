---
title: Latch crabbing and safe nodes
summary: How readers and writers walk a tree hand over hand, why a latch on a child lets you release the parent, what makes a node safe for an insert or a remove, and why top-down ordering cannot deadlock.
minutes: 12
---
A B+ tree used by many threads at once has to answer two questions: *how do two operations avoid corrupting a page*, and *how do they avoid blocking each other more than necessary*. The answer to the first is latches (reader-writer locks on pages). The answer to the second is **latch crabbing**: hold as few latches, for as short a time, as correctness allows. It is named after how a crab walks: one leg moves, the others stay planted.

## Readers: hand over hand

A reader needs a page only until it has found the next one. So: latch the child, **then** release the parent.

```text
latch(header, read)
latch(root, read);  release(header)
while node is internal:
    child = route(node, key)
    latch(child, read);  release(node)      ← the child first, then the parent
node is the leaf: read it, release it
```

Why the child before the parent? If you released the parent first, a writer could split or merge the child away before you latch it, and you would walk into a page that no longer holds your key's range. A reader never holds more than two latches, and never holds a parent while waiting more than one hop below.

## Writers: keep the path, until it is safe

A writer that changes a leaf may have to change the leaf's parent (a split adds a separator; a merge removes one), and that parent's parent, and so on. So a pessimistic writer takes **write** latches top-down and keeps them all. That is correct and slow: the root is exclusively latched by every writer. The improvement is to notice that a node is often **safe**: the operation cannot propagate above it.

| operation | a node is **safe** when | so, once you hold it, you may release |
|---|---|---|
| insert | adding one entry cannot make it split: leaf `size + 1 < max`, internal `size < max` | everything above it |
| remove | taking one entry cannot make it underflow: `size > min` | everything above it |
| remove at the root | a root leaf with `size > 1`; a root internal page with `size > 2` | the header |

```svg
caption: A writer inserting a key descends three levels. The root is not safe (full), so the writer holds it and the header. The middle page is safe (has room): once it is latched, the header and the root are released. The leaf is latched next; only the middle page and the leaf remain held, however tall the tree is. (Animated.)
<svg viewBox="0 0 760 270" role="img" aria-label="Latches held by a writer while descending: header and root are released once a safe middle page is latched">
<style>
@keyframes lc-h{0%,38%{opacity:1}48%,100%{opacity:0.15}}
@keyframes lc-1{0%,12%{opacity:0.15}20%,100%{opacity:1}}
@keyframes lc-2{0%,36%{opacity:0.15}44%,100%{opacity:1}}
@keyframes lc-3{0%,62%{opacity:0.15}70%,100%{opacity:1}}
.lc-h{opacity:.15;animation:lc-h 10s ease-in-out infinite}.lc-1,.lc-2,.lc-3{opacity:1}.lc-1{animation:lc-1 10s ease-in-out infinite}.lc-2{animation:lc-2 10s ease-in-out infinite}.lc-3{animation:lc-3 10s ease-in-out infinite}
</style>
<defs><marker id="lc-a" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto"><path d="M0 0 L9 4.5 L0 9 z" style="fill:var(--dim)"/></marker></defs>
<g class="lc-h"><rect class="never" x="40" y="20" width="140" height="40" rx="4"/><text class="mid dim" x="110" y="45">header (W)</text></g>
<g class="lc-1"><rect class="hot" x="40" y="86" width="140" height="40" rx="4"/><text class="mid fg" x="110" y="111">root, full (W)</text></g>
<g class="lc-2"><rect class="live" x="40" y="152" width="140" height="40" rx="4"/><text class="mid fg" x="110" y="177">middle, room (W)</text></g>
<g class="lc-3"><rect class="live" x="40" y="218" width="140" height="40" rx="4"/><text class="mid fg" x="110" y="243">leaf (W)</text></g>
<path class="ln" d="M110 60 V84" marker-end="url(#lc-a)"/><path class="ln" d="M110 126 V150" marker-end="url(#lc-a)"/><path class="ln" d="M110 192 V216" marker-end="url(#lc-a)"/>
<text class="dim sm" x="220" y="45">held until the root is known to be safe</text>
<text class="t-a sm" x="220" y="111">not safe (full): a split could reach it</text>
<text class="t-g sm" x="220" y="177">safe: it has room, so it absorbs a split → release header and root</text>
<text class="t-g sm" x="220" y="243">the leaf: held; at the end only these two latches remain</text>
</svg>
```

For an insert into a tree of 681-way pages almost every node is safe, so a writer holds the leaf and, momentarily, its parent. Writers to different leaves no longer wait for each other.

## Why it cannot deadlock

A deadlock needs a cycle of threads each holding a latch another wants. Crabbing prevents cycles by an **ordering rule**: latches are taken **top-down**, and among siblings **left to right**. A thread holding page P waits only for pages *below* P or to its *right*; it never waits for an ancestor or a left neighbour. A chain of "waits for" edges can only point down or right, so it can never come back to its start.

> [!WARNING] The rules that make it true
> Never take a latch on a page above, or to the left of, one you hold. Never wait for a latch while holding something a thread you would be waiting for might need *above* you. An iterator that holds a leaf and then latches its left neighbour breaks the rule; that is why the iterator in this course holds nothing between steps.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `std::shared_mutex`, `lock_shared()` / `unlock_shared()` by hand | `RwLock`: `read()`/`write()` return guards that unlock when dropped |
| `std::shared_lock` / `unique_lock` scoped objects | the same guards; storing them in a `Vec` is storing locks |
| releasing "the parent" = calling `unlock` on a saved pointer | `drop(guard)`, `guard = child_guard`, or `vec.clear()` |
| lock hierarchy checked by convention or a tool (TSan's deadlock detector) | convention, plus tests with a watchdog |

## In real code

### Using it: the rules as functions, and hand-over-hand locking with real `RwLock`s

```rust test
#[derive(Clone, Copy)]
struct Page { is_leaf: bool, size: u32, max: u32, is_root: bool }

impl Page {
    fn min(&self) -> u32 { if self.is_leaf { self.max / 2 } else { self.max.div_ceil(2) } }
    fn safe_to_insert(&self) -> bool { if self.is_leaf { self.size + 1 < self.max } else { self.size < self.max } }
    fn safe_to_remove(&self) -> bool {
        match (self.is_root, self.is_leaf) {
            (true, true) => self.size > 1,
            (true, false) => self.size > 2,
            (false, _) => self.size > self.min(),
        }
    }
}

/// How many latches a writer holds once it reaches the leaf of a path, releasing everything above each safe page.
fn latches_held(path: &[Page], safe: impl Fn(&Page) -> bool) -> usize {
    let mut held = 0;           // the header counts as one more, released with the rest
    for page in path {
        if safe(page) { held = 0; }
        held += 1;
    }
    held
}

#[test]
fn the_safety_rules() {
    let leaf = |size| Page { is_leaf: true, size, max: 4, is_root: false };
    assert!(leaf(2).safe_to_insert());                    // 3 < 4: still room
    assert!(!leaf(3).safe_to_insert());                   // the insert would bring it to max: it splits
    let node = |size| Page { is_leaf: false, size, max: 4, is_root: false };
    assert!(node(3).safe_to_insert());
    assert!(!node(4).safe_to_insert(), "an internal page at max cannot take another child");
    assert!(leaf(3).safe_to_remove() && !leaf(2).safe_to_remove(), "a leaf at its minimum (2) would underflow");
    let root_leaf = |size| Page { is_leaf: true, size, max: 4, is_root: true };
    assert!(root_leaf(2).safe_to_remove() && !root_leaf(1).safe_to_remove(), "removing a root's last pair empties the tree");
    let root_node = |size| Page { is_leaf: false, size, max: 4, is_root: true };
    assert!(root_node(3).safe_to_remove() && !root_node(2).safe_to_remove(), "a root with 2 children would be left with 1");
}

#[test]
fn a_writer_holds_few_latches_however_tall_the_tree() {
    let page = |is_leaf, size| Page { is_leaf, size, max: 100, is_root: false };
    // every page has room: only the leaf (plus nothing above) is held
    let path: Vec<Page> = std::iter::repeat_n(page(false, 50), 5).chain([page(true, 50)]).collect();
    assert_eq!(latches_held(&path, Page::safe_to_insert), 1);
    // the leaf is full (it will split) but its parent has room: the parent absorbs the split, so only those two are held
    let mut path = path;
    path[5] = page(true, 99);
    assert_eq!(latches_held(&path, Page::safe_to_insert), 2);
    // a full page above safe ones does not matter: everything above the last safe page is released
    path[2] = page(false, 100);
    assert_eq!(latches_held(&path, Page::safe_to_insert), 2);
    // everything full all the way up: the writer must hold the whole path (and the header)
    let full: Vec<Page> = std::iter::repeat_n(page(false, 100), 5).chain([page(true, 99)]).collect();
    assert_eq!(latches_held(&full, Page::safe_to_insert), 6);
}
```

```rust test
use std::sync::mpsc;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

/// A fixed-shape tree: node 0 is the root (children 1 and 2); 1 and 2 are leaves. Nodes are in a Vec so guards borrow the Vec.
enum Kind { Internal { separator: i32, children: [usize; 2] }, Leaf(Vec<i32>) }

struct Tree { nodes: Vec<RwLock<Kind>> }

impl Tree {
    fn new() -> Tree {
        Tree { nodes: vec![
            RwLock::new(Kind::Internal { separator: 100, children: [1, 2] }),
            RwLock::new(Kind::Leaf(vec![])),
            RwLock::new(Kind::Leaf(vec![])),
        ] }
    }

    /// Hand-over-hand read: the child is latched before the parent guard is dropped by the assignment.
    fn contains(&self, key: i32) -> bool {
        let mut guard = self.nodes[0].read().unwrap();
        loop {
            let child = match &*guard {
                Kind::Leaf(keys) => return keys.contains(&key),
                Kind::Internal { separator, children } => children[(key >= *separator) as usize],
            };
            guard = self.nodes[child].read().unwrap();      // evaluated first: the child is latched, then the parent is dropped
        }
    }

    /// Optimistic insert: read-latch the root, write-latch only the leaf, release the root.
    fn insert(&self, key: i32) {
        let root = self.nodes[0].read().unwrap();
        let child = match &*root {
            Kind::Internal { separator, children } => children[(key >= *separator) as usize],
            Kind::Leaf(_) => unreachable!(),
        };
        let mut leaf = self.nodes[child].write().unwrap();  // the root's read latch keeps the leaf where it is
        drop(root);
        if let Kind::Leaf(keys) = &mut *leaf {
            if let Err(at) = keys.binary_search(&key) { keys.insert(at, key); }
        }
    }
}

#[test]
fn concurrent_readers_and_writers_on_different_leaves() {
    let tree = Arc::new(Tree::new());
    let handles: Vec<_> = (0..4).map(|t| {
        let tree = Arc::clone(&tree);
        thread::spawn(move || {
            for i in 0..200 {
                let key = if t % 2 == 0 { i * 4 + t } else { 100 + i * 4 + t };     // even threads: left leaf; odd threads: right leaf
                tree.insert(key);
                assert!(tree.contains(key));
            }
        })
    }).collect();
    for h in handles { h.join().unwrap(); }
    let total: usize = [1, 2].into_iter().map(|n| match &*tree.nodes[n].read().unwrap() { Kind::Leaf(k) => k.len(), _ => 0 }).sum();
    assert_eq!(total, 800, "no insert was lost");
}

#[test]
fn a_leaf_write_does_not_wait_for_a_reader_on_the_root_but_a_root_write_would() {
    let tree = Arc::new(Tree::new());
    let reader = tree.nodes[0].read().unwrap();             // someone is reading the root
    assert!(tree.nodes[0].try_write().is_err(), "a pessimistic writer needs the root exclusively: it would have to wait");
    let (tx, rx) = mpsc::channel();
    let t2 = Arc::clone(&tree);
    let handle = thread::spawn(move || { t2.insert(5); tx.send(()).unwrap(); });
    let finished = rx.recv_timeout(Duration::from_secs(5)).is_ok();
    drop(reader);
    handle.join().unwrap();
    assert!(finished, "the optimistic writer only needs the root shared, so it finished while the reader held it");
}
```

### In the exercises

- **2c-02:** the read descent takes the child's latch before dropping the parent's, and the write descent keeps every guard on a stack.
- **2e-01:** "safe for an insert" and "safe for a remove" decide where the header and the ancestors are released; the stage's tests hold leaves from the test thread and check that a waiting writer has let go of the header.
- **2e-04 (boss):** the concurrent tests are the first concurrency test above, with a deadlock watchdog.

### Where it is used

- **PostgreSQL's nbtree** uses Lehman and Yao's B-link tree: each page has a right-link, so a reader that lands in a page that has just split can follow it, and no parent latch is needed while descending; BusTub's course uses the textbook crabbing variant.
- **InnoDB** takes index-tree latches in modes (S, SX, X) and releases ancestors on "optimistic" descents that assume no structure change, the same two ideas.
- **Concurrent maps**: `dashmap`'s shard locks, `BTreeMap` under an `RwLock` (coarse), and lock-free skip lists (Java's `ConcurrentSkipListMap`) are points on the same spectrum.
- **Linked structures with fine-grained locks**: hand-over-hand ("lock coupling") traversal of a list or a tree is the standard way to keep each lock short.
