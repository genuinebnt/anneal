//! RECIPE 843 (database): lightweight adaptive indexing: zone maps, column imprints and database cracking.
//! LEVEL: intermediate
//! Use in: column stores, data-lake scanners, any scan-heavy engine that cannot afford (or does not know which) full indexes to build.   Run: cargo run --release --example database_cracking_zone_maps_imprints
//! COMMON USES: zone maps (min/max per block) are Netezza "zone maps", Parquet/ORC row-group statistics, Delta/Iceberg file
//! statistics, Snowflake micro-partition pruning, ClickHouse sparse primary index and skip indexes, PostgreSQL BRIN, Oracle storage
//! indexes; column imprints came out of MonetDB; database cracking was prototyped in MonetDB and its adaptive-indexing descendants.
//! Course: CMU 15-721 (storage/scans); the CIDR 2007 cracking paper below.
#![allow(dead_code)]
//!
//! WHAT. Three ways to avoid reading data a range predicate `lo <= v < hi` cannot match, ordered by how much they know up front:
//!   * ZONE MAP: store (min,max) per block of 1024 values. A block with max<lo or min>=hi is skipped; a block entirely inside
//!     [lo,hi) is counted from metadata without touching a value. Cost: 16 bytes per block, built in one pass. Effectiveness is a
//!     property of the DATA LAYOUT, not of the structure: on sorted or clustered data nearly everything is pruned, on random data
//!     nothing is (every block spans the whole domain). That is why warehouses sort/cluster on load (see the Z-order recipe).
//!   * COLUMN IMPRINT: per cache-line block of 64 values keep a 64-bit vector with one bit per value BIN (bins are equi-depth,
//!     taken from a sample). A query becomes a bit mask over bins; a block whose imprint has no bit in common with the mask is
//!     skipped, and a block whose bits are all inside fully-covered bins is counted wholesale. Unlike min/max an imprint notices
//!     HOLES: a block holding values from two narrow, distant windows is pruned for a query between them where min/max cannot.
//!   * DATABASE CRACKING: no upfront work at all. The column starts as an unsorted COPY; every range query PARTITIONS the pieces it
//!     touches around its bounds (crack-in-two: one pass splitting a piece by a pivot; crack-in-three: one pass when both bounds fall
//!     into the same piece), remembers the pivot -> position in a sorted map (the CRACKER INDEX, a BTreeMap here; AVL tree in the
//!     paper), and answers the query as the contiguous slice between the two pivots. Each query leaves the column a bit more sorted
//!     for the next one: the first query costs about one scan, later ones touch ever smaller pieces. Invariant: for every
//!     (pivot, pos) in the index, everything before `pos` is < pivot and everything from `pos` on is >= pivot.
//!   * Described, not implemented: ADAPTIVE MERGING (sort small runs first, then merge only the key ranges queries ask for: more
//!     upfront work, converges in far fewer queries), SIDEWAYS CRACKING (for multi-column queries keep a cracker MAP of (A,B) pairs so
//!     B follows A's swaps and tuple reconstruction stays cheap), STOCHASTIC CRACKING (add a random crack to each query so
//!     sequential workloads, shown below, do not degrade).
//!
//! CHECKS. 2000 random range queries over 200k values: scan, zone map, imprint, sorted-array binary search and cracking return
//! identical counts and sums; the cracker invariant holds after every one of the first 64 queries and at the end, and the cracked
//! column is a permutation of the input. Work is counted in values touched (plus a model of n*ceil(log2 n) comparisons for the
//! sort). Asserted: cracking's first query costs about one scan, its cumulative work crosses below the scan's within 3 queries,
//! its average per-query work falls monotonically across query windows, it is still cheaper than sorting after 2000 queries but
//! does overtake the sort's total if the random queries keep coming, and a sequential workload costs several times more than a
//! random one. Zone map pruning is asserted high on sorted data and ~zero on random data; imprints beat zone maps on holey blocks.
//! Read: Idreos, Kersten, Manegold, "Database Cracking" CIDR 2007 http://cidrdb.org/cidr2007/papers/cidr07p07.pdf
//!       Sidirourgos, Kersten, "Column Imprints: A Secondary Index Structure" SIGMOD 2013 (cited by title; URL not checked)
//!       Idreos et al., "Merging What's Cracked, Cracking What's Merged: Adaptive Indexing in Main-Memory Column-Stores" (cited by title)
//!       PostgreSQL BRIN https://www.postgresql.org/docs/current/brin.html
//!
//! TRY NEXT:
//!   - [S] Make `crack_in_two` pick a RANDOM pivot inside the piece before cracking on the query bound (stochastic cracking); rerun the
//!     sequential workload and compare total work. Predict first: how much does it help the random workload?
//!   - [S] Change the zone-map block size from 1024 to 64 and 16384 and plot pruned fraction against metadata bytes for the "nearly sorted" dataset.
//!   - [M] Add `delete`/`insert` to the cracker column using a pending-updates buffer merged lazily into only the pieces a query touches.
//!   - [M] Replace the equi-depth bins of `Imprints` with equi-width bins; on which dataset in `datasets()` does it hurt, and why?
//!   - [A] Project: combine with `columnar_encodings` and `bitweaving_bit_sliced_scans`: store the cracked column dictionary-encoded and
//!     answer the per-piece scans with bit-sliced predicates; measure values-touched vs bytes-touched.
//!   - [A] Project: combine with `learned_index_rmi`: replace the cracker BTreeMap by a learned model of pivot -> position and test
//!     how often it must be corrected as the index grows.

