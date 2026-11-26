//! RECIPE 1041 (std-iterators): every lazy Iterator adapter, with laziness proven by side-effect logs.
//! LEVEL: intermediate
//! Use in: any pipeline; look-up page for `map`, `filter_map`, `flat_map`, `peekable`, `scan`, `take_while`, `by_ref`...   Run: cargo run --release --example std_iterator_adapters_lazy
//! COMMON USES: log/CSV processing, tokenisers (peekable), pagination (flat_map), running totals (scan), batching, windowing, dedup of adjacent items.
//! METHODS: Iterator::map, Iterator::filter, Iterator::filter_map, Iterator::enumerate, Iterator::zip, Iterator::take, Iterator::skip,
//! Iterator::chain, Iterator::flat_map, Iterator::flatten, Iterator::rev, Iterator::cloned, Iterator::copied, Iterator::take_while,
//! Iterator::skip_while, Iterator::map_while, Iterator::step_by, Iterator::peekable, Peekable::peek, Peekable::peek_mut, Peekable::next_if,
//! Peekable::next_if_eq, Iterator::fuse, Iterator::inspect, Iterator::cycle, Iterator::scan, Iterator::by_ref, Iterator::unzip,
//! Iterator::zip, std::iter::zip, slice::windows, slice::chunks, Iterator::last
//! DEFINED IN: core: every adapter is an `Iterator` method in `core::iter`; `windows`/`chunks` are `core` slice methods. All of this works in no_std.
//! DOCS: https://doc.rust-lang.org/std/iter/trait.Iterator.html
//! NOT COVERED (and why): Iterator::array_chunks, Iterator::intersperse, Iterator::map_windows, Iterator::advance_by (nightly in 1.98, probed with rustc);
//! Iterator::partition_in_place, Iterator::collect_into (nightly). `windows`/`chunks` on slices are the stable stand-ins and ARE shown.
//!
//! Cheat sheet (I want to ... -> use ...):
//!   transform each item                      -> `map`
//!   drop items                               -> `filter`; drop and transform at once: `filter_map`
//!   each item expands to many                -> `flat_map` / `map(..).flatten()`; flatten Options/Results: `flatten()`
//!   index with item                          -> `enumerate` (then `.rev()` ORDER matters, see section 6)
//!   first n / all but first n                -> `take(n)` / `skip(n)`
//!   while a condition holds                  -> `take_while`; `skip_while`; and map+stop at first None: `map_while`
//!   every k-th                               -> `step_by(k)`
//!   join sequences / pair them               -> `chain` / `zip` (stops at the SHORTER)
//!   look ahead without consuming             -> `peekable()` + `peek` / `next_if` / `next_if_eq`
//!   running total / stateful map             -> `scan`
//!   debug a pipeline                         -> `inspect(|x| eprintln!(..))`
//!   repeat forever / round robin             -> `cycle` (+ `take`)
//!   use a prefix then continue               -> `by_ref()`
//!   iterate references as values             -> `copied()` (Copy) / `cloned()` (Clone)
//!   overlapping / non-overlapping groups     -> slice `windows(n)` / `chunks(n)`
//!   safe after first None                    -> `fuse`
//!
//! TRY NEXT:
//!   - [S] In section 1 remove the `.count()` at the end of the lazy pipeline; predict the log, then assert it is empty (nothing runs without a consumer).
//!   - [S] Section 6: swap `.enumerate().rev()` for `.rev().enumerate()` on a 4-item slice and predict the index of each item before running.
//!   - [M] Extend the `tokenize` function in section 8 with floating literals (`3.14`) using `next_if` only, no manual index.
//!   - [M] Write `dedup_adjacent` with `peekable` and `next_if_eq`; compare against `Vec::dedup` on 500 seeded random vectors.
//!   - [A] PROJECT: combine with `std_iterator_sources_custom` (pagination iterator) and `std_iterator_consumers` (try_fold): stream a fake paginated API through flat_map, stop on the first error, and assert how many pages were fetched (laziness).
//! Read: Iterator docs https://doc.rust-lang.org/std/iter/trait.Iterator.html ; Rust by Example iterators https://doc.rust-lang.org/rust-by-example/trait/iter.html
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::HashMap;

