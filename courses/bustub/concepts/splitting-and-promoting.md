---
title: Splitting and promoting: how a B+ tree grows
summary: The two kinds of split (copy up from a leaf, move up from an internal page), the cascade to the root, the exact arithmetic, and a complete in-memory insert you can run.
minutes: 12
---
A B+ tree never grows downwards. New keys go into leaves; when a leaf has no room it **splits**, and the news travels *up*: the parent gets one more child, and if that overflows it splits too. Only when the news reaches the root does the tree grow a level, at the top. That is why every leaf is always at the same depth, and why no balancing pass is ever needed.

## Leaf split: copy up

A leaf that reaches `max` pairs after an insert is split. The lower `ceil(n/2)` pairs stay; the rest go to a new leaf on the right; the two are linked; and the new leaf's **first key is copied** into the parent as the separator. Copied, because the leaf must keep it: internal pages are signposts, the data lives in the leaves.

```svg
caption: Inserting 3 into the full leaf [1,2] (max_size 3): the leaf splits into [1,2] and [3]; the key 3 is copied up into the parent, and the new leaf is chained after the old one. (Animated.)
<svg viewBox="0 0 760 250" role="img" aria-label="A leaf with 1, 2 and 3 splitting into two leaves with the key 3 copied into a new parent">
<style>
@keyframes sp-in{0%,30%{opacity:0}45%,100%{opacity:1}}
@keyframes sp-out{0%,30%{opacity:1}45%,100%{opacity:0}}
.sp-in{opacity:1;animation:sp-in 8s ease-in-out infinite}.sp-out{opacity:0;animation:sp-out 8s ease-in-out infinite}
</style>
<g class="sp-out"><rect class="live" x="290" y="120" width="180" height="46" rx="4"/><text class="mid fg" x="380" y="148">1  2  3</text><text class="dim sm" x="380" y="190">reached max_size 3: split</text></g>
<g class="sp-in">
<rect class="blue" x="320" y="30" width="120" height="40" rx="4"/><text class="mid t-b" x="380" y="48">internal</text><text class="mid fg sm" x="380" y="64">key 3</text>
<path class="ln" d="M350 70 L260 118"/><path class="ln" d="M410 70 L500 118"/>
<rect class="live" x="190" y="120" width="140" height="46" rx="4"/><text class="mid fg" x="260" y="148">1  2</text>
<rect class="live" x="430" y="120" width="140" height="46" rx="4"/><text class="mid fg" x="500" y="148">3</text>
<path class="ln-g dash" d="M330 143 H428"/><text class="t-g sm" x="380" y="190">3 is copied up and also stays in the leaf</text>
</g>
</svg>
```

## Internal split: move up

When the parent has no room for the new child, it splits. Here the **middle key moves up**: it separates the two halves, so it is needed in neither. The left page keeps `ceil((max+1)/2)` children and the right page gets the rest; the key between them goes to the next level.

| | leaf split | internal split |
|---|---|---|
| trigger | an insert brings the leaf to `max` pairs | a child is added to a page already holding `max` children |
| left keeps | `ceil(n / 2)` pairs | `ceil((n + 1) / 2)` children |
| separator | the right leaf's first key, **copied** | the key between the halves, **moved** up |
| also | next-leaf pointers are relinked | nothing |

```svg
caption: A full internal page (3 children, max 3) receives a 4th child. The four children are cut 2 + 2; the key between them (4) moves up to the parent and appears in neither half.
<svg viewBox="0 0 760 230" role="img" aria-label="An internal page with four children being cut into two pages with the middle key moving up">
<rect class="blue" x="120" y="20" width="520" height="44" rx="4"/><text class="mid t-b" x="380" y="48">internal page, 3 children + 1 arriving: keys 2 | 4 | 6</text>
<g>
<rect class="box" x="60" y="130" width="250" height="44" rx="4"/><text class="mid fg" x="185" y="157">left: children c0 c1 · key 2</text>
<rect class="box" x="450" y="130" width="250" height="44" rx="4"/><text class="mid fg" x="575" y="157">right: children c2 c3 · key 6</text>
</g>
<path class="ln-g" d="M380 66 V104"/><rect class="hot" x="350" y="84" width="60" height="26" rx="4"/><text class="mid fg" x="380" y="102">4 ↑</text>
<path class="ln" d="M340 110 L210 128"/><path class="ln" d="M420 110 L550 128"/>
<text class="dim sm" x="380" y="206">the key 4 goes to the parent and is in neither new page</text>
</svg>
```

## The cascade

After a leaf split, the parent is asked to take `(separator, new child)`. If it has room, done. If not, it splits and asks *its* parent. The loop ends at a page with room, or at the root; if the **root** splits, a new root is made with the two halves as its only children, the header page records it, and the height is one more. A cascade of depth `k` happens roughly once in `fan-out^k` inserts.

> [!NOTE] Why the new root has two children
> A root is allowed as few as two children. Two is exactly what a root split produces, so a root never needs special treatment on the way up. It does need special treatment on the way down (merging), where it is exempt from the minimum.

## C++ comparison

| C / C++ | Rust |
|---|---|
| a recursive `InsertIntoParent(old, key, new)` | a `loop` over the stack of write guards, or a recursion that returns `Option<(key, new_node)>` |
| `std::vector::insert`, `std::copy`/`memcpy` for the upper half | `Vec::insert`, `Vec::split_off` (in memory); loops of `entry_at`/`set_entry_at` (in a page) |
| an oversize temporary array for `max + 1` entries | a `Vec`, sized exactly |

