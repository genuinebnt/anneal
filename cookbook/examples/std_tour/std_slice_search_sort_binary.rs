//! RECIPE 1023 (std): slice look-up page: access, search, binary search, sorting, selection, rearranging, copying.
//! LEVEL: intermediate
//! Use in: leaderboards, time-range queries over sorted data, percentiles, ring rotation, buffer copies.   Run: cargo run --release --example std_slice_search_sort_binary
//! COMMON USES: "events between t1 and t2" on a sorted Vec, top-k results, p50/p99 latency, shard lookup in a sorted
//! boundary table, in-place buffer shuffles. Everything here also works on Vec<T> and [T; N] through deref/coercion.
//! METHODS: (everyday first) slice::len, slice::is_empty, slice::first, slice::last, slice::get, slice::get_mut, slice::iter, slice::iter_mut,
//! METHODS: slice::contains, slice::starts_with, slice::ends_with, slice::to_vec, slice::concat, slice::join, slice::sort, slice::sort_unstable,
//! METHODS: slice::sort_by_key, slice::reverse, slice::binary_search, slice::binary_search_by, slice::binary_search_by_key, slice::partition_point,
//! METHODS: slice::select_nth_unstable, slice::select_nth_unstable_by, slice::select_nth_unstable_by_key, slice::is_sorted, slice::is_sorted_by,
//! METHODS: Iterator::position, Iterator::rposition, Iterator::rev, Iterator::find, Iterator::max_by_key,
//! METHODS: slice::split_first, slice::split_last, slice::first_mut, slice::last_mut, slice::strip_prefix, slice::strip_suffix,
//! METHODS: slice::swap, slice::swap_with_slice, slice::rotate_left, slice::rotate_right, slice::fill, slice::fill_with, slice::copy_from_slice,
//! METHODS: slice::clone_from_slice, slice::copy_within, slice::into_vec, slice::repeat, slice::first_chunk, slice::last_chunk,
//! METHODS: slice::split_first_chunk, slice::split_last_chunk, slice::first_chunk_mut, slice::get_disjoint_mut, slice::strip_circumfix,
//! METHODS: slice::subslice_range, slice::element_offset, slice::as_flattened, slice::as_array
//! METHODS: Iterator::max_by, Iterator::min_by, Vec::dedup
//! DOCS: https://doc.rust-lang.org/std/primitive.slice.html
//! DEFINED IN: core (`core::slice`: access, search, copy, sort_unstable*, select_nth_unstable*, binary search), usable in no_std.
//! alloc (`alloc::slice`): sort, sort_by, sort_by_key, sort_by_cached_key, to_vec, into_vec, concat, join, repeat.
//! NOT COVERED (and why): sort_unstable_by, sort_unstable_by_key, is_sorted_by_key (see std_cmp_hash_default_traits), get_unchecked*/swap_unchecked/split_at_unchecked (unsafe), chunking, windows and split*
//! (see std_slice_chunks_windows_split), sort_by_cached_key and float sorting (see std_cmp_hash_default_traits), the other
//! *_mut twins (same shape), split_off_first/split_off_last family (shrinks a `&mut &[T]`, niche), as_ptr/as_mut_ptr.
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   read index i without panicking            -> get(i)          sub-range: get(a..b)
//!   head + rest                               -> split_first / `if let [head, rest @ ..] = s`
//!   fixed-size header from the front          -> first_chunk::<N>() / split_first_chunk::<N>()
//!   is x in a SORTED slice, and where         -> binary_search  (Err(i) = insertion point)
//!   first index of the sorted range >= x      -> partition_point(|v| v < x)
//!   first index of the sorted range > x       -> partition_point(|v| v <= x)
//!   first / last match, unsorted              -> iter().position(..) / iter().rposition(..)
//!   k-th smallest, median, top-k              -> select_nth_unstable* (average and worst case O(n))
//!   sort, keeping equal items in order        -> sort / sort_by / sort_by_key
//!   sort, order of equal items irrelevant     -> sort_unstable* (does not allocate)
//!   move the first k items to the back        -> rotate_left(k)
//!   copy bytes between buffers                -> copy_from_slice (lengths must match)
//!   overlapping move inside one buffer        -> copy_within
//!   two &mut elements at once                 -> get_disjoint_mut([i, j])  or split_at_mut
//!
//! Read: https://doc.rust-lang.org/std/primitive.slice.html , https://doc.rust-lang.org/reference/patterns.html#slice-patterns
//! TRY NEXT:
//!   - [S] Remove the sort in `range_query`'s caller and run: which assert notices, and could a wrong result slip through?
//!   - [S] Change `top_k` to return ties deterministically when scores are equal (extend the key).
//!   - [M] Write `merge_intervals` with sort_unstable_by_key plus one pass, and differential-test it against a bitmap.
//!   - [M] Benchmark top_k(10) of 10 million items: full sort vs select_nth_unstable vs a BinaryHeap of size k.
//!   - [A] PROJECT: a sorted in-memory time index: partition_point range queries (this file) over a VecDeque for retention
//!         (std_vec_vecdeque_methods), with percentile reports using total_cmp (std_cmp_hash_default_traits).
use std::cmp::Reverse;
use std::collections::HashSet;

