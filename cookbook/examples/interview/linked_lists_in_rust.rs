//! RECIPE 610 (interview): linked-list interview questions in Rust (Box lists, Rc sharing, Weak back-pointers, arenas, iterative Drop).
//! LEVEL: intermediate
//! Use in: coding interviews (LeetCode-style `Option<Box<ListNode>>`), and any code that builds lists, stacks or deques by hand.   Run: cargo run --release --example linked_lists_in_rust
//! COMMON USES: reverse/merge/partition/rotate drills, cycle detection, LRU-style deques, undo stacks, intrusive-list discussions.
#![allow(dead_code)]
//! Read: https://www.techinterview.org/post/3233460609/linked-list-interview-patterns/ , https://rust-unofficial.github.io/too-many-lists/ ,
//! https://dev.to/seanchen1991/implementing-an-lru-cache-in-rust-33pp , https://interviewing.io/rust-interview-questions , https://www.techinterviewhandbook.org/grind75
//! See also (unsafe doubly linked list, not rewritten here): examples/unsafe_ffi/nonnull_linked_list.rs
//!
//! QUESTION BANK
//! Q1. Reverse a singly linked list, iteratively and recursively. [Easy] [asked: very common]
//! Q2. Reverse the nodes of a list k at a time (leave a short tail as is). [Hard] [asked: common]
//! Q3. Reverse the sublist between positions m and n (1-indexed) in one pass. [Medium] [asked: common]
//! Q4. Merge two sorted linked lists into one sorted list. [Easy] [asked: very common]
//! Q5. Merge k sorted linked lists. [Hard] [asked: very common]
//! Q6. Find the middle node of a list (second middle when even) in one pass. [Easy] [asked: very common]
//! Q7. Remove the n-th node from the end of a list. [Medium] [asked: very common]
//! Q8. Delete duplicates from a sorted list (keep one / remove all) and remove all nodes with value x. [Medium] [asked: common]
//! Q9. Partition a list around x, keeping relative order. [Medium] [asked: common]
//! Q10. Rotate a list to the right by k places. [Medium] [asked: common]
//! Q11. Is a linked list a palindrome? O(n) time, O(1) extra space. [Easy] [asked: very common]
//! Q12. Add two numbers stored as reversed-digit lists. [Medium] [asked: very common]
//! Q13. Group the odd-indexed nodes together followed by the even-indexed nodes. [Medium] [asked: occasional]
//! Q14. Find the node where two lists intersect (shared tail). [Easy] [asked: common]
//! Q15. Detect a cycle and find where it starts (Floyd). Why can't you build one with Box? [Medium] [asked: very common]
//! Q16. Deep-copy a list whose nodes have a random pointer. [Medium] [asked: very common]
//! Q17. Flatten a multilevel list (nodes with child lists). [Medium] [asked: common]
//! Q18. Why does dropping a 1,000,000-node Box list overflow the stack, and how do you fix it? [Medium] [asked: common]
//! Q19. Implement a generic singly linked stack with push/pop/peek/iter/iter_mut/into_iter. [Medium] [asked: very common]
//! Q20. Implement a doubly linked list safely with Rc<RefCell> and Weak. [Hard] [asked: common]
//! Q21. Implement a doubly linked list with indices (arena); when would you reach for unsafe NonNull? [Hard] [asked: common]

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::{Rc, Weak};

// ---------------------------------------------------------------------------------------------------------------------
// Shared plumbing: seeded xorshift, the LeetCode ListNode and from_vec/to_vec.
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
fn rand_vec(r: &mut Rng, max_len: usize, lo: i32, hi: i32) -> Vec<i32> {
    let n = r.range(max_len + 1);
    (0..n).map(|_| r.int(lo, hi)).collect()
}

/// The shape LeetCode gives you. Rust wrinkle: ownership is a TREE, so `next` is an owning `Option<Box<_>>` and every
/// rewiring is "take it out, change it, put it back" (`take()`), never "point at it".
struct ListNode {
    val: i32,
    next: Option<Box<ListNode>>,
}
type List = Option<Box<ListNode>>;

/// The compiler-generated Drop of this type is recursive (one stack frame per node). We write our own iterative Drop (see Q18),
/// which means we may never destructure a node by move (`let ListNode { next, .. } = *b` is error E0509): always use `take()`.
impl Drop for ListNode {
    fn drop(&mut self) {
        let mut cur = self.next.take();
        while let Some(mut n) = cur {
            cur = n.next.take(); // detach before `n` drops, so each drop is O(1) deep
        }
    }
}

fn from_vec(v: &[i32]) -> List {
    let mut head = None;
    for &x in v.iter().rev() {
        head = Some(Box::new(ListNode { val: x, next: head }));
    }
    head
}
fn to_vec(l: &List) -> Vec<i32> {
    let (mut out, mut cur) = (Vec::new(), l.as_deref());
    while let Some(n) = cur {
        out.push(n.val);
        cur = n.next.as_deref(); // as_deref(): &Option<Box<T>> -> Option<&T>, the read-only walk
    }
    out
}
fn count(l: &List) -> usize {
    let (mut c, mut cur) = (0, l.as_deref());
    while let Some(n) = cur {
        c += 1;
        cur = n.next.as_deref();
    }
    c
}
/// Cursor to the empty slot after the last node: the `&mut Option<Box<_>>` pattern used all over this file.
/// `while cur.is_some() { cur = &mut cur.as_mut().unwrap().next }` is the form the borrow checker accepts (a `while let Some(n) = cur`
/// version moves `cur` and then cannot be used after the loop).
fn tail_slot(list: &mut List) -> &mut List {
    let mut cur = list;
    while cur.is_some() {
        cur = &mut cur.as_mut().unwrap().next;
    }
    cur
}

// ---------------------------------------------------------------------------------------------------------------------
// Q1. Reverse a linked list.
// Approach A (iterative): walk once; for each node `take()` its next, point it at the reversed prefix, push it on. O(n) time,
// O(1) space. Approach B (recursive): same with an accumulator (tail-call shaped, but Rust does not guarantee TCO, so depth is O(n)
// and 1M nodes overflow; the iterative form is the one to write).
// Rust wrinkle: `head = n.next.take(); n.next = prev; prev = Some(n);` is the three-move dance: you may not read n.next after moving n.
// Interviewer is testing: pointer rewiring without losing the rest of the list; empty and one-node cases.
// Mistakes: assigning `n.next = prev` before saving the old next (the rest of the list is dropped); trying `&mut` to everything.
// Follow-ups: reverse in place on a doubly linked list (swap prev/next); reverse using a stack (O(n) space); print in reverse without reversing.
// ---------------------------------------------------------------------------------------------------------------------
fn reverse(mut head: List) -> List {
    let mut prev = None;
    while let Some(mut n) = head {
        head = n.next.take();
        n.next = prev;
        prev = Some(n);
    }
    prev
}
fn reverse_rec(head: List) -> List {
    fn go(cur: List, prev: List) -> List {
        match cur {
            None => prev,
            Some(mut n) => {
                let next = n.next.take();
                n.next = prev;
                go(next, Some(n))
            }
        }
    }
    go(head, None)
}
fn q1() {
    assert!(reverse(None).is_none());
    assert_eq!(to_vec(&reverse(from_vec(&[7]))), vec![7]);
    let mut r = Rng(0x9E3779B97F4A7C15);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 30, -50, 50);
        let mut o = v.clone();
        o.reverse();
        assert_eq!(to_vec(&reverse(from_vec(&v))), o);
        assert_eq!(to_vec(&reverse_rec(from_vec(&v))), o);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q2. Reverse nodes in k-group. 1->2->3->4->5, k=2 => 2->1->4->3->5 ; k=3 => 3->2->1->4->5. A final group shorter than k is untouched.
