//! RECIPE 716 (trees): learned indexes: a two-stage Recursive Model Index (RMI), a PGM-style piecewise-linear index with a hard epsilon, and an updatable PGM with a delta buffer.
//! LEVEL: advanced
//! Use in: read-mostly sorted lookups (static SSTable/segment indexes, analytics columns, dictionaries), research into replacing B-trees by models.   Run: cargo run --release --example learned_index_rmi
#![allow(dead_code)]
//!
//! WHAT: an index is a function key -> position in a sorted array, i.e. the key's CDF scaled by n. A learned index FITS that CDF
//! with a model and then fixes the model's mistakes with a small bounded search, instead of walking a tree.
//!   RMI  : stage 1 is one linear model over all keys; its prediction picks one of m stage-2 linear models; that model predicts
//!          the position. Every stage-2 model remembers the MIN and MAX of (true position - predicted position) over the keys it
//!          was trained on, so a lookup binary-searches only the window [pred+min_err, pred+max_err]. Guarantee: every key that
//!          was trained on is inside its window (routing is a deterministic function of the key), so present keys are never missed.
//!   PGM  : one greedy pass cuts the sorted keys into segments, each a line that is within EPSILON positions of every key it
//!          covers (the "shrinking cone": keep the interval of slopes that still satisfies all points so far, cut when it empties).
//!          The segment start keys are indexed the same way, recursively, until one segment remains. A lookup is a few model
//!          evaluations plus a binary search over at most 2*eps+3 slots. Unlike the RMI the bound is a PARAMETER, not a measurement.
//!   Updatable PGM : the learned part is immutable, so inserts go to a small sorted delta set, deletes of base keys to a tombstone
//!          set, and when those exceed a fraction of the base the structure is rebuilt by a linear merge (an LSM in miniature).
//! WHY it can win: for smooth key distributions a few dozen bytes of model replace a whole tree level hierarchy, and a model
//! evaluation is a multiply-add instead of a cache-missing pointer hop. WHY it can lose: a straight line is a poor fit for
//! clustered or stepped keys (windows blow up or you need many segments), models need retraining on updates, and the guarantee
//! is only about keys the model was built on (absent keys need a fix-up walk to find their insertion point).
//! Comparisons are counted in an instrumented binary search so the numbers are reproducible; wall-clock time is NOT measured or asserted.
//! Verified by exactness checks on 2000 random datasets (uniform, lognormal, clustered, step, quadratic): every present key is
//! found INSIDE its predicted window (zero fix-up steps, i.e. the error bound is never violated), absent keys get the exact
//! insertion point, and range queries equal the oracle's; plus a 20k-op differential test of the updatable PGM against `BTreeSet`.
//! COMMON USES: the idea comes from "The Case for Learned Index Structures"; PGM-index is an open-source library; learned
//! models also index sorted runs in research LSM engines (for example Bourbon) and are used as cardinality/CDF estimators.
//! Course: CMU 15-721 (Advanced Database Systems): learned indexes / index structures lecture; CMU 15-445 for the B+tree baseline.
//! Read: Kraska et al., "The Case for Learned Index Structures": https://arxiv.org/pdf/1712.01208
//! Read: Ferragina, Vinciguerra, "The PGM-index: a fully-dynamic compressed learned index with provable worst-case bounds": http://www.vldb.org/pvldb/vol13/p1162-ferragina.pdf
//! Read: Ding et al., "ALEX: An Updatable Adaptive Learned Index": https://arxiv.org/pdf/1905.08898
use std::collections::BTreeSet;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Counts comparisons of the final binary search (`cmps`) and corrective steps outside the predicted window (`fix`).
#[derive(Default, Clone, Copy)]
struct Stat {
    cmps: u64,
    fix: u64,
    inner: u64, // corrective steps at the inner PGM levels (a key past a segment's last point extrapolates freely: allowed)
}

/// First index in [lo, hi) with keys[idx] >= key, or hi. Every probe is counted.
fn lb(keys: &[u64], lo: usize, hi: usize, key: u64, st: &mut Stat) -> usize {
    let (mut l, mut h) = (lo, hi);
    while l < h {
        let m = (l + h) / 2;
        st.cmps += 1;
        if keys[m] < key {
            l = m + 1;
        } else {
            h = m;
        }
    }
    l
}

