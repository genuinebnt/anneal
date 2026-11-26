//! RECIPE 864 (database): analytics on sample streams: LTTB / min-max downsampling, DDSketch and t-digest quantiles, irregular EWMA, Holt-Winters, anomaly detectors, change points, forecasting.
//! LEVEL: advanced
//! Use in: the analysis layer on top of a time-series store: chart rendering, latency percentiles, alerting, capacity forecasts.   Run: cargo run --release --example timeseries_sketches_downsampling_anomaly
//! COMMON USES: LTTB in Grafana-style chart panels; DDSketch in Datadog's agent and OpenTelemetry exponential histograms (same log-bucket idea); t-digest in Elasticsearch,
//! Prometheus-adjacent tools and many OLAP engines; CUSUM/EWMA control charts in SPC and alerting; PELT in the `ruptures` library; Holt-Winters in RRDtool and older Graphite/Grafana forecasts.
//! Course: CMU 15-445/721 (approximate queries and sketches); time-series talks (downsampling for dashboards).
#![allow(dead_code)]
//!
//! WHAT/HOW, section by section (each is verified, then printed as a table):
//! * DOWNSAMPLING FOR CHARTS: LTTB (Largest Triangle Three Buckets) keeps first and last point and, per bucket, the point forming the largest triangle with the
//!   previously chosen point and the AVERAGE of the next bucket: it preserves visual shape. MIN/MAX-per-bucket keeps both extremes of every bucket (<= 2 points
//!   per bucket), so the tallest spike of a bucket can never disappear (a second one in the same bucket can); asserted: global and per-bucket extrema survive. Error = linear-interpolation reconstruction vs the original.
//!   WHAT LTTB DOES NOT PROMISE: it does not minimise reconstruction error. The triangle's third corners (previous pick, next-bucket average) are fixed while a bucket
//!   is scanned, so area = 1/2 * |their x distance| * |candidate's vertical distance from the chord between them|: LTTB picks the point FARTHEST from that chord.
//!   On white noise those are the outliers, so its mean error is clearly WORSE than blind every-nth decimation (asserted, as the counterexample). What it buys is
//!   shape: the drawn line fills much more of each column's true [min, max] range ("envelope" column in the table; asserted on the noisy shapes). Asserted exactly:
//!   every pick is the farthest-from-chord point of its bucket (separate formula as oracle), and a lone spike on a flat line is always kept.
//! * DDSKETCH: log-spaced buckets with ratio gamma = (1+a)/(1-a); a value goes to bucket ceil(log_gamma x) and the bucket answers 2 gamma^i/(gamma+1), which is within
//!   RELATIVE error a of every value in it. Hence any quantile is within a (asserted against exact quantiles). Sketches MERGE by adding bucket counts, so shards combine exactly.
//!   Negative values use a mirrored store; zeros a counter; a bucket cap collapses the lowest buckets: a quantile stays
//!   accurate only while it is within `cap` bucket indices of the maximum (asserted both ways).
//! * T-DIGEST: centroids (mean, weight) whose allowed size shrinks toward the tails by the scale function k(q) = delta/(2 pi) asin(2q-1) (a cluster may span at most one
//!   unit of k), so tail quantiles are very accurate in RANK error; merge = concatenate centroids and recompress. Asserted: bounded rank error and centroid count.
//! * EWMA with irregular spacing: alpha_t = 1 - exp(-dt / tau), so the filter depends on elapsed time, not on how many samples arrived (asserted: step response is
//!   independent of how the interval is split; zero-gap samples are no-ops; regular spacing reduces to the textbook recursion).
//! * DECOMPOSITION-LITE and HOLT-WINTERS (additive): moving-average trend + per-phase seasonal means; level/trend/season smoothing with one-step forecasts.
//! * ANOMALIES: robust z-score (median and MAD of a trailing window, so outliers cannot inflate the scale), CUSUM (accumulates drift above k, alarms at h) and an EWMA
//!   control chart. Measured by simulation: false alarms per sample on null noise and detection delay after injected mean shifts, with asserted bounds.
//! * CHANGE POINTS: PELT (optimal partitioning with pruning, exact for an additive cost such as L2) checked against the O(n^2) dynamic program; binary segmentation as the
//!   greedy baseline; a penalty ~ sigma^2 ln n decides how much gain a split must bring.
//! * FORECASTING: AR(1), seasonal-naive, naive and Holt-Winters compared by MAE on series where each should (and should not) win.
//!
//! Read: Steinarsson, "Downsampling Time Series for Visual Representation" (MSc thesis, University of Iceland, 2013; the LTTB source; link not reachable by curl here, cited by title)
//! Read: Masson, Rim, Lee, "DDSketch: A Fast and Fully-Mergeable Quantile Sketch with Relative-Error Guarantees", VLDB 2019  https://www.vldb.org/pvldb/vol12/p2195-masson.pdf
//! Read: Dunning and Ertl, "Computing Extremely Accurate Quantiles Using t-Digests"  https://github.com/tdunning/t-digest/blob/main/docs/t-digest-paper/histo.pdf
//! Read: Killick, Fearnhead, Eckley, "Optimal detection of changepoints with a linear computational cost" (PELT), JASA 2012 (cited by title)
//! Read: Page (1954) CUSUM and Roberts (1959) EWMA control charts; Hyndman and Athanasopoulos, "Forecasting: Principles and Practice" (cited by title)
//!
//! TRY NEXT:
//!   - [S] Run the DDSketch tests with `alpha = 0.05` and then `0.001`. Predict the bucket count for the lognormal data in each case, then print `buckets()` to check.
//!   - [S] In `lttb`, choose the candidate CLOSEST to the chord instead of farthest from it. Predict which way "mean err" and "envelope" move on the noise rows, then
//!     measure (the optimality assertion will fail, as it should: comment it out for the experiment).
//!   - [M] Add `TDigest::cdf(x)` and use it to build a two-sample comparison (max CDF gap) that flags a latency distribution shift between two shards' merged digests.
//!   - [M] Make CUSUM self-calibrating: estimate mu0 and sigma from the first 100 samples with the median/MAD functions here, and measure how the false-alarm rate changes
//!     when the calibration window itself contains a spike.
//!   - [A] PROJECT: use the `downsample` output of the `tsm_time_series_engine` recipe as detector input and `stream_processing_windows` to run the CUSUM detector per
//!     tumbling window with checkpointed detector state; assert exactly-once alarms across a restart.

// ---------------------------------------------------------------- rng
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn f64(&mut self) -> f64 {
        ((self.next() >> 11) as f64 + 1.0) / ((1u64 << 53) as f64 + 1.0) // (0, 1)
    }
    fn gauss(&mut self) -> f64 {
        (-2.0 * self.f64().ln()).sqrt() * (2.0 * std::f64::consts::PI * self.f64()).cos() // Box-Muller
    }
    fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }
}
fn median(v: &[f64]) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let n = s.len();
    if n % 2 == 1 {
        s[n / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}
fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}
fn mae(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f64>() / a.len() as f64
}

