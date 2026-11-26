//! RECIPE 1042 (std-iterators): iterator sources, writing your own iterators, and iterator-returning APIs.
//! LEVEL: advanced
//! Use in: library APIs that return sequences; parsers; pagination; tree walks.   Run: cargo run --release --example std_iterator_sources_custom
//! COMMON USES: paginated API clients, tokenisers, tree/graph traversals without recursion, extension traits (itertools-style), merging sorted streams, sequences (fib/collatz).
//! METHODS: std::iter::once, std::iter::once_with, std::iter::repeat, std::iter::repeat_n, std::iter::repeat_with, std::iter::empty,
//! std::iter::from_fn, std::iter::successors, std::iter::zip, Iterator::next, Iterator::size_hint, DoubleEndedIterator::next_back,
//! ExactSizeIterator::len, std::iter::FusedIterator, IntoIterator::into_iter, Iterator::by_ref, Iterator::peekable, Iterator::collect,
//! std::ops::Range, std::ops::RangeInclusive, Box<dyn Iterator>
//! DEFINED IN: core: the source functions and iterator traits are in `core::iter`, ranges in `core::ops`. `Box<dyn Iterator>` needs alloc.
//! DOCS: https://doc.rust-lang.org/std/iter/index.html
//! NOT COVERED (and why): std::iter::Step (unstable to implement), TrustedLen (unstable), Iterator::advance_by / array_chunks / intersperse (nightly).
//!
//! Cheat sheet (I want to ... -> use ...):
//!   one value as an iterator               -> `iter::once(x)`; lazily computed: `once_with(|| ..)`
//!   n copies / forever                     -> `repeat_n(x, n)` / `repeat(x)`; fresh value each time: `repeat_with(f)`
//!   nothing                                -> `iter::empty()`
//!   stateful generator, closure style      -> `from_fn(|| ..)`; next from previous: `successors(first, |prev| ..)`
//!   a range of numbers/chars               -> `0..n`, `a..=b`, `'a'..='z'`, `.rev()`, `.step_by(k)`
//!   make my struct iterable                -> `impl Iterator for It` (+ `IntoIterator` for T, &T, &mut T)
//!   iterate a field of &self               -> `fn iter(&self) -> impl Iterator<Item=&T> + '_`
//!   return different iterators             -> `Box<dyn Iterator<Item = T> + '_>` (or `Either`)
//!   add `.my_method()` to every iterator   -> extension trait with a blanket `impl<I: Iterator> Ext for I {}`
//!   hand out &mut windows                  -> NOT possible with Iterator (lending problem): use indices or `chunks_mut`
//!   tell collect how many items            -> implement `size_hint` honestly (or `ExactSizeIterator`)
//!   promise None stays None                -> `impl FusedIterator`
//!
//! TRY NEXT:
//!   - [S] Make `Fib` overflow-safe: stop with None when checked_add fails; assert the 94th value is the last u64 Fibonacci.
//!   - [S] What does `size_hint` return for `Pages` if you forget to implement it? Predict, then check `collect` capacity.
//!   - [M] Implement `DoubleEndedIterator` for `Countdown` and test `.rev()` against `Vec::reverse` on 300 random sizes.
//!   - [M] Add an `iter_mut` to `Tree` (explicit stack of `&mut` nodes) and double every key in place.
//!   - [A] PROJECT: combine with `std_btree_binaryheap_ordered` (heap) and `std_iterator_adapters_lazy`: build a k-way merge of N sorted iterators with a BinaryHeap of (Reverse(item), index) and compare with `merge_sorted` here.
//! Read: std::iter docs https://doc.rust-lang.org/std/iter/index.html ; Rust API Guidelines (C-ITER, C-ITER-TY) https://rust-lang.github.io/api-guidelines/naming.html
#![allow(dead_code)]

use std::collections::VecDeque;
use std::iter::FusedIterator;

