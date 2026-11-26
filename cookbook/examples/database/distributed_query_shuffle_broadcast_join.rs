//! RECIPE 744 (database): distributed query processing: partitioning, pruning, two-phase aggregation, co-located/broadcast/shuffle/Bloom joins, skew, sample sort.
//! LEVEL: advanced
//! Use in: shared-nothing MPP engines and query planners deciding what to ship over the network.   Run: cargo run --release --example distributed_query_shuffle_broadcast_join
//! COMMON USES: Spark SQL (broadcast hash join, shuffle hash/sort-merge, AQE skew-join splitting), Presto/Trino (partitioned vs broadcast
//! distribution), Snowflake and Redshift (distribution keys, co-located joins), Greenplum/Citus (hash distribution, reference tables),
//! Impala, Vitess scatter queries, TeraSort-style distributed sorting, Bloom-filter runtime filters in Impala/Spark/Doris.
//! Course: CMU 15-445 Database Systems (Fall), lectures 23-24 on distributed OLAP databases and parallel query execution.
#![allow(dead_code)]
//!
//! WHAT: a simulated SHARED-NOTHING cluster of N nodes. Every table is split across nodes by a PARTITIONING SCHEME (hash of a column,
//! hash of another column, round robin, or key ranges). The only real cost in the model is BYTES SHIPPED BETWEEN NODES; local work
//! is free. Operators:
//! * PARTITION PRUNING: with a hash scheme a point lookup touches 1 node; with range partitioning a range predicate touches the nodes
//!   whose key interval overlaps; round robin always touches all.
//! * AGGREGATION: CENTRAL (ship every row to the coordinator) versus TWO-PHASE (each node pre-aggregates, shuffles the small
//!   partial states by group key, final aggregation per owner; partial states must be MERGEABLE: count, sum, min, max).
//! * JOINS, Fact JOIN Dim on key: CO-LOCATED (both already hash-partitioned on the key: zero network), BROADCAST the small side to all
//!   nodes, SHUFFLE (repartition) one or both sides by hash of the key, and a BLOOM SEMI-JOIN REDUCTION (nodes exchange a Bloom filter
//!   of the build keys, drop probe rows that cannot match BEFORE shuffling them; false positives cost bytes, never correctness).
//! * COST MODEL: for each applicable strategy the planner computes the exact bytes from statistics (an N x N matrix of "rows at node i
//!   whose key belongs to node j", and the Bloom survivor counts) and picks the cheapest. The model is checked against the executor's
//!   measured traffic; real optimizers have only estimates, here perfect statistics isolate the PLANNING LOGIC from estimation error.
//! * SKEW: a hash shuffle sends all rows of a hot key to one node, which becomes the straggler. Detect heavy hitters from a sample,
//!   then SALT: spread the hot key's probe rows over all nodes randomly and REPLICATE the matching build rows to every node.
//! * SORT / TOP-N: DISTRIBUTED SAMPLE SORT (sample, choose N-1 splitters, range-shuffle, local sort; concatenating nodes in order is a
//!   globally sorted result) versus top-N where each node sends only its local best N rows.
//!
//! CHECKS: 300 random queries over random cluster sizes, table sizes, skew and partitionings: every applicable join strategy, central
//! and two-phase aggregation return exactly the single-node answer; the model's predicted bytes equal measured bytes and the chosen plan
//! ships the fewest bytes among all candidates; pruning never skips a node holding a match; salting reduces the maximum node load while
//! keeping the result; sample sort equals a single-node sort; top-N equals the sorted prefix.
//!
//! Read: Spark SQL join strategy hints (broadcast / shuffle) https://spark.apache.org/docs/latest/sql-performance-tuning.html
//!       Bloom filter https://en.wikipedia.org/wiki/Bloom_filter
//!       CMU 15-445 Database Systems schedule (distributed OLAP lectures) https://15445.courses.cs.cmu.edu/fall2024/schedule.html

use std::collections::{BTreeMap, HashMap, HashSet};

type F = (u64, u64, i64); // fact row: (key, grp, val)
type D = (u64, i64); // dim row: (key, attr)
type O = (u64, u64, i64, i64); // join output
const FSZ: usize = 24;
const DSZ: usize = 16;
const PSZ: usize = 40; // partial aggregate state: grp, count, sum, min, max

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

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^ (x >> 31)
}

