import textwrap

from author import T, write_track

MINIVEC = """
use std::alloc::{self, Layout};
use std::marker::PhantomData;
use std::ptr::{self, NonNull};

/// A growable array on a raw allocation. Zero-sized `T` isn't supported.
pub struct MiniVec<T> {
    ptr: NonNull<T>,
    cap: usize,
    len: usize,
    _owns: PhantomData<T>,
}

impl<T> MiniVec<T> {
    pub fn new() -> Self {
        assert!(std::mem::size_of::<T>() != 0, "zero-sized types aren't supported");
        MiniVec { ptr: NonNull::dangling(), cap: 0, len: 0, _owns: PhantomData }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn push(&mut self, value: T) {
        if self.len == self.cap {
            self.grow();
        }
        // SAFETY: len < cap, so the slot is inside the allocation and not yet initialised.
        unsafe { ptr::write(self.ptr.as_ptr().add(self.len), value) };
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: the slot was initialised and is now past len, so it's read exactly once.
        Some(unsafe { ptr::read(self.ptr.as_ptr().add(self.len)) })
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        // SAFETY: i < len, so the slot is initialised and inside the allocation.
        (i < self.len).then(|| unsafe { &*self.ptr.as_ptr().add(i) })
    }

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 { 4 } else { self.cap * 2 };
        let new_layout = Layout::array::<T>(new_cap).expect("capacity overflow");
        let raw = if self.cap == 0 {
            // SAFETY: new_layout has a non-zero size because T isn't zero-sized.
            unsafe { alloc::alloc(new_layout) }
        } else {
            let old_layout = Layout::array::<T>(self.cap).expect("existing layout");
            // SAFETY: ptr was allocated with old_layout by this allocator.
            unsafe { alloc::realloc(self.ptr.as_ptr().cast(), old_layout, new_layout.size()) }
        };
        self.ptr = NonNull::new(raw.cast()).unwrap_or_else(|| alloc::handle_alloc_error(new_layout));
        self.cap = new_cap;
    }
}

impl<T> Default for MiniVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for MiniVec<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
        if self.cap != 0 {
            // SAFETY: ptr was allocated with this layout and every element has been dropped.
            unsafe { alloc::dealloc(self.ptr.as_ptr().cast(), Layout::array::<T>(self.cap).expect("layout")) };
        }
    }
}
"""

GRID = """
        pub struct Grid {
            w: usize,
            h: usize,
            cells: Vec<u8>,
        }

        impl Grid {
            pub fn new(w: usize, h: usize) -> Self {
                Grid { w, h, cells: vec![0; w * h] }
            }

            fn index(&self, x: usize, y: usize) -> Option<usize> {
                (x < self.w && y < self.h).then(|| y * self.w + x)
            }

            pub fn get(&self, x: usize, y: usize) -> Option<u8> {
                self.index(x, y).map(|i| self.cells[i])
            }

            pub fn set(&mut self, x: usize, y: usize, value: u8) -> bool {
                match self.index(x, y) {
                    Some(i) => {
                        self.cells[i] = value;
                        true
                    }
                    None => false,
                }
            }

            pub fn row(&self, y: usize) -> Option<&[u8]> {
                (y < self.h).then(|| &self.cells[y * self.w..(y + 1) * self.w])
            }
        }
    """

SMALLVEC = """
        pub enum SmallVec4<T> {
            Inline { items: [Option<T>; 4], len: usize },
            Heap(Vec<T>),
        }

        impl<T> SmallVec4<T> {
            pub fn new() -> Self {
                SmallVec4::Inline { items: [None, None, None, None], len: 0 }
            }

            pub fn push(&mut self, value: T) {
                match self {
                    SmallVec4::Inline { items, len } if *len < 4 => {
                        items[*len] = Some(value);
                        *len += 1;
                    }
                    SmallVec4::Inline { items, .. } => {
                        let mut heap: Vec<T> = Vec::with_capacity(8);
                        heap.extend(items.iter_mut().filter_map(Option::take));
                        heap.push(value);
                        *self = SmallVec4::Heap(heap);
                    }
                    SmallVec4::Heap(heap) => heap.push(value),
                }
            }

            pub fn len(&self) -> usize {
                match self {
                    SmallVec4::Inline { len, .. } => *len,
                    SmallVec4::Heap(heap) => heap.len(),
                }
            }

            pub fn get(&self, i: usize) -> Option<&T> {
                match self {
                    SmallVec4::Inline { items, len } if i < *len => items[i].as_ref(),
                    SmallVec4::Inline { .. } => None,
                    SmallVec4::Heap(heap) => heap.get(i),
                }
            }

            pub fn is_inline(&self) -> bool {
                matches!(self, SmallVec4::Inline { .. })
            }
        }

        impl<T> Default for SmallVec4<T> {
            fn default() -> Self {
                Self::new()
            }
        }
    """

RING = """
        pub struct Ring<T> {
            buf: Box<[Option<T>]>,
            head: usize,
            len: usize,
        }

        impl<T> Ring<T> {
            /// Panics if `capacity` is 0.
            pub fn with_capacity(capacity: usize) -> Self {
                assert!(capacity > 0, "capacity must be at least 1");
                Ring {
                    buf: (0..capacity).map(|_| None).collect(),
                    head: 0,
                    len: 0,
                }
            }

            /// Adds `value`, returning the evicted oldest value if the ring was full.
            pub fn push(&mut self, value: T) -> Option<T> {
                let cap = self.buf.len();
                if self.len == cap {
                    let old = self.buf[self.head].replace(value);
                    self.head = (self.head + 1) % cap;
                    old
                } else {
                    self.buf[(self.head + self.len) % cap] = Some(value);
                    self.len += 1;
                    None
                }
            }

            pub fn len(&self) -> usize {
                self.len
            }

            pub fn is_empty(&self) -> bool {
                self.len == 0
            }

            pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
                let cap = self.buf.len();
                (0..self.len).map(move |i| self.buf[(self.head + i) % cap].as_ref().expect("live slot"))
            }
        }
    """

DEREF_IMPLS = """
impl<T> MiniVec<T> {
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: ptr is non-null and aligned (dangling is fine when len is 0), and the first len
        // elements are initialised. The slice borrows self, so it can't outlive the buffer.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: as above, and &mut self guarantees no other reference to the elements exists.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> std::ops::Deref for MiniVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T> std::ops::DerefMut for MiniVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}
"""

def sub(code, *edits):
    """`code`, dedented, with each (old, new) replacement made; every `old` must be there."""
    code = textwrap.dedent(code)
    for old, new in edits:
        assert old in code, old
        code = code.replace(old, new)
    return code


P = []

P.append(dict(
    slug="vec-ops", title="Push, pop, insert, remove", level="easy", stage="use-it", tags=["Vec", "match"],
    teaches=["The core `Vec` operations and which ones panic.", "Guard clauses in a `match` for out-of-range indices."],
    statement="""
        Apply `ops` to an empty `Vec<i32>` and return it. `Pop` on an empty vec does nothing.
        `Insert(i, x)` with `i > len` and `Remove(i)` with `i >= len` do nothing instead of panicking.
    """,
    starter="""
        #[derive(Debug, Clone, Copy)]
        pub enum Op {
            Push(i32),
            Pop,
            Insert(usize, i32),
            Remove(usize),
        }

        pub fn apply(ops: &[Op]) -> Vec<i32> {
            todo!()
        }
    """,
    solution="""
        #[derive(Debug, Clone, Copy)]
        pub enum Op {
            Push(i32),
            Pop,
            Insert(usize, i32),
            Remove(usize),
        }

        pub fn apply(ops: &[Op]) -> Vec<i32> {
            let mut v = Vec::new();
            for &op in ops {
                match op {
                    Op::Push(x) => v.push(x),
                    Op::Pop => {
                        v.pop();
                    }
                    Op::Insert(i, x) if i <= v.len() => v.insert(i, x),
                    Op::Remove(i) if i < v.len() => {
                        v.remove(i);
                    }
                    Op::Insert(..) | Op::Remove(_) => {}
                }
            }
            v
        }
    """,
    visible=[
        T("push_pop", "Push 1, Push 2, Pop, Push 3", "apply(&[Op::Push(1), Op::Push(2), Op::Pop, Op::Push(3)])", "vec![1, 3]"),
        T("insert_front", "Push 2, Insert(0, 1)", "apply(&[Op::Push(2), Op::Insert(0, 1)])", "vec![1, 2]"),
        T("pop_empty", "Pop", "apply(&[Op::Pop])", "Vec::<i32>::new()"),
        T("remove_middle", "Push 1, Push 2, Push 3, Remove(1)", "apply(&[Op::Push(1), Op::Push(2), Op::Push(3), Op::Remove(1)])", "vec![1, 3]"),
        T("insert_past_end_ignored", "Insert(1, 5) on an empty vec", "apply(&[Op::Insert(1, 5)])", "Vec::<i32>::new()"),
        T("insert_at_len_appends", "Push 1, Insert(1, 2)", "apply(&[Op::Push(1), Op::Insert(1, 2)])", "vec![1, 2]"),
    ],
    hidden=[
        T("out_of_range", "Push 5, Insert(3, 9), Remove(1)", "apply(&[Op::Push(5), Op::Insert(3, 9), Op::Remove(1)])", "vec![5]"),
        T("insert_at_end", "Push 1, Insert(1, 2), Remove(0)", "apply(&[Op::Push(1), Op::Insert(1, 2), Op::Remove(0)])", "vec![2]"),
        T("no_ops", "[]", "apply(&[])", "Vec::<i32>::new()"),
        T("remove_on_empty", "Remove(0)", "apply(&[Op::Remove(0)])", "Vec::<i32>::new()"),
        T("remove_last_index", "Push 1, Push 2, Remove(1)", "apply(&[Op::Push(1), Op::Push(2), Op::Remove(1)])", "vec![1]"),
        T("insert_middle", "Push 1, Push 3, Insert(1, 2)", "apply(&[Op::Push(1), Op::Push(3), Op::Insert(1, 2)])", "vec![1, 2, 3]"),
        T("extremes", "Push i32::MIN, Push i32::MAX, Insert(0, 0)", "apply(&[Op::Push(i32::MIN), Op::Push(i32::MAX), Op::Insert(0, 0)])", "vec![0, i32::MIN, i32::MAX]"),
        T("pop_until_empty_then_more", "Push 1, Pop, Pop, Push 2", "apply(&[Op::Push(1), Op::Pop, Op::Pop, Op::Push(2)])", "vec![2]"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2301);
            for _ in 0..300 {
                let n = rng.below(12);
                let mut ops = Vec::new();
                let mut want: Vec<i32> = Vec::new();
                for _ in 0..n {
                    let x = rng.int(-9, 9) as i32;
                    let i = rng.below(5);
                    let op = match rng.below(4) {
                        0 => Op::Push(x),
                        1 => Op::Pop,
                        2 => Op::Insert(i, x),
                        _ => Op::Remove(i),
                    };
                    ops.push(op);
                    want = match op {
                        Op::Push(x) => [&want[..], &[x]].concat(),
                        Op::Pop => want[..want.len().saturating_sub(1)].to_vec(),
                        Op::Insert(i, x) if i <= want.len() => [&want[..i], &[x], &want[i..]].concat(),
                        Op::Remove(i) if i < want.len() => [&want[..i], &want[i + 1..]].concat(),
                        _ => want,
                    };
                }
                check!(format!("ops = {ops:?}"), apply(&ops), want);
            }
        }

        #[test]
        fn scale_200k() {
            let mut ops: Vec<Op> = (0..200_000).map(Op::Push).collect();
            ops.extend((0..100_000).map(|_| Op::Pop));
            let v = apply(&ops);
            check!("200000 pushes, then 100000 pops", (v.len(), v[99_999]), (100_000, 99_999));
        }
        """,
    ],
    wrong=dict(
        insert_bound_strict="""
            #[derive(Debug, Clone, Copy)]
            pub enum Op {
                Push(i32),
                Pop,
                Insert(usize, i32),
                Remove(usize),
            }

            pub fn apply(ops: &[Op]) -> Vec<i32> {
                let mut v = Vec::new();
                for &op in ops {
                    match op {
                        Op::Push(x) => v.push(x),
                        Op::Pop => {
                            v.pop();
                        }
                        Op::Insert(i, x) if i < v.len() => v.insert(i, x),
                        Op::Remove(i) if i < v.len() => {
                            v.remove(i);
                        }
                        Op::Insert(..) | Op::Remove(_) => {}
                    }
                }
                v
            }
        """,
        remove_bound_loose="""
            #[derive(Debug, Clone, Copy)]
            pub enum Op {
                Push(i32),
                Pop,
                Insert(usize, i32),
                Remove(usize),
            }

            pub fn apply(ops: &[Op]) -> Vec<i32> {
                let mut v = Vec::new();
                for &op in ops {
                    match op {
                        Op::Push(x) => v.push(x),
                        Op::Pop => {
                            v.pop();
                        }
                        Op::Insert(i, x) if i <= v.len() => v.insert(i, x),
                        Op::Remove(i) if i <= v.len() => {
                            v.remove(i);
                        }
                        Op::Insert(..) | Op::Remove(_) => {}
                    }
                }
                v
            }
        """,
        pop_takes_front="""
            #[derive(Debug, Clone, Copy)]
            pub enum Op {
                Push(i32),
                Pop,
                Insert(usize, i32),
                Remove(usize),
            }

            pub fn apply(ops: &[Op]) -> Vec<i32> {
                let mut v = Vec::new();
                for &op in ops {
                    match op {
                        Op::Push(x) => v.push(x),
                        Op::Pop => {
                            if !v.is_empty() {
                                v.remove(0);
                            }
                        }
                        Op::Insert(i, x) if i <= v.len() => v.insert(i, x),
                        Op::Remove(i) if i < v.len() => {
                            v.remove(i);
                        }
                        Op::Insert(..) | Op::Remove(_) => {}
                    }
                }
                v
            }
        """,
    ),
    hints=[("rust", "`insert(i, x)` allows `i == len`; `remove(i)` needs `i < len`. Both panic otherwise.")],
    notes=("Match guards keep each operation's bounds check next to the operation.", "O(n) per insert or remove", "O(n)"),
    follow_up="Which of these operations are O(1), and which shift elements?",
    related=["S3"],
))

P.append(dict(
    slug="retain-and-dedup", title="retain and dedup", level="easy", stage="use-it", tags=["retain", "dedup"],
    teaches=["`retain` filters in place without a second Vec.", "`dedup` removes only consecutive repeats."],
    statement="Remove the negative numbers from `v`, then collapse runs of equal neighbours into one.",
    examples=[("v = [1, 1, -2, 1, 3, 3, -3, 3]", "[1, 3]")],
    starter="""
        pub fn clean(v: &mut Vec<i32>) {
            todo!()
        }
    """,
    solution="""
        pub fn clean(v: &mut Vec<i32>) {
            v.retain(|&x| x >= 0);
            v.dedup();
        }
    """,
    visible=[
        T("mixed", "v = [1, 1, -2, 1, 3, 3, -3, 3]", "{ let mut v = vec![1, 1, -2, 1, 3, 3, -3, 3]; clean(&mut v); v }", "vec![1, 3]"),
        T("no_change", "v = [1, 2]", "{ let mut v = vec![1, 2]; clean(&mut v); v }", "vec![1, 2]"),
        T("empty", "v = []", "{ let mut v: Vec<i32> = vec![]; clean(&mut v); v }", "Vec::<i32>::new()"),
        T("all_same", "v = [2, 2, 2]", "{ let mut v = vec![2, 2, 2]; clean(&mut v); v }", "vec![2]"),
        T("zero_is_kept", "v = [0, -1, 0]", "{ let mut v = vec![0, -1, 0]; clean(&mut v); v }", "vec![0]"),
    ],
    hidden=[
        T("non_adjacent", "v = [2, 1, 2]", "{ let mut v = vec![2, 1, 2]; clean(&mut v); v }", "vec![2, 1, 2]"),
        T("all_negative", "v = [-1, -1]", "{ let mut v = vec![-1, -1]; clean(&mut v); v }", "Vec::<i32>::new()"),
        T("single", "v = [5]", "{ let mut v = vec![5]; clean(&mut v); v }", "vec![5]"),
        T("single_negative", "v = [-5]", "{ let mut v = vec![-5]; clean(&mut v); v }", "Vec::<i32>::new()"),
        T("negative_between_equals", "v = [5, -1, 5]", "{ let mut v = vec![5, -1, 5]; clean(&mut v); v }", "vec![5]"),
        T("extremes", "v = [i32::MIN, i32::MAX, i32::MAX, i32::MIN]", "{ let mut v = vec![i32::MIN, i32::MAX, i32::MAX, i32::MIN]; clean(&mut v); v }", "vec![i32::MAX]"),
        T("order_kept", "v = [3, 1, 2, 2, 1]", "{ let mut v = vec![3, 1, 2, 2, 1]; clean(&mut v); v }", "vec![3, 1, 2, 1]"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2302);
            for _ in 0..300 {
                let n = rng.below(12);
                let v: Vec<i32> = rng.vec(n, -2, 2);
                let mut want: Vec<i32> = Vec::new();
                for &x in &v {
                    if x >= 0 && want.last() != Some(&x) {
                        want.push(x);
                    }
                }
                let mut got = v.clone();
                clean(&mut got);
                check!(format!("v = {v:?}"), got, want);
            }
        }

        #[test]
        fn scale_200k() {
            let mut v: Vec<i32> = (0..200_000).map(|i| if i % 2 == 0 { -1 } else { i / 1000 }).collect();
            clean(&mut v);
            check!("v = [-1, 0, -1, 0, …, -1, 199] (200000 values)", (v.len(), v[0], v[199]), (200, 0, 199));
        }
        """,
    ],
    wrong=dict(
        dedup_first="""
            pub fn clean(v: &mut Vec<i32>) {
                v.dedup();
                v.retain(|&x| x >= 0);
            }
        """,
        drops_every_repeat="""
            pub fn clean(v: &mut Vec<i32>) {
                let mut seen = std::collections::HashSet::new();
                v.retain(|&x| x >= 0 && seen.insert(x));
            }
        """,
        drops_zero="""
            pub fn clean(v: &mut Vec<i32>) {
                v.retain(|&x| x > 0);
                v.dedup();
            }
        """,
    ),
    hints=[("rust", "Order matters: removing negatives can make equal values adjacent.")],
    notes=("Filtering first lets `dedup` see the neighbours that removal created.", "O(n)", "O(1)"),
    follow_up="How would you remove every duplicate, not just adjacent ones, and keep first-seen order?",
    related=["D1"],
))

