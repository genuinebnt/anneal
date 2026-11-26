//! RECIPE 1025 (std): Vec<T> and VecDeque<T> look-up page: editing, bulk moves, capacity, ring-buffer queues.
//! LEVEL: simple
//! Use in: buffers, work queues, batching, undo stacks, sliding windows, in-place filtering.   Run: cargo run --release --example std_vec_vecdeque_methods
//! COMMON USES: row buffers from a DB, batch flushers, connection pools, job queues (VecDeque), BFS frontiers,
//! "keep the last N" logs, request pipelining, reusing one allocation across loop iterations.
//! METHODS: (everyday first) Vec::new, Vec::with_capacity, Vec::push, Vec::pop, Vec::len, Vec::is_empty, Vec::insert, Vec::remove, Vec::swap_remove,
//! METHODS: Vec::retain, Vec::extend_from_slice, Vec::extend, Vec::drain, Vec::clear, Vec::truncate, Vec::dedup, Vec::as_slice, Vec::as_mut_slice,
//! METHODS: Vec::retain_mut, Vec::dedup_by_key, Vec::dedup_by, Vec::extract_if, Vec::splice, Vec::split_off, Vec::append, Vec::extend_from_within,
//! METHODS: Vec::resize, Vec::resize_with, Vec::pop_if, Vec::push_mut, Vec::insert_mut, Vec::reserve, Vec::reserve_exact, Vec::try_reserve,
//! METHODS: Vec::try_reserve_exact, Vec::shrink_to_fit, Vec::shrink_to, Vec::capacity, Vec::leak, Vec::into_boxed_slice, Vec::spare_capacity_mut,
//! METHODS: Vec::into_flattened, Vec::as_ptr, Vec::as_mut_ptr, std::vec, std::mem::take,
//! METHODS: VecDeque::new, VecDeque::with_capacity, VecDeque::push_back, VecDeque::push_front, VecDeque::pop_back, VecDeque::pop_front,
//! METHODS: VecDeque::front, VecDeque::back, VecDeque::front_mut, VecDeque::back_mut, VecDeque::get, VecDeque::get_mut, VecDeque::len, VecDeque::is_empty,
//! METHODS: VecDeque::iter, VecDeque::iter_mut, VecDeque::drain, VecDeque::pop_front_if, VecDeque::pop_back_if, VecDeque::make_contiguous,
//! METHODS: VecDeque::as_slices, VecDeque::as_mut_slices, VecDeque::rotate_left, VecDeque::rotate_right, VecDeque::range, VecDeque::range_mut,
//! METHODS: VecDeque::binary_search, VecDeque::binary_search_by, VecDeque::binary_search_by_key, VecDeque::partition_point, VecDeque::contains,
//! METHODS: VecDeque::insert, VecDeque::remove, VecDeque::swap, VecDeque::swap_remove_back, VecDeque::swap_remove_front, VecDeque::split_off,
//! METHODS: VecDeque::append, VecDeque::retain, VecDeque::retain_mut, VecDeque::truncate, VecDeque::clear, VecDeque::resize, VecDeque::resize_with,
//! METHODS: VecDeque::push_back_mut, VecDeque::push_front_mut, VecDeque::insert_mut, VecDeque::reserve, VecDeque::reserve_exact, VecDeque::try_reserve,
//! METHODS: VecDeque::try_reserve_exact, VecDeque::shrink_to_fit, VecDeque::shrink_to, VecDeque::capacity
//! DOCS: https://doc.rust-lang.org/std/vec/struct.Vec.html
//! DEFINED IN: alloc (`alloc::vec::Vec`, `alloc::collections::VecDeque`, `vec!`), re-exported by std. Methods reached through
//! Deref to slice (sort, binary_search, chunks, windows...) are core/alloc slice methods: see std_slice_search_sort_binary.
//! NOT COVERED (and why): from_raw_parts / into_raw_parts (1.93) / set_len (unsafe or raw-pointer ownership: note in section 6),
//! peek_mut, push_within_capacity, try_with_capacity, from_fn, into_chunks, recycle, try_remove, split_at_spare_mut and every `_in`
//! allocator method (unstable in 1.98.1); VecDeque::extract_if, splice, extend_from_within, prepend, truncate_front (unstable).
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   build when the size is known            -> Vec::with_capacity(n)
//!   remove at i, keep order (O(n))          -> remove(i)
//!   remove at i, order irrelevant (O(1))    -> swap_remove(i)
//!   delete all items matching a predicate   -> retain / retain_mut (one O(n) pass)
//!   same, but KEEP the deleted items        -> extract_if(.., pred)
//!   move a range out                        -> drain(range)
//!   move the tail out as a new Vec          -> split_off(at)
//!   move everything from another Vec        -> append(&mut other)   (other is left empty, keeps its capacity)
//!   replace a range with other items        -> splice(range, items)
//!   copy a slice onto the end               -> extend_from_slice
//!   remove consecutive duplicates           -> dedup (sort first for global uniqueness)
//!   pop only if the last item qualifies     -> pop_if
//!   take the whole Vec out of &mut          -> std::mem::take
//!   FIFO queue / pop from the front         -> VecDeque::push_back + pop_front (never Vec::remove(0) in a loop)
//!   need one &[T] from a VecDeque           -> make_contiguous (or handle both halves of as_slices)
//!   allocation that may fail                -> try_reserve
//!
//! DESIGN NOTES: Vec is (pointer, capacity, length). push is amortised O(1); the growth factor is explicitly NOT guaranteed.
//! `capacity() >= len()` always holds. Operations that shorten a Vec never shrink the allocation by themselves.
//! VecDeque is a ring buffer over one allocation, so its contents may be split into two slices.
//!
//! Read: https://doc.rust-lang.org/std/vec/struct.Vec.html#guarantees , https://doc.rust-lang.org/std/collections/struct.VecDeque.html ,
//!       https://doc.rust-lang.org/nomicon/vec/vec.html
//! TRY NEXT:
//!   - [S] In section 2 swap `swap_remove` for `remove` in `evict` and predict the order of the survivors before running.
//!   - [S] Print `caps` from section 6 for `Vec<u8>`, `Vec<u64>` and `Vec<[u8; 4096]>`. Is the first non-zero capacity the same? Why must no test depend on it?
//!   - [M] Change `BoundedLog` to return the evicted item from `push`, then use it to maintain a running sum in O(1).
//!   - [M] Time `Vec::remove(0)` against `VecDeque::pop_front` for 10k, 20k, 40k items and explain the two growth curves.
//!   - [A] PROJECT: a batching sink: a VecDeque work queue (this file) feeding a BufWriter (std_bufreader_bufwriter_patterns); flush on size or
//!         injected-clock age, and use `drain(..n)` so one allocation is reused.
use std::collections::{HashSet, VecDeque};

