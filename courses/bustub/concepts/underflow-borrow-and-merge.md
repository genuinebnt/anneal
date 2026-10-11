---
title: Underflow, borrow and merge: how a B+ tree shrinks
summary: What too empty means, why borrowing from a neighbour is tried before merging, how the separator key moves in each case, and the cascade that ends with a root collapse; a complete in-memory remove you can run.
minutes: 12
---
Removing a key from a leaf is easy. The trouble is what comes after: every page except the root must stay at least about half full, or the tree degrades into a tall sparse structure with the disk reads to match. A page that falls below its minimum has **underflowed**, and the tree repairs it locally, in the order of cheapest first.

## The three repairs

1. **Nothing**, if the page is still at least `min_size`. Most removals.
2. **Borrow** (also *redistribute*): a neighbour has an entry to spare, so move one across. The tree's shape does not change; only the two pages and one separator in the parent do.
3. **Merge** (also *coalesce*): no neighbour can spare one, so the two pages together fit in one page; combine them, drop the separator between them from the parent, and free the empty page. The parent now has one child fewer, which may make *it* underflow.

Prefer the **left** sibling, then the right, in a fixed order, so concurrent writers latch neighbours in the same order and cannot deadlock.

```svg
caption: Leaf level. Left: the leaf [4] is below min_size 2, its left sibling [1,2,3] has a spare, so 3 moves over and the separator becomes 3. Right: neither sibling has a spare, so [4] merges into [1,2] and the separator disappears.
<svg viewBox="0 0 760 270" role="img" aria-label="A borrow from the left sibling on the left and a merge on the right">
<text class="t-g" x="20" y="22">borrow</text>
<rect class="blue" x="130" y="30" width="110" height="30" rx="4"/><text class="mid t-b sm" x="185" y="50">key 4</text>
<rect class="live" x="40" y="100" width="130" height="40" rx="4"/><text class="mid fg" x="105" y="125">1  2  3</text>
<rect class="hot" x="200" y="100" width="90" height="40" rx="4"/><text class="mid fg" x="245" y="125">4</text>
<path class="ln-g" d="M150 140 q30 40 80 0" /><text class="t-g sm" x="190" y="176">3 moves over</text>
<rect class="blue" x="130" y="196" width="110" height="30" rx="4"/><text class="mid t-b sm" x="185" y="216">key 3</text>
<rect class="live" x="40" y="236" width="100" height="26" rx="4"/><text class="mid fg sm" x="90" y="254">1  2</text>
<rect class="live" x="170" y="236" width="100" height="26" rx="4"/><text class="mid fg sm" x="220" y="254">3  4</text>
<text class="t-w" x="400" y="22">merge</text>
<rect class="blue" x="500" y="30" width="110" height="30" rx="4"/><text class="mid t-b sm" x="555" y="50">key 3</text>
<rect class="live" x="420" y="100" width="100" height="40" rx="4"/><text class="mid fg" x="470" y="125">1  2</text>
<rect class="hot" x="570" y="100" width="90" height="40" rx="4"/><text class="mid fg" x="615" y="125">3</text>
<path class="ln-w" d="M575 140 q-50 40 -90 0" /><text class="t-w sm" x="540" y="176">merge, free the page</text>
<rect class="live" x="450" y="236" width="140" height="26" rx="4"/><text class="mid fg sm" x="520" y="254">1  2  3</text>
<text class="dim sm" x="600" y="254">key 3 leaves the parent</text>
</svg>
```

## How the separator moves

| case | leaf level | internal level |
|---|---|---|
| **borrow from the left** | the left leaf's last pair moves to the front; the parent key becomes **that key** | the left page's last child moves to the front; the **parent's key comes down** to sit before the old first child; the left page's last key goes **up** into the parent |
| **borrow from the right** | the right leaf's first pair moves to the end; the parent key becomes the right leaf's **new first key** | the right page's first child moves to the end under the **parent's key**; the right page's first real key goes **up** |
| **merge** | append all pairs of the right leaf; fix the next-leaf pointer; the parent's key is simply **dropped** | append the right page's children; the **parent's key comes down** as the key before its first child (it was the only thing separating the two ranges) |