use std::collections::BTreeMap;
use std::ops::Bound;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

const N: usize = 200_000;
const DOMAIN: i64 = 1_000_000;

/// Ground truth: (count, sum) of values in [lo,hi).
fn scan(col: &[i64], lo: i64, hi: i64) -> (u64, i64) {
    col.iter()
        .filter(|&&v| v >= lo && v < hi)
        .fold((0, 0), |(c, s), &v| (c + 1, s + v))
}

// ---------------- zone maps ----------------
const ZBLOCK: usize = 1024;
struct ZoneMap {
    mins: Vec<i64>,
    maxs: Vec<i64>,
}
#[derive(Default, Clone, Copy, Debug)]
struct ZStats {
    blocks: u64,
    pruned: u64,
    wholesale: u64,
    scanned_blocks: u64,
    values_read: u64,
}
impl ZoneMap {
    fn build(col: &[i64]) -> Self {
        ZoneMap {
            mins: col
                .chunks(ZBLOCK)
                .map(|c| *c.iter().min().unwrap())
                .collect(),
            maxs: col
                .chunks(ZBLOCK)
                .map(|c| *c.iter().max().unwrap())
                .collect(),
        }
    }
    fn query(&self, col: &[i64], lo: i64, hi: i64, st: &mut ZStats) -> (u64, i64) {
        let (mut c, mut s) = (0, 0);
        for (b, chunk) in col.chunks(ZBLOCK).enumerate() {
            st.blocks += 1;
            let (mn, mx) = (self.mins[b], self.maxs[b]);
            if mx < lo || mn >= hi {
                st.pruned += 1; // cannot contain a match
            } else if mn >= lo && mx < hi {
                st.wholesale += 1; // every value matches: answer from the block's own aggregate, no value read
                c += chunk.len() as u64;
                s += chunk.iter().sum::<i64>(); // (a real engine would keep SUM per block; reading it here is test-only, not counted)
            } else {
                st.scanned_blocks += 1;
                st.values_read += chunk.len() as u64;
                let (a, b2) = scan(chunk, lo, hi);
                c += a;
                s += b2;
            }
        }
        (c, s)
    }
}