const COVERED: &[&str] = &[
    "new",
    "with_capacity",
    "push",
    "pop",
    "len",
    "is_empty",
    "insert",
    "remove",
    "swap_remove",
    "retain",
    "extend_from_slice",
    "extend",
    "drain",
    "clear",
    "truncate",
    "dedup",
    "as_slice",
    "as_mut_slice",
    "retain_mut",
    "dedup_by_key",
    "dedup_by",
    "extract_if",
    "splice",
    "split_off",
    "append",
    "extend_from_within",
    "resize",
    "resize_with",
    "pop_if",
    "push_mut",
    "insert_mut",
    "reserve",
    "reserve_exact",
    "try_reserve",
    "try_reserve_exact",
    "shrink_to_fit",
    "shrink_to",
    "capacity",
    "leak",
    "into_boxed_slice",
    "spare_capacity_mut",
    "into_flattened",
    "as_ptr",
    "as_mut_ptr",
    "push_back",
    "push_front",
    "pop_back",
    "pop_front",
    "front",
    "back",
    "front_mut",
    "back_mut",
    "get",
    "get_mut",
    "iter",
    "iter_mut",
    "pop_front_if",
    "pop_back_if",
    "make_contiguous",
    "as_slices",
    "as_mut_slices",
    "rotate_left",
    "rotate_right",
    "range",
    "range_mut",
    "binary_search",
    "binary_search_by",
    "binary_search_by_key",
    "partition_point",
    "contains",
    "swap",
    "swap_remove_back",
    "swap_remove_front",
    "push_back_mut",
    "push_front_mut",
];

/// Runs `f`, silencing the panic message, and reports whether it panicked.
fn panics(f: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f).is_err();
    std::panic::set_hook(hook);
    r
}

