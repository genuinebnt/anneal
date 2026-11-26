//! RECIPE 764 (database): adaptive query processing at runtime: eddies, runtime filters (sideways information passing), join switching, skew re-partitioning, look-ahead information passing.
//! LEVEL: advanced
//! Use in: query executors that cannot trust optimizer estimates, stream processors, parallel engines suffering skew, star-schema join pipelines.   Run: cargo run --release --example adaptive_query_processing_eddies_sip
//! COMMON USES: eddies (Telegraph/CACQ research systems, continuous-query routing), runtime Bloom / min-max filters pushed to scans (Spark dynamic
//! partition pruning and Bloom filter joins, Impala and Trino runtime filters, DuckDB / Snowflake / SQL Server batch-mode Bloom bitmaps), adaptive join
//! switching (SQL Server adaptive joins, Oracle adaptive plans, Spark AQE broadcast switching), skew handling by splitting heavy keys (Spark AQE skew
//! join, parallel aggregation with partial pre-aggregation), look-ahead information passing in star-schema engines (Zhu et al.; Quickstep).
//! Course: CMU 15-799 Query Optimization (Spring 2025), lectures 17-18 (Adaptivity: Runtime Processing), plus CMU 15-721 adaptive query processing
#![allow(dead_code)]
//!
//! WHAT: five mechanisms that let an executor adapt WHILE the query runs; each is checked against a static reference and measured in work units.
//! * EDDY: a router sits between operators and chooses, PER TUPLE, which filter to visit next. Operators have a cost and a pass rate; an
//!   operator that drops many tuples cheaply should run first (rank = (1 - pass) / cost, the optimal order for independent filters).
//!   The eddy learns pass rates online (exponentially weighted) and draws the next operator by LOTTERY with weight rank^4 plus a small
//!   exploration probability, so it keeps sampling every operator and notices drift. A static plan fixed from initial statistics keeps the old
//!   order after the data changes; the eddy re-converges.
//! * SIDEWAYS INFORMATION PASSING / RUNTIME FILTERS: after the build side of a hash join is read, derive a min/max range and a Bloom filter of
//!   its keys and push them to the probe-side scan. Rows that cannot match are dropped before joining. Bloom filters have no false
//!   negatives, so results are identical; false positives only cost a wasted probe. Min/max helps for clustered keys, Bloom for scattered ones.
//! * ADAPTIVE JOIN SWITCH: start with index nested loop (cheap for few outer rows), and switch to a hash join once the nested-loop work so
//!   far equals the cost of building the hash table. This is the ski-rental rule: total work is at most 2x the best choice in hindsight,
//!   whatever the (unknown) outer cardinality turns out to be.
//! * SKEW RE-PARTITIONING: sample the first 10% of a stream, find heavy keys (share > 1/(8P)), and spread those across all workers
//!   (partial aggregates merge afterwards) while other keys stay hash-partitioned; the makespan (max worker load) drops.
//! * LOOK-AHEAD INFORMATION PASSING (Zhu et al.): in a star join, test every fact row against the Bloom filters of ALL dimensions before
//!   any join, ordering the filters adaptively by observed pass rate. Surviving rows are few, so the join order stops mattering: the plan becomes
//!   ROBUST to a bad order. A semi-join reduction prepass is the exact (no false positive) version of the same idea.
//!
//! WHY: optimizer estimates are wrong often enough (see the cardinality lab) that executors need self-correcting operators whose worst case is bounded.
//!
//! CHECKS (asserted): eddy and static output are identical to the reference; on drifting data the eddy beats the stale static order and ends within
//! 15% of the per-phase optimum, and on stable data it costs at most 25% more than the optimal static order; runtime filters never change join
//! results, Bloom never has false negatives and its false-positive rate stays under 3%; the adaptive join switch is within 2x of the hindsight best on 800
//! random workloads and returns identical results; skew re-partitioning keeps results equal and cuts the makespan; LIP shrinks the best-to-worst
//! join-order gap from >= 1.8x to <= 1.15x.
//! Read: Adaptivity: Runtime Processing, CMU 15-799 lecture 18 https://15799.courses.cs.cmu.edu/spring2025/schedule.html
//!       Adaptive Query Processing in the Looking Glass (Babu et al., CIDR 2015)
//!       Looking Ahead Makes Query Plans Robust (Zhu et al., VLDB 2017)
//!       Bloom filter https://en.wikipedia.org/wiki/Bloom_filter