const COVERED: &[&str] = &[
    "once",
    "once_with",
    "repeat",
    "repeat_n",
    "repeat_with",
    "empty",
    "from_fn",
    "successors",
    "zip",
    "Range",
    "RangeInclusive",
    "Iterator for own type",
    "DoubleEndedIterator",
    "ExactSizeIterator",
    "FusedIterator",
    "IntoIterator owned",
    "IntoIterator &",
    "IntoIterator &mut",
    "impl Iterator + '_",
    "Box<dyn Iterator>",
    "generic adaptor struct",
    "extension trait",
    "lending windows_mut",
    "size_hint",
    "tree inorder",
    "merge sorted",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ------------------------------------------------------------ 1. Everyday: std sources
fn s01_std_sources() {
    use std::iter::*;
    assert_eq!(once(5).chain(once(6)).collect::<Vec<_>>(), [5, 6]);
    // once_with: the value is only computed if the iterator is actually polled
    let mut calls = 0;
    let mut it = once_with(|| {
        calls += 1;
        42
    });
    assert_eq!(it.next(), Some(42));
    assert_eq!(it.next(), None);
    drop(it);
    assert_eq!(calls, 1);
    assert_eq!(repeat('=').take(4).collect::<String>(), "====");
    assert_eq!(repeat_n("ab", 3).collect::<String>(), "ababab"); // since 1.82, ExactSize, clearer than repeat+take
    assert_eq!(repeat_n(0u8, 5).len(), 5);
    // repeat_with: fresh value per item (e.g. fresh Vec, counter, random)
    let mut n = 0;
    let v: Vec<i32> = repeat_with(|| {
        n += 2;
        n
    })
    .take(3)
    .collect();
    assert_eq!(v, [2, 4, 6]);
    let grid: Vec<Vec<u8>> = repeat_with(|| vec![0; 2]).take(2).collect(); // `vec![vec![0;2];2]` clones; repeat_with builds
    assert_eq!(grid, [[0, 0], [0, 0]]);
    assert_eq!(empty::<u32>().count(), 0);
    // from_fn: closure-state generator
    let mut state = (0u32, 1u32);
    let fibs: Vec<u32> = from_fn(|| {
        let r = state.0;
        state = (state.1, state.0 + state.1);
        Some(r)
    })
    .take(8)
    .collect();
    assert_eq!(fibs, [0, 1, 1, 2, 3, 5, 8, 13]);
    // successors: next derived from previous; None ends it. Collatz:
    let collatz: Vec<u64> = successors(Some(6u64), |&n| {
        if n == 1 {
            None
        } else {
            Some(if n % 2 == 0 { n / 2 } else { 3 * n + 1 })
        }
    })
    .collect();
    assert_eq!(collatz, [6, 3, 10, 5, 16, 8, 4, 2, 1]);
    // powers of two until overflow: successors + checked op is the idiom
    let pows = successors(Some(1u8), |&x| x.checked_mul(2)).count();
    assert_eq!(pows, 8);
    // ranges
    assert_eq!((1..=3).rev().collect::<Vec<_>>(), [3, 2, 1]);
    assert_eq!(('a'..='e').step_by(2).collect::<String>(), "ace");
    assert_eq!((5..5).count(), 0);
    assert_eq!((0..=u8::MAX).count(), 256); // inclusive range reaches MAX without overflow
    assert_eq!(zip(1..4, "xyz".chars()).last(), Some((3, 'z')));
    println!("1. std sources ok");
}

// ------------------------------------------------------------ 2. Custom iterator, double-ended, exact-size, fused
struct Countdown {
    lo: u32,
    hi: u32,
} // yields hi-1 .. lo? no: yields lo..hi ascending, back yields descending
impl Iterator for Countdown {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.lo < self.hi {
            self.lo += 1;
            Some(self.lo - 1)
        } else {
            None
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = (self.hi - self.lo) as usize;
        (n, Some(n)) // exact: collect pre-allocates; ExactSizeIterator::len is now valid
    }
}
impl DoubleEndedIterator for Countdown {
    fn next_back(&mut self) -> Option<u32> {
        if self.lo < self.hi {
            self.hi -= 1;
            Some(self.hi)
        } else {
            None
        }
    }
}
impl ExactSizeIterator for Countdown {}
impl FusedIterator for Countdown {} // our next() keeps returning None once lo >= hi

fn s02_custom_iterator() {
    let c = Countdown { lo: 0, hi: 5 };
    assert_eq!(c.len(), 5);
    assert_eq!(c.rev().collect::<Vec<_>>(), [4, 3, 2, 1, 0]);
    // from both ends, meeting in the middle
    let mut c = Countdown { lo: 0, hi: 4 };
    assert_eq!(
        (c.next(), c.next_back(), c.next(), c.next_back(), c.next()),
        (Some(0), Some(3), Some(1), Some(2), None)
    );
    // we get every adapter for free by implementing only `next`
    let s: u32 = Countdown { lo: 1, hi: 5 }.map(|x| x * x).sum();
    assert_eq!(s, 1 + 4 + 9 + 16);
    // honest size_hint => collect allocates once
    let v: Vec<u32> = Countdown { lo: 0, hi: 1000 }.collect();
    assert_eq!(v.capacity(), 1000);
    println!("2. custom iterator ok");
}

// ------------------------------------------------------------ 3. IntoIterator for owned, &, &mut
struct Stack<T> {
    items: Vec<T>,
}
impl<T> IntoIterator for Stack<T> {
    type Item = T;
    type IntoIter = std::iter::Rev<std::vec::IntoIter<T>>; // pop order: top first
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter().rev()
    }
}
impl<'a, T> IntoIterator for &'a Stack<T> {
    type Item = &'a T;
    type IntoIter = std::iter::Rev<std::slice::Iter<'a, T>>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter().rev()
    }
}
impl<'a, T> IntoIterator for &'a mut Stack<T> {
    type Item = &'a mut T;
    type IntoIter = std::iter::Rev<std::slice::IterMut<'a, T>>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut().rev()
    }
}
impl<T> FromIterator<T> for Stack<T> {
    fn from_iter<I: IntoIterator<Item = T>>(i: I) -> Self {
        Stack {
            items: i.into_iter().collect(),
        }
    }
}
impl<T> Extend<T> for Stack<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, i: I) {
        self.items.extend(i)
    }
}