// ---------------------------------------------------------------- downsampling
/// Index range of LTTB bucket `i`: the n-2 interior points split into threshold-2 buckets. Integer arithmetic, so no float rounding can drop a point.
fn lttb_bucket(i: usize, n: usize, threshold: usize) -> (usize, usize) {
    let edge = |j: usize| (j * (n - 2) / (threshold - 2) + 1).min(n);
    (edge(i), edge(i + 1))
}
/// Largest-Triangle-Three-Buckets. Returns indices of the kept points (length == threshold, increasing, includes first and last).
fn lttb(data: &[(f64, f64)], threshold: usize) -> Vec<usize> {
    let n = data.len();
    if threshold >= n || threshold < 3 {
        return (0..n).collect();
    }
    let mut out = vec![0];
    let mut a = 0usize;
    for i in 0..threshold - 2 {
        // average of the NEXT bucket (after the last real bucket that is just the final point)
        let (avg_lo, avg_hi) = lttb_bucket(i + 1, n, threshold);
        let cnt = (avg_hi - avg_lo) as f64;
        let (ax, ay) = data[avg_lo..avg_hi]
            .iter()
            .fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1));
        let (avg_x, avg_y) = (ax / cnt, ay / cnt);
        // current bucket: pick the point with the largest triangle area (a, candidate, avg)
        let (lo, hi) = lttb_bucket(i, n, threshold);
        let (px, py) = data[a];
        let mut best = (-1.0, lo);
        for j in lo..hi {
            let area =
                ((px - avg_x) * (data[j].1 - py) - (px - data[j].0) * (avg_y - py)).abs() * 0.5;
            if area > best.0 {
                best = (area, j);
            }
        }
        out.push(best.1);
        a = best.1;
    }
    out.push(n - 1);
    out
}
/// For each of `buckets` equal index ranges keep the indices of the minimum and the maximum (time ordered, deduplicated).
fn minmax(ys: &[f64], buckets: usize) -> Vec<usize> {
    let n = ys.len();
    let mut out = vec![];
    for b in 0..buckets {
        let (lo, hi) = (b * n / buckets, (b + 1) * n / buckets);
        if lo >= hi {
            continue;
        }
        let imin = (lo..hi).min_by(|&i, &j| ys[i].total_cmp(&ys[j])).unwrap();
        let imax = (lo..hi).max_by(|&i, &j| ys[i].total_cmp(&ys[j])).unwrap();
        let (a, c) = (imin.min(imax), imin.max(imax));
        out.push(a);
        if c != a {
            out.push(c);
        }
    }
    out
}
fn every_nth(n: usize, k: usize) -> Vec<usize> {
    (0..k).map(|i| i * (n - 1) / (k - 1)).collect()
}
/// Reconstruct the polyline through the kept points at every original x; return (max abs error, mean abs error).
fn reconstruction_error(data: &[(f64, f64)], kept: &[usize]) -> (f64, f64) {
    let (mut linf, mut sum) = (0.0f64, 0.0);
    let mut k = 0;
    for (i, &(x, y)) in data.iter().enumerate() {
        while k + 2 < kept.len() && kept[k + 1] < i {
            k += 1;
        }
        let (p, q) = (data[kept[k]], data[kept[(k + 1).min(kept.len() - 1)]]);
        let est = if q.0 == p.0 {
            p.1
        } else {
            p.1 + (q.1 - p.1) * (x - p.0) / (q.0 - p.0)
        };
        let e = (est - y).abs();
        linf = linf.max(e);
        sum += e;
    }
    (linf, sum / data.len() as f64)
}
/// Mean over `cols` pixel columns of the share of the true [min, max] range that the drawn polyline covers inside that column (1.0 = same vertical extent).
fn envelope_coverage(ys: &[f64], kept: &[usize], cols: usize) -> f64 {
    let n = ys.len();
    let mut k = 0;
    let mut sum = 0.0;
    for c in 0..cols {
        let (lo, hi) = (c * n / cols, (c + 1) * n / cols);
        let (mut tmin, mut tmax, mut dmin, mut dmax) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for i in lo..hi {
            while k + 2 < kept.len() && kept[k + 1] < i {
                k += 1;
            }
            let (p, q) = (kept[k], kept[k + 1]);
            // exact at the kept points themselves, interpolated in between
            let drawn = if i == q {
                ys[q]
            } else {
                ys[p] + (ys[q] - ys[p]) * (i as f64 - p as f64) / (q - p) as f64
            };
            (tmin, tmax) = (tmin.min(ys[i]), tmax.max(ys[i]));
            (dmin, dmax) = (dmin.min(drawn), dmax.max(drawn));
        }
        sum += if tmax > tmin {
            (dmax.min(tmax) - dmin.max(tmin)).max(0.0) / (tmax - tmin)
        } else {
            1.0
        };
    }
    sum / cols as f64
}
/// Oracle for LTTB's greedy rule, phrased as vertical distance from the chord (not as a triangle area): each pick lies in its bucket and no other
/// point of that bucket is farther from the line joining the previous pick to the average of the next bucket.
fn assert_lttb_greedy(data: &[(f64, f64)], kept: &[usize], threshold: usize) {
    let n = data.len();
    assert_eq!(kept.len(), threshold);
    assert!(
        kept[0] == 0 && kept[threshold - 1] == n - 1 && kept.windows(2).all(|w| w[0] < w[1]),
        "lttb invariants"
    );
    for i in 0..threshold - 2 {
        let (lo, hi) = lttb_bucket(i, n, threshold);
        let (nlo, nhi) = lttb_bucket(i + 1, n, threshold);
        let next = &data[nlo..nhi];
        let (cx, cy) = (
            next.iter().map(|p| p.0).sum::<f64>() / next.len() as f64,
            next.iter().map(|p| p.1).sum::<f64>() / next.len() as f64,
        );
        let (ax, ay) = data[kept[i]];
        let dist = |j: usize| (data[j].1 - (ay + (cy - ay) * (data[j].0 - ax) / (cx - ax))).abs();
        let pick = kept[i + 1];
        let farthest = (lo..hi).map(dist).fold(0.0, f64::max);
        assert!(
            (lo..hi).contains(&pick),
            "pick {pick} outside bucket {lo}..{hi}"
        );
        assert!(
            dist(pick) >= farthest - 1e-9 * (1.0 + farthest),
            "bucket {i}: picked distance {} but {farthest} was available",
            dist(pick)
        );
    }
}
fn test_lttb_properties(rng: &mut Rng) {
    // greedy rule on small random inputs with IRREGULAR x spacing and awkward n/threshold ratios
    for _ in 0..2000 {
        let n = 4 + rng.below(200) as usize;
        let threshold = 3 + rng.below(n as u64 - 3) as usize;
        let mut x = 0.0;
        let data: Vec<(f64, f64)> = (0..n)
            .map(|_| {
                x += 0.01 + rng.f64() * 5.0;
                (x, rng.gauss() * 10.0)
            })
            .collect();
        assert_lttb_greedy(&data, &lttb(&data, threshold), threshold);
    }
    // a lone spike (either sign) on a flat line is always kept: in its own bucket it is the only point off the chord
    let mut nth_hits = 0;
    for _ in 0..2000 {
        let n = 50 + rng.below(2000) as usize;
        let threshold = 3 + rng.below(40) as usize;
        let at = rng.below(n as u64) as usize;
        let mut data: Vec<(f64, f64)> = (0..n).map(|i| (i as f64, 0.0)).collect();
        data[at].1 = if rng.chance(0.5) { 40.0 } else { -40.0 };
        assert!(
            lttb(&data, threshold).contains(&at),
            "lone spike at {at} lost (n={n}, threshold={threshold})"
        );
        nth_hits += every_nth(n, threshold).contains(&at) as usize;
    }
    assert!(nth_hits < 400, "decimation only keeps a spike by luck");
    println!(
        "LTTB: farthest-from-chord rule on 2000 irregular inputs ok; lone spike kept 2000/2000 (every-nth: {nth_hits}/2000)\n"
    );
}
fn test_downsampling(rng: &mut Rng) {
    let n = 20_000;
    let shapes: [(&str, Box<dyn Fn(usize, &mut Rng) -> f64>); 3] = [
        ("sine", Box::new(|i, _| (i as f64 / 400.0).sin() * 10.0)),
        ("noise", Box::new(|_, r| r.gauss())),
        (
            "spiky",
            Box::new(|i, r| {
                (i as f64 / 700.0).sin() * 5.0
                    + r.gauss() * 0.3
                    + if r.chance(0.002) { 40.0 } else { 0.0 }
            }),
        ),
    ];
    println!(
        "downsampling {n} points to ~200 (error = polyline reconstruction vs original; peaks = share of the 20 largest points kept)"
    );
    println!(
        "  {:<6} {:<10} {:>7} {:>9} {:>9} {:>7} {:>9}",
        "shape", "method", "points", "max err", "mean err", "peaks", "envelope"
    );
    for (name, f) in &shapes {
        let ys: Vec<f64> = (0..n).map(|i| f(i, rng)).collect();
        let data: Vec<(f64, f64)> = ys.iter().enumerate().map(|(i, &y)| (i as f64, y)).collect();
        let mut top: Vec<usize> = (0..n).collect();
        top.sort_by(|&a, &b| ys[b].abs().total_cmp(&ys[a].abs()));
        top.truncate(20);
        let methods: [(&str, Vec<usize>); 3] = [
            ("lttb", lttb(&data, 200)),
            ("minmax", minmax(&ys, 100)),
            ("every-nth", every_nth(n, 200)),
        ];
        let mut errs = vec![];
        for (m, kept) in &methods {
            let (linf, l1) = reconstruction_error(&data, kept);
            let peaks = top.iter().filter(|i| kept.binary_search(i).is_ok()).count();
            let envelope = envelope_coverage(&ys, kept, 100);
            println!(
                "  {name:<6} {m:<10} {:>7} {linf:>9.3} {l1:>9.3} {:>4}/20 {envelope:>9.3}",
                kept.len(),
                peaks
            );
            errs.push((linf, l1, peaks, envelope));
        }
        let kept = &methods[0].1;
        assert_lttb_greedy(&data, kept, 200);
        // min/max keeps every bucket's extremes, hence the global extremes
        let mm = &methods[1].1;
        assert!(
            (errs[1].3 - 1.0).abs() < 1e-12,
            "{name}: min/max covers each of its 100 columns completely"
        );
        let (gmin, gmax) = (
            ys.iter().cloned().fold(f64::INFINITY, f64::min),
            ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        assert!(
            mm.iter().any(|&i| ys[i] == gmin) && mm.iter().any(|&i| ys[i] == gmax),
            "{name}: global extrema must survive min/max"
        );
        for b in 0..100 {
            let (lo, hi) = (b * n / 100, (b + 1) * n / 100);
            let bmax = ys[lo..hi].iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            assert!(
                mm.iter().any(|&i| i >= lo && i < hi && ys[i] == bmax),
                "bucket {b} max lost"
            );
        }
        if *name == "spiky" {
            // NOT "every spike": a bucket keeps one maximum, so a second spike in the same bucket can be dropped. The exact guarantee is that
            // a large positive peak is missing only if its bucket already shows one at least as tall. (Max error does not separate the methods
            // here: one dropped spike, under any method, costs about the full spike height.)
            for &i in top.iter().filter(|&&i| ys[i] > 0.0) {
                assert!(
                    mm.iter()
                        .any(|&j| j * 100 / n == i * 100 / n && ys[j] >= ys[i]),
                    "peak at {i} lost without a taller one in its bucket"
                );
            }
            assert!(
                errs[1].2 > errs[2].2,
                "min/max keeps more of the largest peaks than decimation"
            );
        }
        if *name != "sine" {
            assert!(
                errs[0].3 > 1.5 * errs[2].3,
                "{name}: lttb must draw far more of each column's range than every-nth"
            );
        }
        if *name == "spiky" {
            assert!(
                errs[0].2 > errs[2].2,
                "lttb keeps more of the largest peaks than decimation"
            );
        }
        if *name == "noise" {
            // The counterexample to "LTTB is more accurate": it selects outliers, and a line through outliers is far from the typical sample.
            let mean_abs =
                |kept: &[usize]| kept.iter().map(|&i| ys[i].abs()).sum::<f64>() / kept.len() as f64;
            assert!(
                mean_abs(kept) > 2.0 * mean_abs(&methods[2].1),
                "lttb picks are biased towards extremes"
            );
            assert!(
                errs[0].1 > 1.15 * errs[2].1,
                "on white noise lttb has WORSE mean error than every-nth"
            );
        }
    }
}

// ---------------------------------------------------------------- DDSketch
use std::collections::BTreeMap;
#[derive(Clone)]
struct DdSketch {
    alpha: f64,
    gamma: f64,
    ln_gamma: f64,
    pos: BTreeMap<i32, u64>,
    neg: BTreeMap<i32, u64>,
    zeros: u64,
    count: u64,
    max_buckets: usize,
    collapsed: bool,
}
impl DdSketch {
    fn new(alpha: f64, max_buckets: usize) -> Self {
        let gamma = (1.0 + alpha) / (1.0 - alpha);
        DdSketch {
            alpha,
            gamma,
            ln_gamma: gamma.ln(),
            pos: BTreeMap::new(),
            neg: BTreeMap::new(),
            zeros: 0,
            count: 0,
            max_buckets,
            collapsed: false,
        }
    }
    fn index(&self, x: f64) -> i32 {
        (x.ln() / self.ln_gamma).ceil() as i32
    }
    fn value(&self, i: i32) -> f64 {
        2.0 * self.gamma.powi(i) / (self.gamma + 1.0)
    }
    fn add(&mut self, x: f64) {
        assert!(x.is_finite());
        self.count += 1;
        if x == 0.0 {
            self.zeros += 1;
        } else if x > 0.0 {
            *self.pos.entry(self.index(x)).or_default() += 1;
        } else {
            *self.neg.entry(self.index(-x)).or_default() += 1;
        }
        self.enforce_cap();
    }
    fn enforce_cap(&mut self) {
        while self.pos.len() + self.neg.len() > self.max_buckets {
            // collapse the smallest-magnitude positive bucket into its neighbour (accuracy is sacrificed for the LOWEST quantiles)
            let store = if self.pos.len() >= 2 {
                &mut self.pos
            } else {
                &mut self.neg
            };
            let (&lo, &c) = store.iter().next().unwrap();
            store.remove(&lo);
            let (&next, _) = store.iter().next().expect("cap too small");
            *store.get_mut(&next).unwrap() += c;
            self.collapsed = true;
        }
    }
    fn merge(&mut self, other: &DdSketch) {
        assert_eq!(
            self.alpha, other.alpha,
            "sketches must share the mapping to merge"
        );
        for (&i, &c) in &other.pos {
            *self.pos.entry(i).or_default() += c;
        }
        for (&i, &c) in &other.neg {
            *self.neg.entry(i).or_default() += c;
        }
        self.zeros += other.zeros;
        self.count += other.count;
        self.collapsed |= other.collapsed;
        self.enforce_cap();
    }
    fn buckets(&self) -> usize {
        self.pos.len() + self.neg.len()
    }
    /// Estimate of the element with 0-based rank floor(q (n-1)).
    fn quantile(&self, q: f64) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        let rank = (q * (self.count - 1) as f64).floor() as u64;
        let mut seen = 0;
        for (&i, &c) in self.neg.iter().rev() {
            seen += c;
            if seen > rank {
                return Some(-self.value(i));
            }
        }
        seen += self.zeros;
        if seen > rank {
            return Some(0.0);
        }
        for (&i, &c) in &self.pos {
            seen += c;
            if seen > rank {
                return Some(self.value(i));
            }
        }
        None
    }
}
fn exact_quantile(sorted: &[f64], q: f64) -> f64 {
    sorted[(q * (sorted.len() - 1) as f64).floor() as usize]
}
fn test_ddsketch(rng: &mut Rng) {
    println!(
        "\nDDSketch relative error vs exact quantiles (alpha = 1%), 200k values per distribution"
    );
    println!(
        "  {:<12} {:>8} {:>10} {:>12}",
        "data", "buckets", "max rel.err", "merged==single"
    );
    let qs = [
        0.0, 0.001, 0.01, 0.1, 0.25, 0.5, 0.75, 0.9, 0.99, 0.999, 1.0,
    ];
    let dists: [(&str, Box<dyn Fn(&mut Rng) -> f64>); 4] = [
        ("lognormal", Box::new(|r| (r.gauss() * 1.5 + 3.0).exp())),
        ("uniform", Box::new(|r| r.f64() * 1000.0)),
        (
            "mixed signs",
            Box::new(|r| {
                if r.chance(0.5) {
                    r.gauss() * 50.0
                } else {
                    -(r.gauss().abs() * 10.0)
                }
            }),
        ),
        (
            "with zeros",
            Box::new(|r| {
                if r.chance(0.3) {
                    0.0
                } else {
                    r.f64().powi(3) * 1e4
                }
            }),
        ),
    ];
    for (name, f) in &dists {
        let data: Vec<f64> = (0..200_000).map(|_| f(rng)).collect();
        let mut single = DdSketch::new(0.01, usize::MAX);
        let mut shards: Vec<DdSketch> = (0..8).map(|_| DdSketch::new(0.01, usize::MAX)).collect();
        for (i, &x) in data.iter().enumerate() {
            single.add(x);
            shards[(i * 7 + (x.abs() as usize)) % 8].add(x);
        }
        let mut merged = DdSketch::new(0.01, usize::MAX);
        shards.iter().for_each(|s| merged.merge(s));
        let mut sorted = data.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let mut worst = 0.0f64;
        for &q in &qs {
            let (truth, est) = (exact_quantile(&sorted, q), single.quantile(q).unwrap());
            let rel = if truth == 0.0 {
                est.abs()
            } else {
                (est - truth).abs() / truth.abs()
            };
            worst = worst.max(rel);
            assert!(
                rel <= 0.01 * (1.0 + 1e-9),
                "{name} q={q}: est {est} truth {truth} rel {rel}"
            );
        }
        assert!(
            single.pos == merged.pos && single.neg == merged.neg && single.zeros == merged.zeros,
            "{name}: merge must equal the single sketch exactly"
        );
        println!(
            "  {name:<12} {:>8} {worst:>10.5} {:>12}",
            single.buckets(),
            "yes"
        );
    }
    // bucket cap: the LOWEST buckets collapse. A positive quantile x_q keeps its guarantee while it is within `cap` bucket indices of the maximum,
    // index(max) - index(x_q) < cap: then there can never be `cap` occupied buckets above its own, so it is never merged upwards. This is a limit on
    // the VALUE RANGE max/x_q (about gamma^cap), not on q: too small a cap for heavy-tailed data loses even the median.
    let data: Vec<f64> = (0..200_000)
        .map(|_| (rng.gauss() * 2.0 + 3.0).exp())
        .collect();
    let mut sorted = data.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut full = DdSketch::new(0.01, usize::MAX);
    data.iter().for_each(|&x| full.add(x));
    println!(
        "  bucket cap on heavy-tailed data (uncapped uses {} buckets):",
        full.buckets()
    );
    for cap in [150usize, 600] {
        let mut capped = DdSketch::new(0.01, cap);
        data.iter().for_each(|&x| capped.add(x));
        assert!(capped.collapsed && capped.buckets() <= cap);
        let top = capped.index(sorted[sorted.len() - 1]);
        let mut line = format!("    cap {cap:>4}:");
        for q in [0.0001, 0.5, 0.9, 0.99, 0.999] {
            let (t, e) = (exact_quantile(&sorted, q), capped.quantile(q).unwrap());
            let rel = (e - t).abs() / t;
            let covered = ((top - capped.index(t)) as usize) < cap;
            assert!(
                !covered || rel <= 0.01 * (1.0 + 1e-9),
                "cap {cap} q={q}: inside the covered range but off by {rel}"
            );
            line += &format!(
                "  p{} {:.1}%{}",
                q * 100.0,
                100.0 * rel,
                if covered { "" } else { " (collapsed)" }
            );
        }
        println!("{line}");
        if cap == 150 {
            let (t, e) = (exact_quantile(&sorted, 0.5), capped.quantile(0.5).unwrap());
            assert!(
                (e - t).abs() / t > 0.01,
                "150 buckets cannot span median..max here, so the median is lost"
            );
        } else {
            assert!(
                ((top - capped.index(exact_quantile(&sorted, 0.5))) as usize) < cap,
                "600 buckets reach down to the median (so the check above is not vacuous)"
            );
        }
    }
}