const COVERED: &[&str] = &[
    "len",
    "is_empty",
    "first",
    "last",
    "get",
    "get_mut",
    "iter",
    "iter_mut",
    "contains",
    "starts_with",
    "ends_with",
    "to_vec",
    "concat",
    "join",
    "sort",
    "sort_unstable",
    "sort_by_key",
    "reverse",
    "binary_search",
    "binary_search_by",
    "binary_search_by_key",
    "partition_point",
    "select_nth_unstable",
    "select_nth_unstable_by",
    "select_nth_unstable_by_key",
    "is_sorted",
    "is_sorted_by",
    "position",
    "rposition",
    "rev",
    "find",
    "max_by_key",
    "max_by",
    "min_by",
    "dedup",
    "split_first",
    "split_last",
    "first_mut",
    "last_mut",
    "strip_prefix",
    "strip_suffix",
    "swap",
    "swap_with_slice",
    "rotate_left",
    "rotate_right",
    "fill",
    "fill_with",
    "copy_from_slice",
    "clone_from_slice",
    "copy_within",
    "into_vec",
    "repeat",
    "first_chunk",
    "last_chunk",
    "split_first_chunk",
    "split_last_chunk",
    "first_chunk_mut",
    "get_disjoint_mut",
    "strip_circumfix",
    "subslice_range",
    "element_offset",
    "as_flattened",
    "as_array",
];

fn panics(f: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f).is_err();
    std::panic::set_hook(hook);
    r
}