fn s03_into_iterator() {
    let mut s: Stack<i32> = (1..=3).collect();
    s.extend([4]);
    let by_ref: Vec<_> = (&s).into_iter().copied().collect(); // `for x in &s`
    assert_eq!(by_ref, [4, 3, 2, 1]);
    for x in &mut s {
        *x *= 10;
    } // `for x in &mut s`
    let mut total = 0;
    for x in &s {
        total += x;
    }
    assert_eq!(total, 100);
    let owned: Vec<i32> = s.into_iter().collect(); // consumes
    assert_eq!(owned, [40, 30, 20, 10]);
    // WHY three impls: `for x in &s` and `for x in s` are different operations; std's Vec does the same.
    println!("3. IntoIterator ok");
}

// ------------------------------------------------------------ 4. Returning iterators: impl Iterator vs Box<dyn>
struct Graph {
    adj: Vec<Vec<usize>>,
}
impl Graph {
    // impl Iterator + '_ : zero allocation, static dispatch, the default choice
    fn neighbours(&self, n: usize) -> impl Iterator<Item = usize> + '_ {
        self.adj[n].iter().copied()
    }
    fn edges(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.adj
            .iter()
            .enumerate()
            .flat_map(|(a, ns)| ns.iter().map(move |&b| (a, b)))
    }
    // Box<dyn Iterator>: needed when branches return DIFFERENT iterator types
    fn maybe_reversed(&self, n: usize, rev: bool) -> Box<dyn Iterator<Item = usize> + '_> {
        if rev {
            Box::new(self.adj[n].iter().rev().copied())
        } else {
            Box::new(self.adj[n].iter().copied())
        }
    }
}

fn s04_returning_iterators() {
    let g = Graph {
        adj: vec![vec![1, 2], vec![2], vec![]],
    };
    assert_eq!(g.neighbours(0).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(g.edges().count(), 3);
    assert_eq!(g.maybe_reversed(0, true).collect::<Vec<_>>(), [2, 1]);
    assert_eq!(g.maybe_reversed(0, false).collect::<Vec<_>>(), [1, 2]);
    // Edition 2024: `impl Trait` in return position captures ALL in-scope lifetimes by default, so `+ '_` is often redundant,
    // but it remains accepted and documents intent.
    println!("4. returning iterators ok");
}

// ------------------------------------------------------------ 5. Generic adaptor struct + extension trait
struct Dedup<I: Iterator> {
    it: std::iter::Peekable<I>,
}
impl<I: Iterator> Iterator for Dedup<I>
where
    I::Item: PartialEq,
{
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        let x = self.it.next()?;
        while self.it.next_if(|y| *y == x).is_some() {}
        Some(x)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lo, hi) = self.it.size_hint();
        (lo.min(1), hi) // at least 1 if non-empty, at most all
    }
}

