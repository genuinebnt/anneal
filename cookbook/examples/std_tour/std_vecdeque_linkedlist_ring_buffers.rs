//! RECIPE 1045 (std-collections): `VecDeque` as the standard ring buffer, when `LinkedList` is justified, and how to choose a priority structure by counting work.
//! LEVEL: intermediate
//! Use in: FIFO queues, bounded history, sliding windows, round-robin dispatch, LRU caches.   Run: cargo run --release --example std_vecdeque_linkedlist_ring_buffers
//! COMMON USES: BFS and work queues; "keep the last N log lines"; undo/redo; sliding-window max; rotating through backends.
//! METHODS: VecDeque::new, VecDeque::with_capacity, VecDeque::from, VecDeque::push_back, VecDeque::push_front, VecDeque::pop_back, VecDeque::pop_front,
//! VecDeque::pop_front_if, VecDeque::pop_back_if, VecDeque::front, VecDeque::back, VecDeque::front_mut, VecDeque::get, VecDeque::len, VecDeque::capacity,
//! VecDeque::as_slices, VecDeque::make_contiguous, VecDeque::rotate_left, VecDeque::rotate_right, VecDeque::range, VecDeque::iter, VecDeque::drain,
//! VecDeque::truncate, VecDeque::split_off, VecDeque::append, VecDeque::extend, VecDeque::retain, VecDeque::retain_mut, VecDeque::remove,
//! VecDeque::insert, VecDeque::swap_remove_back, VecDeque::swap_remove_front, VecDeque::binary_search, VecDeque::partition_point, VecDeque::contains,
//! Vec::from(VecDeque), LinkedList::from, LinkedList::push_back, LinkedList::push_front, LinkedList::pop_front, LinkedList::pop_back,
//! LinkedList::append, LinkedList::split_off, LinkedList::extract_if, LinkedList::contains, LinkedList::iter_mut
//! DOCS: https://doc.rust-lang.org/std/collections/struct.VecDeque.html
//! DEFINED IN: alloc: `VecDeque`, `LinkedList`, `BinaryHeap`, `BTreeSet` (`alloc::collections`, re-exported by std). std: `HashMap`.
//! NOT COVERED (and why): LinkedList cursors (`cursor_front_mut`, splicing in the middle), `LinkedList::{remove, retain}` (unstable in 1.98.1);
//! `VecDeque::truncate_front` (unstable); `VecDeque::resize_with`, `shrink_to_fit`, `try_reserve` (same meaning as on Vec); intrusive and lock-free queues.
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   FIFO queue                                  -> `VecDeque`: `push_back` + `pop_front` (never `Vec::remove(0)`, which is O(n))
//!   keep the last N                             -> `if q.len() == n { q.pop_front(); } q.push_back(x)`
//!   remove the head only if it is expired       -> `pop_front_if(|x| ..)` (1.93)
//!   one contiguous slice (sort, write)          -> `make_contiguous()`; without mutation: handle BOTH halves of `as_slices()`
//!   round-robin                                 -> `rotate_left(1)`, or pop front and push back
//!   search a sorted deque                       -> `binary_search` / `partition_point`
//!   convert to / from Vec                       -> `VecDeque::from(vec)` is O(1), no reallocation; back to Vec may shift data (O(n))
//!   join two long lists without moving items    -> `LinkedList::append` is O(1). `split_off(at)` is NOT: it walks min(at, len-at) nodes
//!   repeatedly take the min/max                 -> `BinaryHeap`; also need remove/update by key: `BTreeSet`; build once, then drain: sort a Vec
//!
//! LinkedList, honestly: std's docs say a Vec or VecDeque is almost always better. Without stable cursors you cannot even splice in the middle.
//! The remaining cases: O(1) concatenation, or elements that must never move. (`Vec<Box<T>>` also keeps the T in place.)
//! TRY NEXT:
//!   - [S] Replace `VecDeque` in `History` by `Vec` + `remove(0)` and count element moves for 10_000 pushes at limit 1_000.
//!   - [S] Predict `as_slices()` for a fresh `VecDeque::from(vec![1,2,3])` and after one `push_front`. Which parts of your prediction are guaranteed?
//!   - [M] Remove the compaction step in `Lru` and write an assertion that shows the queue growing without bound under repeated gets.
//!   - [M] Make `round_robin` weighted (a job of weight w gets w quanta per turn) and keep the oracle test.
//!   - [A] PROJECT: combine with `std_sync_primitives_use_cases`: a bounded MPMC work queue on `Mutex<VecDeque>` + two `Condvar`s, with batch draining via `drain(..k)`.
//! Read: https://doc.rust-lang.org/std/collections/index.html ; Too Many Linked Lists https://rust-unofficial.github.io/too-many-lists/
#![allow(dead_code)]