P.append(dict(
    slug="sort-by-key", title="sort_by with a tie-breaker", level="easy", stage="use-it", tags=["sort_by", "Ordering::then_with"],
    teaches=["`Ordering::then_with` for multi-key sorts.", "Why `sort_by_key` with a `String` key clones every comparison."],
    statement="Sort `words` by length, shortest first, and alphabetically among words of equal length.",
    examples=[("[\"pear\", \"fig\", \"apple\", \"kiwi\"]", "[\"fig\", \"kiwi\", \"pear\", \"apple\"]")],
    starter="""
        pub fn by_len_then_alpha(words: &mut [String]) {
            todo!()
        }
    """,
    solution="""
        pub fn by_len_then_alpha(words: &mut [String]) {
            words.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        }
    """,
    visible=[
        T("fruit", "[\"pear\", \"fig\", \"apple\", \"kiwi\"]", '{ let mut w: Vec<String> = ["pear", "fig", "apple", "kiwi"].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["fig", "kiwi", "pear", "apple"]'),
        T("empty", "[]", "{ let mut w: Vec<String> = vec![]; by_len_then_alpha(&mut w); w }", "Vec::<String>::new()"),
        T("ties_alphabetical", "[\"dd\", \"cc\", \"a\"]", '{ let mut w: Vec<String> = ["dd", "cc", "a"].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["a", "cc", "dd"]'),
        T("duplicates", "[\"b\", \"a\", \"b\"]", '{ let mut w: Vec<String> = ["b", "a", "b"].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["a", "b", "b"]'),
        T("shorter_first", "[\"abc\", \"z\"]", '{ let mut w: Vec<String> = ["abc", "z"].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["z", "abc"]'),
    ],
    hidden=[
        T("same_length", "[\"b\", \"a\", \"c\"]", '{ let mut w: Vec<String> = ["b", "a", "c"].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["a", "b", "c"]'),
        T("single", "[\"x\"]", '{ let mut w = vec!["x".to_string()]; by_len_then_alpha(&mut w); w }', 'vec!["x"]'),
        T("empty_string_first", "[\"a\", \"\"]", '{ let mut w: Vec<String> = ["a", ""].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["", "a"]'),
        T("uppercase_before_lowercase", "[\"b\", \"B\", \"a\"]", '{ let mut w: Vec<String> = ["b", "B", "a"].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["B", "a", "b"]'),
        T("already_sorted", "[\"a\", \"bb\", \"ccc\"]", '{ let mut w: Vec<String> = ["a", "bb", "ccc"].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["a", "bb", "ccc"]'),
        T("reverse_sorted", "[\"ccc\", \"bb\", \"a\"]", '{ let mut w: Vec<String> = ["ccc", "bb", "a"].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["a", "bb", "ccc"]'),
        T("prefix_ties", "[\"abd\", \"abc\", \"ab\"]", '{ let mut w: Vec<String> = ["abd", "abc", "ab"].map(String::from).to_vec(); by_len_then_alpha(&mut w); w }', 'vec!["ab", "abc", "abd"]'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2303);
            for _ in 0..300 {
                let n = rng.below(8);
                let mut words = Vec::new();
                for _ in 0..n {
                    let len = rng.below(4);
                    words.push(rng.string(len, "abc"));
                }
                let mut want = words.clone();
                want.sort_by_key(|w| (w.len(), w.clone()));
                let mut got = words.clone();
                by_len_then_alpha(&mut got);
                check!(format!("words = {words:?}"), got, want);
            }
        }

        #[test]
        fn scale_200k() {
            let mut w: Vec<String> = (0..200_000u32).rev().map(|i| format!("{:x}", i.wrapping_mul(2_654_435_761))).collect();
            by_len_then_alpha(&mut w);
            let ok = w.windows(2).all(|p| (p[0].len(), &p[0]) <= (p[1].len(), &p[1]));
            check!("200000 hex strings", (w.len(), ok), (200_000, true));
        }
        """,
    ],
    wrong=dict(
        length_only="""
            pub fn by_len_then_alpha(words: &mut [String]) {
                words.sort_by_key(|w| w.len());
            }
        """,
        alphabetical_only="""
            pub fn by_len_then_alpha(words: &mut [String]) {
                words.sort();
            }
        """,
        insertion_sort="""
            pub fn by_len_then_alpha(words: &mut [String]) {
                for i in 1..words.len() {
                    let mut j = i;
                    while j > 0 && (words[j].len(), &words[j]) < (words[j - 1].len(), &words[j - 1]) {
                        words.swap(j, j - 1);
                        j -= 1;
                    }
                }
            }
        """,
    ),
    hints=[("rust", "Compare lengths, and only if they're equal, compare the strings: `then_with`.")],
    notes=("`then_with` only evaluates the second comparison on a tie. `sort_by_key(|w| (w.len(), w.clone()))` also works but allocates per comparison; `sort_by_cached_key` would allocate once per element.", "O(n log n)", "O(n)"),
    follow_up="When is `sort_by_cached_key` the right choice?",
    related=["S8"],
))

P.append(dict(
    slug="slices-as-views", title="Slices as views", level="easy", stage="use-it", tags=["&[T]", "ranges", "position"],
    teaches=["Returning a sub-slice borrows from the input; nothing is copied.", "`get(range)` instead of indexing when the range might be invalid."],
    statement="""
        Write `middle`, which drops the first and last elements (empty if there are fewer than two),
        and `trim_zeros`, which drops leading and trailing zeros. Both return views into the input.
    """,
    starter="""
        pub fn middle(v: &[i32]) -> &[i32] {
            todo!()
        }

        pub fn trim_zeros(v: &[i32]) -> &[i32] {
            todo!()
        }
    """,
    solution="""
        pub fn middle(v: &[i32]) -> &[i32] {
            v.get(1..v.len().saturating_sub(1)).unwrap_or(&[])
        }

        pub fn trim_zeros(v: &[i32]) -> &[i32] {
            let start = v.iter().position(|&x| x != 0).unwrap_or(v.len());
            let end = v.iter().rposition(|&x| x != 0).map_or(start, |i| i + 1);
            &v[start..end]
        }
    """,
    visible=[
        T("middle_of_four", "v = [1, 2, 3, 4]", "middle(&[1, 2, 3, 4])", "&[2, 3][..]"),
        T("middle_of_one", "v = [1]", "middle(&[1])", "&[][..]"),
        T("trim", "v = [0, 0, 5, 0, 7, 0]", "trim_zeros(&[0, 0, 5, 0, 7, 0])", "&[5, 0, 7][..]"),
        T("middle_of_three", "v = [1, 2, 3]", "middle(&[1, 2, 3])", "&[2][..]"),
        T("trim_nothing_to_trim", "v = [1, 2]", "trim_zeros(&[1, 2])", "&[1, 2][..]"),
    ],
    hidden=[
        T("middle_of_empty", "v = []", "middle(&[])", "&[][..]"),
        T("trim_all_zero", "v = [0, 0]", "trim_zeros(&[0, 0])", "&[][..]"),
        T("middle_of_two", "v = [1, 2]", "middle(&[1, 2])", "&[][..]"),
        T("trim_empty", "v = []", "trim_zeros(&[])", "&[][..]"),
        T("trim_single_zero", "v = [0]", "trim_zeros(&[0])", "&[][..]"),
        T("trim_single_value", "v = [7]", "trim_zeros(&[7])", "&[7][..]"),
        T("trim_negatives", "v = [0, -1, 0, -2, 0]", "trim_zeros(&[0, -1, 0, -2, 0])", "&[-1, 0, -2][..]"),
        T("trim_trailing_only", "v = [4, 0, 0]", "trim_zeros(&[4, 0, 0])", "&[4][..]"),
        T("views_not_copies", "results point into v", "(middle(&v).as_ptr() == v[1..].as_ptr(), trim_zeros(&v).as_ptr() == v[1..].as_ptr())", "(true, true)", setup="let v = vec![0, 3, 4, 0];"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2304);
            for _ in 0..300 {
                let n = rng.below(8);
                let v: Vec<i32> = rng.vec(n, -1, 1);
                let want_middle: Vec<i32> = if n < 2 { vec![] } else { v[1..n - 1].to_vec() };
                let mut t = v.clone();
                while t.first() == Some(&0) {
                    t.remove(0);
                }
                while t.last() == Some(&0) {
                    t.pop();
                }
                check!(format!("v = {v:?}"), (middle(&v).to_vec(), trim_zeros(&v).to_vec()), (want_middle, t));
            }
        }

        #[test]
        fn scale_200k() {
            let mut v = vec![0; 200_000];
            v[100_000] = 5;
            check!("v = 200000 zeros with a 5 at index 100000", (trim_zeros(&v), middle(&v).len()), (&[5][..], 199_998));
        }
        """,
    ],
    wrong=dict(
        trims_leading_only="""
            pub fn middle(v: &[i32]) -> &[i32] {
                v.get(1..v.len().saturating_sub(1)).unwrap_or(&[])
            }

            pub fn trim_zeros(v: &[i32]) -> &[i32] {
                let start = v.iter().position(|&x| x != 0).unwrap_or(v.len());
                &v[start..]
            }
        """,
        end_off_by_one="""
            pub fn middle(v: &[i32]) -> &[i32] {
                v.get(1..v.len().saturating_sub(1)).unwrap_or(&[])
            }

            pub fn trim_zeros(v: &[i32]) -> &[i32] {
                let start = v.iter().position(|&x| x != 0).unwrap_or(v.len());
                let end = v.iter().rposition(|&x| x != 0).unwrap_or(start);
                &v[start..end.max(start)]
            }
        """,
        middle_keeps_last="""
            pub fn middle(v: &[i32]) -> &[i32] {
                v.get(1..).unwrap_or(&[])
            }

            pub fn trim_zeros(v: &[i32]) -> &[i32] {
                let start = v.iter().position(|&x| x != 0).unwrap_or(v.len());
                let end = v.iter().rposition(|&x| x != 0).map_or(start, |i| i + 1);
                &v[start..end]
            }
        """,
    ),
    hints=[("rust", "`v.get(a..b)` returns `None` when the range is invalid, including a > b."),
           ("rust", "`position` and `rposition` find the first and last non-zero.")],
    notes=("The returned slices borrow `v`, so their lifetime is tied to the input by elision.", "O(n)", "O(1)"),
    follow_up="Why can these functions omit lifetime annotations?",
    related=["L3"],
))