/// Make a window result exact. A correct window needs zero steps for present keys; absent keys may need a few.
fn fixup(keys: &[u64], mut p: usize, key: u64, st: &mut Stat) -> usize {
    while p > 0 && keys[p - 1] >= key {
        p -= 1;
        st.fix += 1;
        st.cmps += 1;
    }
    while p < keys.len() && keys[p] < key {
        p += 1;
        st.fix += 1;
        st.cmps += 1;
    }
    p
}

fn window(pred: f64, lo_off: f64, hi_off: f64, n: usize) -> (usize, usize) {
    let lo = (pred + lo_off).floor().max(0.0) as usize;
    let hi = ((pred + hi_off).ceil().max(0.0) as usize + 1).min(n);
    (lo.min(n), hi.max(lo.min(n)))
}

trait Index {
    fn name(&self) -> String;
    fn lower_bound(&self, keys: &[u64], key: u64, st: &mut Stat) -> usize;
    fn mem_bytes(&self) -> usize;
}

// ------------------------------------------------------------------------------------------------ baselines
struct BinSearch;
impl Index for BinSearch {
    fn name(&self) -> String {
        "binary search".into()
    }
    fn lower_bound(&self, keys: &[u64], key: u64, st: &mut Stat) -> usize {
        lb(keys, 0, keys.len(), key, st)
    }
    fn mem_bytes(&self) -> usize {
        0
    }
}

/// Static B+tree-like index: upper levels are every F-th key of the level below (implicit child pointers).
struct StaticBTree {
    upper: Vec<Vec<u64>>, // upper[0] samples the data, upper[1] samples upper[0], ... last one has <= F keys
}
const F: usize = 64;
impl StaticBTree {
    fn build(keys: &[u64]) -> Self {
        let mut upper: Vec<Vec<u64>> = vec![];
        let mut cur: Vec<u64> = keys.iter().step_by(F).cloned().collect();
        while cur.len() > F {
            let next: Vec<u64> = cur.iter().step_by(F).cloned().collect();
            upper.push(cur);
            cur = next;
        }
        upper.push(cur);
        StaticBTree { upper }
    }
}
impl Index for StaticBTree {
    fn name(&self) -> String {
        format!("static B+tree F={F}")
    }
    fn lower_bound(&self, keys: &[u64], key: u64, st: &mut Stat) -> usize {
        // At each level, find the first separator >= key at index i; the answer below lies in ((i-1)*F, i*F].
        let top = self.upper.last().unwrap();
        let mut i = lb(top, 0, top.len(), key, st);
        for l in (0..self.upper.len()).rev() {
            let below: &[u64] = if l == 0 { keys } else { &self.upper[l - 1] };
            let lo = if i == 0 { 0 } else { (i - 1) * F + 1 };
            let hi = (i * F + 1).min(below.len());
            i = lb(below, lo.min(hi), hi, key, st);
        }
        i
    }
    fn mem_bytes(&self) -> usize {
        self.upper.iter().map(|l| l.len() * 8).sum()
    }
}

// ------------------------------------------------------------------------------------------------ RMI
/// Least-squares line through (x, y) points, computed on centred values for numerical stability.
fn fit<I: Iterator<Item = (f64, f64)> + Clone>(pts: I) -> (f64, f64) {
    let n = pts.clone().count() as f64;
    let (mx, my) = (
        pts.clone().map(|p| p.0).sum::<f64>() / n,
        pts.clone().map(|p| p.1).sum::<f64>() / n,
    );
    let (sxx, sxy) = pts.fold((0.0, 0.0), |a, p| {
        (a.0 + (p.0 - mx) * (p.0 - mx), a.1 + (p.0 - mx) * (p.1 - my))
    });
    let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
    (slope, my - slope * mx)
}

#[derive(Clone, Copy, Default)]
struct Leaf {
    slope: f64,
    icpt: f64,
    emin: i64, // min over trained keys of (true pos - floor(pred))
    emax: i64,
}