/// Extension trait: `iter.dedup_adjacent()` on EVERY iterator. Same trick as itertools / futures::StreamExt.
trait IterExt: Iterator + Sized {
    fn dedup_adjacent(self) -> Dedup<Self>
    where
        Self::Item: PartialEq,
    {
        Dedup {
            it: self.peekable(),
        }
    }
    /// Consume and return (min, max) in one pass.
    fn min_max(mut self) -> Option<(Self::Item, Self::Item)>
    where
        Self::Item: Ord + Clone,
    {
        let first = self.next()?;
        Some(self.fold((first.clone(), first), |(lo, hi), x| {
            (lo.min(x.clone()), hi.max(x))
        }))
    }
}
impl<I: Iterator> IterExt for I {} // blanket impl: all iterators get the methods

fn s05_adaptors_and_extension_traits() {
    assert_eq!(
        [1, 1, 2, 2, 2, 1]
            .into_iter()
            .dedup_adjacent()
            .collect::<Vec<_>>(),
        [1, 2, 1]
    );
    assert_eq!("aabccc".chars().dedup_adjacent().collect::<String>(), "abc");
    assert_eq!([3, 9, 1].into_iter().min_max(), Some((1, 9)));
    assert_eq!(Vec::<i32>::new().into_iter().min_max(), None);
    // differential test against Vec::dedup on seeded random data
    let mut rng = Rng(7);
    for _ in 0..300 {
        let v: Vec<u8> = (0..rng.below(30)).map(|_| rng.below(3) as u8).collect();
        let mut oracle = v.clone();
        oracle.dedup();
        assert_eq!(
            v.iter().dedup_adjacent().copied().collect::<Vec<_>>(),
            oracle
        );
    }
    println!("5. adaptors/extension ok");
}

// ------------------------------------------------------------ 6. Lending iterator problem + windows_mut workaround
/// `Iterator::next(&mut self) -> Option<Self::Item>`: Item cannot borrow from `self`, so an iterator
/// cannot hand out `&mut [T]` windows that overlap (each would alias the previous). That is why std has
/// `windows` (shared) and `chunks_mut` (non-overlapping) but no `windows_mut`.
/// Workaround: an inherent method that takes a callback, so the borrow ends when the callback returns.
fn for_each_window_mut<T>(s: &mut [T], n: usize, mut f: impl FnMut(&mut [T])) {
    assert!(n > 0);
    if s.len() < n {
        return;
    }
    for i in 0..=s.len() - n {
        f(&mut s[i..i + n]);
    }
}

fn s06_lending() {
    let mut v = [1u32, 2, 3, 4, 5];
    // prefix-sum-ish smoothing: each window's last element += first
    for_each_window_mut(&mut v, 2, |w| w[1] += w[0]);
    assert_eq!(v, [1, 3, 6, 10, 15]); // running totals (each window saw the PREVIOUS window's write)
    // Non-overlapping case is fine with the std iterator:
    let mut u = [1, 2, 3, 4];
    for c in u.chunks_mut(2) {
        c.swap(0, 1);
    }
    assert_eq!(u, [2, 1, 4, 3]);
    // Crates like `lending-iterator` / GATs model the real signature `fn next(&mut self) -> Option<Self::Item<'_>>`.
    println!("6. lending ok");
}

