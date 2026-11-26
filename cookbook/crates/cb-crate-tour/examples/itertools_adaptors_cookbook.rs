//! RECIPE 1105 (crates): itertools: grouping, merging, windows, combinatorics and folding, each checked against a std-only version
//! LEVEL: intermediate
//! Use in: report building, log processing, test-case generation, any pipeline where hand-written loops hide the intent.   Run: cargo run --release -p cb-crate-tour --example itertools_adaptors_cookbook
//! COMMON USES: run-length grouping of sorted data, merging sorted shards, exhaustive combinations in tests, frequency maps.
//! METHODS: Itertools::chunk_by, Itertools::chunks, Itertools::sorted_by, Itertools::sorted_by_key, Itertools::sorted_by_cached_key, Itertools::sorted_unstable_by, Itertools::k_largest_by_key, Itertools::tail, Itertools::sorted_unstable, Itertools::unique, Itertools::unique_by, Itertools::dedup, Itertools::dedup_with_count, Itertools::duplicates, Itertools::tuple_windows, Itertools::tuples, Itertools::circular_tuple_windows, Itertools::interleave, Itertools::intersperse, Itertools::kmerge, Itertools::merge, Itertools::merge_join_by, itertools::EitherOrBoth, itertools::iproduct, Itertools::cartesian_product, Itertools::multi_cartesian_product, Itertools::permutations, Itertools::combinations, Itertools::combinations_with_replacement, Itertools::powerset, Itertools::zip_longest, itertools::izip, itertools::zip_eq, Itertools::fold_while, Itertools::fold_ok, Itertools::process_results, Itertools::try_collect, Itertools::minmax, Itertools::position_max, Itertools::min_set, Itertools::k_smallest, Itertools::counts, Itertools::counts_by, Itertools::into_group_map, Itertools::into_grouping_map_by, Itertools::join, Itertools::format_with, Itertools::sum1, Itertools::positions, Itertools::find_position, Itertools::exactly_one, Itertools::at_most_one, Itertools::with_position, Itertools::batching, Itertools::tee, Itertools::peekable, Itertools::peeking_take_while, Itertools::take_while_ref, Itertools::collect_vec, Itertools::collect_tuple, Itertools::partition_map, Itertools::all_equal, Itertools::pad_using, Itertools::while_some, Itertools::merge_by, Itertools::kmerge_by, Itertools::coalesce, Itertools::map_ok, Itertools::filter_ok, Itertools::flatten_ok, Itertools::partition_result, Itertools::tree_reduce, Itertools::minmax_by_key, Itertools::max_set_by_key, Itertools::all_unique, Itertools::array_combinations, Itertools::collect_array, Itertools::next_tuple, Itertools::map_into, Itertools::update, Itertools::multipeek, Itertools::take_while_inclusive, GroupingMap::{sum, max_by_key, min, collect, aggregate}
//! DOCS: https://docs.rs/itertools/0.14/itertools/trait.Itertools.html
//! CRATES: itertools = "0.14"
//!
//! CHEAT SHEET
//!   group ADJACENT equal keys        -> chunk_by (sort first to group globally)
//!   group globally into a map        -> into_group_map / into_grouping_map_by (aggregate without building Vecs)
//!   count occurrences                -> counts / counts_by
//!   merge sorted inputs              -> merge (2), kmerge (k), merge_join_by (full outer join)
//!   sort in the middle of a chain    -> sorted_by / sorted_by_key (stable) ; sorted_by_cached_key if the key is costly
//!   best k of many                   -> k_smallest / k_largest_by_key (O(n log k), O(k) memory) instead of sorted().take(k)
//!   neighbours                       -> tuple_windows (tuple items must be Clone)
//!   all pairs / subsets / orderings  -> combinations, powerset, permutations, cartesian_product
//!   Results inside an iterator       -> try_collect, process_results, fold_ok
//!   zip of unequal lengths           -> zip_longest ; "must be equal" -> zip_eq (panics)
//!
//! PITFALLS: `chunk_by` is lazy and NOT an Iterator by itself (iterate `&grouping`); `unique`, `counts` and `sorted` buffer or
//! hash everything (memory is O(distinct) or O(n)); `sorted` and `permutations` on an infinite iterator never return;
//! method names can collide with newer std methods (e.g. `intersperse`), so call `Itertools::intersperse(it, x)` if warned.
//!
//! Read: https://docs.rs/itertools/0.14/itertools/ ; https://github.com/rust-itertools/itertools
//!
//! TRY NEXT:
//!   - [S] Replace `chunk_by` with `sorted_by_key().chunk_by()` in the sessions demo and predict how the output changes.
//!   - [M] Write `merge_join_by` yourself with two `Peekable`s, then fuzz it against the library version.
//!   - [M] Use `powerset` to brute-force a small knapsack and use it as the oracle for a DP solution.
//!   - [A] PROJECT: with `tempfile_walkdir_glob_ignore`, walk a tree and report size per extension with `into_grouping_map_by`.
#![allow(dead_code)]
use itertools::{Either, EitherOrBoth, FoldWhile, Itertools, Position, iproduct, izip};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::iter::Peekable;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn vec(&mut self, max_len: u64, domain: u64) -> Vec<u32> {
        let n = self.next() % (max_len + 1);
        (0..n).map(|_| (self.next() % domain) as u32).collect()
    }
}