struct Rmi {
    k0: u64,
    n: usize,
    root: (f64, f64),
    leaves: Vec<Leaf>,
}

impl Rmi {
    fn route(&self, key: u64) -> usize {
        let pos = self.root.0 * (key as f64 - self.k0 as f64) + self.root.1;
        ((pos / self.n as f64) * self.leaves.len() as f64)
            .floor()
            .clamp(0.0, (self.leaves.len() - 1) as f64) as usize
    }
    fn predict(&self, leaf: &Leaf, key: u64) -> i64 {
        (leaf.slope * (key as f64 - self.k0 as f64) + leaf.icpt)
            .floor()
            .clamp(0.0, (self.n - 1) as f64) as i64
    }

    fn build(keys: &[u64], m: usize) -> Rmi {
        let n = keys.len();
        let k0 = keys[0];
        let x = |i: usize| keys[i] as f64 - k0 as f64;
        let root = fit((0..n).map(|i| (x(i), i as f64)));
        let mut r = Rmi {
            k0,
            n,
            root,
            leaves: vec![Leaf::default(); m.max(1)],
        };
        // Group keys by routed leaf. Root slope >= 0 and floor is monotone, so groups are contiguous runs in key order.
        let mut groups: Vec<Vec<usize>> = vec![vec![]; r.leaves.len()];
        for i in 0..n {
            groups[r.route(keys[i])].push(i);
        }
        let mut start = 0usize; // global position where an empty leaf's keys would begin
        for (li, g) in groups.iter().enumerate() {
            if g.is_empty() {
                r.leaves[li] = Leaf {
                    slope: 0.0,
                    icpt: start as f64,
                    emin: 0,
                    emax: 0,
                };
                continue;
            }
            let (slope, icpt) = fit(g.iter().map(|&i| (x(i), i as f64)));
            let mut leaf = Leaf {
                slope,
                icpt,
                emin: i64::MAX,
                emax: i64::MIN,
            };
            for &i in g {
                let e = i as i64 - r.predict(&leaf, keys[i]);
                leaf.emin = leaf.emin.min(e);
                leaf.emax = leaf.emax.max(e);
            }
            r.leaves[li] = leaf;
            start = g[g.len() - 1] + 1;
        }
        r
    }
}

impl Index for Rmi {
    fn name(&self) -> String {
        format!("RMI m={}", self.leaves.len())
    }
    fn lower_bound(&self, keys: &[u64], key: u64, st: &mut Stat) -> usize {
        let leaf = &self.leaves[self.route(key)];
        let p = self.predict(leaf, key) as f64;
        let (lo, hi) = window(p, leaf.emin as f64, leaf.emax as f64, self.n);
        let r = lb(keys, lo, hi, key, st);
        fixup(keys, r, key, st)
    }
    fn mem_bytes(&self) -> usize {
        16 + self.leaves.len() * 32 // root + (slope, icpt, emin, emax) per leaf
    }
}

// ------------------------------------------------------------------------------------------------ PGM
#[derive(Clone, Copy)]
struct Seg {
    x0: u64,
    y0: f64,
    slope: f64,
}
impl Seg {
    fn predict(&self, key: u64) -> f64 {
        self.y0 + self.slope * (key as f64 - self.x0 as f64)
    }
}

/// Greedy "shrinking cone" segmentation of points (xs[i], i) so every point is within `eps` of its segment's line.
fn segment(xs: &[u64], eps: f64) -> Vec<Seg> {
    let (mut segs, mut i) = (vec![], 0);
    while i < xs.len() {
        let (mut lo, mut hi) = (0.0f64, f64::INFINITY); // slope >= 0 keeps the model monotone
        let mut j = i + 1;
        while j < xs.len() {
            let (dx, dy) = ((xs[j] - xs[i]) as f64, (j - i) as f64);
            let (nlo, nhi) = (lo.max((dy - eps) / dx), hi.min((dy + eps) / dx));
            if nlo > nhi {
                break; // no line through the anchor satisfies all points any more: cut here
            }
            (lo, hi) = (nlo, nhi);
            j += 1;
        }
        let slope = if hi.is_infinite() {
            0.0
        } else {
            (lo + hi) / 2.0
        };
        let seg = Seg {
            x0: xs[i],
            y0: i as f64,
            slope,
        };
        for k in i..j {
            assert!(
                (seg.predict(xs[k]) - k as f64).abs() <= eps + 1e-6,
                "segmentation violated its epsilon"
            );
        }
        segs.push(seg);
        i = j;
    }
    segs
}