At the internal level the keys travel through the parent like a rotation in a balanced binary tree; at the leaf level the data is in the leaves, so the parent's key just needs to stay a valid signpost.

## The sizes make it work

A short page has `min - 1` entries. If its sibling has more than `min` it can lend one; if the sibling has exactly `min`, then `(min - 1) + min` entries fit in one page: for a leaf `2 * floor(max/2) - 1 <= max - 1`, for an internal page `2 * ceil(max/2) - 1 <= max`. A merge therefore never overflows, and a borrow never leaves the donor short. (These inequalities are the reason `min_size` has two definitions.)

## The cascade, and the root

A merge removes an entry from the parent. If the parent falls below its minimum the same repair runs one level up, and so on. The **root** has no minimum, but:

- a root **leaf** that loses its last pair means the tree is empty (the header goes back to `INVALID`, the page is freed);
- a root **internal page** left with a single child is replaced by that child: the tree is one level shorter. This is the only way the height decreases, the mirror image of the root split.

> [!WARNING] Free pages last
> A page that was merged away or replaced is still pinned and latched by the operation that emptied it. `delete_page` fails on a pinned page, so collect the ids and delete them after the guards are gone.

## C++ comparison

| C / C++ | Rust |
|---|---|
| `CoalesceOrRedistribute(node)` recursing on the parent | a loop whose state is the short node's guard, or recursion that returns "I am short" |
| `std::vector::erase(pos)` / `memmove` | `Vec::remove`, `PageArray::remove_at` |
| `std::swap` of two siblings' halves | `split_at_mut` to hold `&mut` to two children at once |

> [!NOTE] Two `&mut` into one `Vec`
> Borrowing from a sibling needs mutable access to two elements of the same `Vec`. `children.split_at_mut(i)` returns two disjoint slices, which is the safe way to ask for it; indexing twice (`&mut children[i]` and `&mut children[i - 1]`) is rejected by the borrow checker, correctly.

## In real code

### Using it: a complete in-memory remove, with the tree from "Splitting and promoting"

Insert (copied from the previous article so this example is self-contained) and remove, with the borrow and merge cases as small functions. Every scenario below is the shape the stages' tests assert.