fn owner(key: u64, n: usize) -> usize {
    (mix(key) % n as u64) as usize
}

// ---------------------------------------------------------------- partitioning and pruning

#[derive(Clone, Copy, PartialEq, Debug)]
enum Scheme {
    HashKey,
    HashGrp,
    RoundRobin,
    RangeKey { domain: u64 },
}

fn place_fact(rows: &[F], s: Scheme, n: usize) -> Vec<Vec<F>> {
    let mut parts = vec![vec![]; n];
    for (i, r) in rows.iter().enumerate() {
        let node = match s {
            Scheme::HashKey => owner(r.0, n),
            Scheme::HashGrp => owner(r.1, n),
            Scheme::RoundRobin => i % n,
            Scheme::RangeKey { domain } => (r.0 * n as u64 / domain) as usize,
        };
        parts[node].push(*r);
    }
    parts
}

fn place_dim(rows: &[D], s: Scheme, n: usize) -> Vec<Vec<D>> {
    let mut parts = vec![vec![]; n];
    for (i, r) in rows.iter().enumerate() {
        let node = match s {
            Scheme::HashKey | Scheme::HashGrp => owner(r.0, n),
            Scheme::RoundRobin => i % n,
            Scheme::RangeKey { domain } => (r.0 * n as u64 / domain) as usize,
        };
        parts[node].push(*r);
    }
    parts
}

/// Which nodes can hold rows with key in [lo, hi]? (a point lookup is lo == hi)
fn prune(s: Scheme, n: usize, lo: u64, hi: u64) -> Vec<usize> {
    match s {
        Scheme::HashKey if lo == hi => vec![owner(lo, n)],
        Scheme::RangeKey { domain } => ((lo * n as u64 / domain) as usize
            ..=(hi.min(domain - 1) * n as u64 / domain) as usize)
            .collect(),
        _ => (0..n).collect(),
    }
}

// ---------------------------------------------------------------- network accounting and exchange operators

#[derive(Default)]
struct Net {
    bytes: usize,
}

/// Repartition: every row goes to the node `dest` names; rows already on the right node cost nothing.
fn shuffle<T: Clone>(
    parts: &[Vec<T>],
    dest: impl Fn(&T) -> usize,
    size: usize,
    net: &mut Net,
) -> Vec<Vec<T>> {
    let mut out = vec![vec![]; parts.len()];
    for (i, p) in parts.iter().enumerate() {
        for r in p {
            let j = dest(r);
            if i != j {
                net.bytes += size;
            }
            out[j].push(r.clone());
        }
    }
    out
}

/// Replicate every row to all other nodes; returns the full table that each node now holds.
fn broadcast<T: Clone>(parts: &[Vec<T>], size: usize, net: &mut Net) -> Vec<T> {
    let all: Vec<T> = parts.iter().flatten().cloned().collect();
    net.bytes += all.len() * (parts.len() - 1) * size;
    all
}

fn local_join(facts: &[F], dims: &[D]) -> Vec<O> {
    let mut ht: HashMap<u64, Vec<i64>> = HashMap::new();
    for &(k, a) in dims {
        ht.entry(k).or_default().push(a);
    }
    facts
        .iter()
        .flat_map(|&(k, g, v)| ht.get(&k).into_iter().flatten().map(move |&a| (k, g, v, a)))
        .collect()
}

// ---------------------------------------------------------------- Bloom filter

struct Bloom {
    bits: Vec<u64>,
}

impl Bloom {
    fn build(keys: impl Iterator<Item = u64> + Clone) -> Bloom {
        let n = keys.clone().count().max(1);
        let words = (n * 10).div_ceil(64).max(1); // about 10 bits per key, 3 hashes: ~2% false positives
        let mut b = Bloom {
            bits: vec![0; words],
        };
        for k in keys {
            for i in 0..3 {
                let h = (mix(k ^ (i * 0x1234_5678_9ABC)) % (words as u64 * 64)) as usize;
                b.bits[h / 64] |= 1 << (h % 64);
            }
        }
        b
    }
    fn contains(&self, k: u64) -> bool {
        (0..3).all(|i| {
            let h = (mix(k ^ (i * 0x1234_5678_9ABC)) % (self.bits.len() as u64 * 64)) as usize;
            self.bits[h / 64] >> (h % 64) & 1 == 1
        })
    }
    fn bytes(&self) -> usize {
        self.bits.len() * 8
    }
}

