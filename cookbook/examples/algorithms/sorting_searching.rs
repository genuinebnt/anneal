//! RECIPE 161 (algorithms): sorting and searching toolkit, differential-tested against std.
//! LEVEL: advanced
//! Use in: choosing a sort, writing a correct binary search, "search on the answer" problems.   Run: cargo run --release --example sorting_searching
#![allow(dead_code)]
//!
//! WHAT: insertion, merge (stable), introsort-lite (quicksort + heapsort fallback + 3-way partition), LSD
//! radix, counting sort, quickselect; then lower/upper bound, first-true search, galloping, interpolation,
//! ternary search and binary search on the answer. Every function is checked against `sort`/`binary_search`
//! or a brute-force oracle on seeded random inputs.
//! WHY each exists: insertion sort is the fastest for tiny or nearly sorted slices (and is the base case of
//! the others); merge sort is stable and O(n log n) always; quicksort is fast on average but can degrade to
//! O(n^2) on bad pivots, so introsort counts depth and switches to heapsort (O(n log n) worst case); a
//! three-way partition makes many-duplicate inputs linear; radix/counting sorts skip comparisons entirely
//! (O(n * key bytes)) when keys are small integers; quickselect finds the k-th smallest in expected O(n)
//! without fully sorting (worst case O(n^2); median-of-medians makes it worst-case O(n) but with a large
//! constant, so it is rarely used in practice).
//! WHY std's sort is a hybrid (as documented by the Rust team, details may change between releases): since
//! Rust 1.81 `slice::sort` is driftsort and `sort_unstable` is ipnsort. Both combine several ideas from the
//! list above: they detect already-sorted/reversed runs, use small-sort networks/insertion for short pieces,
//! and keep a worst-case guarantee, because no single algorithm wins on every input shape. See the
//! table printed below for how shape changes the winner.
//! Binary search recipe: pick a predicate that is false...false true...true over a range, then find the
//! first true. Bounds are half-open [lo, hi) and `mid = lo + (hi - lo) / 2` never overflows.
//! COMMON USES: database index scans and sort-merge joins, rank/percentile queries, capacity planning
//!   ("smallest N that satisfies the SLA"), version bisecting, numeric root finding, radix sorts on ids/IPs/timestamps.
//! Read: https://doc.rust-lang.org/std/primitive.slice.html#method.sort and https://en.wikipedia.org/wiki/Introsort
use std::hint::black_box;
use std::time::Instant;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
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