use std::collections::{HashMap, HashSet};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

// ---------------------------------------------------------------------------------------------------------------------
// 1. Eddy
// ---------------------------------------------------------------------------------------------------------------------

const NOPS: usize = 4;
const COST: [f64; NOPS] = [1.0, 4.0, 2.0, 8.0];
const N: usize = 100_000;

/// Tuple attributes are generated so that operator i (`x_i < 0.5`) passes with probability `pass[i]` of the phase the tuple belongs to.
fn gen_stream(r: &mut Rng, phases: &[(usize, [f64; NOPS])]) -> Vec<[f64; NOPS]> {
    let mut out = vec![];
    for &(n, pass) in phases {
        for _ in 0..n {
            let mut t = [0.0; NOPS];
            for i in 0..NOPS {
                t[i] = r.unit() * 0.5 / pass[i]; // x < 0.5 exactly when the uniform draw is below pass[i]
            }
            out.push(t);
        }
    }
    out
}

fn rank(pass: f64, cost: f64) -> f64 {
    (1.0 - pass) / cost
}

fn best_order(pass: &[f64; NOPS]) -> Vec<usize> {
    let mut o: Vec<usize> = (0..NOPS).collect();
    o.sort_by(|&a, &b| {
        rank(pass[b], COST[b])
            .partial_cmp(&rank(pass[a], COST[a]))
            .unwrap()
    });
    o
}

enum Policy {
    Fixed(Vec<usize>),
    Eddy,
}

struct EddyState {
    pass: [f64; NOPS], // exponentially weighted pass-rate estimates
}

/// Route every tuple through the operators; returns (surviving tuple ids, total work, per-10k-tuples work).
fn run_stream(stream: &[[f64; NOPS]], policy: &Policy, seed: u64) -> (Vec<u32>, f64, Vec<f64>) {
    let mut r = Rng(seed);
    let mut st = EddyState { pass: [0.5; NOPS] };
    let (mut out, mut total, mut windows, mut win) = (vec![], 0.0, vec![], 0.0);
    for (id, t) in stream.iter().enumerate() {
        let mut remaining: Vec<usize> = (0..NOPS).collect();
        let mut alive = true;
        let fixed = if let Policy::Fixed(o) = policy {
            Some(o.clone())
        } else {
            None
        };
        let mut step = 0;
        while alive && !remaining.is_empty() {
            let op = match &fixed {
                Some(o) => o[step],
                None => {
                    // lottery: weight = (rank / best rank)^6, plus a small floor so that no operator starves (exploration)
                    let top = remaining
                        .iter()
                        .map(|&i| rank(st.pass[i], COST[i]))
                        .fold(1e-9, f64::max);
                    let w: Vec<f64> = remaining
                        .iter()
                        .map(|&i| (rank(st.pass[i], COST[i]).max(0.0) / top).powi(6) + 0.01)
                        .collect();
                    let mut x = r.unit() * w.iter().sum::<f64>();
                    let mut k = 0;
                    while k + 1 < w.len() && x >= w[k] {
                        x -= w[k];
                        k += 1;
                    }
                    remaining[k]
                }
            };
            remaining.retain(|&i| i != op);
            step += 1;
            let ok = t[op] < 0.5;
            total += COST[op];
            win += COST[op];
            st.pass[op] += 0.02 * ((ok as u8 as f64) - st.pass[op]); // learn the pass rate from what was observed
            alive = ok;
        }
        if alive {
            out.push(id as u32);
        }
        if (id + 1) % 10_000 == 0 {
            windows.push(win / 10_000.0);
            win = 0.0;
        }
    }
    (out, total, windows)
}

fn expected_work(order: &[usize], pass: &[f64; NOPS]) -> f64 {
    let (mut alive, mut w) = (1.0, 0.0);
    for &i in order {
        w += alive * COST[i];
        alive *= pass[i];
    }
    w
}