// ---- 1. Everyday core: an undo stack --------------------------------------------------------------------------
struct Editor {
    text: String,
    undo: Vec<String>,
}
impl Editor {
    fn edit(&mut self, add: &str) {
        self.undo.push(self.text.clone());
        self.text.push_str(add);
    }
    fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.text = prev;
                true
            }
            None => false, // pop on empty is None, never a panic
        }
    }
}

fn section_everyday() {
    let mut e = Editor {
        text: String::new(),
        undo: Vec::new(),
    };
    e.edit("ab");
    e.edit("cd");
    assert_eq!((e.text.as_str(), e.undo.len()), ("abcd", 2));
    assert!(e.undo() && e.text == "ab");
    assert!(e.undo() && e.text.is_empty());
    assert!(!e.undo() && e.undo.is_empty());

    // vec! forms. `vec![value; n]` CLONES value n times (so Vec::with_capacity inside it is a trap: clones have capacity 0).
    assert_eq!(vec![0u8; 3], [0, 0, 0]);
    let trap: Vec<Vec<u8>> = vec![Vec::with_capacity(64); 3];
    assert!(
        trap.iter().filter(|v| v.capacity() == 0).count() >= 2,
        "a cloned empty Vec does not keep the spare capacity"
    );
    let right: Vec<Vec<u8>> = (0..3).map(|_| Vec::with_capacity(64)).collect();
    assert!(right.iter().all(|v| v.capacity() >= 64));

    // insert/remove shift the tail: O(len - index). Panic conditions differ by one: insert allows index == len.
    let mut v = vec![1, 2, 4];
    v.insert(2, 3);
    v.insert(v.len(), 5);
    assert_eq!(v, [1, 2, 3, 4, 5]);
    assert_eq!(v.remove(0), 1);
    assert!(panics(|| {
        vec![1].insert(2, 0);
    }));
    assert!(panics(|| {
        vec![1].remove(1);
    }));
    // get is the non-panicking read; v[i] panics.
    assert_eq!(v.get(99), None);

    // extend takes any IntoIterator; extend_from_slice needs T: Clone and a slice.
    v.extend([6, 7]);
    v.extend_from_slice(&[8]);
    v.truncate(6); // no-op if the new length is >= len; never panics
    v.truncate(100);
    assert_eq!(v.as_slice(), [2, 3, 4, 5, 6, 7]);
    v.as_mut_slice().reverse();
    assert_eq!(v[0], 7);
    let cap = v.capacity();
    v.clear();
    assert_eq!((v.len(), v.capacity()), (0, cap)); // clear keeps the allocation: reuse the buffer
    println!("1. everyday core ok");
}

// ---- 2. Removing: remove vs swap_remove vs retain vs extract_if vs dedup ---------------------------------------
#[derive(Debug, Clone, PartialEq)]
struct Job {
    id: u32,
    done: bool,
    attempts: u8,
}
fn jobs() -> Vec<Job> {
    (1..=6)
        .map(|id| Job {
            id,
            done: id % 2 == 0,
            attempts: 0,
        })
        .collect()
}
fn ids(v: &[Job]) -> Vec<u32> {
    v.iter().map(|j| j.id).collect()
}
fn evict(pool: &mut Vec<u32>, idx: usize) -> u32 {
    pool.swap_remove(idx) // O(1): the LAST element moves into the hole
}