// ---------------------------------------------------------------- t-digest
#[derive(Clone)]
struct TDigest {
    delta: f64,
    cents: Vec<(f64, f64)>, // (mean, weight) sorted by mean
    buf: Vec<f64>,
    total: f64,
    min: f64,
    max: f64,
}
impl TDigest {
    fn new(delta: f64) -> Self {
        TDigest {
            delta,
            cents: vec![],
            buf: vec![],
            total: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }
    fn k(&self, q: f64) -> f64 {
        self.delta / (2.0 * std::f64::consts::PI) * (2.0 * q - 1.0).clamp(-1.0, 1.0).asin()
    }
    fn add(&mut self, x: f64) {
        self.buf.push(x);
        self.min = self.min.min(x);
        self.max = self.max.max(x);
        self.total += 1.0;
        if self.buf.len() >= (self.delta as usize) * 20 {
            self.compress();
        }
    }
    fn compress(&mut self) {
        let mut all: Vec<(f64, f64)> = std::mem::take(&mut self.cents);
        all.extend(self.buf.drain(..).map(|x| (x, 1.0)));
        all.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut out: Vec<(f64, f64)> = vec![];
        let mut cur = all[0];
        let mut w_before = 0.0; // weight strictly before `cur`
        for &(m, w) in &all[1..] {
            let (q_left, q_right) = (w_before / self.total, (w_before + cur.1 + w) / self.total);
            if self.k(q_right) - self.k(q_left) <= 1.0 {
                cur = ((cur.0 * cur.1 + m * w) / (cur.1 + w), cur.1 + w);
            } else {
                w_before += cur.1;
                out.push(cur);
                cur = (m, w);
            }
        }
        out.push(cur);
        self.cents = out;
    }
    fn merge(&mut self, other: &TDigest) {
        let mut o = other.clone();
        o.compress();
        self.cents.extend(o.cents);
        self.total += o.total;
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
        if !self.cents.is_empty() {
            self.cents.sort_by(|a, b| a.0.total_cmp(&b.0));
            self.compress();
        }
    }
    fn quantile(&mut self, q: f64) -> f64 {
        if !self.buf.is_empty() || self.cents.is_empty() {
            self.compress();
        }
        let target = q * self.total;
        let c = &self.cents;
        let first_half = c[0].1 / 2.0;
        if target <= first_half {
            return self.min + (c[0].0 - self.min) * (target / first_half);
        }
        let mut cum = 0.0;
        for i in 0..c.len() {
            let center = cum + c[i].1 / 2.0;
            if i + 1 < c.len() {
                let next_center = cum + c[i].1 + c[i + 1].1 / 2.0;
                if target < next_center {
                    return c[i].0
                        + (c[i + 1].0 - c[i].0) * (target - center) / (next_center - center);
                }
            }
            cum += c[i].1;
        }
        let last_half = c[c.len() - 1].1 / 2.0;
        // upper tail (target >= last centre): interpolate from the last centroid (at its centre) to the exact max (at q = 1)
        self.max
            + (c[c.len() - 1].0 - self.max) * ((self.total - target) / last_half).clamp(0.0, 1.0)
    }
}
fn rank_error(sorted: &[f64], est: f64, q: f64) -> f64 {
    let le = sorted.partition_point(|&x| x <= est) as f64 / sorted.len() as f64;
    let lt = sorted.partition_point(|&x| x < est) as f64 / sorted.len() as f64;
    if q >= lt && q <= le {
        0.0
    } else {
        (q - lt).abs().min((q - le).abs())
    }
}
fn test_tdigest(rng: &mut Rng) {
    println!("\nt-digest rank error (delta = 100), 200k values");
    println!(
        "  {:<10} {:>9} {:>14} {:>14} {:>10}",
        "data", "centroids", "max err (mid)", "max err (tail)", "merged (8)"
    );
    let dists: [(&str, Box<dyn Fn(&mut Rng) -> f64>); 3] = [
        ("lognormal", Box::new(|r| (r.gauss() * 1.5).exp())),
        ("uniform", Box::new(|r| r.f64())),
        (
            "bimodal",
            Box::new(|r| {
                if r.chance(0.5) {
                    r.gauss()
                } else {
                    100.0 + r.gauss() * 5.0
                }
            }),
        ),
    ];
    for (name, f) in &dists {
        let data: Vec<f64> = (0..200_000).map(|_| f(rng)).collect();
        let mut td = TDigest::new(100.0);
        let mut shards: Vec<TDigest> = (0..8).map(|_| TDigest::new(100.0)).collect();
        for (i, &x) in data.iter().enumerate() {
            td.add(x);
            shards[i % 8].add(x);
        }
        let mut merged = TDigest::new(100.0);
        shards.iter().for_each(|s| merged.merge(s));
        let mut sorted = data.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let (mut mid, mut tail, mut mid_m, mut tail_m) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for q in [0.001, 0.01, 0.05, 0.25, 0.5, 0.75, 0.95, 0.99, 0.999] {
            let (e1, e2) = (
                rank_error(&sorted, td.quantile(q), q),
                rank_error(&sorted, merged.quantile(q), q),
            );
            if (0.05..=0.95).contains(&q) {
                mid = mid.max(e1);
                mid_m = mid_m.max(e2);
            } else {
                tail = tail.max(e1);
                tail_m = tail_m.max(e2);
            }
        }
        td.compress();
        merged.compress();
        println!(
            "  {name:<10} {:>9} {mid:>14.5} {tail:>14.5} {:>10.5}",
            td.cents.len(),
            mid_m.max(tail_m)
        );
        assert!(
            td.cents.len() < 200 && merged.cents.len() < 300,
            "centroid count stays O(delta)"
        );
        assert!(
            mid < 0.01 && tail < 0.002,
            "{name}: single digest rank errors {mid} {tail}"
        );
        assert!(
            mid_m < 0.012 && tail_m < 0.003,
            "{name}: merged digest rank errors {mid_m} {tail_m}"
        );
    }
}

// ---------------------------------------------------------------- smoothing, decomposition, Holt-Winters
fn ewma_irregular(ts: &[f64], xs: &[f64], tau: f64) -> Vec<f64> {
    let mut out = vec![xs[0]];
    for i in 1..xs.len() {
        let a = 1.0 - (-(ts[i] - ts[i - 1]) / tau).exp();
        out.push(a * xs[i] + (1.0 - a) * out[i - 1]);
    }
    out
}
fn test_ewma(rng: &mut Rng) {
    // step response independent of how the interval is split into samples
    let tau = 7.0;
    for _ in 0..200 {
        let (e0, c) = (rng.gauss() * 5.0, rng.gauss() * 5.0);
        let (mut t, mut ts, mut xs) = (0.0, vec![0.0], vec![e0]);
        for _ in 0..1 + rng.below(30) {
            t += rng.f64() * 3.0; // gaps may be tiny
            ts.push(t);
            xs.push(c);
        }
        let e = ewma_irregular(&ts, &xs, tau);
        let analytic = c + (e0 - c) * (-(t) / tau).exp();
        assert!(
            (e.last().unwrap() - analytic).abs() < 1e-9,
            "split-independence"
        );
    }
    // zero gap: a repeated timestamp has alpha = 0 and changes nothing
    let e = ewma_irregular(&[0.0, 1.0, 1.0, 2.0], &[0.0, 10.0, 999.0, 10.0], 3.0);
    let e_no_dup = ewma_irregular(&[0.0, 1.0, 2.0], &[0.0, 10.0, 10.0], 3.0);
    assert!((e[3] - e_no_dup[2]).abs() < 1e-12 && (e[2] - e[1]).abs() < 1e-12);
    // regular spacing == fixed-alpha recursion
    let xs: Vec<f64> = (0..50).map(|_| rng.gauss()).collect();
    let ts: Vec<f64> = (0..50).map(|i| i as f64 * 2.0).collect();
    let a = 1.0 - (-2.0f64 / tau).exp();
    let mut s = xs[0];
    let fixed: Vec<f64> = std::iter::once(s)
        .chain(xs[1..].iter().map(|&x| {
            s = a * x + (1.0 - a) * s;
            s
        }))
        .collect();
    assert!(mae(&ewma_irregular(&ts, &xs, tau), &fixed) < 1e-12);
    println!(
        "\nirregular EWMA: split-independent step response, zero-gap no-op, regular == fixed alpha: ok"
    );
}

fn seasonal_decompose(x: &[f64], m: usize) -> (Vec<Option<f64>>, Vec<f64>, Vec<Option<f64>>) {
    let n = x.len();
    // centred moving average of length m (2 x m MA for even m)
    let trend: Vec<Option<f64>> = (0..n)
        .map(|i| {
            let h = m / 2;
            if i < h || i + h >= n {
                return None;
            }
            Some(if m % 2 == 1 {
                mean(&x[i - h..=i + h])
            } else {
                (x[i - h] / 2.0 + x[i - h + 1..i + h].iter().sum::<f64>() + x[i + h] / 2.0)
                    / m as f64
            })
        })
        .collect();
    let mut sums = vec![(0.0, 0usize); m];
    for i in 0..n {
        if let Some(t) = trend[i] {
            sums[i % m].0 += x[i] - t;
            sums[i % m].1 += 1;
        }
    }
    let raw: Vec<f64> = sums.iter().map(|&(s, c)| s / c as f64).collect();
    let centre = mean(&raw);
    let seasonal: Vec<f64> = raw.iter().map(|s| s - centre).collect();
    let resid = (0..n)
        .map(|i| trend[i].map(|t| x[i] - t - seasonal[i % m]))
        .collect();
    (trend, seasonal, resid)
}
struct HoltWinters {
    level: f64,
    trend: f64,
    season: Vec<f64>,
    t: usize,
    a: f64,
    b: f64,
    g: f64,
}
impl HoltWinters {
    fn init(x: &[f64], m: usize, (a, b, g): (f64, f64, f64)) -> Self {
        let l0 = mean(&x[..m]);
        let trend = (mean(&x[m..2 * m]) - l0) / m as f64;
        HoltWinters {
            level: l0,
            trend,
            season: x[..m].iter().map(|v| v - l0).collect(),
            t: 0,
            a,
            b,
            g,
        }
    }
    fn forecast1(&self) -> f64 {
        self.level + self.trend + self.season[self.t % self.season.len()]
    }
    fn update(&mut self, x: f64) {
        let m = self.season.len();
        let s = self.season[self.t % m];
        let new_level = self.a * (x - s) + (1.0 - self.a) * (self.level + self.trend);
        self.trend = self.b * (new_level - self.level) + (1.0 - self.b) * self.trend;
        self.season[self.t % m] = self.g * (x - new_level) + (1.0 - self.g) * s;
        self.level = new_level;
        self.t += 1;
    }
}
fn test_decomposition(rng: &mut Rng) {
    let m = 12;
    let pattern: Vec<f64> = (0..m)
        .map(|i| 5.0 * (i as f64 / m as f64 * 6.2832).sin())
        .collect();
    let x: Vec<f64> = (0..240)
        .map(|i| 0.05 * i as f64 + pattern[i % m] + 0.3 * rng.gauss())
        .collect();
    let (trend, seasonal, resid) = seasonal_decompose(&x, m);
    let centred_pattern: Vec<f64> = pattern.iter().map(|p| p - mean(&pattern)).collect();
    let err = seasonal
        .iter()
        .zip(&centred_pattern)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f64::max);
    let rs: Vec<f64> = resid.iter().flatten().copied().collect();
    let rstd = (rs.iter().map(|r| r * r).sum::<f64>() / rs.len() as f64).sqrt();
    println!(
        "\ndecomposition-lite (period 12, slope 0.05, noise 0.3): max seasonal error {err:.3}, residual std {rstd:.3}"
    );
    assert!(err < 0.2 && rstd < 0.45 && trend.iter().flatten().count() == 240 - m);
}

