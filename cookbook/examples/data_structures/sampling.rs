//! RECIPE 127 (probabilistic): sampling: reservoir (Algorithm R), weighted (A-Res), shuffles, replacement.
//! LEVEL: intermediate
//! Use in: sampling logs/traces/metrics, A/B and test-data selection, shuffling.   Run: cargo run --release --example sampling
//!
//! RESERVOIR (Algorithm R): keep k items from a stream of UNKNOWN length in O(k) memory. Fill the
//! first k; for item i (0-based, i >= k) pick j uniformly in 0..=i and, if j < k, overwrite slot j.
//! Induction shows every item seen so far is in the reservoir with probability exactly k/(i+1).
//! WEIGHTED (A-Res, Efraimidis-Spirakis): give item with weight w the key u^(1/w), u ~ U(0,1), and
//! keep the k largest keys. For k=1 the chance of being chosen is exactly w / sum(w).
//! SHUFFLE: Fisher-Yates swaps position i with a random j in i..n, giving each of the n! orderings
//! probability exactly 1/n!. The "naive" shuffle picks j from the WHOLE range 0..n for each i: that
//! makes n^n equally likely swap sequences, which cannot divide evenly into n! permutations (n^n is
//! not a multiple of n! for n>=3), so some orders are more likely. We count it exactly for n=3.
//! WITH vs WITHOUT replacement: with = independent draws (duplicates possible, n draws from n items
//! see ~63% distinct); without = partial Fisher-Yates (a prefix of a shuffle), never repeats.
//!
//! COMMON USES: taking a fixed-size uniform sample of a log or event stream, trace/request sampling,
//! picking test rows or training mini-batches, weighted sampling for load balancing or importance
//! sampling, shuffling playlists / cards / experiment assignments, and bootstrapping (with replacement).
//! Read: https://en.wikipedia.org/wiki/Reservoir_sampling
//! Read: https://en.wikipedia.org/wiki/Fisher%E2%80%93Yates_shuffle
#![allow(dead_code)]
use std::collections::HashMap;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// Uniform in 0..n. Plain modulo has a negligible bias for n << 2^64; fine for a demo.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    /// Uniform in (0, 1], never exactly 0 so ln/powf stay finite.
    fn unit(&mut self) -> f64 {
        ((self.next() >> 11) + 1) as f64 / (1u64 << 53) as f64
    }
}

/// Algorithm R over any iterator; the stream length is never needed.
fn reservoir<T>(stream: impl IntoIterator<Item = T>, k: usize, rng: &mut Rng) -> Vec<T> {
    let mut res = Vec::with_capacity(k);
    for (i, item) in stream.into_iter().enumerate() {
        if i < k {
            res.push(item);
        } else {
            let j = rng.below(i + 1);
            if j < k {
                res[j] = item; // WHY: item i survives with prob k/(i+1), evicting a uniform slot
            }
        }
    }
    res
}

/// A-Res: keep the k items with the largest u^(1/w). k is small, so a linear scan for the min is fine.
fn weighted_reservoir<T>(
    stream: impl IntoIterator<Item = (T, f64)>,
    k: usize,
    rng: &mut Rng,
) -> Vec<T> {
    let mut keep: Vec<(f64, T)> = Vec::with_capacity(k);
    for (item, w) in stream {
        let key = rng.unit().powf(1.0 / w);
        if keep.len() < k {
            keep.push((key, item));
        } else {
            let (min_i, min_key) = keep
                .iter()
                .enumerate()
                .map(|(i, e)| (i, e.0))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            if key > min_key {
                keep[min_i] = (key, item);
            }
        }
    }
    keep.into_iter().map(|(_, t)| t).collect()
}

fn fisher_yates<T>(v: &mut [T], rng: &mut Rng) {
    for i in 0..v.len() {
        let j = i + rng.below(v.len() - i); // only from the not-yet-fixed suffix
        v.swap(i, j);
    }
}

fn naive_shuffle<T>(v: &mut [T], rng: &mut Rng) {
    for i in 0..v.len() {
        let j = rng.below(v.len()); // BUG: whole range every time
        v.swap(i, j);
    }
}

/// Without replacement: first k slots of a partial Fisher-Yates.
fn sample_without_replacement(n: usize, k: usize, rng: &mut Rng) -> Vec<usize> {
    let mut pool: Vec<usize> = (0..n).collect();
    for i in 0..k {
        let j = i + rng.below(n - i);
        pool.swap(i, j);
    }
    pool.truncate(k);
    pool
}

fn sample_with_replacement(n: usize, k: usize, rng: &mut Rng) -> Vec<usize> {
    (0..k).map(|_| rng.below(n)).collect()
}