P.append(dict(
    slug="fix-neighbour-loop", title="Fix: off-by-one in a neighbour loop", mode="fix", level="easy", stage="use-it", tags=["windows", "index out of bounds"],
    teaches=["`windows(2)` yields each pair of neighbours without index arithmetic."],
    statement="`pair_sums` should return the sum of each pair of neighbours. It panics.",
    examples=[("v = [1, 2, 3]", "[3, 5]")],
    starter="""
        /// Sums of neighbouring pairs: [a, b, c] → [a + b, b + c].
        pub fn pair_sums(v: &[i32]) -> Vec<i32> {
            let mut out = Vec::new();
            for i in 0..v.len() {
                out.push(v[i] + v[i + 1]);
            }
            out
        }
    """,
    solution="""
        /// Sums of neighbouring pairs: [a, b, c] → [a + b, b + c].
        pub fn pair_sums(v: &[i32]) -> Vec<i32> {
            v.windows(2).map(|w| w[0] + w[1]).collect()
        }
    """,
    visible=[
        T("three", "v = [1, 2, 3]", "pair_sums(&[1, 2, 3])", "vec![3, 5]"),
        T("one", "v = [9]", "pair_sums(&[9])", "Vec::<i32>::new()"),
        T("four", "v = [1, 2, 3, 4]", "pair_sums(&[1, 2, 3, 4])", "vec![3, 5, 7]"),
        T("two", "v = [5, 6]", "pair_sums(&[5, 6])", "vec![11]"),
        T("empty_visible", "v = []", "pair_sums(&[])", "Vec::<i32>::new()"),
    ],
    hidden=[
        T("empty", "v = []", "pair_sums(&[])", "Vec::<i32>::new()"),
        T("negatives", "v = [-1, 1, -1]", "pair_sums(&[-1, 1, -1])", "vec![0, 0]"),
        T("zeros", "v = [0, 0, 0]", "pair_sums(&[0, 0, 0])", "vec![0, 0]"),
        T("big_values", "v = [1000000000, 1000000000, -1000000000]", "pair_sums(&[1_000_000_000, 1_000_000_000, -1_000_000_000])", "vec![2_000_000_000, 0]"),
        T("extremes", "v = [i32::MAX, 0, i32::MIN]", "pair_sums(&[i32::MAX, 0, i32::MIN])", "vec![i32::MAX, i32::MIN]"),
        T("overlapping_pairs", "v = [1, 10, 100, 1000]", "pair_sums(&[1, 10, 100, 1000])", "vec![11, 110, 1100]"),
        T("length", "v = [1; 1000]", "pair_sums(&[1; 1000]).len()", "999"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2305);
            for _ in 0..300 {
                let n = rng.below(10);
                let v: Vec<i32> = rng.vec(n, -1000, 1000);
                let mut want = Vec::new();
                for i in 1..n {
                    want.push(v[i - 1] + v[i]);
                }
                check!(format!("v = {v:?}"), pair_sums(&v), want);
            }
        }

        #[test]
        fn scale_200k() {
            let v: Vec<i32> = (0..200_000).collect();
            let out = pair_sums(&v);
            check!("v = 0..200000", (out.len(), out[0], out[199_998]), (199_999, 1, 399_997));
        }
        """,
    ],
    wrong=dict(
        len_minus_one="""
            /// Sums of neighbouring pairs: [a, b, c] → [a + b, b + c].
            pub fn pair_sums(v: &[i32]) -> Vec<i32> {
                let mut out = Vec::new();
                for i in 0..v.len() - 1 {
                    out.push(v[i] + v[i + 1]);
                }
                out
            }
        """,
        chunks_not_windows="""
            /// Sums of neighbouring pairs: [a, b, c] → [a + b, b + c].
            pub fn pair_sums(v: &[i32]) -> Vec<i32> {
                v.chunks(2).map(|c| c.iter().sum()).collect()
            }
        """,
    ),
    hints=[("approach", "The last index has no right neighbour."), ("rust", "Slices can hand you overlapping pairs directly.")],
    notes=("`windows(2)` yields nothing for fewer than two elements, so the empty and single cases need no special handling.", "O(n)", "O(n)"),
    follow_up="How is `windows` different from `chunks`?",
    related=["S6"],
))

P.append(dict(
    slug="rotate-in-place", title="Rotate a slice in place", level="easy", stage="use-it", tags=["reverse", "rotate"],
    teaches=["Three reversals rotate a slice in O(1) space.", "Reduce `k` modulo the length first."],
    statement="Rotate `v` right by `k` positions in place.",
    examples=[("v = [1, 2, 3, 4, 5], k = 2", "[4, 5, 1, 2, 3]")],
    starter="""
        pub fn rotate_right(v: &mut [i32], k: usize) {
            todo!()
        }
    """,
    solution="""
        pub fn rotate_right(v: &mut [i32], k: usize) {
            if v.is_empty() {
                return;
            }
            let k = k % v.len();
            v.reverse();
            v[..k].reverse();
            v[k..].reverse();
        }
    """,
    visible=[
        T("two", "v = [1, 2, 3, 4, 5], k = 2", "{ let mut v = [1, 2, 3, 4, 5]; rotate_right(&mut v, 2); v }", "[4, 5, 1, 2, 3]"),
        T("full_turn", "v = [1, 2], k = 2", "{ let mut v = [1, 2]; rotate_right(&mut v, 2); v }", "[1, 2]"),
        T("leetcode_189_first", "v = [1, 2, 3, 4, 5, 6, 7], k = 3", "{ let mut v = [1, 2, 3, 4, 5, 6, 7]; rotate_right(&mut v, 3); v }", "[5, 6, 7, 1, 2, 3, 4]"),
        T("leetcode_189_second", "v = [-1, -100, 3, 99], k = 2", "{ let mut v = [-1, -100, 3, 99]; rotate_right(&mut v, 2); v }", "[3, 99, -1, -100]"),
        T("k_zero", "v = [1, 2, 3], k = 0", "{ let mut v = [1, 2, 3]; rotate_right(&mut v, 0); v }", "[1, 2, 3]"),
    ],
    hidden=[
        T("empty", "v = [], k = 3", "{ let mut v: [i32; 0] = []; rotate_right(&mut v, 3); v }", "[]"),
        T("large_k", "v = [1, 2, 3], k = 7", "{ let mut v = [1, 2, 3]; rotate_right(&mut v, 7); v }", "[3, 1, 2]"),
        T("single", "v = [5], k = 4", "{ let mut v = [5]; rotate_right(&mut v, 4); v }", "[5]"),
        T("by_one", "v = [1, 2, 3, 4], k = 1", "{ let mut v = [1, 2, 3, 4]; rotate_right(&mut v, 1); v }", "[4, 1, 2, 3]"),
        T("len_minus_one", "v = [1, 2, 3, 4], k = 3", "{ let mut v = [1, 2, 3, 4]; rotate_right(&mut v, 3); v }", "[2, 3, 4, 1]"),
        T("k_max", "v = [1, 2, 3, 4, 5, 6, 7], k = usize::MAX (≡ 1 mod 7)", "{ let mut v = [1, 2, 3, 4, 5, 6, 7]; rotate_right(&mut v, usize::MAX); v }", "[7, 1, 2, 3, 4, 5, 6]"),
        T("duplicates", "v = [1, 1, 2, 2], k = 1", "{ let mut v = [1, 1, 2, 2]; rotate_right(&mut v, 1); v }", "[2, 1, 1, 2]"),
        T("extremes", "v = [i32::MIN, 0, i32::MAX], k = 2", "{ let mut v = [i32::MIN, 0, i32::MAX]; rotate_right(&mut v, 2); v }", "[0, i32::MAX, i32::MIN]"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2306);
            for _ in 0..300 {
                let n = rng.below(9);
                let v: Vec<i32> = rng.vec(n, -9, 9);
                let k = rng.below(20);
                let want: Vec<i32> = (0..n).map(|i| v[(i + n - k % n) % n]).collect();
                let mut got = v.clone();
                rotate_right(&mut got, k);
                check!(format!("v = {v:?}, k = {k}"), got, want);
            }
        }

        #[test]
        fn scale_200k() {
            let mut v: Vec<i32> = (0..200_000).collect();
            rotate_right(&mut v, 100_000);
            check!("v = 0..200000, k = 100000", (v[0], v[99_999], v[100_000], v[199_999]), (100_000, 199_999, 0, 99_999));
        }
        """,
    ],
    wrong=dict(
        rotates_left="""
            pub fn rotate_right(v: &mut [i32], k: usize) {
                if v.is_empty() {
                    return;
                }
                let k = k % v.len();
                v[..k].reverse();
                v[k..].reverse();
                v.reverse();
            }
        """,
        k_not_reduced="""
            pub fn rotate_right(v: &mut [i32], k: usize) {
                if v.is_empty() || k > v.len() {
                    return;
                }
                let k = k % v.len();
                v.reverse();
                v[..k].reverse();
                v[k..].reverse();
            }
        """,
        one_step_at_a_time="""
            pub fn rotate_right(v: &mut [i32], k: usize) {
                if v.is_empty() {
                    return;
                }
                for _ in 0..k % v.len() {
                    let last = v[v.len() - 1];
                    for i in (1..v.len()).rev() {
                        v[i] = v[i - 1];
                    }
                    v[0] = last;
                }
            }
        """,
    ),
    hints=[("approach", "Reverse the whole slice, then reverse the first k and the rest separately."), ("edge case", "k can be larger than the length, and the length can be 0.")],
    notes=("std has `slice::rotate_right`, which does the same in O(n) time and O(1) space; the three-reversal trick is what interviews ask for.", "O(n)", "O(1)"),
    follow_up="Prove that the three reversals produce the rotation.",
    related=["D2"],
))

P.append(dict(
    slug="remove-duplicates-sorted", title="Remove duplicates from a sorted slice", level="easy", stage="use-it", tags=["two pointers", "in place"],
    teaches=["A write index for in-place compaction.", "Returning a length instead of shrinking the slice."],
    statement="`v` is sorted. Move its distinct values to the front, in order, and return how many there are.",
    examples=[("v = [0, 0, 1, 1, 1, 2]", "3, with v starting [0, 1, 2]")],
    starter="""
        pub fn dedup_sorted(v: &mut [i32]) -> usize {
            todo!()
        }
    """,
    solution="""
        pub fn dedup_sorted(v: &mut [i32]) -> usize {
            if v.is_empty() {
                return 0;
            }
            let mut write = 1;
            for read in 1..v.len() {
                if v[read] != v[write - 1] {
                    v[write] = v[read];
                    write += 1;
                }
            }
            write
        }
    """,
    visible=[
        T("classic", "v = [0, 0, 1, 1, 1, 2]", "{ let mut v = [0, 0, 1, 1, 1, 2]; let k = dedup_sorted(&mut v); v[..k].to_vec() }", "vec![0, 1, 2]"),
        T("count", "v = [1, 1, 2]", "dedup_sorted(&mut [1, 1, 2])", "2"),
        T("leetcode_26_first", "v = [1, 1, 2]", "{ let mut v = [1, 1, 2]; let k = dedup_sorted(&mut v); (k, v[..k].to_vec()) }", "(2, vec![1, 2])"),
        T("leetcode_26_second", "v = [0, 0, 1, 1, 1, 2, 2, 3, 3, 4]", "{ let mut v = [0, 0, 1, 1, 1, 2, 2, 3, 3, 4]; let k = dedup_sorted(&mut v); (k, v[..k].to_vec()) }", "(5, vec![0, 1, 2, 3, 4])"),
        T("single", "v = [7]", "{ let mut v = [7]; let k = dedup_sorted(&mut v); (k, v[..k].to_vec()) }", "(1, vec![7])"),
    ],
    hidden=[
        T("empty", "v = []", "dedup_sorted(&mut [])", "0"),
        T("all_distinct", "v = [-3, 0, 7]", "{ let mut v = [-3, 0, 7]; let k = dedup_sorted(&mut v); v[..k].to_vec() }", "vec![-3, 0, 7]"),
        T("all_same", "v = [3, 3, 3, 3, 3]", "{ let mut v = [3; 5]; let k = dedup_sorted(&mut v); (k, v[..k].to_vec()) }", "(1, vec![3])"),
        T("negatives", "v = [-2, -2, -1]", "{ let mut v = [-2, -2, -1]; let k = dedup_sorted(&mut v); (k, v[..k].to_vec()) }", "(2, vec![-2, -1])"),
        T("extremes", "v = [i32::MIN, i32::MIN, i32::MAX]", "{ let mut v = [i32::MIN, i32::MIN, i32::MAX]; let k = dedup_sorted(&mut v); (k, v[..k].to_vec()) }", "(2, vec![i32::MIN, i32::MAX])"),
        T("run_at_end", "v = [1, 2, 2, 2]", "{ let mut v = [1, 2, 2, 2]; let k = dedup_sorted(&mut v); (k, v[..k].to_vec()) }", "(2, vec![1, 2])"),
        T("two_equal", "v = [4, 4]", "{ let mut v = [4, 4]; let k = dedup_sorted(&mut v); (k, v[..k].to_vec()) }", "(1, vec![4])"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2307);
            for _ in 0..300 {
                let n = rng.below(12);
                let mut v: Vec<i32> = rng.vec(n, -3, 3);
                v.sort();
                let mut want = v.clone();
                want.dedup();
                let mut got = v.clone();
                let k = dedup_sorted(&mut got);
                check!(format!("v = {v:?}"), (k, got[..k].to_vec()), (want.len(), want));
            }
        }

        #[test]
        fn scale_200k() {
            let mut v: Vec<i32> = (0..200_000).map(|i| i / 2).collect();
            v.extend(vec![100_000; 100_000]);
            let k = dedup_sorted(&mut v);
            check!("v = [0, 0, 1, 1, …, 99999, 99999] then 100000 × 100000", (k, v[k - 1]), (100_001, 100_000));
        }
        """,
    ],
    wrong=dict(
        counts_without_moving="""
            pub fn dedup_sorted(v: &mut [i32]) -> usize {
                if v.is_empty() {
                    return 0;
                }
                1 + v.windows(2).filter(|w| w[0] != w[1]).count()
            }
        """,
        shift_left_each_time="""
            pub fn dedup_sorted(v: &mut [i32]) -> usize {
                let mut len = v.len();
                let mut i = 1;
                while i < len {
                    if v[i] == v[i - 1] {
                        for j in i..len - 1 {
                            v[j] = v[j + 1];
                        }
                        len -= 1;
                    } else {
                        i += 1;
                    }
                }
                len
            }
        """,
    ),
    hints=[("approach", "Keep a write position; copy a value there only if it differs from the last value written.")],
    notes=("A slice can't shrink, so the length is the result. `Vec::dedup` does this and then truncates.", "O(n)", "O(1)"),
    follow_up="How would you allow each value at most twice?",
    related=["D2"],
))

P.append(dict(
    slug="exact-capacity", title="Capacity and reallocation", level="medium", stage="understand-it", tags=["with_capacity", "String"],
    source="W41",
    teaches=["Pre-sizing a `String` or `Vec` avoids reallocating as it grows.", "Computing the exact final length up front."],
    statement="""
        Join `parts` with `sep` between them. The result must be allocated exactly once:
        its capacity must equal its length. The tests count heap allocations, so building the string
        in several steps and calling `shrink_to_fit` at the end doesn't pass.
    """,
    perf=dict(allocs=True),
    examples=[("parts = [\"a\", \"bb\", \"ccc\"], sep = \", \"", "\"a, bb, ccc\"")],
    starter="""
        pub fn join_with(parts: &[&str], sep: &str) -> String {
            todo!()
        }
    """,
    solution="""
        pub fn join_with(parts: &[&str], sep: &str) -> String {
            let total =
                parts.iter().map(|p| p.len()).sum::<usize>() + sep.len() * parts.len().saturating_sub(1);
            let mut out = String::with_capacity(total);
            for (i, p) in parts.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep);
                }
                out.push_str(p);
            }
            out
        }
    """,
    visible=[
        T("three", "parts = [\"a\", \"bb\", \"ccc\"], sep = \", \"", '{ let s = join_with(&["a", "bb", "ccc"], ", "); (s.clone(), s.capacity() == s.len()) }', '("a, bb, ccc".to_string(), true)'),
        T("one", "parts = [\"solo\"], sep = \"-\"", '{ let s = join_with(&["solo"], "-"); (s.clone(), s.capacity() == s.len()) }', '("solo".to_string(), true)'),
        T("empty_sep", "parts = [\"a\", \"b\"], sep = \"\"", '{ let s = join_with(&["a", "b"], ""); (s.clone(), s.capacity() == s.len()) }', '("ab".to_string(), true)'),
        T("no_parts", "parts = [], sep = \",\"", 'join_with(&[], ",")', "String::new()"),
        T("empty_parts_keep_seps", "parts = [\"\", \"\"], sep = \",\"", '{ let s = join_with(&["", ""], ","); (s.clone(), s.capacity() == s.len()) }', '(",".to_string(), true)'),
        T("allocates_once", "parts = [\"a\", \"bb\", \"ccc\"], sep = \", \": heap allocations", 'anneal_prelude::allocs(|| join_with(&["a", "bb", "ccc"], ", ")).1.count', "1"),
    ],
    hidden=[
        T("none", "parts = [], sep = \",\"", 'join_with(&[], ",")', '""'),
        T("many", "100 parts of \"xyz\", sep = \"/\"", '{ let parts = vec!["xyz"; 100]; let s = join_with(&parts, "/"); (s.len(), s.capacity() == s.len()) }', "(399, true)"),
        T("single_empty_part", "parts = [\"\"], sep = \"-\"", 'join_with(&[""], "-")', "String::new()"),
        T("unicode", "parts = [\"日本\", \"é\"], sep = \" → \"", '{ let s = join_with(&["日本", "é"], " → "); (s.clone(), s.capacity()) }', '("日本 → é".to_string(), 13)'),
        T("long_sep", "parts = [\"a\", \"b\", \"c\"], sep = \"<->\"", 'join_with(&["a", "b", "c"], "<->")', '"a<->b<->c".to_string()'),
        T("capacity_ten_parts", "10 parts of \"abc\", sep = \", \": (length, capacity, heap allocations)", '{ let (s, seen) = anneal_prelude::allocs(|| join_with(&["abc"; 10], ", ")); (s.len(), s.capacity(), seen.count) }', "(48, 48, 1)"),
        T("no_trailing_sep", "parts = [\"x\", \"y\"], sep = \";\"", 'join_with(&["x", "y"], ";")', '"x;y".to_string()'),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2308);
            for _ in 0..300 {
                let n = rng.below(6);
                let mut parts = Vec::new();
                for _ in 0..n {
                    let len = rng.below(4);
                    parts.push(rng.string(len, "ab日"));
                }
                let len = rng.below(3);
                let sep = rng.string(len, ",→");
                let refs: Vec<&str> = parts.iter().map(|p| p.as_str()).collect();
                let mut want = String::new();
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        want += &sep;
                    }
                    want += p;
                }
                let got = join_with(&refs, &sep);
                check!(format!("parts = {parts:?}, sep = {sep:?}"), (got.capacity(), got), (want.len(), want));
            }
        }

        #[test]
        fn scale_500k_parts() {
            let parts = vec!["ab"; 500_000];
            let s = join_with(&parts, ",");
            check!("parts = [\\"ab\\"; 500000], sep = \\",\\"", (s.len(), s.capacity()), (1_499_999, 1_499_999));
        }
        """,
    ],
    wrong=dict(
        grows_as_it_goes="""
            pub fn join_with(parts: &[&str], sep: &str) -> String {
                let mut out = String::new();
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        out.push_str(sep);
                    }
                    out.push_str(p);
                }
                out
            }
        """,
        one_separator_too_many="""
            pub fn join_with(parts: &[&str], sep: &str) -> String {
                let total = parts.iter().map(|p| p.len() + sep.len()).sum::<usize>();
                let mut out = String::with_capacity(total);
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        out.push_str(sep);
                    }
                    out.push_str(p);
                }
                out
            }
        """,
        rebuilds_each_time="""
            pub fn join_with(parts: &[&str], sep: &str) -> String {
                let mut out = String::new();
                for (i, p) in parts.iter().enumerate() {
                    out = if i == 0 { p.to_string() } else { format!("{out}{sep}{p}") };
                }
                out.shrink_to_fit();
                out
            }
        """,
    ),
    hints=[("approach", "Add up the parts' lengths and the separators before building anything."), ("rust", "`String::with_capacity(n)`, then `push_str`.")],
    notes=("One allocation of the exact size. std's `[&str]::join` computes the same total internally.", "O(total length)", "O(total length)"),
    follow_up="How much memory does pushing into an empty `String` waste on average?",
    related=["Y4", "S2"],
))

