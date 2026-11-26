//! RECIPE 1024 (std): slicing a slice: chunks, chunks_exact, rchunks, windows, as_chunks, chunk_by, split*, split_at_mut.
//! LEVEL: intermediate
//! Use in: packet framing, batching, moving averages, run-length encoding, image/matrix rows, parallel processing.   Run: cargo run --release --example std_slice_chunks_windows_split
//! COMMON USES: batch inserts of N rows, fixed-size block checksums, sliding-window metrics, grouping sorted
//! records, giving each worker thread its own `&mut` region.
//! METHODS: (everyday first) slice::chunks, slice::chunks_exact, ChunksExact::remainder, slice::windows, slice::split_at, slice::split_at_mut,
//! METHODS: slice::split, slice::splitn, slice::chunks_mut, slice::chunks_exact_mut, ChunksExactMut::into_remainder, slice::rchunks,
//! METHODS: slice::rchunks_mut, slice::rchunks_exact, slice::rchunks_exact_mut, slice::as_chunks, slice::as_chunks_mut, slice::as_rchunks,
//! METHODS: slice::as_rchunks_mut, slice::array_windows, slice::chunk_by, slice::chunk_by_mut, slice::split_mut, slice::splitn_mut,
//! METHODS: slice::rsplit, slice::rsplit_mut, slice::rsplitn, slice::rsplitn_mut, slice::split_inclusive, slice::split_inclusive_mut,
//! METHODS: slice::split_at_checked, slice::split_at_mut_checked, slice::split_first_mut, slice::split_last_mut, slice::as_ptr_range,
//! METHODS: Iterator::step_by, Iterator::zip, std::array::from_fn, std::thread::scope
//! DOCS: https://doc.rust-lang.org/std/primitive.slice.html
//! DEFINED IN: every slice method and array::from_fn here is core (`core::slice`, `core::array`), usable in no_std.
//! `thread::scope` is std. `Iterator::step_by`/`zip` are core.
//! NOT COVERED (and why): split_at_unchecked / as_chunks_unchecked (unsafe), split_off_first/last family, itertools-style
//! `group_by` (chunk_by is the std equivalent for adjacent runs); string splitting is std_str_search_split_parse.
#![allow(dead_code)]
//!
//! CHEAT SHEET (I want to ... -> use ...)
//!   batches of n, last one may be short            -> chunks(n)
//!   fixed records, handle the tail yourself        -> chunks_exact(n) + remainder()
//!   fixed records as real arrays (&[T; N])         -> as_chunks::<N>()          -> (&[[T; N]], tail)
//!   batches counted from the END                   -> rchunks(n)
//!   overlapping windows                            -> windows(n)  /  array_windows::<N>()
//!   non-overlapping with a stride                  -> iter().step_by(k)
//!   runs of ADJACENT items that belong together    -> chunk_by(|a, b| ..)
//!   break on a delimiter element                   -> split(pred)      keep delimiter: split_inclusive(pred)
//!   header, then the rest (at most n pieces)       -> splitn(n, pred)
//!   two disjoint &mut halves                       -> split_at_mut(mid)
//!   one &mut element plus the others               -> split_first_mut / split_at_mut twice
//!   parallel work on disjoint regions              -> chunks_mut inside thread::scope
//!
//! PANICS (slice docs): chunks, chunks_exact, rchunks (and _mut forms) and windows panic when the size is 0;
//! as_chunks/array_windows reject N = 0 at compile time; split_at(mid) panics if mid > len; step_by(0) panics.
//!
//! Read: https://doc.rust-lang.org/std/primitive.slice.html
//! TRY NEXT:
//!   - [S] Change `checksum` to use `chunks(4)` and drop the remainder branch. Which input now gives a different answer?
//!   - [S] Predict `[1,2,3,4,5].windows(2).step_by(2)` and then run it.
//!   - [M] Change `moving_average` to keep a running sum (O(n) instead of O(n*w)) and differential-test the two.
//!   - [M] In `parallel_square`, make the chunk size 1 and then len/cores; time both on 10 million items and explain.
//!   - [A] PROJECT: a length-prefixed frame decoder over a VecDeque read buffer (std_vec_vecdeque_methods) using
//!         split_first_chunk (std_slice_search_sort_binary), fuzzed by splitting the input at random points.
use std::collections::HashSet;