/// Scenario: split a time-ordered click log into (user, session length) runs.
fn grouping() {
    let log = [("ann", 1), ("ann", 2), ("bob", 3), ("ann", 9), ("ann", 10)];
    let mut runs = vec![];
    for (user, group) in &log.iter().chunk_by(|e| e.0) {
        runs.push((user, group.count()));
    }
    assert_eq!(
        runs,
        [("ann", 2), ("bob", 1), ("ann", 2)],
        "ADJACENT grouping: ann appears twice"
    );
    // std equivalent for slices
    let std_runs: Vec<(&str, usize)> = log
        .chunk_by(|a, b| a.0 == b.0)
        .map(|c| (c[0].0, c.len()))
        .collect();
    assert_eq!(std_runs, runs);

    // Global grouping, order of values inside a group preserved.
    let by_user: HashMap<&str, Vec<i32>> = log.iter().copied().into_group_map();
    assert_eq!(by_user["ann"], [1, 2, 9, 10]);
    let mut manual: HashMap<&str, Vec<i32>> = HashMap::new();
    for (k, v) in log {
        manual.entry(k).or_default().push(v);
    }
    assert_eq!(manual, by_user);
    // Aggregate per key without materialising the groups.
    let last_seen = log
        .iter()
        .into_grouping_map_by(|e| e.0)
        .fold(0, |acc, _, e| acc.max(e.1));
    assert_eq!((last_seen["ann"], last_seen["bob"]), (10, 3));

    // Fixed-size chunks of a NON-slice iterator (slices have .chunks()).
    let sums: Vec<u32> = (1..=7u32).chunks(3).into_iter().map(|c| c.sum()).collect();
    assert_eq!(sums, [6, 15, 7]);
}

fn dedup_unique_count_extremes() {
    let mut r = Rng(0x1105);
    for _ in 0..500 {
        let v = r.vec(30, 8);
        // unique: first occurrence order, global
        let mut seen = HashSet::new();
        assert_eq!(
            v.iter().copied().unique().collect_vec(),
            v.iter()
                .copied()
                .filter(|x| seen.insert(*x))
                .collect::<Vec<_>>()
        );
        // dedup: adjacent only, like Vec::dedup
        let mut d = v.clone();
        d.dedup();
        assert_eq!(v.iter().copied().dedup().collect_vec(), d);
        assert_eq!(
            v.iter().dedup_with_count().map(|(n, _)| n).sum::<usize>(),
            v.len()
        );
        // counts
        let mut freq = HashMap::new();
        for &x in &v {
            *freq.entry(x).or_insert(0usize) += 1;
        }
        assert_eq!(v.iter().copied().counts(), freq);
        assert_eq!(
            v.iter()
                .copied()
                .duplicates()
                .sorted_unstable()
                .collect_vec(),
            freq.iter()
                .filter(|(_, n)| **n > 1)
                .map(|(k, _)| *k)
                .sorted()
                .collect_vec()
        );
        // sorted + k smallest
        let mut s = v.clone();
        s.sort_unstable();
        assert_eq!(v.iter().copied().sorted_unstable().collect_vec(), s);
        assert_eq!(
            v.iter().copied().k_smallest(3).collect_vec(),
            s.iter().copied().take(3).collect_vec()
        );
        // minmax in one pass
        assert_eq!(
            v.iter().copied().minmax().into_option(),
            v.iter().copied().min().zip(v.iter().copied().max())
        );
        assert_eq!(
            v.iter().min_set().len(),
            v.iter()
                .filter(|x| Some(**x) == v.iter().copied().min())
                .count()
        );
    }
    assert_eq!(
        ["bb", "a", "cc"]
            .into_iter()
            .unique_by(|s| s.len())
            .collect_vec(),
        ["bb", "a"]
    );
    assert_eq!(
        ["bb", "a", "cc"].iter().counts_by(|s| s.len()),
        HashMap::from([(2, 2), (1, 1)])
    );
    // std max_by_key returns the LAST maximum; so does position_max.
    assert_eq!([3, 9, 1, 9].iter().position_max(), Some(3));
    assert_eq!(
        [(2, 'b'), (1, 'z'), (2, 'a')]
            .into_iter()
            .sorted_by_key(|p| p.0)
            .collect_vec(),
        [(1, 'z'), (2, 'b'), (2, 'a')],
        "sorted_by_key is stable"
    );
}