// ---------------------------------------------------------------- join strategies, executor and cost model

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
enum Strategy {
    Colocated,
    BroadcastDim,
    BroadcastFact,
    ShuffleBoth,
    ShuffleDim,
    ShuffleFact,
    BloomShuffle,
}

struct Data {
    n: usize,
    fact: Vec<Vec<F>>, // after filter pushdown
    dim: Vec<Vec<D>>,
    fs: Scheme,
    ds: Scheme,
}

fn applicable(d: &Data) -> Vec<Strategy> {
    let mut v = vec![
        Strategy::BroadcastDim,
        Strategy::BroadcastFact,
        Strategy::ShuffleBoth,
        Strategy::BloomShuffle,
    ];
    if d.fs == Scheme::HashKey && d.ds == Scheme::HashKey {
        v.push(Strategy::Colocated);
    }
    if d.fs == Scheme::HashKey {
        v.push(Strategy::ShuffleDim);
    }
    if d.ds == Scheme::HashKey {
        v.push(Strategy::ShuffleFact);
    }
    v
}

/// Execute a strategy; returns per-node outputs and the bytes it actually moved.
fn execute(d: &Data, s: Strategy) -> (Vec<Vec<O>>, usize) {
    let n = d.n;
    let mut net = Net::default();
    let key_f = |r: &F| owner(r.0, n);
    let key_d = |r: &D| owner(r.0, n);
    let out = match s {
        Strategy::Colocated => (0..n).map(|i| local_join(&d.fact[i], &d.dim[i])).collect(),
        Strategy::BroadcastDim => {
            let all = broadcast(&d.dim, DSZ, &mut net);
            (0..n).map(|i| local_join(&d.fact[i], &all)).collect()
        }
        Strategy::BroadcastFact => {
            let all = broadcast(&d.fact, FSZ, &mut net);
            (0..n).map(|i| local_join(&all, &d.dim[i])).collect()
        }
        Strategy::ShuffleBoth => {
            let (f, dm) = (
                shuffle(&d.fact, key_f, FSZ, &mut net),
                shuffle(&d.dim, key_d, DSZ, &mut net),
            );
            (0..n).map(|i| local_join(&f[i], &dm[i])).collect()
        }
        Strategy::ShuffleDim => {
            let dm = shuffle(&d.dim, key_d, DSZ, &mut net);
            (0..n).map(|i| local_join(&d.fact[i], &dm[i])).collect()
        }
        Strategy::ShuffleFact => {
            let f = shuffle(&d.fact, key_f, FSZ, &mut net);
            (0..n).map(|i| local_join(&f[i], &d.dim[i])).collect()
        }
        Strategy::BloomShuffle => {
            let bloom = Bloom::build(d.dim.iter().flatten().map(|r| r.0));
            net.bytes += 2 * (n - 1) * bloom.bytes(); // gather local filters, broadcast the union
            let kept: Vec<Vec<F>> = d
                .fact
                .iter()
                .map(|p| p.iter().filter(|r| bloom.contains(r.0)).copied().collect())
                .collect();
            let (f, dm) = (
                shuffle(&kept, key_f, FSZ, &mut net),
                shuffle(&d.dim, key_d, DSZ, &mut net),
            );
            (0..n).map(|i| local_join(&f[i], &dm[i])).collect()
        }
    };
    (out, net.bytes)
}

/// Rows whose current node differs from their key's owner (exact, from statistics).
fn off_diagonal<T>(parts: &[Vec<T>], key: impl Fn(&T) -> u64) -> usize {
    let n = parts.len();
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| p.iter().filter(|r| owner(key(r), n) != i).count())
        .sum()
}