// ---------------------------------------------------------------- anomaly detection
fn robust_z_alarms(x: &[f64], window: usize, thresh: f64) -> Vec<usize> {
    (window..x.len())
        .filter(|&i| {
            let w = &x[i - window..i];
            let med = median(w);
            let mad = median(&w.iter().map(|v| (v - med).abs()).collect::<Vec<_>>()) * 1.4826;
            mad > 0.0 && (x[i] - med).abs() / mad > thresh
        })
        .collect()
}
/// Two-sided CUSUM on standardised data; resets after each alarm.
fn cusum_alarms(x: &[f64], mu: f64, sigma: f64, k: f64, h: f64) -> Vec<usize> {
    let (mut hi, mut lo, mut out) = (0.0f64, 0.0f64, vec![]);
    for (i, &v) in x.iter().enumerate() {
        let z = (v - mu) / sigma;
        hi = (hi + z - k).max(0.0);
        lo = (lo - z - k).max(0.0);
        if hi > h || lo > h {
            out.push(i);
            hi = 0.0;
            lo = 0.0;
        }
    }
    out
}
fn ewma_chart_alarms(x: &[f64], mu: f64, sigma: f64, lambda: f64, l: f64) -> Vec<usize> {
    let (mut z, mut t, mut out) = (mu, 0, vec![]);
    for (i, &v) in x.iter().enumerate() {
        z = lambda * v + (1.0 - lambda) * z;
        t += 1;
        let var = sigma * sigma * lambda / (2.0 - lambda) * (1.0 - (1.0 - lambda).powi(2 * t));
        if (z - mu).abs() > l * var.sqrt() {
            out.push(i);
            z = mu;
            t = 0;
        }
    }
    out
}
fn test_anomaly(rng: &mut Rng) {
    // 1. false alarms per sample on null noise
    let (trials, len) = (400, 500);
    let (mut fz, mut fc, mut fe) = (0, 0, 0);
    for _ in 0..trials {
        let x: Vec<f64> = (0..len).map(|_| rng.gauss()).collect();
        fz += robust_z_alarms(&x, 50, 3.5).len();
        fc += cusum_alarms(&x, 0.0, 1.0, 0.5, 5.0).len();
        fe += ewma_chart_alarms(&x, 0.0, 1.0, 0.2, 3.0).len();
    }
    let per = (trials * len) as f64;
    println!(
        "\nanomaly detectors on {trials} null series of {len} samples: false alarms per 1000 samples"
    );
    println!(
        "  robust z (w=50, 3.5): {:.2}   CUSUM (k=.5, h=5): {:.2}   EWMA chart (l=.2, L=3): {:.2}",
        fz as f64 / per * 1000.0,
        fc as f64 / per * 1000.0,
        fe as f64 / per * 1000.0
    );
    assert!(
        (fz as f64 / per) < 0.01 && (fc as f64 / per) < 0.01 && (fe as f64 / per) < 0.01,
        "bounded false-positive rate"
    );

    // 2. detection delay after a mean shift at t0
    println!(
        "  detection delay (samples after a shift at t=100; mean over 400 runs; missed in parentheses)"
    );
    println!(
        "  {:>6} {:>14} {:>14} {:>14}",
        "shift", "robust z", "CUSUM", "EWMA chart"
    );
    let t0 = 100;
    for shift in [0.5, 1.0, 2.0, 3.0, 5.0] {
        let mut cols = vec![];
        for det in 0..3 {
            let (mut sum, mut hit, mut miss) = (0usize, 0usize, 0usize);
            for _ in 0..400 {
                let x: Vec<f64> = (0..400)
                    .map(|i| rng.gauss() + if i >= t0 { shift } else { 0.0 })
                    .collect();
                let alarms = match det {
                    0 => robust_z_alarms(&x, 50, 3.5),
                    1 => cusum_alarms(&x, 0.0, 1.0, 0.5, 5.0),
                    _ => ewma_chart_alarms(&x, 0.0, 1.0, 0.2, 3.0),
                };
                match alarms.into_iter().find(|&a| a >= t0) {
                    Some(a) => {
                        sum += a - t0;
                        hit += 1;
                    }
                    None => miss += 1,
                }
            }
            cols.push((sum as f64 / hit.max(1) as f64, miss));
        }
        println!(
            "  {shift:>5.1}σ {:>9.1} ({:>3}) {:>9.1} ({:>3}) {:>9.1} ({:>3})",
            cols[0].0, cols[0].1, cols[1].0, cols[1].1, cols[2].0, cols[2].1
        );
        if shift == 2.0 {
            assert!(
                cols[1].0 < 10.0 && cols[2].0 < 12.0 && cols[1].1 == 0 && cols[2].1 == 0,
                "CUSUM/EWMA must catch a 2σ shift fast"
            );
        }
        if shift == 1.0 {
            assert!(
                cols[1].0 < 25.0 && cols[2].0 < 25.0 && cols[1].1 == 0,
                "and a 1σ shift within bounded delay"
            );
        }
        if shift == 5.0 {
            assert!(
                cols[0].0 < 5.0 && cols[0].1 == 0,
                "a 5σ shift trips even the single-point z-score at once"
            );
        }
    }

    // 3. spikes: precision and recall of the robust z-score
    let (mut tp, mut fp, mut fnn) = (0, 0, 0);
    for _ in 0..100 {
        let mut x: Vec<f64> = (0..600).map(|_| rng.gauss()).collect();
        let mut truth = vec![];
        for i in 60..600 {
            if rng.chance(0.02) {
                x[i] += if rng.chance(0.5) { 9.0 } else { -9.0 };
                truth.push(i);
            }
        }
        let alarms = robust_z_alarms(&x, 50, 3.5);
        tp += alarms.iter().filter(|a| truth.contains(a)).count();
        fp += alarms.iter().filter(|a| !truth.contains(a)).count();
        fnn += truth.iter().filter(|t| !alarms.contains(t)).count();
    }
    let (prec, rec) = (tp as f64 / (tp + fp) as f64, tp as f64 / (tp + fnn) as f64);
    println!(
        "  injected ±9σ spikes (2% of samples): precision {prec:.3}, recall {rec:.3} (robust window ignores earlier spikes)"
    );
    assert!(rec > 0.95 && prec > 0.9);
}