use std::cell::Cell;
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeSet, BinaryHeap, HashMap, LinkedList, VecDeque};

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

fn items<T: Clone>(q: &VecDeque<T>) -> Vec<T> {
    q.iter().cloned().collect()
}

// ------------------------------------------------------------ 1. Both ends, ring layout
fn s01_basics() {
    let mut q = VecDeque::with_capacity(8);
    q.push_back(2);
    q.push_back(3);
    q.push_front(1);
    assert_eq!(
        (q.front(), q.back(), q.len(), q.get(1)),
        (Some(&1), Some(&3), 3, Some(&2))
    );
    assert_eq!(q[2], 3);
    if let Some(f) = q.front_mut() {
        *f = 10;
    }
    assert_eq!(
        (q.pop_front(), q.pop_back(), q.pop_back(), q.pop_back()),
        (Some(10), Some(3), Some(2), None)
    );

    // steady-state churn within capacity never reallocates
    let mut q: VecDeque<u32> = VecDeque::with_capacity(16);
    q.extend(0..10);
    let cap = q.capacity();
    let mut wrapped = false;
    for i in 10..1_000 {
        q.pop_front();
        q.push_back(i);
        let (a, b) = q.as_slices();
        wrapped |= !b.is_empty();
        assert_eq!(
            [a, b].concat(),
            (i - 9..=i).collect::<Vec<_>>(),
            "the two halves, in order, are the queue"
        );
    }
    assert_eq!(q.capacity(), cap);
    assert!(
        wrapped,
        "on this implementation the ring wraps; WHERE it wraps is not part of the API"
    );
    let slice = q.make_contiguous();
    slice.reverse(); // now every slice method works
    assert_eq!(slice[0], 999);
    assert!(q.as_slices().1.is_empty());

    // From<Vec> is documented as O(1) with no reallocation
    let v: Vec<u8> = Vec::from([1, 2, 3]);
    let p = v.as_ptr();
    let d = VecDeque::from(v);
    assert_eq!(d.as_slices().0.as_ptr(), p);
    let mut d = d;
    d.push_front(0); // may wrap: converting back then moves data into place
    assert_eq!(Vec::from(d), [0, 1, 2, 3]);
    println!("1. ring buffer basics ok");
}

// ------------------------------------------------------------ 2. Editing in the middle and in bulk
fn s02_bulk() {
    let mut q: VecDeque<u32> = (0..10).collect();
    q.rotate_left(3);
    assert_eq!(items(&q), [3, 4, 5, 6, 7, 8, 9, 0, 1, 2]);
    q.rotate_right(3);
    assert_eq!(items(&q), (0..10).collect::<Vec<_>>());
    assert_eq!(q.range(2..5).copied().collect::<Vec<_>>(), [2, 3, 4]);
    assert_eq!(q.drain(..3).collect::<Vec<_>>(), [0, 1, 2]); // batch dequeue
    q.truncate(5); // keeps the FRONT 5
    assert_eq!(items(&q), [3, 4, 5, 6, 7]);
    let mut tail = q.split_off(3);
    assert_eq!((items(&q), items(&tail)), (vec![3, 4, 5], vec![6, 7]));
    q.append(&mut tail);
    assert!(tail.is_empty());
    q.retain(|x| x % 2 == 1);
    q.retain_mut(|x| {
        *x *= 10;
        *x > 30
    });
    assert_eq!(items(&q), [50, 70]);
    q.insert(1, 60);
    assert_eq!((q.remove(0), q.remove(99)), (Some(50), None)); // O(distance to the nearer end); Vec::remove would panic
    q.extend([80, 90, 100]);
    assert_eq!(q.swap_remove_back(0), Some(60)); // the BACK element fills the hole
    assert_eq!(items(&q), [100, 70, 80, 90]);
    assert_eq!(q.swap_remove_front(2), Some(80)); // the FRONT element fills the hole
    assert_eq!(items(&q), [70, 100, 90]);
    assert!(q.contains(&100));

    // sorted deque: time-ordered events with expiry from the front
    let mut ts: VecDeque<u64> = [5, 8, 13, 21, 34].into();
    assert_eq!(
        (ts.binary_search(&13), ts.binary_search(&14)),
        (Ok(2), Err(3))
    );
    assert_eq!(ts.partition_point(|&t| t < 13), 2);
    while ts.pop_front_if(|t| *t < 13).is_some() {} // 1.93
    assert_eq!(ts.front(), Some(&13));
    assert_eq!(ts.pop_back_if(|t| *t > 100), None);
    assert_eq!(ts.pop_back_if(|t| *t == 34), Some(34));
    println!("2. bulk editing ok");
}