// Approach: check that k nodes remain (shared-borrow walk), reverse exactly k nodes with the Q1 loop, recurse on the rest, then append the
// recursion result to the tail of the reversed block. O(n) time, O(n/k) recursion (an iterative version splices with a cursor).
// Rust wrinkle: the "check first" pass uses `as_deref()` so no borrow is alive while we mutate; after reversing, the block's tail is the
// ORIGINAL head, which we already moved, so we find it with `tail_slot` (O(k)).
// Testing: off-by-one on the remaining count, k == 1, k > len. Mistake: reversing the short tail (spec says leave it).
// Follow-ups: do it in O(1) extra space and iteratively; alternate groups (reverse every other k).
// ---------------------------------------------------------------------------------------------------------------------
fn reverse_k_group(head: List, k: usize) -> List {
    if k <= 1 {
        return head;
    }
    let (mut seen, mut c) = (0, head.as_deref());
    while let Some(n) = c {
        seen += 1;
        if seen == k {
            break;
        }
        c = n.next.as_deref();
    }
    if seen < k {
        return head;
    }
    let mut rest = head;
    let mut block = None;
    for _ in 0..k {
        let mut n = rest.unwrap();
        rest = n.next.take();
        n.next = block;
        block = Some(n);
    }
    let tail = reverse_k_group(rest, k);
    *tail_slot(&mut block) = tail;
    block
}
fn q2() {
    assert_eq!(
        to_vec(&reverse_k_group(from_vec(&[1, 2, 3, 4, 5]), 2)),
        vec![2, 1, 4, 3, 5]
    );
    assert_eq!(
        to_vec(&reverse_k_group(from_vec(&[1, 2, 3, 4, 5]), 3)),
        vec![3, 2, 1, 4, 5]
    );
    assert!(reverse_k_group(None, 3).is_none());
    let mut r = Rng(0xA1B2C3D4E5F60789);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 25, -9, 9);
        let k = 1 + r.range(6);
        let mut o = Vec::new();
        for ch in v.chunks(k) {
            let mut c = ch.to_vec();
            if c.len() == k {
                c.reverse();
            }
            o.extend(c);
        }
        assert_eq!(to_vec(&reverse_k_group(from_vec(&v), k)), o);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q3. Reverse the sublist from position m to n (1 <= m <= n <= len) in ONE pass. 1->2->3->4->5, m=2, n=4 => 1->4->3->2->5.
