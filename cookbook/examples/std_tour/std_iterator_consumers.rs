//! RECIPE 1040 (std-iterators): every consuming Iterator method, with a use case each.
//! LEVEL: intermediate
//! Use in: any data pipeline; look-up page for `Iterator::fold`, `try_fold`, `collect`, `partition`...   Run: cargo run --release --example std_iterator_consumers
//! COMMON USES: parsing and validating input (collect into Result), aggregates (sum/min/max/fold), searching (find/position),
//! grouping (collect into maps), early-exit loops (try_fold / try_for_each), comparing sequences (eq/cmp/lt).
//! METHODS: Iterator::collect, Iterator::sum, Iterator::count, Iterator::max, Iterator::min, Iterator::fold, Iterator::any, Iterator::all,
//! Iterator::find, Iterator::position, Iterator::for_each, Iterator::last, Iterator::nth, Iterator::product, Iterator::min_by_key,
//! Iterator::max_by_key, Iterator::max_by, Iterator::min_by, Iterator::find_map, Iterator::rposition, Iterator::reduce, Iterator::try_fold,
//! Iterator::try_for_each, Iterator::unzip, Iterator::partition, Iterator::cmp, Iterator::partial_cmp, Iterator::eq, Iterator::ne,
//! Iterator::lt, Iterator::le, Iterator::gt, Iterator::ge, Iterator::is_sorted, Iterator::is_sorted_by, Iterator::is_sorted_by_key,
//! Iterator::size_hint, Iterator::rev, Iterator::rfold, Iterator::rfind, DoubleEndedIterator::next_back, DoubleEndedIterator::nth_back,
//! ExactSizeIterator::len, FromIterator::from_iter, Extend::extend, std::iter::Sum, std::iter::Product, Iterator::by_ref
//! DEFINED IN: core: `Iterator`, `DoubleEndedIterator`, `ExactSizeIterator`, `FromIterator`, `Extend`, `Sum`, `Product` live in `core::iter` (std re-exports them).
//!   The `collect` TARGET impls live with the type: `Vec`, `String`, `VecDeque`, `BTreeMap`, `Box<[T]>` in alloc; `HashMap`/`HashSet` in std.
//! DOCS: https://doc.rust-lang.org/std/iter/trait.Iterator.html
//! NOT COVERED (and why): Iterator::try_reduce, Iterator::try_collect, Iterator::collect_into, Iterator::partition_in_place,
//! Iterator::is_partitioned, Iterator::array_chunks, Iterator::intersperse, Iterator::map_windows, Iterator::cmp_by (all unstable in 1.98; probed with rustc);
//! adapters live in std_iterator_adapters_lazy.
//!
//! Cheat sheet (I want to ... -> use ...):
//!   count items                         -> `count()` (consumes; use `len()` on ExactSize instead when you can)
//!   total / product                     -> `sum::<T>()` / `product::<T>()`  (annotate the type!)
//!   biggest / smallest                  -> `max()` / `min()`; by a key: `max_by_key`; floats: `max_by(|a,b| a.total_cmp(b))`
//!   custom accumulate                   -> `fold(init, |acc, x| ..)`; no init value: `reduce`
//!   stop early on error                 -> `try_fold` / `try_for_each` (or `collect::<Result<_,_>>()`)
//!   does any / every item match         -> `any` / `all` (short-circuit)
//!   first match / its index             -> `find` / `position`; last index: `rposition`; first Some(map): `find_map`
//!   validate every item                 -> `collect::<Result<Vec<_>, E>>()` (first Err wins)
//!   skip the failures                   -> `filter_map(Result::ok)` or `flatten()`
//!   split in two by predicate           -> `partition`
//!   split pairs into two Vecs           -> `unzip`
//!   to a map / set / string / deque     -> `collect::<HashMap<_,_>>()`, `BTreeSet`, `String`, `VecDeque`
//!   compare two sequences               -> `eq`, `cmp`, `lt` (lexicographic)
//!   is it sorted                        -> `is_sorted()` / `is_sorted_by_key`
//!   last / nth element                  -> `last()` (walks everything!) / `nth(n)`; from the back: `next_back`, `nth_back`
//!   sum Options / Results               -> `sum::<Option<i32>>()` (None if any None)
//!   reason about length                 -> `size_hint()` (lower, Some(upper)); `ExactSizeIterator::len`
//!
//! TRY NEXT:
//!   - [S] In section 3 swap `try_fold` + `checked_add` for `fold` + `wrapping_add`; predict the overflow result, then assert it.
//!   - [S] What does `max_by_key` return on ties vs `min_by_key`? Predict, then add an assertion with equal keys (see section 4).
//!   - [M] Port section 5's `collect::<Result<..>>` to parse a real `KEY=VALUE` env file from disk and report the line number of the first bad line.
//!   - [M] Write `fn mode<T: Hash+Eq+Clone>(it: impl Iterator<Item=T>) -> Option<T>` using only consumers from this file and test against a sort-based oracle.
//!   - [A] PROJECT: combine with `std_hashmap_entry_api_patterns` (grouping) and `std_btree_binaryheap_ordered` (top-k): stream a 1e6-line seeded log, group by status with `collect`/entry, report top-3 groups with a bounded heap.
//! Read: The Rust Book ch13-02 https://doc.rust-lang.org/book/ch13-02-iterators.html ; std::iter module docs https://doc.rust-lang.org/std/iter/index.html
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::ops::ControlFlow;