// ---------------------------------------------------------------- change points
fn prefix(x: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let (mut s1, mut s2) = (vec![0.0], vec![0.0]);
    for &v in x {
        s1.push(s1.last().unwrap() + v);
        s2.push(s2.last().unwrap() + v * v);
    }
    (s1, s2)
}
fn l2_cost(p: &(Vec<f64>, Vec<f64>), a: usize, b: usize) -> f64 {
    let (s, q, n) = (p.0[b] - p.0[a], p.1[b] - p.1[a], (b - a) as f64);
    q - s * s / n
}
/// Optimal partitioning with pruning. F[t] = min over s < t of F[s] + cost(s,t) + beta, F[0] = -beta.
fn pelt(x: &[f64], beta: f64) -> (f64, Vec<usize>) {
    let n = x.len();
    let p = prefix(x);
    let mut f = vec![0.0; n + 1];
    f[0] = -beta;
    let mut last = vec![0usize; n + 1];
    let mut cand = vec![0usize];
    for t in 1..=n {
        let (best, arg) = cand
            .iter()
            .map(|&s| (f[s] + l2_cost(&p, s, t) + beta, s))
            .fold((f64::INFINITY, 0), |a, b| if b.0 < a.0 { b } else { a });
        f[t] = best;
        last[t] = arg;
        cand.retain(|&s| f[s] + l2_cost(&p, s, t) <= f[t]); // pruning rule (valid because cost is additive: K = 0)
        cand.push(t);
    }
    let mut cps = vec![];
    let mut t = n;
    while last[t] != 0 {
        t = last[t];
        cps.push(t);
    }
    cps.reverse();
    (f[n], cps)
}
fn optimal_partition_quadratic(x: &[f64], beta: f64) -> f64 {
    let n = x.len();
    let p = prefix(x);
    let mut f = vec![0.0; n + 1];
    f[0] = -beta;
    for t in 1..=n {
        f[t] = (0..t)
            .map(|s| f[s] + l2_cost(&p, s, t) + beta)
            .fold(f64::INFINITY, f64::min);
    }
    f[n]
}
fn binseg(x: &[f64], beta: f64, min_size: usize) -> Vec<usize> {
    let p = prefix(x);
    let mut out = vec![];
    fn rec(
        p: &(Vec<f64>, Vec<f64>),
        lo: usize,
        hi: usize,
        beta: f64,
        min_size: usize,
        out: &mut Vec<usize>,
    ) {
        if hi - lo < 2 * min_size {
            return;
        }
        let whole = l2_cost(p, lo, hi);
        let (gain, t) = (lo + min_size..=hi - min_size)
            .map(|t| (whole - l2_cost(p, lo, t) - l2_cost(p, t, hi), t))
            .fold((f64::NEG_INFINITY, 0), |a, b| if b.0 > a.0 { b } else { a });
        if gain > beta {
            out.push(t);
            rec(p, lo, t, beta, min_size, out);
            rec(p, t, hi, beta, min_size, out);
        }
    }
    rec(&p, 0, x.len(), beta, min_size, &mut out);
    out.sort();
    out
}
fn objective(x: &[f64], cps: &[usize], beta: f64) -> f64 {
    let p = prefix(x);
    let mut bounds = vec![0];
    bounds.extend_from_slice(cps);
    bounds.push(x.len());
    bounds
        .windows(2)
        .map(|w| l2_cost(&p, w[0], w[1]))
        .sum::<f64>()
        + beta * cps.len() as f64
}
fn test_changepoints(rng: &mut Rng) {
    // PELT is exact: same objective as the O(n^2) dynamic program
    for _ in 0..300 {
        let n = 10 + rng.below(70) as usize;
        let mut level = 0.0;
        let x: Vec<f64> = (0..n)
            .map(|_| {
                if rng.chance(0.06) {
                    level = rng.gauss() * 3.0;
                }
                level + rng.gauss() * 0.5
            })
            .collect();
        let beta = 1.0 + rng.f64() * 10.0;
        let (fp, cps) = pelt(&x, beta);
        let fq = optimal_partition_quadratic(&x, beta);
        assert!(
            (fp - fq).abs() < 1e-7 * (1.0 + fq.abs()),
            "PELT {fp} vs DP {fq}"
        );
        assert!(
            (objective(&x, &cps, beta) - fp).abs() < 1e-6 * (1.0 + fp.abs()),
            "reported changepoints match the objective"
        );
    }
    // piecewise-constant signal with known change points
    let truth = [150usize, 300, 420, 800];
    let means = [0.0, 3.0, -1.0, 2.5, 0.5];
    let n = 1000;
    let x: Vec<f64> = (0..n)
        .map(|i| means[truth.iter().filter(|&&c| i >= c).count()] + rng.gauss())
        .collect();
    let beta = 3.0 * (n as f64).ln();
    let (obj, cps) = pelt(&x, beta);
    let bs = binseg(&x, beta, 5);
    println!("\nchange points (n = {n}, noise sigma 1, penalty beta = {beta:.1}):");
    println!(
        "  true     {truth:?}\n  PELT     {cps:?}  objective {obj:.1}\n  bin.seg. {bs:?}  objective {:.1}",
        objective(&x, &bs, beta)
    );
    assert_eq!(
        cps.len(),
        truth.len(),
        "no missed or spurious change points"
    );
    assert!(
        cps.iter()
            .zip(&truth)
            .all(|(&c, &t)| (c as i64 - t as i64).abs() <= 6),
        "locations within ±6 samples"
    );
    assert!(
        obj <= objective(&x, &bs, beta) + 1e-9,
        "PELT is optimal, so never worse than greedy binary segmentation"
    );
    // pure noise: no changes at this penalty
    let noise: Vec<f64> = (0..1000).map(|_| rng.gauss()).collect();
    assert!(
        pelt(&noise, beta).1.is_empty(),
        "pure noise must give no change point"
    );
}

