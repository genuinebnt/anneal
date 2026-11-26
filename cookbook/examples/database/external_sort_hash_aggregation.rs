//! RECIPE 704 (database): external merge sort, top-N, and hash aggregation that spills (partition, recurse on skew), with IO counted in pages.
//! LEVEL: advanced
//! Use in: query operators that must work when the data does not fit in memory (ORDER BY, GROUP BY, DISTINCT).   Run: cargo run --release --example external_sort_hash_aggregation
//! COMMON USES: the external sort behind ORDER BY and sort-merge join in PostgreSQL (tuplesort: quicksorted runs or
//! replacement selection historically, then a tape merge), spilling hash aggregation in PostgreSQL 13+, DuckDB and Spark,
//! LSM-tree compaction and MapReduce shuffle (both are k-way merges of sorted runs), and top-N (`ORDER BY .. LIMIT n`) planned
//! as a bounded heap instead of a sort.
//! Course: CMU 15-445 Intro to Database Systems, Lecture 11 (External sorting, aggregations).
#![allow(dead_code)]
//!
//! MODEL: pages are 256 bytes; a tuple `(key u64, val i64)` is 16 bytes (16 per page), an aggregate state
//! `(key, count, sum, min, max)` is 40 bytes (6 per page). A memory budget is B pages. Every file access is counted in PAGES,
//! the unit the textbook cost formulas use. Files are `Vec`s; no real IO is needed to see the algorithms and their costs.
//! EXTERNAL MERGE SORT: pass 0 makes sorted RUNS of at most B pages (quicksort the buffer), then each merge pass merges up to
//! B-1 runs at a time (B-1 input buffers + 1 output buffer) with a heap-based k-way merge (a loser tree does about half the
//! comparisons; the IO is the same). Number of passes = 1 + ceil(log_{B-1}(ceil(N/B))), and each pass reads and writes all N
//! pages, so IO = 2N * passes (asserted exactly). REPLACEMENT SELECTION instead keeps a heap of B-2 pages: emit the smallest
//! key not below the last emitted one, else park the tuple for the next run; runs average 2x the memory on random input (fewer
//! runs, sometimes a pass less), one run for sorted input, and degrade to memory-sized runs on reverse-sorted input.
//! DOUBLE BUFFERING prefetches the next page of every input run while the CPU merges the current one, hiding IO latency
//! (we count blocking "exposed" reads) at the price of halving the fan-in, hence possibly more passes.
//! TOP-N: `ORDER BY key LIMIT n` keeps a bounded max-heap of n tuples: one sequential read of the input and no writes.
//! HASH AGGREGATION with a memory budget of (B-1)*6 groups: PHASE 1 partitions by hash into B-1 partition files, PHASE 2
//! aggregates each partition in memory with a different hash; a partition that still has too many distinct groups is
//! RE-PARTITIONED recursively with a new hash seed (skew), and after a depth cap it falls back to SORT-BASED aggregation
//! (always terminates, even if the hash is adversarial). EARLY/PARTIAL AGGREGATION keeps a table during phase 1 and spills
//! partial states only when it fills, so if the groups fit in memory nothing is written at all, and duplicates are combined
//! before they hit the disk. SORT-BASED aggregation sorts then combines neighbours and delivers sorted output for free.
//!
//! VERIFIED: 300 random relations (uniform, skewed, single hot key, all-equal, sorted, reverse, all-distinct; sizes 0..3000;
//! budgets of 3 to 32 pages) are sorted by 4 variants and compared with `sort`; the passes formula and the exact IO are
//! asserted; top-N equals the sort prefix; COUNT/SUM/MIN/MAX/AVG from grace-hash, early-hash and sort aggregation equal a
//! `HashMap` oracle, as do both DISTINCT variants. Targeted cases assert replacement-selection run lengths (about 2M on random
//! data, 1 run sorted, M reverse), forced recursion and forced sort fallback, and that early aggregation does zero spill IO
//! when the groups fit.
//!
//! Read: https://15445.courses.cs.cmu.edu/fall2024/slides/11-sorting.pdf (CMU 15-445 sorting and aggregation lecture),
//! https://pages.cs.wisc.edu/~dbbook/ (Ramakrishnan & Gehrke, Database Management Systems: external sorting chapter) and
//! https://15445.courses.cs.cmu.edu/fall2024/notes/11-sorting.pdf (the lecture notes; Graefe, "Implementing sorting in database systems", ACM Computing Surveys 2006, is the survey)
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap};