const COVERED: &[&str] = &[
    "map",
    "filter",
    "filter_map",
    "enumerate",
    "zip",
    "take",
    "skip",
    "chain",
    "flat_map",
    "flatten",
    "rev",
    "cloned",
    "copied",
    "take_while",
    "skip_while",
    "map_while",
    "step_by",
    "peekable",
    "peek",
    "peek_mut",
    "next_if",
    "next_if_eq",
    "fuse",
    "inspect",
    "cycle",
    "scan",
    "by_ref",
    "unzip",
    "iter::zip",
    "windows",
    "chunks",
    "last",
];

/// A tiny log used to prove laziness / evaluation order.
struct Log(RefCell<Vec<String>>);
impl Log {
    fn new() -> Self {
        Log(RefCell::new(Vec::new()))
    }
    fn add(&self, s: impl Into<String>) {
        self.0.borrow_mut().push(s.into());
    }
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

// ------------------------------------------------------------ 1. Everyday core: map/filter/filter_map/enumerate/zip/take/skip/chain
fn s01_everyday_core() {
    let log = Log::new();
    // Adapters do NOTHING until a consumer pulls. Building the pipeline logs nothing.
    let pipeline = (1..=5)
        .inspect(|x| log.add(format!("saw {x}")))
        .map(|x| x * 10)
        .filter(|x| *x != 30);
    assert!(log.take().is_empty(), "lazy: no work before consuming");
    let out: Vec<_> = pipeline.take(2).collect();
    assert_eq!(out, [10, 20]);
    // take(2) stopped pulling after the 2nd output: items 3,4,5 were never touched
    assert_eq!(log.take(), ["saw 1", "saw 2"]);

    // Interleaving: per-item, each stage runs before the next item starts (not stage-by-stage over the whole list)
    let order = Log::new();
    let _: Vec<_> = (1..=2)
        .inspect(|x| order.add(format!("a{x}")))
        .map(|x| x)
        .inspect(|x| order.add(format!("b{x}")))
        .collect();
    assert_eq!(order.take(), ["a1", "b1", "a2", "b2"]);

    let lines = ["10", "x", "30", ""];
    let nums: Vec<i32> = lines.iter().filter_map(|s| s.parse().ok()).collect(); // map+filter+unwrap in one
    assert_eq!(nums, [10, 30]);
    let named: Vec<String> = ["a", "b"]
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{i}:{s}"))
        .collect();
    assert_eq!(named, ["0:a", "1:b"]);
    assert_eq!(
        (1..).zip("abc".chars()).collect::<Vec<_>>(),
        [(1, 'a'), (2, 'b'), (3, 'c')]
    ); // zip stops at shorter
    assert_eq!((0..10).skip(7).collect::<Vec<_>>(), [7, 8, 9]);
    assert_eq!((0..3).chain(10..12).collect::<Vec<_>>(), [0, 1, 2, 10, 11]);
    // chain + once: prepend/append a single value without allocating
    assert_eq!(
        std::iter::once(0)
            .chain(5..7)
            .chain(std::iter::once(99))
            .collect::<Vec<_>>(),
        [0, 5, 6, 99]
    );
    println!("1. everyday core ok");
}

// ------------------------------------------------------------ 2. flat_map / flatten
fn s02_flat_map_flatten() {
    // Scenario: pagination: each page yields several records; flat_map turns pages into one record stream.
    let pages: Vec<Vec<u32>> = vec![vec![1, 2], vec![], vec![3]];
    let all: Vec<u32> = pages.iter().flat_map(|p| p.iter().copied()).collect();
    assert_eq!(all, [1, 2, 3]);
    assert_eq!(pages.iter().flatten().count(), 3); // flatten == flat_map(identity)
    // flatten on Option / Result: skip Nones / Errs. Option and Result are IntoIterator (0 or 1 items)
    let opts = [Some(1), None, Some(3)];
    assert_eq!(opts.iter().flatten().sum::<i32>(), 4);
    let results = ["1", "x", "3"].iter().map(|s| s.parse::<i32>());
    assert_eq!(results.flatten().collect::<Vec<_>>(), [1, 3]); // silently drops errors: know what you are throwing away!
    // flat_map over chars: expand words to characters
    let letters: String = ["ab", "cd"].iter().flat_map(|w| w.chars().rev()).collect();
    assert_eq!(letters, "badc");
    // Cartesian product with nested flat_map + map
    let pairs: Vec<_> = (0..2).flat_map(|a| (0..2).map(move |b| (a, b))).collect();
    assert_eq!(pairs, [(0, 0), (0, 1), (1, 0), (1, 1)]);
    println!("2. flat_map/flatten ok");
}

// ------------------------------------------------------------ 3. take_while / skip_while / map_while / step_by
fn s03_prefix_adapters() {
    let data = [1, 2, 3, 10, 4, 5];
    assert_eq!(
        data.iter().take_while(|&&x| x < 5).collect::<Vec<_>>(),
        [&1, &2, &3]
    ); // stops at the first failure for good
    assert_eq!(
        data.iter().skip_while(|&&x| x < 5).collect::<Vec<_>>(),
        [&10, &4, &5]
    ); // NOT a filter: resumes everything after
    // PITFALL: take_while consumes the first failing element; if you need it, use peekable or by_ref + find
    let mut it = data.iter();
    let head: Vec<_> = it.by_ref().take_while(|&&x| x < 5).collect();
    assert_eq!(head.len(), 3);
    assert_eq!(it.next(), Some(&4)); // the `10` is gone: consumed by take_while
    // map_while: map and stop at the first None (parse until the first bad token)
    let parsed: Vec<u8> = ["7", "8", "x", "9"]
        .iter()
        .map_while(|s| s.parse().ok())
        .collect();
    assert_eq!(parsed, [7, 8]);
    // filter_map would have kept going: contrast
    let all: Vec<u8> = ["7", "8", "x", "9"]
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    assert_eq!(all, [7, 8, 9]);
    assert_eq!((0..20).step_by(5).collect::<Vec<_>>(), [0, 5, 10, 15]); // first item always included
    assert_eq!(('a'..='z').step_by(10).collect::<String>(), "aku");
    println!("3. prefix adapters ok");
}

// ------------------------------------------------------------ 4. peekable: peek / next_if / next_if_eq / peek_mut
/// Scenario: parse "1,2,,3" style groups where the delimiter decides what happens next.
fn s04_peekable() {
    let mut it = [1, 1, 2, 3, 3, 3].into_iter().peekable();
    assert_eq!(it.peek(), Some(&1)); // peek does not consume
    assert_eq!(it.peek(), Some(&1));
    // run-length encode via next_if_eq: consume while the next equals the current
    let mut rle = Vec::new();
    while let Some(x) = it.next() {
        let mut n = 1;
        while it.next_if_eq(&x).is_some() {
            n += 1;
        }
        rle.push((x, n));
    }
    assert_eq!(rle, [(1, 2), (2, 1), (3, 3)]);
    // next_if with a predicate: consume leading whitespace/digits
    let mut chars = "  42abc".chars().peekable();
    while chars.next_if(|c| c.is_whitespace()).is_some() {}
    let digits: String = std::iter::from_fn(|| chars.next_if(char::is_ascii_digit)).collect();
    assert_eq!(digits, "42");
    assert_eq!(chars.collect::<String>(), "abc");
    // peek_mut: modify the upcoming item in place (e.g. merge a carried value into the next)
    let mut it = [10, 20].into_iter().peekable();
    if let Some(next) = it.peek_mut() {
        *next += 1;
    }
    assert_eq!(it.next(), Some(11));
    // PITFALL: peek takes &mut self (it may have to pull from the source). Hold only one peeked ref at a time.
    println!("4. peekable ok");
}

// ------------------------------------------------------------ 5. scan / fuse / inspect / cycle
fn s05_stateful() {
    // scan: running total with state; returning None ends the iteration (like map_while)
    let running: Vec<i32> = [1, 2, 3, 4]
        .iter()
        .scan(0, |acc, &x| {
            *acc += x;
            Some(*acc)
        })
        .collect();
    assert_eq!(running, [1, 3, 6, 10]);
    // budget: stop when the total would exceed 6
    let within: Vec<i32> = [1, 2, 3, 4]
        .iter()
        .scan(0, |acc, &x| {
            *acc += x;
            (*acc <= 6).then_some(x)
        })
        .collect();
    assert_eq!(within, [1, 2, 3]);
    // cycle + take: round robin assignment
    let workers = ["w0", "w1", "w2"];
    let assign: Vec<_> = (0..5)
        .zip(workers.iter().cycle())
        .map(|(job, w)| format!("{job}->{w}"))
        .collect();
    assert_eq!(assign, ["0->w0", "1->w1", "2->w2", "3->w0", "4->w1"]);
    assert_eq!(std::iter::empty::<i32>().cycle().next(), None); // cycle of empty is empty, not an infinite loop
    // fuse: after the first None, a misbehaving iterator may yield Some again; fuse guarantees None forever
    struct Flaky(u8);
    impl Iterator for Flaky {
        type Item = u8;
        fn next(&mut self) -> Option<u8> {
            self.0 += 1;
            if self.0 == 2 { None } else { Some(self.0) }
        }
    }
    let raw: Vec<_> = {
        let mut f = Flaky(0);
        vec![f.next(), f.next(), f.next()]
    };
    assert_eq!(raw, [Some(1), None, Some(3)]);
    let mut fused = Flaky(0).fuse();
    assert_eq!(
        (fused.next(), fused.next(), fused.next()),
        (Some(1), None, None)
    );
    // inspect: debug without changing the pipeline
    let seen = Log::new();
    let s: i32 = (1..=3).inspect(|x| seen.add(x.to_string())).sum();
    assert_eq!((s, seen.take().join("")), (6, "123".to_string()));
    println!("5. stateful ok");
}

// ------------------------------------------------------------ 6. rev / enumerate order pitfalls
fn s06_rev_enumerate() {
    let v = ['a', 'b', 'c'];
    // enumerate().rev(): indices stay attached to their items (needs ExactSize)
    assert_eq!(
        v.iter()
            .enumerate()
            .rev()
            .map(|(i, c)| (i, *c))
            .collect::<Vec<_>>(),
        [(2, 'c'), (1, 'b'), (0, 'a')]
    );
    // rev().enumerate(): indices count in the new order
    assert_eq!(
        v.iter()
            .rev()
            .enumerate()
            .map(|(i, c)| (i, *c))
            .collect::<Vec<_>>(),
        [(0, 'c'), (1, 'b'), (2, 'a')]
    );
    // rev needs DoubleEnded: chars().rev() is fine, but Chars of non-ASCII is still correct (per char, not per byte)
    assert_eq!("héé".chars().rev().collect::<String>(), "ééh");
    // skip + rev vs rev + skip
    assert_eq!((0..5).skip(1).rev().collect::<Vec<_>>(), [4, 3, 2, 1]);
    assert_eq!((0..5).rev().skip(1).collect::<Vec<_>>(), [3, 2, 1, 0]);
    // zip of three: nest zips and destructure with ((a, b), c)
    let (xs, ys, zs) = ([1, 2, 3], [10, 20, 30], [100, 200, 300]);
    let sums: Vec<_> = xs
        .iter()
        .zip(&ys)
        .zip(&zs)
        .map(|((a, b), c)| a + b + c)
        .collect();
    assert_eq!(sums, [111, 222, 333]);
    // std::iter::zip (1.59): same, reads left to right for IntoIterators
    assert_eq!(std::iter::zip([1, 2], ["a", "b"]).count(), 2);
    println!("6. rev/enumerate ok");
}

// ------------------------------------------------------------ 7. by_ref / references vs values / windows / chunks
fn s07_by_ref_and_refs() {
    // by_ref: parse a header (first 2 lines) then process the body with the same iterator
    let text = "H1\nH2\nb1\nb2\nb3";
    let mut lines = text.lines();
    let header: Vec<_> = lines.by_ref().take(2).collect();
    let body: Vec<_> = lines.collect();
    assert_eq!((header.len(), body.len()), (2, 3));
    // WITHOUT by_ref, `take` takes ownership of the iterator (the compiler stops you).

    // References vs values: iter() -> &T, into_iter() on Vec -> T, iter().copied() -> T (no move out of the Vec)
    let v = vec![String::from("a"), String::from("b")];
    let lens: Vec<usize> = v.iter().map(|s| s.len()).collect(); // borrows; v still usable
    assert_eq!(lens, [1, 1]);
    let cloned: Vec<String> = v.iter().cloned().collect(); // clone each (allocates)
    assert_eq!(cloned, v);
    let owned: Vec<String> = v.into_iter().collect(); // moves, v gone
    assert_eq!(owned.len(), 2);
    let nums = [1, 2, 3];
    let doubled: Vec<i32> = nums.iter().copied().map(|x| x * 2).collect(); // copied is free for Copy types
    assert_eq!(doubled, [2, 4, 6]);
    let mut m = [1, 2, 3];
    m.iter_mut().for_each(|x| *x *= 10); // iter_mut yields &mut T
    assert_eq!(m, [10, 20, 30]);

    // Stand-ins for the nightly array_chunks / map_windows: slice windows and chunks
    let s = [1, 2, 3, 4, 5];
    let diffs: Vec<_> = s.windows(2).map(|w| w[1] - w[0]).collect();
    assert_eq!(diffs, [1, 1, 1, 1]);
    let sums: Vec<_> = s.chunks(2).map(|c| c.iter().sum::<i32>()).collect();
    assert_eq!(sums, [3, 7, 5]); // last chunk shorter
    // unzip for splitting pairs after a map
    let (sq, cu): (Vec<_>, Vec<_>) = (1..4).map(|x| (x * x, x * x * x)).unzip();
    assert_eq!((sq, cu), (vec![1, 4, 9], vec![1, 8, 27]));
    println!("7. by_ref/refs ok");
}

// ------------------------------------------------------------ 8. Scenario: tokenizer with peekable
#[derive(Debug, PartialEq)]
enum Tok {
    Num(u64),
    Ident(String),
    Plus,
    Star,
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let mut it = src.chars().peekable();
    let mut out = Vec::new();
    while let Some(&c) = it.peek() {
        match c {
            c if c.is_whitespace() => {
                it.next();
            }
            '+' => {
                it.next();
                out.push(Tok::Plus);
            }
            '*' => {
                it.next();
                out.push(Tok::Star);
            }
            '0'..='9' => {
                let mut n = 0u64;
                while let Some(d) = it.next_if(char::is_ascii_digit) {
                    n = n
                        .checked_mul(10)
                        .and_then(|n| n.checked_add(d.to_digit(10).unwrap() as u64))
                        .ok_or("number too large")?;
                }
                out.push(Tok::Num(n));
            }
            c if c.is_alphabetic() => {
                let mut s = String::new();
                while let Some(ch) = it.next_if(|c| c.is_alphanumeric()) {
                    s.push(ch);
                }
                out.push(Tok::Ident(s));
            }
            other => return Err(format!("unexpected {other:?}")),
        }
    }
    Ok(out)
}

fn s08_tokenizer() {
    assert_eq!(
        tokenize("x1 + 42*y").unwrap(),
        [
            Tok::Ident("x1".into()),
            Tok::Plus,
            Tok::Num(42),
            Tok::Star,
            Tok::Ident("y".into())
        ]
    );
    assert!(tokenize("1 $ 2").is_err());
    assert_eq!(
        tokenize("99999999999999999999").unwrap_err(),
        "number too large"
    );
    println!("8. tokenizer ok");
}

// ------------------------------------------------------------ 9. Production tips and tricks
fn s09_production_tips() {
    // 1. Don't collect between adapters (clippy::needless_collect): the whole chain should be ONE pass.
    let total: i32 = (1..=10).filter(|x| x % 2 == 0).map(|x| x * x).sum();
    assert_eq!(total, 220);

    // 2. `filter_map(|x| x.ok())` hides errors; prefer `flatten()` only when drop is intended, else collect::<Result<..>>
    //    (clippy::manual_flatten, clippy::lines_filter_map_ok for io lines: a read error would loop forever on some readers!).
    let r: Result<Vec<i32>, _> = ["1", "x"].iter().map(|s| s.parse::<i32>()).collect();
    assert!(r.is_err());

    // 3. `.map(|x| x.clone())` -> `.cloned()`, `.map(|x| *x)` -> `.copied()` (clippy::map_clone).
    let v = vec![1, 2];
    assert_eq!(v.iter().copied().collect::<Vec<_>>(), v);

    // 4. `.filter(..).next()` -> `.find(..)` (clippy::filter_next); `.find().is_some()` -> `.any()` (clippy::search_is_some).
    assert_eq!((1..9).find(|x| x % 4 == 0), Some(4));

    // 5. `.map(..).flatten()` -> `.flat_map(..)` (clippy::map_flatten).
    assert_eq!(["a b", "c"].iter().flat_map(|s| s.split(' ')).count(), 3);

    // 6. `.iter().skip(n).next()` -> `.nth(n)` (clippy::iter_skip_next); `.step_by` instead of `% k == 0` filters (O(n/k) vs O(n)).
    let touched = RefCell::new(0);
    let n = (0..100)
        .inspect(|_| *touched.borrow_mut() += 1)
        .step_by(10)
        .count();
    assert_eq!(n, 10);
    // NOTE: for Range, step_by is specialised (touched < 100); for generic iterators it pulls every item. Measure, don't assume.
    assert!(*touched.borrow() <= 100);

    // 7. Infinite iterators need an explicit bound: `.take(n)`; `cycle` of an empty iterator ends silently (checked above).
    assert_eq!(std::iter::repeat("x").take(3).collect::<String>(), "xxx"); // clippy::manual_repeat_n suggests repeat_n(..)
    assert_eq!(std::iter::repeat_n("x", 3).collect::<String>(), "xxx"); // since 1.82

    // 8. Use `scan` or `fold` for running state instead of a `let mut` captured by a `map` closure (map with side effects is a smell).
    let ids: Vec<u32> = ["a", "b"]
        .iter()
        .scan(100, |next, _| {
            *next += 1;
            Some(*next)
        })
        .collect();
    assert_eq!(ids, [101, 102]);

    // 9. Group consecutive items without itertools: peekable + next_if (see section 4), or `chunk_by` on SLICES (stable 1.77).
    let runs: Vec<&[i32]> = [1, 1, 2, 3, 3].chunk_by(|a, b| a == b).collect();
    assert_eq!(runs, [&[1, 1][..], &[2][..], &[3, 3][..]]);

    // 10. Return `impl Iterator<Item = T> + '_` from methods rather than Vec; callers can stop early (see std_iterator_sources_custom).
    fn big_ones(v: &[u32]) -> impl Iterator<Item = &u32> + '_ {
        v.iter().filter(|x| **x > 5)
    }
    assert_eq!(big_ones(&[1, 9, 7]).next(), Some(&9));

    // 11. Counting words: `entry` in a `for` loop is clearer than `fold` building a map (and avoids moving the map each step).
    let mut freq: HashMap<&str, usize> = HashMap::new();
    for w in "a b a".split(' ') {
        *freq.entry(w).or_default() += 1;
    }
    assert_eq!(freq["a"], 2);

    // 12. Deterministic debug output of lazy pipelines: log with inspect into a Vec, assert on the Vec, don't println in tests.
    let log = Log::new();
    let _ = (1..4).inspect(|x| log.add(x.to_string())).max();
    assert_eq!(log.take(), ["1", "2", "3"]);
    println!("9. production tips ok");
}

fn main() {
    let mut s = COVERED.to_vec();
    s.sort();
    s.dedup();
    assert_eq!(s.len(), COVERED.len(), "COVERED has duplicates");

    s01_everyday_core();
    s02_flat_map_flatten();
    s03_prefix_adapters();
    s04_peekable();
    s05_stateful();
    s06_rev_enumerate();
    s07_by_ref_and_refs();
    s08_tokenizer();
    s09_production_tips();
    println!("all adapter sections passed");
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