// ---------------- column imprints ----------------
const IBLOCK: usize = 64;
struct Imprints {
    bounds: Vec<i64>, // 63 ascending bin boundaries -> 64 bins; bin(v) = #bounds <= v
    imps: Vec<u64>,
}
impl Imprints {
    fn bin(&self, v: i64) -> usize {
        self.bounds.partition_point(|&b| b <= v)
    }
    fn build(col: &[i64]) -> Self {
        let mut sample: Vec<i64> = col
            .iter()
            .step_by((col.len() / 4096).max(1))
            .copied()
            .collect();
        sample.sort_unstable();
        let mut bounds: Vec<i64> = (1..64).map(|i| sample[i * sample.len() / 64]).collect();
        bounds.dedup();
        let mut me = Imprints {
            bounds,
            imps: vec![],
        };
        me.imps = col
            .chunks(IBLOCK)
            .map(|c| c.iter().fold(0u64, |m, &v| m | 1 << me.bin(v)))
            .collect();
        me
    }
    /// Bit mask of bins that may intersect [lo,hi), and of bins entirely inside it.
    fn masks(&self, lo: i64, hi: i64) -> (u64, u64) {
        let nb = self.bounds.len() + 1;
        let (mut any, mut inner) = (0u64, 0u64);
        for i in 0..nb {
            let b_lo = if i == 0 { i64::MIN } else { self.bounds[i - 1] }; // bin i covers [b_lo, b_hi)
            let b_hi = if i == nb - 1 {
                i64::MAX
            } else {
                self.bounds[i]
            };
            if b_hi > lo && b_lo < hi {
                any |= 1 << i;
                if b_lo >= lo && b_hi <= hi && i != 0 && i != nb - 1 {
                    inner |= 1 << i;
                }
            }
        }
        (any, inner)
    }
    fn query(&self, col: &[i64], lo: i64, hi: i64, st: &mut ZStats) -> (u64, i64) {
        let (any, inner) = self.masks(lo, hi);
        let (mut c, mut s) = (0, 0);
        for (b, chunk) in col.chunks(IBLOCK).enumerate() {
            st.blocks += 1;
            let imp = self.imps[b];
            if imp & any == 0 {
                st.pruned += 1;
            } else if imp & !inner == 0 {
                st.wholesale += 1;
                c += chunk.len() as u64;
                s += chunk.iter().sum::<i64>();
            } else {
                st.scanned_blocks += 1;
                st.values_read += chunk.len() as u64;
                let (a, b2) = scan(chunk, lo, hi);
                c += a;
                s += b2;
            }
        }
        (c, s)
    }
}