fn spread(counts: &HashMap<[u8; 3], u32>) -> (u32, u32) {
    (
        *counts.values().max().unwrap(),
        *counts.values().min().unwrap(),
    )
}

fn main() {
    let mut rng = Rng(0x0123_4567_89AB_CDEF);

    // 1) Reservoir uniformity: every one of 50 items should be kept ~ trials*k/50 times.
    let (n, k, trials) = (50usize, 5usize, 20_000usize);
    let mut counts = vec![0u32; n];
    for _ in 0..trials {
        for x in reservoir(0..n, k, &mut rng) {
            counts[x] += 1;
        }
    }
    let expected = (trials * k / n) as f64;
    let chi2: f64 = counts
        .iter()
        .map(|&c| (c as f64 - expected).powi(2) / expected)
        .sum();
    println!(
        "reservoir: expected {expected} per item, chi-square {chi2:.1} (49 dof => mean ~49; limit 120)"
    );
    assert!(chi2 < 120.0);
    // The first and last stream items must be treated alike (the classic off-by-one check).
    assert!((counts[0] as f64 - counts[n - 1] as f64).abs() < 0.15 * expected);

    // 2) Weighted: with k=1 the pick frequency tracks weight / total.
    let weights = [1.0, 2.0, 3.0, 4.0];
    let mut wc = [0u32; 4];
    let wt = 40_000;
    for _ in 0..wt {
        let pick = weighted_reservoir(
            weights.iter().enumerate().map(|(i, &w)| (i, w)),
            1,
            &mut rng,
        );
        wc[pick[0]] += 1;
    }
    for (i, &c) in wc.iter().enumerate() {
        let (got, want) = (c as f64 / wt as f64, weights[i] / 10.0);
        println!("weighted item {i}: freq {got:.3} vs weight share {want:.3}");
        assert!((got - want).abs() < 0.02);
    }

    // 3) Naive swap shuffle on [0,1,2]: exact enumeration of all 3^3 = 27 equally likely swap paths.
    let mut exact: HashMap<[u8; 3], u32> = HashMap::new();
    for (a, b, c) in (0..3).flat_map(|a| (0..3).flat_map(move |b| (0..3).map(move |c| (a, b, c)))) {
        let mut v = [0u8, 1, 2];
        v.swap(0, a);
        v.swap(1, b);
        v.swap(2, c);
        *exact.entry(v).or_default() += 1;
    }
    let mut dist: Vec<_> = exact.iter().collect();
    dist.sort();
    println!("naive shuffle, 27 paths over 6 permutations (uniform would be 4.5 each): {dist:?}");
    let (hi, lo) = spread(&exact);
    assert_eq!(
        (hi, lo),
        (5, 4),
        "27 is not divisible by 6, so bias is unavoidable"
    );

    // 4) Measure it, and compare with Fisher-Yates (6 outcomes, 60k trials => ~10k each, sd ~ 91).
    let (mut naive, mut fy): (HashMap<[u8; 3], u32>, HashMap<[u8; 3], u32>) = Default::default();
    for _ in 0..60_000 {
        let mut a = [0u8, 1, 2];
        naive_shuffle(&mut a, &mut rng);
        *naive.entry(a).or_default() += 1;
        let mut b = [0u8, 1, 2];
        fisher_yates(&mut b, &mut rng);
        *fy.entry(b).or_default() += 1;
    }
    let (nh, nl) = spread(&naive);
    let (fh, fl) = spread(&fy);
    println!(
        "naive: most {nh} / least {nl} = {:.2}x   fisher-yates: most {fh} / least {fl} = {:.2}x",
        nh as f64 / nl as f64,
        fh as f64 / fl as f64
    );
    assert_eq!(fy.len(), 6);
    assert!(
        nh as f64 / nl as f64 > 1.12,
        "naive bias should be clearly visible (5/4 expected)"
    );
    assert!(
        (fh as f64 / fl as f64) < 1.1,
        "fisher-yates must be uniform"
    );

    // 5) With vs without replacement.
    let without = sample_without_replacement(1_000, 1_000, &mut rng);
    let mut sorted = without.clone();
    sorted.sort();
    assert!(
        sorted.iter().copied().eq(0..1_000),
        "without replacement must be a permutation"
    );
    let with = sample_with_replacement(10_000, 10_000, &mut rng);
    let distinct = with.iter().collect::<std::collections::HashSet<_>>().len() as f64 / 10_000.0;
    println!(
        "with replacement: {:.1}% distinct of n draws from n (theory 1 - 1/e = 63.2%)",
        distinct * 100.0
    );
    assert!((0.62..0.645).contains(&distinct));
}