P.append(dict(
    slug="predict-reallocations", title="Predict the allocation count", level="medium", stage="understand-it", tags=["capacity", "growth"],
    teaches=["`Vec` grows geometrically: amortised O(1) push.", "Its first allocation holds 4 small elements, not 1."],
    statement="""
        Don't run anything yet. Pushing the numbers `0..100` one at a time into `Vec::<u32>::new()`:
        how many times does the capacity change, and what is it at the end? Fill in the constants.
    """,
    starter="""
        /// How many times the capacity changes while pushing 0..100 into Vec::<u32>::new().
        pub const REALLOCATIONS: usize = 0;

        /// The capacity after those 100 pushes.
        pub const FINAL_CAPACITY: usize = 0;
    """,
    solution="""
        /// How many times the capacity changes while pushing 0..100 into Vec::<u32>::new().
        /// 0 → 4 → 8 → 16 → 32 → 64 → 128.
        pub const REALLOCATIONS: usize = 6;

        /// The capacity after those 100 pushes.
        pub const FINAL_CAPACITY: usize = 128;
    """,
    visible=[
        """
        #[test]
        fn reallocation_count() {
            let mut v: Vec<u32> = Vec::new();
            let (mut changes, mut cap) = (0, v.capacity());
            for i in 0..100 {
                v.push(i);
                if v.capacity() != cap {
                    changes += 1;
                    cap = v.capacity();
                }
            }
            check!("push 0..100 into Vec::<u32>::new()", REALLOCATIONS, changes);
        }
        """,
        """
        #[test]
        fn final_capacity() {
            let mut v: Vec<u32> = Vec::new();
            v.extend(0..100u32);
            let mut w: Vec<u32> = Vec::new();
            for i in 0..100 {
                w.push(i);
            }
            check!("capacity after 100 pushes", FINAL_CAPACITY, w.capacity());
        }
        """,
        T("some_reallocations", "REALLOCATIONS", "REALLOCATIONS > 0", "true"),
        T("room_for_all_100", "FINAL_CAPACITY", "FINAL_CAPACITY >= 100", "true"),
        T("capacity_is_a_power_of_two", "FINAL_CAPACITY", "FINAL_CAPACITY.is_power_of_two()", "true"),
    ],
    hidden=[
        """
        #[test]
        fn with_capacity_never_reallocates() {
            let mut v: Vec<u32> = Vec::with_capacity(100);
            let cap = v.capacity();
            v.extend(0..100u32);
            check!("Vec::with_capacity(100) then 100 pushes", (REALLOCATIONS > 0, v.capacity() == cap), (true, true));
        }

        fn run() -> (usize, usize) {
            let mut v: Vec<u32> = Vec::new();
            let (mut changes, mut cap) = (0, v.capacity());
            for i in 0..100 {
                v.push(i);
                if v.capacity() != cap {
                    changes += 1;
                    cap = v.capacity();
                }
            }
            (changes, v.capacity())
        }

        #[test]
        fn both_constants() {
            check!("(REALLOCATIONS, FINAL_CAPACITY) for 100 pushes", (REALLOCATIONS, FINAL_CAPACITY), run());
        }

        #[test]
        fn first_capacity_is_not_one() {
            check!("FINAL_CAPACITY >> (REALLOCATIONS - 1): the first capacity", FINAL_CAPACITY >> REALLOCATIONS.saturating_sub(1), 4);
        }

        #[test]
        fn not_an_exact_fit() {
            check!("FINAL_CAPACITY != 100", FINAL_CAPACITY != 100, true);
        }

        #[test]
        fn doubling_steps() {
            check!("REALLOCATIONS as doublings from 4", REALLOCATIONS, (FINAL_CAPACITY / 4).trailing_zeros() as usize + 1);
        }

        #[test]
        fn fewer_than_ten() {
            check!("REALLOCATIONS < 10", REALLOCATIONS < 10, true);
        }

        #[test]
        fn capacity_below_double() {
            check!("FINAL_CAPACITY < 200", FINAL_CAPACITY < 200, true);
        }

        #[test]
        fn reallocations_exact() {
            check!("REALLOCATIONS", REALLOCATIONS, run().0);
        }
        """,
    ],
    wrong=dict(
        doubling_from_one="""
            /// How many times the capacity changes while pushing 0..100 into Vec::<u32>::new().
            pub const REALLOCATIONS: usize = 8;

            /// The capacity after those 100 pushes.
            pub const FINAL_CAPACITY: usize = 128;
        """,
        counts_the_start="""
            /// How many times the capacity changes while pushing 0..100 into Vec::<u32>::new().
            pub const REALLOCATIONS: usize = 7;

            /// The capacity after those 100 pushes.
            pub const FINAL_CAPACITY: usize = 128;
        """,
        exact_fit="""
            /// How many times the capacity changes while pushing 0..100 into Vec::<u32>::new().
            pub const REALLOCATIONS: usize = 1;

            /// The capacity after those 100 pushes.
            pub const FINAL_CAPACITY: usize = 100;
        """,
    ),
    hints=[("rust", "Capacity doubles when full. What's the first non-zero capacity for a small element type?")],
    notes=("std's minimum non-zero capacity is 4 for elements up to 1 KiB, then it doubles, so 100 pushes cost 6 allocations and copy fewer than 128 elements in total.", "O(1) amortised per push", "O(n)"),
    follow_up="Why does doubling give amortised O(1) push, while growing by a constant doesn't?",
    related=["Y1", "Y4"],
))

P.append(dict(
    slug="drain-splice-split-off", title="drain, splice and split_off", level="medium", stage="understand-it", tags=["split_off", "splice"],
    teaches=["`split_off` moves the tail into a new Vec.", "`splice` replaces a range and returns what it removed."],
    statement="""
        Write `take_tail`, which removes everything from index `at` on and returns it (empty if `at`
        is past the end), and `replace_range`, which replaces `v[start..end]` with `with` and returns
        the removed values.
    """,
    starter="""
        pub fn take_tail(v: &mut Vec<i32>, at: usize) -> Vec<i32> {
            todo!()
        }

        pub fn replace_range(v: &mut Vec<i32>, start: usize, end: usize, with: &[i32]) -> Vec<i32> {
            todo!()
        }
    """,
    solution="""
        pub fn take_tail(v: &mut Vec<i32>, at: usize) -> Vec<i32> {
            let at = at.min(v.len());
            v.split_off(at)
        }

        pub fn replace_range(v: &mut Vec<i32>, start: usize, end: usize, with: &[i32]) -> Vec<i32> {
            v.splice(start..end, with.iter().copied()).collect()
        }
    """,
    visible=[
        T("tail", "v = [1, 2, 3, 4], at = 2", "{ let mut v = vec![1, 2, 3, 4]; let t = take_tail(&mut v, 2); (v, t) }", "(vec![1, 2], vec![3, 4])"),
        T("splice", "v = [1, 2, 3, 4], replace 1..3 with [9]", "{ let mut v = vec![1, 2, 3, 4]; let r = replace_range(&mut v, 1, 3, &[9]); (v, r) }", "(vec![1, 9, 4], vec![2, 3])"),
        T("tail_at_zero", "v = [1, 2], at = 0", "{ let mut v = vec![1, 2]; let t = take_tail(&mut v, 0); (v, t) }", "(vec![], vec![1, 2])"),
        T("tail_at_len", "v = [1, 2], at = 2", "{ let mut v = vec![1, 2]; let t = take_tail(&mut v, 2); (v, t) }", "(vec![1, 2], vec![])"),
        T("splice_delete", "v = [1, 2, 3, 4], replace 1..3 with []", "{ let mut v = vec![1, 2, 3, 4]; let r = replace_range(&mut v, 1, 3, &[]); (v, r) }", "(vec![1, 4], vec![2, 3])"),
    ],
    hidden=[
        T("tail_past_end", "v = [1], at = 5", "{ let mut v = vec![1]; let t = take_tail(&mut v, 5); (v, t) }", "(vec![1], vec![])"),
        T("splice_insert", "v = [1, 4], replace 1..1 with [2, 3]", "{ let mut v = vec![1, 4]; let r = replace_range(&mut v, 1, 1, &[2, 3]); (v, r) }", "(vec![1, 2, 3, 4], vec![])"),
        T("tail_of_empty", "v = [], at = 0", "{ let mut v: Vec<i32> = vec![]; let t = take_tail(&mut v, 0); (v, t) }", "(vec![], vec![])"),
        T("tail_huge_at", "v = [1, 2], at = usize::MAX", "{ let mut v = vec![1, 2]; let t = take_tail(&mut v, usize::MAX); (v, t) }", "(vec![1, 2], vec![])"),
        T("splice_whole", "v = [1, 2, 3], replace 0..3 with [7]", "{ let mut v = vec![1, 2, 3]; let r = replace_range(&mut v, 0, 3, &[7]); (v, r) }", "(vec![7], vec![1, 2, 3])"),
        T("splice_longer", "v = [1, 2], replace 0..1 with [5, 6, 7]", "{ let mut v = vec![1, 2]; let r = replace_range(&mut v, 0, 1, &[5, 6, 7]); (v, r) }", "(vec![5, 6, 7, 2], vec![1])"),
        T("splice_at_end", "v = [1], replace 1..1 with [2]", "{ let mut v = vec![1]; let r = replace_range(&mut v, 1, 1, &[2]); (v, r) }", "(vec![1, 2], vec![])"),
        T("splice_empty_vec", "v = [], replace 0..0 with [1, 2]", "{ let mut v = vec![]; let r = replace_range(&mut v, 0, 0, &[1, 2]); (v, r) }", "(vec![1, 2], vec![])"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2309);
            for _ in 0..300 {
                let n = rng.below(8);
                let v: Vec<i32> = rng.vec(n, 0, 9);
                let at = rng.below(10);
                let start = rng.below(n + 1);
                let end = start + rng.below(n - start + 1);
                let wl = rng.below(4);
                let with: Vec<i32> = rng.vec(wl, 10, 19);
                let cut = at.min(n);
                let want_tail = (v[..cut].to_vec(), v[cut..].to_vec());
                let want_splice = ([&v[..start], &with[..], &v[end..]].concat(), v[start..end].to_vec());
                let (mut a, mut b) = (v.clone(), v.clone());
                let t = take_tail(&mut a, at);
                let r = replace_range(&mut b, start, end, &with);
                check!(format!("v = {v:?}, at = {at}, replace {start}..{end} with {with:?}"), ((a, t), (b, r)), (want_tail, want_splice));
            }
        }

        #[test]
        fn scale_200k() {
            let mut v: Vec<i32> = (0..200_000).collect();
            let r = replace_range(&mut v, 1, 199_999, &[-1]);
            let t = take_tail(&mut v, 1);
            check!("v = 0..200000, replace 1..199999 with [-1], then take_tail(1)", (v, t, r.len()), (vec![0], vec![-1, 199_999], 199_998));
        }
        """,
    ],
    wrong=dict(
        tail_not_clamped="""
            pub fn take_tail(v: &mut Vec<i32>, at: usize) -> Vec<i32> {
                v.drain(at..).collect()
            }

            pub fn replace_range(v: &mut Vec<i32>, start: usize, end: usize, with: &[i32]) -> Vec<i32> {
                v.splice(start..end, with.iter().copied()).collect()
            }
        """,
        end_inclusive="""
            pub fn take_tail(v: &mut Vec<i32>, at: usize) -> Vec<i32> {
                let at = at.min(v.len());
                v.split_off(at)
            }

            pub fn replace_range(v: &mut Vec<i32>, start: usize, end: usize, with: &[i32]) -> Vec<i32> {
                let end = (end + 1).min(v.len());
                v.splice(start..end.max(start), with.iter().copied()).collect()
            }
        """,
        returns_the_front="""
            pub fn take_tail(v: &mut Vec<i32>, at: usize) -> Vec<i32> {
                let at = at.min(v.len());
                let tail = v.split_off(at);
                std::mem::replace(v, tail)
            }

            pub fn replace_range(v: &mut Vec<i32>, start: usize, end: usize, with: &[i32]) -> Vec<i32> {
                v.splice(start..end, with.iter().copied()).collect()
            }
        """,
    ),
    hints=[("rust", "`split_off` panics when `at > len`; clamp it."), ("rust", "`splice` returns an iterator of the removed items.")],
    notes=("`splice` shifts the tail once, however long the replacement is. The removed items are yielded as the iterator is consumed, which is why it's collected immediately.", "O(n)", "O(n)"),
    follow_up="What happens to the Vec if you drop the `Splice` iterator without consuming it?",
    related=["S6"],
))

P.append(dict(
    slug="chunks-and-windows", title="chunks and windows", level="medium", stage="understand-it", tags=["chunks", "windows"],
    teaches=["`chunks` for non-overlapping groups, `windows` for overlapping ones.", "`max()` on an iterator returns an `Option`."],
    statement="""
        Write `chunk_sums`, the sum of each consecutive group of `size` (the last may be shorter), and
        `max_window_sum`, the largest sum of `k` consecutive values or `None` if `v` is shorter than `k`.
        `size` and `k` are at least 1.
    """,
    starter="""
        pub fn chunk_sums(v: &[i32], size: usize) -> Vec<i32> {
            todo!()
        }

        pub fn max_window_sum(v: &[i32], k: usize) -> Option<i32> {
            todo!()
        }
    """,
    solution="""
        pub fn chunk_sums(v: &[i32], size: usize) -> Vec<i32> {
            v.chunks(size).map(|c| c.iter().sum()).collect()
        }

        pub fn max_window_sum(v: &[i32], k: usize) -> Option<i32> {
            v.windows(k).map(|w| w.iter().sum()).max()
        }
    """,
    visible=[
        T("chunks", "v = [1, 2, 3, 4, 5], size = 2", "chunk_sums(&[1, 2, 3, 4, 5], 2)", "vec![3, 7, 5]"),
        T("windows", "v = [1, -2, 3, 4, -1], k = 2", "max_window_sum(&[1, -2, 3, 4, -1], 2)", "Some(7)"),
        T("leetcode_643", "v = [1, 12, -5, -6, 50, 3], k = 4", "max_window_sum(&[1, 12, -5, -6, 50, 3], 4)", "Some(51)"),
        T("window_is_whole_slice", "v = [5], k = 1", "max_window_sum(&[5], 1)", "Some(5)"),
        T("chunk_bigger_than_v", "v = [1, 2], size = 5", "chunk_sums(&[1, 2], 5)", "vec![3]"),
    ],
    hidden=[
        T("window_too_big", "v = [1, 2], k = 3", "max_window_sum(&[1, 2], 3)", "None"),
        T("chunks_empty", "v = [], size = 3", "chunk_sums(&[], 3)", "Vec::<i32>::new()"),
        T("window_on_empty", "v = [], k = 1", "max_window_sum(&[], 1)", "None"),
        T("all_negative_windows", "v = [-3, -1, -2], k = 2", "max_window_sum(&[-3, -1, -2], 2)", "Some(-3)"),
        T("chunk_size_one", "v = [4, -5], size = 1", "chunk_sums(&[4, -5], 1)", "vec![4, -5]"),
        T("exact_multiple", "v = [1, 2, 3, 4], size = 2", "chunk_sums(&[1, 2, 3, 4], 2)", "vec![3, 7]"),
        T("k_equals_len", "v = [1, 2, 3], k = 3", "max_window_sum(&[1, 2, 3], 3)", "Some(6)"),
        T("best_window_last", "v = [1, 1, 1, 9, 9], k = 2", "max_window_sum(&[1, 1, 1, 9, 9], 2)", "Some(18)"),
        T("extremes", "v = [i32::MAX, i32::MIN], size = 1, k = 1", "(chunk_sums(&[i32::MAX, i32::MIN], 1), max_window_sum(&[i32::MIN, i32::MAX], 1))", "(vec![i32::MAX, i32::MIN], Some(i32::MAX))"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2310);
            for _ in 0..300 {
                let n = rng.below(10);
                let v: Vec<i32> = rng.vec(n, -50, 50);
                let size = rng.below(4) + 1;
                let k = rng.below(5) + 1;
                let mut want_chunks = Vec::new();
                let mut i = 0;
                while i < n {
                    want_chunks.push(v[i..(i + size).min(n)].iter().sum::<i32>());
                    i += size;
                }
                let mut want_max: Option<i32> = None;
                for s in 0..n {
                    if s + k <= n {
                        let sum: i32 = v[s..s + k].iter().sum();
                        want_max = Some(want_max.map_or(sum, |m| m.max(sum)));
                    }
                }
                check!(format!("v = {v:?}, size = {size}, k = {k}"), (chunk_sums(&v, size), max_window_sum(&v, k)), (want_chunks, want_max));
            }
        }

        #[test]
        fn scale_200k() {
            let v: Vec<i32> = (0..200_000).map(|i| i % 7 - 3).collect();
            let sums = chunk_sums(&v, 3);
            check!("v = 200000 values, size = 3, k = 2", (sums.len(), sums[0], max_window_sum(&v, 2)), (66_667, -6, Some(5)));
        }
        """,
    ],
    wrong=dict(
        max_from_zero="""
            pub fn chunk_sums(v: &[i32], size: usize) -> Vec<i32> {
                v.chunks(size).map(|c| c.iter().sum()).collect()
            }

            pub fn max_window_sum(v: &[i32], k: usize) -> Option<i32> {
                if v.len() < k {
                    return None;
                }
                Some(v.windows(k).map(|w| w.iter().sum()).fold(0, i32::max))
            }
        """,
        drops_short_chunk="""
            pub fn chunk_sums(v: &[i32], size: usize) -> Vec<i32> {
                v.chunks_exact(size).map(|c| c.iter().sum()).collect()
            }

            pub fn max_window_sum(v: &[i32], k: usize) -> Option<i32> {
                v.windows(k).map(|w| w.iter().sum()).max()
            }
        """,
        zero_when_too_short="""
            pub fn chunk_sums(v: &[i32], size: usize) -> Vec<i32> {
                v.chunks(size).map(|c| c.iter().sum()).collect()
            }

            pub fn max_window_sum(v: &[i32], k: usize) -> Option<i32> {
                Some(v.windows(k).map(|w| w.iter().sum()).max().unwrap_or(0))
            }
        """,
    ),
    hints=[("rust", "Both are slice methods that return iterators of sub-slices.")],
    notes=("This `max_window_sum` is O(n·k); a running sum that adds the new element and subtracts the old one is O(n).", "O(n·k)", "O(1)"),
    follow_up="Rewrite `max_window_sum` in O(n).",
    related=["D2"],
))