struct Pgm {
    eps: f64,
    levels: Vec<Vec<Seg>>, // levels[0] indexes the data; levels[l+1] indexes the segment start keys of levels[l]
}
const EPS_INNER: f64 = 4.0;

impl Pgm {
    fn build(keys: &[u64], eps: f64) -> Pgm {
        let mut levels = vec![segment(keys, eps)];
        while levels.last().unwrap().len() > 1 {
            let xs: Vec<u64> = levels.last().unwrap().iter().map(|s| s.x0).collect();
            levels.push(segment(&xs, EPS_INNER));
        }
        Pgm { eps, levels }
    }
    fn segments(&self) -> usize {
        self.levels.iter().map(|l| l.len()).sum()
    }
}

impl Index for Pgm {
    fn name(&self) -> String {
        format!("PGM eps={}", self.eps)
    }
    fn lower_bound(&self, keys: &[u64], key: u64, st: &mut Stat) -> usize {
        let mut seg = self.levels.last().unwrap()[0];
        for l in (0..self.levels.len() - 1).rev() {
            // Find the last segment at level l whose start key is <= key. A key BETWEEN two start keys predicts between
            // their two positions, hence the one-slot slack on each side.
            let arr = &self.levels[l];
            let (lo, hi) = window(
                seg.predict(key),
                -EPS_INNER - 1.0,
                EPS_INNER + 2.0,
                arr.len(),
            );
            let (mut a, mut b) = (lo, hi);
            while a < b {
                let m = (a + b) / 2;
                st.cmps += 1;
                if arr[m].x0 <= key { a = m + 1 } else { b = m }
            }
            let mut j = a.saturating_sub(1);
            while j + 1 < arr.len() && arr[j + 1].x0 <= key {
                j += 1;
                st.inner += 1;
                st.cmps += 1;
            }
            while j > 0 && arr[j].x0 > key {
                j -= 1;
                st.inner += 1;
                st.cmps += 1;
            }
            seg = arr[j];
        }
        let (lo, hi) = window(
            seg.predict(key),
            -self.eps - 1.0,
            self.eps + 1.0,
            keys.len(),
        );
        let r = lb(keys, lo, hi, key, st);
        fixup(keys, r, key, st)
    }
    fn mem_bytes(&self) -> usize {
        self.segments() * 24
    }
}

// ------------------------------------------------------------------------------------------------ updatable PGM
struct DeltaPgm {
    base: Vec<u64>,
    pgm: Pgm,
    delta: BTreeSet<u64>, // keys inserted since the last rebuild (not in base)
    dead: BTreeSet<u64>,  // base keys deleted since the last rebuild (tombstones)
    merges: usize,
}