fn section_removing() {
    let mut pool = vec![10, 11, 12, 13];
    assert_eq!(evict(&mut pool, 0), 10);
    assert_eq!(pool, [13, 11, 12]);
    assert!(panics(|| {
        Vec::<u8>::new().swap_remove(0);
    }));

    // retain: keep what matches, order preserved, one pass.
    let mut a = jobs();
    a.retain(|j| !j.done);
    assert_eq!(ids(&a), [1, 3, 5]);

    // retain_mut: mutate and decide in the same pass.
    let mut b = jobs();
    b.retain_mut(|j| {
        j.attempts += 1;
        !j.done
    });
    assert!(b.iter().all(|j| j.attempts == 1));

    // extract_if (since 1.87): like retain with the predicate inverted, and it YIELDS the removed items.
    let mut c = jobs();
    let finished: Vec<Job> = c.extract_if(.., |j| j.done).collect();
    assert_eq!((ids(&c), ids(&finished)), (vec![1, 3, 5], vec![2, 4, 6]));
    // PITFALL: extract_if is lazy. Dropping the iterator early leaves the unvisited elements in the Vec.
    let mut d = jobs();
    let first = d.extract_if(.., |j| j.done).next().unwrap();
    assert_eq!((first.id, ids(&d)), (2, vec![1, 3, 4, 5, 6]));
    // A range restricts which indices are examined.
    let mut e = jobs();
    e.extract_if(..3, |j| j.done).for_each(drop);
    assert_eq!(ids(&e), [1, 3, 4, 5, 6]);

    // dedup removes only CONSECUTIVE repeats.
    let mut f = vec![1, 1, 2, 1, 1];
    f.dedup();
    assert_eq!(f, [1, 2, 1]);
    f.sort_unstable();
    f.dedup();
    assert_eq!(f, [1, 2]);
    // dedup_by_key: one entry per run of equal keys, keeping the FIRST of the run.
    let mut log = vec![(100, "start"), (101, "start-again"), (205, "stop")];
    log.dedup_by_key(|(t, _)| *t / 100);
    assert_eq!(log, [(100, "start"), (205, "stop")]);
    // dedup_by(|cur, prev_kept|): `cur` is the one that gets removed on true, so merge INTO the second argument.
    let mut counts = vec![("a", 1), ("a", 2), ("b", 5), ("a", 1)];
    counts.dedup_by(|cur, kept| {
        let same = cur.0 == kept.0;
        if same {
            kept.1 += cur.1;
        }
        same
    });
    assert_eq!(counts, [("a", 3), ("b", 5), ("a", 1)]);

    // pop_if (since 1.86): inspect the last element and pop it only when the predicate holds.
    let mut stack = vec![1, 2, 3];
    assert_eq!(stack.pop_if(|x| *x % 2 == 1), Some(3));
    assert_eq!(stack.pop_if(|x| *x % 2 == 1), None);
    assert_eq!(stack, [1, 2]);
    println!("2. removing ok");
}

// ---- 3. Bulk moves: drain, split_off, append, splice, extend_from_within ---------------------------------------
fn flush_in_batches(pending: &mut Vec<u32>, size: usize) -> Vec<Vec<u32>> {
    let mut sent = Vec::new();
    while !pending.is_empty() {
        let n = size.min(pending.len()); // drain(..size) would panic when fewer than `size` remain
        sent.push(pending.drain(..n).collect());
    }
    sent
}

fn section_bulk() {
    let mut p = (1..=7).collect::<Vec<u32>>();
    assert_eq!(
        flush_in_batches(&mut p, 3),
        vec![vec![1, 2, 3], vec![4, 5, 6], vec![7]]
    );
    assert!(panics(|| {
        vec![1].drain(..2);
    }));
    // Dropping the Drain removes the whole range even if you consumed nothing.
    let mut v = vec![0, 1, 2, 3, 4];
    drop(v.drain(1..3));
    assert_eq!(v, [0, 3, 4]);

    // split_off(at): self keeps [0, at), the result owns [at, len). Panics if at > len.
    let mut head = vec![1, 2, 3, 4, 5];
    let tail = head.split_off(2);
    assert_eq!(
        (head.as_slice(), tail.as_slice()),
        (&[1, 2][..], &[3, 4, 5][..])
    );
    assert!(head.split_off(2).is_empty()); // at == len is fine
    assert!(panics(|| {
        let _ = vec![1].split_off(2);
    }));

    // append moves every element and leaves the source empty (its allocation stays with it).
    let (mut dst, mut src) = (vec![1], vec![2, 3]);
    let src_cap = src.capacity();
    dst.append(&mut src);
    assert_eq!(
        (dst.as_slice(), src.len(), src.capacity()),
        (&[1, 2, 3][..], 0, src_cap)
    );

    // splice: replace a range with any number of items; returns the removed ones.
    let mut lines = vec!["a", "OLD1", "OLD2", "z"];
    let removed: Vec<_> = lines.splice(1..3, ["new"]).collect();
    assert_eq!(
        (lines.as_slice(), removed),
        (&["a", "new", "z"][..], vec!["OLD1", "OLD2"])
    );
    lines.splice(1..1, ["x", "y"]); // empty range = insert many
    assert_eq!(lines, ["a", "x", "y", "new", "z"]);

    // extend_from_within: duplicate part of itself. `v.extend_from_slice(&v[..])` does not borrow-check.
    let mut r = vec![1, 2, 3];
    r.extend_from_within(..2);
    assert_eq!(r, [1, 2, 3, 1, 2]);
    assert!(panics(|| {
        vec![1].extend_from_within(2..);
    }));

    // resize clones the value; resize_with calls the closure per new slot. Both truncate when shrinking.
    let mut s = vec![1];
    s.resize(3, 9);
    assert_eq!(s, [1, 9, 9]);
    let mut next = 100;
    s.resize_with(5, || {
        next += 1;
        next
    });
    assert_eq!(s, [1, 9, 9, 101, 102]);
    s.resize(1, 0);
    assert_eq!(s, [1]);

    // into_flattened (since 1.80): Vec<[T; N]> -> Vec<T> without copying element by element in your code.
    assert_eq!(vec![[1, 2], [3, 4]].into_flattened(), [1, 2, 3, 4]);

    // mem::take: move the buffer out of a struct field behind &mut, leaving an empty Vec (no allocation).
    struct Batcher {
        buf: Vec<u8>,
    }
    let mut bt = Batcher { buf: vec![1, 2] };
    let out = std::mem::take(&mut bt.buf);
    assert_eq!((out, bt.buf.capacity()), (vec![1, 2], 0));
    println!("3. bulk moves ok");
}