/// Predicted bytes from table statistics only (no data movement).
fn cost(d: &Data, s: Strategy) -> usize {
    let n = d.n;
    let nf: usize = d.fact.iter().map(|p| p.len()).sum();
    let nd: usize = d.dim.iter().map(|p| p.len()).sum();
    let f_off = off_diagonal(&d.fact, |r| r.0) * FSZ;
    let d_off = off_diagonal(&d.dim, |r| r.0) * DSZ;
    match s {
        Strategy::Colocated => 0,
        Strategy::BroadcastDim => nd * (n - 1) * DSZ,
        Strategy::BroadcastFact => nf * (n - 1) * FSZ,
        Strategy::ShuffleBoth => f_off + d_off,
        Strategy::ShuffleDim => d_off,
        Strategy::ShuffleFact => f_off,
        Strategy::BloomShuffle => {
            let bloom = Bloom::build(d.dim.iter().flatten().map(|r| r.0));
            let surv: Vec<Vec<F>> = d
                .fact
                .iter()
                .map(|p| p.iter().filter(|r| bloom.contains(r.0)).copied().collect())
                .collect();
            2 * (n - 1) * bloom.bytes() + off_diagonal(&surv, |r| r.0) * FSZ + d_off
        }
    }
}

fn plan(d: &Data) -> (Strategy, Vec<(Strategy, usize)>) {
    let mut c: Vec<(Strategy, usize)> =
        applicable(d).into_iter().map(|s| (s, cost(d, s))).collect();
    c.sort_by_key(|&(s, b)| (b, s));
    (c[0].0, c)
}

fn oracle_join(d: &Data) -> Vec<O> {
    let f: Vec<F> = d.fact.iter().flatten().copied().collect();
    let dm: Vec<D> = d.dim.iter().flatten().copied().collect();
    let mut o = local_join(&f, &dm);
    o.sort();
    o
}

fn flat_sorted(parts: Vec<Vec<O>>) -> Vec<O> {
    let mut o: Vec<O> = parts.into_iter().flatten().collect();
    o.sort();
    o
}

// ---------------------------------------------------------------- aggregation

#[derive(Clone, Copy, PartialEq, Debug)]
struct Agg {
    count: u64,
    sum: i64,
    min: i64,
    max: i64,
}

impl Agg {
    fn one(v: i64) -> Agg {
        Agg {
            count: 1,
            sum: v,
            min: v,
            max: v,
        }
    }
    /// Mergeable partial state: this is what makes two-phase aggregation legal (AVG would carry sum and count).
    fn merge(&mut self, o: &Agg) {
        self.count += o.count;
        self.sum += o.sum;
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
    }
}

fn local_agg(rows: &[F]) -> BTreeMap<u64, Agg> {
    let mut m: BTreeMap<u64, Agg> = BTreeMap::new();
    for r in rows {
        m.entry(r.1)
            .and_modify(|a| a.merge(&Agg::one(r.2)))
            .or_insert(Agg::one(r.2));
    }
    m
}

fn agg_central(parts: &[Vec<F>]) -> (BTreeMap<u64, Agg>, usize) {
    let bytes = parts
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 0)
        .map(|(_, p)| p.len() * FSZ)
        .sum();
    let all: Vec<F> = parts.iter().flatten().copied().collect();
    (local_agg(&all), bytes)
}

fn agg_two_phase(parts: &[Vec<F>]) -> (BTreeMap<u64, Agg>, usize) {
    let n = parts.len();
    let mut bytes = 0;
    // Phase 1: local partial aggregation. Phase 2: shuffle partial states by group, merge at the owner. Then gather to node 0.
    let mut at_owner: Vec<BTreeMap<u64, Agg>> = vec![BTreeMap::new(); n];
    for (i, p) in parts.iter().enumerate() {
        for (g, a) in local_agg(p) {
            let j = owner(g, n);
            if i != j {
                bytes += PSZ;
            }
            at_owner[j]
                .entry(g)
                .and_modify(|x| x.merge(&a))
                .or_insert(a);
        }
    }
    let mut result = BTreeMap::new();
    for (j, m) in at_owner.into_iter().enumerate() {
        if j != 0 {
            bytes += m.len() * PSZ;
        }
        result.extend(m);
    }
    (result, bytes)
}

// ---------------------------------------------------------------- skew: heavy hitters and salting

/// Heavy keys: estimated frequency (sample count / sample rate) above total / (2n). Detected from a 10% sample.
fn heavy_hitters(fact: &[Vec<F>], rng: &mut Rng) -> HashSet<u64> {
    let total: usize = fact.iter().map(|p| p.len()).sum();
    let mut counts: HashMap<u64, usize> = HashMap::new();
    for r in fact.iter().flatten() {
        if rng.below(10) == 0 {
            *counts.entry(r.0).or_default() += 1;
        }
    }
    let n = fact.len();
    counts
        .into_iter()
        .filter(|&(_, c)| c * 10 > total / (2 * n))
        .map(|(k, _)| k)
        .collect()
}