// ------------------------------------------------------------ 7. size_hint property test
/// Builds a random pipeline over 0..n and checks: lower <= actual len <= upper (if Some).
fn random_pipeline(rng: &mut Rng) -> (Box<dyn Iterator<Item = u32>>, String) {
    let n = rng.below(20) as u32;
    let mut it: Box<dyn Iterator<Item = u32>> = Box::new(0..n);
    let mut desc = format!("0..{n}");
    for _ in 0..rng.below(5) {
        let k = rng.below(7) as usize;
        match rng.below(11) {
            0 => {
                it = Box::new(it.map(|x| x + 1));
                desc += ".map"
            }
            1 => {
                it = Box::new(it.filter(|x| x % 2 == 0));
                desc += ".filter"
            }
            2 => {
                it = Box::new(it.take(k));
                desc += &format!(".take({k})")
            }
            3 => {
                it = Box::new(it.skip(k));
                desc += &format!(".skip({k})")
            }
            4 => {
                it = Box::new(it.chain(0..k as u32));
                desc += &format!(".chain(0..{k})")
            }
            5 => {
                it = Box::new(it.flat_map(|x| 0..x % 3));
                desc += ".flat_map"
            }
            6 => {
                it = Box::new(it.step_by(k.max(1)));
                desc += &format!(".step_by({k})")
            }
            7 => {
                it = Box::new(it.take_while(|x| *x < 15));
                desc += ".take_while"
            }
            8 => {
                it = Box::new(it.skip_while(|x| *x < 3));
                desc += ".skip_while"
            }
            9 => {
                it = Box::new(it.enumerate().map(|(i, x)| i as u32 + x));
                desc += ".enumerate"
            }
            _ => {
                it = Box::new(it.peekable());
                desc += ".peekable"
            }
        }
    }
    (it, desc)
}

fn s07_size_hint_property() {
    let mut rng = Rng(0xDEADBEEF);
    let mut tight = 0;
    for _ in 0..2000 {
        let (it, desc) = random_pipeline(&mut rng);
        let (lo, hi) = it.size_hint();
        let len = it.count();
        assert!(lo <= len, "{desc}: lower bound {lo} > actual {len}");
        if let Some(h) = hi {
            assert!(len <= h, "{desc}: upper bound {h} < actual {len}");
        }
        if hi == Some(lo) {
            tight += 1;
            assert_eq!(lo, len);
        }
    }
    println!("7. size_hint holds on 2000 pipelines ({tight} had exact hints)");
    // A BROKEN hint is a logic bug (not UB) but misleads collect and `ExactSizeIterator` users:
    struct Liar;
    impl Iterator for Liar {
        type Item = u8;
        fn next(&mut self) -> Option<u8> {
            None
        }
        fn size_hint(&self) -> (usize, Option<usize>) {
            (3, Some(3))
        } // lies
    }
    assert_eq!(Liar.collect::<Vec<_>>().len(), 0); // std survives, but capacity was reserved for nothing
}

// ------------------------------------------------------------ 8. Scenarios
// 8a. pagination over a fake API
struct FakeApi {
    total: usize,
    page_size: usize,
    calls: std::cell::Cell<usize>,
}
impl FakeApi {
    fn fetch(&self, cursor: usize) -> (Vec<usize>, Option<usize>) {
        self.calls.set(self.calls.get() + 1);
        let end = (cursor + self.page_size).min(self.total);
        ((cursor..end).collect(), (end < self.total).then_some(end))
    }
}
struct Pages<'a> {
    api: &'a FakeApi,
    cursor: Option<usize>,
    buf: VecDeque<usize>,
    first: bool,
}
impl<'a> Pages<'a> {
    fn new(api: &'a FakeApi) -> Self {
        Pages {
            api,
            cursor: Some(0),
            buf: VecDeque::new(),
            first: true,
        }
    }
}
impl Iterator for Pages<'_> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        loop {
            if let Some(x) = self.buf.pop_front() {
                return Some(x);
            }
            let c = self.cursor.take()?; // fetch lazily: only when the buffer is empty
            let (items, next) = self.api.fetch(c);
            self.buf.extend(items);
            self.cursor = next;
        }
    }
}