impl DeltaPgm {
    fn new(base: Vec<u64>) -> Self {
        let pgm = Pgm::build(&base, 32.0);
        DeltaPgm {
            base,
            pgm,
            delta: BTreeSet::new(),
            dead: BTreeSet::new(),
            merges: 0,
        }
    }
    fn base_pos(&self, key: u64) -> Option<usize> {
        let p = self.pgm.lower_bound(&self.base, key, &mut Stat::default());
        (p < self.base.len() && self.base[p] == key).then_some(p)
    }
    fn contains(&self, key: u64) -> bool {
        self.delta.contains(&key) || (self.base_pos(key).is_some() && !self.dead.contains(&key))
    }
    fn insert(&mut self, key: u64) {
        if !self.dead.remove(&key) && self.base_pos(key).is_none() {
            self.delta.insert(key);
        }
        self.maybe_merge();
    }
    fn delete(&mut self, key: u64) {
        if !self.delta.remove(&key) && self.base_pos(key).is_some() {
            self.dead.insert(key);
        }
        self.maybe_merge();
    }
    fn range(&self, lo: u64, hi: u64) -> Vec<u64> {
        let mut p = self.pgm.lower_bound(&self.base, lo, &mut Stat::default());
        let mut out = vec![];
        while p < self.base.len() && self.base[p] <= hi {
            if !self.dead.contains(&self.base[p]) {
                out.push(self.base[p]);
            }
            p += 1;
        }
        out.extend(self.delta.range(lo..=hi));
        out.sort_unstable();
        out
    }
    fn len(&self) -> usize {
        self.base.len() - self.dead.len() + self.delta.len()
    }
    fn maybe_merge(&mut self) {
        if self.delta.len() + self.dead.len() <= (self.base.len() / 8).max(64) {
            return;
        }
        // Linear merge of base (minus tombstones) with delta, then retrain: the only place models are rebuilt.
        let mut merged: Vec<u64> = self
            .base
            .iter()
            .filter(|k| !self.dead.contains(k))
            .cloned()
            .collect();
        merged.extend(self.delta.iter());
        merged.sort_unstable();
        self.pgm = Pgm::build(&merged, 32.0);
        self.base = merged;
        self.delta.clear();
        self.dead.clear();
        self.merges += 1;
    }
}

// ------------------------------------------------------------------------------------------------ datasets and tests
fn dataset(kind: u64, n: usize, rng: &mut Rng) -> Vec<u64> {
    let mut v: Vec<u64> = match kind {
        0 => (0..n).map(|_| rng.below(1 << 40)).collect(),
        1 => {
            (0..n) // lognormal: exp of an approximately normal variable (sum of 12 uniforms - 6), heavy right tail
                .map(|_| {
                    let z: f64 = (0..12).map(|_| rng.unit()).sum::<f64>() - 6.0;
                    ((z * 2.0).exp() * 1e6).min((1u64 << 45) as f64) as u64
                })
                .collect()
        }
        2 => {
            // Clustered: a handful of dense clusters separated by huge gaps. Adversarial for ONE line per big region.
            let centers: Vec<u64> = (0..1 + rng.below(8)).map(|_| rng.below(1 << 40)).collect();
            (0..n)
                .map(|_| centers[rng.below(centers.len() as u64) as usize] + rng.below(5000))
                .collect()
        }
        3 => {
            // Step: dense runs of consecutive keys, then a jump. The CDF is a staircase.
            let s = 1 + rng.below(200) as usize;
            (0..n)
                .map(|i| (i / s) as u64 * 1_000_000_007 + (i % s) as u64)
                .collect()
        }
        _ => {
            // Quadratic gaps: smooth but strongly curved; fine for a few segments, bad for one line.
            let mut acc = 0u64;
            (0..n)
                .map(|i| {
                    acc += 1 + (i as u64 * i as u64) / 64 + rng.below(3);
                    acc
                })
                .collect()
        }
    };
    v.sort_unstable();
    v.dedup();
    v
}

const KINDS: [&str; 5] = ["uniform", "lognormal", "clustered", "step", "quadratic"];

/// Exactness check shared by every index: present keys (zero fix-ups), absent keys, and range scans vs the oracle.
fn verify(idx: &dyn Index, keys: &[u64], rng: &mut Rng, probes: usize) {
    for (i, &k) in keys.iter().enumerate() {
        let mut st = Stat::default();
        let p = idx.lower_bound(keys, k, &mut st);
        assert_eq!(p, i, "{}: wrong position for a present key", idx.name());
        assert_eq!(
            st.fix,
            0,
            "{}: error bound violated: key {k} lay outside its window",
            idx.name()
        );
    }
    let maxk = *keys.last().unwrap();
    for _ in 0..probes {
        let q = match rng.below(3) {
            0 => rng.below(maxk.saturating_add(1000)),
            1 => keys[rng.below(keys.len() as u64) as usize].wrapping_add(1),
            _ => keys[rng.below(keys.len() as u64) as usize].saturating_sub(1),
        };
        let p = idx.lower_bound(keys, q, &mut Stat::default());
        assert_eq!(
            p,
            keys.partition_point(|&x| x < q),
            "{}: wrong insertion point for {q}",
            idx.name()
        );
        let hi = q + rng.below(maxk / 4 + 10);
        let mut got = vec![];
        let mut j = p;
        while j < keys.len() && keys[j] <= hi {
            got.push(keys[j]);
            j += 1;
        }
        let want: Vec<u64> = keys
            .iter()
            .cloned()
            .filter(|&x| x >= q && x <= hi)
            .collect();
        assert_eq!(got, want, "{}: range [{q},{hi}] wrong", idx.name());
    }
    // A key beyond the largest, and zero: edge clamps.
    let mut st = Stat::default();
    assert_eq!(idx.lower_bound(keys, u64::MAX >> 4, &mut st), keys.len());
    assert_eq!(
        idx.lower_bound(keys, 0, &mut st),
        keys.partition_point(|&x| x < 0 + 0)
    );
}

