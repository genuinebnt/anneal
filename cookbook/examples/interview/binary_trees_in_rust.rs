//! RECIPE 611 (interview): binary-tree interview questions in Rust (Option<Box<Tree>>, Option<Rc<RefCell<Tree>>>, Weak parents, arenas).
//! LEVEL: intermediate
//! Use in: coding interviews (LeetCode trees), parsers/ASTs, any recursive data structure, ordered maps (BST) and tree serialisation.   Run: cargo run --release --example binary_trees_in_rust
//! COMMON USES: traversals, depth/diameter/balance, BST validation and updates, LCA, serialisation, ASTs and file-system trees.
#![allow(dead_code)]
//! Read: https://www.techinterview.org/post/3233460609/linked-list-interview-patterns/ , https://rust-unofficial.github.io/too-many-lists/ ,
//! https://interviewing.io/rust-interview-questions , https://www.techinterviewhandbook.org/grind75
//!
//! QUESTION BANK
//! Q1. Inorder, preorder and postorder traversal: recursive, iterative with an explicit stack, and Morris (O(1) space). [Easy] [asked: very common]
//! Q2. Level-order, zigzag level-order and right-side view of a binary tree. [Medium] [asked: very common]
//! Q3. Maximum depth and minimum depth of a binary tree. [Easy] [asked: very common]
//! Q4. Invert (mirror) a binary tree, recursively and iteratively. [Easy] [asked: very common]
//! Q5. Same tree, symmetric tree and subtree of another tree. [Easy] [asked: very common]
//! Q6. Is a binary tree height-balanced? Do it in one pass, O(n). [Easy] [asked: very common]
//! Q7. Diameter of a binary tree and the maximum path sum (any node to any node). [Hard] [asked: very common]
//! Q8. Path sum I (does a root-to-leaf path sum to target) and Path sum II (return all such paths). [Medium] [asked: common]
//! Q9. Validate a binary search tree (bounds, not parent-only checks; also via inorder). [Medium] [asked: very common]
//! Q10. Kth smallest element in a BST. [Medium] [asked: very common]
//! Q11. Lowest common ancestor of two nodes: general binary tree and BST. [Medium] [asked: very common]
//! Q12. Insert into and delete from a BST (three delete cases). [Medium] [asked: very common]
//! Q13. Serialize and deserialize a binary tree. [Hard] [asked: common]
//! Q14. Construct a binary tree from preorder and inorder traversals. [Medium] [asked: very common]
//! Q15. Convert a sorted array (or sorted list) to a height-balanced BST. [Easy] [asked: common]
//! Q16. Flatten a binary tree to a linked list in preorder, in place. [Medium] [asked: common]
//! Q17. Count the nodes of a complete binary tree in better than O(n) (O(log^2 n)). [Medium] [asked: common]
//! Q18. Convert a binary tree to a circular doubly linked list in inorder (reuse the pointers). [Medium] [asked: occasional]
//! Q19. Vertical order traversal of a binary tree. [Hard] [asked: common]
//! Q20. Recover a BST in which exactly two nodes were swapped. [Hard] [asked: occasional]
//! Q21. Rust: which borrow-checker patterns do recursive Option<Box<Tree>> functions use (as_ref, as_deref, map, &mut cursor)? [Medium] [asked: common]
//! Q22. Rust: LeetCode's Option<Rc<RefCell<TreeNode>>>: how do you traverse and mutate it, what is Rc::clone vs a deep copy, and why does a double borrow_mut panic? [Medium] [asked: very common]
//! Q23. Rust: how do you give tree nodes parent pointers without leaking (Rc + Weak)? [Hard] [asked: common]
//! Q24. Rust: a degenerate tree 100,000 levels deep: what overflows the stack and how do iterative versions fix it? [Medium] [asked: common]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::rc::{Rc, Weak};

// ---------------------------------------------------------------------------------------------------------------------
// Plumbing: seeded xorshift, the idiomatic Box tree, LeetCode-style level-order builder, arena view, random generators.
// ---------------------------------------------------------------------------------------------------------------------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn int(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1) as u64) as i32
    }
}

/// Idiomatic Rust tree: each node OWNS its children through `Option<Box<_>>`. No Rc, no RefCell, no lifetimes in the type.
struct Tree {
    val: i32,
    left: T,
    right: T,
}
type T = Option<Box<Tree>>;

/// Like `ListNode` in the linked-list recipe, the derived Drop would recurse once per level; a 100,000-deep tree overflows the stack.
/// This Drop flattens the work onto a heap `Vec` (children are `take()`n so each node drops with no children).
impl Drop for Tree {
    fn drop(&mut self) {
        let mut stack: Vec<Box<Tree>> = Vec::new();
        stack.extend(self.left.take());
        stack.extend(self.right.take());
        while let Some(mut n) = stack.pop() {
            stack.extend(n.left.take());
            stack.extend(n.right.take());
        }
    }
}
fn node(v: i32, l: T, r: T) -> T {
    Some(Box::new(Tree {
        val: v,
        left: l,
        right: r,
    }))
}
fn leaf(v: i32) -> T {
    node(v, None, None)
}

/// LeetCode notation: `[1,2,3,null,null,4]` is level order with `None` for missing children (children of missing nodes are not listed).
fn from_levels(v: &[Option<i32>]) -> T {
    if v.is_empty() || v[0].is_none() {
        return None;
    }
    // arena of (val, left, right) built by BFS, then converted bottom-up so each Box is created once with its children
    let mut ar: Vec<(i32, Option<usize>, Option<usize>)> = vec![(v[0].unwrap(), None, None)];
    let mut q = VecDeque::from([0usize]);
    let mut i = 1;
    while let Some(p) = q.pop_front() {
        for side in 0..2 {
            if i >= v.len() {
                break;
            }
            if let Some(x) = v[i] {
                ar.push((x, None, None));
                let id = ar.len() - 1;
                if side == 0 {
                    ar[p].1 = Some(id)
                } else {
                    ar[p].2 = Some(id)
                }
                q.push_back(id);
            }
            i += 1;
        }
    }
    let mut slots: Vec<T> = (0..ar.len()).map(|_| None).collect();
    for id in (0..ar.len()).rev() {
        let (val, l, r) = ar[id];
        let left = l.and_then(|c| slots[c].take());
        let right = r.and_then(|c| slots[c].take());
        slots[id] = node(val, left, right);
    }
    slots[0].take()
}
fn lv(v: &[i32]) -> Vec<Option<i32>> {
    v.iter()
        .map(|&x| if x == i32::MIN { None } else { Some(x) })
        .collect()
}
const N: i32 = i32::MIN; // "null" marker for the shorthand below
fn t(v: &[i32]) -> T {
    from_levels(&lv(v))
}

fn levels(t: &T) -> Vec<Option<i32>> {
    let mut out = Vec::new();
    let mut q: VecDeque<Option<&Tree>> = VecDeque::new();
    q.push_back(t.as_deref());
    while let Some(x) = q.pop_front() {
        match x {
            None => out.push(None),
            Some(n) => {
                out.push(Some(n.val));
                q.push_back(n.left.as_deref());
                q.push_back(n.right.as_deref());
            }
        }
    }
    while out.last() == Some(&None) {
        out.pop();
    }
    out
}
fn clone_tree(t: &T) -> T {
    t.as_ref().map(|n| {
        Box::new(Tree {
            val: n.val,
            left: clone_tree(&n.left),
            right: clone_tree(&n.right),
        })
    })
}
fn size(t: &T) -> usize {
    t.as_ref().map_or(0, |n| 1 + size(&n.left) + size(&n.right))
}
fn height(t: &T) -> usize {
    t.as_ref()
        .map_or(0, |n| 1 + height(&n.left).max(height(&n.right)))
}
// plain recursive traversals: the ORACLES for everything traversal-shaped
fn pre_rec(t: &T, o: &mut Vec<i32>) {
    if let Some(n) = t {
        o.push(n.val);
        pre_rec(&n.left, o);
        pre_rec(&n.right, o);
    }
}
fn in_rec(t: &T, o: &mut Vec<i32>) {
    if let Some(n) = t {
        in_rec(&n.left, o);
        o.push(n.val);
        in_rec(&n.right, o);
    }
}
fn post_rec(t: &T, o: &mut Vec<i32>) {
    if let Some(n) = t {
        post_rec(&n.left, o);
        post_rec(&n.right, o);
        o.push(n.val);
    }
}
fn pre(t: &T) -> Vec<i32> {
    let mut o = vec![];
    pre_rec(t, &mut o);
    o
}
fn ino(t: &T) -> Vec<i32> {
    let mut o = vec![];
    in_rec(t, &mut o);
    o
}
fn post(t: &T) -> Vec<i32> {
    let mut o = vec![];
    post_rec(t, &mut o);
    o
}

/// Random shape with exactly `n` nodes. `bal` biases the split towards the middle (near-balanced trees); otherwise any split (often skewed).
fn rand_tree(r: &mut Rng, n: usize, lo: i32, hi: i32, bal: bool) -> T {
    if n == 0 {
        return None;
    }
    let l = if bal {
        ((n - 1) / 2 + r.range(2)).min(n - 1)
    } else {
        r.range(n)
    };
    let val = r.int(lo, hi);
    let left = rand_tree(r, l, lo, hi, bal);
    let right = rand_tree(r, n - 1 - l, lo, hi, bal);
    node(val, left, right)
}
fn relabel(t: &mut T, it: &mut std::vec::IntoIter<i32>) {
    if let Some(n) = t {
        n.val = it.next().unwrap();
        relabel(&mut n.left, it);
        relabel(&mut n.right, it);
    }
}
/// Random tree whose values are all DIFFERENT (needed for LCA / traversal reconstruction), shuffled, negatives included.
fn unique_tree(r: &mut Rng, n: usize) -> T {
    let mut t = rt(r, n, 0, 0);
    let mut vals: Vec<i32> = (0..n as i32).map(|x| x * 3 - 10).collect();
    for i in (1..vals.len()).rev() {
        vals.swap(i, r.range(i + 1));
    }
    relabel(&mut t, &mut vals.into_iter());
    t
}
fn bst_from(vals: &[i32]) -> T {
    let mut t = None;
    for &v in vals {
        bst_insert(&mut t, v);
    }
    t
}
fn rand_bst(r: &mut Rng, max_n: usize) -> T {
    let n = r.range(max_n + 1);
    let vals: Vec<i32> = (0..n).map(|_| r.int(-20, 20)).collect();
    bst_from(&vals)
}
/// random tree with a randomly chosen shape bias
fn rt(r: &mut Rng, n: usize, lo: i32, hi: i32) -> T {
    let b = r.range(2) == 0;
    rand_tree(r, n, lo, hi, b)
}
fn set_nth(t: &mut T, k: &mut usize, v: i32) -> bool {
    match t {
        None => false,
        Some(n) => {
            if *k == 0 {
                n.val = v;
                return true;
            }
            *k -= 1;
            set_nth(&mut n.left, k, v) || set_nth(&mut n.right, k, v)
        }
    }
}