/// Returns (output, bytes, max rows landed on one node) for plain shuffle vs salted shuffle.
fn skew_join(d: &Data, hot: &HashSet<u64>, salted: bool, rng: &mut Rng) -> (Vec<O>, usize, usize) {
    let n = d.n;
    let mut net = Net::default();
    let mut f_dest: Vec<Vec<(usize, F)>> = vec![vec![]; n];
    for (i, p) in d.fact.iter().enumerate() {
        for &r in p {
            let base = owner(r.0, n);
            let j = if salted && hot.contains(&r.0) {
                (base + rng.below(n as u64) as usize) % n
            } else {
                base
            };
            f_dest[i].push((j, r));
        }
    }
    let f = shuffle(&f_dest, |x| x.0, FSZ, &mut net);
    // Build side: hot keys are REPLICATED to every node (split-and-replicate), cold keys go to their owner.
    let mut dm: Vec<Vec<D>> = vec![vec![]; n];
    for (i, p) in d.dim.iter().enumerate() {
        for &r in p {
            if salted && hot.contains(&r.0) {
                for (j, slot) in dm.iter_mut().enumerate() {
                    if i != j {
                        net.bytes += DSZ;
                    }
                    slot.push(r);
                }
            } else {
                let j = owner(r.0, n);
                if i != j {
                    net.bytes += DSZ;
                }
                dm[j].push(r);
            }
        }
    }
    let max_load = f.iter().map(|p| p.len()).max().unwrap();
    let mut out: Vec<O> = (0..n)
        .flat_map(|i| local_join(&f[i].iter().map(|x| x.1).collect::<Vec<_>>(), &dm[i]))
        .collect();
    out.sort();
    (out, net.bytes, max_load)
}

// ---------------------------------------------------------------- sort and top-N

/// Distributed sample sort. Returns (nodes' sorted runs, bytes shipped, max node load).
fn sample_sort(parts: &[Vec<F>], rng: &mut Rng) -> (Vec<Vec<F>>, usize, usize) {
    let n = parts.len();
    let mut net = Net::default();
    // 1. each node sends a small random sample to the coordinator (8 bytes of key per sample ~ FSZ for simplicity).
    let mut sample: Vec<F> = vec![];
    for (i, p) in parts.iter().enumerate() {
        for _ in 0..(8 * n).min(p.len()) {
            sample.push(p[rng.below(p.len() as u64) as usize]);
            if i != 0 {
                net.bytes += FSZ;
            }
        }
    }
    sample.sort_by_key(|r| (r.2, r.0, r.1));
    // 2. n-1 splitters at equal quantiles of the sample, broadcast to all nodes.
    let splitters: Vec<F> = (1..n)
        .map(|i| sample[(i * sample.len() / n).min(sample.len() - 1)])
        .collect();
    net.bytes += splitters.len() * FSZ * (n - 1);
    // 3. range-shuffle, 4. local sort. Node i holds everything between splitter i-1 and i.
    let dest = |r: &F| splitters.partition_point(|s| (s.2, s.0, s.1) <= (r.2, r.0, r.1));
    let mut out = shuffle(parts, dest, FSZ, &mut net);
    for p in out.iter_mut() {
        p.sort_by_key(|r| (r.2, r.0, r.1));
    }
    let max_load = out.iter().map(|p| p.len()).max().unwrap();
    (out, net.bytes, max_load)
}

// ---------------------------------------------------------------- workload generation

struct Zipf {
    cdf: Vec<f64>,
}

impl Zipf {
    fn new(domain: usize, s: f64) -> Zipf {
        let w: Vec<f64> = (1..=domain).map(|i| 1.0 / (i as f64).powf(s)).collect();
        let tot: f64 = w.iter().sum();
        let mut acc = 0.0;
        Zipf {
            cdf: w
                .iter()
                .map(|x| {
                    acc += x / tot;
                    acc
                })
                .collect(),
        }
    }
    fn sample(&self, rng: &mut Rng) -> u64 {
        let u = rng.unit();
        self.cdf.partition_point(|&c| c < u).min(self.cdf.len() - 1) as u64
    }
}