fn s08a_pagination() {
    let api = FakeApi {
        total: 25,
        page_size: 10,
        calls: Default::default(),
    };
    assert_eq!(Pages::new(&api).count(), 25);
    assert_eq!(api.calls.get(), 3);
    api.calls.set(0);
    // laziness pays: taking 5 items costs ONE request
    assert_eq!(
        Pages::new(&api).take(5).collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
    assert_eq!(api.calls.get(), 1);
    println!("8a. pagination ok");
}

// 8b. inorder traversal of a BST with an explicit stack (no recursion, no stack overflow on degenerate trees)
#[derive(Default)]
struct Node {
    key: i32,
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}
#[derive(Default)]
struct Tree {
    root: Option<Box<Node>>,
    len: usize,
}
impl Tree {
    fn insert(&mut self, key: i32) {
        let mut cur = &mut self.root;
        while let Some(n) = cur {
            if key == n.key {
                return;
            }
            cur = if key < n.key {
                &mut n.left
            } else {
                &mut n.right
            };
        }
        *cur = Some(Box::new(Node {
            key,
            ..Default::default()
        }));
        self.len += 1;
    }
    fn iter(&self) -> InOrder<'_> {
        let mut it = InOrder { stack: Vec::new() };
        it.push_left(self.root.as_deref());
        it
    }
}
struct InOrder<'a> {
    stack: Vec<&'a Node>,
}
impl<'a> InOrder<'a> {
    fn push_left(&mut self, mut n: Option<&'a Node>) {
        while let Some(node) = n {
            self.stack.push(node);
            n = node.left.as_deref();
        }
    }
}
impl<'a> Iterator for InOrder<'a> {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        let n = self.stack.pop()?;
        self.push_left(n.right.as_deref());
        Some(n.key)
    }
}

fn s08b_tree() {
    let mut rng = Rng(99);
    let mut t = Tree::default();
    let mut oracle = std::collections::BTreeSet::new();
    for _ in 0..500 {
        let k = rng.below(200) as i32;
        t.insert(k);
        oracle.insert(k);
    }
    assert_eq!(
        t.iter().collect::<Vec<_>>(),
        oracle.into_iter().collect::<Vec<_>>()
    );
    // degenerate (sorted) insertion: a recursive traversal could overflow the stack; ours uses a heap Vec.
    let mut chain = Tree::default();
    for k in 0..2000 {
        chain.insert(k);
    }
    assert_eq!(chain.iter().count(), 2000);
    // NOTE: dropping a very deep Box chain recurses; std's own LinkedList has a manual Drop for this reason.
    std::mem::forget(chain); // avoids relying on default recursive drop for the demo; leaking is safe
    println!("8b. tree ok");
}

// 8c. merge of sorted iterators
struct MergeSorted<A: Iterator, B: Iterator<Item = A::Item>> {
    a: std::iter::Peekable<A>,
    b: std::iter::Peekable<B>,
}
impl<A: Iterator, B: Iterator<Item = A::Item>> Iterator for MergeSorted<A, B>
where
    A::Item: Ord,
{
    type Item = A::Item;
    fn next(&mut self) -> Option<A::Item> {
        match (self.a.peek(), self.b.peek()) {
            (Some(x), Some(y)) => {
                if x <= y {
                    self.a.next()
                } else {
                    self.b.next()
                }
            } // `<=` keeps it stable (a first)
            (Some(_), None) => self.a.next(),
            (None, _) => self.b.next(),
        }
    }
}
fn merge_sorted<A: IntoIterator, B: IntoIterator<Item = A::Item>>(
    a: A,
    b: B,
) -> MergeSorted<A::IntoIter, B::IntoIter>
where
    A::Item: Ord,
{
    MergeSorted {
        a: a.into_iter().peekable(),
        b: b.into_iter().peekable(),
    }
}

fn s08c_merge() {
    let mut rng = Rng(5);
    for _ in 0..300 {
        let mut a: Vec<u16> = (0..rng.below(20)).map(|_| rng.below(50) as u16).collect();
        let mut b: Vec<u16> = (0..rng.below(20)).map(|_| rng.below(50) as u16).collect();
        a.sort();
        b.sort();
        let mut oracle = [a.clone(), b.clone()].concat();
        oracle.sort();
        assert_eq!(merge_sorted(a, b).collect::<Vec<_>>(), oracle);
    }
    println!("8c. merge ok");
}