struct XorShift(u64);
impl XorShift {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

// ---- 1. Everyday core: access ----------------------------------------------------------------------------------
/// Take `&[T]`: callers can pass a Vec, an array, or a sub-slice.
fn describe(args: &[&str]) -> String {
    match args {
        [] => "no command".into(),
        [cmd] => format!("{cmd} (no args)"),
        [cmd, rest @ ..] => format!("{cmd} with {}", rest.join(",")),
    }
}

fn section_access() {
    let v = vec![10, 20, 30];
    assert_eq!(
        (v.len(), v.is_empty(), v.first(), v.last()),
        (3, false, Some(&10), Some(&30))
    );
    let empty: &[i32] = &[];
    assert_eq!(
        (empty.first(), empty.last(), empty.get(0)),
        (None, None, None)
    );
    // get accepts an index or any range; out of bounds gives None where v[..] panics.
    assert_eq!(v.get(1), Some(&20));
    assert_eq!(v.get(1..), Some(&[20, 30][..]));
    assert_eq!(v.get(2..9), None);
    assert_eq!(v.get(3..), Some(&[][..])); // start == len is a valid empty range
    assert!(panics(|| {
        let a = [1];
        let _ = a[std::hint::black_box(1)];
    }));
    let mut w = v.clone();
    if let Some(x) = w.get_mut(0) {
        *x += 1;
    }
    *w.first_mut().unwrap() += 1;
    *w.last_mut().unwrap() += 1;
    for x in w.iter_mut() {
        *x *= 2;
    }
    assert_eq!(w, [24, 40, 62]);

    assert_eq!(describe(&[]), "no command");
    assert_eq!(describe(&["ls"]), "ls (no args)");
    assert_eq!(describe(&["cp", "a", "b"]), "cp with a,b");
    assert_eq!(v.split_first(), Some((&10, &[20, 30][..])));
    assert_eq!(v.split_last(), Some((&30, &[10, 20][..])));

    // Fixed-size prefixes become &[T; N], so indexing them later cannot fail.
    let packet: &[u8] = &[0xAB, 0xCD, 0, 3, b'a', b'b', b'c'];
    let (magic, rest) = packet.split_first_chunk::<2>().unwrap();
    assert_eq!(*magic, [0xAB, 0xCD]);
    let (len, body) = rest.split_first_chunk::<2>().unwrap();
    assert_eq!((u16::from_be_bytes(*len) as usize, body), (3, &b"abc"[..]));
    assert_eq!(packet.first_chunk::<2>(), Some(&[0xAB, 0xCD]));
    assert_eq!(packet.last_chunk::<2>(), Some(b"bc"));
    assert_eq!(packet.split_last_chunk::<3>().map(|(_, t)| t), Some(b"abc"));
    assert_eq!([1u8].first_chunk::<2>(), None);
    assert_eq!(packet.as_array::<7>().map(|a| a[4]), Some(b'a')); // since 1.93: exact length only
    assert_eq!(packet.as_array::<6>(), None);
    println!("1. access ok");
}

// ---- 2. Everyday core: linear search ---------------------------------------------------------------------------
fn section_linear_search() {
    let log = ["boot", "warn: disk", "ok", "warn: net", "ok"];
    assert!(log.contains(&"ok")); // contains takes &T
    assert!(log.iter().any(|l| l.starts_with("warn"))); // search by property: any
    assert_eq!(log.iter().position(|l| l.starts_with("warn")), Some(1));
    assert_eq!(log.iter().rposition(|l| l.starts_with("warn")), Some(3));
    assert_eq!(log.iter().find(|l| l.contains("net")), Some(&"warn: net"));
    assert_eq!(
        log.iter().rev().take(2).collect::<Vec<_>>(),
        [&"ok", &"warn: net"]
    );
    assert_eq!(
        log.iter()
            .enumerate()
            .max_by_key(|(_, l)| l.len())
            .map(|(i, _)| i),
        Some(1)
    );
    // slice starts_with/ends_with compare SUBSLICES (not a single element). The empty slice matches everything.
    let b: &[u8] = b"GET /x HTTP/1.1";
    assert!(b.starts_with(b"GET ") && b.ends_with(b"HTTP/1.1") && b.starts_with(&[]));
    assert_eq!(b.strip_prefix(b"GET "), Some(&b"/x HTTP/1.1"[..]));
    assert_eq!(b.strip_suffix(b" HTTP/1.1"), Some(&b"GET /x"[..]));
    assert_eq!(b.strip_circumfix(b"GET ", b" HTTP/1.1"), Some(&b"/x"[..])); // since 1.98
    // max_by / min_by take a comparator: the way to rank floats or borrowed fields. Ties: max keeps the last, min the first.
    let temps = [("a", 2.5f64), ("b", 9.0), ("c", -1.0)];
    assert_eq!(
        temps.iter().max_by(|x, y| x.1.total_cmp(&y.1)).map(|t| t.0),
        Some("b")
    );
    assert_eq!(
        temps.iter().min_by(|x, y| x.1.total_cmp(&y.1)).map(|t| t.0),
        Some("c")
    );
    // Dedup a log: dedup is a Vec method (it changes the length); on sorted data it leaves unique values.
    let mut ids = vec![4, 1, 4, 2, 1];
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids, [1, 2, 4]);
    println!("2. linear search ok");
}