// ---- 4. VecDeque: queues, windows, rings -----------------------------------------------------------------------
/// Keep only the newest `cap` entries.
struct BoundedLog {
    cap: usize,
    items: VecDeque<u64>,
}
impl BoundedLog {
    fn push(&mut self, x: u64) {
        if self.cap == 0 {
            return;
        }
        if self.items.len() == self.cap {
            self.items.pop_front();
        }
        self.items.push_back(x);
    }
}

/// Sliding-window rate limiter: allow at most `max` events in the last `window` ticks. Clock is injected.
struct Limiter {
    window: u64,
    max: usize,
    hits: VecDeque<u64>,
}
impl Limiter {
    fn allow(&mut self, now: u64) -> bool {
        // pop_front_if (since 1.93): expire from the old end without a peek-then-pop-then-unwrap dance.
        while self
            .hits
            .pop_front_if(|t| now.saturating_sub(*t) >= self.window)
            .is_some()
        {}
        if self.hits.len() < self.max {
            self.hits.push_back(now);
            true
        } else {
            false
        }
    }
}

fn section_deque() {
    let mut q: VecDeque<u32> = VecDeque::new();
    assert!(q.is_empty() && q.pop_front().is_none());
    q.push_back(2);
    q.push_back(3);
    q.push_front(1);
    assert_eq!(
        (q.front(), q.back(), q.get(1), q.get(9), q.len()),
        (Some(&1), Some(&3), Some(&2), None, 3)
    );
    *q.front_mut().unwrap() += 10;
    *q.back_mut().unwrap() += 10;
    *q.get_mut(1).unwrap() += 10;
    for x in q.iter_mut() {
        *x -= 10;
    }
    assert!(q.iter().eq([1, 2, 3].iter()));
    assert_eq!(q.pop_back_if(|x| *x == 3), Some(3));
    assert_eq!(q.pop_back(), Some(2));
    assert_eq!(q.pop_front(), Some(1));

    let mut log = BoundedLog {
        cap: 3,
        items: VecDeque::with_capacity(3),
    };
    (1..=10).for_each(|i| log.push(i));
    assert!(log.items.iter().copied().eq(8..=10));

    let mut lim = Limiter {
        window: 10,
        max: 2,
        hits: VecDeque::new(),
    };
    let got: Vec<bool> = [0, 1, 2, 10, 10, 11]
        .iter()
        .map(|&t| lim.allow(t))
        .collect();
    assert_eq!(got, [true, true, false, true, false, true]);

    // Work queue with batch drain.
    let mut work: VecDeque<u32> = (1..=5).collect();
    let batch: Vec<u32> = work.drain(..3.min(work.len())).collect();
    assert_eq!((batch, work.len()), (vec![1, 2, 3], 2));
    println!("4. VecDeque queues ok");
}