const COVERED: &[&str] = &[
    "chunks",
    "chunks_exact",
    "remainder",
    "windows",
    "split_at",
    "split_at_mut",
    "split",
    "splitn",
    "chunks_mut",
    "chunks_exact_mut",
    "into_remainder",
    "rchunks",
    "rchunks_mut",
    "rchunks_exact",
    "rchunks_exact_mut",
    "as_chunks",
    "as_chunks_mut",
    "as_rchunks",
    "as_rchunks_mut",
    "array_windows",
    "chunk_by",
    "chunk_by_mut",
    "split_mut",
    "splitn_mut",
    "rsplit",
    "rsplit_mut",
    "rsplitn",
    "rsplitn_mut",
    "split_inclusive",
    "split_inclusive_mut",
    "split_at_checked",
    "split_at_mut_checked",
    "split_first_mut",
    "split_last_mut",
    "as_ptr_range",
    "step_by",
    "zip",
    "from_fn",
    "scope",
];

fn panics(f: impl FnOnce() + std::panic::UnwindSafe) -> bool {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let r = std::panic::catch_unwind(f).is_err();
    std::panic::set_hook(hook);
    r
}

fn collect<'a, T: Clone + 'a>(it: impl Iterator<Item = &'a [T]>) -> Vec<Vec<T>> {
    it.map(<[T]>::to_vec).collect()
}

// ---- 1. Everyday core: chunks, chunks_exact, windows -----------------------------------------------------------
/// Sum of big-endian u32 words; a short tail is zero-padded, never dropped.
fn checksum(data: &[u8]) -> u32 {
    let mut words = data.chunks_exact(4);
    let mut sum = words.by_ref().fold(0u32, |acc, w| {
        acc.wrapping_add(u32::from_be_bytes([w[0], w[1], w[2], w[3]]))
    });
    let tail = words.remainder();
    if !tail.is_empty() {
        let mut last = [0u8; 4];
        last[..tail.len()].copy_from_slice(tail);
        sum = sum.wrapping_add(u32::from_be_bytes(last));
    }
    sum
}

fn moving_average(xs: &[u32], w: usize) -> Vec<f64> {
    if w == 0 {
        return Vec::new(); // windows(0) would panic
    }
    xs.windows(w)
        .map(|win| win.iter().map(|&x| f64::from(x)).sum::<f64>() / w as f64)
        .collect()
}

fn section_everyday() {
    let v = [1, 2, 3, 4, 5, 6, 7];
    // Batches for a bulk insert: the last one is shorter.
    assert_eq!(
        collect(v.chunks(3)),
        [vec![1, 2, 3], vec![4, 5, 6], vec![7]]
    );
    assert_eq!(v.chunks(3).len(), 3); // = len.div_ceil(3), known without iterating
    assert_eq!(v.chunks(99).count(), 1);
    assert_eq!([0u8; 0].chunks(3).count(), 0); // empty input: no chunks at all
    // chunks_exact yields only full chunks; the leftovers are in remainder(), available at any time.
    let ex = v.chunks_exact(3);
    assert_eq!(ex.remainder(), [7]);
    assert_eq!(collect(ex), [vec![1, 2, 3], vec![4, 5, 6]]);
    assert_eq!(checksum(&[0, 0, 0, 1, 0, 0, 0, 2]), 3);
    assert_eq!(checksum(&[0, 0, 0, 1, 2]), 0x0200_0001);

    // windows: overlapping, always exactly w long; there are len - w + 1 of them, or none.
    assert_eq!(
        collect(v[..4].windows(2)),
        [vec![1, 2], vec![2, 3], vec![3, 4]]
    );
    assert_eq!(v.windows(8).count(), 0);
    assert_eq!(moving_average(&[1, 2, 3, 10], 2), [1.5, 2.5, 6.5]);
    assert!(moving_average(&[1], 2).is_empty() && moving_average(&[1], 0).is_empty());
    // Pairwise checks.
    let deltas: Vec<i32> = v.windows(2).map(|p| p[1] - p[0]).collect();
    assert!(deltas.iter().all(|&d| d == 1));
    assert!([1, 1, 2].windows(2).any(|p| p[0] == p[1]));
    for f in [
        || drop([1].chunks(std::hint::black_box(0))),
        || drop([1].chunks_exact(std::hint::black_box(0))),
        || drop([1].windows(std::hint::black_box(0))),
        || drop([1].rchunks(std::hint::black_box(0))),
        || drop([1].iter().step_by(std::hint::black_box(0))),
    ] {
        assert!(panics(f));
    }
    println!("1. chunks and windows ok");
}