fn pick_scheme(rng: &mut Rng, domain: u64) -> Scheme {
    match rng.below(10) {
        0..=3 => Scheme::HashKey,
        4..=5 => Scheme::HashGrp,
        6..=7 => Scheme::RoundRobin,
        _ => Scheme::RangeKey { domain },
    }
}

struct Config {
    n: usize,
    domain: u64,
    nfact: usize,
    ndim: usize,
    skew: f64,
    fs: Scheme,
    ds: Scheme,
    min_val: i64,
}

fn random_config(rng: &mut Rng) -> Config {
    let domain = 200 + rng.below(1300);
    // log-uniform dim size: from tiny lookup tables to tables as large as the key domain.
    let ndim = ((3.0 + (domain as f64 / 2.0 - 3.0) * rng.unit().powi(3)) as usize).max(3);
    Config {
        n: [4, 6, 8][rng.below(3) as usize],
        domain,
        nfact: 300 + rng.below(3700) as usize,
        ndim,
        skew: [0.0, 0.7, 1.1][rng.below(3) as usize],
        fs: pick_scheme(rng, domain),
        ds: pick_scheme(rng, domain),
        min_val: rng.below(400) as i64,
    }
}

fn build(c: &Config, rng: &mut Rng) -> (Data, Vec<F>, Vec<D>) {
    let z = Zipf::new(c.domain as usize, c.skew);
    // Spread hot ranks over the key space so skew is about frequency, not about key adjacency.
    let perm = |r: u64| mix(r) % c.domain;
    let fact_all: Vec<F> = (0..c.nfact)
        .map(|_| (perm(z.sample(rng)), rng.below(20), rng.below(1000) as i64))
        .collect();
    let mut keys: Vec<u64> = (0..c.domain).collect();
    for i in (1..keys.len()).rev() {
        keys.swap(i, rng.below(i as u64 + 1) as usize);
    }
    let dim_all: Vec<D> = keys
        .iter()
        .take(c.ndim)
        .map(|&k| (k, rng.below(100) as i64))
        .collect();
    let filtered: Vec<F> = fact_all
        .iter()
        .filter(|r| r.2 >= c.min_val)
        .copied()
        .collect(); // predicate pushdown
    let data = Data {
        n: c.n,
        fact: place_fact(&filtered, c.fs, c.n),
        dim: place_dim(&dim_all, c.ds, c.n),
        fs: c.fs,
        ds: c.ds,
    };
    (data, fact_all, dim_all)
}