fn windows_and_weaving() {
    let temps = [10, 12, 11, 15];
    let deltas = temps
        .iter()
        .tuple_windows()
        .map(|(a, b)| b - a)
        .collect_vec();
    assert_eq!(deltas, temps.windows(2).map(|w| w[1] - w[0]).collect_vec());
    // tuples(): non-overlapping; an incomplete tail is dropped but retrievable.
    let mut pairs = (1..=5).tuples::<(_, _)>();
    assert_eq!(pairs.by_ref().collect_vec(), [(1, 2), (3, 4)]);
    assert_eq!(pairs.into_buffer().collect_vec(), [5]);
    // Closed polygon edges.
    assert_eq!(
        ['a', 'b', 'c']
            .into_iter()
            .circular_tuple_windows::<(_, _)>()
            .collect_vec(),
        [('a', 'b'), ('b', 'c'), ('c', 'a')]
    );
    assert_eq!(
        [1, 2, 3].into_iter().interleave([10]).collect_vec(),
        [1, 10, 2, 3]
    );
    assert_eq!(
        Itertools::intersperse(["a", "b", "c"].into_iter(), "/").collect::<String>(),
        "a/b/c"
    );
    assert_eq!(
        [1, 2]
            .into_iter()
            .pad_using(4, |i| i as i32 * 100)
            .collect_vec(),
        [1, 2, 200, 300]
    );
    assert_eq!([1, 2, 3].iter().join(", "), "1, 2, 3");
    // format_with writes straight into the output, with no intermediate Strings.
    let shown = format!(
        "{}",
        [("a", 1), ("b", 2)]
            .iter()
            .format_with("&", |(k, v), f| f(&format_args!("{k}={v}")))
    );
    assert_eq!(shown, "a=1&b=2");
}

/// Scenario: merge sorted shards, and reconcile two sorted inventories (a full outer join).
fn merging() {
    let mut r = Rng(0xAE46E);
    for _ in 0..500 {
        let k = (r.next() % 5) as usize;
        let shards: Vec<Vec<u32>> = (0..k)
            .map(|_| r.vec(10, 50).into_iter().sorted().collect())
            .collect();
        let mut flat = shards.concat();
        flat.sort_unstable();
        assert_eq!(
            shards
                .iter()
                .map(|s| s.iter().copied())
                .kmerge()
                .collect_vec(),
            flat
        );

        let a = r.vec(12, 20).into_iter().sorted().dedup().collect_vec();
        let b = r.vec(12, 20).into_iter().sorted().dedup().collect_vec();
        assert_eq!(
            a.iter().merge(&b).count(),
            a.len() + b.len(),
            "merge keeps duplicates"
        );
        let (mut only_a, mut both, mut only_b) =
            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for item in a.iter().merge_join_by(b.iter(), |x, y| x.cmp(y)) {
            match item {
                EitherOrBoth::Left(x) => only_a.insert(*x),
                EitherOrBoth::Both(x, _) => both.insert(*x),
                EitherOrBoth::Right(y) => only_b.insert(*y),
            };
        }
        let (sa, sb): (BTreeSet<u32>, BTreeSet<u32>) =
            (a.iter().copied().collect(), b.iter().copied().collect());
        assert_eq!(only_a, &sa - &sb);
        assert_eq!(only_b, &sb - &sa);
        assert_eq!(both, &sa & &sb);
    }
    // PITFALL: on UNSORTED input merge_join_by silently gives a wrong join.
    let bad = [3, 1]
        .into_iter()
        .merge_join_by([1], |x: &i32, y: &i32| x.cmp(y))
        .count();
    assert_eq!(bad, 3, "the matching 1s were never paired");
}

fn zipping() {
    let z = [1, 2, 3].iter().zip_longest(["a"].iter()).map(|p| match p {
        EitherOrBoth::Both(n, s) => format!("{n}{s}"),
        EitherOrBoth::Left(n) => format!("{n}_"),
        EitherOrBoth::Right(s) => format!("_{s}"),
    });
    assert_eq!(z.collect_vec(), ["1a", "2_", "3_"]);
    // izip!: flat tuples instead of ((a, b), c)
    assert_eq!(
        izip!([1, 2], [3, 4], [5, 6]).collect_vec(),
        [(1, 3, 5), (2, 4, 6)]
    );
    // zip_eq panics when lengths differ; std zip silently truncates.
    assert_eq!([1, 2].iter().zip([1].iter()).count(), 1);
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let caught = std::panic::catch_unwind(|| itertools::zip_eq([1, 2], [1]).count());
    std::panic::set_hook(prev);
    assert!(caught.is_err());
}