fn section_deque_ring() {
    // The ring may wrap: iteration order is logical, memory may be two pieces.
    let mut d: VecDeque<i32> = VecDeque::with_capacity(8);
    d.extend(0..4);
    d.push_front(-1);
    d.push_front(-2);
    let (a, b) = d.as_slices();
    let joined: Vec<i32> = a.iter().chain(b).copied().collect();
    assert_eq!(joined, [-2, -1, 0, 1, 2, 3]); // the two halves in order; WHERE the split falls is unspecified
    let (ma, mb) = d.as_mut_slices();
    ma.iter_mut().chain(mb).for_each(|x| *x *= 10);
    let flat: &mut [i32] = d.make_contiguous(); // may move elements; afterwards all slice methods work
    assert_eq!(flat, [-20, -10, 0, 10, 20, 30]);
    assert!(d.as_slices().1.is_empty());

    // rotate_left(n)/rotate_right(n): panic if n > len; n == len is a no-op. Cost is O(min(n, len - n)).
    let mut ring: VecDeque<u8> = (1..=5).collect();
    ring.rotate_left(2);
    assert!(ring.iter().copied().eq([3, 4, 5, 1, 2]));
    ring.rotate_right(2);
    assert!(ring.iter().copied().eq(1..=5));
    ring.rotate_left(5);
    assert!(panics(|| {
        let mut r: VecDeque<u8> = (1..=5).collect();
        r.rotate_left(6);
    }));

    // range / range_mut and searching (the deque must be sorted by the same key).
    let mut s: VecDeque<(u32, char)> = [(1, 'a'), (3, 'b'), (5, 'c'), (7, 'd')].into();
    assert!(s.range(1..3).map(|p| p.1).eq(['b', 'c']));
    s.range_mut(..1).for_each(|p| p.1 = 'A');
    assert_eq!(s.binary_search(&(5, 'c')), Ok(2));
    assert_eq!(s.binary_search_by(|p| p.0.cmp(&4)), Err(2)); // Err = where to insert
    assert_eq!(s.binary_search_by_key(&7, |p| p.0), Ok(3));
    assert_eq!(s.partition_point(|p| p.0 < 4), 2);
    assert!(s.contains(&(1, 'A')));

    // Positional edits: remove returns Option (unlike Vec::remove, it does not panic).
    let mut m: VecDeque<u8> = (0..6).collect();
    assert_eq!(m.remove(99), None);
    assert_eq!(m.remove(1), Some(1));
    m.insert(1, 1);
    m.swap(0, 5);
    assert_eq!(m.swap_remove_back(0), Some(5)); // hole filled by the back element
    assert_eq!(m.swap_remove_front(4), Some(4)); // hole filled by the front element
    assert!(m.iter().copied().eq([1, 2, 3, 0]));
    let mut back = m.split_off(2);
    m.append(&mut back);
    assert!(back.is_empty() && m.len() == 4);
    m.retain(|x| *x != 2);
    m.retain_mut(|x| {
        *x += 1;
        true
    });
    m.truncate(2);
    assert_eq!(m.len(), 2);
    m.resize(3, 7);
    m.resize_with(4, Default::default);
    assert_eq!((m[2], m[3]), (7, 0));
    m.clear();
    assert!(m.is_empty());
    println!("5. VecDeque ring operations ok");
}