P.append(dict(
    slug="split-at-mut", title="split_at_mut for two halves", level="medium", stage="understand-it", tags=["split_at_mut", "E0499"],
    teaches=["`split_at_mut` gives two non-overlapping `&mut` into one slice.", "Why `&mut v[..m]` and `&mut v[m..]` at once doesn't compile."],
    statement="Add each value in the first half of `v` to the matching value in the second half: `v[mid + i] += v[i]` for `i < len / 2`.",
    examples=[("v = [1, 2, 10, 20]", "[1, 2, 11, 22]")],
    starter="""
        pub fn add_halves(v: &mut [i32]) {
            todo!()
        }
    """,
    solution="""
        pub fn add_halves(v: &mut [i32]) {
            let mid = v.len() / 2;
            let (first, second) = v.split_at_mut(mid);
            for (a, b) in first.iter().zip(second.iter_mut()) {
                *b += *a;
            }
        }
    """,
    visible=[
        T("even", "v = [1, 2, 10, 20]", "{ let mut v = [1, 2, 10, 20]; add_halves(&mut v); v }", "[1, 2, 11, 22]"),
        T("odd", "v = [1, 5, 5]", "{ let mut v = [1, 5, 5]; add_halves(&mut v); v }", "[1, 6, 5]"),
        T("two", "v = [1, 2]", "{ let mut v = [1, 2]; add_halves(&mut v); v }", "[1, 3]"),
        T("five", "v = [1, 2, 3, 4, 5] (mid = 2, the last value is untouched)", "{ let mut v = [1, 2, 3, 4, 5]; add_halves(&mut v); v }", "[1, 2, 4, 6, 5]"),
        T("negatives", "v = [-1, -2, 1, 2]", "{ let mut v = [-1, -2, 1, 2]; add_halves(&mut v); v }", "[-1, -2, 0, 0]"),
    ],
    hidden=[
        T("empty", "v = []", "{ let mut v: [i32; 0] = []; add_halves(&mut v); v }", "[]"),
        T("one", "v = [4]", "{ let mut v = [4]; add_halves(&mut v); v }", "[4]"),
        T("three", "v = [1, 2, 3]", "{ let mut v = [1, 2, 3]; add_halves(&mut v); v }", "[1, 3, 3]"),
        T("zeros", "v = [0, 0, 0, 0]", "{ let mut v = [0; 4]; add_halves(&mut v); v }", "[0; 4]"),
        T("big_values", "v = [1000000000, 1000000000]", "{ let mut v = [1_000_000_000, 1_000_000_000]; add_halves(&mut v); v }", "[1_000_000_000, 2_000_000_000]"),
        T("first_half_unchanged", "v = [7, 8, 9, 1, 2, 3]", "{ let mut v = [7, 8, 9, 1, 2, 3]; add_halves(&mut v); v }", "[7, 8, 9, 8, 10, 12]"),
        T("extremes", "v = [i32::MIN, 0, i32::MAX, 0]", "{ let mut v = [i32::MIN, 0, i32::MAX, 0]; add_halves(&mut v); v }", "[i32::MIN, 0, -1, 0]"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2311);
            for _ in 0..300 {
                let n = rng.below(10);
                let v: Vec<i32> = rng.vec(n, -100, 100);
                let mid = n / 2;
                let mut want = v.clone();
                for i in 0..mid {
                    want[mid + i] += v[i];
                }
                let mut got = v.clone();
                add_halves(&mut got);
                check!(format!("v = {v:?}"), got, want);
            }
        }

        #[test]
        fn scale_200k() {
            let mut v = vec![1; 200_001];
            add_halves(&mut v);
            check!("v = [1; 200001]", (v[99_999], v[100_000], v[199_999], v[200_000]), (1, 2, 2, 1));
        }
        """,
    ],
    wrong=dict(
        mid_rounds_up="""
            pub fn add_halves(v: &mut [i32]) {
                let mid = (v.len() + 1) / 2;
                let (first, second) = v.split_at_mut(mid);
                for (a, b) in first.iter().zip(second.iter_mut()) {
                    *b += *a;
                }
            }
        """,
        adds_into_first="""
            pub fn add_halves(v: &mut [i32]) {
                let mid = v.len() / 2;
                let (first, second) = v.split_at_mut(mid);
                for (a, b) in first.iter_mut().zip(second.iter()) {
                    *a += *b;
                }
            }
        """,
        mirrored="""
            pub fn add_halves(v: &mut [i32]) {
                let n = v.len();
                for i in 0..n / 2 {
                    v[n - 1 - i] += v[i];
                }
            }
        """,
    ),
    hints=[("rust", "Two `&mut` borrows of `v` at once won't compile, even to different ranges. Which slice method splits one borrow into two?")],
    notes=("`split_at_mut` is safe because the halves provably don't overlap; it's implemented with `unsafe` inside std.", "O(n)", "O(1)"),
    follow_up="How would you get `&mut` to two arbitrary indices i and j?",
    related=["L2"],
))

P.append(dict(
    slug="fix-push-while-reading", title="Fix: push to a Vec while reading it", mode="fix", level="medium", stage="understand-it", tags=["E0502", "extend_from_within"],
    teaches=["You can't push to a Vec while iterating it: a push may reallocate under the iterator.", "`extend_from_within` copies a range of the Vec onto its end in one call."],
    statement="`double_up` should append a copy of every element, so `[1, 2]` becomes `[1, 2, 1, 2]`. It doesn't compile.",
    starter="""
        /// Appends a copy of every element: [1, 2] → [1, 2, 1, 2].
        pub fn double_up(v: &mut Vec<i32>) {
            for x in v.iter() {
                v.push(*x);
            }
        }
    """,
    solution="""
        /// Appends a copy of every element: [1, 2] → [1, 2, 1, 2].
        pub fn double_up(v: &mut Vec<i32>) {
            v.extend_from_within(..);
        }
    """,
    rules=dict(methods=["clone", "to_vec", "to_owned", "collect"]),
    visible=[
        T("two", "v = [1, 2]", "{ let mut v = vec![1, 2]; double_up(&mut v); v }", "vec![1, 2, 1, 2]"),
        T("empty", "v = []", "{ let mut v: Vec<i32> = vec![]; double_up(&mut v); v }", "Vec::<i32>::new()"),
        T("single", "v = [7]", "{ let mut v = vec![7]; double_up(&mut v); v }", "vec![7, 7]"),
        T("three", "v = [1, 2, 3]", "{ let mut v = vec![1, 2, 3]; double_up(&mut v); v }", "vec![1, 2, 3, 1, 2, 3]"),
        T("duplicates", "v = [5, 5]", "{ let mut v = vec![5, 5]; double_up(&mut v); v }", "vec![5, 5, 5, 5]"),
    ],
    hidden=[
        T("big", "v = 0..1000", "{ let mut v: Vec<i32> = (0..1000).collect(); double_up(&mut v); (v.len(), v[1000], v[1999]) }", "(2000, 0, 999)"),
        T("negatives", "v = [-1, -2]", "{ let mut v = vec![-1, -2]; double_up(&mut v); v }", "vec![-1, -2, -1, -2]"),
        T("extremes", "v = [i32::MIN, i32::MAX]", "{ let mut v = vec![i32::MIN, i32::MAX]; double_up(&mut v); v }", "vec![i32::MIN, i32::MAX, i32::MIN, i32::MAX]"),
        T("twice", "v = [1, 2], double_up twice", "{ let mut v = vec![1, 2]; double_up(&mut v); double_up(&mut v); v }", "vec![1, 2, 1, 2, 1, 2, 1, 2]"),
        T("order_kept", "v = [3, 1, 2]", "{ let mut v = vec![3, 1, 2]; double_up(&mut v); v }", "vec![3, 1, 2, 3, 1, 2]"),
        T("single_zero", "v = [0]", "{ let mut v = vec![0]; double_up(&mut v); v }", "vec![0, 0]"),
        T("first_half_untouched", "v = [9, 8, 7, 6]", "{ let mut v = vec![9, 8, 7, 6]; double_up(&mut v); v[..4].to_vec() }", "vec![9, 8, 7, 6]"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2312);
            for _ in 0..300 {
                let n = rng.below(10);
                let v: Vec<i32> = rng.vec(n, -9, 9);
                let want = [&v[..], &v[..]].concat();
                let mut got = v.clone();
                double_up(&mut got);
                check!(format!("v = {v:?}"), got, want);
            }
        }

        #[test]
        fn scale_200k() {
            let mut v: Vec<i32> = (0..200_000).collect();
            double_up(&mut v);
            check!("v = 0..200000", (v.len(), v[199_999], v[200_000], v[399_999]), (400_000, 199_999, 0, 199_999));
        }
        """,
    ],
    wrong=dict(
        each_doubled_in_place="""
            /// Appends a copy of every element: [1, 2] → [1, 2, 1, 2].
            pub fn double_up(v: &mut Vec<i32>) {
                let n = v.len();
                for i in 0..n {
                    let x = v[2 * i];
                    v.insert(2 * i + 1, x);
                }
            }
        """,
        appended_reversed="""
            /// Appends a copy of every element: [1, 2] → [1, 2, 1, 2].
            pub fn double_up(v: &mut Vec<i32>) {
                for i in (0..v.len()).rev() {
                    let x = v[i];
                    v.push(x);
                }
            }
        """,
    ),
    hints=[("rust", "The loop holds a shared borrow of `v` while `push` needs a mutable one (E0502)."),
           ("rust", "`Vec` has a method that appends a copy of a range of itself, with no temporary Vec.")],
    notes=("If the loop compiled, `push` could reallocate and leave the iterator pointing at freed memory; that's exactly what the borrow checker prevents.", "O(n)", "O(n)"),
    follow_up="What would go wrong in C++ with the same loop?",
    related=["L2"],
))