```rust test
#[derive(Debug, Clone)]
enum Node {
    Leaf(Vec<i64>),
    /// `keys.len() + 1 == children.len()`: `keys[i]` separates `children[i]` from `children[i + 1]`.
    Internal { keys: Vec<i64>, children: Vec<Node> },
}

struct Tree { leaf_max: usize, internal_max: usize, root: Option<Node> }

impl Tree {
    fn new(leaf_max: usize, internal_max: usize) -> Tree { Tree { leaf_max, internal_max, root: None } }

    /// `false` for a duplicate key.
    fn insert(&mut self, key: i64) -> bool {
        let Some(mut root) = self.root.take() else {
            self.root = Some(Node::Leaf(vec![key]));
            return true;
        };
        let (inserted, split) = self.insert_into(&mut root, key);
        self.root = Some(match split {
            None => root,
            // the root itself split: the tree grows by one level
            Some((separator, right)) => Node::Internal { keys: vec![separator], children: vec![root, right] },
        });
        inserted
    }

    /// Inserts below `node`. If `node` overflowed and split, returns the separator and the new right sibling for the parent to record.
    fn insert_into(&self, node: &mut Node, key: i64) -> (bool, Option<(i64, Node)>) {
        match node {
            Node::Leaf(keys) => {
                let Err(at) = keys.binary_search(&key) else { return (false, None) };
                keys.insert(at, key);
                if keys.len() < self.leaf_max {
                    return (true, None);
                }
                // a leaf reached max: the upper half moves to a new leaf, and the new leaf's first key is COPIED up
                let right = keys.split_off(keys.len().div_ceil(2));
                (true, Some((right[0], Node::Leaf(right))))
            }
            Node::Internal { keys, children } => {
                let i = keys.partition_point(|&k| k <= key);                 // the child whose range holds the key
                let (inserted, split) = self.insert_into(&mut children[i], key);
                let Some((separator, right)) = split else { return (inserted, None) };
                keys.insert(i, separator);                                   // record the child's split here
                children.insert(i + 1, right);
                if children.len() <= self.internal_max {
                    return (inserted, None);
                }
                // too many children: split this page too; the middle key MOVES up (it stays in neither half)
                let keep = children.len().div_ceil(2);
                let right_children = children.split_off(keep);
                let mut right_keys = keys.split_off(keep - 1);
                let up = right_keys.remove(0);
                (inserted, Some((up, Node::Internal { keys: right_keys, children: right_children })))
            }
        }
    }

    fn shape(&self) -> String {
        fn go(n: &Node) -> String {
            match n {
                Node::Leaf(keys) => format!("[{}]", keys.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(",")),
                Node::Internal { keys, children } => {
                    let keys = keys.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(",");
                    format!("{{{} {}}}", keys, children.iter().map(go).collect::<Vec<_>>().join(" "))
                }
            }
        }
        self.root.as_ref().map_or("empty".to_string(), go)
    }

    fn height(&self) -> usize {
        let mut n = self.root.as_ref();
        let mut h = 0;
        while let Some(node) = n {
            h += 1;
            n = match node { Node::Leaf(_) => None, Node::Internal { children, .. } => children.first() };
        }
        h
    }
}
impl Tree {
    fn min_of(&self, n: &Node) -> usize {
        match n { Node::Leaf(_) => self.leaf_max / 2, Node::Internal { .. } => self.internal_max.div_ceil(2) }
    }
    fn size_of(n: &Node) -> usize {
        match n { Node::Leaf(keys) => keys.len(), Node::Internal { children, .. } => children.len() }
    }

    fn remove(&mut self, key: i64) -> bool {
        let Some(mut root) = self.root.take() else { return false };
        let removed = self.remove_from(&mut root, key);
        // the root is exempt from the minimum, but must not be a lie about the height
        self.root = match root {
            Node::Leaf(keys) if keys.is_empty() => None,                              // the last pair is gone: an empty tree
            Node::Internal { mut children, .. } if children.len() == 1 => children.pop(), // a single child becomes the root
            other => Some(other),
        };
        removed
    }

    fn remove_from(&self, node: &mut Node, key: i64) -> bool {
        match node {
            Node::Leaf(keys) => match keys.binary_search(&key) {
                Ok(at) => { keys.remove(at); true }
                Err(_) => false,
            },
            Node::Internal { keys, children } => {
                let i = keys.partition_point(|&k| k <= key);
                let removed = self.remove_from(&mut children[i], key);
                if removed && Self::size_of(&children[i]) < self.min_of(&children[i]) {
                    self.fix_child(keys, children, i);
                }
                removed
            }
        }
    }

    /// `children[i]` is too empty: borrow from the left neighbour, else from the right one, else merge.
    fn fix_child(&self, keys: &mut Vec<i64>, children: &mut Vec<Node>, i: usize) {
        if i > 0 && Self::size_of(&children[i - 1]) > self.min_of(&children[i - 1]) {
            Self::borrow_from_left(keys, children, i);
        } else if i + 1 < children.len() && Self::size_of(&children[i + 1]) > self.min_of(&children[i + 1]) {
            Self::borrow_from_right(keys, children, i);
        } else if i > 0 {
            Self::merge(keys, children, i - 1);                                       // merge with the left sibling
        } else {
            Self::merge(keys, children, i);                                           // no left sibling: merge the right one into this
        }
    }

    fn borrow_from_left(keys: &mut [i64], children: &mut [Node], i: usize) {
        let (left, node) = children.split_at_mut(i);
        match (&mut left[i - 1], &mut node[0]) {
            (Node::Leaf(l), Node::Leaf(n)) => {
                let moved = l.pop().unwrap();
                n.insert(0, moved);
                keys[i - 1] = moved;                                                  // the separator is the right leaf's first key
            }
            (Node::Internal { keys: lk, children: lc }, Node::Internal { keys: nk, children: nc }) => {
                // a rotation through the parent: its key comes down, the donor's last key goes up
                nk.insert(0, keys[i - 1]);
                nc.insert(0, lc.pop().unwrap());
                keys[i - 1] = lk.pop().unwrap();
            }
            _ => unreachable!("siblings are at the same depth"),
        }
    }

    fn borrow_from_right(keys: &mut [i64], children: &mut [Node], i: usize) {
        let (node, right) = children.split_at_mut(i + 1);
        match (&mut node[i], &mut right[0]) {
            (Node::Leaf(n), Node::Leaf(r)) => {
                n.push(r.remove(0));
                keys[i] = r[0];                                                       // the right leaf's new first key
            }
            (Node::Internal { keys: nk, children: nc }, Node::Internal { keys: rk, children: rc }) => {
                nk.push(keys[i]);
                nc.push(rc.remove(0));
                keys[i] = rk.remove(0);
            }
            _ => unreachable!("siblings are at the same depth"),
        }
    }

    /// Merges `children[idx + 1]` into `children[idx]` and drops the separator between them.
    fn merge(keys: &mut Vec<i64>, children: &mut Vec<Node>, idx: usize) {
        let right = children.remove(idx + 1);
        let separator = keys.remove(idx);
        match (&mut children[idx], right) {
            (Node::Leaf(l), Node::Leaf(r)) => l.extend(r),                            // the separator is just dropped
            (Node::Internal { keys: lk, children: lc }, Node::Internal { keys: rk, children: rc }) => {
                lk.push(separator);                                                   // it comes DOWN between the two halves
                lk.extend(rk);
                lc.extend(rc);
            }
            _ => unreachable!("siblings are at the same depth"),
        }
    }
}

fn tree_of(leaf: usize, internal: usize, keys: impl IntoIterator<Item = i64>) -> Tree {
    let mut t = Tree::new(leaf, internal);
    for k in keys { t.insert(k); }
    t
}

#[test]
fn borrow_from_the_left_then_from_the_right_at_the_leaf_level() {
    let mut t = tree_of(5, 10, 1..=7);
    for k in [6, 7] { t.remove(k); }
    assert_eq!(t.shape(), "{4 [1,2,3] [4,5]}");
    t.remove(5);                                                  // [4] is short; the left leaf has a spare: 3 moves over
    assert_eq!(t.shape(), "{3 [1,2] [3,4]}");

    let mut t = tree_of(5, 10, 1..=6);
    for k in [1, 2] { t.remove(k); }                              // [3] is short with no left sibling; the right one has a spare
    assert_eq!(t.shape(), "{5 [3,4] [5,6]}");
}

#[test]
fn merge_and_the_root_collapse() {
    let mut t = tree_of(4, 10, 1..=4);
    assert_eq!(t.shape(), "{3 [1,2] [3,4]}");
    t.remove(4);                                                  // [3] merges into [1,2]; the root has one child left: it is replaced
    assert_eq!(t.shape(), "[1,2,3]");
    let mut t = tree_of(4, 10, 1..=4);
    t.remove(1);                                                  // no left sibling: the right leaf merges into this one
    assert_eq!(t.shape(), "[2,3,4]");
}

#[test]
fn an_internal_page_borrows_or_merges_and_the_separator_comes_down() {
    let mut t = tree_of(3, 3, 1..=12);
    assert_eq!(t.shape(), "{5,9 {3 [1,2] [3,4]} {7 [5,6] [7,8]} {11 [9,10] [11,12]}}");
    t.remove(1);
    t.remove(2);
    assert_eq!(t.shape(), "{5,9 {4 [3] [4]} {7 [5,6] [7,8]} {11 [9,10] [11,12]}}");
    t.remove(3);                                                  // [3] vanishes into [4]; the page {4 [3] [4]} has 1 child and merges with its sibling:
    assert_eq!(t.shape(), "{9 {5,7 [4] [5,6] [7,8]} {11 [9,10] [11,12]}}");   // 5 came DOWN between [4] and [5,6]
    t.remove(4);
    assert_eq!(t.shape(), "{9 {6,7 [5] [6] [7,8]} {11 [9,10] [11,12]}}");
}

#[test]
fn removing_everything_shrinks_the_tree_to_nothing() {
    let mut t = tree_of(2, 3, 1..=9);
    let mut heights = vec![t.height()];
    for k in (1..=9).rev() {
        assert!(t.remove(k));
        heights.push(t.height());
    }
    assert_eq!(t.shape(), "empty");
    assert!(heights.windows(2).all(|w| w[1] <= w[0] && w[0] - w[1] <= 1), "the height only ever drops one level at a time: {heights:?}");
    assert_eq!(heights, vec![4, 4, 3, 3, 3, 3, 2, 2, 1, 0]);
    assert!(!t.remove(1), "removing from an empty tree is a no-op");
}
```