const COVERED: &[&str] = &[
    "collect",
    "sum",
    "count",
    "max",
    "min",
    "fold",
    "any",
    "all",
    "find",
    "position",
    "for_each",
    "last",
    "nth",
    "product",
    "min_by_key",
    "max_by_key",
    "max_by",
    "min_by",
    "find_map",
    "rposition",
    "reduce",
    "try_fold",
    "try_for_each",
    "unzip",
    "partition",
    "cmp",
    "partial_cmp",
    "eq",
    "ne",
    "lt",
    "le",
    "gt",
    "ge",
    "is_sorted",
    "is_sorted_by",
    "is_sorted_by_key",
    "size_hint",
    "rev",
    "rfold",
    "rfind",
    "next_back",
    "nth_back",
    "len",
    "from_iter",
    "extend",
    "by_ref",
    "total_cmp_lexicographic",
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

// ---------------------------------------------------------------- 1. Everyday core
/// Scenario: summarise an order list. count/sum/min/max/fold/any/all/find/position cover 90% of real use.
fn s01_everyday_core() {
    let prices = [12u32, 5, 99, 5, 40];
    assert_eq!(prices.iter().count(), 5);
    assert_eq!(prices.iter().sum::<u32>(), 161); // sum needs a type annotation: Sum is generic over output
    assert_eq!(prices.iter().copied().max(), Some(99));
    assert_eq!(prices.iter().copied().min(), Some(5));
    // fold: anything the dedicated consumers can't do, e.g. min and max in ONE pass
    let (lo, hi) = prices
        .iter()
        .fold((u32::MAX, u32::MIN), |(lo, hi), &p| (lo.min(p), hi.max(p)));
    assert_eq!((lo, hi), (5, 99));
    assert!(prices.iter().any(|&p| p > 90)); // short-circuits at the first hit
    assert!(prices.iter().all(|&p| p > 0));
    assert_eq!(prices.iter().find(|&&p| p < 10), Some(&5)); // find yields a reference to the item
    assert_eq!(prices.iter().position(|&p| p == 5), Some(1)); // first index
    assert_eq!(prices.iter().rposition(|&p| p == 5), Some(3)); // last index (needs ExactSize + DoubleEnded)
    let mut seen = Vec::new();
    prices.iter().for_each(|p| seen.push(*p)); // prefer a `for` loop unless you are ending a long chain
    assert_eq!(seen.len(), 5);
    assert_eq!(prices.iter().last(), Some(&40));
    assert_eq!(prices.iter().nth(2), Some(&99));
    // PITFALL: nth consumes. A second nth continues AFTER the first.
    let mut it = prices.iter();
    assert_eq!(it.nth(1), Some(&5));
    assert_eq!(it.nth(1), Some(&5)); // index 3 of the original, not 1 again
    // PITFALL: sum overflow panics in debug and wraps in release; use try_fold + checked_add when input is untrusted.
    let big = [u8::MAX, 1];
    assert_eq!(big.iter().try_fold(0u8, |a, &x| a.checked_add(x)), None);
    println!("1. everyday core ok");
}

// ---------------------------------------------------------------- 2. product, last, nth, ranges
fn s02_product_last_nth() {
    let fact = |n: u64| (1..=n).product::<u64>();
    assert_eq!(fact(0), 1); // empty product is the multiplicative identity
    assert_eq!(fact(10), 3_628_800);
    assert_eq!((1..=0u32).sum::<u32>(), 0); // empty sum is 0
    assert_eq!((1..=1000).last(), Some(1000));
    assert_eq!((0..100).nth(99), Some(99));
    assert_eq!((0..100).nth(100), None);
    // floats: sum works, max does not (f64 is not Ord)
    let xs = [2.5f64, -1.0, 9.25];
    assert_eq!(xs.iter().sum::<f64>(), 10.75);
    let max = xs.iter().copied().max_by(|a, b| a.total_cmp(b));
    assert_eq!(max, Some(9.25));
    let min = xs.iter().copied().min_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(min, Some(-1.0));
    println!("2. product/last/nth ok");
}

// ---------------------------------------------------------------- 3. try_fold / try_for_each / reduce
/// Scenario: apply a ledger of deltas to a balance, aborting on the first overdraft.
fn apply_ledger(start: i64, deltas: &[i64]) -> Result<i64, String> {
    deltas.iter().enumerate().try_fold(start, |bal, (i, d)| {
        let nb = bal
            .checked_add(*d)
            .ok_or_else(|| format!("overflow at {i}"))?;
        if nb < 0 {
            Err(format!("overdraft at entry {i}: {nb}"))
        } else {
            Ok(nb)
        }
    })
}

fn s03_try_and_reduce() {
    assert_eq!(apply_ledger(10, &[5, -3, -12]), Ok(0));
    assert_eq!(
        apply_ledger(10, &[5, -20, 1000]),
        Err("overdraft at entry 1: -5".into())
    );
    assert_eq!(apply_ledger(i64::MAX, &[1]), Err("overflow at 0".into()));
    // try_fold with ControlFlow: stop at a sentinel without using Result for non-errors
    let r = [1, 2, 3, 99, 5].iter().try_fold(0, |acc, &x| {
        if x == 99 {
            ControlFlow::Break(acc)
        } else {
            ControlFlow::Continue(acc + x)
        }
    });
    assert_eq!(r, ControlFlow::Break(6));
    // try_for_each: side effects with early exit
    let mut out = Vec::new();
    let res: Result<(), String> = ["1", "2", "x", "4"].iter().try_for_each(|s| {
        out.push(s.parse::<i32>().map_err(|e| format!("{s}: {e}"))?);
        Ok(())
    });
    assert!(res.is_err());
    assert_eq!(out, vec![1, 2]); // "4" never visited
    // reduce: fold without an identity element; None on empty
    assert_eq!((1..5).reduce(|a, b| a * b), Some(24));
    assert_eq!(std::iter::empty::<i32>().reduce(|a, b| a + b), None);
    let longest = ["aa", "b", "cccc", "dd"]
        .into_iter()
        .reduce(|a, b| if b.len() > a.len() { b } else { a });
    assert_eq!(longest, Some("cccc"));
    // rfold folds from the back: handy for building right-associated structures
    let s = ["a", "b", "c"]
        .iter()
        .rfold(String::new(), |acc, x| acc + x);
    assert_eq!(s, "cba");
    println!("3. try_fold/reduce ok");
}

// ---------------------------------------------------------------- 4. min/max by key and ties
#[derive(Debug, PartialEq)]
struct Server {
    name: &'static str,
    load: u32,
}

fn s04_extrema_by() {
    let fleet = [
        Server { name: "a", load: 7 },
        Server { name: "b", load: 3 },
        Server { name: "c", load: 7 },
        Server { name: "d", load: 3 },
    ];
    // Tie rule (documented): max_by_key returns the LAST maximum, min_by_key the FIRST minimum.
    assert_eq!(fleet.iter().max_by_key(|s| s.load).unwrap().name, "c");
    assert_eq!(fleet.iter().min_by_key(|s| s.load).unwrap().name, "b");
    // Want the first max instead? Compare with reversed tie-break:
    let first_max = fleet.iter().rev().max_by_key(|s| s.load).unwrap();
    assert_eq!(first_max.name, "a");
    // max_by with a float key
    let temps = [("x", 20.5f64), ("y", 31.0), ("z", 18.25)];
    let hottest = temps.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
    assert_eq!(hottest.0, "y");
    // Reverse key for "smallest by descending"
    let k = fleet
        .iter()
        .min_by_key(|s| std::cmp::Reverse(s.load))
        .unwrap();
    assert_eq!(k.name, "a"); // first minimum of the reversed key == first max
    println!("4. extrema ok");
}

// ---------------------------------------------------------------- 5. find / find_map / position
fn s05_search() {
    let lines = ["# comment", "", "port=8080", "host=x", "port=9"];
    // find_map: search and transform in one step (instead of find + map + unwrap)
    let port = lines
        .iter()
        .find_map(|l| l.strip_prefix("port=")?.parse::<u16>().ok());
    assert_eq!(port, Some(8080)); // first match wins
    // rfind: last matching
    assert_eq!(
        lines.iter().rfind(|l| l.starts_with("port=")),
        Some(&"port=9")
    );
    assert_eq!(lines.iter().position(|l| l.is_empty()), Some(1));
    // `find` on `iter()` yields &&T; use `copied()` or patterns to clean up
    let nums = [3, 8, 12];
    assert_eq!(nums.iter().find(|&&n| n > 5), Some(&8));
    assert_eq!(nums.iter().copied().find(|n| *n > 5), Some(8));
    println!("5. search ok");
}

// ---------------------------------------------------------------- 6. collect targets
fn parse_kv(l: &str) -> Result<(String, u32), String> {
    let (k, v) = l
        .split_once('=')
        .ok_or_else(|| format!("no '=' in {l:?}"))?;
    Ok((
        k.trim().to_string(),
        v.trim().parse().map_err(|e| format!("{l:?}: {e}"))?,
    ))
}

fn s06_collect_targets() {
    let words = ["pear", "fig", "apple", "fig"];
    let v: Vec<&str> = words.iter().copied().collect();
    assert_eq!(v.len(), 4);
    let s: String = words.iter().map(|w| &w[..1]).collect(); // String: FromIterator<&str> and <char>
    assert_eq!(s, "pfaf");
    let joined: String = words.iter().copied().collect::<Vec<_>>().join(",");
    assert_eq!(joined, "pear,fig,apple,fig");
    let set: HashSet<_> = words.iter().collect();
    assert_eq!(set.len(), 3);
    let bset: BTreeSet<_> = words.iter().collect();
    assert_eq!(bset.into_iter().next(), Some(&"apple")); // ordered
    // HashMap from pairs: later duplicates overwrite earlier ones
    let m: HashMap<&str, usize> = words.iter().map(|w| (*w, w.len())).collect();
    assert_eq!(m["fig"], 3);
    let dup: BTreeMap<_, _> = [("k", 1), ("k", 2)].into_iter().collect();
    assert_eq!(dup["k"], 2);
    let bx: Box<[u32]> = (1..4).collect(); // exact-size boxed slice, no spare capacity
    assert_eq!(&*bx, &[1, 2, 3]);
    let dq: VecDeque<_> = (1..4).collect();
    assert_eq!(dq.front(), Some(&1));
    // Result / Option short-circuit
    let ok: Result<Vec<_>, _> = ["a=1", "b=2"].iter().map(|l| parse_kv(l)).collect();
    assert_eq!(ok.unwrap().len(), 2);
    let bad: Result<Vec<_>, _> = ["a=1", "oops", "c=x"].iter().map(|l| parse_kv(l)).collect();
    assert_eq!(bad.unwrap_err(), "no '=' in \"oops\"".to_string()); // FIRST error only
    let opt: Option<Vec<u32>> = ["1", "2"].iter().map(|s| s.parse().ok()).collect();
    assert_eq!(opt, Some(vec![1, 2]));
    let none: Option<Vec<u32>> = ["1", "z"].iter().map(|s| s.parse().ok()).collect();
    assert_eq!(none, None);
    // collect Results into (), just to run fallible effects
    let unit: Result<(), String> = ["a=1"].iter().map(|l| parse_kv(l).map(|_| ())).collect();
    assert!(unit.is_ok());
    // collect::<Vec<Result>> then split, when you need ALL errors
    let (goods, errs): (Vec<_>, Vec<_>) = ["a=1", "oops", "c=x"]
        .iter()
        .map(|l| parse_kv(l))
        .partition(Result::is_ok);
    assert_eq!((goods.len(), errs.len()), (1, 2));
    // extend reuses an existing collection
    let mut acc = vec![0];
    acc.extend((1..4).map(|x| x * 10));
    assert_eq!(acc, [0, 10, 20, 30]);
    println!("6. collect targets ok");
}

// ---------------------------------------------------------------- 7. unzip / partition
fn s07_unzip_partition() {
    let rows = [("a", 1), ("b", 2), ("c", 3)];
    let (names, nums): (Vec<&str>, Vec<i32>) = rows.iter().copied().unzip();
    assert_eq!(names, ["a", "b", "c"]);
    assert_eq!(nums, [1, 2, 3]);
    let (even, odd): (Vec<i32>, Vec<i32>) = (1..=10).partition(|n| n % 2 == 0);
    assert_eq!((even.len(), odd.len()), (5, 5));
    // unzip into different collection types
    let (set, vec): (BTreeSet<_>, Vec<_>) = [(3, 'x'), (1, 'y'), (3, 'z')].into_iter().unzip();
    assert_eq!(set.len(), 2);
    assert_eq!(vec, ['x', 'y', 'z']);
    println!("7. unzip/partition ok");
}

// ---------------------------------------------------------------- 8. comparing iterators
fn s08_compare() {
    use std::cmp::Ordering;
    assert!([1, 2, 3].iter().eq([1, 2, 3].iter()));
    assert!([1, 2].iter().ne([1, 2, 3].iter()));
    assert_eq!([1, 2, 3].iter().cmp([1, 2, 4].iter()), Ordering::Less);
    assert!([1, 2].iter().lt([1, 2, 0].iter())); // prefix is smaller
    assert!([2].iter().gt([1, 9, 9].iter())); // lexicographic, not by length
    assert!([1, 2].iter().le([1, 2].iter()) && [1, 2].iter().ge([1, 2].iter()));
    assert_eq!([1.0, f64::NAN].iter().partial_cmp([1.0, 2.0].iter()), None); // NaN poisons it
    // Compare across different iterator types: Vec vs a lazily-built range
    assert!(vec![1, 2, 3].into_iter().eq(1..=3));
    // sortedness checks (stable since 1.82)
    assert!([1, 2, 2, 5].iter().is_sorted());
    assert!(![3, 1].iter().is_sorted());
    assert!(["a", "bb", "ccc"].iter().is_sorted_by_key(|s| s.len()));
    assert!([5, 4, 4, 1].iter().is_sorted_by(|a, b| a >= b)); // descending
    println!("8. compare ok");
}

// ---------------------------------------------------------------- 9. sum/product of Option and Result
fn s09_sum_of_fallibles() {
    let good: Option<u32> = ["1", "2", "3"].iter().map(|s| s.parse::<u32>().ok()).sum();
    assert_eq!(good, Some(6));
    let bad: Option<u32> = ["1", "x"].iter().map(|s| s.parse::<u32>().ok()).sum();
    assert_eq!(bad, None);
    let r: Result<i32, std::num::ParseIntError> =
        ["4", "5"].iter().map(|s| s.parse::<i32>()).product();
    assert_eq!(r, Ok(20));
    let e: Result<i32, _> = ["4", "five"].iter().map(|s| s.parse::<i32>()).sum();
    assert!(e.is_err());
    println!("9. sum of Option/Result ok");
}

// ---------------------------------------------------------------- 10. DoubleEnded, ExactSize, size_hint
fn s10_ends_and_hints() {
    let mut it = [1, 2, 3, 4, 5].iter();
    assert_eq!(it.len(), 5); // ExactSizeIterator::len: O(1), unlike count()
    assert_eq!(it.next_back(), Some(&5));
    assert_eq!(it.nth_back(1), Some(&3));
    assert_eq!(it.len(), 2);
    assert_eq!((1..10).rev().step_by(3).collect::<Vec<_>>(), [9, 6, 3]);
    // `last()` on a DoubleEnded iterator still walks forward; `next_back()` is O(1)
    let mut r = 0..1_000_000_000u64;
    assert_eq!(r.next_back(), Some(999_999_999));
    // size_hint: (lower, Option<upper>). filter loses the lower bound; chain adds; flat_map is unbounded.
    assert_eq!((0..10).size_hint(), (10, Some(10)));
    assert_eq!((0..10).filter(|x| x % 2 == 0).size_hint(), (0, Some(10)));
    assert_eq!((0..10).chain(0..5).size_hint(), (15, Some(15)));
    assert_eq!((0..10).take(3).size_hint(), (3, Some(3)));
    assert_eq!(std::iter::repeat(1).size_hint(), (usize::MAX, None));
    assert_eq!((0..4).flat_map(|n| 0..n).size_hint().1, None);
    // Why it matters: collect uses the lower bound to pre-allocate.
    let v: Vec<u32> = (0..100).collect();
    assert!(v.capacity() >= 100);
    // by_ref: consume a prefix then keep using the same iterator
    let mut it = 1..10;
    let head: Vec<_> = it.by_ref().take(3).collect();
    assert_eq!(head, [1, 2, 3]);
    assert_eq!(it.next(), Some(4));
    println!("10. ends/size_hint ok");
}

// ---------------------------------------------------------------- 11. Less known
fn s11_less_known() {
    // Sum / Product are implemented for user types too:
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct Money(i64);
    impl std::iter::Sum for Money {
        fn sum<I: Iterator<Item = Money>>(iter: I) -> Money {
            Money(iter.map(|m| m.0).sum())
        }
    }
    impl<'a> std::iter::Sum<&'a Money> for Money {
        fn sum<I: Iterator<Item = &'a Money>>(iter: I) -> Money {
            iter.copied().sum()
        }
    }
    let wallet = [Money(5), Money(7)];
    assert_eq!(wallet.iter().sum::<Money>(), Money(12));
    // FromIterator for your own type: enables `.collect::<Bag>()`
    struct Bag(BTreeMap<char, usize>);
    impl FromIterator<char> for Bag {
        fn from_iter<I: IntoIterator<Item = char>>(i: I) -> Self {
            let mut m = BTreeMap::new();
            for c in i {
                *m.entry(c).or_insert(0) += 1;
            }
            Bag(m)
        }
    }
    let bag: Bag = "hello".chars().collect();
    assert_eq!(bag.0[&'l'], 2);
    // Iterator::cmp on floats via a mapped key: total_cmp as a key-free comparator
    let a = [1.0f64, 2.0];
    let b = [1.0f64, 3.0];
    assert_eq!(
        a.iter()
            .zip(&b)
            .map(|(x, y)| x.total_cmp(y))
            .find(|o| o.is_ne()),
        Some(std::cmp::Ordering::Less)
    );
    // `for_each` vs `for`: for_each can't break/return; fine for a final side effect in a chain
    let mut total = 0;
    (1..=4).for_each(|x| total += x);
    assert_eq!(total, 10);
    println!("11. less known ok");
}