// ---- 2. Mutable and from-the-end chunking ----------------------------------------------------------------------
fn section_mut_and_reverse() {
    // XOR "masking" in 4-byte lanes; chunks_mut handles the short tail with the same code through zip.
    let key = [1u8, 2, 3, 4];
    let mut data = [0u8; 6];
    for block in data.chunks_mut(4) {
        block.iter_mut().zip(key).for_each(|(b, k)| *b ^= k);
    }
    assert_eq!(data, [1, 2, 3, 4, 1, 2]);
    let mut it = data.chunks_exact_mut(4);
    for block in it.by_ref() {
        block.reverse();
    }
    it.into_remainder().fill(9); // into_remainder consumes the iterator and returns the mutable tail
    assert_eq!(data, [4, 3, 2, 1, 9, 9]);

    // rchunks: same, but aligned to the END, so the short chunk is the FIRST one of the slice.
    let v = [1, 2, 3, 4, 5, 6, 7];
    assert_eq!(
        collect(v.rchunks(3)),
        [vec![5, 6, 7], vec![2, 3, 4], vec![1]]
    ); // elements inside keep their order
    let r = v.rchunks_exact(3);
    assert_eq!(r.remainder(), [1]);
    assert_eq!(collect(r), [vec![5, 6, 7], vec![2, 3, 4]]);
    // Thousands grouping is a natural rchunks job.
    let digits = b"1234567";
    let mut groups: Vec<&str> = digits
        .rchunks(3)
        .map(|g| std::str::from_utf8(g).unwrap())
        .collect();
    groups.reverse();
    assert_eq!(groups.join(","), "1,234,567");
    let mut m = v;
    m.rchunks_mut(3)
        .enumerate()
        .for_each(|(i, c)| c.fill(i as i32));
    assert_eq!(m, [2, 1, 1, 1, 0, 0, 0]);
    let mut e = v;
    let mut it = e.rchunks_exact_mut(3);
    it.by_ref().for_each(<[i32]>::reverse);
    it.into_remainder().fill(0); // the leftover is at the FRONT
    assert_eq!(e, [0, 4, 3, 2, 7, 6, 5]);
    println!("2. mutable and reverse chunks ok");
}

// ---- 3. Arrays instead of slices: as_chunks, array_windows -----------------------------------------------------
fn section_array_chunks() {
    let rgb = [10u8, 20, 30, 40, 50, 60, 99];
    // since 1.88: the element type is [u8; 3], so indexing inside is checked at compile time.
    let (pixels, tail) = rgb.as_chunks::<3>();
    assert_eq!(
        (pixels, tail),
        (&[[10, 20, 30], [40, 50, 60]][..], &[99][..])
    );
    let (head, from_end) = rgb.as_rchunks::<3>(); // note the order: (remainder, chunks)
    assert_eq!(
        (head, from_end),
        (&[10][..], &[[20, 30, 40], [50, 60, 99]][..])
    );
    let mut buf = rgb;
    let (px, rest) = buf.as_chunks_mut::<3>();
    px.iter_mut().for_each(|[r, _, b]| std::mem::swap(r, b)); // RGB -> BGR
    rest.fill(0);
    assert_eq!(buf, [30, 20, 10, 60, 50, 40, 0]);
    let (lead, tails) = buf.as_rchunks_mut::<3>();
    lead[0] = 1;
    tails[1] = [0; 3];
    assert_eq!(buf, [1, 20, 10, 60, 0, 0, 0]);
    // array_windows (since 1.94): destructure each window directly.
    let rising = [1, 3, 7, 8].array_windows::<2>().all(|&[a, b]| a < b);
    assert!(rising);
    assert_eq!(
        b"abcabc"
            .array_windows::<3>()
            .filter(|&w| w == b"abc")
            .count(),
        2
    );
    println!("3. array chunks ok");
}

// ---- 4. chunk_by: runs of adjacent elements --------------------------------------------------------------------
fn rle(data: &[u8]) -> Vec<(u8, usize)> {
    data.chunk_by(|a, b| a == b)
        .map(|run| (run[0], run.len()))
        .collect()
}