fn binomial(n: usize, k: usize) -> usize {
    if k > n {
        0
    } else {
        (0..k).fold(1, |acc, i| acc * (n - i) / (i + 1))
    }
}
fn combinatorics() {
    for n in 0..=7usize {
        for k in 0..=7usize {
            let c = (0..n).combinations(k).collect_vec();
            assert_eq!(c.len(), binomial(n, k));
            assert!(c.is_sorted(), "lexicographic order of POSITIONS");
            assert_eq!(
                (0..n).combinations_with_replacement(k).count(),
                if n == 0 {
                    (k == 0) as usize
                } else {
                    binomial(n + k - 1, k)
                }
            );
            assert_eq!(
                (0..n).permutations(k).count(),
                if k > n { 0 } else { (n - k + 1..=n).product() }
            );
        }
        // powerset == every bitmask
        let ps: BTreeSet<Vec<usize>> = (0..n).powerset().collect();
        let masks: BTreeSet<Vec<usize>> = (0u32..1 << n)
            .map(|m| (0..n).filter(|i| m >> i & 1 == 1).collect())
            .collect();
        assert_eq!(ps, masks);
    }
    // combinations choose by position, not by value
    assert_eq!([7, 7].into_iter().combinations(1).collect_vec(), [[7], [7]]);
    let nested = (0..3)
        .flat_map(|x| (0..2).map(move |y| (x, y)))
        .collect_vec();
    assert_eq!((0..3).cartesian_product(0..2).collect_vec(), nested);
    assert_eq!(iproduct!(0..3, 0..2).collect_vec(), nested);
    // Scenario: a test matrix whose NUMBER of dimensions is only known at run time.
    let dims = vec![
        vec!["linux", "mac"],
        vec!["debug", "release"],
        vec!["a", "b", "c"],
    ];
    let matrix = dims.into_iter().multi_cartesian_product().collect_vec();
    assert_eq!(matrix.len(), 12);
    assert_eq!(
        matrix[1],
        ["linux", "debug", "b"],
        "the LAST dimension varies fastest"
    );
}

fn folding_and_results() {
    // fold_while: stop a fold early, e.g. fill a budget.
    let taken = [4, 3, 5, 1].iter().fold_while(0, |used, &c| {
        if used + c <= 8 {
            FoldWhile::Continue(used + c)
        } else {
            FoldWhile::Done(used)
        }
    });
    assert_eq!(taken.into_inner(), 7);
    // std equivalent: try_fold with ControlFlow
    use std::ops::ControlFlow;
    let flow = [4, 3, 5, 1].iter().try_fold(0, |u, &c| {
        if u + c <= 8 {
            ControlFlow::Continue(u + c)
        } else {
            ControlFlow::Break(u)
        }
    });
    assert_eq!(flow, ControlFlow::Break(7));

    let parse = |xs: &'static [&'static str]| xs.iter().map(|s| s.parse::<i32>());
    let ok: Result<Vec<i32>, _> = parse(&["1", "2"]).try_collect();
    assert_eq!(ok, Ok(vec![1, 2]));
    // process_results: run ordinary iterator code over the Ok values; the first Err aborts and is returned.
    let max = itertools::process_results(parse(&["1", "9", "2"]), |it| it.max());
    assert_eq!(max, Ok(Some(9)));
    assert!(itertools::process_results(parse(&["1", "x"]), |it| it.max()).is_err());
    assert_eq!(parse(&["1", "2", "3"]).fold_ok(0, |a, b| a + b), Ok(6));
    // sum1/product1 distinguish "empty" from "sums to zero"
    assert_eq!(std::iter::empty::<i32>().sum1::<i32>(), None);
    assert_eq!([1, -1].into_iter().sum1::<i32>(), Some(0));
    // partition_map: split AND transform (std partition cannot change types)
    let (nums, words): (Vec<i32>, Vec<&str>) =
        ["1", "x", "2"]
            .into_iter()
            .partition_map(|s| match s.parse::<i32>() {
                Ok(n) => Either::Left(n),
                Err(_) => Either::Right(s),
            });
    assert_eq!((nums, words), (vec![1, 2], vec!["x"]));
    assert_eq!(
        [Some(1), Some(2), None, Some(4)]
            .into_iter()
            .while_some()
            .collect_vec(),
        [1, 2]
    );
}