fn section_eddy() {
    println!(
        "== 1. eddy with lottery routing vs static order ({N} tuples, 4 filters with costs {COST:?}) =="
    );
    let p1 = [0.2, 0.8, 0.5, 0.9];
    let p2 = [0.9, 0.2, 0.8, 0.1]; // after the drift the cheap filter is useless and the expensive ones are selective
    let (o1, o2) = (best_order(&p1), best_order(&p2));
    println!(
        "  phase 1 pass rates {p1:?}, optimal order {o1:?} ({:.3} work/tuple)",
        expected_work(&o1, &p1)
    );
    println!(
        "  phase 2 pass rates {p2:?}, optimal order {o2:?} ({:.3} work/tuple)",
        expected_work(&o2, &p2)
    );
    for (name, phases) in [
        ("stable (phase 1 throughout)", vec![(N, p1)]),
        (
            "drifting (phase 2 from tuple 50000)",
            vec![(N / 2, p1), (N / 2, p2)],
        ),
    ] {
        let mut r = Rng(764);
        let stream = gen_stream(&mut r, &phases);
        let reference: Vec<u32> = (0..stream.len() as u32)
            .filter(|&i| stream[i as usize].iter().all(|&x| x < 0.5))
            .collect();
        let (os, ws, wins_s) = run_stream(&stream, &Policy::Fixed(o1.clone()), 1);
        let (oe, we, wins_e) = run_stream(&stream, &Policy::Eddy, 2);
        // Hindsight optimum: the best order of each phase, applied to that phase only (not realisable without foresight).
        let hind: f64 = phases
            .iter()
            .map(|&(n, p)| n as f64 * expected_work(&best_order(&p), &p))
            .sum();
        assert_eq!(os, reference, "static plan changed the result");
        assert_eq!(oe, reference, "eddy changed the result");
        println!(
            "  {name}: static {ws:.0}, eddy {we:.0}, hindsight optimum {hind:.0}   (eddy/static {:.2}, eddy/optimum {:.2})",
            we / ws,
            we / hind
        );
        println!(
            "    work per tuple by 10k window  static: {}",
            wins_s
                .iter()
                .map(|x| format!("{x:.2}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        println!(
            "                                  eddy:   {}",
            wins_e
                .iter()
                .map(|x| format!("{x:.2}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        if phases.len() == 1 {
            assert!(
                we <= hind * 1.25,
                "on stable data the eddy may cost at most 25% over the optimum (exploration tax)"
            );
        } else {
            assert!(
                we < ws,
                "the eddy must beat the stale static order after the drift"
            );
            let tail = wins_e[8..].iter().sum::<f64>() / 2.0;
            assert!(
                tail <= expected_work(&o2, &p2) * 1.15,
                "eddy must converge to within 15% of the phase-2 optimum, got {tail}"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// 2. Runtime filters: min/max and Bloom
// ---------------------------------------------------------------------------------------------------------------------

struct Bloom {
    bits: Vec<u64>,
    m: u64,
    k: u32,
}

impl Bloom {
    fn new(keys: usize, bits_per_key: usize) -> Bloom {
        let m = (keys * bits_per_key).max(64) as u64;
        Bloom {
            bits: vec![0; (m as usize + 63) / 64],
            m,
            k: 3,
        }
    }
    fn slots(&self, key: u64) -> impl Iterator<Item = u64> + '_ {
        let (h1, h2) = (mix(key), mix(key ^ 0xDEAD_BEEF) | 1);
        (0..self.k as u64).map(move |i| h1.wrapping_add(i.wrapping_mul(h2)) % self.m) // double hashing
    }
    fn add(&mut self, key: u64) {
        let s: Vec<u64> = self.slots(key).collect();
        s.into_iter()
            .for_each(|b| self.bits[(b / 64) as usize] |= 1 << (b % 64));
    }
    fn may_contain(&self, key: u64) -> bool {
        self.slots(key)
            .all(|b| self.bits[(b / 64) as usize] >> (b % 64) & 1 == 1)
    }
}

struct JoinRun {
    survivors: usize,
    work: f64,
    matches: usize,
    checksum: u64,
}

/// Hash join of fact keys against the build side; `pre` is an optional runtime filter applied at the scan (cost per row given).
fn hash_join(
    fact: &[u64],
    build: &HashSet<u64>,
    pre: Option<(&dyn Fn(u64) -> bool, f64)>,
) -> JoinRun {
    let mut run = JoinRun {
        survivors: 0,
        work: build.len() as f64,
        matches: 0,
        checksum: 0,
    }; // building costs 1 per build row
    for (rid, &k) in fact.iter().enumerate() {
        if let Some((f, c)) = &pre {
            run.work += c;
            if !f(k) {
                continue;
            }
        }
        run.survivors += 1;
        run.work += 1.0; // hash probe
        if build.contains(&k) {
            run.matches += 1;
            run.checksum = run
                .checksum
                .wrapping_add(mix(rid as u64 ^ k.rotate_left(32)));
        }
    }
    run
}

fn section_runtime_filters() {
    println!(
        "\n== 2. runtime filters (min/max, Bloom 10 bits/key) pushed to the probe scan: 200000 fact rows =="
    );
    let mut r = Rng(2);
    let fact: Vec<u64> = (0..200_000).map(|_| r.below(50_000)).collect();
    let scenarios: [(&str, Vec<u64>); 2] = [
        (
            "clustered build keys (a key range)",
            (10_000..12_000).collect(),
        ),
        ("scattered build keys (random subset)", {
            let mut s: HashSet<u64> = HashSet::new();
            while s.len() < 2000 {
                s.insert(r.below(50_000));
            }
            let mut v: Vec<u64> = s.into_iter().collect();
            v.sort();
            v
        }),
    ];
    println!(
        "  {:<38} {:<12} {:>10} {:>9} {:>10}",
        "scenario", "variant", "rows->join", "saved", "work"
    );
    for (name, keys) in &scenarios {
        let build: HashSet<u64> = keys.iter().copied().collect();
        let (lo, hi) = (*keys.iter().min().unwrap(), *keys.iter().max().unwrap());
        let mut bloom = Bloom::new(keys.len(), 10);
        keys.iter().for_each(|&k| bloom.add(k));
        let base = hash_join(&fact, &build, None);
        let mm = |k: u64| k >= lo && k <= hi;
        let bl = |k: u64| bloom.may_contain(k);
        let both = |k: u64| mm(k) && bl(k);
        let ex = |k: u64| build.contains(&k);
        let variants: Vec<(&str, JoinRun)> = vec![
            ("none", base),
            ("min/max", hash_join(&fact, &build, Some((&mm, 0.05)))),
            ("bloom", hash_join(&fact, &build, Some((&bl, 0.3)))),
            (
                "min/max+bloom",
                hash_join(&fact, &build, Some((&both, 0.3))),
            ),
            ("exact semijoin", hash_join(&fact, &build, Some((&ex, 1.0)))),
        ];
        let (m0, c0, s0) = (
            variants[0].1.matches,
            variants[0].1.checksum,
            variants[0].1.survivors,
        );
        for (v, run) in &variants {
            println!(
                "  {:<38} {:<12} {:>10} {:>8.1}% {:>10.0}",
                name,
                v,
                run.survivors,
                (1.0 - run.survivors as f64 / s0 as f64) * 100.0,
                run.work
            );
            assert_eq!(
                (run.matches, run.checksum),
                (m0, c0),
                "{v}: a runtime filter changed the join result"
            );
            assert!(
                run.survivors >= m0,
                "{v}: filter lost matching rows (false negative)"
            );
        }
        let fp = (variants[2].1.survivors - m0) as f64 / (fact.len() - m0) as f64;
        println!(
            "    bloom false-positive rate {:.2}% (theory for 10 bits/key, k=3: ~1.7%)",
            fp * 100.0
        );
        assert!(fp < 0.03);
        if name.starts_with("clustered") {
            assert!(
                variants[1].1.survivors < fact.len() / 15,
                "min/max must prune clustered keys"
            );
        } else {
            assert!(
                variants[1].1.survivors > fact.len() * 9 / 10,
                "min/max is useless for scattered keys"
            );
            assert!(
                variants[2].1.survivors < fact.len() / 15,
                "Bloom must prune scattered keys"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// 3. Adaptive join switch (ski rental)
// ---------------------------------------------------------------------------------------------------------------------

const INNER: usize = 20_000;
const INL_PER_ROW: f64 = 4.0;
const HASH_PER_ROW: f64 = 0.5;

enum Strategy {
    Inl,
    Hash,
    Adaptive,
}

/// Join `outer` keys with the inner table (key -> payload). Returns (matches, checksum, work, switched at outer row).
fn join_outer(
    inner: &[(u64, u64)],
    sorted_index: &[(u64, u64)],
    outer: &[u64],
    s: Strategy,
) -> (usize, u64, f64, Option<usize>) {
    let build_cost = INNER as f64;
    let (mut m, mut sum, mut work, mut switched) = (0usize, 0u64, 0.0, None);
    let mut ht: Option<HashMap<u64, u64>> = if let Strategy::Hash = s {
        work += build_cost;
        Some(inner.iter().copied().collect())
    } else {
        None
    };
    for (i, &k) in outer.iter().enumerate() {
        if ht.is_none() {
            if let Strategy::Adaptive = s {
                if work >= build_cost {
                    // Spent as much on nested-loop probes as a hash build costs: switch now (2-competitive).
                    work += build_cost;
                    ht = Some(inner.iter().copied().collect());
                    switched = Some(i);
                }
            }
        }
        let hit = match &ht {
            Some(h) => {
                work += HASH_PER_ROW;
                h.get(&k).copied()
            }
            None => {
                work += INL_PER_ROW; // one index descent per outer row
                sorted_index
                    .binary_search_by_key(&k, |e| e.0)
                    .ok()
                    .map(|p| sorted_index[p].1)
            }
        };
        if let Some(p) = hit {
            m += 1;
            sum = sum.wrapping_add(mix(p ^ (i as u64) << 20));
        }
    }
    (m, sum, work, switched)
}

fn section_switch() {
    println!(
        "\n== 3. adaptive join: index nested loop, switch to hash once the work spent equals the build cost =="
    );
    let mut r = Rng(3);
    let inner: Vec<(u64, u64)> = (0..INNER as u64).map(|k| (k * 3, r.next())).collect(); // keys 0,3,6,...
    let mut sorted = inner.clone();
    sorted.sort();
    let (mut worst, mut sum_ratio, mut switches) = (0.0f64, 0.0, 0);
    let (mut tot_inl, mut tot_hash, mut tot_ad) = (0.0, 0.0, 0.0);
    for _ in 0..800 {
        let n = (10f64.powf(1.0 + r.unit() * 4.3)) as usize; // 10 .. 200k outer rows, log-uniform
        let outer: Vec<u64> = (0..n).map(|_| r.below(INNER as u64 * 3)).collect();
        let a = join_outer(&inner, &sorted, &outer, Strategy::Inl);
        let b = join_outer(&inner, &sorted, &outer, Strategy::Hash);
        let c = join_outer(&inner, &sorted, &outer, Strategy::Adaptive);
        assert_eq!((a.0, a.1), (b.0, b.1), "INL and hash disagree");
        assert_eq!((a.0, a.1), (c.0, c.1), "adaptive join changed the result");
        let opt = a.2.min(b.2);
        worst = worst.max(c.2 / opt);
        sum_ratio += c.2 / opt;
        switches += c.3.is_some() as usize;
        (tot_inl, tot_hash, tot_ad) = (tot_inl + a.2, tot_hash + b.2, tot_ad + c.2);
    }
    println!(
        "  800 workloads, outer rows 10..200000 (break-even about {:.0} rows):",
        INNER as f64 / (INL_PER_ROW - HASH_PER_ROW)
    );
    println!(
        "  total work: always-INL {tot_inl:.0}, always-hash {tot_hash:.0}, adaptive {tot_ad:.0}"
    );
    println!(
        "  adaptive / hindsight-best: mean {:.3}, worst {worst:.3}; switched in {switches} workloads",
        sum_ratio / 800.0
    );
    assert!(worst <= 2.0 + 1e-9, "ski-rental bound violated: {worst}");
    assert!(
        tot_ad < tot_inl && tot_ad < tot_hash,
        "adaptive must beat both fixed strategies over a mixed workload"
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// 4. Skew re-partitioning
// ---------------------------------------------------------------------------------------------------------------------

const P: usize = 8;

fn route_hash(k: u64) -> usize {
    (mix(k) % P as u64) as usize
}

/// Partial COUNT(*) per key on P workers; returns (merged counts, per-worker load).
fn partition_count(stream: &[u64], adaptive: bool) -> (HashMap<u64, u64>, [usize; P]) {
    let sample = stream.len() / 10;
    let mut heavy: HashSet<u64> = HashSet::new();
    if adaptive {
        let mut c: HashMap<u64, usize> = HashMap::new();
        stream[..sample]
            .iter()
            .for_each(|&k| *c.entry(k).or_default() += 1);
        heavy = c
            .into_iter()
            .filter(|&(_, n)| n * 8 * P > sample)
            .map(|(k, _)| k)
            .collect();
    }
    let mut loads = [0usize; P];
    let mut partial: Vec<HashMap<u64, u64>> = vec![HashMap::new(); P];
    let mut rr = 0;
    for (i, &k) in stream.iter().enumerate() {
        let w = if i >= sample && heavy.contains(&k) {
            rr += 1;
            rr % P
        } else {
            route_hash(k)
        }; // heavy keys: round-robin
        loads[w] += 1;
        *partial[w].entry(k).or_default() += 1;
    }
    let mut merged: HashMap<u64, u64> = HashMap::new(); // merging partial aggregates is what makes splitting a key legal
    partial
        .iter()
        .flatten()
        .for_each(|(&k, &c)| *merged.entry(k).or_default() += c);
    (merged, loads)
}

fn imbalance(l: &[usize; P]) -> f64 {
    *l.iter().max().unwrap() as f64 / (l.iter().sum::<usize>() as f64 / P as f64)
}

fn section_skew() {
    println!(
        "\n== 4. skew mitigation: {P} workers, 400000 tuples, heavy keys spread after a 10% sample =="
    );
    let mut r = Rng(4);
    let mut acc = 0.0;
    let cdf: Vec<f64> = (1..=5000)
        .map(|k| {
            acc += 1.0 / (k as f64).powf(1.2);
            acc
        })
        .collect();
    let total = acc;
    for (name, skewed) in [("zipf(1.2) keys", true), ("uniform keys", false)] {
        let stream: Vec<u64> = (0..400_000)
            .map(|_| {
                if skewed {
                    let u = r.unit() * total;
                    cdf.partition_point(|&c| c < u) as u64
                } else {
                    r.below(5000)
                }
            })
            .collect();
        let mut reference: HashMap<u64, u64> = HashMap::new();
        stream
            .iter()
            .for_each(|&k| *reference.entry(k).or_default() += 1);
        let (m0, l0) = partition_count(&stream, false);
        let (m1, l1) = partition_count(&stream, true);
        assert_eq!(m0, reference);
        assert_eq!(
            m1, reference,
            "re-partitioned aggregation changed the result"
        );
        println!(
            "  {name:<16} makespan/mean load: hash partitioning {:.2}, adaptive {:.2}   loads {:?} -> {:?}",
            imbalance(&l0),
            imbalance(&l1),
            l0,
            l1
        );
        if skewed {
            assert!(
                imbalance(&l0) >= 1.5
                    && imbalance(&l1) <= 1.3
                    && imbalance(&l1) < imbalance(&l0) * 0.7,
                "skew must be visible and mitigated"
            );
        } else {
            assert!(imbalance(&l1) <= 1.1, "uniform data must not be disturbed");
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// 5. Look-ahead information passing on a star join
// ---------------------------------------------------------------------------------------------------------------------

fn permutations(n: usize) -> Vec<Vec<usize>> {
    if n == 1 {
        return vec![vec![0]];
    }
    let mut out = vec![];
    for p in permutations(n - 1) {
        for i in 0..=p.len() {
            let mut q = p.clone();
            q.insert(i, n - 1);
            out.push(q);
        }
    }
    out
}

fn section_lip() {
    println!(
        "\n== 5. look-ahead information passing: star join, 150000 fact rows, 3 dimensions keeping 2% / 30% / 80% of 20000 keys =="
    );
    let mut r = Rng(5);
    let dims: Vec<HashSet<u64>> = [0.02, 0.30, 0.80]
        .iter()
        .map(|&keep| (0..20_000u64).filter(|_| r.unit() < keep).collect())
        .collect();
    let fact: Vec<[u64; 3]> = (0..150_000)
        .map(|_| [r.below(20_000), r.below(20_000), r.below(20_000)])
        .collect();
    let mut blooms: Vec<Bloom> = dims.iter().map(|d| Bloom::new(d.len(), 10)).collect();
    for (b, d) in blooms.iter_mut().zip(&dims) {
        d.iter().for_each(|&k| b.add(k));
    }
    let reference: usize = fact
        .iter()
        .filter(|t| (0..3).all(|d| dims[d].contains(&t[d])))
        .count();
    let (mut plain, mut lip) = (vec![], vec![]);
    for order in permutations(3) {
        // Plain left-deep hash joins in `order`: every surviving tuple probes the next dimension.
        let (mut w, mut out) = (0.0, 0);
        for t in &fact {
            let mut ok = true;
            for &d in &order {
                w += 1.0;
                if !dims[d].contains(&t[d]) {
                    ok = false;
                    break;
                }
            }
            out += ok as usize;
        }
        assert_eq!(out, reference);
        plain.push((order.clone(), w));
        // LIP: test the Bloom filters first (cost 0.3 per test, most selective filter first by observed pass rate), then join in `order`.
        let (mut w, mut out) = (0.0, 0);
        let mut seen = [1.0f64; 3]; // tuples tested / passed per filter, to order the filters adaptively
        let mut passed = [1.0f64; 3];
        let mut fo = [0usize, 1, 2];
        for (i, t) in fact.iter().enumerate() {
            if i % 1000 == 0 {
                fo.sort_by(|&a, &b| {
                    (passed[a] / seen[a])
                        .partial_cmp(&(passed[b] / seen[b]))
                        .unwrap()
                });
            }
            let mut ok = true;
            for &d in &fo {
                w += 0.3;
                seen[d] += 1.0;
                if blooms[d].may_contain(t[d]) {
                    passed[d] += 1.0
                } else {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            for &d in &order {
                w += 1.0; // survivors now join; false positives are removed by the exact probe
                if !dims[d].contains(&t[d]) {
                    ok = false;
                    break;
                }
            }
            out += ok as usize;
        }
        assert_eq!(out, reference, "LIP changed the join result");
        lip.push((order, w));
    }
    let span = |v: &Vec<(Vec<usize>, f64)>| {
        (
            v.iter().map(|x| x.1).fold(f64::INFINITY, f64::min),
            v.iter().map(|x| x.1).fold(0.0, f64::max),
        )
    };
    let ((pb, pw), (lb, lw)) = (span(&plain), span(&lip));
    println!("  join result rows: {reference} (identical for all 12 variants)");
    println!(
        "  {:<14} {:>12} {:>12} {:>14}",
        "variant", "best order", "worst order", "worst / best"
    );
    println!(
        "  {:<14} {:>12.0} {:>12.0} {:>14.2}",
        "plain joins",
        pb,
        pw,
        pw / pb
    );
    println!(
        "  {:<14} {:>12.0} {:>12.0} {:>14.2}",
        "LIP + joins",
        lb,
        lw,
        lw / lb
    );
    assert!(
        pw / pb >= 1.8,
        "plain join orders must differ a lot, got {}",
        pw / pb
    );
    assert!(
        lw / lb <= 1.15,
        "LIP must make the plan robust to the join order, got {}",
        lw / lb
    );
    assert!(
        lw < pb,
        "even the worst LIP plan must beat the best plain plan here"
    );
}

fn main() {
    section_eddy();
    section_runtime_filters();
    section_switch();
    section_skew();
    section_lip();
    println!("\nall adaptive query processing checks passed");
}