// ---- 3. Binary search and partition_point ----------------------------------------------------------------------
/// All events with lo <= time < hi. `events` MUST be sorted by time.
fn range_query<'a>(events: &'a [(u64, &'a str)], lo: u64, hi: u64) -> &'a [(u64, &'a str)] {
    let start = events.partition_point(|e| e.0 < lo);
    let end = events.partition_point(|e| e.0 < hi);
    &events[start..end.max(start)]
}

/// Sorted lower bounds of shard ranges: which shard owns `key`?
fn shard_for(lower_bounds: &[u32], key: u32) -> Option<usize> {
    lower_bounds.partition_point(|&b| b <= key).checked_sub(1)
}

fn section_binary_search() {
    let s = [10, 20, 20, 20, 30];
    // Ok(i): SOME matching index. With duplicates it is unspecified which one.
    let hit = s.binary_search(&20).unwrap();
    assert!((1..=3).contains(&hit));
    // Err(i): not found, and inserting at i keeps the order.
    assert_eq!(
        (
            s.binary_search(&5),
            s.binary_search(&25),
            s.binary_search(&99)
        ),
        (Err(0), Err(4), Err(5))
    );
    let mut sorted = vec![1, 3, 5];
    if let Err(i) = sorted.binary_search(&4) {
        sorted.insert(i, 4); // sorted insert; O(n) because of the shift
    }
    assert_eq!(sorted, [1, 3, 4, 5]);
    // First and one-past-last of an equal run: use partition_point, which is fully specified.
    let lower = s.partition_point(|&x| x < 20);
    let upper = s.partition_point(|&x| x <= 20);
    assert_eq!((lower, upper, upper - lower), (1, 4, 3));
    // binary_search_by: the closure says where the ELEMENT sits relative to the target (element.cmp(&target)).
    let desc = [9, 7, 5, 3];
    assert_eq!(desc.binary_search_by(|x| 5.cmp(x)), Ok(2)); // reversed order: reverse the comparison
    let by_id = [(1, "a"), (4, "d"), (9, "i")];
    assert_eq!(by_id.binary_search_by_key(&4, |p| p.0), Ok(1));
    // PITFALL: on unsorted data the answer is meaningless (no panic, just wrong).
    let unsorted = [30, 10, 20];
    assert!(unsorted.binary_search(&30).is_err());

    let ev = [(100, "a"), (105, "b"), (105, "c"), (200, "d")];
    assert_eq!(
        range_query(&ev, 105, 200)
            .iter()
            .map(|e| e.1)
            .collect::<String>(),
        "bc"
    );
    assert!(range_query(&ev, 300, 400).is_empty() && range_query(&ev, 200, 100).is_empty());
    let shards = [0, 1000, 5000];
    assert_eq!(
        [
            shard_for(&shards, 0),
            shard_for(&shards, 999),
            shard_for(&shards, 1000),
            shard_for(&shards, 99999)
        ],
        [Some(0), Some(0), Some(1), Some(2)]
    );
    assert_eq!(shard_for(&[10], 3), None);

    // Differential test: partition_point against linear counting.
    let mut rng = XorShift(0xDEAD_BEEF);
    for _ in 0..3000 {
        let mut v: Vec<u64> = (0..rng.below(30)).map(|_| rng.below(20)).collect();
        v.sort_unstable();
        let t = rng.below(22);
        assert_eq!(
            v.partition_point(|&x| x < t),
            v.iter().filter(|&&x| x < t).count()
        );
        match v.binary_search(&t) {
            Ok(i) => assert_eq!(v[i], t),
            Err(i) => assert!(!v.contains(&t) && i == v.iter().filter(|&&x| x < t).count()),
        }
    }
    println!("3. binary search ok");
}

// ---- 4. Sorting and selection ----------------------------------------------------------------------------------
/// Highest k scores, best first: O(n + k log k) instead of O(n log n).
fn top_k(items: &mut [(u32, u32)], k: usize) -> &[(u32, u32)] {
    let k = k.min(items.len());
    if k == 0 {
        return &[];
    }
    if k < items.len() {
        items.select_nth_unstable_by_key(k - 1, |&(score, id)| (Reverse(score), id));
    }
    let head = &mut items[..k];
    head.sort_unstable_by_key(|&(score, id)| (Reverse(score), id));
    head
}

/// Median of a non-empty sample (mean of the two middle values for even length).
fn median(v: &mut [u32]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mid = v.len() / 2;
    let even = v.len() % 2 == 0;
    let (below, upper, _) = v.select_nth_unstable(mid);
    if even {
        let lower = *below.iter().max()?; // everything before mid is <= upper
        Some((f64::from(lower) + f64::from(*upper)) / 2.0)
    } else {
        Some(f64::from(*upper))
    }
}

fn section_sorting() {
    // Stable sort keeps the input order of equal keys.
    let mut v = [(2, 'a'), (1, 'b'), (2, 'c'), (1, 'd')];
    v.sort_by_key(|p| p.0);
    assert_eq!(v, [(1, 'b'), (1, 'd'), (2, 'a'), (2, 'c')]);
    assert!(v.is_sorted_by(|a, b| a.0 <= b.0)); // since 1.82; the closure answers "may a come before b?"
    assert!(!v.is_sorted_by(|a, b| a.0 < b.0)); // strictly increasing fails on duplicates
    let mut u = v;
    u.sort_unstable();
    assert!(u.is_sorted());
    u.reverse(); // descending; note this also reverses the order inside equal keys
    assert_eq!(u[0], (2, 'c'));
    let mut s = [3, 1, 2];
    s.sort();
    assert_eq!(s, [1, 2, 3]);

    // select_nth_unstable returns (before, nth, after).
    let mut d = [50, 10, 40, 20, 30];
    let (lo, nth, hi) = d.select_nth_unstable(2);
    assert_eq!(*nth, 30);
    assert!(lo.iter().all(|x| *x <= 30) && hi.iter().all(|x| *x >= 30));
    let (_, second_largest, _) = d.select_nth_unstable_by(1, |a, b| b.cmp(a));
    assert_eq!(*second_largest, 40);
    assert!(panics(|| {
        let mut e: [u8; 0] = [];
        e.select_nth_unstable(0);
    }));

    let mut scores = [(70, 1), (95, 2), (88, 3), (95, 4), (60, 5)];
    assert_eq!(top_k(&mut scores, 3), [(95, 2), (95, 4), (88, 3)]);
    assert!(top_k(&mut scores, 0).is_empty());
    assert_eq!(top_k(&mut scores, 99).len(), 5);
    assert_eq!(
        (
            median(&mut [5, 1, 9]),
            median(&mut [4, 1, 3, 2]),
            median(&mut [])
        ),
        (Some(5.0), Some(2.5), None)
    );

    // Differential: selection and top-k against a full sort.
    let mut rng = XorShift(42);
    for _ in 0..2000 {
        let n = 1 + rng.below(40) as usize;
        let mut a: Vec<(u32, u32)> = (0..n as u32).map(|id| (rng.below(8) as u32, id)).collect();
        let mut oracle = a.clone();
        oracle.sort_by_key(|&(s, id)| (Reverse(s), id));
        let k = rng.below(n as u64 + 2) as usize;
        assert_eq!(top_k(&mut a, k), &oracle[..k.min(n)]);
        let mut vals: Vec<u32> = (0..n).map(|_| rng.below(100) as u32).collect();
        let mut sorted = vals.clone();
        sorted.sort_unstable();
        let expect = (f64::from(sorted[(n - 1) / 2]) + f64::from(sorted[n / 2])) / 2.0;
        assert_eq!(median(&mut vals), Some(expect));
    }
    println!("4. sorting and selection ok");
}

// ---- 5. Rearranging in place -----------------------------------------------------------------------------------
fn section_rearrange() {
    // Round-robin: the worker at the front goes to the back.
    let mut ring = ["a", "b", "c", "d"];
    ring.rotate_left(1);
    assert_eq!(ring, ["b", "c", "d", "a"]);
    ring.rotate_right(2);
    assert_eq!(ring, ["d", "a", "b", "c"]);
    ring.rotate_left(4); // == len: no-op
    assert!(panics(|| {
        let mut r = [1, 2];
        r.rotate_left(std::hint::black_box(3));
    }));
    // rotate on a SUB-slice moves one item to a new position without remove + insert.
    let mut list = [0, 1, 2, 3, 4];
    list[1..=3].rotate_right(1); // move index 3 to index 1
    assert_eq!(list, [0, 3, 1, 2, 4]);

    let mut s = [1, 2, 3, 4, 5, 6];
    s.swap(0, 5);
    assert_eq!(s, [6, 2, 3, 4, 5, 1]);
    // swap_with_slice needs two non-overlapping &mut of equal length: get them with split_at_mut.
    let (left, right) = s.split_at_mut(3);
    left.swap_with_slice(right);
    assert_eq!(s, [4, 5, 1, 6, 2, 3]);
    // get_disjoint_mut (since 1.86): several &mut at once; Err on overlap or out of bounds.
    let mut accounts = [100u64, 50, 0];
    if let Ok([from, to]) = accounts.get_disjoint_mut([0, 2]) {
        *from -= 30;
        *to += 30;
    }
    assert_eq!(accounts, [70, 50, 30]);
    assert!(
        accounts.get_disjoint_mut([1, 1]).is_err() && accounts.get_disjoint_mut([0, 3]).is_err()
    );
    println!("5. rearranging ok");
}

// ---- 6. Filling and copying ------------------------------------------------------------------------------------
fn section_copy() {
    let mut buf = [0u8; 8];
    buf.fill(0xFF);
    assert_eq!(buf, [0xFF; 8]);
    let mut n = 0;
    buf.fill_with(|| {
        n += 1;
        n
    });
    assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8]);
    // copy_from_slice (T: Copy) and clone_from_slice (T: Clone) require EQUAL lengths: slice the destination.
    let msg = b"hi";
    let mut out = [0u8; 8];
    out[..msg.len()].copy_from_slice(msg);
    assert_eq!(&out[..3], b"hi\0");
    assert!(panics(|| {
        let mut d = [0u8; 3];
        d.copy_from_slice(std::hint::black_box(&[1, 2][..]));
    }));
    let src = vec![String::from("x"), String::from("y")];
    let mut dst = vec![String::new(); 2];
    dst.clone_from_slice(&src);
    assert_eq!(dst, src);
    // copy_within: memmove inside one buffer, overlapping allowed. Typical use: compact a read buffer.
    let mut rb = *b"consumedREST";
    rb.copy_within(8.., 0);
    assert_eq!(&rb[..4], b"REST");
    assert!(panics(|| {
        let mut a = [0u8; 4];
        a.copy_within(0..3, std::hint::black_box(2)); // destination runs past the end
    }));

    // Building new vectors.
    let parts: [Vec<u8>; 2] = [vec![1, 2], vec![3]];
    assert_eq!(parts.concat(), [1, 2, 3]);
    assert_eq!(parts.join(&0), [1, 2, 0, 3]);
    assert_eq!(parts.join(&[0, 0][..]), [1, 2, 0, 0, 3]);
    assert_eq!(["a", "b"].concat(), "ab");
    assert_eq!([1, 2].repeat(3), [1, 2, 1, 2, 1, 2]);
    assert_eq!([[1, 2], [3, 4]].as_flattened(), [1, 2, 3, 4]); // since 1.80: a view, no copy
    let boxed: Box<[u8]> = vec![7, 8].into_boxed_slice();
    assert_eq!(boxed.into_vec(), [7, 8]); // Box<[T]> -> Vec<T> without reallocating
    assert_eq!(buf[..2].to_vec(), vec![1, 2]);
    println!("6. fill/copy/build ok");
}

// ---- 7. Less known: recovering positions from sub-slices -------------------------------------------------------
fn section_less_known() {
    let data = [1u8, 2, 0, 3, 4, 0, 5];
    let second = data.split(|&b| b == 0).nth(1).unwrap();
    // subslice_range / element_offset: pointer arithmetic, NOT a search. None if not borrowed from `data`.
    let r = data.subslice_range(second).unwrap(); // since 1.98
    assert_eq!((r.start, r.end), (3, 5));
    assert_eq!(data.element_offset(&second[1]), Some(4)); // since 1.94
    let copy = second.to_vec();
    assert!(data.subslice_range(&copy).is_none());
    let mut hdr = [0u8; 6];
    if let Some(m) = hdr.first_chunk_mut::<4>() {
        *m = *b"RIFF";
    }
    assert_eq!(&hdr, b"RIFF\0\0");
    println!("7. less known ok");
}

// ---- 8. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. Parameters: &[T], not &Vec<T> (clippy::ptr_arg).
    // 2. Untrusted lengths: get(..n), split_at_checked, first_chunk, never &buf[..n].
    let frame = [1u8, 2, 3];
    assert_eq!((frame.get(..10), frame.split_at_checked(10)), (None, None));
    // 3. `v.iter().any(|x| *x == y)` is contains (clippy::manual_contains).
    assert!(frame.contains(&2));
    // 4. get(0) is first() (clippy::get_first).
    assert_eq!(frame.first(), Some(&1));
    // 5. Primitive keys, no need for stability: sort_unstable (clippy::stable_sort_primitive, pedantic).
    // 6. Need only the best k? select_nth_unstable, not a full sort (section 4).
    // 7. Many lookups: sort once + binary_search beats repeated linear contains; a HashSet if order is irrelevant.
    // 8. One lookup: do NOT sort first; a linear scan is O(n).
    // 9. Loops over indices: iterate or zip instead (clippy::needless_range_loop), which also removes bounds checks.
    let (a, mut b) = ([1, 2, 3], [10, 20, 30]);
    b.iter_mut().zip(&a).for_each(|(y, x)| *y += x);
    assert_eq!(b, [11, 22, 33]);
    // 10. `to_vec()` only when you need ownership; otherwise keep borrowing (clippy::unnecessary_to_owned).
    // 11. Manual element-by-element copies: copy_from_slice (clippy::manual_memcpy) is checked once and vectorised.
    // 12. A "sorted" invariant should live in a type (struct Sorted(Vec<T>)) or be checked in debug builds.
    debug_assert!(a.is_sorted());
    println!("8. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_access();
    section_linear_search();
    section_binary_search();
    section_sorting();
    section_rearrange();
    section_copy();
    section_less_known();
    section_production_tips();
    println!("covered {} slice names; all asserts passed", COVERED.len());
}