// Approach: advance a `&mut Option<Box<_>>` cursor to the slot holding node m, `take()` everything from there, reverse the first n-m+1
// nodes, splice the untouched rest on behind the reversed block, and put the block back in the slot. O(n) time, O(1) space.
// Rust wrinkle: the cursor is a mutable borrow of `head`; we may only return `head` after the cursor's last use (NLL makes that legal).
// Mistakes: forgetting to reconnect the rest; m == 1 (no predecessor; a cursor into `head` itself handles it with no dummy node).
// Follow-ups: reverse a whole list in the same call (m=1, n=len); do it recursively; reverse using only O(1) pointer swaps on a doubly list.
// ---------------------------------------------------------------------------------------------------------------------
fn reverse_between(mut head: List, m: usize, n: usize) -> List {
    let mut cur = &mut head;
    for _ in 0..m - 1 {
        cur = &mut cur.as_mut().unwrap().next;
    }
    let mut seg = cur.take();
    let mut prev = None;
    for _ in 0..(n - m + 1) {
        let mut node = seg.unwrap();
        seg = node.next.take();
        node.next = prev;
        prev = Some(node);
    }
    *tail_slot(&mut prev) = seg;
    *cur = prev;
    head
}
fn q3() {
    assert_eq!(
        to_vec(&reverse_between(from_vec(&[1, 2, 3, 4, 5]), 2, 4)),
        vec![1, 4, 3, 2, 5]
    );
    assert_eq!(to_vec(&reverse_between(from_vec(&[5]), 1, 1)), vec![5]);
    let mut r = Rng(0x0F1E2D3C4B5A6978);
    for _ in 0..2000 {
        let mut v = rand_vec(&mut r, 20, -9, 9);
        if v.is_empty() {
            v.push(1);
        }
        let m = 1 + r.range(v.len());
        let n = m + r.range(v.len() - m + 1);
        let mut o = v.clone();
        o[m - 1..n].reverse();
        assert_eq!(to_vec(&reverse_between(from_vec(&v), m, n)), o);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q4. Merge two sorted lists into one sorted list (splice the existing nodes; do not allocate new ones). [1,2,4] + [1,3,4] => [1,1,2,3,4,4].
// Approach: a `tail: &mut Option<Box<_>>` always points at the empty slot where the next node goes. Pick the smaller head, detach it
// (`src.take()`, then restore `*src = node.next.take()`), store it in the slot, advance the slot. When one side runs out, move the other
// side in whole. O(n+m) time, O(1) space. (The recursive form is shorter but uses O(n) stack.)
// Rust wrinkle: the tail-cursor idiom replaces C's dummy head node; `*tail = Some(node); tail = &mut tail.as_mut().unwrap().next;`.
// Testing: <= vs < (stability), one empty list, all equal, negative values. Follow-ups: merge in descending order; merge k lists (Q5).
// ---------------------------------------------------------------------------------------------------------------------
fn merge_two(mut a: List, mut b: List) -> List {
    let mut head = None;
    let mut tail = &mut head;
    while a.is_some() && b.is_some() {
        let take_a = a.as_ref().unwrap().val <= b.as_ref().unwrap().val;
        let src = if take_a { &mut a } else { &mut b };
        let mut node = src.take().unwrap();
        *src = node.next.take();
        *tail = Some(node);
        tail = &mut tail.as_mut().unwrap().next;
    }
    *tail = if a.is_some() { a } else { b };
    head
}
fn q4() {
    assert_eq!(
        to_vec(&merge_two(from_vec(&[1, 2, 4]), from_vec(&[1, 3, 4]))),
        vec![1, 1, 2, 3, 4, 4]
    );
    assert!(merge_two(None, None).is_none());
    let mut r = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..2000 {
        let (mut x, mut y) = (rand_vec(&mut r, 15, -20, 20), rand_vec(&mut r, 15, -20, 20));
        x.sort();
        y.sort();
        let mut o = [x.clone(), y.clone()].concat();
        o.sort();
        assert_eq!(to_vec(&merge_two(from_vec(&x), from_vec(&y))), o);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q5. Merge k sorted lists (k up to 10^4, total n nodes). Approaches: (1) merge one by one: O(k*n). (2) min-heap of the k heads:
// O(n log k) time, O(k) space (needs a `BinaryHeap<Reverse<(i32, usize)>>` holding values + list index, nodes are taken from the lists).
// (3) divide and conquer (below): pair lists up and merge pairwise, log k rounds, each round touches n nodes: O(n log k), O(1) extra.
// Rust wrinkle: `Vec<Option<Box<ListNode>>>` is consumed with `into_iter`, moving whole lists into `merge_two`; no clones, no unsafe.
// The heap version cannot hold `&ListNode` refs while also mutating, which is why it stores the value + index instead.
// Testing: k = 0, lists that are empty, duplicates across lists. Follow-up: what if the lists are streams (iterators)? Heap of peekable.
// ---------------------------------------------------------------------------------------------------------------------
fn merge_k(mut lists: Vec<List>) -> List {
    if lists.is_empty() {
        return None;
    }
    while lists.len() > 1 {
        let mut next = Vec::with_capacity(lists.len() / 2 + 1);
        let mut it = lists.into_iter();
        while let Some(a) = it.next() {
            match it.next() {
                Some(b) => next.push(merge_two(a, b)),
                None => next.push(a),
            }
        }
        lists = next;
    }
    lists.pop().unwrap()
}
fn q5() {
    assert!(merge_k(vec![]).is_none());
    assert!(merge_k(vec![None, None]).is_none());
    let mut r = Rng(0xDEADBEEF12345);
    for _ in 0..2000 {
        let k = r.range(8);
        let mut all = Vec::new();
        let mut lists = Vec::new();
        for _ in 0..k {
            let mut v = rand_vec(&mut r, 8, -10, 10);
            v.sort();
            all.extend(v.clone());
            lists.push(from_vec(&v));
        }
        all.sort();
        assert_eq!(to_vec(&merge_k(lists)), all);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q6. Return the middle node's value of a list; for an even length return the SECOND middle. [1,2,3,4,5] -> 3 ; [1,2,3,4] -> 3.
// Approach: slow/fast pointers; fast moves 2 per step, slow 1; when fast runs out slow is in the middle. O(n) time, O(1) space, one pass.
// Rust wrinkle: both pointers are SHARED references (`Option<&ListNode>`, obtained with `as_deref()`), which may alias freely: two
// readers are fine. The problem only appears when one pointer must mutate (Q7), where we count first instead of using a lead pointer.
// Testing: length 1, 2 (second middle), odd/even. Mistake: `while fast.next != None && fast != None` order (null deref in C; here
// the `Option` forces you to handle it). Follow-ups: first middle for even lengths; split the list in two halves (needs `take()`).
// ---------------------------------------------------------------------------------------------------------------------
fn middle(head: &List) -> Option<i32> {
    let (mut slow, mut fast) = (head.as_deref(), head.as_deref());
    while let Some(f) = fast {
        match f.next.as_deref() {
            Some(f2) => {
                fast = f2.next.as_deref();
                slow = slow.unwrap().next.as_deref();
            }
            None => break,
        }
    }
    slow.map(|n| n.val)
}
fn q6() {
    assert_eq!(middle(&None), None);
    assert_eq!(middle(&from_vec(&[1, 2, 3, 4, 5])), Some(3));
    assert_eq!(middle(&from_vec(&[1, 2, 3, 4])), Some(3));
    let mut r = Rng(0x77);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 30, -9, 9);
        let want = if v.is_empty() {
            None
        } else {
            Some(v[v.len() / 2])
        };
        assert_eq!(middle(&from_vec(&v)), want);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q7. Remove the n-th node from the END (1 <= n <= len) and return the head. [1,2,3,4,5], n=2 => [1,2,3,5].
// Classic approach: two pointers n apart, one pass. In safe Rust the lead pointer would be a shared `&` into the very list the trailing
// `&mut` cursor is rewriting, which the borrow checker rejects. So: pass 1 counts (shared borrow ends), pass 2 walks a `&mut` cursor
// to the slot before the victim. Still O(n) time, O(1) space; "one pass" is only a constant-factor claim, say so.
// Removing the head is not a special case with a slot cursor. Mistakes: n == len (head removal), n > len (return unchanged).
// Follow-ups: do it in one pass with an index-based or unsafe lead pointer; remove the middle node.
// ---------------------------------------------------------------------------------------------------------------------
fn remove_nth_from_end(mut head: List, n: usize) -> List {
    let len = count(&head);
    if n == 0 || n > len {
        return head;
    }
    let mut cur = &mut head;
    for _ in 0..len - n {
        cur = &mut cur.as_mut().unwrap().next;
    }
    let victim = cur.take();
    *cur = victim.and_then(|mut v| v.next.take());
    head
}
fn q7() {
    assert_eq!(
        to_vec(&remove_nth_from_end(from_vec(&[1, 2, 3, 4, 5]), 2)),
        vec![1, 2, 3, 5]
    );
    assert!(remove_nth_from_end(from_vec(&[1]), 1).is_none());
    let mut r = Rng(0xABCDEF);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 20, -9, 9);
        if v.is_empty() {
            continue;
        }
        let n = 1 + r.range(v.len());
        let mut o = v.clone();
        o.remove(v.len() - n);
        assert_eq!(to_vec(&remove_nth_from_end(from_vec(&v), n)), o);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q8. (a) Sorted list: delete duplicates so each value appears once. (b) Delete ALL values that occur more than once. (c) Remove every
// node equal to x from an unsorted list.   (a) [1,1,2,3,3] => [1,2,3]   (b) => [2]   (c) [6,1,6,2], x=6 => [1,2]
// Approach: one pass with a tail slot (a, b) or a cursor that either unlinks the current node or steps over it (c). O(n) time, O(1) space.
// Rust wrinkle for (c): "unlink the node under the cursor" is `let nxt = cur.as_mut().unwrap().next.take(); *cur = nxt;`, because
// `match cur { Some(n) if .. => *cur = n.next.take() }` keeps `n` borrowed while assigning to `*cur`.
// Testing: duplicates at the head and the tail, all equal, none equal, negative values. Follow-up: unsorted list dedupe with a HashSet.
// ---------------------------------------------------------------------------------------------------------------------
fn dedup_sorted(head: List, keep_one: bool) -> List {
    let mut head = head;
    let mut out = None;
    let mut tail = &mut out;
    while let Some(mut n) = head {
        head = n.next.take();
        let mut dup = false;
        while head.as_ref().is_some_and(|h| h.val == n.val) {
            dup = true;
            let mut h = head.take().unwrap();
            head = h.next.take();
        }
        if keep_one || !dup {
            *tail = Some(n);
            tail = &mut tail.as_mut().unwrap().next;
        }
    }
    out
}
fn remove_elements(mut head: List, val: i32) -> List {
    let mut cur = &mut head;
    while cur.is_some() {
        if cur.as_ref().unwrap().val == val {
            let nxt = cur.as_mut().unwrap().next.take();
            *cur = nxt;
        } else {
            cur = &mut cur.as_mut().unwrap().next;
        }
    }
    head
}
fn q8() {
    assert_eq!(
        to_vec(&dedup_sorted(from_vec(&[1, 1, 2, 3, 3]), true)),
        vec![1, 2, 3]
    );
    assert_eq!(
        to_vec(&dedup_sorted(from_vec(&[1, 1, 2, 3, 3]), false)),
        vec![2]
    );
    let mut r = Rng(0x5151);
    for _ in 0..2000 {
        let mut v = rand_vec(&mut r, 20, -4, 4);
        v.sort();
        let mut once = v.clone();
        once.dedup();
        assert_eq!(to_vec(&dedup_sorted(from_vec(&v), true)), once);
        let uniq: Vec<i32> = v
            .iter()
            .copied()
            .filter(|x| v.iter().filter(|y| *y == x).count() == 1)
            .collect();
        assert_eq!(to_vec(&dedup_sorted(from_vec(&v), false)), uniq);
        let u = rand_vec(&mut r, 20, -3, 3);
        let x = r.int(-3, 3);
        let w: Vec<i32> = u.iter().copied().filter(|&e| e != x).collect();
        assert_eq!(to_vec(&remove_elements(from_vec(&u), x)), w);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q9. Partition a list around x: all nodes < x before all nodes >= x, original relative order kept. [1,4,3,2,5,2], x=3 => [1,2,2,4,3,5].
// Approach: two output lists with two tail slots (`lt`, `ge`); walk the input detaching each node into one of them; finally hang the
// second list off the first one's slot. O(n) time, O(1) space, nodes reused.
// Rust wrinkle: two live `&mut` slots into two DIFFERENT locals (`lo`, `hi`) are fine; `*lt = hi` at the end is legal because `ge`
// is never used again (NLL), so moving `hi` is allowed.  Mistake: forgetting to terminate (cycle) in C; here `take()` guarantees it.
// Follow-ups: three-way partition (< x, == x, > x); quicksort on a linked list; do it stably vs unstably.
// ---------------------------------------------------------------------------------------------------------------------
fn partition(head: List, x: i32) -> List {
    let (mut lo, mut hi) = (None, None);
    let mut lt = &mut lo;
    let mut ge = &mut hi;
    let mut cur = head;
    while let Some(mut n) = cur {
        cur = n.next.take();
        if n.val < x {
            *lt = Some(n);
            lt = &mut lt.as_mut().unwrap().next;
        } else {
            *ge = Some(n);
            ge = &mut ge.as_mut().unwrap().next;
        }
    }
    *lt = hi;
    lo
}
fn q9() {
    assert_eq!(
        to_vec(&partition(from_vec(&[1, 4, 3, 2, 5, 2]), 3)),
        vec![1, 2, 2, 4, 3, 5]
    );
    let mut r = Rng(0x999);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 20, -5, 5);
        let x = r.int(-6, 6);
        let mut o: Vec<i32> = v.iter().copied().filter(|&e| e < x).collect();
        o.extend(v.iter().copied().filter(|&e| e >= x));
        assert_eq!(to_vec(&partition(from_vec(&v), x)), o);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q10. Rotate a list to the right by k places (k may exceed the length). [1,2,3,4,5], k=2 => [4,5,1,2,3].
// Approach: len = count; k %= len; if 0 return; walk a slot cursor len-k steps, `take()` the suffix (it becomes the new head), append the
// old head list to the suffix's tail. O(n) time, O(1) space. Rust wrinkle: the cursor borrows `head`; after `cur.take()` we move `head`.
// Mistakes: k % len == 0 and len == 0 (division by zero!). Follow-ups: rotate left; rotate by making the list circular (needs Rc, Q15).
// ---------------------------------------------------------------------------------------------------------------------
fn rotate_right(mut head: List, k: usize) -> List {
    let len = count(&head);
    if len == 0 || k % len == 0 {
        return head;
    }
    let k = k % len;
    let mut cur = &mut head;
    for _ in 0..len - k {
        cur = &mut cur.as_mut().unwrap().next;
    }
    let mut suffix = cur.take();
    *tail_slot(&mut suffix) = head;
    suffix
}
fn q10() {
    assert_eq!(
        to_vec(&rotate_right(from_vec(&[1, 2, 3, 4, 5]), 2)),
        vec![4, 5, 1, 2, 3]
    );
    assert!(rotate_right(None, 5).is_none());
    let mut r = Rng(0x1010);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 15, -9, 9);
        let k = r.range(40);
        let mut o = v.clone();
        if !o.is_empty() {
            let kk = k % o.len();
            o.rotate_right(kk);
        }
        assert_eq!(to_vec(&rotate_right(from_vec(&v), k)), o);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q11. Is the list a palindrome? Constraint: O(n) time and O(1) extra space. [1,2,2,1] -> true, [1,2] -> false.
// Brute force: copy into a Vec and compare with its reverse (O(n) space; fine to mention first, then improve).
// Optimal: split after the first ceil(n/2) nodes, reverse the second half in place (Q1), compare pairwise, then RE-reverse and re-attach so
// the caller's list is unchanged (interviewers love this "restore" step). Rust wrinkle: take ownership with `head.take()`, work on
// the owned list, put it back with `*head = owned` (a `&mut List` parameter, so the list is restored for the caller).
// Mistakes: forgetting to restore; odd lengths (the middle stays in the first half, zip stops at the shorter side). Follow-up: recursion.
// ---------------------------------------------------------------------------------------------------------------------
fn is_palindrome(head: &mut List) -> bool {
    let n = count(head);
    if n < 2 {
        return true;
    }
    let mut owned = head.take();
    let mut cur = &mut owned;
    for _ in 0..(n + 1) / 2 {
        cur = &mut cur.as_mut().unwrap().next;
    }
    let second = reverse(cur.take());
    let mut ok = true;
    {
        let (mut a, mut b) = (owned.as_deref(), second.as_deref());
        while let (Some(x), Some(y)) = (a, b) {
            if x.val != y.val {
                ok = false;
                break;
            }
            a = x.next.as_deref();
            b = y.next.as_deref();
        }
    }
    let restored = reverse(second);
    *tail_slot(&mut owned) = restored;
    *head = owned;
    ok
}
fn q11() {
    assert!(is_palindrome(&mut from_vec(&[1, 2, 2, 1])));
    assert!(!is_palindrome(&mut from_vec(&[1, 2])));
    let mut r = Rng(0x1111);
    for _ in 0..2000 {
        let mut v = rand_vec(&mut r, 9, 0, 2);
        if r.range(2) == 0 {
            let mut m = v.clone();
            if r.range(2) == 0 {
                m.push(r.int(0, 2));
            }
            m.extend(v.iter().rev());
            v = m;
        }
        let want = v.iter().eq(v.iter().rev());
        let mut l = from_vec(&v);
        assert_eq!(is_palindrome(&mut l), want);
        assert_eq!(to_vec(&l), v, "list must be restored");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q12. Two non-negative numbers are stored as lists of digits in REVERSE order ([2,4,3] = 342). Return their sum in the same form.
// 342 + 465 = 807 => [7,0,8]. Lists may differ in length; there may be a final carry.
// Approach: walk both with shared refs, carry = sum / 10, append with a tail slot. O(max(n,m)) time, O(1) extra (output aside).
// Rust wrinkle: `Option<&ListNode>` cursors advance independently with `if let Some(n) = x { s += n.val; x = n.next.as_deref(); }`; the
// result is built with a tail slot. Mistakes: dropping the last carry; treating a missing node as an error rather than 0.
// Follow-ups: digits stored in FORWARD order (reverse first, or use a stack); numbers too large for u128 (that is the point of the question).
// ---------------------------------------------------------------------------------------------------------------------
fn add_two(a: &List, b: &List) -> List {
    let (mut x, mut y) = (a.as_deref(), b.as_deref());
    let mut carry = 0;
    let mut out = None;
    let mut tail = &mut out;
    while x.is_some() || y.is_some() || carry > 0 {
        let mut s = carry;
        if let Some(n) = x {
            s += n.val;
            x = n.next.as_deref();
        }
        if let Some(n) = y {
            s += n.val;
            y = n.next.as_deref();
        }
        carry = s / 10;
        *tail = Some(Box::new(ListNode {
            val: s % 10,
            next: None,
        }));
        tail = &mut tail.as_mut().unwrap().next;
    }
    out
}
fn q12() {
    assert_eq!(
        to_vec(&add_two(&from_vec(&[2, 4, 3]), &from_vec(&[5, 6, 4]))),
        vec![7, 0, 8]
    );
    assert_eq!(
        to_vec(&add_two(&from_vec(&[9, 9]), &from_vec(&[1]))),
        vec![0, 0, 1]
    );
    assert_eq!(to_vec(&add_two(&None, &None)), Vec::<i32>::new());
    let mut r = Rng(0x1212);
    let num = |d: &[i32]| d.iter().rev().fold(0u128, |a, &x| a * 10 + x as u128);
    for _ in 0..2000 {
        let (x, y) = (rand_vec(&mut r, 18, 0, 9), rand_vec(&mut r, 18, 0, 9));
        let sum = num(&x) + num(&y);
        let got = to_vec(&add_two(&from_vec(&x), &from_vec(&y)));
        assert_eq!(num(&got), sum);
        assert!(got.iter().all(|d| (0..=9).contains(d)));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q13. Group all nodes at odd INDICES (1st, 3rd, ...) first, then the even ones, keeping order inside each group, O(1) extra space.
// [1,2,3,4,5] => [1,3,5,2,4]. Approach: two tail slots, alternate by a parity counter, concatenate. O(n) time, O(1) space.
// Rust wrinkle: same two-slot trick as partition (Q9); the C solution's `odd.next = even.next` pointer juggling disappears.
// Mistake: confusing index parity with VALUE parity. Follow-up: group by value parity (that is Q9 with a predicate).
// ---------------------------------------------------------------------------------------------------------------------
fn odd_even(head: List) -> List {
    let (mut odd, mut even) = (None, None);
    let mut ot = &mut odd;
    let mut et = &mut even;
    let mut cur = head;
    let mut i = 0;
    while let Some(mut n) = cur {
        cur = n.next.take();
        if i % 2 == 0 {
            *ot = Some(n);
            ot = &mut ot.as_mut().unwrap().next;
        } else {
            *et = Some(n);
            et = &mut et.as_mut().unwrap().next;
        }
        i += 1;
    }
    *ot = even;
    odd
}
fn q13() {
    assert_eq!(
        to_vec(&odd_even(from_vec(&[1, 2, 3, 4, 5]))),
        vec![1, 3, 5, 2, 4]
    );
    let mut r = Rng(0x1313);
    for _ in 0..2000 {
        let v = rand_vec(&mut r, 20, -9, 9);
        let mut o: Vec<i32> = v.iter().copied().step_by(2).collect();
        o.extend(v.iter().copied().skip(1).step_by(2));
        assert_eq!(to_vec(&odd_even(from_vec(&v))), o);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q14. Two singly linked lists may share a common tail (same NODES, not equal values). Return the first shared node or None.
// Approach: walk pa over A then B, pb over B then A; both travel len(A)+len(B) so they meet at the intersection (or both reach None
// together). O(n+m) time, O(1) space. Brute force: compare every pair with pointer equality, O(n*m); a HashSet of node addresses, O(n) space.
// Rust wrinkle: `Box` is UNIQUE ownership, so two lists can never share a node: with Box this question cannot even be posed. The
// representation must be `Option<Rc<Node>>`, and "same node" is `Rc::ptr_eq`, not `==` on values. Cloning an Rc is a refcount bump.
// Mistakes: comparing `val`s; infinite loop when there is no intersection (the None == None exit handles it).
// Follow-ups: also find the intersection after unequal lengths by advancing the longer list first; detect intersection with a cycle.
// ---------------------------------------------------------------------------------------------------------------------
struct SNode {
    val: i32,
    next: Option<Rc<SNode>>,
}
type SList = Option<Rc<SNode>>;
fn shared_from(v: &[i32], tail: SList) -> SList {
    let mut head = tail;
    for &x in v.iter().rev() {
        head = Some(Rc::new(SNode { val: x, next: head }));
    }
    head
}
fn same(a: &SList, b: &SList) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}
fn intersection(a: &SList, b: &SList) -> SList {
    let (mut pa, mut pb) = (a.clone(), b.clone());
    while !same(&pa, &pb) {
        pa = match pa {
            Some(n) => n.next.clone(),
            None => b.clone(),
        };
        pb = match pb {
            Some(n) => n.next.clone(),
            None => a.clone(),
        };
    }
    pa
}
fn intersection_brute(a: &SList, b: &SList) -> SList {
    let mut x = a.clone();
    while let Some(nx) = x {
        let mut y = b.clone();
        while let Some(ny) = y {
            if Rc::ptr_eq(&nx, &ny) {
                return Some(nx);
            }
            y = ny.next.clone();
        }
        x = nx.next.clone();
    }
    None
}
fn q14() {
    let mut r = Rng(0x1414);
    for _ in 0..2000 {
        let common = shared_from(&rand_vec(&mut r, 6, 0, 3), None);
        let a = shared_from(&rand_vec(&mut r, 6, 0, 3), common.clone());
        let b = shared_from(&rand_vec(&mut r, 6, 0, 3), common.clone());
        let got = intersection(&a, &b);
        assert!(same(&got, &common), "meets at the first shared node");
        assert!(same(&got, &intersection_brute(&a, &b)));
        // equal values but DIFFERENT nodes never intersect
        let c = shared_from(&[1, 2, 3], None);
        let d = shared_from(&[1, 2, 3], None);
        assert!(intersection(&c, &d).is_none());
    }
    let t = shared_from(&[9], None);
    assert_eq!(Rc::strong_count(t.as_ref().unwrap()), 1);
    let (_a, _b) = (shared_from(&[1], t.clone()), shared_from(&[2], t.clone()));
    assert_eq!(
        Rc::strong_count(t.as_ref().unwrap()),
        3,
        "tail owned by t, a's node and b's node"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q15. Detect whether a list has a cycle and return the node where it begins (Floyd's tortoise and hare). O(n) time, O(1) space.
// Approach: slow moves 1, fast moves 2; if they meet there is a cycle. Then restart slow at head; moving both 1 step at a time they
// meet exactly at the cycle entry (distance argument: head->entry = k, meeting point is k steps before the entry going around).
// Brute force oracle: walk with a HashSet of visited nodes (O(n) space).
// WHY NOT BOX: `Box` is unique ownership, so a `Box` list is a TREE: no node can be reachable from itself, a cycle is impossible by
// construction, and the question is meaningless for LeetCode's Rust `ListNode`. (LeetCode's Rust version just does not offer cycle problems.)
// To express one you need shared ownership (`Rc<RefCell<..>>`, which LEAKS because the count never reaches zero, unless a back edge is
// `Weak`), or an ARENA: nodes in a `Vec` and `next: Option<usize>`. The arena is the clean answer; both are shown.
// Mistakes: moving fast without checking `fast.next` first; comparing values instead of identity (indices / `Rc::ptr_eq`).
// Follow-ups: cycle length; remove the cycle; happy-number and duplicate-number-in-array problems (same algorithm, Floyd on a function).
// ---------------------------------------------------------------------------------------------------------------------
fn cycle_entry(next: &[Option<usize>], head: Option<usize>) -> Option<usize> {
    let h = head?;
    let (mut slow, mut fast) = (h, h);
    loop {
        fast = next[fast]?;
        fast = next[fast]?;
        slow = next[slow]?;
        if slow == fast {
            break;
        }
    }
    slow = h;
    while slow != fast {
        slow = next[slow]?;
        fast = next[fast]?;
    }
    Some(slow)
}
fn cycle_entry_set(next: &[Option<usize>], head: Option<usize>) -> Option<usize> {
    let (mut seen, mut cur) = (HashSet::new(), head);
    while let Some(i) = cur {
        if !seen.insert(i) {
            return Some(i);
        }
        cur = next[i];
    }
    None
}
struct RcN {
    val: i32,
    next: RefCell<Option<Rc<RcN>>>,
}
fn floyd_rc(head: &Rc<RcN>) -> bool {
    let (mut slow, mut fast) = (head.clone(), head.clone());
    loop {
        let f1 = fast.next.borrow().clone();
        let Some(f1) = f1 else { return false };
        let f2 = f1.next.borrow().clone();
        let Some(f2) = f2 else { return false };
        fast = f2;
        let s = slow.next.borrow().clone().unwrap();
        slow = s;
        if Rc::ptr_eq(&slow, &fast) {
            return true;
        }
    }
}
fn q15() {
    let mut r = Rng(0x1515);
    for _ in 0..2000 {
        let n = r.range(12);
        let mut perm: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            perm.swap(i, r.range(i + 1));
        }
        let mut next = vec![None; n];
        for w in 0..n.saturating_sub(1) {
            next[perm[w]] = Some(perm[w + 1]);
        }
        let mut entry = None;
        if n > 0 && r.range(2) == 0 {
            let p = r.range(n);
            next[perm[n - 1]] = Some(perm[p]); // tail points back to position p
            entry = Some(perm[p]);
        }
        let head = perm.first().copied();
        assert_eq!(cycle_entry(&next, head), entry);
        assert_eq!(cycle_entry_set(&next, head), entry);
    }
    // The Rc<RefCell> shape: a 3-cycle exists, Floyd finds it, and it leaks until we break it.
    let mk = |v| {
        Rc::new(RcN {
            val: v,
            next: RefCell::new(None),
        })
    };
    let (a, b, c) = (mk(1), mk(2), mk(3));
    *a.next.borrow_mut() = Some(b.clone());
    *b.next.borrow_mut() = Some(c.clone());
    assert!(!floyd_rc(&a));
    *c.next.borrow_mut() = Some(a.clone());
    assert!(floyd_rc(&a));
    assert_eq!(
        Rc::strong_count(&a),
        2,
        "variable + c.next: dropping the variable alone would leak the ring"
    );
    *c.next.borrow_mut() = None; // break the cycle so everything frees
    assert_eq!(Rc::strong_count(&a), 1);
    assert_eq!(a.val + b.val + c.val, 6);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q16. Each node has `next` and a `random` pointer to any node (or null). Return a DEEP copy. O(n) time.
// Approaches: (1) HashMap old node -> new node, two passes, O(n) space. (2) interleave copies after each original, set randoms, split:
// O(1) space but needs aliasing mutation of two lists at once. Rust wrinkle: "pointer to any node" is aliasing, so the idiomatic model is
// an ARENA: `Vec<Node>` with `next/random: Option<usize>`; indices are Copy and a copy is `HashMap<old_index, new_index>` (here a Vec map).
// Copying "the nodes" of an arena is just `Vec::clone`, which makes the question trivial; the interview version is to rebuild in LIST order
// (so positions are compact and independent of the old arena). With `Rc<RefCell<Node>>` the map would key on `Rc::as_ptr`.
// Testing: random pointing to None / self / earlier / later nodes; copy is independent (mutate copy, original unchanged).
// Mistakes: copying the pointer instead of the pointee (shallow copy); forgetting random == None.
// ---------------------------------------------------------------------------------------------------------------------
#[derive(Clone)]
struct RNode {
    val: i32,
    next: Option<usize>,
    random: Option<usize>,
}
struct RList {
    nodes: Vec<RNode>,
    head: Option<usize>,
}
fn order(l: &RList) -> Vec<usize> {
    let (mut o, mut c) = (Vec::new(), l.head);
    while let Some(i) = c {
        o.push(i);
        c = l.nodes[i].next;
    }
    o
}
fn copy_random_list(l: &RList) -> RList {
    let ord = order(l);
    let map: HashMap<usize, usize> = ord
        .iter()
        .enumerate()
        .map(|(new, &old)| (old, new))
        .collect();
    let nodes = ord
        .iter()
        .enumerate()
        .map(|(new, &old)| RNode {
            val: l.nodes[old].val,
            next: if new + 1 < ord.len() {
                Some(new + 1)
            } else {
                None
            },
            random: l.nodes[old].random.map(|t| map[&t]),
        })
        .collect();
    RList {
        nodes,
        head: if ord.is_empty() { None } else { Some(0) },
    }
}
fn q16() {
    let mut r = Rng(0x1616);
    for _ in 0..2000 {
        let n = r.range(10);
        let mut perm: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            perm.swap(i, r.range(i + 1));
        }
        let mut nodes: Vec<RNode> = (0..n)
            .map(|_| RNode {
                val: r.int(-9, 9),
                next: None,
                random: None,
            })
            .collect();
        for w in 0..n.saturating_sub(1) {
            nodes[perm[w]].next = Some(perm[w + 1]);
        }
        for i in 0..n {
            nodes[i].random = if r.range(3) == 0 {
                None
            } else {
                Some(r.range(n))
            };
        }
        let l = RList {
            nodes,
            head: perm.first().copied(),
        };
        let mut c = copy_random_list(&l);
        let (po, pc) = (order(&l), order(&c));
        assert_eq!(po.len(), pc.len());
        let pos: HashMap<usize, usize> = po.iter().enumerate().map(|(p, &i)| (i, p)).collect();
        for (p, &i) in po.iter().enumerate() {
            let (o, n2) = (&l.nodes[i], &c.nodes[pc[p]]);
            assert_eq!(o.val, n2.val);
            assert_eq!(
                o.random.map(|t| pos[&t]),
                n2.random.map(|t| order_pos(&pc, t))
            );
        }
        if let Some(h) = c.head {
            c.nodes[h].val += 1000; // independence: the original is untouched
            assert_ne!(l.nodes[l.head.unwrap()].val, c.nodes[h].val);
        }
    }
}
fn order_pos(ord: &[usize], idx: usize) -> usize {
    ord.iter().position(|&i| i == idx).unwrap()
}

// ---------------------------------------------------------------------------------------------------------------------
// Q17. Flatten a multilevel list: each node may have a `child` list (which may have children); splice so the output visits
// a node, then its whole child list (recursively), then continues with `next`. 1-2-3 with 2.child = 7-8 => 1,2,7,8,3.
// Approach: explicit stack of unvisited `next` tails: visit a node, push its `next`, push its `child` last so it is popped first.
// O(n) time, O(depth) stack space (recursion would also be O(depth), but a hostile input can be 100k levels deep).
// Rust wrinkle: unlike Q15, `next` AND `child` are both owning `Box`es, so this structure is still a tree and needs no Rc: moving the
// boxes onto a `Vec<Box<_>>` stack is the borrow-checker-friendly way to "remember where to continue". We `take()` both links off.
// Testing: child at head/tail, nested children, empty. Follow-ups: the doubly-linked LeetCode 430 variant (also fix `prev`); unflatten.
// ---------------------------------------------------------------------------------------------------------------------
struct MNode {
    val: i32,
    next: Option<Box<MNode>>,
    child: Option<Box<MNode>>,
}
fn flatten(head: Option<Box<MNode>>) -> List {
    let mut out = None;
    let mut tail = &mut out;
    let mut stack: Vec<Box<MNode>> = head.into_iter().collect();
    while let Some(mut n) = stack.pop() {
        if let Some(nx) = n.next.take() {
            stack.push(nx);
        }
        if let Some(c) = n.child.take() {
            stack.push(c);
        }
        *tail = Some(Box::new(ListNode {
            val: n.val,
            next: None,
        }));
        tail = &mut tail.as_mut().unwrap().next;
    }
    out
}
fn build_multilevel(r: &mut Rng, depth: usize) -> Option<Box<MNode>> {
    let len = if depth > 3 { 0 } else { r.range(4) };
    let mut items = Vec::new();
    for _ in 0..len {
        let v = r.int(-9, 9);
        let child = if r.range(3) == 0 {
            build_multilevel(r, depth + 1)
        } else {
            None
        };
        items.push((v, child));
    }
    let mut head = None;
    for (v, child) in items.into_iter().rev() {
        head = Some(Box::new(MNode {
            val: v,
            next: head,
            child,
        }));
    }
    head
}
fn preorder_oracle(n: &Option<Box<MNode>>, out: &mut Vec<i32>) {
    let mut cur = n.as_deref();
    while let Some(x) = cur {
        out.push(x.val);
        preorder_oracle(&x.child, out);
        cur = x.next.as_deref();
    }
}
fn q17() {
    let child = Some(Box::new(MNode {
        val: 7,
        next: Some(Box::new(MNode {
            val: 8,
            next: None,
            child: None,
        })),
        child: None,
    }));
    let three = Some(Box::new(MNode {
        val: 3,
        next: None,
        child: None,
    }));
    let two = Some(Box::new(MNode {
        val: 2,
        next: three,
        child,
    }));
    let one = Some(Box::new(MNode {
        val: 1,
        next: two,
        child: None,
    }));
    assert_eq!(to_vec(&flatten(one)), vec![1, 2, 7, 8, 3]);
    let mut r = Rng(0x1717);
    for _ in 0..2000 {
        let h = build_multilevel(&mut r, 0);
        let mut expect = Vec::new();
        preorder_oracle(&h, &mut expect);
        assert_eq!(to_vec(&flatten(h)), expect);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Q18. `let list = from_vec(&(0..1_000_000)...)` then the list goes out of scope: the program SEGFAULTS (stack overflow). Why? Fix it.
// Cause: the compiler-generated drop glue for `Option<Box<ListNode>>` drops `next` INSIDE dropping the node: drop(node1) calls
// drop(node2) calls drop(node3) ... one stack frame per node (~ dozens of bytes each) => 1M frames > 8 MB main stack (2 MB for a spawned thread).
// It is not catchable (stack overflow aborts), so this recipe does not run the naive version; it PROVES THE FIX instead.
// Fix: implement `Drop` for the node (see the impl near the top): repeatedly `take()` the next pointer so every node is dropped with
// an empty `next` (O(1) depth). Total O(n) time, O(1) stack. Same rule applies to trees: a degenerate tree drops recursively (see
// binary_trees_in_rust); the cure there is an explicit Vec stack of boxes.
// Related recursion hazards: derived Clone / Debug / PartialEq on a long list are recursive too (so is `reverse_rec` above), and a naive
// recursive `to_vec`. Testing: we run a 1,000,000-node build + sum + drop on a thread with only a 256 KiB stack, which would not even fit
// 10,000 recursive drop frames.
// Mistake: `impl Drop` + destructuring move (`let ListNode { next, .. } = *boxed`) does not compile (E0509): use `take()`.
// Follow-ups: how does `std::collections::LinkedList` avoid this? (its Drop pops nodes in a loop); drop order of fields.
// ---------------------------------------------------------------------------------------------------------------------
fn q18() {
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let n = 1_000_000usize;
            let mut head: List = None;
            for i in (0..n).rev() {
                head = Some(Box::new(ListNode {
                    val: (i % 7) as i32,
                    next: head,
                }));
            }
            let (mut sum, mut cur) = (0u64, head.as_deref());
            while let Some(x) = cur {
                sum += x.val as u64;
                cur = x.next.as_deref();
            }
            drop(head); // 1,000,000 nodes freed with O(1) stack
            sum
        })
        .unwrap();
    let expect: u64 = (0..1_000_000u64).map(|i| i % 7).sum();
    assert_eq!(
        handle.join().expect("iterative drop must not overflow"),
        expect
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Q19. Implement a generic singly linked STACK (too-many-lists "second list"): push, pop, peek, peek_mut, iter, iter_mut, into_iter.
// O(1) push/pop/peek. Rust wrinkles, in one place:
//   * `type Link<T> = Option<Box<Node<T>>>`; `pop` is `self.head.take().map(|n| { self.head = n.next; n.elem })`.
//   * `peek` is `self.head.as_ref().map(|n| &n.elem)`; `peek_mut` uses `as_mut()`.
//   * `Iter<'a, T>` holds `Option<&'a Node<T>>` and advances with `as_deref()`.
//   * `IterMut<'a, T>` holds `Option<&'a mut Node<T>>`; it MUST `take()` it (a `&mut` cannot be copied) and advance with `as_deref_mut()`.
//   * `IntoIter<T>(Stack<T>)` just calls `pop`.   * `Drop` is iterative (Q18).
// Interviewer is testing: ownership vs borrowing in each accessor, lifetimes on iterators, `Option::take`, why `peek` returns `Option<&T>`.
// Mistakes: `self.head.map(..)` (moves out of a borrow: needs take()), recursive Drop. Follow-ups: queue with a tail pointer (needs unsafe or Rc);
// a persistent (shared-tail) list with Rc; why is a Vec-backed stack usually faster? (cache locality, no allocation per push).
// ---------------------------------------------------------------------------------------------------------------------
type Link<T> = Option<Box<Node<T>>>;
struct Node<T> {
    elem: T,
    next: Link<T>,
}
pub struct Stack<T> {
    head: Link<T>,
    len: usize,
}
impl<T> Stack<T> {
    pub fn new() -> Self {
        Stack { head: None, len: 0 }
    }
    pub fn push(&mut self, elem: T) {
        let n = Box::new(Node {
            elem,
            next: self.head.take(),
        });
        self.head = Some(n);
        self.len += 1;
    }
    pub fn pop(&mut self) -> Option<T> {
        self.head.take().map(|n| {
            self.head = n.next;
            self.len -= 1;
            n.elem
        })
    }
    pub fn peek(&self) -> Option<&T> {
        self.head.as_ref().map(|n| &n.elem)
    }
    pub fn peek_mut(&mut self) -> Option<&mut T> {
        self.head.as_mut().map(|n| &mut n.elem)
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            next: self.head.as_deref(),
        }
    }
    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        IterMut {
            next: self.head.as_deref_mut(),
        }
    }
}
impl<T> Drop for Stack<T> {
    fn drop(&mut self) {
        let mut cur = self.head.take();
        while let Some(mut n) = cur {
            cur = n.next.take();
        }
    }
}
pub struct Iter<'a, T> {
    next: Option<&'a Node<T>>,
}
impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        self.next.map(|n| {
            self.next = n.next.as_deref();
            &n.elem
        })
    }
}
pub struct IterMut<'a, T> {
    next: Option<&'a mut Node<T>>,
}
impl<'a, T> Iterator for IterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        self.next.take().map(|n| {
            self.next = n.next.as_deref_mut();
            &mut n.elem
        })
    }
}
pub struct IntoIter<T>(Stack<T>);
impl<T> Iterator for IntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.0.pop()
    }
}
impl<T> IntoIterator for Stack<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;
    fn into_iter(self) -> IntoIter<T> {
        IntoIter(self)
    }
}
fn q19() {
    let mut s: Stack<String> = Stack::new();
    assert!(s.pop().is_none() && s.peek().is_none());
    let mut model: Vec<String> = Vec::new();
    let mut r = Rng(0x1919);
    for step in 0..4000 {
        match r.range(5) {
            0 | 1 => {
                let v = format!("v{step}");
                s.push(v.clone());
                model.push(v);
            }
            2 => assert_eq!(s.pop(), model.pop()),
            3 => {
                assert_eq!(s.peek(), model.last());
                if let Some(t) = s.peek_mut() {
                    t.push('!');
                    model.last_mut().unwrap().push('!');
                }
            }
            _ => {
                for e in s.iter_mut() {
                    e.push('.');
                }
                for e in model.iter_mut() {
                    e.push('.');
                }
                assert!(s.iter().eq(model.iter().rev()));
            }
        }
        assert_eq!(s.len(), model.len());
    }
    let drained: Vec<String> = s.into_iter().collect();
    let want: Vec<String> = model.into_iter().rev().collect();
    assert_eq!(drained, want);
    let mut big = Stack::new();
    for i in 0..1_000_000u32 {
        big.push(i);
    }
    assert_eq!(big.iter().count(), 1_000_000);
}

// ---------------------------------------------------------------------------------------------------------------------
// Q20. Implement a DOUBLY linked deque safely: push/pop at both ends O(1). Each node needs a forward and a backward pointer, i.e. two
// owners per node, which `Box` cannot express. Standard safe answer: `next: Option<Rc<RefCell<Node>>>` (strong, owning) and
// `prev: Option<Weak<RefCell<Node>>>` (weak, non-owning), so the ring of pointers is NOT a ref-count cycle (no leak). Deque holds a strong
// `head` and `tail`. Costs: a refcount touch per hop, runtime borrow checks, and you cannot return `&T` out of a `RefCell` (peek returns a `Ref`
// or clones through a closure). Pop must prove it holds the LAST strong ref (`Rc::try_unwrap`), which is why we clear the neighbour's
// pointer to the node first. Drop: iterative via `pop_front` (Q18); Weak does not keep anything alive.
// Mistakes: both directions strong (leak: counts never reach 0); holding a `borrow_mut()` across a call that borrows again (runtime panic);
// forgetting the single-element case (head and tail are the same node). Follow-ups: LRU cache (see data_structures/lru_cache.rs, uses
// indices), why std's LinkedList uses `NonNull`, cursor API.
// ---------------------------------------------------------------------------------------------------------------------
type DLink<T> = Option<Rc<RefCell<DNode<T>>>>;
struct DNode<T> {
    elem: T,
    next: DLink<T>,
    prev: Option<Weak<RefCell<DNode<T>>>>,
}
struct RcDeque<T> {
    head: DLink<T>,
    tail: DLink<T>,
}
impl<T> RcDeque<T> {
    fn new() -> Self {
        RcDeque {
            head: None,
            tail: None,
        }
    }
    fn push_back(&mut self, elem: T) {
        let n = Rc::new(RefCell::new(DNode {
            elem,
            next: None,
            prev: None,
        }));
        match self.tail.take() {
            Some(old) => {
                n.borrow_mut().prev = Some(Rc::downgrade(&old));
                old.borrow_mut().next = Some(n.clone());
                self.tail = Some(n);
            }
            None => {
                self.head = Some(n.clone());
                self.tail = Some(n);
            }
        }
    }
    fn push_front(&mut self, elem: T) {
        let n = Rc::new(RefCell::new(DNode {
            elem,
            next: None,
            prev: None,
        }));
        match self.head.take() {
            Some(old) => {
                old.borrow_mut().prev = Some(Rc::downgrade(&n));
                n.borrow_mut().next = Some(old);
                self.head = Some(n);
            }
            None => {
                self.tail = Some(n.clone());
                self.head = Some(n);
            }
        }
    }
    fn pop_front(&mut self) -> Option<T> {
        self.head.take().map(|old| {
            let nx = old.borrow_mut().next.take();
            match nx {
                Some(nx) => {
                    nx.borrow_mut().prev = None;
                    self.head = Some(nx);
                }
                None => {
                    self.tail.take(); // the second strong ref to `old`
                }
            }
            Rc::try_unwrap(old)
                .ok()
                .expect("last strong ref")
                .into_inner()
                .elem
        })
    }
    fn pop_back(&mut self) -> Option<T> {
        self.tail.take().map(|old| {
            let pv = old.borrow_mut().prev.take().and_then(|w| w.upgrade());
            match pv {
                Some(pv) => {
                    pv.borrow_mut().next.take(); // drops the strong ref to `old`
                    self.tail = Some(pv);
                }
                None => {
                    self.head.take();
                }
            }
            Rc::try_unwrap(old)
                .ok()
                .expect("last strong ref")
                .into_inner()
                .elem
        })
    }
    fn peek_front<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        self.head.as_ref().map(|n| f(&n.borrow().elem))
    }
    fn forward(&self) -> Vec<T>
    where
        T: Clone,
    {
        let (mut out, mut cur) = (Vec::new(), self.head.clone());
        while let Some(n) = cur {
            out.push(n.borrow().elem.clone());
            cur = n.borrow().next.clone();
        }
        out
    }
    fn backward(&self) -> Vec<T>
    where
        T: Clone,
    {
        let (mut out, mut cur) = (Vec::new(), self.tail.clone());
        while let Some(n) = cur {
            out.push(n.borrow().elem.clone());
            cur = n.borrow().prev.as_ref().and_then(|w| w.upgrade());
        }
        out
    }
}
impl<T> Drop for RcDeque<T> {
    fn drop(&mut self) {
        while self.pop_front().is_some() {}
    }
}
fn q20() {
    let mut d = RcDeque::new();
    let mut model: VecDeque<i32> = VecDeque::new();
    let mut r = Rng(0x2020);
    for step in 0..6000 {
        let v = step as i32;
        match r.range(4) {
            0 => {
                d.push_back(v);
                model.push_back(v);
            }
            1 => {
                d.push_front(v);
                model.push_front(v);
            }
            2 => assert_eq!(d.pop_front(), model.pop_front()),
            _ => assert_eq!(d.pop_back(), model.pop_back()),
        }
        assert_eq!(d.peek_front(|x| *x), model.front().copied());
        if step % 97 == 0 {
            assert_eq!(d.forward(), model.iter().copied().collect::<Vec<_>>());
            assert_eq!(
                d.backward(),
                model.iter().rev().copied().collect::<Vec<_>>()
            );
        }
    }
    // Weak does not own: after the deque drops, an observer's Weak is dead (no leak, no cycle).
    let mut d2 = RcDeque::new();
    d2.push_back(1);
    d2.push_back(2);
    let w = Rc::downgrade(d2.head.as_ref().unwrap());
    assert_eq!(
        Rc::weak_count(d2.head.as_ref().unwrap()),
        2,
        "our observer + node 2's prev pointer"
    );
    drop(d2);
    assert!(w.upgrade().is_none());
    let mut big = RcDeque::new();
    for i in 0..200_000 {
        big.push_back(i);
    }
    drop(big); // iterative Drop: no stack overflow
}

// ---------------------------------------------------------------------------------------------------------------------
// Q21. The same deque with INDICES (an arena), supporting O(1) removal of an arbitrary element by HANDLE (the LRU-cache building block).
// Nodes live in a `Vec<Slot<T>>`; `prev/next` are `usize` (NIL = usize::MAX); freed slots go on a free list and are reused.
// No Rc, no RefCell, no unsafe, no per-node allocation; it is faster than Q20 and trivially `Send`. Costs: handles can go stale after
// removal+reuse (production code adds a generation counter), and the arena only grows to the high-water mark.
// WHEN UNSAFE: `std::collections::LinkedList` and many crates use `Option<NonNull<Node<T>>>` with `Box::into_raw`/`from_raw`
// because they need a stable node address, no handle indirection and `Iterator`/cursor APIs returning `&T`. That version needs
// invariants (head/tail consistency, unique ownership, PhantomData for variance and dropck); it is worked through in
// examples/unsafe_ffi/nonnull_linked_list.rs and deliberately not rewritten here. `Option<NonNull<_>>` and `Option<Box<_>>` are both
// pointer-sized thanks to the null-pointer niche; checked below.
// Mistakes: unlinking without fixing head/tail; reading a removed slot. Follow-ups: add a generation to handles; implement LRU on top.
// ---------------------------------------------------------------------------------------------------------------------
const NIL: usize = usize::MAX;
struct Slot<T> {
    elem: Option<T>,
    prev: usize,
    next: usize,
}
struct IdxDeque<T> {
    slots: Vec<Slot<T>>,
    free: Vec<usize>,
    head: usize,
    tail: usize,
    len: usize,
}
impl<T> IdxDeque<T> {
    fn new() -> Self {
        IdxDeque {
            slots: Vec::new(),
            free: Vec::new(),
            head: NIL,
            tail: NIL,
            len: 0,
        }
    }
    fn alloc(&mut self, elem: T) -> usize {
        let s = Slot {
            elem: Some(elem),
            prev: NIL,
            next: NIL,
        };
        match self.free.pop() {
            Some(i) => {
                self.slots[i] = s;
                i
            }
            None => {
                self.slots.push(s);
                self.slots.len() - 1
            }
        }
    }
    fn push_back(&mut self, elem: T) -> usize {
        let i = self.alloc(elem);
        self.slots[i].prev = self.tail;
        if self.tail != NIL {
            self.slots[self.tail].next = i
        } else {
            self.head = i
        }
        self.tail = i;
        self.len += 1;
        i
    }
    fn push_front(&mut self, elem: T) -> usize {
        let i = self.alloc(elem);
        self.slots[i].next = self.head;
        if self.head != NIL {
            self.slots[self.head].prev = i
        } else {
            self.tail = i
        }
        self.head = i;
        self.len += 1;
        i
    }
    /// O(1) removal of any live handle.
    fn remove(&mut self, i: usize) -> Option<T> {
        let elem = self.slots[i].elem.take()?;
        let (p, n) = (self.slots[i].prev, self.slots[i].next);
        if p != NIL {
            self.slots[p].next = n
        } else {
            self.head = n
        }
        if n != NIL {
            self.slots[n].prev = p
        } else {
            self.tail = p
        }
        self.free.push(i);
        self.len -= 1;
        Some(elem)
    }
    fn pop_front(&mut self) -> Option<T> {
        if self.head == NIL {
            None
        } else {
            self.remove(self.head)
        }
    }
    fn pop_back(&mut self) -> Option<T> {
        if self.tail == NIL {
            None
        } else {
            self.remove(self.tail)
        }
    }
    fn iter(&self) -> impl Iterator<Item = &T> {
        let mut cur = self.head;
        std::iter::from_fn(move || {
            if cur == NIL {
                return None;
            }
            let s = &self.slots[cur];
            cur = s.next;
            s.elem.as_ref()
        })
    }
}
fn q21() {
    let mut d = IdxDeque::new();
    let mut model: VecDeque<(usize, i32)> = VecDeque::new(); // (handle, value)
    let mut r = Rng(0x2121);
    for step in 0..6000 {
        let v = step as i32;
        match r.range(5) {
            0 => model.push_back((d.push_back(v), v)),
            1 => model.push_front((d.push_front(v), v)),
            2 => assert_eq!(d.pop_front(), model.pop_front().map(|x| x.1)),
            3 => assert_eq!(d.pop_back(), model.pop_back().map(|x| x.1)),
            _ => {
                if !model.is_empty() {
                    let p = r.range(model.len());
                    let (h, val) = model.remove(p).unwrap();
                    assert_eq!(d.remove(h), Some(val));
                    assert_eq!(
                        d.remove(h),
                        None,
                        "second removal of the same handle is a no-op"
                    );
                }
            }
        }
        assert_eq!(d.len, model.len());
    }
    assert!(d.iter().copied().eq(model.iter().map(|x| x.1)));
    assert_eq!(
        std::mem::size_of::<Option<Box<ListNode>>>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::size_of::<Option<std::ptr::NonNull<ListNode>>>(),
        std::mem::size_of::<usize>()
    );
    let _unused: Cell<u8> = Cell::new(0);
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
}