fn main() {
    let mut rng = Rng(0xD157_0FFE_2024_0001);

    println!(
        "== 300 random queries: strategies vs single-node oracle, cost model vs measured bytes"
    );
    let mut chosen: BTreeMap<Strategy, usize> = BTreeMap::new();
    let (mut sum_best, mut sum_naive, mut worst_ratio) = (0usize, 0usize, 0f64);
    let (mut agg_c, mut agg_t) = (0usize, 0usize);
    for q in 0..300 {
        let cfg = random_config(&mut rng);
        let (d, fact_all, _dim_all) = build(&cfg, &mut rng);
        let want = oracle_join(&d);
        let (best, costs) = plan(&d);
        let mut measured: Vec<(Strategy, usize)> = vec![];
        for s in applicable(&d) {
            let (out, bytes) = execute(&d, s);
            assert_eq!(
                flat_sorted(out),
                want,
                "query {q}: {s:?} differs from the single-node join"
            );
            let predicted = costs.iter().find(|c| c.0 == s).unwrap().1;
            assert_eq!(
                predicted, bytes,
                "query {q}: cost model for {s:?} disagrees with measured traffic"
            );
            measured.push((s, bytes));
        }
        let min = measured.iter().map(|m| m.1).min().unwrap();
        let chosen_bytes = measured.iter().find(|m| m.0 == best).unwrap().1;
        assert_eq!(
            chosen_bytes, min,
            "query {q}: planner picked {best:?} which is not the cheapest"
        );
        *chosen.entry(best).or_default() += 1;
        let shuffle_both = measured
            .iter()
            .find(|m| m.0 == Strategy::ShuffleBoth)
            .unwrap()
            .1;
        sum_best += chosen_bytes;
        sum_naive += shuffle_both;
        worst_ratio = worst_ratio.max(shuffle_both as f64 / chosen_bytes.max(1) as f64);

        // Aggregation: central vs two-phase must equal the oracle.
        let filtered: Vec<F> = fact_all
            .iter()
            .filter(|r| r.2 >= cfg.min_val)
            .copied()
            .collect();
        let truth = local_agg(&filtered);
        let (c1, b1) = agg_central(&d.fact);
        let (c2, b2) = agg_two_phase(&d.fact);
        assert_eq!(c1, truth);
        assert_eq!(c2, truth);
        agg_c += b1;
        agg_t += b2;
    }
    println!(
        "  planner choices: {}",
        chosen
            .iter()
            .map(|(s, c)| format!("{s:?} x{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "  total bytes: chosen plans {sum_best}, always-shuffle-both {sum_naive} ({:.1}x more); worst single query {worst_ratio:.1}x",
        sum_naive as f64 / sum_best.max(1) as f64
    );
    println!(
        "  aggregation bytes over all queries: central {agg_c}, two-phase {agg_t} ({:.1}x less)",
        agg_c as f64 / agg_t as f64
    );
    assert!(
        chosen.len() >= 3,
        "the cost model should pick different strategies for different shapes"
    );
    assert!(agg_t < agg_c);

    println!("== network bytes by strategy for four hand-picked shapes (8 nodes)");
    let shapes: [(&str, usize, usize, u64, Scheme, Scheme); 4] = [
        (
            "big fact, tiny dim (round robin)",
            4000,
            20,
            2000,
            Scheme::RoundRobin,
            Scheme::RoundRobin,
        ),
        (
            "both large, hashed on other cols",
            4000,
            1500,
            2000,
            Scheme::HashGrp,
            Scheme::RoundRobin,
        ),
        (
            "both hashed on the join key",
            4000,
            1500,
            2000,
            Scheme::HashKey,
            Scheme::HashKey,
        ),
        (
            "big fact, selective mid-size dim",
            4000,
            120,
            6000,
            Scheme::RoundRobin,
            Scheme::RoundRobin,
        ),
    ];
    let order = [
        Strategy::Colocated,
        Strategy::BroadcastDim,
        Strategy::BroadcastFact,
        Strategy::ShuffleBoth,
        Strategy::ShuffleDim,
        Strategy::ShuffleFact,
        Strategy::BloomShuffle,
    ];
    print!("  {:<36}", "shape");
    for s in order {
        print!("{:>13}", format!("{s:?}"));
    }
    println!("  chosen");
    for (name, nfact, ndim, domain, fs, ds) in shapes {
        let cfg = Config {
            n: 8,
            domain,
            nfact,
            ndim,
            skew: 0.0,
            fs,
            ds,
            min_val: 0,
        };
        let (d, _, _) = build(&cfg, &mut rng);
        let (best, _) = plan(&d);
        print!("  {name:<36}");
        for s in order {
            if applicable(&d).contains(&s) {
                print!("{:>13}", execute(&d, s).1);
            } else {
                print!("{:>13}", "-");
            }
        }
        println!("  {best:?}");
    }

    println!("== partition pruning (point and range lookups on the key)");
    let (mut touched, mut total) = (BTreeMap::<&str, usize>::new(), 0usize);
    for _ in 0..300 {
        let domain = 1000;
        let n = 8;
        let rows: Vec<F> = (0..2000)
            .map(|_| (rng.below(domain), rng.below(20), rng.below(1000) as i64))
            .collect();
        let (lo, w) = (
            rng.below(domain),
            if rng.below(2) == 0 { 0 } else { rng.below(100) },
        );
        let hi = (lo + w).min(domain - 1);
        let want: Vec<F> = rows
            .iter()
            .filter(|r| r.0 >= lo && r.0 <= hi)
            .copied()
            .collect();
        for (name, s) in [
            ("hash(key)", Scheme::HashKey),
            ("range(key)", Scheme::RangeKey { domain }),
            ("round robin", Scheme::RoundRobin),
        ] {
            let parts = place_fact(&rows, s, n);
            let nodes = prune(s, n, lo, hi);
            let mut got: Vec<F> = nodes
                .iter()
                .flat_map(|&i| parts[i].iter().filter(|r| r.0 >= lo && r.0 <= hi).copied())
                .collect();
            // Soundness: no node outside the pruned set holds a match.
            for (i, p) in parts.iter().enumerate() {
                if !nodes.contains(&i) {
                    assert!(
                        p.iter().all(|r| r.0 < lo || r.0 > hi),
                        "pruned node {i} held a match under {name}"
                    );
                }
            }
            let mut w2 = want.clone();
            got.sort();
            w2.sort();
            assert_eq!(got, w2);
            *touched.entry(name).or_default() += nodes.len();
        }
        total += 1;
    }
    for (k, v) in &touched {
        println!(
            "  {k:<12} average nodes touched: {:.2} of 8",
            *v as f64 / total as f64
        );
    }
    assert!(touched["range(key)"] < touched["round robin"]);

    println!("== data skew: heavy hitters, salting, split-and-replicate");
    let (mut plain_load, mut salt_load, mut plain_b, mut salt_b) = (0usize, 0usize, 0usize, 0usize);
    for _ in 0..40 {
        let cfg = Config {
            n: 8,
            domain: 400,
            nfact: 3000,
            ndim: 150,
            skew: 1.2,
            fs: Scheme::RoundRobin,
            ds: Scheme::RoundRobin,
            min_val: 0,
        };
        let (d, _, _) = build(&cfg, &mut rng);
        let want = oracle_join(&d);
        let hot = heavy_hitters(&d.fact, &mut rng);
        let (o1, b1, l1) = skew_join(&d, &hot, false, &mut rng);
        let (o2, b2, l2) = skew_join(&d, &hot, true, &mut rng);
        assert_eq!(o1, want);
        assert_eq!(o2, want, "salted join must equal the plain join");
        assert!(
            !hot.is_empty() && l2 <= l1,
            "salting must not make the hottest node worse"
        );
        plain_load += l1;
        salt_load += l2;
        plain_b += b1;
        salt_b += b2;
    }
    println!(
        "  max rows on the hottest node (mean of 40 runs): plain shuffle {} -> salted {} (ideal {})",
        plain_load / 40,
        salt_load / 40,
        3000 / 8
    );
    println!(
        "  extra network for replicating hot dim rows: {} -> {} bytes per run",
        plain_b / 40,
        salt_b / 40
    );
    assert!(
        salt_load * 10 < plain_load * 8,
        "salting should cut the straggler load clearly"
    );

    println!("== distributed sample sort and top-N");
    let (mut imbalance, mut sort_bytes, mut topn_bytes) = (0f64, 0usize, 0usize);
    for _ in 0..50 {
        let cfg = random_config(&mut rng);
        let (d, _, _) = build(&cfg, &mut rng);
        let all: Vec<F> = d.fact.iter().flatten().copied().collect();
        if all.len() < 50 {
            continue;
        }
        let mut want = all.clone();
        want.sort_by_key(|r| (r.2, r.0, r.1));
        let (runs, bytes, max_load) = sample_sort(&d.fact, &mut rng);
        let got: Vec<F> = runs.into_iter().flatten().collect();
        assert_eq!(got, want, "concatenated node runs must be globally sorted");
        imbalance += max_load as f64 / (all.len() as f64 / d.n as f64);
        sort_bytes += bytes;
        // Top-N: every node sends only its local best N, the coordinator merges.
        let top = 10 + rng.below(30) as usize;
        let mut cand: Vec<F> = vec![];
        for (i, p) in d.fact.iter().enumerate() {
            let mut l = p.clone();
            l.sort_by_key(|r| (r.2, r.0, r.1));
            l.truncate(top);
            if i != 0 {
                topn_bytes += l.len() * FSZ;
            }
            cand.extend(l);
        }
        cand.sort_by_key(|r| (r.2, r.0, r.1));
        cand.truncate(top);
        assert_eq!(cand, want[..top.min(want.len())].to_vec());
    }
    println!(
        "  sample sort: mean max-node load {:.2}x the average; shipped {} bytes in total; top-N shipped {} bytes in total",
        imbalance / 50.0,
        sort_bytes,
        topn_bytes
    );
    assert!(topn_bytes < sort_bytes);
    assert!(imbalance / 50.0 < 1.6);

    println!("distributed_query_shuffle_broadcast_join: all checks passed");
}