fn avg_cmps(idx: &dyn Index, keys: &[u64], rng: &mut Rng, samples: usize) -> f64 {
    let mut st = Stat::default();
    for _ in 0..samples {
        idx.lower_bound(keys, keys[rng.below(keys.len() as u64) as usize], &mut st);
    }
    st.cmps as f64 / samples as f64
}

fn main() {
    // 1. exactness on 2000 random datasets
    let mut rng = Rng(0x1EA4);
    let (mut checked, mut max_pgm_seg_ratio) = (0usize, 0f64);
    for round in 0..2000 {
        let kind = rng.below(5);
        let n = 50 + rng.below(1500) as usize;
        let keys = dataset(kind, n, &mut rng);
        if keys.len() < 2 {
            continue;
        }
        let eps = [4.0, 8.0, 32.0][rng.below(3) as usize];
        let m = 1 + rng.below(40) as usize;
        let rmi = Rmi::build(&keys, m);
        let pgm = Pgm::build(&keys, eps);
        let bt = StaticBTree::build(&keys);
        verify(&rmi, &keys, &mut rng, 30);
        verify(&pgm, &keys, &mut rng, 30);
        verify(&bt, &keys, &mut rng, 30);
        verify(&BinSearch, &keys, &mut rng, 5);
        checked += keys.len();
        max_pgm_seg_ratio = max_pgm_seg_ratio.max(pgm.levels[0].len() as f64 / keys.len() as f64);
        let _ = round;
    }
    println!(
        "exactness: {checked} keys x 4 indexes over 2000 datasets, zero window violations; worst PGM segments/keys = {max_pgm_seg_ratio:.3}"
    );

    // 2. comparison table at scale
    let n = 200_000;
    println!("\n-- n = {n} keys: avg comparisons per present-key lookup, and index memory --");
    println!(
        "{:<10} {:>8} | {:>9} | {:>9} {:>8} | {:>9} {:>8} | {:>9} {:>6} {:>8}",
        "dist",
        "bin cmps",
        "btree cmp",
        "btree B",
        "",
        "RMI cmp",
        "RMI B",
        "PGM cmp",
        "segs",
        "PGM B"
    );
    let mut uniform_ok = false;
    let mut spread = vec![];
    for (kind, name) in KINDS.iter().enumerate() {
        let keys = dataset(kind as u64, n, &mut Rng(1000 + kind as u64));
        let rmi = Rmi::build(&keys, keys.len() / 100);
        let pgm = Pgm::build(&keys, 32.0);
        let bt = StaticBTree::build(&keys);
        let mut r = Rng(5);
        let (cb, cbt, cr, cp) = (
            avg_cmps(&BinSearch, &keys, &mut r, 20_000),
            avg_cmps(&bt, &keys, &mut r, 20_000),
            avg_cmps(&rmi, &keys, &mut r, 20_000),
            avg_cmps(&pgm, &keys, &mut r, 20_000),
        );
        println!(
            "{:<10} {:>8.1} | {:>9.1} | {:>9} {:>8} | {:>9.1} {:>8} | {:>9.1} {:>6} {:>8}",
            name,
            cb,
            cbt,
            bt.mem_bytes(),
            "",
            cr,
            rmi.mem_bytes(),
            cp,
            pgm.segments(),
            pgm.mem_bytes()
        );
        let max_win = rmi
            .leaves
            .iter()
            .map(|l| l.emax - l.emin + 1)
            .max()
            .unwrap();
        spread.push((*name, max_win, pgm.segments()));
        if kind == 0 {
            assert!(
                cr < cb,
                "on uniform keys the RMI window should beat a full binary search"
            );
            assert!(
                pgm.mem_bytes() < bt.mem_bytes(),
                "on uniform keys the PGM should be smaller than the B+tree index"
            );
            uniform_ok = true;
        }
    }
    assert!(uniform_ok);
    println!("\nworst RMI leaf window / PGM segment count per distribution:");
    for (name, win, segs) in &spread {
        println!("  {name:<10} worst window = {win:>7} slots, PGM(eps=32) segments = {segs}");
    }
    let uniform_win = spread[0].1;
    assert!(
        spread[1..].iter().any(|s| s.1 > uniform_win * 4),
        "some skewed distribution should produce a much wider worst-case window"
    );
    assert!(
        spread[0].2 * 5 < spread[3].2,
        "staircase data needs far more PGM segments than uniform"
    );

    // 3. updatable PGM vs BTreeSet
    for seed in 0..3u64 {
        let mut rng = Rng(900 + seed);
        let base = dataset(0, 5000, &mut rng);
        let mut oracle: BTreeSet<u64> = base.iter().cloned().collect();
        let mut idx = DeltaPgm::new(base);
        for step in 0..20_000 {
            let k = rng.below(1 << 40) >> 20 << 20 | rng.below(64); // collisions with base are rare; also test re-insert below
            match rng.below(100) {
                0..=39 => {
                    idx.insert(k);
                    oracle.insert(k);
                }
                40..=64 => {
                    let victim = oracle
                        .iter()
                        .nth(rng.below(oracle.len() as u64) as usize)
                        .cloned()
                        .unwrap();
                    idx.delete(victim);
                    oracle.remove(&victim);
                    if rng.below(4) == 0 {
                        idx.insert(victim); // resurrect a tombstoned/deleted key
                        oracle.insert(victim);
                    }
                }
                65..=89 => {
                    let probe = if rng.below(2) == 0 {
                        k
                    } else {
                        *oracle
                            .iter()
                            .nth(rng.below(oracle.len() as u64) as usize)
                            .unwrap()
                    };
                    assert_eq!(
                        idx.contains(probe),
                        oracle.contains(&probe),
                        "contains mismatch at step {step}"
                    );
                }
                _ => {
                    let lo = rng.below(1 << 40);
                    let hi = lo + rng.below(1 << 36);
                    let want: Vec<u64> = oracle.range(lo..=hi).cloned().collect();
                    assert_eq!(idx.range(lo, hi), want, "range mismatch at step {step}");
                }
            }
            assert_eq!(idx.len(), oracle.len());
        }
        assert!(
            idx.merges > 0,
            "the delta buffer should have triggered rebuilds"
        );
        println!(
            "updatable PGM seed {seed}: 20k ops OK, {} merges/rebuilds, final len {}",
            idx.merges,
            idx.len()
        );
    }

    println!(
        "\nWHERE LEARNED INDEXES WIN: smooth CDFs (uniform, mildly curved): a handful of segments replace whole tree levels, memory is"
    );
    println!(
        "  far below a B+tree's upper levels (PGM: ~2 KB vs ~25 KB here; the RMI with n/100 leaves is NOT smaller, it buys fewer comparisons),"
    );
    println!(
        "  and the last-mile search covers ~2*eps slots instead of log2(n). Step data is easy for an RMI (its leaves fit each run) yet costs the PGM 1000+ segments."
    );
    println!(
        "WHERE THEY LOSE: staircases for the PGM (many segments), heavy tails and curvature for a 2-stage RMI (windows of thousands of slots), frequent updates (retrain/merge cost, a delta"
    );
    println!(
        "  buffer to search too), and absent-key / range-start correctness which needs fix-ups; trees give worst-case bounds for free."
    );
    println!("all learned-index checks passed");
}