// ------------------------------------------------------------ 3. Bounded history with undo / redo
struct History {
    undo: VecDeque<String>,
    redo: Vec<String>,
    current: String,
    limit: usize,
}
impl History {
    fn new(limit: usize) -> Self {
        History {
            undo: VecDeque::with_capacity(limit),
            redo: Vec::new(),
            current: String::new(),
            limit,
        }
    }
    fn edit(&mut self, next: &str) {
        if self.undo.len() == self.limit {
            self.undo.pop_front(); // forget the oldest
        }
        self.undo
            .push_back(std::mem::replace(&mut self.current, next.to_string()));
        self.redo.clear(); // a new edit forks history
    }
    fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.current, prev));
        true
    }
    fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo
            .push_back(std::mem::replace(&mut self.current, next));
        true
    }
}

fn s03_history() {
    let mut h = History::new(3);
    for s in ["a", "b", "c", "d", "e"] {
        h.edit(s);
    }
    assert!(h.undo() && h.undo() && h.undo());
    assert_eq!(h.current, "b");
    assert!(!h.undo(), "older states were evicted");
    assert!(h.redo());
    assert_eq!(h.current, "c");
    h.edit("x");
    assert!(!h.redo());
    println!("3. bounded history ok");
}

// ------------------------------------------------------------ 4. Sliding windows
/// Maximum of every window of width `w`. The deque holds indices whose values strictly decrease: O(n) total.
fn window_max(xs: &[i64], w: usize) -> Vec<i64> {
    assert!(w > 0);
    let mut dq: VecDeque<usize> = VecDeque::new();
    let mut out = Vec::new();
    for (i, &x) in xs.iter().enumerate() {
        while dq.pop_back_if(|&mut j| xs[j] <= x).is_some() {} // dominated: older and not larger
        dq.push_back(i);
        dq.pop_front_if(|&mut j| j + w <= i); // left the window
        if i + 1 >= w {
            out.push(xs[dq[0]]);
        }
    }
    out
}

/// A "token budget" window: keep the newest messages whose total size fits the budget.
struct TokenWindow {
    msgs: VecDeque<(u32, usize)>, // (id, tokens)
    used: usize,
    budget: usize,
}
impl TokenWindow {
    /// Returns the ids evicted to make room. A message larger than the whole budget is rejected and evicts nothing.
    fn push(&mut self, id: u32, tokens: usize) -> Result<Vec<u32>, usize> {
        if tokens > self.budget {
            return Err(tokens);
        }
        let mut evicted = Vec::new();
        while self.used + tokens > self.budget {
            let (old, t) = self
                .msgs
                .pop_front()
                .expect("used > 0 implies a message exists");
            self.used -= t;
            evicted.push(old);
        }
        self.msgs.push_back((id, tokens));
        self.used += tokens;
        Ok(evicted)
    }
}

fn s04_windows() {
    assert_eq!(
        window_max(&[1, 3, -1, -3, 5, 3, 6, 7], 3),
        [3, 3, 5, 5, 6, 7]
    );
    let mut rng = Rng(5);
    for case in 0..2_000 {
        let xs: Vec<i64> = (0..rng.below(40)).map(|_| rng.below(9) as i64).collect();
        let w = 1 + rng.below(8) as usize;
        let want: Vec<i64> = xs.windows(w).map(|s| *s.iter().max().unwrap()).collect();
        assert_eq!(window_max(&xs, w), want, "case {case}");
    }
    let mut t = TokenWindow {
        msgs: VecDeque::new(),
        used: 0,
        budget: 100,
    };
    assert_eq!(t.push(1, 40), Ok(vec![]));
    assert_eq!(t.push(2, 50), Ok(vec![]));
    assert_eq!(t.push(3, 101), Err(101));
    assert_eq!(
        t.push(4, 60),
        Ok(vec![1, 2]),
        "evicting 1 alone would leave 50 + 60 > 100"
    );
    assert_eq!(t.push(5, 100), Ok(vec![4]));
    assert_eq!((t.used, t.msgs.len()), (100, 1));
    println!("4. sliding windows ok");
}

// ------------------------------------------------------------ 5. Round-robin dispatch
/// Each job needs `need` units; each turn gives at most `quantum`. Returns the completion order.
fn round_robin(jobs: &[(char, u32)], quantum: u32) -> Vec<(char, u32)> {
    assert!(quantum > 0);
    let mut q: VecDeque<(char, u32)> = jobs.iter().copied().collect();
    let (mut clock, mut finished) = (0, Vec::new());
    while let Some((name, need)) = q.pop_front() {
        let run = need.min(quantum);
        clock += run;
        if need > run {
            q.push_back((name, need - run));
        } else {
            finished.push((name, clock));
        }
    }
    finished
}