// ---------------- cracking ----------------
struct Cracker {
    col: Vec<i64>,
    idx: BTreeMap<i64, usize>, // pivot -> first position holding a value >= pivot
    work: u64,
}
impl Cracker {
    fn new(col: &[i64]) -> Self {
        Cracker {
            col: col.to_vec(),
            idx: BTreeMap::new(),
            work: 0,
        }
    }
    /// The piece [start,end) that contains values near `v`: bounded by the nearest pivots at or below / above it.
    fn piece(&self, v: i64) -> (usize, usize) {
        let start = self.idx.range(..=v).next_back().map_or(0, |(_, &p)| p);
        let end = self
            .idx
            .range((Bound::Excluded(v), Bound::Unbounded))
            .next()
            .map_or(self.col.len(), |(_, &p)| p);
        (start, end)
    }
    /// Partition [l,h) so that values < pivot come first; returns the split position. One pass, each element examined once.
    fn crack_in_two(&mut self, l: usize, h: usize, pivot: i64) -> usize {
        let (mut i, mut j) = (l, h); // invariant: [l,i) < pivot, [j,h) >= pivot
        while i < j {
            self.work += 1;
            if self.col[i] < pivot {
                i += 1;
            } else {
                j -= 1;
                self.col.swap(i, j);
            }
        }
        i
    }
    /// Dutch-national-flag partition of [l,h) into < lo | [lo,hi) | >= hi in a single pass.
    fn crack_in_three(&mut self, l: usize, h: usize, lo: i64, hi: i64) -> (usize, usize) {
        let (mut lt, mut i, mut gt) = (l, l, h);
        while i < gt {
            self.work += 1;
            let v = self.col[i];
            if v < lo {
                self.col.swap(lt, i);
                lt += 1;
                i += 1;
            } else if v >= hi {
                gt -= 1;
                self.col.swap(i, gt);
            } else {
                i += 1;
            }
        }
        (lt, gt)
    }
    fn crack(&mut self, pivot: i64) -> usize {
        if let Some(&p) = self.idx.get(&pivot) {
            return p; // already a boundary: free
        }
        let (l, h) = self.piece(pivot);
        let pos = self.crack_in_two(l, h, pivot);
        self.idx.insert(pivot, pos);
        pos
    }
    /// Count and sum of values in [lo,hi): crack on both bounds, then the answer is one contiguous slice.
    fn query(&mut self, lo: i64, hi: i64, three_way: bool) -> (u64, i64) {
        if lo >= hi {
            return (0, 0);
        }
        let (p1, p2);
        if three_way
            && !self.idx.contains_key(&lo)
            && !self.idx.contains_key(&hi)
            && self.piece(lo) == self.piece(hi)
        {
            let (l, h) = self.piece(lo);
            (p1, p2) = self.crack_in_three(l, h, lo, hi);
            self.idx.insert(lo, p1);
            self.idx.insert(hi, p2);
        } else {
            p1 = self.crack(lo);
            p2 = self.crack(hi);
        }
        (self.col[p1..p2].len() as u64, self.col[p1..p2].iter().sum())
    }
    fn check_invariant(&self) {
        // Linear-time check: with monotone positions, "everything before pos is < pivot and everything after is >= pivot"
        // for every pivot is equivalent to "each piece lies inside [its pivot, the next pivot)".
        let cuts: Vec<(i64, usize)> = self.idx.iter().map(|(&p, &q)| (p, q)).collect();
        assert!(
            cuts.windows(2).all(|w| w[0].1 <= w[1].1),
            "pivot positions must be monotone in the pivot"
        );
        let mut start = 0;
        let mut lower = i64::MIN;
        for &(pivot, pos) in cuts
            .iter()
            .chain(std::iter::once(&(i64::MAX, self.col.len())))
        {
            assert!(
                self.col[start..pos]
                    .iter()
                    .all(|&v| v >= lower && (v < pivot || pivot == i64::MAX)),
                "piece [{start},{pos}) escapes [{lower},{pivot})"
            );
            start = pos;
            lower = pivot;
        }
    }
    fn max_piece(&self) -> usize {
        let mut cuts: Vec<usize> = vec![0];
        cuts.extend(self.idx.values());
        cuts.push(self.col.len());
        cuts.windows(2).map(|w| w[1] - w[0]).max().unwrap()
    }
}

// ---------------- datasets and workloads ----------------
fn datasets(r: &mut Rng) -> Vec<(&'static str, Vec<i64>)> {
    let random: Vec<i64> = (0..N).map(|_| r.below(DOMAIN as u64) as i64).collect();
    let mut sorted = random.clone();
    sorted.sort_unstable();
    let mut nearly = sorted.clone();
    for _ in 0..N / 20 {
        let (a, b) = (r.below(N as u64) as usize, r.below(N as u64) as usize);
        nearly.swap(a, b); // 5% of positions displaced anywhere: every block gets a few outliers
    }
    // clustered: time-like partitions in random order, each internally shuffled (typical of ingest-by-batch tables)
    let mut parts: Vec<Vec<i64>> = sorted.chunks(2 * ZBLOCK).map(|c| c.to_vec()).collect(); // partitions aligned to zone-map blocks
    for i in (1..parts.len()).rev() {
        parts.swap(i, r.below(i as u64 + 1) as usize);
    }
    let clustered: Vec<i64> = parts.into_iter().flatten().collect();
    // two-window blocks: every 64-value block alternates between two narrow value windows at random places in the domain, so
    // min/max spans most of the domain while only a few imprint bins are ever set
    let w = DOMAIN as u64 / 50;
    let mut holey = Vec::with_capacity(N);
    for _ in 0..N / IBLOCK {
        let (a, b) = (r.below(DOMAIN as u64 - w), r.below(DOMAIN as u64 - w));
        for i in 0..IBLOCK {
            holey.push(((if i % 2 == 0 { a } else { b }) + r.below(w)) as i64);
        }
    }
    holey.resize(N, 0);
    vec![
        ("random", random),
        ("sorted", sorted),
        ("nearly sorted", nearly),
        ("batch-clustered", clustered),
        ("two-window blocks", holey),
    ]
}
fn range_queries(r: &mut Rng, count: usize, sel_pct_x10: u64) -> Vec<(i64, i64)> {
    (0..count)
        .map(|_| {
            let w = (DOMAIN as u64 * (1 + r.below(sel_pct_x10)) / 1000) as i64;
            let lo = r.below((DOMAIN - w) as u64) as i64;
            (lo, lo + w)
        })
        .collect()
}