fn section_chunk_by() {
    assert_eq!(
        rle(b"aaabccd"),
        [(b'a', 3), (b'b', 1), (b'c', 2), (b'd', 1)]
    );
    assert!(rle(b"").is_empty());
    // Grouping is by ADJACENCY, like SQL GROUP BY only after ORDER BY.
    let mut orders = [("bob", 5), ("ann", 1), ("bob", 2)];
    assert_eq!(orders.chunk_by(|a, b| a.0 == b.0).count(), 3);
    orders.sort_by_key(|o| o.0);
    let totals: Vec<(&str, i32)> = orders
        .chunk_by(|a, b| a.0 == b.0)
        .map(|g| (g[0].0, g.iter().map(|o| o.1).sum()))
        .collect();
    assert_eq!(totals, [("ann", 1), ("bob", 7)]);
    // The predicate sees NEIGHBOURS, not the first element of the run: this splits into consecutive sequences.
    let seqs = collect([1, 2, 3, 7, 8, 10].chunk_by(|a, b| a + 1 == *b));
    assert_eq!(seqs, [vec![1, 2, 3], vec![7, 8], vec![10]]);
    let mut v = [3, 1, 2, 9, 4];
    // Sort each ascending run boundary: here chunk_by_mut works on descending runs and reverses them.
    v.chunk_by_mut(|a, b| a > b).for_each(<[i32]>::reverse);
    assert_eq!(v, [1, 3, 2, 4, 9]);

    // Differential test: run-length encode then decode.
    let mut s = 5u64;
    for _ in 0..2000 {
        let data: Vec<u8> = (0..s % 40)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
                (s >> 61) as u8 % 3
            })
            .collect();
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
        let enc = rle(&data);
        assert_eq!(
            enc.iter()
                .flat_map(|&(b, n)| std::iter::repeat_n(b, n))
                .collect::<Vec<_>>(),
            data
        );
        assert!(enc.windows(2).all(|w| w[0].0 != w[1].0));
    }
    println!("4. chunk_by ok");
}

// ---- 5. split on a delimiter element ---------------------------------------------------------------------------
/// Frames are [len: u8][len bytes]. Returns complete frames and the unconsumed tail (a partial frame).
fn deframe(mut buf: &[u8]) -> (Vec<&[u8]>, &[u8]) {
    let mut frames = Vec::new();
    while let Some((&len, rest)) = buf.split_first() {
        let Some((body, next)) = rest.split_at_checked(usize::from(len)) else {
            break;
        };
        frames.push(body);
        buf = next;
    }
    (frames, buf)
}

fn section_split() {
    // Packet framing: split_first reads the header, split_at_checked takes the body only if all of it has arrived.
    let stream = [2, b'h', b'i', 0, 3, b'a', b'b'];
    let (frames, partial) = deframe(&stream);
    assert_eq!(frames, [&b"hi"[..], &b""[..]]);
    assert_eq!(partial, [3, b'a', b'b']); // keep it and append the next read
    let v = [1, 0, 2, 3, 0, 0, 4];
    let zero = |x: &i32| *x == 0;
    // Delimiters are removed; adjacent delimiters give EMPTY pieces.
    assert_eq!(
        collect(v.split(zero)),
        [vec![1], vec![2, 3], vec![], vec![4]]
    );
    assert_eq!(collect([0, 1, 0].split(zero)), [vec![], vec![1], vec![]]);
    assert_eq!(collect([0i32; 0].split(zero)), [Vec::<i32>::new()]); // empty input: one empty piece
    assert_eq!(
        collect(v.split_inclusive(zero)),
        [vec![1, 0], vec![2, 3, 0], vec![0], vec![4]]
    );
    assert_eq!(v.split_inclusive(zero).flatten().count(), v.len()); // nothing is lost
    // splitn(n): at most n pieces, the last one holds the unsplit rest.
    assert_eq!(collect(v.splitn(2, zero)), [vec![1], vec![2, 3, 0, 0, 4]]);
    assert_eq!(v.splitn(0, zero).count(), 0);
    // r* variants yield pieces from the back; contents are not reversed.
    assert_eq!(
        collect(v.rsplit(zero)),
        [vec![4], vec![], vec![2, 3], vec![1]]
    );
    assert_eq!(collect(v.rsplitn(2, zero)), [vec![4], vec![1, 0, 2, 3, 0]]);

    // NUL-separated records, ignoring the empty trailing piece.
    let wire = b"GET\0/key\0\0";
    let fields: Vec<&[u8]> = wire.split(|&b| b == 0).filter(|f| !f.is_empty()).collect();
    assert_eq!(fields, [&b"GET"[..], b"/key"]);

    // Mutable forms edit each piece in place.
    let mut a = [3, 1, 0, 9, 5];
    a.split_mut(zero).for_each(<[i32]>::sort_unstable);
    assert_eq!(a, [1, 3, 0, 5, 9]);
    let mut b = [1, 0, 1, 0, 1];
    b.splitn_mut(2, zero)
        .enumerate()
        .for_each(|(i, p)| p.fill(i as i32 + 7));
    assert_eq!(b, [7, 0, 8, 8, 8]);
    let mut c = [1, 0, 1, 0, 1];
    c.rsplit_mut(zero)
        .enumerate()
        .for_each(|(i, p)| p.fill(i as i32 + 7));
    assert_eq!(c, [9, 0, 8, 0, 7]);
    let mut d = [1, 0, 1, 0, 1];
    d.rsplitn_mut(2, zero)
        .enumerate()
        .for_each(|(i, p)| p.fill(i as i32 + 7));
    assert_eq!(d, [8, 8, 8, 0, 7]);
    let mut e = [5, 0, 6, 0];
    e.split_inclusive_mut(zero).for_each(<[i32]>::reverse);
    assert_eq!(e, [0, 5, 0, 6]);
    println!("5. split ok");
}