P.append(dict(
    slug="flat-grid", title="A 2-D grid in one Vec", level="medium", stage="understand-it", tags=["strides", "Vec<u8>"],
    teaches=["Row-major indexing: `y * width + x`.", "Returning `&[u8]` rows straight from the backing Vec."],
    statement="""
        Store a `w × h` grid of bytes in one `Vec<u8>`. `get` and `set` return `None` / `false` out of
        bounds; `row(y)` returns that row as a slice.
    """,
    starter="""
        pub struct Grid {
            w: usize,
            h: usize,
            cells: Vec<u8>,
        }

        impl Grid {
            pub fn new(w: usize, h: usize) -> Self {
                todo!()
            }

            pub fn get(&self, x: usize, y: usize) -> Option<u8> {
                todo!()
            }

            pub fn set(&mut self, x: usize, y: usize, value: u8) -> bool {
                todo!()
            }

            pub fn row(&self, y: usize) -> Option<&[u8]> {
                todo!()
            }
        }
    """,
    solution=GRID,
    visible=[
        T("set_and_get", "3×2 grid, set (2, 1) = 7", "{ let mut g = Grid::new(3, 2); g.set(2, 1, 7); (g.get(2, 1), g.get(0, 0)) }", "(Some(7), Some(0))"),
        T("row", "3×2 grid, set (1, 1) = 5", "g.row(1)", "Some(&[0, 5, 0][..])", setup="let mut g = Grid::new(3, 2);\ng.set(1, 1, 5);"),
        T("out_of_bounds", "3×2 grid", "{ let mut g = Grid::new(3, 2); (g.get(3, 0), g.set(0, 2, 1), g.row(2).is_none()) }", "(None, false, true)"),
        T("fresh_row", "2×3 grid, row 0", "Grid::new(2, 3).row(0).map(|r| r.to_vec())", "Some(vec![0, 0])"),
        T("set_in_bounds", "2×2 grid, set (1, 0) = 4", "{ let mut g = Grid::new(2, 2); (g.set(1, 0, 4), g.get(1, 0), g.get(0, 1)) }", "(true, Some(4), Some(0))"),
    ],
    hidden=[
        T("empty_grid", "0×0 grid", "{ let g = Grid::new(0, 0); (g.get(0, 0), g.row(0).is_none()) }", "(None, true)"),
        T("x_not_wrapping", "2×2 grid, get (2, 0)", "{ let mut g = Grid::new(2, 2); g.set(0, 1, 9); g.get(2, 0) }", "None"),
        T("one_by_one", "1×1 grid", "{ let mut g = Grid::new(1, 1); (g.set(0, 0, 3), g.get(0, 0), g.row(0).map(|r| r.to_vec())) }", "(true, Some(3), Some(vec![3]))"),
        T("wide", "5×1 grid, set (4, 0) = 9", "{ let mut g = Grid::new(5, 1); g.set(4, 0, 9); g.row(0).map(|r| r.to_vec()) }", "Some(vec![0, 0, 0, 0, 9])"),
        T("tall", "1×5 grid, set (0, 4) = 9", "{ let mut g = Grid::new(1, 5); g.set(0, 4, 9); (g.row(4).map(|r| r.to_vec()), g.get(0, 3)) }", "(Some(vec![9]), Some(0))"),
        T("row_major_not_column_major", "3×2 grid, set (2, 0) = 5", "{ let mut g = Grid::new(3, 2); g.set(2, 0, 5); (g.row(0).map(|r| r.to_vec()), g.row(1).map(|r| r.to_vec())) }", "(Some(vec![0, 0, 5]), Some(vec![0, 0, 0]))"),
        T("overwrite", "2×2 grid, set (1, 1) = 1 then 2", "{ let mut g = Grid::new(2, 2); g.set(1, 1, 1); g.set(1, 1, 2); g.get(1, 1) }", "Some(2)"),
        T("huge_coordinates", "2×2 grid, x or y = usize::MAX", "{ let mut g = Grid::new(2, 2); (g.get(usize::MAX, 0), g.get(0, usize::MAX), g.set(usize::MAX, 1, 1), g.row(usize::MAX).is_none()) }", "(None, None, false, true)"),
        T("zero_width_rows", "0×3 grid", "{ let g = Grid::new(0, 3); (g.get(0, 0), g.row(2).map(|r| r.len())) }", "(None, Some(0))"),
        """
        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(2313);
            for _ in 0..200 {
                let (w, h) = (rng.below(5), rng.below(5));
                let mut g = Grid::new(w, h);
                let mut model = vec![vec![0u8; w]; h];
                let mut ops = Vec::new();
                for _ in 0..10 {
                    let (x, y, value) = (rng.below(6), rng.below(6), rng.below(256) as u8);
                    ops.push((x, y, value));
                    let ok = x < w && y < h;
                    if ok {
                        model[y][x] = value;
                    }
                    check!(format!("{w}×{h} grid, sets {ops:?}"), g.set(x, y, value), ok);
                }
                for y in 0..6 {
                    for x in 0..6 {
                        let want = if x < w && y < h { Some(model[y][x]) } else { None };
                        check!(format!("{w}×{h} grid, sets {ops:?}, get({x}, {y})"), g.get(x, y), want);
                    }
                    check!(format!("{w}×{h} grid, sets {ops:?}, row({y})"), g.row(y).map(|r| r.to_vec()), model.get(y).cloned());
                }
            }
        }

        #[test]
        fn big_grid() {
            let mut g = Grid::new(1000, 1000);
            g.set(999, 0, 1);
            g.set(0, 999, 2);
            g.set(999, 999, 3);
            check!("1000×1000 grid, corners set", (g.get(999, 0), g.get(0, 999), g.row(999).map(|r| r[999]), g.get(1000, 999)), (Some(1), Some(2), Some(3), None));
        }
        """,
    ],
    wrong=dict(
        no_x_check=sub(GRID, ("(x < self.w && y < self.h).then(|| y * self.w + x)", "let i = y * self.w + x;\n        (y < self.h && i < self.cells.len()).then_some(i)")),
        column_major=sub(GRID, ("(x < self.w && y < self.h).then(|| y * self.w + x)", "(x < self.w && y < self.h).then(|| x * self.h + y)")),
    ),
    hints=[("approach", "Cell (x, y) lives at index y · width + x."), ("edge case", "Check x against the width, or (width, 0) silently reads the next row.")],
    notes=("One allocation and contiguous rows make this cache-friendly compared with `Vec<Vec<u8>>`.", "O(1) per access", "O(w · h)"),
    follow_up="When is `Vec<Vec<T>>` still the better choice?",
    related=["Y1", "D9"],
))