fn s05_round_robin() {
    assert_eq!(
        round_robin(&[('a', 5), ('b', 2), ('c', 3)], 2),
        [('b', 4), ('c', 9), ('a', 10)]
    );
    // balancing over backends: rotate the ring, skipping unhealthy ones, without losing anyone's place
    let mut ring: VecDeque<(&str, bool)> = [("n1", true), ("n2", false), ("n3", true)].into();
    let mut pick = || {
        for _ in 0..ring.len() {
            ring.rotate_left(1);
            let &(name, healthy) = ring.back().unwrap();
            if healthy {
                return Some(name);
            }
        }
        None
    };
    assert_eq!(
        (0..5).map(|_| pick().unwrap()).collect::<Vec<_>>(),
        ["n1", "n3", "n1", "n3", "n1"]
    );
    println!("5. round robin ok");
}

// ------------------------------------------------------------ 6. LRU from HashMap + VecDeque with lazy deletion
/// Every use appends (key, stamp). An entry in the queue is live only if its stamp is the key's latest one.
/// (std has no linked hash map; the textbook O(1) LRU needs an intrusive list or index-based links, see the ptr recipe.)
struct Lru {
    map: HashMap<u32, (u32, u64)>, // key -> (value, latest stamp)
    order: VecDeque<(u32, u64)>,
    clock: u64,
    cap: usize,
}
impl Lru {
    fn new(cap: usize) -> Self {
        assert!(cap > 0);
        Lru {
            map: HashMap::new(),
            order: VecDeque::new(),
            clock: 0,
            cap,
        }
    }
    fn touch(&mut self, k: u32) {
        self.clock += 1;
        self.map.get_mut(&k).expect("present").1 = self.clock;
        self.order.push_back((k, self.clock));
        // compaction bounds the queue: without it, a read-heavy cache grows forever
        if self.order.len() > 2 * self.cap + 8 {
            let map = &self.map;
            self.order
                .retain(|&(k, s)| map.get(&k).is_some_and(|e| e.1 == s));
        }
    }
    fn get(&mut self, k: u32) -> Option<u32> {
        let v = self.map.get(&k)?.0;
        self.touch(k);
        Some(v)
    }
    fn put(&mut self, k: u32, v: u32) {
        if !self.map.contains_key(&k) && self.map.len() == self.cap {
            loop {
                let (old, stamp) = self
                    .order
                    .pop_front()
                    .expect("a full cache has live entries");
                if self.map.get(&old).is_some_and(|e| e.1 == stamp) {
                    self.map.remove(&old);
                    break;
                }
            }
        }
        self.map.insert(k, (v, 0));
        self.touch(k);
    }
}

fn s06_lru() {
    let mut rng = Rng(99);
    for cap in 1..=6 {
        let mut lru = Lru::new(cap);
        let mut oracle: Vec<(u32, u32)> = Vec::new(); // least recently used first
        for step in 0..5_000 {
            let k = rng.below(10) as u32;
            let pos = oracle.iter().position(|e| e.0 == k);
            if rng.below(2) == 0 {
                let want = pos.map(|i| {
                    let e = oracle.remove(i);
                    oracle.push(e);
                    e.1
                });
                assert_eq!(lru.get(k), want, "cap {cap} step {step}");
            } else {
                if let Some(i) = pos {
                    oracle.remove(i);
                } else if oracle.len() == cap {
                    oracle.remove(0);
                }
                oracle.push((k, step));
                lru.put(k, step);
            }
            assert_eq!(lru.map.len(), oracle.len());
            assert!(lru.order.len() <= 2 * cap + 8, "queue stays bounded");
        }
    }
    println!("6. lru matches a naive oracle ok");
}