// ---- 6. split_at_mut and the borrow checker --------------------------------------------------------------------
/// Add row `src` into row `dst` of a row-major matrix. Two rows of one Vec need split_at_mut.
fn add_row(m: &mut [i32], cols: usize, src: usize, dst: usize) {
    assert_ne!(src, dst);
    let (lo, hi) = (src.min(dst), src.max(dst));
    let (top, bottom) = m.split_at_mut(hi * cols);
    let low_row = &mut top[lo * cols..(lo + 1) * cols];
    let high_row = &mut bottom[..cols];
    let (from, to) = if src < dst {
        (low_row, high_row)
    } else {
        (high_row, low_row)
    };
    to.iter_mut().zip(from.iter()).for_each(|(t, f)| *t += f);
}

fn section_split_at() {
    let v = [1, 2, 3, 4];
    assert_eq!(v.split_at(1), (&[1][..], &[2, 3, 4][..]));
    assert_eq!(v.split_at(4).1.len(), 0); // mid == len is fine
    assert!(panics(|| {
        let _ = [1].split_at(std::hint::black_box(2));
    }));
    assert_eq!(v.split_at_checked(5), None); // since 1.80
    let mut w = v;
    assert!(w.split_at_mut_checked(5).is_none());
    // `let a = &mut w[0]; let b = &mut w[1];` does not compile: both borrow all of w. These do.
    let (l, r) = w.split_at_mut(2);
    std::mem::swap(&mut l[0], &mut r[0]);
    assert_eq!(w, [3, 2, 1, 4]);
    if let Some((first, rest)) = w.split_first_mut() {
        rest.iter_mut().for_each(|x| *x += *first);
    }
    if let Some((last, init)) = w.split_last_mut() {
        init.iter_mut().for_each(|x| *x -= *last);
    }
    assert_eq!(w, [-4, -2, -3, 7]);

    // Matrix as one flat slice: rows are chunks, columns are step_by.
    let mut m = vec![1, 2, 3, 10, 20, 30];
    let cols = 3;
    let rows: Vec<i32> = m.chunks_exact(cols).map(|r| r.iter().sum()).collect();
    let col_sums: [i32; 3] = std::array::from_fn(|c| m.iter().skip(c).step_by(cols).sum());
    assert_eq!((rows, col_sums), (vec![6, 60], [11, 22, 33]));
    add_row(&mut m, cols, 0, 1);
    add_row(&mut m, cols, 1, 0);
    assert_eq!(m, [12, 24, 36, 11, 22, 33]);
    // Deinterleave stereo audio frames L R L R.
    let frames = [1i16, -1, 2, -2, 3, -3];
    let left: Vec<i16> = frames.iter().copied().step_by(2).collect();
    let right: Vec<i16> = frames.iter().copied().skip(1).step_by(2).collect();
    assert_eq!(
        (left.clone(), right.clone()),
        (vec![1, 2, 3], vec![-1, -2, -3])
    );
    let again: Vec<i16> = left
        .iter()
        .zip(&right)
        .flat_map(|(&l, &r)| [l, r])
        .collect();
    assert_eq!(again, frames);
    // as_ptr_range: the half-open address range; handy to ask "does this sub-slice come from that buffer?"
    let range = frames.as_ptr_range();
    assert!(range.contains(&frames[2..].as_ptr()));
    assert_eq!(
        range.end as usize - range.start as usize,
        std::mem::size_of_val(&frames)
    );
    println!("6. split_at_mut and layouts ok");
}