## In real code

### Using it: a complete in-memory B+ tree insert

The whole algorithm without pages, latches or a buffer pool: nodes are `enum`s, the recursion returns "I split; here is the separator and the new sibling". Read it beside the stages; the page version is this code with the nodes stored in bytes and the recursion replaced by a stack of guards.

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

#[test]
fn a_leaf_split_copies_the_key_up() {
    let mut t = Tree::new(3, 20);
    for k in [1, 2] { t.insert(k); }
    assert_eq!(t.shape(), "[1,2]");
    t.insert(3);
    assert_eq!(t.shape(), "{3 [1,2] [3]}");                  // 3 is in the parent AND in the right leaf
    let mut t = Tree::new(5, 20);
    for k in 1..=5 { t.insert(k); }
    assert_eq!(t.shape(), "{4 [1,2,3] [4,5]}", "the left half keeps the extra pair");
}

#[test]
fn an_internal_split_moves_the_middle_key_up() {
    let mut t = Tree::new(2, 3);
    for k in 1..=3 { t.insert(k); }
    assert_eq!(t.shape(), "{2,3 [1] [2] [3]}");                // the root has 3 children: full
    t.insert(4);
    assert_eq!(t.shape(), "{3 {2 [1] [2]} {4 [3] [4]}}");      // 3 moved up: it is in neither internal page
    t.insert(5);
    assert_eq!(t.shape(), "{3 {2 [1] [2]} {4,5 [3] [4] [5]}}");
    assert_eq!(t.height(), 3);
}

#[test]
fn nine_keys_make_a_known_four_level_tree() {
    let mut t = Tree::new(2, 3);
    for k in 1..=9 { assert!(t.insert(k)); }
    assert_eq!(t.shape(), "{5 {3 {2 [1] [2]} {4 [3] [4]}} {7 {6 [5] [6]} {8,9 [7] [8] [9]}}}");
    assert_eq!(t.height(), 4);
    assert!(!t.insert(5), "a duplicate is refused and changes nothing");
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

fn keys_in_order(n: &Node, out: &mut Vec<i64>) {
    match n {
        Node::Leaf(keys) => out.extend(keys),
        Node::Internal { children, .. } => children.iter().for_each(|c| keys_in_order(c, out)),
    }
}

/// Every leaf at the same depth, every page within its size bounds.
fn check(t: &Tree) {
    fn depth_of_leaves(t: &Tree, n: &Node, depth: usize, is_root: bool, leaf_depth: &mut Option<usize>) {
        match n {
            Node::Leaf(keys) => {
                assert!(!keys.is_empty() && keys.len() < t.leaf_max, "leaf size {}", keys.len());
                if !is_root { assert!(keys.len() >= t.leaf_max / 2, "leaf below min"); }
                assert!(leaf_depth.map_or(true, |d| d == depth), "leaves at different depths");
                *leaf_depth = Some(depth);
            }
            Node::Internal { keys, children } => {
                assert_eq!(keys.len() + 1, children.len());
                assert!(children.len() <= t.internal_max);
                assert!(children.len() >= if is_root { 2 } else { t.internal_max.div_ceil(2) }, "internal below min");
                for c in children { depth_of_leaves(t, c, depth + 1, false, leaf_depth); }
            }
        }
    }
    depth_of_leaves(t, t.root.as_ref().unwrap(), 1, true, &mut None);
}

#[test]
fn any_arrival_order_gives_a_valid_tree_of_the_same_keys() {
    for (leaf, internal) in [(2, 3), (3, 3), (3, 4), (4, 5), (5, 4), (6, 7)] {
        for order in 0..3 {
            let mut keys: Vec<i64> = (1..=200).collect();
            match order {
                1 => keys.reverse(),
                2 => { let mut s = 99u64; for i in (1..keys.len()).rev() { s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); keys.swap(i, (s >> 33) as usize % (i + 1)); } }
                _ => {}
            }
            let mut t = Tree::new(leaf, internal);
            for &k in &keys { assert!(t.insert(k)); }
            check(&t);
            let mut out = vec![];
            keys_in_order(t.root.as_ref().unwrap(), &mut out);
            assert_eq!(out, (1..=200).collect::<Vec<_>>(), "({leaf},{internal}) order {order}");
        }
    }
}
```

### In the exercises

- **2c-02:** the leaf split (the first key of the right page is copied up), the parent that gets the new child when it has room, and the new root when the root split.
- **2c-03:** the internal split (`keep`, `split_off`, the middle key moves up) and the climb to the next parent.
- **2d-03:** when a full leaf with tombstones may reclaim instead of splitting.
- **Structure checker:** `check` is the idea behind `check_structure` in the tests: same depth, sizes within bounds, keys in order.

### Where it is used

- **Every B+ tree index**: PostgreSQL's `_bt_split` and `_bt_insert_parent` are this algorithm on 8 KiB pages (it also picks the split point to keep the tuple sizes balanced, not just the counts).
- **SQLite's** `balance()` redistributes among siblings before splitting (it looks at neighbours first), a refinement of the same idea.
- **Rust's `BTreeMap`** splits full nodes on insert exactly this way (`split_off` of the upper half, the median moves up).
- **Bulk loading**: building an index from sorted data avoids splits altogether by filling leaves left to right and building the levels above from the leaf's first keys.