fn searching_and_position() {
    let v = [5, 8, 5, 2];
    assert_eq!(v.iter().positions(|&x| x == 5).collect_vec(), [0, 2]);
    assert_eq!(v.iter().find_position(|&&x| x == 2), Some((3, &2)));
    // exactly_one: "there must be a single match" as a checked operation
    assert_eq!(v.iter().filter(|&&x| x == 8).exactly_one().ok(), Some(&8));
    let err = v
        .iter()
        .filter(|&&x| x == 5)
        .exactly_one()
        .map_err(|rest| rest.count());
    assert_eq!(err, Err(2), "the error hands back all the items");
    assert_eq!(std::iter::empty::<u8>().at_most_one().ok(), Some(None));
    assert!([1, 1].iter().all_equal() && std::iter::empty::<u8>().all_equal());
    // with_position: treat the last element differently without peeking by hand
    let sentence = ["a", "b", "c"]
        .iter()
        .with_position()
        .map(|(p, s)| match p {
            Position::First | Position::Only => s.to_string(),
            Position::Middle => format!(", {s}"),
            Position::Last => format!(" and {s}"),
        })
        .collect::<String>();
    assert_eq!(sentence, "a, b and c");
    let (a, b, c) = "x y z".split(' ').collect_tuple().unwrap();
    assert_eq!((a, b, c), ("x", "y", "z"));
    assert_eq!(
        "x y".split(' ').collect_tuple::<(_, _, _)>(),
        None,
        "arity mismatch is None"
    );
}

/// Consume a prefix without losing the first item that does NOT match. std's take_while swallows it.
fn read_number(it: &mut Peekable<std::str::Chars>) -> u32 {
    it.peeking_take_while(char::is_ascii_digit)
        .fold(0, |n, d| n * 10 + d.to_digit(10).unwrap())
}
fn stateful_consumption() {
    let mut it = "12+34".chars().peekable();
    assert_eq!(read_number(&mut it), 12);
    assert_eq!(it.next(), Some('+'), "the delimiter is still there");
    assert_eq!(read_number(&mut it), 34);
    let mut std_it = "12+".chars();
    let digits: String = std_it.by_ref().take_while(char::is_ascii_digit).collect();
    assert_eq!(
        (digits.as_str(), std_it.next()),
        ("12", None),
        "std take_while ate the '+'"
    );
    let mut cl = "ab1".chars();
    assert_eq!(
        cl.take_while_ref(|c| c.is_alphabetic()).collect::<String>(),
        "ab"
    );
    assert_eq!(cl.next(), Some('1'));

    // batching: each call pulls as many items as it needs (length-prefixed records).
    let wire = [2u32, 7, 8, 0, 1, 9];
    let records = wire.iter().copied().batching(|it| {
        let n = it.next()? as usize;
        Some(it.take(n).collect_vec())
    });
    assert_eq!(records.collect_vec(), [vec![7, 8], vec![], vec![9]]);
    // tee: two independent readers of one single-pass iterator (buffers the gap between them).
    let (a, b) = (1..=4).map(|x| x * x).tee();
    assert_eq!(
        a.zip(b.skip(1)).map(|(x, y)| y - x).collect_vec(),
        [3, 5, 7]
    );
    let sizes: BTreeMap<usize, usize> = ["a", "bb", "cc"]
        .iter()
        .map(|s| s.len())
        .counts()
        .into_iter()
        .collect();
    assert_eq!(sizes, BTreeMap::from([(1, 1), (2, 2)]));
}

/// sorted_by and its relatives: `slice::sort_by` as an adaptor, so a chain does not need a temporary `let mut`.
fn sorting_adaptors() {
    let mut r = Rng(0x50F7);
    for _ in 0..500 {
        // (key, original position): the position makes stability observable.
        let items: Vec<(u32, usize)> = r
            .vec(30, 6)
            .into_iter()
            .enumerate()
            .map(|(i, k)| (k, i))
            .collect();
        // Descending by key. Equal keys keep their input order, exactly like Vec::sort_by.
        let got = items
            .iter()
            .copied()
            .sorted_by(|a, b| b.0.cmp(&a.0))
            .collect_vec();
        let mut want = items.clone();
        want.sort_by(|a, b| b.0.cmp(&a.0));
        assert_eq!(got, want);
        assert!(
            got.windows(2)
                .all(|w| w[0].0 > w[1].0 || (w[0].0 == w[1].0 && w[0].1 < w[1].1)),
            "stable"
        );
        // PITFALL: "sort ascending then reverse" is NOT the same: it also reverses the ties.
        let reversed = items
            .iter()
            .copied()
            .sorted_by_key(|p| p.0)
            .rev()
            .collect_vec();
        assert!(
            reversed
                .windows(2)
                .all(|w| w[0].0 > w[1].0 || w[0].1 > w[1].1)
        );
        // The unstable version promises order by the comparator only, so give it a TOTAL order.
        let total = items
            .iter()
            .copied()
            .sorted_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)))
            .collect_vec();
        assert_eq!(total, want);
        // k_largest_by_key: the k biggest, largest first, without sorting everything.
        let top = items
            .iter()
            .copied()
            .k_largest_by_key(3, |p| (p.0, p.1))
            .collect_vec();
        assert_eq!(
            top,
            items
                .iter()
                .copied()
                .sorted_by(|a, b| b.cmp(a))
                .take(3)
                .collect_vec()
        );
        // tail: the last k items of an iterator that cannot be sliced.
        assert_eq!(
            items.iter().copied().tail(4).collect_vec(),
            items[items.len().saturating_sub(4)..]
        );
    }
    // Multi-key order with Ordering::then_with, the common real use of sorted_by.
    let scores = [("bo", 7), ("al", 9), ("cy", 7)];
    let board = scores
        .iter()
        .sorted_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)))
        .map(|p| p.0)
        .join(" ");
    assert_eq!(board, "al bo cy");
    // sorted_by_key calls the key function on every comparison; sorted_by_cached_key calls it once per item.
    let words = [
        "delta", "Alpha", "charlie", "Bravo", "echo", "foxtrot", "golf", "hotel",
    ];
    let (mut plain, mut cached) = (0, 0);
    let a = words
        .iter()
        .sorted_by_key(|w| {
            plain += 1;
            w.to_lowercase()
        })
        .collect_vec();
    let b = words
        .iter()
        .sorted_by_cached_key(|w| {
            cached += 1;
            w.to_lowercase()
        })
        .collect_vec();
    assert_eq!(a, b);
    assert_eq!(cached, words.len());
    assert!(plain > cached, "{plain} key computations");
    // sorted_by cannot borrow from the item in the KEY (K may not borrow), but a comparator can: no clone needed.
    let owned = vec![String::from("b"), String::from("a")];
    assert_eq!(
        owned
            .iter()
            .sorted_by(|x, y| x.as_str().cmp(y.as_str()))
            .collect_vec(),
        ["a", "b"]
    );
}