// ---- 6. Capacity and allocation --------------------------------------------------------------------------------
fn section_capacity() {
    assert_eq!(Vec::<u64>::new().capacity(), 0); // new() does not allocate
    let mut v: Vec<u32> = Vec::with_capacity(10);
    assert!(v.capacity() >= 10 && v.is_empty()); // "at least", per the docs
    let before = v.as_ptr();
    v.extend(0..10);
    assert_eq!(v.as_ptr(), before, "no reallocation while len <= capacity");

    // Structural growth experiment. The factor is NOT guaranteed; amortised O(1) push IS.
    let mut g = Vec::new();
    let mut caps = Vec::new();
    for i in 0..100_000u32 {
        g.push(i);
        assert!(g.capacity() >= g.len());
        if caps.last() != Some(&g.capacity()) {
            caps.push(g.capacity());
        }
    }
    println!(
        "   observed capacities (this toolchain): {:?} ... {} reallocation steps",
        &caps[..6.min(caps.len())],
        caps.len()
    );
    assert!(
        caps.len() < 40,
        "amortised O(1) implies only logarithmically many reallocations"
    );

    // reserve(n) means "room for n MORE than len", not "capacity n".
    let mut r = vec![0u8; 5];
    r.reserve(10);
    assert!(r.capacity() >= 15);
    r.reserve_exact(20);
    assert!(r.capacity() >= 25);
    r.shrink_to(8);
    assert!(r.capacity() >= 8);
    r.shrink_to_fit();
    assert!(r.capacity() >= 5);
    // try_reserve: an Err instead of an abort, for sizes that come from the network.
    assert!(Vec::<u8>::new().try_reserve(usize::MAX).is_err());
    assert!(Vec::<u64>::new().try_reserve_exact(usize::MAX).is_err());
    assert!(Vec::<u8>::new().try_reserve(1024).is_ok());

    // spare_capacity_mut: the uninitialised tail as &mut [MaybeUninit<T>]. Writing is safe; set_len afterwards is unsafe.
    let mut sc: Vec<u8> = Vec::with_capacity(4);
    for (i, slot) in sc.spare_capacity_mut().iter_mut().take(3).enumerate() {
        slot.write(i as u8);
    }
    // SAFETY: capacity >= 4 and the first 3 slots were just initialised.
    unsafe { sc.set_len(3) };
    assert_eq!(sc, [0, 1, 2]);
    let p = sc.as_mut_ptr(); // raw pointers are invalidated by any reallocation
    assert_eq!(p.cast_const(), sc.as_ptr());
    // from_raw_parts(ptr, len, cap) rebuilds a Vec; ptr/cap must come from the same allocator with the same layout.
    // Getting that wrong is undefined behaviour, so this page only names it.

    // into_boxed_slice: drops spare capacity, 2 words instead of 3. leak: &'static mut [T] for process-lifetime tables.
    let bx: Box<[u32]> = v.into_boxed_slice();
    assert_eq!(bx.len(), 10);
    assert!(size_of::<Box<[u32]>>() < size_of::<Vec<u32>>());
    let table: &'static mut [u8] = vec![1, 2, 3].leak();
    table[0] = 9;
    assert_eq!(table, [9, 2, 3]);

    // VecDeque has the same capacity family.
    let mut d: VecDeque<u8> = VecDeque::with_capacity(4);
    d.reserve(16);
    d.reserve_exact(32);
    assert!(d.capacity() >= 32);
    d.shrink_to(8);
    assert!(d.capacity() >= 8);
    d.shrink_to_fit();
    assert!(d.try_reserve(10).is_ok() && d.try_reserve_exact(usize::MAX).is_err());
    println!("6. capacity ok");
}

// ---- 7. Less known: the *_mut insertion methods (since 1.95) ---------------------------------------------------
fn section_less_known() {
    // push_mut / insert_mut return &mut to the element just added: no `last_mut().unwrap()`.
    let mut v: Vec<Vec<u8>> = Vec::new();
    v.push_mut(Vec::new()).push(1);
    v.insert_mut(0, Vec::new()).push(2);
    assert_eq!(v, [vec![2], vec![1]]);
    let mut d: VecDeque<String> = VecDeque::new();
    d.push_back_mut(String::new()).push('b');
    d.push_front_mut(String::new()).push('a');
    d.insert_mut(2, String::new()).push('c');
    assert_eq!(d.make_contiguous().concat(), "abc");
    println!("7. less known ok");
}

// ---- 8. Differential test: VecDeque against a Vec model --------------------------------------------------------
fn section_differential() {
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut rnd = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let (mut d, mut model) = (VecDeque::new(), Vec::new());
    for step in 0..20_000u32 {
        match rnd(7) {
            0 => {
                d.push_back(step);
                model.push(step);
            }
            1 => {
                d.push_front(step);
                model.insert(0, step);
            }
            2 => assert_eq!(d.pop_back(), model.pop()),
            3 => assert_eq!(d.pop_front(), (!model.is_empty()).then(|| model.remove(0))),
            4 => {
                let n = rnd(model.len() as u64 + 1) as usize;
                d.rotate_left(n);
                model.rotate_left(n);
            }
            5 => {
                let i = rnd(model.len() as u64 + 1) as usize;
                d.insert(i, step);
                model.insert(i, step);
            }
            _ => {
                let i = rnd(model.len() as u64 + 1) as usize;
                assert_eq!(d.remove(i), (i < model.len()).then(|| model.remove(i)));
            }
        }
        assert_eq!(d.len(), model.len());
        if step % 97 == 0 {
            assert!(d.iter().eq(model.iter()));
        }
    }
    assert!(d.iter().eq(model.iter()));
    println!("8. 20000-step differential test ok");
}