```rust test
#[derive(Debug, Clone)]
enum Node {
    Leaf(Vec<i64>),
    /// `keys.len() + 1 == children.len()`: `keys[i]` separates `children[i]` from `children[i + 1]`.
    Internal { keys: Vec<i64>, children: Vec<Node> },
}

struct Tree { leaf_max: usize, internal_max: usize, root: Option<Node> }

impl Tree {
    fn new(leaf_max: usize, internal_max: usize) -> Tree { Tree { leaf_max, internal_max, root: None } }

    /// `false` for a duplicate key.
    fn insert(&mut self, key: i64) -> bool {
        let Some(mut root) = self.root.take() else {
            self.root = Some(Node::Leaf(vec![key]));
            return true;
        };
        let (inserted, split) = self.insert_into(&mut root, key);
        self.root = Some(match split {
            None => root,
            // the root itself split: the tree grows by one level
            Some((separator, right)) => Node::Internal { keys: vec![separator], children: vec![root, right] },
        });
        inserted
    }

    /// Inserts below `node`. If `node` overflowed and split, returns the separator and the new right sibling for the parent to record.
    fn insert_into(&self, node: &mut Node, key: i64) -> (bool, Option<(i64, Node)>) {
        match node {
            Node::Leaf(keys) => {
                let Err(at) = keys.binary_search(&key) else { return (false, None) };
                keys.insert(at, key);
                if keys.len() < self.leaf_max {
                    return (true, None);
                }
                // a leaf reached max: the upper half moves to a new leaf, and the new leaf's first key is COPIED up
                let right = keys.split_off(keys.len().div_ceil(2));
                (true, Some((right[0], Node::Leaf(right))))
            }
            Node::Internal { keys, children } => {
                let i = keys.partition_point(|&k| k <= key);                 // the child whose range holds the key
                let (inserted, split) = self.insert_into(&mut children[i], key);
                let Some((separator, right)) = split else { return (inserted, None) };
                keys.insert(i, separator);                                   // record the child's split here
                children.insert(i + 1, right);
                if children.len() <= self.internal_max {
                    return (inserted, None);
                }
                // too many children: split this page too; the middle key MOVES up (it stays in neither half)
                let keep = children.len().div_ceil(2);
                let right_children = children.split_off(keep);
                let mut right_keys = keys.split_off(keep - 1);
                let up = right_keys.remove(0);
                (inserted, Some((up, Node::Internal { keys: right_keys, children: right_children })))
            }
        }
    }

    fn shape(&self) -> String {
        fn go(n: &Node) -> String {
            match n {
                Node::Leaf(keys) => format!("[{}]", keys.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(",")),
                Node::Internal { keys, children } => {
                    let keys = keys.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(",");
                    format!("{{{} {}}}", keys, children.iter().map(go).collect::<Vec<_>>().join(" "))
                }
            }
        }
        self.root.as_ref().map_or("empty".to_string(), go)
    }

    fn height(&self) -> usize {
        let mut n = self.root.as_ref();
        let mut h = 0;
        while let Some(node) = n {
            h += 1;
            n = match node { Node::Leaf(_) => None, Node::Internal { children, .. } => children.first() };
        }
        h
    }
}
impl Tree {
    fn min_of(&self, n: &Node) -> usize {
        match n { Node::Leaf(_) => self.leaf_max / 2, Node::Internal { .. } => self.internal_max.div_ceil(2) }
    }
    fn size_of(n: &Node) -> usize {
        match n { Node::Leaf(keys) => keys.len(), Node::Internal { children, .. } => children.len() }
    }

    fn remove(&mut self, key: i64) -> bool {
        let Some(mut root) = self.root.take() else { return false };
        let removed = self.remove_from(&mut root, key);
        // the root is exempt from the minimum, but must not be a lie about the height
        self.root = match root {
            Node::Leaf(keys) if keys.is_empty() => None,                              // the last pair is gone: an empty tree
            Node::Internal { mut children, .. } if children.len() == 1 => children.pop(), // a single child becomes the root
            other => Some(other),
        };
        removed
    }

    fn remove_from(&self, node: &mut Node, key: i64) -> bool {
        match node {
            Node::Leaf(keys) => match keys.binary_search(&key) {
                Ok(at) => { keys.remove(at); true }
                Err(_) => false,
            },
            Node::Internal { keys, children } => {
                let i = keys.partition_point(|&k| k <= key);
                let removed = self.remove_from(&mut children[i], key);
                if removed && Self::size_of(&children[i]) < self.min_of(&children[i]) {
                    self.fix_child(keys, children, i);
                }
                removed
            }
        }
    }

    /// `children[i]` is too empty: borrow from the left neighbour, else from the right one, else merge.
    fn fix_child(&self, keys: &mut Vec<i64>, children: &mut Vec<Node>, i: usize) {
        if i > 0 && Self::size_of(&children[i - 1]) > self.min_of(&children[i - 1]) {
            Self::borrow_from_left(keys, children, i);
        } else if i + 1 < children.len() && Self::size_of(&children[i + 1]) > self.min_of(&children[i + 1]) {
            Self::borrow_from_right(keys, children, i);
        } else if i > 0 {
            Self::merge(keys, children, i - 1);                                       // merge with the left sibling
        } else {
            Self::merge(keys, children, i);                                           // no left sibling: merge the right one into this
        }
    }

    fn borrow_from_left(keys: &mut [i64], children: &mut [Node], i: usize) {
        let (left, node) = children.split_at_mut(i);
        match (&mut left[i - 1], &mut node[0]) {
            (Node::Leaf(l), Node::Leaf(n)) => {
                let moved = l.pop().unwrap();
                n.insert(0, moved);
                keys[i - 1] = moved;                                                  // the separator is the right leaf's first key
            }
            (Node::Internal { keys: lk, children: lc }, Node::Internal { keys: nk, children: nc }) => {
                // a rotation through the parent: its key comes down, the donor's last key goes up
                nk.insert(0, keys[i - 1]);
                nc.insert(0, lc.pop().unwrap());
                keys[i - 1] = lk.pop().unwrap();
            }
            _ => unreachable!("siblings are at the same depth"),
        }
    }

    fn borrow_from_right(keys: &mut [i64], children: &mut [Node], i: usize) {
        let (node, right) = children.split_at_mut(i + 1);
        match (&mut node[i], &mut right[0]) {
            (Node::Leaf(n), Node::Leaf(r)) => {
                n.push(r.remove(0));
                keys[i] = r[0];                                                       // the right leaf's new first key
            }
            (Node::Internal { keys: nk, children: nc }, Node::Internal { keys: rk, children: rc }) => {
                nk.push(keys[i]);
                nc.push(rc.remove(0));
                keys[i] = rk.remove(0);
            }
            _ => unreachable!("siblings are at the same depth"),
        }
    }

    /// Merges `children[idx + 1]` into `children[idx]` and drops the separator between them.
    fn merge(keys: &mut Vec<i64>, children: &mut Vec<Node>, idx: usize) {
        let right = children.remove(idx + 1);
        let separator = keys.remove(idx);
        match (&mut children[idx], right) {
            (Node::Leaf(l), Node::Leaf(r)) => l.extend(r),                            // the separator is just dropped
            (Node::Internal { keys: lk, children: lc }, Node::Internal { keys: rk, children: rc }) => {
                lk.push(separator);                                                   // it comes DOWN between the two halves
                lk.extend(rk);
                lc.extend(rc);
            }
            _ => unreachable!("siblings are at the same depth"),
        }
    }
}
use std::collections::BTreeSet;

fn collect(n: &Node, out: &mut Vec<i64>) {
    match n { Node::Leaf(k) => out.extend(k), Node::Internal { children, .. } => children.iter().for_each(|c| collect(c, out)) }
}

fn check(t: &Tree) {
    fn go(t: &Tree, n: &Node, depth: usize, is_root: bool, leaf_depth: &mut Option<usize>) {
        match n {
            Node::Leaf(keys) => {
                assert!(keys.len() < t.leaf_max && !keys.is_empty());
                if !is_root { assert!(keys.len() >= t.leaf_max / 2, "leaf below min: {keys:?}"); }
                assert!(leaf_depth.map_or(true, |d| d == depth));
                *leaf_depth = Some(depth);
            }
            Node::Internal { keys, children } => {
                assert_eq!(keys.len() + 1, children.len());
                assert!(children.len() <= t.internal_max);
                assert!(children.len() >= if is_root { 2 } else { t.internal_max.div_ceil(2) }, "internal below min");
                children.iter().for_each(|c| go(t, c, depth + 1, false, leaf_depth));
            }
        }
    }
    if let Some(root) = &t.root { go(t, root, 1, true, &mut None); }
}

#[test]
fn random_inserts_and_removes_agree_with_a_btreeset_and_keep_every_rule() {
    for (leaf, internal) in [(2, 3), (3, 3), (3, 4), (4, 5), (5, 4), (6, 7)] {
        let mut t = Tree::new(leaf, internal);
        let mut model = BTreeSet::new();
        let mut s = 12345u64 + leaf as u64;
        for step in 0..1500 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let key = ((s >> 33) % 80) as i64;
            if (s >> 20) % 5 < 3 {
                assert_eq!(t.insert(key), model.insert(key), "({leaf},{internal}) step {step}: insert {key}");
            } else {
                assert_eq!(t.remove(key), model.remove(&key), "({leaf},{internal}) step {step}: remove {key}");
            }
            check(&t);
        }
        let mut keys = vec![];
        if let Some(r) = &t.root { collect(r, &mut keys); }
        assert_eq!(keys, model.iter().copied().collect::<Vec<_>>());
    }
}
```

### In the exercises

- **2c-04:** the leaf and internal arms of borrowing from either sibling, `merge` and the cascade up the tree (each level's repair), the root checks (an empty root leaf empties the tree; a root with one child is replaced by it).
- **Model test:** the last test is the shape of the stage's `random_inserts_and_removes_agree_with_a_btreemap`, with the structure checker after every step.

### Where it is used

- **Databases differ on purpose**: PostgreSQL's B-tree never merges underfull pages; it deletes a leaf only once it is *completely empty* and lets sparse pages be reused by later inserts (and `REINDEX` rebuilds). SQLite rebalances siblings eagerly. InnoDB merges a page with a neighbour when its fill drops below a configurable threshold (`MERGE_THRESHOLD`), a hysteresis that avoids merge/split ping-pong.
- **Rust's `BTreeMap`** fixes underflow after `remove` by stealing from a sibling or merging (`fix_node_and_affected_ancestors`), the same two moves in memory.
- **File systems** (Btrfs) keep their B-trees balanced the same way on delete.