// ------------------------------------------------------------ 7. LinkedList: append moves nothing
fn s07_linked_list() {
    let mut a: LinkedList<u64> = (0..1_000).collect();
    let mut b: LinkedList<u64> = (1_000..2_000).collect();
    let addresses = |l: &LinkedList<u64>| {
        l.iter()
            .map(|x| x as *const u64 as usize)
            .collect::<Vec<_>>()
    };
    let (mut want, tail) = (addresses(&a), addresses(&b));
    want.extend(tail);
    a.append(&mut b); // documented O(1): relink two pointers
    assert!(b.is_empty() && a.len() == 2_000);
    assert_eq!(addresses(&a), want, "not one element moved");

    // the VecDeque equivalent has to copy b's elements into a's buffer
    let mut va: VecDeque<u64> = (0..1_000).collect();
    let mut vb: VecDeque<u64> = (1_000..2_000).collect();
    let first_of_b = &vb[0] as *const u64 as usize;
    va.append(&mut vb);
    assert_ne!(&va[1_000] as *const u64 as usize, first_of_b);

    // split_off keeps nodes in place too, but must first WALK to the split point: O(min(at, len - at))
    let back = a.split_off(1_500);
    assert_eq!(
        (a.len(), back.len(), back.front()),
        (1_500, 500, Some(&1_500))
    );
    assert_eq!(addresses(&back), want[1_500..]);

    let mut l = LinkedList::from([1, 2, 3, 4]);
    l.push_front(0);
    l.push_back(5);
    l.iter_mut().for_each(|x| *x *= 10);
    assert!(l.contains(&30)); // O(n); there is no indexing at all
    let odd_tens: Vec<i32> = l.extract_if(|x| *x % 20 != 0).collect(); // 1.87
    assert_eq!(odd_tens, [10, 30, 50]);
    assert_eq!((l.pop_front(), l.pop_back()), (Some(0), Some(40)));
    assert_eq!(l.into_iter().collect::<Vec<_>>(), [20]);
    println!("7. linkedlist ok");
}

// ------------------------------------------------------------ 8. Choosing a priority structure by counting work
thread_local! { static COMPARES: Cell<u64> = const { Cell::new(0) }; }

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
struct Counted(u64);
impl Ord for Counted {
    fn cmp(&self, o: &Self) -> Ordering {
        COMPARES.set(COMPARES.get() + 1);
        self.0.cmp(&o.0)
    }
}
impl PartialOrd for Counted {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
    // BinaryHeap compares with <= and friends, which go through partial_cmp, hence the counting.
}

fn counted<R>(f: impl FnOnce() -> R) -> (R, u64) {
    COMPARES.set(0);
    let r = f();
    (r, COMPARES.get())
}

fn s08_priority_costs() {
    for n in [1_000u64, 16_000] {
        let mut rng = Rng(123);
        // distinct priorities, so the BTreeSet keeps all of them
        let data: Vec<Counted> = (0..n)
            .map(|i| Counted(rng.below(1 << 30) << 20 | i))
            .collect();
        let (heap_out, heap_cmp) = counted(|| {
            let mut h = BinaryHeap::new();
            data.iter().for_each(|&x| h.push(Reverse(x)));
            std::iter::from_fn(|| h.pop().map(|r| r.0)).collect::<Vec<_>>()
        });
        let (set_out, set_cmp) = counted(|| {
            let mut s = BTreeSet::new();
            data.iter().for_each(|&x| {
                s.insert(x);
            });
            std::iter::from_fn(|| s.pop_first()).collect::<Vec<_>>()
        });
        let mut shifted = 0u64;
        let (vec_out, vec_cmp) = counted(|| {
            let mut v: Vec<Reverse<Counted>> = Vec::new(); // descending, so the minimum pops from the end
            for &x in &data {
                let at = v.binary_search(&Reverse(x)).unwrap_err();
                shifted += (v.len() - at) as u64;
                v.insert(at, Reverse(x));
            }
            std::iter::from_fn(|| v.pop().map(|r| r.0)).collect::<Vec<_>>()
        });
        assert!(heap_out == set_out && set_out == vec_out && heap_out.len() as u64 == n);
        let nlogn = n * u64::from(n.ilog2() + 1);
        assert!(
            heap_cmp < 4 * nlogn && set_cmp < 4 * nlogn && vec_cmp < 2 * nlogn,
            "{heap_cmp} {set_cmp} {vec_cmp}"
        );
        // the sorted Vec compares little but MOVES about n^2/4 elements on random input
        assert!(shifted > n * n / 8 && shifted < n * n / 2, "{shifted}");
        println!(
            "   n={n:>6}: compares heap={heap_cmp} btreeset={set_cmp} sorted-vec={vec_cmp}; sorted-vec element shifts={shifted}"
        );
    }
    // Compare counts are not run time: memmove is very fast, and B-tree nodes cost allocations. Benchmark before choosing.
    // Capability differences matter more: only BTreeSet removes or re-prioritises an arbitrary element in O(log n).
    println!("8. priority structure costs ok");
}

fn main() {
    s01_basics();
    s02_bulk();
    s03_history();
    s04_windows();
    s05_round_robin();
    s06_lru();
    s07_linked_list();
    s08_priority_costs();
    println!("all deque sections passed");
}