// ---- 7. Parallel processing of disjoint chunks -----------------------------------------------------------------
fn parallel_square(data: &mut [u64], workers: usize) -> u64 {
    if data.is_empty() {
        return 0;
    }
    let size = data.len().div_ceil(workers.max(1)); // never 0 here, so chunks_mut cannot panic
    std::thread::scope(|s| {
        let handles: Vec<_> = data
            .chunks_mut(size)
            .map(|part| {
                s.spawn(move || {
                    part.iter_mut().for_each(|x| *x *= *x);
                    part.iter().sum::<u64>()
                })
            })
            .collect(); // collect first: spawning lazily and joining in one chain would run them one at a time
        handles.into_iter().map(|h| h.join().expect("worker")).sum()
    })
}

fn section_parallel() {
    for (n, workers) in [
        (0usize, 4usize),
        (1, 4),
        (10, 3),
        (1000, 8),
        (1000, 1),
        (7, 100),
    ] {
        let mut data: Vec<u64> = (0..n as u64).collect();
        let expect: Vec<u64> = data.iter().map(|x| x * x).collect();
        assert_eq!(parallel_square(&mut data, workers), expect.iter().sum());
        assert_eq!(data, expect);
    }
    println!("7. scoped threads over chunks ok");
}

// ---- 8. Production tips and tricks -----------------------------------------------------------------------------
fn section_production_tips() {
    // 1. A chunk size from config can be 0: validate it before chunks(n).
    let n = 0usize;
    assert_eq!([1, 2, 3].chunks(n.max(1)).count(), 3);
    // 2. chunks_exact(n) lets the optimiser assume the length; index loops with manual bounds do not.
    // 3. Constant sizes: as_chunks::<N>() over chunks_exact + try_into().unwrap() (clippy::chunks_exact_to_as_chunks on new toolchains).
    let (words, rest) = [0u8, 0, 0, 5].as_chunks::<4>();
    assert!(rest.is_empty() && u32::from_be_bytes(words[0]) == 5);
    // 4. Never silently drop the remainder of external data: check it is empty or report an error.
    let frame_ok = |d: &[u8]| d.as_chunks::<4>().1.is_empty();
    assert!(frame_ok(&[0; 8]) && !frame_ok(&[0; 9]));
    // 5. Batch database writes with chunks to stay under parameter limits.
    let params_per_row = 4;
    let max_params = 10;
    assert_eq!(
        [(); 5]
            .chunks(max_params / params_per_row)
            .map(<[()]>::len)
            .collect::<Vec<_>>(),
        [2, 2, 1]
    );
    // 6. Manual `for i in 0..len/n { &v[i*n..(i+1)*n] }` is off-by-one bait: use chunks_exact.
    // 7. split_at_mut instead of unsafe or cloning when two parts of one buffer interact.
    // 8. Thread-per-chunk is for CPU-bound work; size chunks by core count (thread::available_parallelism), not by item.
    assert!(std::thread::available_parallelism().map_or(1, usize::from) >= 1);
    println!("8. production tips ok");
}

fn main() {
    let mut seen = HashSet::new();
    assert!(
        COVERED.iter().all(|m| seen.insert(*m)),
        "COVERED has duplicates"
    );
    section_everyday();
    section_mut_and_reverse();
    section_array_chunks();
    section_chunk_by();
    section_split();
    section_split_at();
    section_parallel();
    section_production_tips();
    println!(
        "covered {} chunk/window/split names; all asserts passed",
        COVERED.len()
    );
}