// ---------------- checks ----------------
fn check_zone_maps_and_imprints(r: &mut Rng) {
    println!(
        "pruning by data layout (1% selectivity queries, 200 per dataset); % of values actually read:"
    );
    println!(
        "{:>18} {:>10} {:>10} {:>11} {:>11}",
        "dataset", "zone map", "imprints", "zm pruned%", "imp pruned%"
    );
    let mut table = BTreeMap::new();
    for (name, col) in datasets(r) {
        let zm = ZoneMap::build(&col);
        let im = Imprints::build(&col);
        let (mut zs, mut is) = (ZStats::default(), ZStats::default());
        for (lo, hi) in range_queries(r, 200, 10) {
            let want = scan(&col, lo, hi);
            assert_eq!(
                zm.query(&col, lo, hi, &mut zs),
                want,
                "zone map wrong on {name}"
            );
            assert_eq!(
                im.query(&col, lo, hi, &mut is),
                want,
                "imprints wrong on {name}"
            );
        }
        let total = (200 * col.len()) as f64;
        let (zr, ir) = (
            zs.values_read as f64 / total * 100.0,
            is.values_read as f64 / total * 100.0,
        );
        let (zp, ip) = (
            zs.pruned as f64 / zs.blocks as f64 * 100.0,
            is.pruned as f64 / is.blocks as f64 * 100.0,
        );
        println!("{name:>18} {zr:>9.1}% {ir:>9.1}% {zp:>10.1}% {ip:>10.1}%");
        table.insert(name, (zr, ir));
    }
    let (zm_bytes, im_bytes) = (N / ZBLOCK * 16, N / IBLOCK * 8);
    println!(
        "metadata size: zone map {zm_bytes} bytes, imprints {im_bytes} bytes, column {} bytes",
        N * 8
    );
    assert!(
        table["sorted"].0 < 3.0,
        "sorted data: zone maps should read ~1% of values"
    );
    assert!(
        table["random"].0 > 90.0,
        "random data: every block spans the domain, zone maps cannot prune"
    );
    assert!(
        table["batch-clustered"].0 < 10.0,
        "clustered batches keep ranges narrow"
    );
    assert!(
        table["two-window blocks"].0 > 90.0 && table["two-window blocks"].1 < 30.0,
        "imprints must see the hole that min/max cannot"
    );
    assert!(
        table["nearly sorted"].0 > table["sorted"].0,
        "outliers wreck min/max pruning"
    );
}