P.append(dict(
    slug="keep-sorted", title="Keep a Vec sorted with partition_point", level="medium", stage="understand-it", tags=["partition_point", "binary search"],
    teaches=["`partition_point` finds the insertion index in O(log n).", "Counting a range with two partition points."],
    statement="`v` is sorted. Write `insert_sorted`, which inserts `x` keeping `v` sorted, and `count_in_range`, how many values lie in `lo..=hi`.",
    starter="""
        pub fn insert_sorted(v: &mut Vec<i32>, x: i32) {
            todo!()
        }

        pub fn count_in_range(v: &[i32], lo: i32, hi: i32) -> usize {
            todo!()
        }
    """,
    solution="""
        pub fn insert_sorted(v: &mut Vec<i32>, x: i32) {
            let i = v.partition_point(|&y| y < x);
            v.insert(i, x);
        }

        pub fn count_in_range(v: &[i32], lo: i32, hi: i32) -> usize {
            let start = v.partition_point(|&y| y < lo);
            let end = v.partition_point(|&y| y <= hi);
            end.saturating_sub(start)
        }
    """,
    visible=[
        T("insert", "v = [1, 3, 5], x = 4", "{ let mut v = vec![1, 3, 5]; insert_sorted(&mut v, 4); v }", "vec![1, 3, 4, 5]"),
        T("count", "v = [1, 2, 2, 3, 7], lo = 2, hi = 3", "count_in_range(&[1, 2, 2, 3, 7], 2, 3)", "3"),
        T("insert_duplicate", "v = [1, 2, 2, 3], x = 2", "{ let mut v = vec![1, 2, 2, 3]; insert_sorted(&mut v, 2); v }", "vec![1, 2, 2, 2, 3]"),
        T("count_none_inside", "v = [1, 5], lo = 2, hi = 4", "count_in_range(&[1, 5], 2, 4)", "0"),
        T("count_bounds_inclusive", "v = [1, 2, 3], lo = 1, hi = 3", "count_in_range(&[1, 2, 3], 1, 3)", "3"),
    ],
    hidden=[
        T("insert_front_and_back", "v = [5], x = 1 then 9", "{ let mut v = vec![5]; insert_sorted(&mut v, 1); insert_sorted(&mut v, 9); v }", "vec![1, 5, 9]"),
        T("empty_range", "v = [1, 2, 3], lo = 5, hi = 1", "count_in_range(&[1, 2, 3], 5, 1)", "0"),
        T("insert_into_empty", "v = [], x = 4", "{ let mut v = vec![]; insert_sorted(&mut v, 4); v }", "vec![4]"),
        T("count_on_empty", "v = [], lo = 0, hi = 9", "count_in_range(&[], 0, 9)", "0"),
        T("lo_equals_hi", "v = [1, 2, 2, 3], lo = 2, hi = 2", "count_in_range(&[1, 2, 2, 3], 2, 2)", "2"),
        T("negatives", "v = [-5, -3, 0], lo = -4, hi = 0", "count_in_range(&[-5, -3, 0], -4, 0)", "2"),
        T("extremes", "v = [i32::MIN, 0, i32::MAX], lo = i32::MIN, hi = i32::MAX", "count_in_range(&[i32::MIN, 0, i32::MAX], i32::MIN, i32::MAX)", "3"),
        T("insert_extremes", "v = [0], x = i32::MAX then i32::MIN", "{ let mut v = vec![0]; insert_sorted(&mut v, i32::MAX); insert_sorted(&mut v, i32::MIN); v }", "vec![i32::MIN, 0, i32::MAX]"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2314);
            for _ in 0..300 {
                let n = rng.below(10);
                let mut v: Vec<i32> = rng.vec(n, -5, 5);
                v.sort();
                let x = rng.int(-6, 6) as i32;
                let (lo, hi) = (rng.int(-6, 6) as i32, rng.int(-6, 6) as i32);
                let want_count = v.iter().filter(|&&y| lo <= y && y <= hi).count();
                let mut want_v = v.clone();
                want_v.push(x);
                want_v.sort();
                let mut got_v = v.clone();
                insert_sorted(&mut got_v, x);
                check!(format!("v = {v:?}, x = {x}, lo = {lo}, hi = {hi}"), (got_v, count_in_range(&v, lo, hi)), (want_v, want_count));
            }
        }

        #[test]
        fn scale_200k_queries() {
            let v: Vec<i32> = (0..200_000).map(|i| i * 2).collect();
            let total: usize = (0..200_000).map(|q| count_in_range(&v, q, q + 10)).sum();
            check!("v = [0, 2, 4, …, 399998], 200000 queries lo = q, hi = q + 10", total, 1_100_000);
        }
        """,
    ],
    wrong=dict(
        linear_count="""
            pub fn insert_sorted(v: &mut Vec<i32>, x: i32) {
                let i = v.partition_point(|&y| y < x);
                v.insert(i, x);
            }

            pub fn count_in_range(v: &[i32], lo: i32, hi: i32) -> usize {
                v.iter().filter(|&&y| lo <= y && y <= hi).count()
            }
        """,
        exclusive_hi="""
            pub fn insert_sorted(v: &mut Vec<i32>, x: i32) {
                let i = v.partition_point(|&y| y < x);
                v.insert(i, x);
            }

            pub fn count_in_range(v: &[i32], lo: i32, hi: i32) -> usize {
                let start = v.partition_point(|&y| y < lo);
                let end = v.partition_point(|&y| y < hi);
                end.saturating_sub(start)
            }
        """,
        unchecked_subtraction="""
            pub fn insert_sorted(v: &mut Vec<i32>, x: i32) {
                let i = v.partition_point(|&y| y < x);
                v.insert(i, x);
            }

            pub fn count_in_range(v: &[i32], lo: i32, hi: i32) -> usize {
                let start = v.partition_point(|&y| y < lo);
                let end = v.partition_point(|&y| y <= hi);
                end - start
            }
        """,
    ),
    hints=[("rust", "`partition_point(|&y| y < x)` returns the first index where the predicate is false.")],
    notes=("The search is O(log n), but `insert` still shifts the tail in O(n). For many insertions, a `BTreeSet` is better.", "O(n) insert, O(log n) count", "O(1)"),
    follow_up="When would you switch from a sorted Vec to a BTreeSet?",
    related=["D4", "S4"],
))

P.append(dict(
    slug="minivec-core", title="MiniVec: push, pop and Drop", level="hard", stage="build-it", tags=["std::alloc", "unsafe", "NonNull"],
    teaches=["What `Vec` is: a pointer, a capacity and a length.", "Growing with `alloc`/`realloc` and freeing in `Drop`.", "Writing a `// SAFETY:` comment for every `unsafe` block."],
    statement="""
        Build `MiniVec<T>` on a raw allocation from `std::alloc`, without using `Vec`. Implement `new`,
        `push`, `pop`, `get`, `len`, `capacity` and `Drop`. Grow from 0 to 4, then double. `Drop` must drop
        every remaining element and free the buffer. You don't need to support zero-sized `T`.
    """,
    starter="""
        use std::alloc::{self, Layout};
        use std::marker::PhantomData;
        use std::ptr::{self, NonNull};

        /// A growable array on a raw allocation. Zero-sized `T` isn't supported.
        pub struct MiniVec<T> {
            ptr: NonNull<T>,
            cap: usize,
            len: usize,
            _owns: PhantomData<T>,
        }

        impl<T> MiniVec<T> {
            pub fn new() -> Self {
                todo!()
            }

            pub fn len(&self) -> usize {
                todo!()
            }

            pub fn capacity(&self) -> usize {
                todo!()
            }

            pub fn push(&mut self, value: T) {
                todo!()
            }

            pub fn pop(&mut self) -> Option<T> {
                todo!()
            }

            pub fn get(&self, i: usize) -> Option<&T> {
                todo!()
            }
        }

        impl<T> Drop for MiniVec<T> {
            fn drop(&mut self) {
                // TODO: drop the remaining elements, then free the buffer.
                // (Left empty rather than todo!(): a panic in drop during a failing test aborts every test.)
            }
        }
    """,
    solution=MINIVEC,
    visible=[
        T("push_pop", "push 1, 2, 3 then pop twice", "{ let mut v = MiniVec::new(); v.push(1); v.push(2); v.push(3); (v.pop(), v.pop(), v.len()) }", "(Some(3), Some(2), 1)"),
        T("growth", "capacity after 0, 1 and 5 pushes", "{ let mut v = MiniVec::new(); let a = v.capacity(); v.push(1u64); let b = v.capacity(); for i in 0..4 { v.push(i); } (a, b, v.capacity()) }", "(0, 4, 8)"),
        T("get", "push \"a\", \"b\"", '{ let mut v = MiniVec::new(); v.push(String::from("a")); v.push(String::from("b")); (v.get(1).cloned(), v.get(2).cloned()) }', '(Some("b".to_string()), None)'),
        T("get_on_empty", "new MiniVec, get(0)", "MiniVec::<i32>::new().get(0).copied()", "None"),
        T("len_tracks_pushes_and_pops", "push 1, 2, 3, pop", "{ let mut v = MiniVec::new(); v.push(1); v.push(2); v.push(3); v.pop(); (v.len(), v.get(1).copied(), v.get(2).copied()) }", "(2, Some(2), None)"),
    ],
    hidden=[
        """
        use std::rc::Rc;

        #[test]
        fn drop_releases_every_element() {
            let counter = Rc::new(());
            {
                let mut v = MiniVec::new();
                for _ in 0..10 {
                    v.push(Rc::clone(&counter));
                }
                v.pop();
            }
            check!("10 Rc clones pushed, one popped, then the MiniVec dropped", Rc::strong_count(&counter), 1);
        }

        #[test]
        fn many_strings_survive_reallocation() {
            let mut v = MiniVec::new();
            for i in 0..1000 {
                v.push(i.to_string());
            }
            check!("push 0..1000 as Strings", (v.len(), v.get(999).cloned()), (1000, Some("999".to_string())));
        }
        """,
        T("pop_empty", "new MiniVec", "MiniVec::<u8>::new().pop()", "None"),
        T("capacity_sequence", "capacity after 1, 4, 5, 8, 9, 16 and 17 pushes", "{ let mut v = MiniVec::new(); let mut caps = vec![]; for i in 1..=17u32 { v.push(i); if [1, 4, 5, 8, 9, 16, 17].contains(&i) { caps.push(v.capacity()); } } caps }", "vec![4, 4, 8, 8, 16, 16, 32]"),
        T("lifo", "push 0..10, then pop everything", "{ let mut v = MiniVec::new(); for i in 0..10 { v.push(i); } let mut out = vec![]; while let Some(x) = v.pop() { out.push(x); } (out, v.len()) }", "((0..10).rev().collect::<Vec<_>>(), 0)"),
        T("pop_then_push", "push 1, 2, pop, push 3", "{ let mut v = MiniVec::new(); v.push(1); v.push(2); v.pop(); v.push(3); (v.len(), v.get(0).copied(), v.get(1).copied()) }", "(2, Some(1), Some(3))"),
        T("get_after_pop", "push 1, pop, get(0)", "{ let mut v = MiniVec::new(); v.push(1); v.pop(); v.get(0).copied() }", "None"),
        """
        #[test]
        fn drop_releases_a_thousand() {
            let counter = Rc::new(());
            {
                let mut v = MiniVec::new();
                for _ in 0..1000 {
                    v.push(Rc::clone(&counter));
                }
                check!("1000 Rc clones inside the MiniVec", Rc::strong_count(&counter), 1001);
            }
            check!("1000 Rc clones pushed, then the MiniVec dropped", Rc::strong_count(&counter), 1);
        }

        #[test]
        fn popped_value_is_owned() {
            let counter = Rc::new(());
            let mut v = MiniVec::new();
            v.push(Rc::clone(&counter));
            let x = v.pop();
            drop(v);
            check!("an Rc popped out, then the MiniVec dropped", Rc::strong_count(&counter), 2);
            drop(x);
        }

        #[test]
        fn random_vs_vec() {
            let mut rng = anneal_prelude::Rng::new(2315);
            for _ in 0..200 {
                let mut ours = MiniVec::new();
                let mut want: Vec<String> = Vec::new();
                let mut ops = Vec::new();
                for _ in 0..rng.below(30) {
                    if rng.below(3) == 0 {
                        ops.push("pop".to_string());
                        check!(format!("{ops:?}"), ours.pop(), want.pop());
                    } else {
                        let s = rng.below(100).to_string();
                        ops.push(format!("push {s}"));
                        ours.push(s.clone());
                        want.push(s);
                    }
                }
                let got: Vec<Option<String>> = (0..want.len() + 1).map(|i| ours.get(i).cloned()).collect();
                let expected: Vec<Option<String>> = want.iter().cloned().map(Some).chain([None]).collect();
                check!(format!("{ops:?}, then get(0..=len)"), (ours.len(), got), (want.len(), expected));
            }
        }

        #[test]
        fn scale_200k() {
            let mut v = MiniVec::new();
            for i in 0..200_000u64 {
                v.push(i);
            }
            let mut sum = 0;
            while let Some(x) = v.pop() {
                sum += x;
            }
            check!("push 0..200000, then pop everything", (sum, v.len(), v.capacity()), (19_999_900_000, 0, 262_144));
        }
        """,
    ],
    wrong=dict(
        grows_by_four=sub(MINIVEC, ("self.cap * 2", "self.cap + 4")),
        leaks_elements=sub(MINIVEC, ("        while self.pop().is_some() {}\n", "")),
    ),
    hints=[("approach", "`new` uses `NonNull::dangling()` and capacity 0; allocate on the first push."),
           ("rust", "`Layout::array::<T>(cap)` describes the buffer; `alloc`, `realloc` and `dealloc` must all get the same layout for the same block."),
           ("rust", "`ptr::write` into uninitialised memory and `ptr::read` out of it; plain assignment would drop garbage.")],
    notes=("`PhantomData<T>` tells the compiler the MiniVec owns `T`s, which matters for drop checking. `Drop` pops everything first so each element's destructor runs exactly once, then frees the buffer.", "O(1) amortised push", "O(n)"),
    follow_up="What would you need to change to support zero-sized types?",
    related=["Y2", "S7", "Y1"],
))

P.append(dict(
    slug="minivec-deref", title="MiniVec: slices and Deref", level="hard", stage="build-it", tags=["Deref", "slice::from_raw_parts"],
    teaches=["Implementing `Deref<Target = [T]>` gives a collection every slice method.", "`slice::from_raw_parts` and its safety contract."],
    statement="""
        `MiniVec` works. Add `as_slice` and `as_mut_slice`, then `Deref` and `DerefMut` to `[T]`,
        so `v.iter()`, `v.sort()` and `&v[1..]` all work on a `MiniVec`.
    """,
    starter=MINIVEC + """
impl<T> MiniVec<T> {
    pub fn as_slice(&self) -> &[T] {
        todo!()
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        todo!()
    }
}

impl<T> std::ops::Deref for MiniVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T> std::ops::DerefMut for MiniVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}
""",
    solution=MINIVEC + DEREF_IMPLS,
    visible=[
        T("iter_sum", "push 1..=4", "{ let mut v = MiniVec::new(); for i in 1..=4 { v.push(i); } v.iter().sum::<i32>() }", "10"),
        T("sort", "push 3, 1, 2", "{ let mut v = MiniVec::new(); v.push(3); v.push(1); v.push(2); v.sort(); v.as_slice().to_vec() }", "vec![1, 2, 3]"),
        T("index", "push 10, 20", "{ let mut v = MiniVec::new(); v.push(10); v.push(20); (v[0], v[1]) }", "(10, 20)"),
        T("slice_length", "push 3 values", "{ let mut v = MiniVec::new(); for i in 0..3 { v.push(i); } (v.as_slice().len(), v.len()) }", "(3, 3)"),
        T("write_through_as_mut_slice", "push 1, 2, then as_mut_slice()[0] = 9", "{ let mut v = MiniVec::new(); v.push(1); v.push(2); v.as_mut_slice()[0] = 9; v.as_slice().to_vec() }", "vec![9, 2]"),
    ],
    hidden=[
        T("empty_slice", "new MiniVec", "MiniVec::<String>::new().as_slice().len()", "0"),
        T("range_index", "push 10, 20, 30", "{ let mut v = MiniVec::new(); v.push(10); v.push(20); v.push(30); v[1..].to_vec() }", "vec![20, 30]"),
        T("mutate_through_slice", "push 1, 2", "{ let mut v = MiniVec::new(); v.push(1); v.push(2); for x in v.iter_mut() { *x *= 5; } v.as_slice().to_vec() }", "vec![5, 10]"),
        T("empty_mut_slice", "new MiniVec", "MiniVec::<u8>::new().as_mut_slice().len()", "0"),
        T("contains_and_search", "push 1, 3, 5", "{ let mut v = MiniVec::new(); for x in [1, 3, 5] { v.push(x); } (v.contains(&3), v.contains(&4), v.binary_search(&5)) }", "(true, false, Ok(2))"),
        T("first_last", "push \"a\", \"b\", \"c\"", '{ let mut v = MiniVec::new(); for s in ["a", "b", "c"] { v.push(s.to_string()); } (v.first().cloned(), v.last().cloned()) }', '(Some("a".to_string()), Some("c".to_string()))'),
        T("reverse", "push 1, 2, 3, then reverse", "{ let mut v = MiniVec::new(); for x in 1..=3 { v.push(x); } v.reverse(); v.to_vec() }", "vec![3, 2, 1]"),
        T("windows_through_deref", "push 1, 2, 3", "{ let mut v = MiniVec::new(); for x in 1..=3 { v.push(x); } v.windows(2).map(|w| w[0] + w[1]).collect::<Vec<_>>() }", "vec![3, 5]"),
        T("after_pop", "push 1, 2, 3, pop", "{ let mut v = MiniVec::new(); for x in 1..=3 { v.push(x); } v.pop(); v.to_vec() }", "vec![1, 2]"),
        """
        #[test]
        fn random_vs_vec() {
            let mut rng = anneal_prelude::Rng::new(2316);
            for _ in 0..200 {
                let n = rng.below(20);
                let values: Vec<i32> = rng.vec(n, -50, 50);
                let mut v = MiniVec::new();
                for &x in &values {
                    v.push(x);
                }
                let mut want = values.clone();
                want.sort();
                v.sort();
                check!(format!("push {values:?}, then sort"), (v.to_vec(), v.len()), (want, n));
            }
        }

        #[test]
        fn scale_200k_sort() {
            let mut v = MiniVec::new();
            for i in 0..200_000u32 {
                v.push(i.wrapping_mul(2_654_435_761));
            }
            v.sort_unstable();
            check!("200000 values, sorted through DerefMut", (v.len(), v.windows(2).all(|w| w[0] <= w[1])), (200_000, true));
        }
        """,
    ],
    wrong=dict(
        drops_the_last=MINIVEC + DEREF_IMPLS.replace("self.len)", "self.len.saturating_sub(1))"),
        always_empty=MINIVEC + """
impl<T> MiniVec<T> {
    pub fn as_slice(&self) -> &[T] {
        &[]
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut []
    }
}

impl<T> std::ops::Deref for MiniVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T> std::ops::DerefMut for MiniVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}
""",
    ),
    hints=[("rust", "`std::slice::from_raw_parts(ptr, len)` builds a slice from a pointer and a length."),
           ("edge case", "Is a dangling pointer OK when len is 0? Read the function's safety section.")],
    notes=("Once `Deref` to `[T]` exists, every slice method works through auto-deref, which is how `Vec` gets `sort`, `iter` and indexing.", "O(1)", "O(1)"),
    follow_up="Why shouldn't you implement Deref for types that aren't smart pointers or containers?",
    related=["S8", "Y2"],
))

P.append(dict(
    slug="small-vec", title="An inline small-vector", level="hard", stage="build-it", tags=["enum", "spill to heap"],
    teaches=["Store up to N items inline and spill to a `Vec` after that.", "Replacing `*self` with a new variant after moving data out."],
    statement="""
        `SmallVec4<T>` keeps up to four items inline, with no heap allocation, and moves them into a
        `Vec` when a fifth arrives. Implement `new`, `push`, `len`, `get` and `is_inline` without `unsafe`.
    """,
    starter="""
        pub enum SmallVec4<T> {
            Inline { items: [Option<T>; 4], len: usize },
            Heap(Vec<T>),
        }

        impl<T> SmallVec4<T> {
            pub fn new() -> Self {
                todo!()
            }

            pub fn push(&mut self, value: T) {
                todo!()
            }

            pub fn len(&self) -> usize {
                todo!()
            }

            pub fn get(&self, i: usize) -> Option<&T> {
                todo!()
            }

            pub fn is_inline(&self) -> bool {
                todo!()
            }
        }
    """,
    solution=SMALLVEC,
    visible=[
        T("stays_inline", "push 4 items", "{ let mut v = SmallVec4::new(); for i in 0..4 { v.push(i); } (v.len(), v.is_inline(), v.get(3).copied()) }", "(4, true, Some(3))"),
        T("spills", "push 5 items", "{ let mut v = SmallVec4::new(); for i in 0..5 { v.push(i * 10); } (v.len(), v.is_inline(), v.get(0).copied(), v.get(4).copied()) }", "(5, false, Some(0), Some(40))"),
        T("new_is_empty", "new SmallVec4", "{ let v = SmallVec4::<i32>::new(); (v.len(), v.is_inline(), v.get(0).copied()) }", "(0, true, None)"),
        T("order_after_spill", "push 0..6", "{ let mut v = SmallVec4::new(); for i in 0..6 { v.push(i); } (0..6).map(|i| v.get(i).copied()).collect::<Vec<_>>() }", "(0..6).map(Some).collect::<Vec<_>>()"),
        T("get_past_len_after_spill", "push 5 items, get(5)", "{ let mut v = SmallVec4::new(); for i in 0..5 { v.push(i); } v.get(5).copied() }", "None"),
    ],
    hidden=[
        T("get_past_len", "push 1 item", "{ let mut v = SmallVec4::new(); v.push('a'); (v.get(1).copied(), v.get(0).copied()) }", "(None, Some('a'))"),
        T("strings", "push 6 Strings", '{ let mut v = SmallVec4::new(); for w in ["a", "b", "c", "d", "e", "f"] { v.push(w.to_string()); } v.get(5).cloned() }', 'Some("f".to_string())'),
        T("three_inline", "push 3 items", "{ let mut v = SmallVec4::new(); for i in 0..3 { v.push(i); } (v.len(), v.is_inline(), v.get(3).copied()) }", "(3, true, None)"),
        T("hundred", "push 0..100", "{ let mut v = SmallVec4::new(); for i in 0..100 { v.push(i); } (v.len(), v.is_inline(), v.get(99).copied(), v.get(100).copied()) }", "(100, false, Some(99), None)"),
        T("spilled_keeps_first_four", "push 5 Strings", '{ let mut v = SmallVec4::new(); for w in ["a", "b", "c", "d", "e"] { v.push(w.to_string()); } (0..5).map(|i| v.get(i).cloned().unwrap_or_default()).collect::<String>() }', '"abcde".to_string()'),
        T("get_past_len_inline", "push 4 items, get(4)", "{ let mut v = SmallVec4::new(); for i in 0..4 { v.push(i); } (v.get(4).copied(), v.is_inline()) }", "(None, true)"),
        """
        use std::rc::Rc;

        #[test]
        fn nothing_leaks_after_spill() {
            let counter = Rc::new(());
            {
                let mut v = SmallVec4::new();
                for _ in 0..6 {
                    v.push(Rc::clone(&counter));
                }
                check!("6 Rc clones inside", Rc::strong_count(&counter), 7);
            }
            check!("6 Rc clones pushed, then the SmallVec4 dropped", Rc::strong_count(&counter), 1);
        }

        #[test]
        fn random_vs_vec() {
            let mut rng = anneal_prelude::Rng::new(2317);
            for _ in 0..200 {
                let n = rng.below(12);
                let values: Vec<i32> = rng.vec(n, -50, 50);
                let mut v = SmallVec4::new();
                for &x in &values {
                    v.push(x);
                }
                let got: Vec<Option<i32>> = (0..n + 1).map(|i| v.get(i).copied()).collect();
                let want: Vec<Option<i32>> = values.iter().copied().map(Some).chain([None]).collect();
                check!(format!("push {values:?}"), (v.len(), v.is_inline(), got), (n, n <= 4, want));
            }
        }

        #[test]
        fn scale_200k() {
            let mut v = SmallVec4::new();
            for i in 0..200_000u32 {
                v.push(i);
            }
            check!("push 0..200000", (v.len(), v.get(3).copied(), v.get(199_999).copied()), (200_000, Some(3), Some(199_999)));
        }
        """,
    ],
    wrong=dict(
        spills_at_four=sub(SMALLVEC, ("if *len < 4 =>", "if *len < 3 =>")),
        loses_the_fifth=sub(SMALLVEC, ("                heap.push(value);\n", "")),
        spills_reversed=sub(SMALLVEC, ("items.iter_mut().filter_map(Option::take)", "items.iter_mut().rev().filter_map(Option::take)")),
    ),
    hints=[("approach", "When the inline array is full, move its items into a new Vec, push the new one, and switch variants."),
           ("rust", "`Option::take` moves each item out of the array and leaves `None`, so nothing is cloned.")],
    notes=("`[Option<T>; 4]` keeps this safe; real small-vectors use `MaybeUninit` to avoid the `Option` tags. Assigning `*self` after the items are moved out is fine because the borrow of `items` has ended.", "O(1) amortised push", "O(n)"),
    follow_up="How does `MaybeUninit<[T; N]>` let you drop the Option tags, and what does it cost in unsafe code?",
    related=["Y1", "S11"],
))

P.append(dict(
    slug="ring-buffer", title="Ring buffer on a boxed slice", level="hard", stage="build-it", tags=["Box<[T]>", "modular index"],
    teaches=["`Box<[T]>` for a fixed-size buffer: no capacity field, no growth.", "Head plus length with wrap-around indexing."],
    statement="""
        `Ring<T>` holds up to `capacity` items. `push` adds to the newest end and, when full, evicts
        and returns the oldest. `iter` yields oldest to newest.

        A value stays in the slot it was written to until it's evicted: `push` writes one slot and moves
        no other value. The tests check this through the addresses of the references `iter` yields.
    """,
    starter="""
        pub struct Ring<T> {
            buf: Box<[Option<T>]>,
            head: usize,
            len: usize,
        }

        impl<T> Ring<T> {
            /// Panics if `capacity` is 0.
            pub fn with_capacity(capacity: usize) -> Self {
                todo!()
            }

            /// Adds `value`, returning the evicted oldest value if the ring was full.
            pub fn push(&mut self, value: T) -> Option<T> {
                todo!()
            }

            pub fn len(&self) -> usize {
                todo!()
            }

            pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
                std::iter::empty() // TODO
            }
        }
    """,
    solution=RING,
    visible=[
        T("not_full", "capacity 3, push 1, 2", "{ let mut r = Ring::with_capacity(3); r.push(1); r.push(2); r.iter().copied().collect::<Vec<_>>() }", "vec![1, 2]"),
        T("evicts_oldest", "capacity 2, push 1, 2, 3", "{ let mut r = Ring::with_capacity(2); r.push(1); r.push(2); let e = r.push(3); (e, r.iter().copied().collect::<Vec<_>>()) }", "(Some(1), vec![2, 3])"),
        T("exactly_full", "capacity 3, push 1, 2, 3", "{ let mut r = Ring::with_capacity(3); let e = (r.push(1), r.push(2), r.push(3)); (e, r.iter().copied().collect::<Vec<_>>()) }", "((None, None, None), vec![1, 2, 3])"),
        T("len_stops_at_capacity", "capacity 2, push 1, 2, 3", "{ let mut r = Ring::with_capacity(2); r.push(1); r.push(2); r.push(3); r.len() }", "2"),
        T("evictions_in_order", "capacity 2, push 1, 2, 3, 4", "{ let mut r = Ring::with_capacity(2); r.push(1); r.push(2); (r.push(3), r.push(4)) }", "(Some(1), Some(2))"),
    ],
    hidden=[
        T("wraps_many_times", "capacity 3, push 0..10", "{ let mut r = Ring::with_capacity(3); for i in 0..10 { r.push(i); } (r.len(), r.iter().copied().collect::<Vec<_>>()) }", "(3, vec![7, 8, 9])"),
        T("capacity_one", "capacity 1, push \"a\", \"b\"", '{ let mut r = Ring::with_capacity(1); r.push("a"); (r.push("b"), r.iter().copied().collect::<Vec<_>>()) }', '(Some("a"), vec!["b"])'),
        T("empty_ring", "capacity 3, nothing pushed", "{ let r = Ring::<u8>::with_capacity(3); (r.len(), r.iter().count()) }", "(0, 0)"),
        T("zero_capacity_panics", "capacity 0", "std::panic::catch_unwind(|| Ring::<u8>::with_capacity(0)).is_err()", "true"),
        T("strings", "capacity 2, push \"a\", \"b\", \"c\"", '{ let mut r = Ring::with_capacity(2); r.push("a".to_string()); r.push("b".to_string()); (r.push("c".to_string()), r.iter().cloned().collect::<Vec<_>>()) }', '(Some("a".to_string()), vec!["b".to_string(), "c".to_string()])'),
        T("big", "capacity 1000, push 0..2500", "{ let mut r = Ring::with_capacity(1000); let evicted = (0..2500).filter_map(|i| r.push(i)).count(); let v: Vec<i32> = r.iter().copied().collect(); (evicted, v.len(), v[0], v[999]) }", "(1500, 1000, 1500, 2499)"),
        T("iter_twice", "capacity 3, push 1..=4, iterate twice", "{ let mut r = Ring::with_capacity(3); for i in 1..=4 { r.push(i); } (r.iter().copied().collect::<Vec<_>>(), r.iter().copied().collect::<Vec<_>>()) }", "(vec![2, 3, 4], vec![2, 3, 4])"),
        """
        #[test]
        fn push_moves_no_other_value() {
            for cap in [2usize, 3, 8] {
                let mut r = Ring::with_capacity(cap);
                let mut before: Vec<(u32, *const u32)> = Vec::new();
                for x in 0..4 * cap as u32 {
                    r.push(x);
                    let now: Vec<(u32, *const u32)> = r.iter().map(|v| (*v, v as *const u32)).collect();
                    for &(value, slot) in &before {
                        if let Some(&(_, new_slot)) = now.iter().find(|&&(v, _)| v == value) {
                            check!(format!("capacity {cap}: after push({x}), is {value} still in the same slot?"), new_slot == slot, true);
                        }
                    }
                    before = now;
                }
            }
        }

        #[test]
        fn random_vs_model() {
            let mut rng = anneal_prelude::Rng::new(2318);
            for _ in 0..300 {
                let cap = rng.below(5) + 1;
                let mut r = Ring::with_capacity(cap);
                let mut model = std::collections::VecDeque::new();
                let mut pushed = Vec::new();
                for _ in 0..rng.below(15) {
                    let x = rng.below(100) as u32;
                    pushed.push(x);
                    model.push_back(x);
                    let want = if model.len() > cap { model.pop_front() } else { None };
                    check!(format!("capacity {cap}, push {pushed:?}"), r.push(x), want);
                }
                check!(format!("capacity {cap}, push {pushed:?}, then iter"), (r.len(), r.iter().copied().collect::<Vec<_>>()), (model.len(), model.into_iter().collect::<Vec<_>>()));
            }
        }

        #[test]
        fn scale_300k_pushes() {
            let mut r = Ring::with_capacity(150_000);
            let mut evicted = 0u64;
            for i in 0..300_000u64 {
                evicted += r.push(i).unwrap_or(0);
            }
            let first = r.iter().next().copied();
            check!("capacity 150000, push 0..300000", (r.len(), first, evicted), (150_000, Some(150_000), 11_249_925_000));
        }
        """,
    ],
    wrong=dict(
        evicts_newest=sub(RING, ("let old = self.buf[self.head].replace(value);\n            self.head = (self.head + 1) % cap;\n            old", "let newest = (self.head + self.len - 1) % cap;\n            self.buf[newest].replace(value)")),
        iter_ignores_head=sub(RING, ("self.buf[(self.head + i) % cap]", "self.buf[i % cap]")),
        shifts_on_evict=sub(RING, ("let old = self.buf[self.head].replace(value);\n            self.head = (self.head + 1) % cap;\n            old", "let old = self.buf[0].take();\n            self.buf.rotate_left(1);\n            self.buf[cap - 1] = Some(value);\n            old")),
    ),
    hints=[("approach", "The newest slot is `(head + len) % capacity`. When full, overwrite `head` and advance it."),
           ("rust", "`Option::replace` puts the new value in and hands back the old one.")],
    notes=("`Box<[T]>` is a pointer and a length: the right type for a buffer that never grows. `iter` borrows `self`, hence the `+ '_`.", "O(1) push", "O(capacity)"),
    follow_up="How would you make this lock-free for one producer and one consumer?",
    related=["S5", "C3"],
))

P.append(dict(
    slug="pairs-iterator", title="A zero-cost iterator over pairs", level="hard", stage="build-it", tags=["Iterator", "lifetimes", "slice patterns"],
    teaches=["An iterator that holds a sub-slice and advances it.", "Item references tied to the slice, not the iterator: `(&'a T, &'a T)`.", "`ExactSizeIterator` via an exact `size_hint`."],
    statement="""
        Write `pairs(v)`, which yields non-overlapping pairs `(&v[0], &v[1])`, `(&v[2], &v[3])`, … and drops
        a trailing odd element. The items must outlive the iterator, and `len()` must work.
    """,
    starter="""
        pub struct Pairs<'a, T> {
            rest: &'a [T],
        }

        pub fn pairs<T>(v: &[T]) -> Pairs<'_, T> {
            Pairs { rest: v }
        }

        impl<'a, T> Iterator for Pairs<'a, T> {
            type Item = (&'a T, &'a T);

            fn next(&mut self) -> Option<Self::Item> {
                todo!()
            }

            // TODO: an exact size_hint, so that len() below works.
        }

        impl<T> ExactSizeIterator for Pairs<'_, T> {}
    """,
    solution="""
        pub struct Pairs<'a, T> {
            rest: &'a [T],
        }

        pub fn pairs<T>(v: &[T]) -> Pairs<'_, T> {
            Pairs { rest: v }
        }

        impl<'a, T> Iterator for Pairs<'a, T> {
            type Item = (&'a T, &'a T);

            fn next(&mut self) -> Option<Self::Item> {
                match self.rest {
                    [a, b, rest @ ..] => {
                        self.rest = rest;
                        Some((a, b))
                    }
                    _ => None,
                }
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                let n = self.rest.len() / 2;
                (n, Some(n))
            }
        }

        impl<T> ExactSizeIterator for Pairs<'_, T> {}
    """,
    visible=[
        T("odd_length", "v = [1, 2, 3, 4, 5]", "pairs(&[1, 2, 3, 4, 5]).map(|(a, b)| (*a, *b)).collect::<Vec<_>>()", "vec![(1, 2), (3, 4)]"),
        T("items_outlive_iterator", "v = [\"a\", \"b\"]", "first", '(&"a", &"b")', setup='let v = ["a", "b"];\nlet first = { let mut it = pairs(&v); it.next().unwrap() };'),
        T("even_length", "v = [1, 2, 3, 4]", "pairs(&[1, 2, 3, 4]).map(|(a, b)| (*a, *b)).collect::<Vec<_>>()", "vec![(1, 2), (3, 4)]"),
        T("single", "v = [1]", "pairs(&[1]).count()", "0"),
        T("len_of_five", "v = [1, 2, 3, 4, 5]", "pairs(&[1, 2, 3, 4, 5]).len()", "2"),
    ],
    hidden=[
        T("len", "v = [0; 7]", "pairs(&[0; 7]).len()", "3"),
        T("empty", "v = []", "pairs::<u8>(&[]).next()", "None"),
        T("len_after_next", "v = [0; 7], one next()", "{ let mut it = pairs(&[0; 7]); it.next(); it.len() }", "2"),
        T("size_hint_exact", "v = [0; 6]", "pairs(&[0; 6]).size_hint()", "(3, Some(3))"),
        T("two", "v = [8, 9]", "pairs(&[8, 9]).map(|(a, b)| (*a, *b)).collect::<Vec<_>>()", "vec![(8, 9)]"),
        T("strings", "v = [\"a\", \"b\", \"c\", \"d\", \"e\"]", 'pairs(&["a", "b", "c", "d", "e"]).map(|(a, b)| format!("{a}{b}")).collect::<Vec<_>>()', 'vec!["ab", "cd"]'),
        T("next_after_end", "v = [1, 2]", "{ let mut it = pairs(&[1, 2]); it.next(); (it.next(), it.next(), it.len()) }", "(None, None, 0)"),
        T("refs_point_into_v", "v = [1, 2]", "{ let (a, b) = pairs(&v).next().unwrap(); (std::ptr::eq(a, &v[0]), std::ptr::eq(b, &v[1])) }", "(true, true)", setup="let v = [1, 2];"),
        """
        #[test]
        fn random_vs_brute_force() {
            let mut rng = anneal_prelude::Rng::new(2319);
            for _ in 0..300 {
                let n = rng.below(10);
                let v: Vec<i32> = rng.vec(n, -9, 9);
                let want: Vec<(i32, i32)> = (0..n / 2).map(|i| (v[2 * i], v[2 * i + 1])).collect();
                let it = pairs(&v);
                let len = it.len();
                check!(format!("v = {v:?}"), (len, it.map(|(a, b)| (*a, *b)).collect::<Vec<_>>()), (n / 2, want));
            }
        }

        #[test]
        fn scale_200k() {
            let v: Vec<u32> = (0..200_001).collect();
            let sum: u64 = pairs(&v).map(|(a, b)| (*a + *b) as u64).sum();
            check!("v = 0..200001", (pairs(&v).len(), sum), (100_000, 19_999_900_000));
        }
        """,
    ],
    wrong=dict(
        overlapping="""
            pub struct Pairs<'a, T> {
                rest: &'a [T],
            }

            pub fn pairs<T>(v: &[T]) -> Pairs<'_, T> {
                Pairs { rest: v }
            }

            impl<'a, T> Iterator for Pairs<'a, T> {
                type Item = (&'a T, &'a T);

                fn next(&mut self) -> Option<Self::Item> {
                    match self.rest {
                        [a, b, ..] => {
                            self.rest = &self.rest[1..];
                            Some((a, b))
                        }
                        _ => None,
                    }
                }

                fn size_hint(&self) -> (usize, Option<usize>) {
                    let n = self.rest.len().saturating_sub(1);
                    (n, Some(n))
                }
            }

            impl<T> ExactSizeIterator for Pairs<'_, T> {}
        """,
        size_hint_rounds_up="""
            pub struct Pairs<'a, T> {
                rest: &'a [T],
            }

            pub fn pairs<T>(v: &[T]) -> Pairs<'_, T> {
                Pairs { rest: v }
            }

            impl<'a, T> Iterator for Pairs<'a, T> {
                type Item = (&'a T, &'a T);

                fn next(&mut self) -> Option<Self::Item> {
                    match self.rest {
                        [a, b, rest @ ..] => {
                            self.rest = rest;
                            Some((a, b))
                        }
                        _ => None,
                    }
                }

                fn size_hint(&self) -> (usize, Option<usize>) {
                    let n = (self.rest.len() + 1) / 2;
                    (n, Some(n))
                }
            }

            impl<T> ExactSizeIterator for Pairs<'_, T> {}
        """,
    ),
    hints=[("rust", "A slice pattern `[a, b, rest @ ..]` takes two elements and the remainder in one match."),
           ("rust", "Items borrow from `'a`, the slice's lifetime, so they can outlive the `Pairs` value.")],
    notes=("`Pairs` is one fat pointer; the loop compiles to index arithmetic. std's `chunks_exact(2)` is the library version.", "O(1) per item", "O(1)"),
    follow_up="Why can't a `pairs_mut` returning `(&'a mut T, &'a mut T)` be written the same way without care?",
    related=["S6", "L3"],
))

STAGES = [("use-it", "Use it", "easy"), ("understand-it", "Understand it", "medium"), ("build-it", "Build it", "hard")]

if __name__ == "__main__":
    n = write_track("s3-vec-slices", "S3", "Vec & slices", "S", "core", 3,
                    "Rust's workhorse collection: the API, its cost model, and building one yourself on a raw allocation.",
                    STAGES, P)
    print("S3", n)