// 8d. tokenizer iterator (struct holding &str, yields &str slices: zero copy)
struct Words<'a> {
    rest: &'a str,
}
impl<'a> Iterator for Words<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<&'a str> {
        let s = self.rest.trim_start();
        if s.is_empty() {
            return None;
        }
        let end = s.find(char::is_whitespace).unwrap_or(s.len());
        let (w, rest) = s.split_at(end);
        self.rest = rest;
        Some(w)
    }
}
fn s08d_words() {
    assert_eq!(
        Words {
            rest: "  a bb\t ccc\n"
        }
        .collect::<Vec<_>>(),
        ["a", "bb", "ccc"]
    );
    assert_eq!(Words { rest: "   " }.count(), 0);
    println!("8d. words ok");
}

// ------------------------------------------------------------ 9. Production tips and tricks
fn s09_production_tips() {
    // 1. Return `impl Iterator` and let callers choose to collect (clippy::needless_collect / avoid returning Vec just to iterate).
    fn evens(n: u32) -> impl Iterator<Item = u32> {
        (0..n).filter(|x| x % 2 == 0)
    }
    assert_eq!(evens(10).count(), 5);

    // 2. Implement `size_hint` when you know it; collect() uses it to allocate once (Countdown above asserted capacity == 1000).
    // 3. Implement `FusedIterator` when your next() is naturally fused: it unlocks optimisations and documents the promise.
    fn is_fused<I: FusedIterator>(_: &I) {}
    is_fused(&Countdown { lo: 0, hi: 1 });

    // 4. Provide all three IntoIterator impls on public collections (C-ITER of the API Guidelines): owned, &, &mut (section 3).
    // 5. Name your iterator types (`Iter`, `IntoIter`, `IterMut`) for public APIs so users can store them (C-ITER-TY); `impl Iterator` hides the name.
    // 6. `repeat_with(Vec::new).take(n)` beats `vec![Vec::new(); n]` when T is not Clone.
    struct NoClone(u8);
    let slots: Vec<Option<NoClone>> = std::iter::repeat_with(|| None).take(3).collect();
    assert_eq!(slots.len(), 3);

    // 7. Avoid recursion for tree/graph walks over untrusted shapes: explicit stack (section 8b) cannot overflow the call stack.
    // 8. `successors` with `checked_*` for sequences that must stop before overflow (section 1).
    // 9. Boxed iterators in a hot loop cost a vtable call per item: prefer `impl Iterator` or an enum of the concrete cases.
    let boxed: Box<dyn Iterator<Item = u32>> = Box::new(0..4);
    assert_eq!(boxed.sum::<u32>(), 6);

    // 10. Don't implement Iterator for a type that also needs `&mut` access to the thing it iterates while yielded items live:
    //     that is the lending problem (section 6); give it a callback API or index API instead.
    // 11. Test iterators by comparing against a simple oracle on seeded random data (sections 5, 8b, 8c do exactly this).
    // 12. Make fallible sources yield `Result<T, E>` items so `collect::<Result<_,_>>()` and `?` compose (see Pages: swap Item for Result<usize, ApiError>).
    struct Flaky(u8);
    impl Iterator for Flaky {
        type Item = Result<u8, String>;
        fn next(&mut self) -> Option<Self::Item> {
            self.0 += 1;
            match self.0 {
                1..=2 => Some(Ok(self.0)),
                3 => Some(Err("boom".into())),
                _ => None,
            }
        }
    }
    assert_eq!(
        Flaky(0).collect::<Result<Vec<_>, _>>(),
        Err("boom".to_string())
    );
    assert_eq!(
        Flaky(0).take(2).collect::<Result<Vec<_>, _>>(),
        Ok(vec![1, 2])
    ); // laziness: error never pulled
    println!("9. production tips ok");
}

fn main() {
    let mut s = COVERED.to_vec();
    s.sort();
    s.dedup();
    assert_eq!(s.len(), COVERED.len(), "COVERED has duplicates");

    s01_std_sources();
    s02_custom_iterator();
    s03_into_iterator();
    s04_returning_iterators();
    s05_adaptors_and_extension_traits();
    s06_lending();
    s07_size_hint_property();
    s08a_pagination();
    s08b_tree();
    s08c_merge();
    s08d_words();
    s09_production_tips();
    println!("all source/custom sections passed");
}

#[cfg(test)]
mod tests {
    #[test]
    fn covered_unique() {
        let mut v = super::COVERED.to_vec();
        v.sort();
        let n = v.len();
        v.dedup();
        assert_eq!(n, v.len());
    }
}