/// The std-only twin of each remaining adaptor from the cheat sheet, compared on random inputs.
fn std_equivalents() {
    fn permutations_std(pool: &[u32], k: usize) -> Vec<Vec<u32>> {
        if k == 0 {
            return vec![vec![]];
        }
        (0..pool.len())
            .flat_map(|i| {
                let mut rest = pool.to_vec();
                let head = rest.remove(i);
                permutations_std(&rest, k - 1)
                    .into_iter()
                    .map(move |mut tail| {
                        tail.insert(0, head);
                        tail
                    })
            })
            .collect()
    }
    let mut r = Rng(0x57D);
    for _ in 0..300 {
        let a = r.vec(8, 10);
        let b = r.vec(8, 10);

        // interleave: alternate, then whatever is left of the longer one.
        let mut want: Vec<&u32> = vec![];
        for i in 0..a.len().max(b.len()) {
            want.extend(a.get(i));
            want.extend(b.get(i));
        }
        assert_eq!(a.iter().interleave(&b).collect_vec(), want);

        // zip_longest
        let want = (0..a.len().max(b.len()))
            .map(|i| (a.get(i), b.get(i)))
            .collect_vec();
        let got = a
            .iter()
            .zip_longest(&b)
            .map(|e| (e.as_ref().left().copied(), e.as_ref().right().copied()))
            .collect_vec();
        assert_eq!(got, want);

        // join: std joins only slices of strings, so a generic iterator needs the intermediate Vec<String>.
        assert_eq!(
            a.iter().join("-"),
            a.iter().map(u32::to_string).collect::<Vec<_>>().join("-")
        );
        // positions and sum1
        assert_eq!(
            a.iter().positions(|&x| x % 3 == 0).collect_vec(),
            a.iter()
                .enumerate()
                .filter_map(|(i, &x)| (x % 3 == 0).then_some(i))
                .collect_vec()
        );
        assert_eq!(
            a.iter().copied().sum1::<u32>(),
            a.iter().copied().reduce(|x, y| x + y)
        );
        // with_position: std needs a Peekable and a "first" flag.
        let mut it = a.iter().peekable();
        let mut first = true;
        let mut want = vec![];
        while let Some(x) = it.next() {
            let pos = match (first, it.peek().is_none()) {
                (true, true) => Position::Only,
                (true, false) => Position::First,
                (false, true) => Position::Last,
                (false, false) => Position::Middle,
            };
            first = false;
            want.push((pos, x));
        }
        assert_eq!(a.iter().with_position().collect_vec(), want);
        // partition_map
        let split = |x: u32| {
            if x.is_multiple_of(2) {
                Either::Left(x / 2)
            } else {
                Either::Right(x.to_string())
            }
        };
        let (mut left, mut right) = (vec![], vec![]);
        for &x in &a {
            match split(x) {
                Either::Left(l) => left.push(l),
                Either::Right(s) => right.push(s),
            }
        }
        assert_eq!(
            a.iter()
                .copied()
                .partition_map::<Vec<u32>, Vec<String>, _, _, _>(split),
            (left, right)
        );
        // process_results versus "collect first": same answer, but std buffers every Ok value.
        let raw = a.iter().map(|&x| if x == 9 { Err(x) } else { Ok(x) });
        assert_eq!(
            itertools::process_results(raw.clone(), |ok| ok.max()),
            raw.collect::<Result<Vec<_>, _>>()
                .map(|v| v.into_iter().max())
        );
        // peeking_take_while versus std's Peekable::next_if.
        let mut x = a.iter().copied().peekable();
        let mut y = a.iter().copied().peekable();
        let got = x.peeking_take_while(|&v| v < 7).collect_vec();
        let want: Vec<u32> = std::iter::from_fn(|| y.next_if(|&v| v < 7)).collect();
        assert_eq!((got, x.collect_vec()), (want, y.collect_vec()));
        // batching (here: pairs, keeping a short tail) versus an explicit from_fn loop.
        let mut src = a.iter().copied();
        let want: Vec<(u32, Option<u32>)> =
            std::iter::from_fn(|| src.next().map(|h| (h, src.next()))).collect();
        assert_eq!(
            a.iter()
                .copied()
                .batching(|it| it.next().map(|h| (h, it.next())))
                .collect_vec(),
            want
        );
        // tee versus materialising: same items; tee only buffers the distance between the two readers.
        let (p, q) = a.iter().tee();
        assert_eq!(p.collect_vec(), q.collect_vec());

        // permutations: same sequences in the same order as the recursive definition.
        let pool = &a[..a.len().min(5)];
        for k in 0..=pool.len() {
            assert_eq!(
                pool.iter().copied().permutations(k).collect_vec(),
                permutations_std(pool, k)
            );
            // combinations: every bitmask with exactly k bits.
            let by_mask: BTreeSet<Vec<usize>> = (0u32..1 << pool.len())
                .filter(|m| m.count_ones() as usize == k)
                .map(|m| (0..pool.len()).filter(|i| m >> i & 1 == 1).collect())
                .collect();
            assert_eq!(
                (0..pool.len()).combinations(k).collect::<BTreeSet<_>>(),
                by_mask
            );
        }
        // multi_cartesian_product: std needs a fold that multiplies out one dimension at a time.
        let dims: Vec<Vec<u32>> = (0..1 + r.next() % 3).map(|_| r.vec(3, 10)).collect();
        let want = dims.iter().fold(vec![vec![]], |acc: Vec<Vec<u32>>, dim| {
            acc.iter()
                .flat_map(|prefix| {
                    dim.iter()
                        .map(move |&x| prefix.iter().copied().chain([x]).collect())
                })
                .collect()
        });
        assert_eq!(
            dims.iter()
                .map(|d| d.iter().copied())
                .multi_cartesian_product()
                .collect_vec(),
            want
        );
    }
}