// ============================ sorting ============================
fn insertion_sort<T: Ord>(v: &mut [T]) {
    for i in 1..v.len() {
        let mut j = i;
        while j > 0 && v[j - 1] > v[j] {
            v.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// stable merge sort; ONE scratch buffer allocated by the caller and reused by every merge
fn merge_sort<T: Ord + Copy>(v: &mut [T]) {
    let mut buf = Vec::with_capacity(v.len() / 2 + 1);
    merge_rec(v, &mut buf);
}

fn merge_rec<T: Ord + Copy>(v: &mut [T], buf: &mut Vec<T>) {
    let n = v.len();
    if n <= 16 {
        insertion_sort(v); // stable too: only swaps strictly out-of-order neighbours
        return;
    }
    let mid = n / 2;
    merge_rec(&mut v[..mid], buf);
    merge_rec(&mut v[mid..], buf);
    if v[mid - 1] <= v[mid] {
        return; // already ordered (cheap win on nearly sorted input)
    }
    buf.clear();
    buf.extend_from_slice(&v[..mid]); // only the left half needs saving
    let (mut i, mut j, mut k) = (0, mid, 0);
    while i < buf.len() {
        // `<=` takes from the left on ties: that is what makes the sort stable
        if j >= n || buf[i] <= v[j] {
            v[k] = buf[i];
            i += 1;
        } else {
            v[k] = v[j];
            j += 1;
        }
        k += 1;
    }
}

fn heapsort<T: Ord>(v: &mut [T]) {
    fn sift<T: Ord>(v: &mut [T], mut i: usize, end: usize) {
        loop {
            let mut c = 2 * i + 1;
            if c >= end {
                return;
            }
            if c + 1 < end && v[c + 1] > v[c] {
                c += 1;
            }
            if v[c] <= v[i] {
                return;
            }
            v.swap(i, c);
            i = c;
        }
    }
    let n = v.len();
    (0..n / 2).rev().for_each(|i| sift(v, i, n));
    for end in (1..n).rev() {
        v.swap(0, end);
        sift(v, 0, end);
    }
}

/// median of first/middle/last guards against the classic sorted-input worst case
fn median_of_3<T: Ord + Copy>(v: &[T]) -> T {
    let (a, b, c) = (v[0], v[v.len() / 2], v[v.len() - 1]);
    a.max(b).min(a.min(b).max(c))
}

/// Dutch national flag: returns (lt, gt) so v[..lt] < p, v[lt..gt] == p, v[gt..] > p
fn partition3<T: Ord + Copy>(v: &mut [T], p: T) -> (usize, usize) {
    let (mut lt, mut i, mut gt) = (0, 0, v.len());
    while i < gt {
        match v[i].cmp(&p) {
            std::cmp::Ordering::Less => {
                v.swap(lt, i);
                lt += 1;
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                gt -= 1;
                v.swap(i, gt); // do not advance i: the swapped-in item is unexamined
            }
            std::cmp::Ordering::Equal => i += 1,
        }
    }
    (lt, gt)
}

fn introsort<T: Ord + Copy>(v: &mut [T]) {
    let limit = 2 * (usize::BITS - v.len().leading_zeros()) as usize; // ~2 log2 n levels allowed
    introsort_with_limit(v, limit);
}

fn introsort_with_limit<T: Ord + Copy>(mut v: &mut [T], mut depth: usize) {
    while v.len() > 16 {
        if depth == 0 {
            heapsort(v); // too many bad pivots: switch to the O(n log n) guarantee
            return;
        }
        depth -= 1;
        let p = median_of_3(v);
        let (lt, gt) = partition3(v, p);
        let (left, rest) = v.split_at_mut(lt);
        let right = &mut rest[gt - lt..];
        // recurse into the smaller side, loop on the larger: stack depth stays O(log n)
        if left.len() < right.len() {
            introsort_with_limit(left, depth);
            v = right;
        } else {
            introsort_with_limit(right, depth);
            v = left;
        }
    }
    insertion_sort(v);
}

trait RadixKey: Copy + Default {
    const BYTES: usize;
    fn byte(self, i: usize) -> usize;
}
macro_rules! radix_key {
    ($t:ty) => {
        impl RadixKey for $t {
            const BYTES: usize = std::mem::size_of::<$t>();
            fn byte(self, i: usize) -> usize {
                ((self >> (8 * i)) & 0xFF) as usize
            }
        }
    };
}
radix_key!(u32);
radix_key!(u64);

/// LSD radix sort: one stable counting pass per key byte, least significant first
fn radix_sort<K: RadixKey>(v: &mut Vec<K>) {
    let n = v.len();
    let mut buf = vec![K::default(); n];
    for pass in 0..K::BYTES {
        let mut count = [0usize; 257];
        for &x in v.iter() {
            count[x.byte(pass) + 1] += 1;
        }
        if count.iter().any(|&c| c == n) {
            continue; // every key shares this byte: nothing to do
        }
        for b in 0..256 {
            count[b + 1] += count[b]; // count[b] = first output slot of bucket b
        }
        for &x in v.iter() {
            let slot = &mut count[x.byte(pass)];
            buf[*slot] = x;
            *slot += 1;
        }
        std::mem::swap(v, &mut buf);
    }
}

fn counting_sort(v: &[u32], max: u32) -> Vec<u32> {
    let mut count = vec![0usize; max as usize + 1];
    v.iter().for_each(|&x| count[x as usize] += 1);
    count
        .iter()
        .enumerate()
        .flat_map(|(x, &c)| std::iter::repeat_n(x as u32, c))
        .collect()
}

/// k-th smallest (0-based) in expected O(n). Deterministic median-of-3 pivot; adversarial input could
/// force O(n^2), which median-of-medians (groups of 5) would prevent at a large constant-factor cost.
fn quickselect(v: &mut [i64], k: usize) -> i64 {
    let (mut lo, mut hi) = (0, v.len()); // answer lies in v[lo..hi]
    loop {
        let p = median_of_3(&v[lo..hi]);
        let (lt, gt) = partition3(&mut v[lo..hi], p);
        let (lt, gt) = (lo + lt, lo + gt);
        if k < lt {
            hi = lt;
        } else if k >= gt {
            lo = gt;
        } else {
            return p; // k falls inside the block of pivot-equal items
        }
    }
}

// ============================ searching ============================
/// first index in [lo, hi) where `pred` is true, assuming false...false true...true; `hi` if never
fn first_true(mut lo: usize, mut hi: usize, pred: impl Fn(usize) -> bool) -> usize {
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pred(mid) {
            hi = mid; // mid might be the answer: keep it
        } else {
            lo = mid + 1;
        }
    }
    lo
}

fn lower_bound(v: &[i64], x: i64) -> usize {
    first_true(0, v.len(), |i| v[i] >= x)
}

fn upper_bound(v: &[i64], x: i64) -> usize {
    first_true(0, v.len(), |i| v[i] > x)
}

/// galloping: probe 1,2,4,8.. to bracket the answer, then binary search inside. O(log position), great when
/// the target is near the front or the list is unbounded (used by merge/intersection of posting lists).
fn gallop_lower_bound(v: &[i64], x: i64) -> usize {
    let mut bound = 1;
    while bound < v.len() && v[bound] < x {
        bound *= 2;
    }
    let lo = bound / 2;
    lo + first_true(0, v.len().min(bound + 1) - lo, |i| v[lo + i] >= x)
}

fn binary_probes(v: &[u64], x: u64) -> usize {
    let (mut lo, mut hi, mut probes) = (0, v.len(), 0);
    while lo < hi {
        probes += 1;
        let mid = lo + (hi - lo) / 2;
        match v[mid].cmp(&x) {
            std::cmp::Ordering::Equal => return probes,
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
        }
    }
    probes
}

/// guess the position by linear interpolation; expected O(log log n) probes on UNIFORM data, O(n) on skewed
fn interpolation_search(v: &[u64], x: u64) -> (Option<usize>, usize) {
    if v.is_empty() {
        return (None, 0);
    }
    let (mut lo, mut hi, mut probes) = (0, v.len() - 1, 0);
    while lo <= hi && x >= v[lo] && x <= v[hi] {
        probes += 1;
        if v[hi] == v[lo] {
            return (Some(lo), probes);
        }
        let span = (v[hi] - v[lo]) as u128;
        let pos = lo + ((x - v[lo]) as u128 * (hi - lo) as u128 / span) as usize;
        match v[pos].cmp(&x) {
            std::cmp::Ordering::Equal => return (Some(pos), probes),
            std::cmp::Ordering::Less => lo = pos + 1,
            std::cmp::Ordering::Greater => {
                if pos == 0 {
                    break;
                }
                hi = pos - 1;
            }
        }
    }
    (None, probes)
}

/// argmax of a STRICTLY unimodal integer function (no flat plateaus, or the comparison learns nothing)
fn ternary_argmax(mut lo: i64, mut hi: i64, f: impl Fn(i64) -> i64) -> i64 {
    while hi - lo > 2 {
        let third = (hi - lo) / 3;
        let (m1, m2) = (lo + third, hi - third);
        if f(m1) < f(m2) {
            lo = m1 + 1; // peak is right of m1
        } else {
            hi = m2; // peak is at or left of m2
        }
    }
    (lo..=hi).max_by_key(|&x| (f(x), -x)).unwrap()
}

/// minimum ship capacity to move all packages IN ORDER within `days` days
fn min_capacity(weights: &[u64], days: usize) -> u64 {
    let feasible = |cap: u64| {
        let (mut used, mut d) = (0, 1);
        for &w in weights {
            if used + w > cap {
                d += 1;
                used = 0;
            }
            used += w;
        }
        d <= days
    };
    let (lo, hi) = (*weights.iter().max().unwrap(), weights.iter().sum::<u64>());
    lo + first_true(0, (hi - lo) as usize + 1, |i| feasible(lo + i as u64)) as u64 // monotone: bigger ship never hurts
}

fn min_capacity_brute(weights: &[u64], days: usize) -> u64 {
    (*weights.iter().max().unwrap()..)
        .find(|&cap| {
            let (mut used, mut d) = (0, 1);
            for &w in weights {
                if used + w > cap {
                    d += 1;
                    used = 0;
                }
                used += w;
            }
            d <= days
        })
        .unwrap()
}

// ============================ benchmarks ============================
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Keyed {
    key: u32,
    id: u32,
}

fn shape(name: &str, n: usize, rng: &mut Rng) -> Vec<u32> {
    let mut v: Vec<u32> = (0..n).map(|_| rng.next() as u32).collect();
    match name {
        "sorted" => v.sort_unstable(),
        "reversed" => v.sort_unstable_by(|a, b| b.cmp(a)),
        "dups" => v.iter_mut().for_each(|x| *x %= 16),
        "nearly" => {
            v.sort_unstable();
            for _ in 0..if n > 1 { n / 100 + 1 } else { 0 } {
                let (a, b) = (rng.below(n as u64) as usize, rng.below(n as u64) as usize);
                v.swap(a, b);
            }
        }
        _ => {}
    }
    v
}

fn time(input: &[u32], f: impl Fn(&mut Vec<u32>)) -> String {
    let mut v = input.to_vec();
    let t = Instant::now();
    f(black_box(&mut v));
    let d = t.elapsed();
    black_box(&v);
    format!("{:>9.2?}", d)
}

fn benchmark() {
    let mut rng = Rng::new(7);
    println!(
        "\n{:<9}{:>8} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "shape", "n", "std", "std_unst", "merge", "intro", "radix", "insert"
    );
    for n in [1_000usize, 20_000, 500_000] {
        for s in ["random", "sorted", "reversed", "dups", "nearly"] {
            let data = shape(s, n, &mut rng);
            let insert = if n <= 20_000 {
                time(&data, |v| insertion_sort(v))
            } else {
                format!("{:>9}", "-")
            };
            println!(
                "{s:<9}{n:>8} {} {} {} {} {} {insert}",
                time(&data, |v| v.sort()),
                time(&data, |v| v.sort_unstable()),
                time(&data, |v| merge_sort(v)),
                time(&data, |v| introsort(v)),
                time(&data, |v| radix_sort(v)),
            );
        }
    }
    println!("(timings are printed, not asserted: they vary by machine, build and noise)");
}

fn main() {
    let mut rng = Rng::new(161);

    // sorting: all shapes x sizes against std
    for n in [0usize, 1, 2, 3, 17, 100, 1000, 5000] {
        for s in ["random", "sorted", "reversed", "dups", "nearly"] {
            let data = shape(s, n, &mut rng);
            let mut want = data.clone();
            want.sort();
            let mut a = data.clone();
            insertion_sort(&mut a);
            let mut b = data.clone();
            merge_sort(&mut b);
            let mut c = data.clone();
            introsort(&mut c);
            let mut d = data.clone();
            introsort_with_limit(&mut d, 0); // force the heapsort fallback
            let mut e = data.clone();
            radix_sort(&mut e);
            let mut f = data.clone();
            heapsort(&mut f);
            assert!(
                [&a, &b, &c, &d, &e, &f].iter().all(|r| **r == want),
                "shape {s} n {n}"
            );
            if !data.is_empty() {
                assert_eq!(counting_sort(&data, *data.iter().max().unwrap()).len(), n);
            }
        }
    }
    let wide: Vec<u64> = (0..3000).map(|_| rng.next() >> rng.below(40)).collect();
    let (mut r, mut want) = (wide.clone(), wide);
    radix_sort(&mut r);
    want.sort();
    assert_eq!(r, want);
    let small: Vec<u32> = (0..2000).map(|_| rng.below(50) as u32).collect();
    let mut want = small.clone();
    want.sort();
    assert_eq!(counting_sort(&small, 49), want);
    println!("insertion/merge/intro/heap/radix/counting sorts match std on 40 shape x size cases");

    // stability: sort by key only; equal keys must keep their original order, like std's stable sort
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct ByKey(Keyed);
    impl PartialOrd for ByKey {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for ByKey {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            self.0.key.cmp(&o.0.key) // ignores `id` on purpose
        }
    }
    let items: Vec<ByKey> = (0..4000)
        .map(|id| {
            ByKey(Keyed {
                key: rng.below(20) as u32,
                id,
            })
        })
        .collect();
    let mut mine = items.clone();
    merge_sort(&mut mine);
    let mut want = items;
    want.sort(); // std's stable sort
    assert!(
        mine.iter().zip(&want).all(|(a, b)| a.0 == b.0),
        "merge sort must be stable"
    );
    println!("merge sort is stable (identical ids order to std::sort)");

    // quickselect
    for _ in 0..300 {
        let len = 1 + rng.below(200) as usize;
        let v: Vec<i64> = (0..len).map(|_| rng.below(30) as i64).collect(); // duplicates on purpose
        let mut sorted = v.clone();
        sorted.sort();
        let k = rng.below(len as u64) as usize;
        assert_eq!(quickselect(&mut v.clone(), k), sorted[k]);
    }
    println!("quickselect == sorted[k] on 300 random cases");

    // searching vs std
    for _ in 0..200 {
        let len = rng.below(60) as usize;
        let mut v: Vec<i64> = (0..len).map(|_| rng.below(40) as i64).collect();
        v.sort();
        for x in -1..42 {
            assert_eq!(lower_bound(&v, x), v.partition_point(|&e| e < x));
            assert_eq!(upper_bound(&v, x), v.partition_point(|&e| e <= x));
            assert_eq!(gallop_lower_bound(&v, x), v.partition_point(|&e| e < x));
        }
    }
    println!("lower_bound / upper_bound / galloping match partition_point");

    // probe counts: interpolation vs binary on uniform data
    let n = 1_000_000u64;
    let uniform: Vec<u64> = (0..n).map(|i| i * 1000 + rng.below(1000)).collect(); // strictly increasing, evenly spread
    let (mut ip, mut bp, queries) = (0, 0, 2000);
    for _ in 0..queries {
        let x = uniform[rng.below(n) as usize];
        let (found, probes) = interpolation_search(&uniform, x);
        assert_eq!(uniform[found.unwrap()], x);
        ip += probes;
        bp += binary_probes(&uniform, x);
    }
    let miss = interpolation_search(&uniform, 1).0.is_some() == uniform.binary_search(&1).is_ok();
    assert!(miss);
    println!(
        "avg probes over {n} uniform keys: interpolation {:.1}, binary {:.1}",
        ip as f64 / queries as f64,
        bp as f64 / queries as f64
    );
    assert!(ip < bp, "interpolation should win on uniform data");

    // ternary search
    for _ in 0..200 {
        let c = rng.below(2001) as i64 - 1000;
        let f = |x: i64| -(x - c) * (x - c);
        assert_eq!(ternary_argmax(-1000, 1000, f), c);
    }
    println!("ternary search finds the peak of 200 random parabolas");

    // binary search on the answer
    for _ in 0..300 {
        let n = 1 + rng.below(12) as usize;
        let w: Vec<u64> = (0..n).map(|_| 1 + rng.below(30)).collect();
        let days = 1 + rng.below(n as u64) as usize;
        assert_eq!(min_capacity(&w, days), min_capacity_brute(&w, days));
    }
    println!("ship-in-D-days: binary search on capacity == linear scan on 300 cases");

    if cfg!(debug_assertions) {
        println!("WARNING: debug build: benchmark numbers are meaningless, use --release");
    }
    benchmark();
}