fn check_cracking(r: &mut Rng) {
    let col: Vec<i64> = (0..N).map(|_| r.below(DOMAIN as u64) as i64).collect();
    let queries = range_queries(r, 2000, 50);
    let mut sorted = col.clone();
    sorted.sort_unstable();
    let lg = (N as f64).log2().ceil() as u64;
    let sort_cost = N as u64 * lg;
    let prefix: Vec<i64> = sorted
        .iter()
        .scan(0i64, |s, &v| {
            *s += v;
            Some(*s)
        })
        .collect();
    let mut cr = Cracker::new(&col);
    let mut cr2 = Cracker::new(&col); // identical queries, but always two crack-in-two passes (no crack-in-three)
    let (mut cum_scan, mut cum_sort, mut cum_crack) = (0u64, 0u64, 0u64);
    let mut per_query = vec![];
    let mut marks = vec![];
    for (i, &(lo, hi)) in queries.iter().enumerate() {
        let want = scan(&col, lo, hi);
        let before = cr.work;
        assert_eq!(cr.query(lo, hi, true), want, "cracking wrong at query {i}");
        assert_eq!(cr2.query(lo, hi, false), want);
        let (a, b) = (
            sorted.partition_point(|&v| v < lo),
            sorted.partition_point(|&v| v < hi),
        );
        assert_eq!(
            (
                (b - a) as u64,
                prefix.get(b.wrapping_sub(1)).copied().unwrap_or(0)
                    - prefix.get(a.wrapping_sub(1)).copied().unwrap_or(0)
            ),
            want,
            "sorted array wrong"
        );
        per_query.push(cr.work - before);
        cum_scan += N as u64;
        cum_sort += if i == 0 { sort_cost } else { 0 } + 2 * lg;
        cum_crack = cr.work + 2 * lg * (i as u64 + 1); // same lookup charge as binary search: two O(log) probes
        if i < 64 {
            cr.check_invariant();
        }
        if [0, 1, 4, 9, 19, 49, 99, 199, 499, 999, 1999].contains(&i) {
            marks.push((i + 1, cum_scan, cum_sort, cum_crack));
        }
    }
    cr.check_invariant();
    let mut a = col.clone();
    a.sort_unstable();
    let mut b = cr.col.clone();
    b.sort_unstable();
    assert_eq!(
        a, b,
        "the cracked column must be a permutation of the original"
    );

    println!(
        "\ncumulative work (values touched; sort modelled as n*ceil(log2 n) comparisons), {} queries of 0.1-5% width:",
        queries.len()
    );
    println!(
        "{:>8} {:>14} {:>14} {:>14}",
        "queries", "scan", "sort+bsearch", "cracking"
    );
    for (q, s, so, c) in &marks {
        println!("{q:>8} {s:>14} {so:>14} {c:>14}");
    }
    let crossover_scan = (1..=queries.len()).find(|&q| {
        let c: u64 = per_query[..q].iter().sum();
        c < (q * N) as u64
    });
    println!(
        "cracking first query: {} values touched (a scan is {N}); cracking undercuts scanning from query {:?}",
        per_query[0], crossover_scan
    );
    println!(
        "cracker index: {} pivots, largest remaining piece {} values; crack-in-three saved {} touches over always-two-pass",
        cr.idx.len(),
        cr.max_piece(),
        cr2.work as i64 - cr.work as i64
    );
    let win = |a: usize, b: usize| per_query[a..b].iter().sum::<u64>() as f64 / (b - a) as f64;
    let (w1, w2, w3, w4) = (win(0, 10), win(10, 100), win(100, 1000), win(1000, 2000));
    println!(
        "average values touched per query: first 10: {w1:.0}, 10-100: {w2:.0}, 100-1000: {w3:.0}, 1000-2000: {w4:.0}  (convergence)"
    );
    assert!(
        per_query[0] <= N as u64 && per_query[0] > N as u64 / 2,
        "first query costs about one scan"
    );
    assert!(
        crossover_scan.unwrap() <= 3,
        "cracking's cumulative cost must undercut scanning within 3 queries"
    );
    assert!(
        w1 > w2 && w2 > w3 && w3 > w4,
        "work per query must fall as the column gets cracked"
    );
    assert!(
        cum_crack < cum_scan / 20,
        "after 2000 queries cracking must have saved >20x vs scanning"
    );
    assert!(
        cr.work <= cr2.work,
        "crack-in-three never costs more than two passes"
    );

    // The OTHER crossover. Both sides pay the same per-query lookups, so cracking has spent more in total than "sort once" exactly
    // when its partitioning work exceeds the sort's comparisons. Random cracking is a lazy quicksort (~2 n ln n comparisons if run
    // to completion, about 1.39 n log2 n), so with enough distinct bounds it ends up paying MORE than an eager sort. Keep querying
    // a COPY of the workload's cracker until that happens.
    assert!(
        cr.work < sort_cost,
        "within {} queries cracking is still cheaper than sorting up front",
        queries.len()
    );
    let mut long = Cracker {
        col: cr.col.clone(),
        idx: cr.idx.clone(),
        work: cr.work,
    };
    let mut more = Rng(0x10C6);
    let mut total_queries = queries.len();
    while long.work <= sort_cost {
        let (lo, hi) = range_queries(&mut more, 1, 50)[0];
        let (a, b) = (
            sorted.partition_point(|&v| v < lo),
            sorted.partition_point(|&v| v < hi),
        );
        let sum_before = |i: usize| if i == 0 { 0 } else { prefix[i - 1] };
        assert_eq!(
            long.query(lo, hi, true),
            ((b - a) as u64, sum_before(b) - sum_before(a)),
            "cracking wrong at query {total_queries}"
        );
        total_queries += 1;
        assert!(
            total_queries <= 500_000,
            "no crossover with the sort within 500k queries"
        );
    }
    long.check_invariant();
    println!(
        "cracking's cumulative work overtakes sort+binary-search at query {total_queries} ({} pivots by then); before that, lazy wins",
        long.idx.len()
    );

    // sequential workload: bounds advance monotonically, each crack re-partitions the huge right-hand piece
    let step = (DOMAIN / 2000).max(1);
    let seq: Vec<(i64, i64)> = (0..2000)
        .map(|i| (i as i64 * step, i as i64 * step + step))
        .collect();
    let mut cs = Cracker::new(&col);
    for &(lo, hi) in &seq {
        assert_eq!(cs.query(lo, hi, true), scan(&col, lo, hi));
    }
    cs.check_invariant();
    let mut cr_rand = Cracker::new(&col);
    for &(lo, hi) in &range_queries(r, 2000, 50) {
        cr_rand.query(lo, hi, true);
    }
    println!(
        "sequential workload: {} values touched vs {} for random ranges of similar cost ({:.1}x): why stochastic cracking exists",
        cs.work,
        cr_rand.work,
        cs.work as f64 / cr_rand.work as f64
    );
    assert!(
        cs.work > 3 * cr_rand.work,
        "sequential bounds should be several times worse than random ones"
    );
}

fn check_edge_cases() {
    let col = vec![5, 1, 9, 3, 7, 3, 3, 8];
    let mut c = Cracker::new(&col);
    for (lo, hi) in [(3, 4), (0, 100), (4, 4), (10, 20), (3, 4), (-5, 2), (1, 9)] {
        assert_eq!(c.query(lo, hi, true), scan(&col, lo, hi), "[{lo},{hi})");
        c.check_invariant();
    }
    // zone map block with all-equal values and the empty range
    let flat = vec![7i64; 3000];
    let zm = ZoneMap::build(&flat);
    let mut st = ZStats::default();
    assert_eq!(zm.query(&flat, 7, 8, &mut st), (3000, 21000));
    assert_eq!(zm.query(&flat, 8, 9, &mut st), (0, 0));
    assert_eq!(
        st.values_read, 0,
        "all-equal blocks are decided from metadata alone"
    );
    println!("edge cases (duplicates, empty/out-of-domain ranges, constant blocks) pass");
}

fn main() {
    let mut r = Rng(0xC8AC_0843);
    check_edge_cases();
    check_zone_maps_and_imprints(&mut r);
    check_cracking(&mut r);
    println!("all checks passed");
}