/// Index view of a tree (preorder ids, parent and depth arrays): used for Morris (needs temporary cycles, impossible with Box), the
/// all-pairs oracles, and the doubly linked list question, where "pointers" are just `usize`.
struct Arena {
    val: Vec<i32>,
    left: Vec<Option<usize>>,
    right: Vec<Option<usize>>,
    parent: Vec<Option<usize>>,
    depth: Vec<usize>,
    root: Option<usize>,
}
fn to_arena(t: &T) -> Arena {
    fn go(t: &T, p: Option<usize>, d: usize, a: &mut Arena) -> Option<usize> {
        let n = t.as_ref()?;
        let id = a.val.len();
        a.val.push(n.val);
        a.left.push(None);
        a.right.push(None);
        a.parent.push(p);
        a.depth.push(d);
        let l = go(&n.left, Some(id), d + 1, a);
        let r = go(&n.right, Some(id), d + 1, a);
        a.left[id] = l;
        a.right[id] = r;
        Some(id)
    }
    let mut a = Arena {
        val: vec![],
        left: vec![],
        right: vec![],
        parent: vec![],
        depth: vec![],
        root: None,
    };
    a.root = go(t, None, 0, &mut a);
    a
}
fn path_nodes(a: &Arena, mut u: usize, mut v: usize) -> Vec<usize> {
    let (mut lp, mut rp) = (vec![], vec![]);
    while a.depth[u] > a.depth[v] {
        lp.push(u);
        u = a.parent[u].unwrap();
    }
    while a.depth[v] > a.depth[u] {
        rp.push(v);
        v = a.parent[v].unwrap();
    }
    while u != v {
        lp.push(u);
        rp.push(v);
        u = a.parent[u].unwrap();
        v = a.parent[v].unwrap();
    }
    lp.push(u);
    lp.extend(rp.into_iter().rev());
    lp
}
/// every root-to-leaf path, found with an explicit stack (oracle for depth and path-sum questions)
fn root_leaf_paths(t: &T) -> Vec<Vec<i32>> {
    let mut out = vec![];
    let mut st: Vec<(&Tree, Vec<i32>)> = vec![];
    if let Some(n) = t.as_deref() {
        st.push((n, vec![n.val]));
    }
    while let Some((n, p)) = st.pop() {
        if n.left.is_none() && n.right.is_none() {
            out.push(p.clone());
        }
        for c in [n.left.as_deref(), n.right.as_deref()]
            .into_iter()
            .flatten()
        {
            let mut q = p.clone();
            q.push(c.val);
            st.push((c, q));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------------
// Q1. Traversals. Inorder/preorder/postorder of [1,null,2,3] are [1,3,2] / [1,2,3] / [3,2,1] (the left child of 2 is 3).
// Approaches: recursive (O(n) time, O(h) stack); iterative with an explicit stack (same bounds, but the stack lives on the heap so it
// survives depth 100k, see Q24); Morris threading (O(n) time, O(1) extra space): temporarily point each predecessor's `right` at the node.
// Rust wrinkles: (1) the iterative stack holds `&Tree` borrowed with `as_deref()`, no ownership change. (2) Morris writes a pointer back to
// an ANCESTOR, which makes a cycle: with `Box` (unique ownership) that is not expressible, so we run it on the index arena (or Rc<RefCell>).
// (3) postorder iteratively = preorder variant (root, right, left) reversed: the easy trick; the "real" one-stack version needs a last-visited flag.
// Interviewer is testing: can you convert recursion to a stack, and do you know the inorder = sorted for a BST fact. Mistakes: forgetting to
// restore the threaded pointers; pushing the right child before the left in preorder. Follow-ups: successor/predecessor, iterator over a BST (Q10).
// ---------------------------------------------------------------------------------------------------------------------
fn inorder_iter(t: &T) -> Vec<i32> {
    let (mut out, mut st, mut cur) = (vec![], vec![], t.as_deref());
    while cur.is_some() || !st.is_empty() {
        while let Some(n) = cur {
            st.push(n);
            cur = n.left.as_deref();
        }
        let n = st.pop().unwrap();
        out.push(n.val);
        cur = n.right.as_deref();
    }
    out
}
fn preorder_iter(t: &T) -> Vec<i32> {
    let (mut out, mut st) = (vec![], vec![]);
    st.extend(t.as_deref());
    while let Some(n) = st.pop() {
        out.push(n.val);
        st.extend(n.right.as_deref()); // right first so left pops first
        st.extend(n.left.as_deref());
    }
    out
}
fn postorder_iter(t: &T) -> Vec<i32> {
    let (mut out, mut st) = (vec![], vec![]);
    st.extend(t.as_deref());
    while let Some(n) = st.pop() {
        out.push(n.val); // root, right, left ...
        st.extend(n.left.as_deref());
        st.extend(n.right.as_deref());
    }
    out.reverse(); // ... reversed = left, right, root
    out
}
fn morris(a: &Arena, want_inorder: bool) -> Vec<i32> {
    let (left, mut right) = (a.left.clone(), a.right.clone());
    let (mut out, mut cur) = (vec![], a.root);
    while let Some(c) = cur {
        match left[c] {
            None => {
                out.push(a.val[c]);
                cur = right[c];
            }
            Some(l) => {
                let mut p = l;
                while let Some(r) = right[p] {
                    if r == c {
                        break;
                    }
                    p = r;
                }
                if right[p] == Some(c) {
                    right[p] = None; // second visit: remove the thread, go right
                    if want_inorder {
                        out.push(a.val[c]);
                    }
                    cur = right[c];
                } else {
                    right[p] = Some(c); // first visit: thread predecessor -> c, descend left
                    if !want_inorder {
                        out.push(a.val[c]);
                    }
                    cur = Some(l);
                }
            }
        }
    }
    assert_eq!(right, a.right, "Morris must restore every pointer");
    out
}
fn q1() {
    let x = t(&[1, N, 2, 3]);
    assert_eq!(
        (ino(&x), pre(&x), post(&x)),
        (vec![1, 3, 2], vec![1, 2, 3], vec![3, 2, 1])
    );
    assert!(inorder_iter(&None).is_empty() && morris(&to_arena(&None), true).is_empty());
    let mut r = Rng(0x1001);
    for _ in 0..2000 {
        let n = r.range(15);
        let tr = rt(&mut r, n, -9, 9);
        let a = to_arena(&tr);
        assert_eq!(inorder_iter(&tr), ino(&tr));
        assert_eq!(preorder_iter(&tr), pre(&tr));
        assert_eq!(postorder_iter(&tr), post(&tr));
        assert_eq!(morris(&a, true), ino(&tr));
        assert_eq!(morris(&a, false), pre(&tr));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q2. Level order: [3,9,20,null,null,15,7] -> [[3],[9,20],[15,7]]. Zigzag: alternate direction per level -> [[3],[20,9],[15,7]].
// Right-side view: the last node of each level -> [3,20,7]. Approach: BFS with a `VecDeque<&Tree>`; process `q.len()` nodes per round to
// separate levels. O(n) time, O(width) space. Rust wrinkle: the queue stores borrowed `&Tree` (cheap, no clone, no Rc), `as_deref()`
// turns `Option<Box<Tree>>` into `Option<&Tree>`; `q.len()` is read once before the inner loop.
// Mistakes: reading `q.len()` inside the loop condition; reversing the wrong levels in zigzag (reverse levels 1,3,5...).
// Follow-ups: bottom-up level order; average per level; connect next pointers; DFS solution with a depth parameter (the oracle below).
// ---------------------------------------------------------------------------------------------------------------------
fn level_order(t: &T) -> Vec<Vec<i32>> {
    let mut out = vec![];
    let mut q: VecDeque<&Tree> = VecDeque::new();
    q.extend(t.as_deref());
    while !q.is_empty() {
        let mut lvl = vec![];
        for _ in 0..q.len() {
            let n = q.pop_front().unwrap();
            lvl.push(n.val);
            q.extend(n.left.as_deref());
            q.extend(n.right.as_deref());
        }
        out.push(lvl);
    }
    out
}
fn zigzag(t: &T) -> Vec<Vec<i32>> {
    let mut l = level_order(t);
    for (i, lvl) in l.iter_mut().enumerate() {
        if i % 2 == 1 {
            lvl.reverse();
        }
    }
    l
}
fn right_view(t: &T) -> Vec<i32> {
    level_order(t).iter().map(|l| *l.last().unwrap()).collect()
}
fn levels_dfs(t: &T, d: usize, out: &mut Vec<Vec<i32>>) {
    if let Some(n) = t {
        if out.len() == d {
            out.push(vec![]);
        }
        out[d].push(n.val);
        levels_dfs(&n.left, d + 1, out);
        levels_dfs(&n.right, d + 1, out);
    }
}
fn q2() {
    let x = t(&[3, 9, 20, N, N, 15, 7]);
    assert_eq!(level_order(&x), vec![vec![3], vec![9, 20], vec![15, 7]]);
    assert_eq!(zigzag(&x), vec![vec![3], vec![20, 9], vec![15, 7]]);
    assert_eq!(right_view(&x), vec![3, 20, 7]);
    assert!(level_order(&None).is_empty());
    let mut r = Rng(0x1002);
    for _ in 0..2000 {
        let n = r.range(16);
        let tr = rt(&mut r, n, -9, 9);
        let mut o = vec![];
        levels_dfs(&tr, 0, &mut o);
        assert_eq!(level_order(&tr), o);
        let mut z = o.clone();
        for i in (1..z.len()).step_by(2) {
            z[i].reverse();
        }
        assert_eq!(zigzag(&tr), z);
        assert_eq!(
            right_view(&tr),
            o.iter().map(|l| l[l.len() - 1]).collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q3. Maximum depth (nodes on the longest root-to-leaf path) and MINIMUM depth (nearest LEAF). [3,9,20,null,null,15,7] -> 3 and 2.
// Approach: recursion, O(n) time, O(h) space; min depth by BFS stops at the first leaf (O(n) worst, often much less).
// Classic trap: for min depth a node with ONE child is not a leaf, so `1 + min(left, right)` wrongly returns 1 for [1,2]; you must take the
// existing side. Rust wrinkle: `match (&n.left, &n.right)` on references handles the four shapes without moving anything.
// Follow-ups: iterative max depth (level_order().len()); depth of n-ary tree; depth without recursion (Q24).
// ---------------------------------------------------------------------------------------------------------------------
fn max_depth(t: &T) -> usize {
    t.as_ref()
        .map_or(0, |n| 1 + max_depth(&n.left).max(max_depth(&n.right)))
}
fn min_depth(t: &T) -> usize {
    match t {
        None => 0,
        Some(n) => match (&n.left, &n.right) {
            (None, None) => 1,
            (Some(_), None) => 1 + min_depth(&n.left),
            (None, Some(_)) => 1 + min_depth(&n.right),
            (Some(_), Some(_)) => 1 + min_depth(&n.left).min(min_depth(&n.right)),
        },
    }
}
fn min_depth_bfs(t: &T) -> usize {
    let mut q: VecDeque<(&Tree, usize)> = t.as_deref().map(|n| (n, 1)).into_iter().collect();
    while let Some((n, d)) = q.pop_front() {
        if n.left.is_none() && n.right.is_none() {
            return d;
        }
        q.extend(n.left.as_deref().map(|c| (c, d + 1)));
        q.extend(n.right.as_deref().map(|c| (c, d + 1)));
    }
    0
}
fn q3() {
    let x = t(&[3, 9, 20, N, N, 15, 7]);
    assert_eq!((max_depth(&x), min_depth(&x)), (3, 2));
    assert_eq!(min_depth(&t(&[1, 2])), 2, "one-child root is not a leaf");
    let mut r = Rng(0x1003);
    for _ in 0..2000 {
        let n = r.range(16);
        let tr = rt(&mut r, n, -9, 9);
        let paths = root_leaf_paths(&tr);
        assert_eq!(
            max_depth(&tr),
            paths.iter().map(|p| p.len()).max().unwrap_or(0)
        );
        assert_eq!(
            min_depth(&tr),
            paths.iter().map(|p| p.len()).min().unwrap_or(0)
        );
        assert_eq!(min_depth_bfs(&tr), min_depth(&tr));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q4. Invert a binary tree (swap left and right everywhere). [4,2,7,1,3,6,9] -> [4,7,2,9,6,3,1]. O(n) time, O(h) space.
// Rust wrinkle: `std::mem::swap(&mut n.left, &mut n.right)` swaps two fields of the same node without clones or `take()`.
// Recursion takes `&mut Option<Box<Tree>>`; the iterative form keeps `&mut Box<Tree>` on a stack (fine: each pointee is a distinct node, so
// the borrows never overlap). With `Rc<RefCell>` (Q22) the same swap needs `borrow_mut()`.
// Mistakes: swapping values instead of subtrees; swapping only at the root. Follow-up: check symmetry (Q5); mirror in place on an n-ary tree.
// ---------------------------------------------------------------------------------------------------------------------
fn invert(t: &mut T) {
    if let Some(n) = t {
        std::mem::swap(&mut n.left, &mut n.right);
        invert(&mut n.left);
        invert(&mut n.right);
    }
}
fn invert_iter(t: &mut T) {
    let mut st: Vec<&mut Box<Tree>> = vec![];
    st.extend(t.as_mut());
    while let Some(n) = st.pop() {
        std::mem::swap(&mut n.left, &mut n.right);
        st.extend(n.left.as_mut());
        st.extend(n.right.as_mut());
    }
}
fn mirror_pre(t: &T, o: &mut Vec<i32>) {
    if let Some(n) = t {
        o.push(n.val);
        mirror_pre(&n.right, o);
        mirror_pre(&n.left, o);
    }
}
fn q4() {
    let mut x = t(&[4, 2, 7, 1, 3, 6, 9]);
    invert(&mut x);
    assert_eq!(levels(&x), lv(&[4, 7, 2, 9, 6, 3, 1]));
    let mut e: T = None;
    invert(&mut e);
    invert_iter(&mut e);
    let mut r = Rng(0x1004);
    for _ in 0..2000 {
        let n = r.range(16);
        let tr = rt(&mut r, n, -9, 9);
        let (mut a, mut b) = (clone_tree(&tr), clone_tree(&tr));
        invert(&mut a);
        invert_iter(&mut b);
        let mut m = vec![];
        mirror_pre(&tr, &mut m);
        assert_eq!(pre(&a), m);
        assert_eq!(levels(&a), levels(&b));
        invert(&mut a);
        assert_eq!(levels(&a), levels(&tr), "inverting twice is the identity");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q5. (a) Are two trees identical (same shape AND values)? (b) Is a tree symmetric (a mirror of itself)? (c) Is `sub` a subtree of `root`
// (a node of root whose whole subtree equals sub)? Approaches: (a) simultaneous recursion; (b) `mirror(a, b)` recursion comparing a.left
// with b.right; (c) for each node of root run (a): O(n*m); or compare preorder serialisations with null markers (substring test; needs a
// delimiter before each token, otherwise "12" matches "2"). Rust wrinkle: `match (a, b)` on a pair of `&Option<Box<_>>` covers all four
// null/non-null cases in 4 lines. Mistakes: comparing only values (shape must match), forgetting the null markers in serialisation.
// Follow-ups: do (c) in O(n+m) with hashing (Merkle) or KMP on the serialisation.
// ---------------------------------------------------------------------------------------------------------------------
fn same_tree(a: &T, b: &T) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            x.val == y.val && same_tree(&x.left, &y.left) && same_tree(&x.right, &y.right)
        }
        _ => false,
    }
}
fn is_mirror(a: &T, b: &T) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            x.val == y.val && is_mirror(&x.left, &y.right) && is_mirror(&x.right, &y.left)
        }
        _ => false,
    }
}
fn is_symmetric(t: &T) -> bool {
    t.as_ref().is_none_or(|n| is_mirror(&n.left, &n.right))
}
fn is_subtree(root: &T, sub: &T) -> bool {
    same_tree(root, sub)
        || root
            .as_ref()
            .is_some_and(|n| is_subtree(&n.left, sub) || is_subtree(&n.right, sub))
}
fn ser(t: &T) -> String {
    match t {
        None => ",#".to_string(),
        Some(n) => format!(",{}{}{}", n.val, ser(&n.left), ser(&n.right)),
    }
}
fn q5() {
    assert!(same_tree(&t(&[1, 2, 3]), &t(&[1, 2, 3])) && !same_tree(&t(&[1, 2]), &t(&[1, N, 2])));
    assert!(is_symmetric(&t(&[1, 2, 2, 3, 4, 4, 3])) && !is_symmetric(&t(&[1, 2, 2, N, 3, N, 3])));
    assert!(
        is_subtree(&t(&[3, 4, 5, 1, 2]), &t(&[4, 1, 2]))
            && !is_subtree(&t(&[3, 4, 5, 1, 2, N, N, N, N, 0]), &t(&[4, 1, 2]))
    );
    let mut r = Rng(0x1005);
    let (mut sym_true, mut sub_true) = (0, 0);
    for _ in 0..2000 {
        let n = r.range(8);
        let a = rt(&mut r, n, 0, 2);
        let b = if r.range(2) == 0 {
            clone_tree(&a)
        } else {
            rand_tree(&mut r, n, 0, 2, true)
        };
        assert_eq!(same_tree(&a, &b), levels(&a) == levels(&b));
        // symmetric: half the time build one by mirroring
        let s = if r.range(2) == 0 {
            let half = rand_tree(&mut r, n, 0, 2, true);
            let mut m = clone_tree(&half);
            invert(&mut m);
            node(1, half, m)
        } else {
            a
        };
        let mut mir = clone_tree(&s);
        invert(&mut mir);
        let want = levels(&s) == levels(&mir);
        sym_true += want as usize;
        assert_eq!(is_symmetric(&s), want);
        // subtree: take a random subtree of s (or a random tree) and compare with the serialisation oracle
        let cand = if r.range(2) == 0 {
            b
        } else {
            {
                let k = r.range(4);
                rand_tree(&mut r, k, 0, 2, true)
            }
        };
        let want = ser(&s).contains(&ser(&cand));
        sub_true += want as usize;
        assert_eq!(is_subtree(&s, &cand), want);
    }
    assert!(
        sym_true > 100 && sub_true > 100,
        "both outcomes must be exercised"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q6. Is a binary tree height-balanced (every node's subtree heights differ by at most 1)? Do it in O(n), not O(n^2).
// Naive: at each node call `height` twice, recurse on children: O(n^2) on a skewed tree. Optimal: ONE post-order pass returning the height,
// or -1 as an "unbalanced" sentinel that short-circuits upward (so we also stop early). O(n) time, O(h) space.
// Rust wrinkle: encode the sentinel in `Option<usize>` (`None` = unbalanced) with `?`: no magic -1 needed and the early exit is automatic.
// Mistakes: checking only the root; checking balance only locally without recursing. Follow-ups: AVL rotations; build a balanced tree (Q15).
// ---------------------------------------------------------------------------------------------------------------------
fn balanced_height(t: &T) -> Option<usize> {
    match t {
        None => Some(0),
        Some(n) => {
            let (l, r) = (balanced_height(&n.left)?, balanced_height(&n.right)?);
            if l.abs_diff(r) > 1 {
                None
            } else {
                Some(1 + l.max(r))
            }
        }
    }
}
fn is_balanced(t: &T) -> bool {
    balanced_height(t).is_some()
}
fn is_balanced_naive(t: &T) -> bool {
    match t {
        None => true,
        Some(n) => {
            height(&n.left).abs_diff(height(&n.right)) <= 1
                && is_balanced_naive(&n.left)
                && is_balanced_naive(&n.right)
        }
    }
}
fn q6() {
    assert!(
        is_balanced(&t(&[3, 9, 20, N, N, 15, 7]))
            && !is_balanced(&t(&[1, 2, 2, 3, 3, N, N, 4, 4]))
            && is_balanced(&None)
    );
    let (mut yes, mut no) = (0, 0);
    let mut r = Rng(0x1006);
    for _ in 0..2000 {
        let n = r.range(20);
        let tr = {
            let b = r.range(3) != 0;
            rand_tree(&mut r, n, 0, 9, b)
        };
        let b = is_balanced(&tr);
        assert_eq!(b, is_balanced_naive(&tr));
        if b { yes += 1 } else { no += 1 }
    }
    assert!(yes > 100 && no > 100, "{yes} {no}");
}

// ---------------------------------------------------------------------------------------------------------------------
// Q7. (a) Diameter: the number of EDGES on the longest path between any two nodes (not necessarily through the root).
// (b) Maximum path sum: a path is any connected node sequence going up then down, no node twice; values may be negative; return the max sum.
// [-10,9,20,null,null,15,7] -> 42 (15+20+7). Approach (both): ONE post-order pass; each call returns the best DOWNWARD chain through the node
// and updates a global best with the best path that BENDS at the node (left + right + self). (a) returns height, best = l + r. (b) returns
// n.val + max(0, best child gain) so negative branches are dropped, best = n.val + max(0,l) + max(0,r). O(n) time, O(h) space.
// Rust wrinkle: the "global" is a `&mut` accumulator parameter (no `static`, no `Cell` needed). Mistakes: returning the bent path upward
// (a path can't fork), clamping with 0 on the wrong side, initialising best to 0 (all-negative trees must return the largest single node).
// Follow-ups: print the path; n-ary tree; tree with edge weights.
// ---------------------------------------------------------------------------------------------------------------------
fn diameter(t: &T) -> usize {
    fn go(t: &T, best: &mut usize) -> usize {
        match t {
            None => 0,
            Some(n) => {
                let (l, r) = (go(&n.left, best), go(&n.right, best));
                *best = (*best).max(l + r);
                1 + l.max(r)
            }
        }
    }
    let mut b = 0;
    go(t, &mut b);
    b
}
fn max_path_sum(t: &T) -> Option<i32> {
    fn go(t: &T, best: &mut i32) -> i32 {
        match t {
            None => 0,
            Some(n) => {
                let (l, r) = (go(&n.left, best).max(0), go(&n.right, best).max(0));
                *best = (*best).max(n.val + l + r);
                n.val + l.max(r)
            }
        }
    }
    let mut b = i32::MIN;
    go(t, &mut b);
    t.as_ref().map(|_| b)
}
fn q7() {
    assert_eq!(max_path_sum(&t(&[-10, 9, 20, N, N, 15, 7])), Some(42));
    assert_eq!(max_path_sum(&t(&[-3])), Some(-3));
    assert_eq!(max_path_sum(&t(&[-3, -1, -2])), Some(-1));
    assert_eq!(diameter(&t(&[1, 2, 3, 4, 5])), 3);
    assert_eq!(diameter(&None), 0);
    let mut r = Rng(0x1007);
    for _ in 0..2000 {
        let n = r.range(13);
        let tr = rt(&mut r, n, -9, 9);
        let a = to_arena(&tr);
        let (mut best_len, mut best_sum) = (0, None::<i32>);
        for u in 0..a.val.len() {
            for v in u..a.val.len() {
                let p = path_nodes(&a, u, v);
                best_len = best_len.max(p.len() - 1);
                let s: i32 = p.iter().map(|&i| a.val[i]).sum();
                best_sum = Some(best_sum.map_or(s, |b| b.max(s)));
            }
        }
        assert_eq!(diameter(&tr), best_len);
        assert_eq!(max_path_sum(&tr), best_sum);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q8. Path sum I: does a ROOT-TO-LEAF path sum to target? Path sum II: return all such paths. [5,4,8,11,null,13,4,7,2,null,null,5,1], 22 ->
// true; paths [[5,4,11,2],[5,8,4,5]]. Approach: DFS subtracting from the target; at a leaf test remaining == 0. II keeps one shared `Vec` as the
// current path with push/pop (backtracking) and clones only when a solution is found. O(n) time (O(n log n) with copies), O(h) space.
// Mistakes: treating a non-leaf with remaining == 0 as success (e.g. [1,2], target 1 is FALSE); `None` returning true for target 0.
// Rust wrinkle: the output `Vec<Vec<i32>>` and the scratch path are `&mut` parameters; no borrowed references escape.
// Follow-ups: Path Sum III (any downward path: prefix-sum HashMap); count paths; return only the number.
// ---------------------------------------------------------------------------------------------------------------------
fn has_path_sum(t: &T, target: i32) -> bool {
    match t {
        None => false,
        Some(n) => {
            let rem = target - n.val;
            if n.left.is_none() && n.right.is_none() {
                rem == 0
            } else {
                has_path_sum(&n.left, rem) || has_path_sum(&n.right, rem)
            }
        }
    }
}
fn path_sum_all(t: &T, target: i32) -> Vec<Vec<i32>> {
    fn go(t: &T, rem: i32, path: &mut Vec<i32>, out: &mut Vec<Vec<i32>>) {
        if let Some(n) = t {
            path.push(n.val);
            let rem = rem - n.val;
            if n.left.is_none() && n.right.is_none() {
                if rem == 0 {
                    out.push(path.clone());
                }
            } else {
                go(&n.left, rem, path, out);
                go(&n.right, rem, path, out);
            }
            path.pop();
        }
    }
    let mut out = vec![];
    go(t, target, &mut vec![], &mut out);
    out
}
fn q8() {
    let x = t(&[5, 4, 8, 11, N, 13, 4, 7, 2, N, N, 5, 1]);
    assert!(has_path_sum(&x, 22));
    assert_eq!(
        path_sum_all(&x, 22),
        vec![vec![5, 4, 11, 2], vec![5, 8, 4, 5]]
    );
    assert!(!has_path_sum(&t(&[1, 2]), 1) && !has_path_sum(&None, 0));
    let mut r = Rng(0x1008);
    for _ in 0..2000 {
        let n = r.range(14);
        let tr = rt(&mut r, n, -3, 5);
        let target = r.int(-4, 12);
        let mut o: Vec<Vec<i32>> = root_leaf_paths(&tr)
            .into_iter()
            .filter(|p| p.iter().sum::<i32>() == target)
            .collect();
        let mut got = path_sum_all(&tr, target);
        o.sort();
        got.sort();
        assert_eq!(got, o);
        assert_eq!(has_path_sum(&tr, target), !o.is_empty());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q9. Validate a BST: left subtree values < node < right subtree values for EVERY node (strict, no duplicates). [5,1,4,null,null,3,6] -> false.
// Wrong answer everyone gives first: check only `left.val < val < right.val` (parent-child). It accepts [5,4,6,null,null,3,7] where 3 sits in
// 5's right subtree. Correct: (1) pass an open interval (lo, hi) down: O(n) time, O(h) space; (2) inorder must be strictly increasing (keep `prev`).
// Rust wrinkle: bounds are `Option<i32>` (None = unbounded); using `i32::MIN/MAX` as sentinels breaks when a node holds exactly that value
// (tested below). Mistakes: `<=` vs `<` with duplicates; using i32 sentinels. Follow-ups: allow duplicates on one side; count BST subtrees.
// ---------------------------------------------------------------------------------------------------------------------
fn is_bst(t: &T) -> bool {
    fn go(t: &T, lo: Option<i32>, hi: Option<i32>) -> bool {
        match t {
            None => true,
            Some(n) => {
                lo.is_none_or(|l| n.val > l)
                    && hi.is_none_or(|h| n.val < h)
                    && go(&n.left, lo, Some(n.val))
                    && go(&n.right, Some(n.val), hi)
            }
        }
    }
    go(t, None, None)
}
fn is_bst_inorder(t: &T) -> bool {
    let (mut st, mut cur, mut prev) = (vec![], t.as_deref(), None::<i32>);
    while cur.is_some() || !st.is_empty() {
        while let Some(n) = cur {
            st.push(n);
            cur = n.left.as_deref();
        }
        let n = st.pop().unwrap();
        if prev.is_some_and(|p| p >= n.val) {
            return false;
        }
        prev = Some(n.val);
        cur = n.right.as_deref();
    }
    true
}
fn is_bst_parent_only(t: &T) -> bool {
    t.as_ref().is_none_or(|n| {
        n.left.as_ref().is_none_or(|l| l.val < n.val)
            && n.right.as_ref().is_none_or(|r| r.val > n.val)
            && is_bst_parent_only(&n.left)
            && is_bst_parent_only(&n.right)
    })
}
fn is_bst_oracle(t: &T) -> bool {
    match t {
        None => true,
        Some(n) => {
            ino(&n.left).iter().all(|&x| x < n.val)
                && ino(&n.right).iter().all(|&x| x > n.val)
                && is_bst_oracle(&n.left)
                && is_bst_oracle(&n.right)
        }
    }
}
fn q9() {
    assert!(!is_bst(&t(&[5, 1, 4, N, N, 3, 6])) && is_bst(&t(&[2, 1, 3])) && is_bst(&None));
    let tricky = t(&[5, 4, 6, N, N, 3, 7]);
    assert!(
        is_bst_parent_only(&tricky) && !is_bst(&tricky),
        "parent-only check is wrong"
    );
    assert!(
        is_bst(&node(0, leaf(i32::MIN), leaf(i32::MAX)))
            && is_bst(&node(i32::MIN, None, leaf(i32::MAX)))
    );
    assert!(!is_bst(&node(5, leaf(5), None)), "duplicates are invalid");
    let (mut yes, mut no) = (0, 0);
    let mut r = Rng(0x1009);
    for _ in 0..2000 {
        let mut tr = match r.range(3) {
            0 => rand_tree(&mut r, 8, 0, 6, false),
            _ => rand_bst(&mut r, 10),
        };
        if r.range(2) == 0 && size(&tr) > 0 {
            let mut k = r.range(size(&tr));
            set_nth(&mut tr, &mut k, r.int(-20, 20));
        }
        let want = is_bst_oracle(&tr);
        assert_eq!(is_bst(&tr), want);
        assert_eq!(is_bst_inorder(&tr), want);
        if want { yes += 1 } else { no += 1 }
    }
    assert!(yes > 100 && no > 100);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q10. Kth smallest element in a BST (1-indexed), k <= n. [3,1,4,null,2], k=1 -> 1. Approach: iterative inorder, stop after the k-th pop:
// O(h + k) time, O(h) space. Follow-up (the real question): the tree is modified often and k varies: store subtree sizes in every node
// (order-statistic tree): O(h) per query. Rust wrinkle: the same `&Tree` stack as Q1; early `return` leaves the stack to be dropped.
// Mistakes: collecting the full inorder (O(n)); off-by-one with 0- vs 1-indexing.
// ---------------------------------------------------------------------------------------------------------------------
fn kth_smallest(t: &T, mut k: usize) -> Option<i32> {
    let (mut st, mut cur) = (vec![], t.as_deref());
    while cur.is_some() || !st.is_empty() {
        while let Some(n) = cur {
            st.push(n);
            cur = n.left.as_deref();
        }
        let n = st.pop()?;
        k -= 1;
        if k == 0 {
            return Some(n.val);
        }
        cur = n.right.as_deref();
    }
    None
}
fn q10() {
    assert_eq!(kth_smallest(&t(&[3, 1, 4, N, 2]), 1), Some(1));
    assert_eq!(kth_smallest(&t(&[5, 3, 6, 2, 4, N, N, 1]), 3), Some(3));
    let mut r = Rng(0x100A);
    for _ in 0..2000 {
        let tr = rand_bst(&mut r, 15);
        let s = ino(&tr);
        for k in 1..=s.len() {
            assert_eq!(kth_smallest(&tr, k), Some(s[k - 1]));
        }
        assert_eq!(kth_smallest(&tr, s.len() + 1), None);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q11. Lowest common ancestor of p and q (a node may be its own ancestor). General tree and BST. [3,5,1,6,2,0,8,null,null,7,4], p=5, q=1 -> 3;
// p=5, q=4 -> 5. General: post-order, return the node if it is p or q; if both sides return something the current node is the LCA: O(n) time,
// O(h) space. BST: walk down: both smaller go left, both larger go right, else split point: O(h) time, O(1) space (iterative).
// Rust wrinkle: the function returns `Option<&'a Tree>` borrowed from the input (lifetime `'a` ties the result to the tree, no clone);
// `match (l, r)` merges the two answers. Mistakes: assuming the BST property in the general version; assuming both nodes exist (follow-up).
// Follow-ups: with parent pointers (Q23), many queries (Euler tour + RMQ, binary lifting), LCA of k nodes.
// ---------------------------------------------------------------------------------------------------------------------
fn lca<'a>(t: Option<&'a Tree>, p: i32, q: i32) -> Option<&'a Tree> {
    let n = t?;
    if n.val == p || n.val == q {
        return Some(n);
    }
    match (lca(n.left.as_deref(), p, q), lca(n.right.as_deref(), p, q)) {
        (Some(_), Some(_)) => Some(n),
        (x, None) => x,
        (None, y) => y,
    }
}
fn lca_bst(t: &T, p: i32, q: i32) -> Option<i32> {
    let mut cur = t.as_deref();
    while let Some(n) = cur {
        if p < n.val && q < n.val {
            cur = n.left.as_deref();
        } else if p > n.val && q > n.val {
            cur = n.right.as_deref();
        } else {
            return Some(n.val);
        }
    }
    None
}
fn path_to(t: &T, v: i32) -> Option<Vec<i32>> {
    let n = t.as_ref()?;
    if n.val == v {
        return Some(vec![v]);
    }
    let mut p = path_to(&n.left, v).or_else(|| path_to(&n.right, v))?;
    p.insert(0, n.val);
    Some(p)
}
fn lca_oracle(t: &T, p: i32, q: i32) -> Option<i32> {
    let (a, b) = (path_to(t, p)?, path_to(t, q)?);
    a.iter()
        .zip(&b)
        .take_while(|(x, y)| x == y)
        .last()
        .map(|(x, _)| *x)
}
fn q11() {
    let x = t(&[3, 5, 1, 6, 2, 0, 8, N, N, 7, 4]);
    assert_eq!(lca(x.as_deref(), 5, 1).map(|n| n.val), Some(3));
    assert_eq!(lca(x.as_deref(), 5, 4).map(|n| n.val), Some(5));
    let mut r = Rng(0x100B);
    for _ in 0..2000 {
        let n = 1 + r.range(14);
        let tr = unique_tree(&mut r, n);
        let vals = pre(&tr);
        let (p, q) = (vals[r.range(n)], vals[r.range(n)]);
        assert_eq!(
            lca(tr.as_deref(), p, q).map(|x| x.val),
            lca_oracle(&tr, p, q)
        );
        let b = bst_from(&{
            let mut v: Vec<i32> = (0..n as i32).map(|x| x * 2 - 7).collect();
            for i in (1..v.len()).rev() {
                v.swap(i, r.range(i + 1));
            }
            v
        });
        let bv = pre(&b);
        let (p, q) = (bv[r.range(n)], bv[r.range(n)]);
        assert_eq!(lca_bst(&b, p, q), lca_oracle(&b, p, q));
        assert_eq!(lca_bst(&b, p, q), lca(b.as_deref(), p, q).map(|x| x.val));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q12. BST insert and delete. Insert: walk down comparing, attach at the first empty slot (O(h)). Delete has THREE cases: (1) leaf: remove;
// (2) one child: replace the node by that child; (3) two children: replace the value by the inorder successor (minimum of the right subtree)
// and remove the successor from the right subtree (or symmetric predecessor). O(h) time. Duplicates are ignored on insert.
// Rust wrinkle: both functions take `&mut Option<Box<Tree>>` (the SLOT that holds the node), so "replace this node by its child" is just
// `*slot = child`: no parent pointer, no special-casing the root. Deleting needs the node moved out (`slot.take()`) before the slot is
// reassigned, because `n` borrowed from the slot must be dead first. `take_min` detaches the minimum and splices in its right child.
// Mistakes: forgetting to re-link the successor's right child; deleting the root; leaking the old node (Rust frees it for you).
// Follow-ups: AVL/red-black rebalancing; iterative versions (see Q21); delete by predecessor alternately to avoid skew.
// ---------------------------------------------------------------------------------------------------------------------
fn bst_insert(t: &mut T, v: i32) -> bool {
    match t {
        None => {
            *t = leaf(v);
            true
        }
        Some(n) => {
            if v < n.val {
                bst_insert(&mut n.left, v)
            } else if v > n.val {
                bst_insert(&mut n.right, v)
            } else {
                false
            }
        }
    }
}
fn bst_contains(t: &T, v: i32) -> bool {
    let mut cur = t.as_deref();
    while let Some(n) = cur {
        cur = if v < n.val {
            n.left.as_deref()
        } else if v > n.val {
            n.right.as_deref()
        } else {
            return true;
        };
    }
    false
}
fn take_min(slot: &mut T) -> Option<i32> {
    if slot.as_ref()?.left.is_some() {
        take_min(&mut slot.as_mut().unwrap().left)
    } else {
        let mut n = slot.take()?;
        *slot = n.right.take(); // the successor's right subtree takes its place
        Some(n.val)
    }
}
fn bst_delete(slot: &mut T, v: i32) -> bool {
    let Some(n) = slot else { return false };
    if v < n.val {
        return bst_delete(&mut n.left, v);
    }
    if v > n.val {
        return bst_delete(&mut n.right, v);
    }
    let mut node = slot.take().unwrap(); // found: n is dead from here on
    match (node.left.take(), node.right.take()) {
        (None, None) => {}                                    // case 1: leaf
        (Some(c), None) | (None, Some(c)) => *slot = Some(c), // case 2: one child
        (Some(l), Some(r)) => {
            // case 3: two children
            let mut right = Some(r);
            node.val = take_min(&mut right).unwrap();
            node.left = Some(l);
            node.right = right;
            *slot = Some(node);
        }
    }
    true
}
fn q12() {
    let mut x = t(&[5, 3, 6, 2, 4, N, 7]);
    assert!(bst_delete(&mut x, 3));
    assert_eq!(ino(&x), vec![2, 4, 5, 6, 7]);
    assert!(bst_delete(&mut x, 5) && is_bst(&x) && !bst_delete(&mut x, 99));
    let mut e: T = None;
    assert!(!bst_delete(&mut e, 1));
    let mut model = BTreeSet::new();
    let mut tr: T = None;
    let mut r = Rng(0x100C);
    for _ in 0..6000 {
        let v = r.int(-15, 15);
        match r.range(3) {
            0 => assert_eq!(bst_insert(&mut tr, v), model.insert(v)),
            1 => assert_eq!(bst_delete(&mut tr, v), model.remove(&v)),
            _ => assert_eq!(bst_contains(&tr, v), model.contains(&v)),
        }
        assert!(is_bst(&tr));
        assert_eq!(ino(&tr), model.iter().copied().collect::<Vec<_>>());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q13. Serialize a binary tree to a string and deserialize it back (any format). Approach: preorder with a `#` marker for null:
// "1,2,#,#,3,4,#,#,5,#,#"; deserialize by consuming tokens recursively (a preorder with null markers determines the tree uniquely). O(n)
// time and space. Alternative: level-order like LeetCode (`levels` above). Both round-trip below, including negative values.
// Rust wrinkle: the deserializer takes `&mut impl Iterator<Item = &str>` so the recursion shares one cursor; no index juggling, no clones.
// Mistakes: omitting null markers (ambiguous), splitting on a delimiter that may appear in values, deep recursion on skewed input
// (iterate with a stack for 100k+ levels). Follow-ups: serialize a BST compactly (preorder alone suffices); n-ary trees; binary encoding.
// ---------------------------------------------------------------------------------------------------------------------
fn serialize(t: &T) -> String {
    fn go(t: &T, out: &mut Vec<String>) {
        match t {
            None => out.push("#".into()),
            Some(n) => {
                out.push(n.val.to_string());
                go(&n.left, out);
                go(&n.right, out);
            }
        }
    }
    let mut v = vec![];
    go(t, &mut v);
    v.join(",")
}
fn deserialize(s: &str) -> T {
    fn go<'a>(it: &mut impl Iterator<Item = &'a str>) -> T {
        match it.next() {
            None | Some("#") => None,
            Some(x) => {
                let left = go(it);
                let right = go(it);
                node(x.parse().unwrap(), left, right)
            }
        }
    }
    go(&mut s.split(','))
}
fn q13() {
    assert_eq!(
        serialize(&t(&[1, 2, 3, N, N, 4, 5])),
        "1,2,#,#,3,4,#,#,5,#,#"
    );
    assert!(deserialize("#").is_none());
    let mut r = Rng(0x100D);
    for _ in 0..2000 {
        let n = r.range(16);
        let tr = rt(&mut r, n, -50, 50);
        assert_eq!(levels(&deserialize(&serialize(&tr))), levels(&tr));
        assert_eq!(
            levels(&from_levels(&levels(&tr))),
            levels(&tr),
            "level-order format round-trips too"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q14. Build a binary tree from its preorder and inorder traversals (all values distinct). preorder [3,9,20,15,7], inorder [9,3,15,20,7] ->
// [3,9,20,null,null,15,7]. Approach: preorder's next element is the root; its position in inorder splits the left/right subtrees. A HashMap
// value -> inorder index makes the split O(1): O(n) time and space (the naive `position` scan is O(n^2)). Build left first, then right,
// advancing one shared preorder cursor. Rust wrinkle: indices (`lo..hi` into the inorder array) instead of slice copies; the cursor is
// `&mut usize`. Mistakes: building right before left (cursor order!); copying slices (O(n^2)). Follow-ups: inorder+postorder (root from the
// back, build right first); why preorder+postorder is ambiguous; BST from preorder only.
// ---------------------------------------------------------------------------------------------------------------------
fn build_pre_in(pre: &[i32], ino: &[i32]) -> T {
    fn go(pre: &[i32], pi: &mut usize, pos: &HashMap<i32, usize>, lo: usize, hi: usize) -> T {
        if lo >= hi {
            return None;
        }
        let v = pre[*pi];
        *pi += 1;
        let m = pos[&v];
        let left = go(pre, pi, pos, lo, m);
        let right = go(pre, pi, pos, m + 1, hi);
        node(v, left, right)
    }
    let pos: HashMap<i32, usize> = ino.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    go(pre, &mut 0, &pos, 0, ino.len())
}
fn q14() {
    let x = build_pre_in(&[3, 9, 20, 15, 7], &[9, 3, 15, 20, 7]);
    assert_eq!(levels(&x), lv(&[3, 9, 20, N, N, 15, 7]));
    assert!(build_pre_in(&[], &[]).is_none());
    let mut r = Rng(0x100E);
    for _ in 0..2000 {
        let n = r.range(20);
        let tr = unique_tree(&mut r, n);
        let rebuilt = build_pre_in(&pre(&tr), &ino(&tr));
        assert_eq!(levels(&rebuilt), levels(&tr));
        assert_eq!(post(&rebuilt), post(&tr));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q15. Convert a sorted array to a height-balanced BST (heights of subtrees differ by <= 1). [-10,-3,0,5,9] -> [0,-3,9,-10,null,5].
// Approach: the middle element is the root; recurse on the halves: O(n) time, O(log n) stack. For a sorted LINKED LIST (no random access)
// the trick is to build in INORDER: `build(iter, n)` makes the left subtree from the first n/2 items, takes the next item as the root, then
// builds the right: O(n) time without finding middles (the "slow/fast pointer" solution is O(n log n)). Rust wrinkle: a slice split
// `&v[..m]`, `&v[m+1..]` is free (no copy); the list variant consumes any `Iterator`, so it works on a linked list's `iter()` as well.
// Mistakes: (lo+hi)/2 overflow in other languages; building unbalanced by always taking the first element. Follow-ups: balance an existing BST.
// ---------------------------------------------------------------------------------------------------------------------
fn sorted_to_bst(v: &[i32]) -> T {
    if v.is_empty() {
        return None;
    }
    let m = v.len() / 2;
    node(v[m], sorted_to_bst(&v[..m]), sorted_to_bst(&v[m + 1..]))
}
fn sorted_iter_to_bst(it: &mut impl Iterator<Item = i32>, n: usize) -> T {
    if n == 0 {
        return None;
    }
    let left = sorted_iter_to_bst(it, n / 2);
    let v = it.next().unwrap();
    let right = sorted_iter_to_bst(it, n - n / 2 - 1);
    node(v, left, right)
}
fn q15() {
    assert_eq!(
        levels(&sorted_to_bst(&[-10, -3, 0, 5, 9])),
        lv(&[0, -3, 9, -10, N, 5])
    );
    assert!(sorted_to_bst(&[]).is_none());
    let mut r = Rng(0x100F);
    for _ in 0..2000 {
        let n = r.range(40);
        let mut v: Vec<i32> = (0..n).map(|_| r.int(-100, 100)).collect();
        v.sort();
        v.dedup();
        for tr in [
            sorted_to_bst(&v),
            sorted_iter_to_bst(&mut v.iter().copied(), v.len()),
        ] {
            assert_eq!(ino(&tr), v);
            assert!(is_bst(&tr) && is_balanced(&tr));
            let min_h = (usize::BITS - v.len().leading_zeros()) as usize; // ceil(log2(n+1))
            assert_eq!(
                height(&tr),
                min_h,
                "a balanced BST of n nodes has the minimum possible height"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q16. Flatten a binary tree to a "linked list" IN PLACE using the right pointers, in preorder; every left must become null.
// [1,2,5,3,4,null,6] -> 1->2->3->4->5->6. Approaches: (1) preorder into a Vec then relink: O(n) space. (2) reverse postorder with a `prev`
// pointer: visit right, then left, then the node, setting node.right = prev: O(n) time, O(h) space. (3) Morris-style: O(1) space.
// Rust wrinkle: we do (2) FUNCTIONALLY: `flat(tree, rest)` returns the flattened tree followed by `rest`. Ownership moves through the call
// (`take()` both children, flatten the right one first against `rest`, then the left one against that result, then hook it under the node), so
// there are no aliasing pointers and no `prev` variable. Mistakes: forgetting `left = None`; linking before saving the right child.
// ---------------------------------------------------------------------------------------------------------------------
fn flat(t: T, rest: T) -> T {
    match t {
        None => rest,
        Some(mut n) => {
            let (l, r) = (n.left.take(), n.right.take());
            let tail = flat(r, rest);
            n.right = flat(l, tail);
            Some(n)
        }
    }
}
fn flatten_tree(t: T) -> T {
    flat(t, None)
}
fn q16() {
    let x = flatten_tree(t(&[1, 2, 5, 3, 4, N, 6]));
    assert_eq!(pre(&x), vec![1, 2, 3, 4, 5, 6]);
    let mut r = Rng(0x1010);
    for _ in 0..2000 {
        let n = r.range(20);
        let tr = rt(&mut r, n, -9, 9);
        let before = pre(&tr);
        let f = flatten_tree(tr);
        let (mut chain, mut cur) = (vec![], f.as_deref());
        while let Some(n) = cur {
            assert!(n.left.is_none(), "all left pointers must be null");
            chain.push(n.val);
            cur = n.right.as_deref();
        }
        assert_eq!(chain, before);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q17. Count the nodes of a COMPLETE binary tree faster than O(n). Approach: measure the leftmost depth and the rightmost depth. If equal,
// the tree is perfect: 2^h - 1 nodes (no recursion). Otherwise 1 + count(left) + count(right); at every level at least one side is perfect, so
// only one recursive call does real work: O(log n) levels * O(log n) depth measurements = O(log^2 n). Rust wrinkle: depth is measured by
// walking `as_deref()` links; a `steps` counter proves the bound (no global state). Mistakes: counting with plain recursion (O(n)); using
// `pow` with floating point (use shifts). Follow-up: implement by binary search on the last level using the bit path of the index.
// ---------------------------------------------------------------------------------------------------------------------
fn count_complete(t: &T, steps: &mut usize) -> usize {
    let depth = |go_left: bool, steps: &mut usize| {
        let (mut d, mut c) = (0, t.as_deref());
        while let Some(n) = c {
            d += 1;
            *steps += 1;
            c = if go_left {
                n.left.as_deref()
            } else {
                n.right.as_deref()
            };
        }
        d
    };
    let (lh, rh) = (depth(true, steps), depth(false, steps));
    if lh == rh {
        return (1usize << lh) - 1;
    }
    match t {
        Some(n) => 1 + count_complete(&n.left, steps) + count_complete(&n.right, steps),
        None => 0,
    }
}
fn q17() {
    for n in 0..=600usize {
        let lvls: Vec<Option<i32>> = (0..n as i32).map(Some).collect(); // level order with no gaps is a complete tree
        let tr = from_levels(&lvls);
        let mut steps = 0;
        assert_eq!(count_complete(&tr, &mut steps), n);
        let lg = (usize::BITS - n.leading_zeros()) as usize + 1;
        assert!(
            steps <= 2 * lg * lg,
            "n={n} steps={steps}: must be O(log^2 n)"
        );
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q18. Convert a BST/binary tree into a sorted CIRCULAR DOUBLY LINKED list IN PLACE: use `left` as prev and `right` as next, in inorder.
// Approach: iterative inorder; keep `prev`; for each visited node link prev<->node; at the end link the tail back to the head: O(n) time,
// O(h) space. Rust wrinkle: a doubly linked structure cannot be built from `Box` children (a node would need two owners), so the nodes are
// ARENA INDICES (`Option<usize>`) and re-linking is plain assignment. We must read a node's original `right` BEFORE overwriting it.
// Mistakes: overwriting `right` before descending; forgetting to close the ring; returning the root instead of the leftmost node.
// Follow-ups: do it recursively; split the list back into a balanced BST in O(n).
// ---------------------------------------------------------------------------------------------------------------------
fn tree_to_dll(a: &mut Arena) -> Option<usize> {
    let (mut st, mut cur, mut prev, mut head) = (vec![], a.root, None::<usize>, None::<usize>);
    while cur.is_some() || !st.is_empty() {
        while let Some(c) = cur {
            st.push(c);
            cur = a.left[c];
        }
        let n = st.pop().unwrap();
        let next_cur = a.right[n]; // read before rewiring
        match prev {
            Some(p) => {
                a.right[p] = Some(n);
                a.left[n] = Some(p);
            }
            None => {
                head = Some(n);
                a.left[n] = None;
            }
        }
        prev = Some(n);
        cur = next_cur;
    }
    if let (Some(h), Some(tl)) = (head, prev) {
        a.left[h] = Some(tl);
        a.right[tl] = Some(h);
    }
    head
}
fn q18() {
    assert!(tree_to_dll(&mut to_arena(&None)).is_none());
    let mut r = Rng(0x1012);
    for _ in 0..2000 {
        let n = r.range(16);
        let tr = rt(&mut r, n, -9, 9);
        let want = ino(&tr);
        let mut a = to_arena(&tr);
        let head = tree_to_dll(&mut a);
        let mut fwd = vec![];
        if let Some(h) = head {
            let mut c = h;
            loop {
                fwd.push(a.val[c]);
                c = a.right[c].unwrap();
                if c == h {
                    break;
                }
            }
            let tail = a.left[h].unwrap();
            let mut back = vec![];
            let mut c = tail;
            loop {
                back.push(a.val[c]);
                c = a.left[c].unwrap();
                if c == tail {
                    break;
                }
            }
            back.reverse();
            assert_eq!(back, want);
        }
        assert_eq!(fwd, want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q19. Vertical order traversal: group nodes by column (root = 0, left child = col-1, right child = col+1), columns left to right; inside a
// column order by row (top to bottom), ties by value. [3,9,20,null,null,15,7] -> [[9],[3,15],[20],[7]]. Approach: BFS carrying (col, row),
// collect into a `BTreeMap<col, Vec<(row, val)>>` (sorted keys for free), sort each column. O(n log n) time. Rust wrinkle: `BTreeMap` gives
// ordered iteration so no min/max column bookkeeping; `entry().or_default()` replaces the "if absent insert vec" dance. Mistakes: DFS without
// tracking rows (wrong order inside a column); ties by insertion order instead of value (LeetCode 987 sorts by value).
// ---------------------------------------------------------------------------------------------------------------------
fn vertical_order(t: &T) -> Vec<Vec<i32>> {
    let mut cols: BTreeMap<i32, Vec<(usize, i32)>> = BTreeMap::new();
    let mut q: VecDeque<(&Tree, i32, usize)> =
        t.as_deref().map(|n| (n, 0, 0)).into_iter().collect();
    while let Some((n, c, row)) = q.pop_front() {
        cols.entry(c).or_default().push((row, n.val));
        q.extend(n.left.as_deref().map(|l| (l, c - 1, row + 1)));
        q.extend(n.right.as_deref().map(|r| (r, c + 1, row + 1)));
    }
    cols.into_values()
        .map(|mut v| {
            v.sort();
            v.into_iter().map(|x| x.1).collect()
        })
        .collect()
}
fn vertical_oracle(t: &T) -> Vec<Vec<i32>> {
    fn go(t: &T, c: i32, r: usize, out: &mut Vec<(i32, usize, i32)>) {
        if let Some(n) = t {
            out.push((c, r, n.val));
            go(&n.left, c - 1, r + 1, out);
            go(&n.right, c + 1, r + 1, out);
        }
    }
    let mut all = vec![];
    go(t, 0, 0, &mut all);
    all.sort();
    let mut res: Vec<Vec<i32>> = vec![];
    let mut last = None;
    for (c, _, v) in all {
        if last == Some(c) {
            res.last_mut().unwrap().push(v);
        } else {
            res.push(vec![v]);
            last = Some(c);
        }
    }
    res
}
fn q19() {
    assert_eq!(
        vertical_order(&t(&[3, 9, 20, N, N, 15, 7])),
        vec![vec![9], vec![3, 15], vec![20], vec![7]]
    );
    assert!(vertical_order(&None).is_empty());
    let mut r = Rng(0x1013);
    for _ in 0..2000 {
        let n = r.range(18);
        let tr = rt(&mut r, n, -3, 3);
        assert_eq!(vertical_order(&tr), vertical_oracle(&tr));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q20. Exactly two nodes of a BST were swapped by mistake; restore the tree WITHOUT changing its structure. Idea: in the inorder sequence
// a swap creates one descent (adjacent swap) or two descents (far apart). First violation's LARGER element and last violation's SMALLER
// element are the culprits. O(n) time, O(h) space with an explicit stack (Morris gives O(1)). Then swap the two VALUES.
// Rust wrinkle: the read-only inorder borrows the tree (`&Tree` stack); only after that borrow ends do we mutate through `&mut`, which is
// exactly the two-phase "find, then fix" shape the borrow checker wants (no two live pointers into the tree at once).
// Mistakes: assuming two violations always exist (adjacent swap has one: [3,2] case); swapping the wrong pair; comparing with `>=`.
// ---------------------------------------------------------------------------------------------------------------------
fn swap_vals(t: &mut T, a: i32, b: i32) {
    if let Some(n) = t {
        if n.val == a {
            n.val = b;
        } else if n.val == b {
            n.val = a;
        }
        swap_vals(&mut n.left, a, b);
        swap_vals(&mut n.right, a, b);
    }
}
fn recover_tree(t: &mut T) {
    let (mut first, mut second, mut prev) = (None::<i32>, None::<i32>, None::<i32>);
    {
        let (mut st, mut cur) = (vec![], t.as_deref());
        while cur.is_some() || !st.is_empty() {
            while let Some(n) = cur {
                st.push(n);
                cur = n.left.as_deref();
            }
            let n = st.pop().unwrap();
            if let Some(p) = prev {
                if p > n.val {
                    if first.is_none() {
                        first = Some(p);
                    }
                    second = Some(n.val);
                }
            }
            prev = Some(n.val);
            cur = n.right.as_deref();
        }
    }
    if let (Some(a), Some(b)) = (first, second) {
        swap_vals(t, a, b);
    }
}
fn q20() {
    let mut x = t(&[1, 3, N, N, 2]);
    recover_tree(&mut x);
    assert_eq!(levels(&x), lv(&[3, 1, N, N, 2]));
    let mut adj = t(&[2, 3, 1]);
    recover_tree(&mut adj);
    assert!(is_bst(&adj));
    let mut r = Rng(0x1014);
    for _ in 0..2000 {
        let mut vals: Vec<i32> = (0..2 + r.range(14) as i32).map(|x| x * 2 - 9).collect();
        for i in (1..vals.len()).rev() {
            vals.swap(i, r.range(i + 1));
        }
        let good = bst_from(&vals);
        let sorted = ino(&good);
        let (a, b) = {
            let i = r.range(sorted.len());
            let mut j = r.range(sorted.len());
            if j == i {
                j = (j + 1) % sorted.len();
            }
            (sorted[i], sorted[j])
        };
        let mut bad = clone_tree(&good);
        swap_vals(&mut bad, a, b);
        assert!(!is_bst(&bad));
        recover_tree(&mut bad);
        assert_eq!(levels(&bad), levels(&good));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q21. Which borrow-checker patterns do recursive functions over `Option<Box<Tree>>` use? The four to know (all tested below):
//  (1) READ-ONLY recursion takes `&T` and uses `as_ref().map_or(..)` / `match t { None => .., Some(n) => .. }`: `n` is `&Box<Tree>`.
//  (2) MUTATION takes `&mut T` (the slot) and `if let Some(n) = t { n.val += d; recurse(&mut n.left) ... }`.
//  (3) TRAVERSAL WITHOUT RECURSION: `as_deref()` converts `&Option<Box<Tree>>` into `Option<&Tree>`, which is Copy, so cursors and `Vec<&Tree>`
//      stacks are cheap and several can coexist (shared borrows).
//  (4) DESCEND-AND-REPLACE with a `&mut Option<Box<Tree>>` cursor: `while cur.is_some() { cur = &mut cur.as_mut().unwrap().left }` and finally
//      `*cur = Some(new)`; the `while let Some(n) = cur` spelling moves the cursor and fails after the loop.
// Rust wrinkle summary: you cannot hold `&mut` to a node and its child's parent at once, so algorithms are written in terms of SLOTS (the
// Option holding a node) and `take()`, not parent pointers. `Option::map` returns new owned data (e.g. cloning a tree); `Option::take` moves out.
// ---------------------------------------------------------------------------------------------------------------------
fn sum_tree(t: &T) -> i64 {
    t.as_ref()
        .map_or(0, |n| n.val as i64 + sum_tree(&n.left) + sum_tree(&n.right))
}
fn add_all(t: &mut T, d: i32) {
    if let Some(n) = t {
        n.val += d;
        add_all(&mut n.left, d);
        add_all(&mut n.right, d);
    }
}
fn leftmost(t: &T) -> Option<&i32> {
    let mut cur = t.as_deref()?;
    while let Some(l) = cur.left.as_deref() {
        cur = l;
    }
    Some(&cur.val)
}
fn insert_iter(t: &mut T, v: i32) {
    let mut cur = t;
    while cur.is_some() {
        let n = cur.as_mut().unwrap();
        cur = if v < n.val { &mut n.left } else { &mut n.right };
    }
    *cur = leaf(v);
}
fn q21() {
    let mut r = Rng(0x1015);
    for _ in 0..2000 {
        let n = r.range(15);
        let mut tr = rt(&mut r, n, -50, 50);
        let vals = pre(&tr);
        assert_eq!(sum_tree(&tr), vals.iter().map(|&x| x as i64).sum::<i64>());
        add_all(&mut tr, 7);
        assert_eq!(pre(&tr), vals.iter().map(|x| x + 7).collect::<Vec<_>>());
        assert_eq!(leftmost(&tr).copied(), {
            let mut c = tr.as_deref();
            let mut last = None;
            while let Some(x) = c {
                last = Some(x.val);
                c = x.left.as_deref();
            }
            last
        });
    }
    let (mut a, mut b): (T, T) = (None, None);
    let mut r = Rng(0x1016);
    for _ in 0..500 {
        let v = r.int(-1000, 1000);
        insert_iter(&mut a, v);
        let mut c = b.take();
        // recursive insert variant ignores duplicates; insert_iter puts them right. Compare after normalising on unique values.
        if !bst_contains(&c, v) {
            bst_insert(&mut c, v);
        }
        b = c;
    }
    let mut ua = ino(&a);
    ua.dedup();
    assert_eq!(ua, ino(&b));
}

// ---------------------------------------------------------------------------------------------------------------------
// Q22. LeetCode's Rust tree type is `Option<Rc<RefCell<TreeNode>>>`. How do you work with it? (a) Traversal: `node.borrow()` gives a `Ref`
// that must be dropped before you `borrow_mut()` the same node; clone the child `Rc` (a refcount bump, NOT a copy of the subtree) to keep it
// beyond the borrow. (b) Mutation: `node.borrow_mut().left = ...`. (c) `Rc::clone(&x)` shares ONE node (mutations through either handle are
// visible, `Rc::strong_count` rises); a DEEP COPY must rebuild every node. (d) Why `RefCell`? `Rc` alone is immutable; RefCell moves the
// borrow rules to RUNTIME, so a second `borrow_mut()` while one is alive PANICS ("already borrowed") instead of failing to compile.
// Interviewer is testing: do you know the type is unavoidable on LeetCode, but that in your own code `Option<Box<_>>` is simpler and faster
// (no counts, no runtime checks). Mistakes: holding `borrow()` across a recursive call that mutates the same node; `unwrap()` on a shared Rc
// inside `Rc::try_unwrap`. Follow-up: Rc cycles leak (use Weak, Q23). All functions below are checked against their Box twins.
// ---------------------------------------------------------------------------------------------------------------------
struct RcTree {
    val: i32,
    left: R,
    right: R,
}
type R = Option<Rc<RefCell<RcTree>>>;
fn to_rc(t: &T) -> R {
    t.as_ref().map(|n| {
        Rc::new(RefCell::new(RcTree {
            val: n.val,
            left: to_rc(&n.left),
            right: to_rc(&n.right),
        }))
    })
}
fn from_rc(r: &R) -> T {
    r.as_ref().map(|n| {
        let b = n.borrow();
        Box::new(Tree {
            val: b.val,
            left: from_rc(&b.left),
            right: from_rc(&b.right),
        })
    })
}
fn max_depth_rc(r: &R) -> usize {
    r.as_ref().map_or(0, |n| {
        let b = n.borrow();
        1 + max_depth_rc(&b.left).max(max_depth_rc(&b.right))
    })
}
fn invert_rc(r: &R) {
    if let Some(n) = r {
        let mut b = n.borrow_mut();
        let (l, rr) = (b.left.take(), b.right.take());
        b.left = rr;
        b.right = l;
        invert_rc(&b.left); // children are different RefCells, so these borrows are fine
        invert_rc(&b.right);
    }
}
fn inorder_rc(root: &R) -> Vec<i32> {
    let (mut out, mut st, mut cur) = (vec![], vec![], root.clone());
    while cur.is_some() || !st.is_empty() {
        while let Some(n) = cur {
            let l = n.borrow().left.clone();
            st.push(n);
            cur = l;
        }
        let n: Rc<RefCell<RcTree>> = st.pop().unwrap();
        out.push(n.borrow().val);
        cur = n.borrow().right.clone();
    }
    out
}
fn lca_rc(r: &R, p: i32, q: i32) -> R {
    let n = r.as_ref()?;
    if n.borrow().val == p || n.borrow().val == q {
        return r.clone(); // Rc::clone: a new handle to the SAME node
    }
    let (l, rr) = (
        lca_rc(&n.borrow().left, p, q),
        lca_rc(&n.borrow().right, p, q),
    );
    match (l, rr) {
        (Some(_), Some(_)) => r.clone(),
        (x, None) => x,
        (None, y) => y,
    }
}
fn q22() {
    let mut r = Rng(0x1017);
    for _ in 0..2000 {
        let n = r.range(14);
        let tr = unique_tree(&mut r, n);
        let rc = to_rc(&tr);
        assert_eq!(max_depth_rc(&rc), max_depth(&tr));
        assert_eq!(inorder_rc(&rc), ino(&tr));
        if n > 0 {
            let vals = pre(&tr);
            let (p, q) = (vals[r.range(n)], vals[r.range(n)]);
            assert_eq!(
                lca_rc(&rc, p, q).map(|x| x.borrow().val),
                lca(tr.as_deref(), p, q).map(|x| x.val)
            );
        }
        invert_rc(&rc);
        let mut b = clone_tree(&tr);
        invert(&mut b);
        assert_eq!(levels(&from_rc(&rc)), levels(&b));
    }
    // Rc::clone shares the node, a deep copy does not
    let root = to_rc(&t(&[1, 2, 3]));
    let shared = root.clone();
    assert_eq!(Rc::strong_count(root.as_ref().unwrap()), 2);
    shared.as_ref().unwrap().borrow_mut().val = 99;
    assert_eq!(
        root.as_ref().unwrap().borrow().val,
        99,
        "same node: change is visible through both handles"
    );
    let deep = to_rc(&from_rc(&root));
    deep.as_ref().unwrap().borrow_mut().val = -1;
    assert_eq!(
        root.as_ref().unwrap().borrow().val,
        99,
        "deep copy is independent"
    );
    assert_eq!(Rc::strong_count(deep.as_ref().unwrap()), 1);
    // RefCell enforces the borrow rules at RUNTIME
    let cell = Rc::new(RefCell::new(5));
    {
        let _g1 = cell.borrow_mut();
        assert!(
            cell.try_borrow_mut().is_err() && cell.try_borrow().is_err(),
            "second borrow_mut is refused"
        );
    }
    assert!(cell.try_borrow_mut().is_ok());
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {})); // keep the expected panic message out of the output
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _a = cell.borrow_mut();
        let _b = cell.borrow_mut(); // panics: already borrowed
    }));
    std::panic::set_hook(hook);
    assert!(
        caught.is_err(),
        "a double borrow_mut panics instead of compiling or silently aliasing"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q23. Give every node a PARENT pointer (needed for LCA with parent links, successor queries, "walk to root"). A child owns nothing of its
// parent, so the parent link must be non-owning: `Weak<Node>`. Children are strong (`Rc`) and parent links weak, so there is no reference
// cycle: dropping the root frees the whole tree (a strong parent<->child pair would leak forever). Reading a parent is `weak.upgrade()`,
// which returns `Option<Rc<_>>` (None if the parent is already gone). Cost: counts, runtime checks, `RefCell` for the late-bound fields
// (the Rc must exist before children can point at it). Approach for the LCA: lift the deeper node until depths match, then lift both:
// O(h) time, O(1) space (same shape as the list-intersection trick). Mistakes: `Rc` parent pointers (leak); `unwrap()` on `upgrade()`.
// Follow-up: arena/index trees (parent = usize) avoid all of this and are the preferred design for big trees.
// ---------------------------------------------------------------------------------------------------------------------
struct PNode {
    val: i32,
    parent: RefCell<Weak<PNode>>,
    left: RefCell<Option<Rc<PNode>>>,
    right: RefCell<Option<Rc<PNode>>>,
}
fn to_parented(t: &T, parent: &Weak<PNode>) -> Option<Rc<PNode>> {
    t.as_ref().map(|n| {
        let me = Rc::new(PNode {
            val: n.val,
            parent: RefCell::new(parent.clone()),
            left: RefCell::new(None),
            right: RefCell::new(None),
        });
        let w = Rc::downgrade(&me);
        *me.left.borrow_mut() = to_parented(&n.left, &w);
        *me.right.borrow_mut() = to_parented(&n.right, &w);
        me
    })
}
fn find_p(n: &Rc<PNode>, v: i32) -> Option<Rc<PNode>> {
    if n.val == v {
        return Some(n.clone());
    }
    let l = n.left.borrow().as_ref().and_then(|c| find_p(c, v));
    l.or_else(|| n.right.borrow().as_ref().and_then(|c| find_p(c, v)))
}
fn depth_p(n: &Rc<PNode>) -> usize {
    let (mut d, mut cur) = (0, n.parent.borrow().upgrade());
    while let Some(p) = cur {
        d += 1;
        cur = p.parent.borrow().upgrade();
    }
    d
}
fn lca_parent(a: &Rc<PNode>, b: &Rc<PNode>) -> Rc<PNode> {
    let (mut x, mut y) = (a.clone(), b.clone());
    let (mut dx, mut dy) = (depth_p(&x), depth_p(&y));
    while dx > dy {
        let p = x.parent.borrow().upgrade().unwrap();
        x = p;
        dx -= 1;
    }
    while dy > dx {
        let p = y.parent.borrow().upgrade().unwrap();
        y = p;
        dy -= 1;
    }
    while !Rc::ptr_eq(&x, &y) {
        let (px, py) = (
            x.parent.borrow().upgrade().unwrap(),
            y.parent.borrow().upgrade().unwrap(),
        );
        x = px;
        y = py;
    }
    x
}
fn q23() {
    let mut r = Rng(0x1018);
    for _ in 0..2000 {
        let n = 1 + r.range(14);
        let tr = unique_tree(&mut r, n);
        let root = to_parented(&tr, &Weak::new()).unwrap();
        let vals = pre(&tr);
        let (p, q) = (vals[r.range(n)], vals[r.range(n)]);
        let (a, b) = (find_p(&root, p).unwrap(), find_p(&root, q).unwrap());
        assert_eq!(Some(lca_parent(&a, &b).val), lca_oracle(&tr, p, q));
    }
    // no cycle: children point UP weakly, so dropping the root really frees everything
    let tr = t(&[1, 2, 3]);
    let root = to_parented(&tr, &Weak::new()).unwrap();
    let child = root.left.borrow().clone().unwrap();
    assert_eq!(
        Rc::strong_count(&root),
        1,
        "children hold only Weak references to the root"
    );
    assert_eq!(Rc::weak_count(&root), 2);
    assert_eq!(child.parent.borrow().upgrade().map(|p| p.val), Some(1));
    drop(root);
    assert!(
        child.parent.borrow().upgrade().is_none(),
        "parent is gone, Weak::upgrade returns None (no leak, no dangling pointer)"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q24. Why does a 100,000-level-deep tree crash a recursive solution, and which parts of your code are affected? A skewed tree (a linked list
// in disguise) has height n. Every recursive function (height, inorder, invert, even the compiler-generated Drop!) uses one stack frame per
// level; ~100 bytes * 100k blows an 8 MB main stack, a 2 MB thread stack, and certainly the 256 KiB used below. Stack overflow is an ABORT in
// Rust (not a catchable panic), so this recipe never runs the recursive version on it. Fixes: (1) iterative traversals with a heap `Vec` stack
// (`inorder_iter`, `level_order`, Morris on indices); (2) a custom iterative `Drop` (the one at the top of this file); (3) in production,
// balance the tree. Below, a 100,000-deep left spine is built, traversed, measured and dropped on a thread with only 256 KiB of stack.
// Mistakes: assuming "O(h) space" is small (h can be n); deriving Clone/Debug/PartialEq (all recursive) on a deep tree type.
// ---------------------------------------------------------------------------------------------------------------------
fn q24() {
    let h = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut tr: T = None;
            for i in 0..100_000 {
                tr = node(i, tr, None);
            }
            let depth = level_order(&tr).len();
            let first = inorder_iter(&tr)[0];
            let sum: i64 = preorder_iter(&tr).iter().map(|&x| x as i64).sum();
            invert_iter(&mut tr); // spine now leans right
            let after = inorder_iter(&tr);
            drop(tr); // iterative Drop: no overflow
            (depth, first, sum, after.len())
        })
        .unwrap();
    let (depth, first, sum, n) = h
        .join()
        .expect("iterative code must survive a 100k-deep tree");
    assert_eq!((depth, first, n), (100_000, 0, 100_000));
    assert_eq!(sum, (0..100_000i64).sum::<i64>());
}

fn main() {
    q1();
    println!("Q1: ok");
    q2();
    println!("Q2: ok");
    q3();
    println!("Q3: ok");
    q4();
    println!("Q4: ok");
    q5();
    println!("Q5: ok");
    q6();
    println!("Q6: ok");
    q7();
    println!("Q7: ok");
    q8();
    println!("Q8: ok");
    q9();
    println!("Q9: ok");
    q10();
    println!("Q10: ok");
    q11();
    println!("Q11: ok");
    q12();
    println!("Q12: ok");
    q13();
    println!("Q13: ok");
    q14();
    println!("Q14: ok");
    q15();
    println!("Q15: ok");
    q16();
    println!("Q16: ok");
    q17();
    println!("Q17: ok");
    q18();
    println!("Q18: ok");
    q19();
    println!("Q19: ok");
    q20();
    println!("Q20: ok");
    q21();
    println!("Q21: ok");
    q22();
    println!("Q22: ok");
    q23();
    println!("Q23: ok");
    q24();
    println!("Q24: ok");
}