// ---- 9. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Known size: with_capacity (clippy::vec_init_then_push flags new()+push of literals: use vec![..]).
    let n = 1000;
    let mut out = Vec::with_capacity(n);
    let cap = out.capacity();
    out.extend(0..n);
    assert_eq!(out.capacity(), cap);
    // 2. Never size from untrusted input: Vec::with_capacity(header_len) can abort the process. Clamp, or try_reserve.
    let claimed = usize::MAX;
    assert!(Vec::<u8>::with_capacity(claimed.min(64 * 1024)).capacity() >= 64 * 1024);
    // 3. retain instead of remove-in-a-loop: O(n) not O(n^2), and no index bookkeeping bugs.
    let mut v: Vec<u32> = (0..10).collect();
    v.retain(|x| x % 3 != 0);
    assert_eq!(v, [1, 2, 4, 5, 7, 8]);
    // 4. FIFO: VecDeque::pop_front is O(1); Vec::remove(0) is O(n) (see TRY NEXT to measure).
    // 5. Reuse one scratch buffer across iterations: clear(), do not reallocate.
    let mut scratch = Vec::with_capacity(16);
    let first_ptr = scratch.as_ptr();
    for chunk in [[1u8, 2], [3, 4]] {
        scratch.clear();
        scratch.extend_from_slice(&chunk);
    }
    assert_eq!(scratch.as_ptr(), first_ptr);
    // 6. A burst can leave a huge capacity behind in a long-lived buffer: shrink_to(sane_limit) after the burst.
    let mut burst: Vec<u8> = Vec::with_capacity(1 << 20);
    burst.shrink_to(4096);
    assert!(burst.capacity() >= 4096);
    // 7. Accept &[T] in parameters, not &Vec<T> (clippy::ptr_arg).
    fn total(xs: &[u32]) -> u32 {
        xs.iter().sum()
    }
    assert_eq!(total(&v), 27);
    // 8. into_iter() to move items out; iter().cloned() allocates one clone per element (clippy::redundant_clone).
    let owned: Vec<String> = vec!["a".into()];
    let moved: Vec<String> = owned
        .into_iter()
        .map(|mut s| {
            s.push('!');
            s
        })
        .collect();
    assert_eq!(moved, ["a!"]);
    // 9. Check emptiness with is_empty(), not len() == 0 (clippy::len_zero).
    // 10. Do not collect only to iterate or count (clippy::needless_collect): `iter.count()`, `iter.any(..)`.
    assert!((0..10).any(|x| x == 3));
    // 11. Order does not matter? swap_remove. Indices held elsewhere? Remember it MOVED the last element: fix that index.
    let mut slots = vec!["a", "b", "c"];
    let moved_from = slots.len() - 1;
    slots.swap_remove(0);
    assert!(moved_from == 2 && slots[0] == "c");
    // 12. Bound every queue: an unbounded VecDeque between a fast producer and slow consumer is a memory leak.
    fn try_enqueue(q: &mut VecDeque<u8>, max: usize, x: u8) -> Result<(), u8> {
        if q.len() >= max {
            return Err(x); // hand the item back: the caller decides to drop, retry or block
        }
        q.push_back(x);
        Ok(())
    }
    let mut q = VecDeque::new();
    assert_eq!(
        (try_enqueue(&mut q, 1, 1), try_enqueue(&mut q, 1, 2)),
        (Ok(()), Err(2))
    );
    println!("9. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday();
    section_removing();
    section_bulk();
    section_deque();
    section_deque_ring();
    section_capacity();
    section_less_known();
    section_differential();
    section_production_tips();
    println!(
        "covered {} distinct Vec/VecDeque method names; all asserts passed",
        COVERED.len()
    );
}