// ---------------------------------------------------------------- forecasting
fn ar1_fit(x: &[f64]) -> (f64, f64) {
    let mu = mean(x);
    let (num, den) = (1..x.len()).fold((0.0, 0.0), |(n, d), i| {
        (
            n + (x[i] - mu) * (x[i - 1] - mu),
            d + (x[i - 1] - mu).powi(2),
        )
    });
    (mu, num / den)
}
fn test_forecasting(rng: &mut Rng) {
    let n = 6000;
    let m = 24;
    // three worlds: each favours a different model
    let mut ar = vec![0.0f64; n];
    for i in 1..n {
        ar[i] = 0.8 * ar[i - 1] + rng.gauss();
    }
    let seasonal: Vec<f64> = (0..n)
        .map(|i| 0.01 * i as f64 + 8.0 * (i as f64 / m as f64 * 6.2832).sin() + 0.5 * rng.gauss())
        .collect();
    let mut walk = vec![0.0f64; n];
    for i in 1..n {
        walk[i] = walk[i - 1] + rng.gauss();
    }
    println!("\none-step-ahead forecasting MAE on the last 30% (fit/initialise on the first 70%)");
    println!(
        "  {:<14} {:>8} {:>8} {:>10} {:>13}",
        "series", "naive", "AR(1)", "seas.naive", "Holt-Winters"
    );
    let mut rows = vec![];
    for (name, x) in [
        ("AR(1) phi=.8", &ar),
        ("trend+seasonal", &seasonal),
        ("random walk", &walk),
    ] {
        let split = n * 7 / 10;
        let (mu, phi) = ar1_fit(&x[..split]);
        let mut hw = HoltWinters::init(&x[..split], m, (0.2, 0.02, 0.1));
        for &v in &x[..split] {
            hw.update(v);
        }
        let truth = &x[split..];
        let naive: Vec<f64> = (split..n).map(|i| x[i - 1]).collect();
        let ar_f: Vec<f64> = (split..n).map(|i| mu + phi * (x[i - 1] - mu)).collect();
        let sn: Vec<f64> = (split..n).map(|i| x[i - m]).collect();
        let hwf: Vec<f64> = (split..n)
            .map(|i| {
                let f = hw.forecast1();
                hw.update(x[i]);
                f
            })
            .collect();
        let r = [
            mae(&naive, truth),
            mae(&ar_f, truth),
            mae(&sn, truth),
            mae(&hwf, truth),
        ];
        println!(
            "  {name:<14} {:>8.3} {:>8.3} {:>10.3} {:>13.3}",
            r[0], r[1], r[2], r[3]
        );
        rows.push(r);
    }
    assert!(
        rows[0][1] < rows[0][0] && rows[0][1] < rows[0][2],
        "AR(1) wins on an AR(1) process"
    );
    assert!(
        rows[1][3] < rows[1][2] && rows[1][3] < rows[1][0] && rows[1][3] < rows[1][1],
        "Holt-Winters wins on trend + seasonality"
    );
    assert!(
        rows[2][0] <= rows[2][2] && rows[2][0] < rows[2][3],
        "naive beats seasonal-naive and HW on a random walk"
    );
    assert!(
        rows[2][1] < rows[2][0] * 1.05,
        "AR(1) fitted on a random walk degenerates to ~naive (phi near 1)"
    );
}

fn main() {
    let mut rng = Rng(0x5E7C_41D5_0864);
    test_lttb_properties(&mut rng);
    test_downsampling(&mut rng);
    test_ddsketch(&mut rng);
    test_tdigest(&mut rng);
    test_ewma(&mut rng);
    test_decomposition(&mut rng);
    test_anomaly(&mut rng);
    test_changepoints(&mut rng);
    test_forecasting(&mut rng);
    println!("\nall timeseries_sketches_downsampling_anomaly checks passed");
}