fn aggregation_merging_and_results() {
    // into_grouping_map: per-key aggregates in one pass, without Vec-per-key.
    let sales = [("eu", 5), ("us", 7), ("eu", 11), ("us", 2), ("ap", 1)];
    let by = || sales.iter().copied().into_grouping_map();
    assert_eq!(
        by().sum(),
        HashMap::from([("eu", 16), ("us", 9), ("ap", 1)])
    );
    assert_eq!(by().min()["us"], 2);
    assert_eq!(by().collect::<BTreeSet<_>>()["eu"], BTreeSet::from([5, 11]));
    assert_eq!(
        sales
            .iter()
            .into_grouping_map_by(|s| s.0)
            .max_by_key(|_, s| s.1)["eu"],
        &("eu", 11)
    );
    // aggregate: returning None DROPS the key, so it can filter while it folds.
    let big = by().aggregate(|acc, _, v| {
        let total = acc.unwrap_or(0) + v;
        (total > 3).then_some(total)
    });
    assert_eq!(big, HashMap::from([("eu", 16), ("us", 9)]));
    // PITFALL of that: once dropped, the key restarts from None on its next item.
    let restarted = [("k", 1), ("k", 10)]
        .into_iter()
        .into_grouping_map()
        .aggregate(|acc: Option<i32>, _, v| (v > 3).then(|| acc.unwrap_or(0) + v));
    assert_eq!(restarted, HashMap::from([("k", 10)]));

    // Ties: minmax returns the FIRST minimum and the LAST maximum (matching std's min_by_key and max_by_key).
    let ties = [(1, 'a'), (3, 'b'), (1, 'c'), (3, 'd')];
    assert_eq!(
        ties.iter().minmax_by_key(|p| p.0).into_option(),
        Some((&(1, 'a'), &(3, 'd')))
    );
    assert_eq!(
        ties.iter()
            .min_by_key(|p| p.0)
            .zip(ties.iter().max_by_key(|p| p.0)),
        Some((&(1, 'a'), &(3, 'd')))
    );
    assert_eq!(ties.iter().max_set_by_key(|p| p.0), [&(3, 'b'), &(3, 'd')]);

    // Merging by a key: streams ordered newest-first need a comparator ("does a go first?").
    let a = [(9, "a9"), (4, "a4")];
    let b = [(7, "b7"), (4, "b4")];
    let merged = a
        .iter()
        .merge_by(&b, |x, y| x.0 >= y.0)
        .map(|e| e.1)
        .collect_vec();
    assert_eq!(
        merged,
        ["a9", "b7", "a4", "b4"],
        "`>=` keeps the left stream first on ties"
    );
    let shards = [vec![9, 3], vec![8, 1], vec![5]];
    assert_eq!(
        shards
            .iter()
            .map(|s| s.iter().copied())
            .kmerge_by(|x, y| x > y)
            .collect_vec(),
        [9, 8, 5, 3, 1]
    );

    // coalesce: merge ADJACENT items when you can. Here: overlapping intervals sorted by start.
    let spans = [(1, 4), (6, 7), (2, 5), (7, 9), (12, 13)];
    let joined = spans
        .into_iter()
        .sorted()
        .coalesce(|a, b| {
            if b.0 <= a.1 {
                Ok((a.0, a.1.max(b.1)))
            } else {
                Err((a, b))
            }
        })
        .collect_vec();
    assert_eq!(joined, [(1, 5), (6, 9), (12, 13)]);

    // Adapting the Ok side of a stream of Results; errors pass through untouched and in place.
    let input: [Result<i32, &str>; 4] = [Ok(1), Err("x"), Ok(2), Ok(3)];
    assert_eq!(
        input
            .into_iter()
            .map_ok(|v| v * 10)
            .filter_ok(|v| *v > 10)
            .collect_vec(),
        [Err("x"), Ok(20), Ok(30)]
    );
    let nested: [Result<Vec<u8>, &str>; 2] = [Ok(vec![1, 2]), Err("y")];
    assert_eq!(
        nested.into_iter().flatten_ok().collect_vec(),
        [Ok(1), Ok(2), Err("y")]
    );
    // partition_result keeps going and returns ALL errors, unlike try_collect.
    let (oks, errs): (Vec<i32>, Vec<&str>) = input.into_iter().partition_result();
    assert_eq!((oks, errs), (vec![1, 2, 3], vec!["x"]));

    // tree_reduce combines neighbours pairwise (balanced); reduce is a left-leaning chain.
    let show = |a: String, b: String| format!("({a}{b})");
    let letters = || ["a", "b", "c", "d"].into_iter().map(String::from);
    assert_eq!(letters().reduce(show).unwrap(), "(((ab)c)d)");
    assert_eq!(letters().tree_reduce(show).unwrap(), "((ab)(cd))");

    // Fixed-size results as arrays or tuples.
    assert_eq!("1 2 3".split(' ').collect_array(), Some(["1", "2", "3"]));
    assert_eq!("1 2".split(' ').collect_array::<3>(), None);
    let mut fields = "GET /x HTTP/1.1 extra".split(' ');
    let (method, path): (&str, &str) = fields.next_tuple().unwrap(); // takes a prefix, leaves the rest
    assert_eq!(
        (method, path, fields.next()),
        ("GET", "/x", Some("HTTP/1.1"))
    );
    assert_eq!(
        [1, 2, 3].into_iter().array_combinations().collect_vec(),
        [[1, 2], [1, 3], [2, 3]]
    );
    assert!([1, 2, 3].iter().all_unique() && ![1, 2, 1].iter().all_unique());
    assert_eq!(
        [1u8, 2]
            .into_iter()
            .map_into::<u32>()
            .update(|x| *x *= 100)
            .collect_vec(),
        [100u32, 200]
    );
    // multipeek: look several items ahead without consuming; next() resets the peek cursor.
    let mut mp = itertools::multipeek(1..=3);
    assert_eq!((mp.peek().copied(), mp.peek().copied()), (Some(1), Some(2)));
    assert_eq!(mp.next(), Some(1));
    assert_eq!(mp.peek().copied(), Some(2));
    // take_while_inclusive keeps the item that ended the run (e.g. "read up to and including the terminator").
    assert_eq!(
        "ab;cd"
            .chars()
            .take_while_inclusive(|&c| c != ';')
            .collect::<String>(),
        "ab;"
    );
}

pub fn main() {
    grouping();
    sorting_adaptors();
    std_equivalents();
    aggregation_merging_and_results();
    dedup_unique_count_extremes();
    windows_and_weaving();
    merging();
    zipping();
    combinatorics();
    folding_and_results();
    searching_and_position();
    stateful_consumption();
    println!("itertools: all sections passed");
}