const PAGE_BYTES: usize = 256;
const TUPLE_BYTES: usize = 16;
const STATE_BYTES: usize = 40;
const PER_PAGE: usize = PAGE_BYTES / TUPLE_BYTES;
const STATES_PER_PAGE: usize = PAGE_BYTES / STATE_BYTES;

type Tuple = (u64, i64);

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n.max(1)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}
fn hash(key: u64, seed: u64) -> u64 {
    let mut x = key
        ^ seed
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(0x1234_5678);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn pages(items: usize, bytes: usize) -> u64 {
    (items * bytes).div_ceil(PAGE_BYTES) as u64
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
struct Io {
    reads: u64,
    writes: u64,
    exposed: u64, // reads the merge had to wait for (not hidden by prefetching)
}
impl Io {
    fn total(&self) -> u64 {
        self.reads + self.writes
    }
    fn add(&mut self, o: &Io) {
        self.reads += o.reads;
        self.writes += o.writes;
        self.exposed += o.exposed;
    }
}

// ======================= external merge sort =======================
#[derive(Clone, Copy, PartialEq, Debug)]
enum RunGen {
    QuickChunks,
    ReplacementSelection,
}

struct Sorted<T> {
    out: Vec<T>,
    passes: u32,
    runs0: usize,
    run_lens: Vec<usize>,
    fanin: usize,
    io: Io,
}

fn replacement_selection<T: Ord + Clone>(input: &[T], m: usize) -> Vec<Vec<T>> {
    let m = m.max(1);
    let mut heap: BinaryHeap<Reverse<(usize, T)>> = input
        .iter()
        .take(m)
        .map(|x| Reverse((0, x.clone())))
        .collect();
    let mut rest = input.iter().skip(m);
    let mut runs: Vec<Vec<T>> = Vec::new();
    while let Some(Reverse((r, x))) = heap.pop() {
        if runs.len() <= r {
            runs.push(Vec::new()); // the heap's smallest tuple belongs to a newer run: the old run is complete
        }
        if let Some(next) = rest.next() {
            // A tuple smaller than what we just emitted cannot join the current run: park it for the next one.
            let nr = if next >= &x { r } else { r + 1 };
            heap.push(Reverse((nr, next.clone())));
        }
        runs[r].push(x);
    }
    runs
}

fn kway_merge<T: Ord + Clone>(runs: &[Vec<T>]) -> Vec<T> {
    let mut heap: BinaryHeap<Reverse<(T, usize, usize)>> = BinaryHeap::new();
    for (i, r) in runs.iter().enumerate() {
        if let Some(x) = r.first() {
            heap.push(Reverse((x.clone(), i, 0)));
        }
    }
    let mut out = Vec::with_capacity(runs.iter().map(Vec::len).sum());
    while let Some(Reverse((x, i, p))) = heap.pop() {
        out.push(x);
        if let Some(n) = runs[i].get(p + 1) {
            heap.push(Reverse((n.clone(), i, p + 1)));
        }
    }
    out
}

fn merge_fanin(b: usize, double_buffered: bool) -> usize {
    assert!(b >= 3, "need at least 2 inputs + 1 output buffer");
    if double_buffered {
        ((b - 1) / 2).max(2)
    } else {
        b - 1
    }
}

/// Textbook pass count: 1 + ceil(log_fanin(ceil(N/B))), computed independently of the simulation.
fn expected_passes(n_pages: u64, b: usize, fanin: usize) -> u32 {
    let runs = n_pages.div_ceil(b as u64).max(1);
    let (mut p, mut cap) = (0u32, 1u64);
    while cap < runs {
        cap *= fanin as u64;
        p += 1;
    }
    1 + p
}

fn external_sort<T: Ord + Clone>(
    items: &[T],
    bytes: usize,
    b: usize,
    strat: RunGen,
    double_buffered: bool,
) -> Sorted<T> {
    let per_page = PAGE_BYTES / bytes;
    let fanin = merge_fanin(b, double_buffered);
    let mut io = Io {
        reads: pages(items.len(), bytes),
        ..Io::default()
    };
    let mut runs: Vec<Vec<T>> = match strat {
        RunGen::QuickChunks => items
            .chunks((b * per_page).max(1))
            .map(|c| {
                let mut v = c.to_vec();
                v.sort();
                v
            })
            .collect(),
        RunGen::ReplacementSelection => replacement_selection(items, (b - 2) * per_page),
    };
    io.writes += runs.iter().map(|r| pages(r.len(), bytes)).sum::<u64>();
    let (runs0, run_lens) = (runs.len(), runs.iter().map(Vec::len).collect());
    let mut passes = 1;
    while runs.len() > 1 {
        let mut next = Vec::new();
        for group in runs.chunks(fanin) {
            io.reads += group.iter().map(|r| pages(r.len(), bytes)).sum::<u64>();
            io.exposed += if double_buffered {
                group.len() as u64
            } else {
                group.iter().map(|r| pages(r.len(), bytes)).sum()
            };
            let merged = kway_merge(group);
            io.writes += pages(merged.len(), bytes);
            next.push(merged);
        }
        runs = next;
        passes += 1;
    }
    Sorted {
        out: runs.pop().unwrap_or_default(),
        passes,
        runs0,
        run_lens,
        fanin,
        io,
    }
}

/// ORDER BY key LIMIT n with a bounded max-heap: sequential read, no spill.
fn top_n<T: Ord + Clone>(items: &[T], n: usize) -> Vec<T> {
    let mut heap: BinaryHeap<T> = BinaryHeap::new();
    for x in items {
        if heap.len() < n {
            heap.push(x.clone());
        } else if n > 0 && x < heap.peek().unwrap() {
            heap.pop();
            heap.push(x.clone());
        }
    }
    heap.into_sorted_vec()
}

// ======================= aggregation =======================
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AggState {
    key: u64,
    count: u64,
    sum: i64,
    min: i64,
    max: i64,
}
impl AggState {
    fn of(t: Tuple) -> AggState {
        AggState {
            key: t.0,
            count: 1,
            sum: t.1,
            min: t.1,
            max: t.1,
        }
    }
    fn combine(&mut self, o: &AggState) {
        assert_eq!(self.key, o.key);
        self.count += o.count;
        self.sum += o.sum;
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
    }
    fn avg(&self) -> f64 {
        self.sum as f64 / self.count as f64
    }
}

#[derive(Default, Debug, Clone)]
struct AggStats {
    io: Io,
    spilled: bool,
    repartitions: u64,
    sort_fallbacks: u64,
    max_depth: usize,
}

#[derive(Clone, Copy)]
struct AggCfg {
    b: usize,
    early: bool,
    max_depth: usize,
}
impl AggCfg {
    fn group_capacity(&self) -> usize {
        (self.b - 1) * STATES_PER_PAGE
    }
    fn fanout(&self) -> usize {
        self.b - 1
    }
}

fn spill(
    states: Vec<AggState>,
    bytes: usize,
    seed: u64,
    cfg: &AggCfg,
    st: &mut AggStats,
) -> Vec<Vec<AggState>> {
    let mut parts: Vec<Vec<AggState>> = vec![Vec::new(); cfg.fanout()];
    for s in states {
        parts[(hash(s.key, seed) % cfg.fanout() as u64) as usize].push(s);
    }
    st.io.writes += parts.iter().map(|p| pages(p.len(), bytes)).sum::<u64>();
    parts
}

fn process_partition(
    states: Vec<AggState>,
    bytes: usize,
    depth: usize,
    cfg: &AggCfg,
    st: &mut AggStats,
    out: &mut Vec<AggState>,
) {
    st.max_depth = st.max_depth.max(depth);
    st.io.reads += pages(states.len(), bytes);
    let cap = cfg.group_capacity();
    let mut table: HashMap<u64, AggState> = HashMap::new();
    let mut fits = true;
    for s in &states {
        if let Some(e) = table.get_mut(&s.key) {
            e.combine(s);
        } else if table.len() < cap {
            table.insert(s.key, *s);
        } else {
            fits = false; // too many distinct groups for the budget
            break;
        }
    }
    if fits {
        out.extend(table.into_values());
    } else if depth >= cfg.max_depth {
        // Fallback that always works: sort the partition externally, combine neighbours in one streaming pass.
        st.sort_fallbacks += 1;
        let sorted = external_sort(&states, STATE_BYTES, cfg.b, RunGen::QuickChunks, false);
        st.io.add(&sorted.io);
        for s in sorted.out {
            match out.last_mut() {
                Some(l) if l.key == s.key => l.combine(&s), // keys are disjoint across partitions
                _ => out.push(s),
            }
        }
    } else {
        st.repartitions += 1;
        let parts = spill(states, bytes, depth as u64 + 1, cfg, st);
        for p in parts.into_iter().filter(|p| !p.is_empty()) {
            process_partition(p, bytes, depth + 1, cfg, st, out);
        }
    }
}
fn hash_aggregate(rel: &[Tuple], cfg: AggCfg) -> (Vec<AggState>, AggStats) {
    let mut st = AggStats::default();
    st.io.reads += pages(rel.len(), TUPLE_BYTES);
    let mut out = Vec::new();
    let cap = cfg.group_capacity();
    let (parts, bytes) = if cfg.early {
        let mut table: HashMap<u64, AggState> = HashMap::new();
        let mut spilled: Vec<Vec<AggState>> = vec![Vec::new(); cfg.fanout()];
        let flush = |table: &mut HashMap<u64, AggState>, spilled: &mut Vec<Vec<AggState>>| {
            for (_, s) in table.drain() {
                spilled[(hash(s.key, 0) % cfg.fanout() as u64) as usize].push(s);
            }
        };
        for &t in rel {
            let s = AggState::of(t);
            if let Some(e) = table.get_mut(&t.0) {
                e.combine(&s);
            } else {
                if table.len() == cap {
                    st.spilled = true;
                    flush(&mut table, &mut spilled); // partial aggregates go to disk, the table starts over
                }
                table.insert(t.0, s);
            }
        }
        if !st.spilled {
            return (table.into_values().collect(), st); // everything fit: zero spill IO
        }
        flush(&mut table, &mut spilled);
        st.io.writes += spilled
            .iter()
            .map(|p| pages(p.len(), STATE_BYTES))
            .sum::<u64>();
        (spilled, STATE_BYTES)
    } else {
        st.spilled = true;
        let states: Vec<AggState> = rel.iter().map(|&t| AggState::of(t)).collect();
        (spill(states, TUPLE_BYTES, 0, &cfg, &mut st), TUPLE_BYTES)
    };
    for p in parts.into_iter().filter(|p| !p.is_empty()) {
        process_partition(p, bytes, 1, &cfg, &mut st, &mut out);
    }
    (out, st)
}

/// Sort-based aggregation: external sort on (key, val), then one streaming pass combining equal keys.
fn sort_aggregate(rel: &[Tuple], b: usize) -> (Vec<AggState>, Io) {
    let sorted = external_sort(rel, TUPLE_BYTES, b, RunGen::QuickChunks, false);
    let mut out: Vec<AggState> = Vec::new();
    for &t in &sorted.out {
        match out.last_mut() {
            Some(l) if l.key == t.0 => l.combine(&AggState::of(t)),
            _ => out.push(AggState::of(t)),
        }
    }
    (out, sorted.io)
}

fn distinct_hash(keys: &[u64], b: usize) -> Vec<u64> {
    let rel: Vec<Tuple> = keys.iter().map(|&k| (k, 0)).collect();
    let (st, _) = hash_aggregate(
        &rel,
        AggCfg {
            b,
            early: true,
            max_depth: 12,
        },
    );
    let mut v: Vec<u64> = st.into_iter().map(|s| s.key).collect();
    v.sort();
    v
}
fn distinct_sort(keys: &[u64], b: usize) -> Vec<u64> {
    let mut v = external_sort(keys, 8, b, RunGen::QuickChunks, false).out;
    v.dedup();
    v
}

fn oracle(rel: &[Tuple]) -> Vec<AggState> {
    let mut m: HashMap<u64, AggState> = HashMap::new();
    for &t in rel {
        match m.get_mut(&t.0) {
            Some(e) => e.combine(&AggState::of(t)),
            None => {
                m.insert(t.0, AggState::of(t));
            }
        }
    }
    let mut v: Vec<AggState> = m.into_values().collect();
    v.sort();
    v
}
fn sorted_states(mut v: Vec<AggState>) -> Vec<AggState> {
    v.sort();
    v
}

// ======================= data =======================
fn gen_relation(rng: &mut Rng, n: usize, kind: usize) -> Vec<Tuple> {
    let d = 1 + rng.below(n.max(1));
    let mut rel: Vec<Tuple> = (0..n)
        .map(|i| {
            let key = match kind {
                0 => rng.below(d) as u64,
                1 => (d as f64 * rng.unit().powi(4)) as u64, // heavy skew toward small keys
                2 => {
                    if rng.below(10) < 9 {
                        7
                    } else {
                        rng.below(d) as u64
                    }
                }
                3 => 42,
                4 | 5 => rng.below(d) as u64,
                _ => i as u64 * 7 + 1, // all distinct
            };
            (key, rng.below(2001) as i64 - 1000)
        })
        .collect();
    match kind {
        4 => rel.sort(),
        5 => rel.sort_by(|a, b| b.cmp(a)),
        _ => {}
    }
    rel
}

fn check_relation(rel: &[Tuple], b: usize, rng: &mut Rng) {
    let mut expect = rel.to_vec();
    expect.sort();
    let n_pages = pages(rel.len(), TUPLE_BYTES);
    // --- sorting: four variants must equal std sort ---
    for (strat, db) in [
        (RunGen::QuickChunks, false),
        (RunGen::QuickChunks, true),
        (RunGen::ReplacementSelection, false),
        (RunGen::ReplacementSelection, true),
    ] {
        let s = external_sort(rel, TUPLE_BYTES, b, strat, db);
        assert_eq!(s.out, expect, "{strat:?} db={db} b={b} n={}", rel.len());
        if strat == RunGen::QuickChunks {
            assert_eq!(
                s.passes,
                expected_passes(n_pages, b, s.fanin),
                "pass count formula (b={b}, pages={n_pages}, db={db})"
            );
            if rel.len() % PER_PAGE == 0 && !rel.is_empty() {
                assert_eq!(
                    s.io.total(),
                    2 * n_pages * s.passes as u64,
                    "IO = 2N x passes"
                );
            }
        }
        if strat == RunGen::ReplacementSelection && !rel.is_empty() {
            assert!(
                s.runs0 <= rel.len().div_ceil(((b - 2) * PER_PAGE).max(1)).max(1),
                "RS runs are never shorter than memory"
            );
        }
    }
    // --- top-N ---
    let n = rng.below(40);
    assert_eq!(
        top_n(rel, n),
        expect.iter().take(n).cloned().collect::<Vec<_>>()
    );
    // --- aggregation: grace, early, sort-based, tiny depth cap to exercise the fallback ---
    let want = oracle(rel);
    for cfg in [
        AggCfg {
            b,
            early: false,
            max_depth: 12,
        },
        AggCfg {
            b,
            early: true,
            max_depth: 12,
        },
        AggCfg {
            b,
            early: true,
            max_depth: 1,
        },
        AggCfg {
            b,
            early: false,
            max_depth: 2,
        },
    ] {
        let (got, _) = hash_aggregate(rel, cfg);
        assert_eq!(
            sorted_states(got),
            want,
            "hash aggregate early={} b={b} depth={}",
            cfg.early,
            cfg.max_depth
        );
    }
    let (got, _) = sort_aggregate(rel, b);
    assert_eq!(got, want, "sort aggregate (also sorted by key)");
    for s in &want {
        assert!((s.avg() - s.sum as f64 / s.count as f64).abs() < 1e-12 && s.min <= s.max);
    }
    // --- distinct ---
    let keys: Vec<u64> = rel.iter().map(|t| t.0).collect();
    let truth: Vec<u64> = keys
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(distinct_hash(&keys, b), truth);
    assert_eq!(distinct_sort(&keys, b), truth);
}

fn main() {
    // ---------- 1. 300 random relations ----------
    let mut rng = Rng(0x0e57_0001);
    let budgets = [3usize, 4, 5, 6, 8, 16, 32];
    let kinds = [
        "uniform",
        "zipf-ish",
        "hot key",
        "all equal",
        "sorted",
        "reverse",
        "all distinct",
    ];
    let mut seen = [0usize; 7];
    for i in 0..300 {
        let n = match i % 10 {
            0 => 0,
            1 => 1,
            _ => rng.below(3000),
        };
        let kind = i % 7;
        let b = budgets[rng.below(budgets.len())];
        let rel = gen_relation(&mut rng, n, kind);
        check_relation(&rel, b, &mut rng);
        seen[kind] += 1;
    }
    println!(
        "300 random relations OK: sorts (4 variants) vs std sort, pass formula, IO = 2N x passes, top-N, 4 aggregation configs vs HashMap, DISTINCT"
    );
    println!(
        "  relation kinds exercised: {}",
        kinds
            .iter()
            .zip(&seen)
            .map(|(k, c)| format!("{k}={c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // ---------- 2. sort cost table ----------
    let big: Vec<Tuple> = (0..16_000)
        .map(|_| (rng.next() >> 20, rng.below(100) as i64))
        .collect();
    let mut expect = big.clone();
    expect.sort();
    let n_pages = pages(big.len(), TUPLE_BYTES);
    println!(
        "\nexternal sort of {} tuples = N = {n_pages} pages: passes and IO (pages) by memory budget B",
        big.len()
    );
    println!(
        "{:>5} {:>9} {:>7} {:>9} | {:>9} {:>7} {:>9} {:>9} | {:>8} {:>7} {:>9}",
        "B",
        "chunk run",
        "passes",
        "IO",
        "dbl-buf",
        "passes",
        "IO",
        "exposed",
        "RS runs",
        "passes",
        "IO"
    );
    for b in [3usize, 5, 10, 35, 100, 1000] {
        let a = external_sort(&big, TUPLE_BYTES, b, RunGen::QuickChunks, false);
        let d = external_sort(&big, TUPLE_BYTES, b, RunGen::QuickChunks, true);
        let r = external_sort(&big, TUPLE_BYTES, b, RunGen::ReplacementSelection, false);
        assert!(a.out == expect && d.out == expect && r.out == expect);
        assert_eq!(a.passes, expected_passes(n_pages, b, b - 1));
        assert_eq!(a.io.total(), 2 * n_pages * a.passes as u64);
        println!(
            "{b:>5} {:>9} {:>7} {:>9} | {:>9} {:>7} {:>9} {:>9} | {:>8} {:>7} {:>9}",
            a.runs0,
            a.passes,
            a.io.total(),
            d.runs0,
            d.passes,
            d.io.total(),
            d.io.exposed,
            r.runs0,
            r.passes,
            r.io.total()
        );
        if a.passes > 1 && b >= 10 {
            assert!(
                d.io.exposed < a.io.exposed / 3,
                "prefetching hides most blocking reads"
            );
        }
        // RS only gets B-2 pages of heap (chunks use all B), so it can have more runs at tiny B; never more than memory-sized.
        assert!(r.runs0 <= big.len().div_ceil((b - 2) * PER_PAGE));
    }
    let one = external_sort(&big, TUPLE_BYTES, 1000, RunGen::QuickChunks, false);
    assert_eq!(
        one.passes, 1,
        "if the data fits in memory it is a single pass: read + write once"
    );

    // ---------- 3. replacement selection run lengths ----------
    let b = 18;
    let m = (b - 2) * PER_PAGE;
    let rand: Vec<Tuple> = (0..40_000).map(|_| (rng.next(), 0)).collect();
    let rs = external_sort(&rand, TUPLE_BYTES, b, RunGen::ReplacementSelection, false);
    let body = &rs.run_lens[..rs.run_lens.len() - 1];
    let avg = body.iter().sum::<usize>() as f64 / body.len() as f64;
    let chunk = external_sort(&rand, TUPLE_BYTES, b, RunGen::QuickChunks, false);
    println!(
        "\nreplacement selection, heap of {m} tuples: {} runs of average length {avg:.0} = {:.2} x memory (quicksorted chunks: {} runs)",
        rs.runs0,
        avg / m as f64,
        chunk.runs0
    );
    assert!(
        (1.7..2.3).contains(&(avg / m as f64)),
        "knuth: expected run length is 2x memory on random input"
    );
    assert!(rs.runs0 < chunk.runs0);
    let mut sorted = rand.clone();
    sorted.sort();
    assert_eq!(
        external_sort(&sorted, TUPLE_BYTES, b, RunGen::ReplacementSelection, false).runs0,
        1,
        "sorted input: a single run"
    );
    let mut rev = sorted.clone();
    rev.reverse();
    let rr = external_sort(&rev, TUPLE_BYTES, b, RunGen::ReplacementSelection, false);
    assert!(
        rr.run_lens[..rr.run_lens.len() - 1].iter().all(|&l| l == m),
        "reverse input: runs exactly the size of memory"
    );
    println!(
        "  sorted input -> 1 run; reverse-sorted input -> {} runs of exactly {m} (worst case)",
        rr.runs0
    );

    // ---------- 4. top-N vs full sort ----------
    let sort_io = external_sort(&big, TUPLE_BYTES, 10, RunGen::QuickChunks, false)
        .io
        .total();
    let heap_io = n_pages;
    assert_eq!(top_n(&big, 25), expect[..25].to_vec());
    println!(
        "\ntop-25 of {n_pages} pages with B=10: bounded heap {heap_io} page reads, 0 writes; full sort + limit {sort_io} page IOs ({:.0}x)",
        sort_io as f64 / heap_io as f64
    );
    assert!(heap_io * 4 < sort_io);

    // ---------- 5. aggregation: sort vs hash comparison ----------
    let b = 16;
    let n = 20_000;
    let cap = (b - 1) * STATES_PER_PAGE;
    println!(
        "\nGROUP BY over {n} tuples ({} pages), B={b} pages -> hash table holds {cap} groups; IO in pages",
        pages(n, TUPLE_BYTES)
    );
    println!(
        "{:>9} | {:>10} {:>7} | {:>10} {:>6} | {:>10} {:>6} {:>7} {:>9}",
        "groups",
        "sort-agg",
        "passes",
        "grace-hash",
        "recur",
        "early-hash",
        "recur",
        "spilled",
        "max depth"
    );
    let mut results = Vec::new();
    for d in [10usize, 80, 500, 5_000, 20_000] {
        let rel: Vec<Tuple> = (0..n)
            .map(|i| {
                (
                    if d == 20_000 {
                        i as u64
                    } else {
                        rng.below(d) as u64
                    },
                    rng.below(100) as i64,
                )
            })
            .collect();
        let want = oracle(&rel);
        let (s, sio) = sort_aggregate(&rel, b);
        let (g, gs) = hash_aggregate(
            &rel,
            AggCfg {
                b,
                early: false,
                max_depth: 12,
            },
        );
        let (e, es) = hash_aggregate(
            &rel,
            AggCfg {
                b,
                early: true,
                max_depth: 12,
            },
        );
        assert_eq!(s, want);
        assert_eq!(sorted_states(g), want);
        assert_eq!(sorted_states(e), want);
        println!(
            "{:>9} | {:>10} {:>7} | {:>10} {:>6} | {:>10} {:>6} {:>7} {:>9}",
            want.len(),
            sio.total(),
            expected_passes(pages(n, TUPLE_BYTES), b, b - 1),
            gs.io.total(),
            gs.repartitions,
            es.io.total(),
            es.repartitions,
            es.spilled,
            es.max_depth
        );
        results.push((want.len(), sio, gs, es));
    }
    let n_pg = pages(n, TUPLE_BYTES);
    let (few, mid) = (&results[0], &results[2]);
    assert!(
        !few.3.spilled && few.3.io.total() == n_pg,
        "groups fit in memory: early aggregation = one sequential read, nothing spilled"
    );
    assert!(
        few.3.io.total() * 4 < few.1.total(),
        "so hashing beats sorting by a wide margin when groups are few"
    );
    let f = (b - 1) as u64;
    assert!(
        mid.2.repartitions == 0 && (3 * n_pg..=3 * n_pg + 2 * f).contains(&mid.2.io.total()),
        "grace hash without recursion = read + write + read = 3N (+ page rounding): {}",
        mid.2.io.total()
    );
    assert!(
        results[0].3.io.total() < results[0].2.io.total(),
        "early aggregation avoids the spill that grace hashing always pays"
    );
    assert!(results[4].3.spilled);

    // ---------- 6. skew, recursion and the sort fallback ----------
    let rel: Vec<Tuple> = (0..6000).map(|i| ((i * 7 % 3000) as u64, 1)).collect(); // 3000 groups, 12-group budget at B=3
    let want = oracle(&rel);
    let (g, st) = hash_aggregate(
        &rel,
        AggCfg {
            b: 3,
            early: false,
            max_depth: 14,
        },
    );
    assert_eq!(sorted_states(g), want);
    assert!(st.repartitions > 10 && st.max_depth >= 5);
    println!(
        "\nskew/tiny budget: 3000 groups in 12-group memory (B=3): {} recursive re-partitions, max depth {}, {} page IOs",
        st.repartitions,
        st.max_depth,
        st.io.total()
    );
    let (g2, st2) = hash_aggregate(
        &rel,
        AggCfg {
            b: 3,
            early: false,
            max_depth: 2,
        },
    );
    assert_eq!(sorted_states(g2), want);
    assert!(st2.sort_fallbacks > 0);
    println!(
        "depth cap 2: {} partitions fell back to sort-based aggregation, same answer, {} page IOs",
        st2.sort_fallbacks,
        st2.io.total()
    );

    // ---------- 7. aggregates: a worked example ----------
    let mini: Vec<Tuple> = vec![(1, 10), (2, -5), (1, 30), (2, 15), (3, 7), (1, -4)];
    let (agg, _) = sort_aggregate(&mini, 3);
    assert_eq!(
        agg[0],
        AggState {
            key: 1,
            count: 3,
            sum: 36,
            min: -4,
            max: 30
        }
    );
    assert!((agg[0].avg() - 12.0).abs() < 1e-12);
    let m2: BTreeMap<u64, (u64, i64)> = agg.iter().map(|s| (s.key, (s.count, s.sum))).collect();
    println!("\nSELECT key, COUNT(*), SUM(v), MIN(v), MAX(v), AVG(v) GROUP BY key:");
    for s in &agg {
        println!(
            "  key {}: count {} sum {} min {} max {} avg {:.2}",
            s.key,
            s.count,
            s.sum,
            s.min,
            s.max,
            s.avg()
        );
    }
    assert_eq!(m2[&2], (2, 10));
    println!("\nall external sort / hash aggregation checks passed");
}