// ---------------------------------------------------------------- 12. Production tips and tricks
fn s12_production_tips() {
    // 1. `collect::<Result<Vec<_>,_>>()` instead of a loop with push + `?` (clippy::manual_try_fold / manual_flatten style cleanups).
    let v: Result<Vec<i32>, _> = "1,2,3".split(',').map(str::parse::<i32>).collect();
    assert_eq!(v.unwrap(), [1, 2, 3]);

    // 2. `count()` after `filter` rather than collecting to a Vec just for `.len()` (clippy::needless_collect).
    let n = (0..100).filter(|x| x % 7 == 0).count();
    assert_eq!(n, 15);

    // 3. Annotate `sum::<T>()` explicitly: inference fails or silently picks i32 otherwise.
    let total: u64 = [u32::MAX, 1].iter().map(|&x| u64::from(x)).sum();
    assert_eq!(total, 4_294_967_296);

    // 4. Use `any`/`all` not `filter().count() > 0` (clippy::search_is_some, clippy::iter_count adjacent) - and it short-circuits.
    let mut visited = 0;
    let found = (0..1_000_000).any(|x| {
        visited += 1;
        x == 3
    });
    assert!(found && visited == 4);

    // 5. `find_map` instead of `.filter_map(..).next()` (clippy::filter_map_next).
    assert_eq!(
        ["a", "7", "9"].iter().find_map(|s| s.parse::<i32>().ok()),
        Some(7)
    );

    // 6. `min_by_key`/`max_by_key` with a borrowed key avoids sorting a whole Vec for one element (clippy::manual_min_max-like).
    let names = ["bob", "alexander", "eve"];
    assert_eq!(names.iter().max_by_key(|n| n.len()), Some(&"alexander"));

    // 7. `copied()` for Copy items instead of `.map(|x| *x)` (clippy::map_clone).
    let doubled: Vec<i32> = [1, 2].iter().copied().map(|x| x * 2).collect();
    assert_eq!(doubled, [2, 4]);

    // 8. Pre-size with `Vec::with_capacity` only when you do NOT collect; collect already uses size_hint.
    let mut out = Vec::with_capacity(8);
    out.extend((0..8).map(|x| x * x));
    assert_eq!(out.capacity(), 8);

    // 9. Float max: `iter().cloned().fold(f64::MIN, f64::max)` ignores NaN; `max_by(total_cmp)` ORDERS it. Pick deliberately.
    let xs = [1.0, f64::NAN, 3.0];
    assert_eq!(xs.iter().copied().fold(f64::MIN, f64::max), 3.0);
    assert!(
        xs.iter()
            .copied()
            .max_by(|a, b| a.total_cmp(b))
            .unwrap()
            .is_nan()
    ); // positive NaN sorts above everything

    // 10. Don't `collect` into Vec just to iterate again; return `impl Iterator` (clippy::needless_collect).
    fn evens(v: &[u32]) -> impl Iterator<Item = u32> + '_ {
        v.iter().copied().filter(|x| x % 2 == 0)
    }
    assert_eq!(evens(&[1, 2, 3, 4]).sum::<u32>(), 6);

    // 11. Last element of a slice: `.last()` on the SLICE is O(1); `.iter().last()` walks (clippy::iter_last... via `double_ended_iterator_last`).
    let big: Vec<u32> = (0..1000).collect();
    assert_eq!(big.last(), Some(&999));
    assert_eq!(big.iter().next_back(), Some(&999)); // O(1) when you must stay in iterator-land

    // 12. Deterministic output from hash containers: collect then sort, never print iteration order.
    let m: HashMap<_, _> = [("b", 2), ("a", 1)].into_iter().collect();
    let mut kv: Vec<_> = m.iter().collect();
    kv.sort();
    assert_eq!(kv, [(&"a", &1), (&"b", &2)]);

    // 13. `partition` over two `filter`s: single pass, no double evaluation of the predicate.
    let (a, b): (Vec<_>, Vec<_>) = (0..6).partition(|x| x % 3 == 0);
    assert_eq!((a, b), (vec![0, 3], vec![1, 2, 4, 5]));

    // 14. Differential test habit: check an optimised pipeline against the obvious loop on seeded random input.
    let mut rng = Rng(0x9E3779B97F4A7C15);
    for _ in 0..200 {
        let data: Vec<i64> = (0..(rng.next() % 40))
            .map(|_| (rng.next() % 100) as i64 - 50)
            .collect();
        let mut best = None;
        for (i, &x) in data.iter().enumerate() {
            if best.is_none_or(|(_, b)| x >= b) {
                best = Some((i, x)); // `>=` mimics max_by_key's "last max wins"
            }
        }
        let got = data
            .iter()
            .enumerate()
            .max_by_key(|(_, x)| **x)
            .map(|(i, x)| (i, *x));
        assert_eq!(got, best);
        assert_eq!(data.iter().sum::<i64>(), data.iter().fold(0, |a, x| a + x));
    }
    println!("12. production tips ok");
}

fn main() {
    let mut sorted = COVERED.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), COVERED.len(), "COVERED has duplicates");

    s01_everyday_core();
    s02_product_last_nth();
    s03_try_and_reduce();
    s04_extrema_by();
    s05_search();
    s06_collect_targets();
    s07_unzip_partition();
    s08_compare();
    s09_sum_of_fallibles();
    s10_ends_and_hints();
    s11_less_known();
    s12_production_tips();
    println!(
        "all consumer sections passed ({} methods listed)",
        COVERED.len()
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn covered_has_no_duplicates() {
        let mut v = super::COVERED.to_vec();
        v.sort();
        let n = v.len();
        v.dedup();
        assert_eq!(n, v.len());
    }
}
