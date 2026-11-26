//! RECIPE 741 (database): cloud data warehouse core ideas: micro-partitions, pruning metadata, clustering, time travel, zero-copy clone, elastic compute.
//! LEVEL: advanced
//! Use in: cloud warehouses and lakehouse engines, any system that stores immutable columnar files plus a metadata catalog.   Run: cargo run --release --example cloud_warehouse_micro_partitions
//! COMMON USES: Snowflake (micro-partitions, clustering, time travel, zero-copy clone, virtual warehouses, result cache), BigQuery
//! and Redshift RA3 (storage separated from compute), Delta Lake / Iceberg / Hudi (file lists + min/max stats), ClickHouse parts.
//! Course: CMU 15-721 Advanced Database Systems (Spring), lecture on cloud databases and the Snowflake architecture.
#![allow(dead_code)]
//!
//! WHAT: a tiny in-memory model of the architecture in the Snowflake paper, with three decoupled layers:
//! * STORAGE: an object store of IMMUTABLE micro-partitions (columnar, ~500 rows here, tens of MB in real systems). Files are
//!   never edited; DML writes new files. Each file is reference counted by the table versions that list it.
//! * METADATA / CATALOG: per partition and column min, max, null count and a distinct-count estimate (linear-counting bitmap),
//!   kept OUTSIDE the files so a planner can skip a file without opening it. A table VERSION is just a list of partition ids.
//! * COMPUTE: stateless VIRTUAL WAREHOUSES (worker pools) that read partitions from storage into a local cache, plus a RESULT
//!   CACHE keyed by query and table version.
//!
//! WHY it works: because versions are lists of immutable ids, TIME TRAVEL is "read an older list", a ZERO-COPY CLONE is "copy a
//! list" (copy-on-write: the clone's later DML creates new files and leaves shared ones alone), and a file is physically freed only
//! when its reference count reaches zero. PARTITION PRUNING turns min/max metadata into skipped I/O; it is only effective when
//! the data is CLUSTERED on the filter column, so a background CLUSTERING service rewrites files when the average overlap depth
//! (how many partitions cover a typical key value) is too high.
//!
//! HOW compute scales: queries are split per surviving partition and routed by hash(partition id) % workers so a given file
//! keeps landing on the same worker (cache affinity); adding workers adds cache capacity, so the hit ratio rises with size.
//! The result cache stores (version, answer); any DML bumps the table version, which invalidates dependent entries.
//! MULTI-CLUSTER AUTOSCALING is a feedback controller on queue depth: add a cluster when the backlog per cluster exceeds a
//! threshold, remove one after a quiet period.
//!
//! CHECKS: results through the pruned, cached, multi-worker path equal a plain full scan oracle; on 2000 random predicates
//! pruning never drops a partition that holds a matching row (for 3 layouts); clustering improves the pruned fraction; time
//! travel returns the exact historical contents; modifying a clone leaves the original unchanged and shares storage; refcounts
//! free exactly the unreferenced files; the autoscaler beats a fixed single cluster on tail wait.
//!
//! Read: The Snowflake Elastic Data Warehouse (SIGMOD 2016), summarized in the micro-partition docs https://docs.snowflake.com/en/user-guide/tables-clustering-micropartitions
//!       CMU 15-721 course schedule (cloud data warehouse lectures) https://15721.courses.cs.cmu.edu/spring2024/
//!       Delta Lake: High-Performance ACID Table Storage over Cloud Object Stores https://www.vldb.org/pvldb/vol13/p3411-armbrust.pdf

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

// ---------------------------------------------------------------- data model

const NC: usize = 4; // columns: 0 = ts (clustering key), 1 = customer, 2 = amount, 3 = score (nullable)
const PSIZE: usize = 500; // rows per micro-partition
const TS_RANGE: i64 = 1_000_000;

type Row = [Option<i64>; NC];

/// One immutable columnar file.
struct Part {
    n: usize,
    cols: Vec<Vec<Option<i64>>>,
}

impl Part {
    fn from_rows(rows: &[Row]) -> Part {
        let cols = (0..NC)
            .map(|c| rows.iter().map(|r| r[c]).collect())
            .collect();
        Part {
            n: rows.len(),
            cols,
        }
    }
    fn row(&self, i: usize) -> Row {
        std::array::from_fn(|c| self.cols[c][i])
    }
    fn bytes(&self) -> usize {
        self.n * NC * 8
    }
}

/// Catalog entry for one column of one partition: enough to skip the file without opening it.
#[derive(Clone, Debug)]
struct ColMeta {
    min: Option<i64>,
    max: Option<i64>,
    nulls: usize,
    ndv: f64,
}

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^ (x >> 31)
}

/// Linear counting: hash values into an m-bit bitmap, estimate n = -m ln(zero fraction). Cheap enough to compute at write time.
fn ndv_estimate(vals: impl Iterator<Item = i64>) -> f64 {
    const M: usize = 2048;
    let mut bits = [false; M];
    for v in vals {
        bits[(mix(v as u64) % M as u64) as usize] = true;
    }
    let zeros = bits.iter().filter(|b| !**b).count().max(1);
    -(M as f64) * (zeros as f64 / M as f64).ln()
}

fn make_meta(p: &Part) -> Vec<ColMeta> {
    p.cols
        .iter()
        .map(|c| {
            let vals = c.iter().flatten().copied();
            ColMeta {
                min: vals.clone().min(),
                max: vals.clone().max(),
                nulls: c.iter().filter(|v| v.is_none()).count(),
                ndv: ndv_estimate(vals),
            }
        })
        .collect()
}

// ---------------------------------------------------------------- storage layer with reference counts

#[derive(Default)]
struct Store {
    parts: HashMap<u64, Arc<Part>>,
    meta: HashMap<u64, Vec<ColMeta>>, // the catalog, kept apart from the data files
    refs: HashMap<u64, usize>,
    next: u64,
    puts: usize,
}

impl Store {
    fn put(&mut self, rows: &[Row]) -> u64 {
        let p = Part::from_rows(rows);
        let id = self.next;
        self.next += 1;
        self.meta.insert(id, make_meta(&p));
        self.parts.insert(id, Arc::new(p));
        self.refs.insert(id, 0);
        self.puts += 1;
        id
    }
    fn incref(&mut self, ids: &[u64]) {
        for id in ids {
            *self.refs.get_mut(id).expect("live partition") += 1;
        }
    }
    /// Drop one reference per id; physically delete files nobody lists any more. Returns files freed.
    fn decref(&mut self, ids: &[u64]) -> usize {
        let mut freed = 0;
        for id in ids {
            let r = self.refs.get_mut(id).expect("live partition");
            *r -= 1;
            if *r == 0 {
                self.refs.remove(id);
                self.parts.remove(id);
                self.meta.remove(id);
                freed += 1;
            }
        }
        freed
    }
    fn read_all(&self, ids: &[u64]) -> Vec<Row> {
        ids.iter()
            .flat_map(|id| {
                (0..self.parts[id].n)
                    .map(|i| self.parts[id].row(i))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

/// A table is a history of versions; a version is a list of partition ids.
struct Table {
    versions: Vec<Vec<u64>>,
}

impl Table {
    fn new() -> Table {
        Table {
            versions: vec![vec![]],
        }
    }
    fn head(&self) -> &Vec<u64> {
        self.versions.last().unwrap()
    }
    fn version(&self) -> usize {
        self.versions.len() - 1
    }
    fn commit(&mut self, store: &mut Store, list: Vec<u64>) {
        store.incref(&list);
        self.versions.push(list);
    }
    fn insert(&mut self, store: &mut Store, rows: &[Row]) {
        let mut list = self.head().clone();
        list.extend(rows.chunks(PSIZE).map(|c| store.put(c)));
        self.commit(store, list);
    }
    /// DELETE WHERE ts in [lo, hi]: copy-on-write. Untouched partitions are re-listed; touched ones are rewritten.
    fn delete_ts(&mut self, store: &mut Store, lo: i64, hi: i64) -> usize {
        let q = Query {
            preds: vec![(0, lo, hi)],
        };
        let (mut list, mut removed) = (vec![], 0);
        for &id in self.head() {
            if !may_match(&store.meta[&id], &q) {
                list.push(id);
                continue;
            }
            let rows = store.read_all(&[id]);
            let keep: Vec<Row> = rows.iter().filter(|r| !matches(r, &q)).copied().collect();
            removed += rows.len() - keep.len();
            if keep.len() == rows.len() {
                list.push(id);
            } else if !keep.is_empty() {
                list.push(store.put(&keep));
            }
        }
        self.commit(store, list);
        removed
    }
    /// Zero-copy clone: a new history whose first version is the current list. No data file is copied.
    fn clone_table(&self, store: &mut Store) -> Table {
        store.incref(self.head());
        Table {
            versions: vec![self.head().clone()],
        }
    }
    /// Drop versions older than the newest `keep` (end of the time-travel window); returns files freed.
    fn expire(&mut self, store: &mut Store, keep: usize) -> usize {
        let cut = self.versions.len().saturating_sub(keep);
        let old: Vec<Vec<u64>> = self.versions.drain(..cut).collect();
        old.iter().map(|v| store.decref(v)).sum()
    }
    fn drop_table(self, store: &mut Store) -> usize {
        self.versions.iter().map(|v| store.decref(v)).sum()
    }
}

// ---------------------------------------------------------------- queries, pruning, clustering

#[derive(Clone, Debug)]
struct Query {
    preds: Vec<(usize, i64, i64)>, // conjunction of col BETWEEN lo AND hi
}

fn matches(r: &Row, q: &Query) -> bool {
    q.preds
        .iter()
        .all(|&(c, lo, hi)| r[c].is_some_and(|v| v >= lo && v <= hi))
}

/// Conservative test: false only if no row of the partition can satisfy the query. Must never give a false negative.
fn may_match(m: &[ColMeta], q: &Query) -> bool {
    q.preds
        .iter()
        .all(|&(c, lo, hi)| match (m[c].min, m[c].max) {
            (Some(mn), Some(mx)) => mx >= lo && mn <= hi,
            _ => false, // all values NULL: a range predicate can never be true
        })
}

type Res = (u64, i64); // COUNT(*), SUM(amount)

fn oracle(rows: &[Row], q: &Query) -> Res {
    rows.iter()
        .filter(|r| matches(r, q))
        .fold((0, 0), |(c, s), r| (c + 1, s + r[2].unwrap()))
}

fn scan_part(p: &Part, q: &Query) -> Res {
    (0..p.n)
        .map(|i| p.row(i))
        .filter(|r| matches(r, q))
        .fold((0, 0), |(c, s), r| (c + 1, s + r[2].unwrap()))
}

/// Average number of partitions whose [min,max] covers a key value, over evenly spaced probe points (1.0 = perfectly clustered).
fn overlap_depth(store: &Store, list: &[u64], col: usize) -> f64 {
    let probes = 200;
    let total: usize = (0..probes)
        .map(|i| {
            let x = (i as i64) * TS_RANGE / probes as i64;
            list.iter().filter(|id| matches!((store.meta[id][col].min, store.meta[id][col].max), (Some(a), Some(b)) if a <= x && x <= b)).count()
        })
        .sum();
    total as f64 / probes as f64
}

/// Background clustering service: if the key is poorly clustered, rewrite the table sorted by the key as a NEW version.
/// The old files stay referenced by the old version (time travel) until that version expires.
fn auto_cluster(table: &mut Table, store: &mut Store, col: usize, threshold: f64) -> bool {
    if overlap_depth(store, table.head(), col) <= threshold {
        return false;
    }
    let mut rows = store.read_all(table.head());
    rows.sort_by_key(|r| r[col]);
    let list = rows.chunks(PSIZE).map(|c| store.put(c)).collect();
    table.commit(store, list);
    true
}

// ---------------------------------------------------------------- compute layer

struct Worker {
    cap: usize,
    cache: HashMap<u64, Arc<Part>>,
    order: VecDeque<u64>, // LRU order, front = coldest
    hits: usize,
    misses: usize,
    remote_bytes: usize,
}

impl Worker {
    fn new(cap: usize) -> Worker {
        Worker {
            cap,
            cache: HashMap::new(),
            order: VecDeque::new(),
            hits: 0,
            misses: 0,
            remote_bytes: 0,
        }
    }
    fn fetch(&mut self, store: &Store, id: u64) -> Arc<Part> {
        if let Some(p) = self.cache.get(&id) {
            self.hits += 1;
            self.order.retain(|x| *x != id);
            self.order.push_back(id);
            return p.clone();
        }
        self.misses += 1;
        let p = store.parts[&id].clone(); // the "remote" GET from the object store
        self.remote_bytes += p.bytes();
        if self.cache.len() >= self.cap {
            let cold = self.order.pop_front().unwrap();
            self.cache.remove(&cold);
        }
        self.cache.insert(id, p.clone());
        self.order.push_back(id);
        p
    }
}

/// An elastic pool of stateless workers over shared storage.
struct Warehouse {
    workers: Vec<Worker>,
    affinity: bool,
    rr: usize,
}

impl Warehouse {
    fn new(n: usize, cap: usize, affinity: bool) -> Warehouse {
        Warehouse {
            workers: (0..n).map(|_| Worker::new(cap)).collect(),
            affinity,
            rr: 0,
        }
    }
    /// Prune with the catalog, fan the surviving partitions out to workers, merge partial aggregates.
    fn run(&mut self, store: &Store, list: &[u64], q: &Query) -> (Res, usize) {
        let keep: Vec<u64> = list
            .iter()
            .copied()
            .filter(|id| may_match(&store.meta[id], q))
            .collect();
        let mut acc = (0, 0);
        for &id in &keep {
            let w = if self.affinity {
                (mix(id) % self.workers.len() as u64) as usize
            } else {
                self.rr % self.workers.len()
            };
            self.rr += 1;
            let p = self.workers[w].fetch(store, id);
            let (c, s) = scan_part(&p, q);
            acc = (acc.0 + c, acc.1 + s);
        }
        (acc, list.len() - keep.len())
    }
    fn hit_ratio(&self) -> f64 {
        let h: usize = self.workers.iter().map(|w| w.hits).sum();
        let m: usize = self.workers.iter().map(|w| w.misses).sum();
        h as f64 / (h + m).max(1) as f64
    }
}

/// Result cache keyed by query text; each entry remembers the table version it was computed at.
#[derive(Default)]
struct ResultCache {
    map: HashMap<String, (usize, Res)>,
    hits: usize,
    misses: usize,
    invalidated: usize,
}

impl ResultCache {
    fn get(&mut self, key: &str, version: usize) -> Option<Res> {
        match self.map.get(key) {
            Some(&(v, r)) if v == version => {
                self.hits += 1;
                Some(r)
            }
            Some(_) => {
                self.map.remove(key); // the table changed underneath this entry
                self.invalidated += 1;
                self.misses += 1;
                None
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }
    fn put(&mut self, key: String, version: usize, r: Res) {
        self.map.insert(key, (version, r));
    }
}

// ---------------------------------------------------------------- autoscaling controller

struct Sim {
    p95_wait: usize,
    max_wait: usize,
    cluster_ticks: usize,
    max_clusters: usize,
    served: usize,
}

/// Discrete-time queue: each cluster serves `per_cluster` queries per tick. `auto` = scale on backlog per cluster.
fn simulate_load(arrivals: &[usize], auto: bool, max_clusters: usize) -> Sim {
    let per_cluster = 4;
    let (mut queue, mut waits) = (VecDeque::<usize>::new(), vec![]);
    let (mut clusters, mut quiet, mut cluster_ticks, mut peak) = (1usize, 0usize, 0usize, 1usize);
    let mut t = 0;
    while t < arrivals.len() || !queue.is_empty() {
        if t < arrivals.len() {
            queue.extend(std::iter::repeat_n(t, arrivals[t]));
        }
        if auto {
            if queue.len() > 6 * clusters && clusters < max_clusters {
                clusters += 1; // backlog per cluster too high: start another cluster
                quiet = 0;
            } else if queue.is_empty() {
                quiet += 1;
                if quiet >= 10 && clusters > 1 {
                    clusters -= 1; // sustained idleness: shut one down to save credits
                    quiet = 0;
                }
            } else {
                quiet = 0;
            }
        }
        peak = peak.max(clusters);
        cluster_ticks += clusters;
        for _ in 0..clusters * per_cluster {
            if let Some(a) = queue.pop_front() {
                waits.push(t - a);
            }
        }
        t += 1;
        assert!(t < 100_000, "queue never drained");
    }
    waits.sort();
    Sim {
        p95_wait: waits[waits.len() * 95 / 100],
        max_wait: *waits.last().unwrap(),
        cluster_ticks,
        max_clusters: peak,
        served: waits.len(),
    }
}

// ---------------------------------------------------------------- workload

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
}

fn gen_rows(n: usize, rng: &mut Rng) -> Vec<Row> {
    (0..n)
        .map(|_| {
            [
                Some(rng.below(TS_RANGE as u64) as i64),
                Some(rng.below(5000) as i64),
                Some(rng.below(1000) as i64),
                if rng.below(10) < 3 {
                    None
                } else {
                    Some(rng.below(100) as i64)
                },
            ]
        })
        .collect()
}

fn random_query(rng: &mut Rng) -> Query {
    let width = |r: &mut Rng, dom: i64, pct: i64| {
        let w = (dom * pct / 100).max(1);
        let lo = r.below((dom - w).max(1) as u64) as i64;
        (lo, lo + r.below(w as u64) as i64)
    };
    let kind = rng.below(100);
    let pct = 1 + rng.below(5) as i64;
    let mut preds = vec![];
    if kind < 70 {
        let (lo, hi) = width(rng, TS_RANGE, pct);
        preds.push((0, lo, hi));
    } else if kind < 85 {
        let (lo, hi) = width(rng, 5000, 10);
        preds.push((1, lo, hi));
    } else {
        let (lo, hi) = width(rng, TS_RANGE, pct * 4);
        preds.push((0, lo, hi));
        let (lo, hi) = width(rng, 1000, 50);
        preds.push((2, lo, hi));
    }
    Query { preds }
}

/// For 2000 random predicates: pruning must keep every partition containing a matching row. Returns % of partitions pruned.
fn pruning_experiment(store: &Store, list: &[u64], seed: u64, queries: usize) -> f64 {
    let mut rng = Rng(seed);
    let (mut pruned, mut total) = (0usize, 0usize);
    for _ in 0..queries {
        let q = random_query(&mut rng);
        for id in list {
            total += 1;
            if may_match(&store.meta[id], &q) {
                continue;
            }
            pruned += 1;
            let p = &store.parts[id];
            assert!(
                (0..p.n).all(|i| !matches(&p.row(i), &q)),
                "pruned a partition that contains a matching row: {q:?}"
            );
        }
    }
    100.0 * pruned as f64 / total as f64
}

fn main() {
    let mut rng = Rng(0xC10D_DA7A_0123_4567);
    let rows = gen_rows(20_000, &mut rng);
    let mut store = Store::default();

    println!("== layouts: same 20000 rows, {} per micro-partition", PSIZE);
    // (a) arrival order random (late/out-of-order ingest); (b) arrival order = time order; (c) (a) after auto-clustering.
    let mut unclustered = Table::new();
    unclustered.insert(&mut store, &rows);
    let mut sorted_rows = rows.clone();
    sorted_rows.sort_by_key(|r| r[0]);
    let mut natural = Table::new();
    natural.insert(&mut store, &sorted_rows);
    let mut reclustered = Table::new();
    reclustered.insert(&mut store, &rows);
    let depth_before = overlap_depth(&store, reclustered.head(), 0);
    assert!(
        auto_cluster(&mut reclustered, &mut store, 0, 2.0),
        "clustering service should trigger"
    );
    let depth_after = overlap_depth(&store, reclustered.head(), 0);
    assert!(
        !auto_cluster(&mut reclustered, &mut store, 0, 2.0),
        "second pass must be a no-op"
    );
    println!(
        "  overlap depth on ts: random arrival {depth_before:.1} -> after auto-clustering {depth_after:.1} (natural order {:.1})",
        overlap_depth(&store, natural.head(), 0)
    );
    assert!(depth_after < depth_before / 5.0);

    println!(
        "== partition pruning on 2000 random predicates (never drops a matching partition: asserted)"
    );
    let pu = pruning_experiment(&store, unclustered.head(), 7, 2000);
    let pn = pruning_experiment(&store, natural.head(), 7, 2000);
    let pr = pruning_experiment(&store, reclustered.head(), 7, 2000);
    println!(
        "  pruned: unclustered {pu:.1}%   natural(time-ordered) {pn:.1}%   re-clustered {pr:.1}%"
    );
    assert!(
        pn > pu + 20.0 && pr > pu + 20.0,
        "clustering must improve pruning"
    );
    let ndv_err: f64 = {
        let id = natural.head()[0];
        let m = &store.meta[&id][0];
        let truth = store.parts[&id].cols[0]
            .iter()
            .flatten()
            .collect::<HashSet<_>>()
            .len() as f64;
        (m.ndv - truth).abs() / truth
    };
    println!(
        "  distinct-count estimate error on one partition ts column: {:.1}%",
        ndv_err * 100.0
    );
    assert!(ndv_err < 0.2);

    println!("== query results equal the full-scan oracle (all layouts, 1/2/4/8 workers)");
    let queries: Vec<Query> = (0..300).map(|_| random_query(&mut rng)).collect();
    for (name, t) in [
        ("unclustered", &unclustered),
        ("natural", &natural),
        ("reclustered", &reclustered),
    ] {
        let all = store.read_all(t.head());
        for workers in [1, 2, 4, 8] {
            let mut wh = Warehouse::new(workers, 6, true);
            for q in &queries {
                let (got, _) = wh.run(&store, t.head(), q);
                assert_eq!(got, oracle(&all, q), "{name} with {workers} workers");
            }
        }
    }
    println!("  300 queries x 3 layouts x 4 warehouse sizes: all equal");

    println!(
        "== local cache and elasticity: hit ratio vs warehouse size (hot set of 12 time ranges, 6 partitions cached per worker)"
    );
    let hot: Vec<Query> = (0..12)
        .map(|i| Query {
            preds: vec![(0, i * 70_000, i * 70_000 + 40_000)],
        })
        .collect();
    let mut last = 0.0;
    for workers in [1, 2, 4, 8] {
        let mut aff = Warehouse::new(workers, 6, true);
        let mut blind = Warehouse::new(workers, 6, false);
        let mut r = Rng(99);
        for _ in 0..600 {
            let q = &hot[r.below(12) as usize];
            let (a, _) = aff.run(&store, natural.head(), q);
            let (b, _) = blind.run(&store, natural.head(), q);
            assert_eq!(a, b);
        }
        let remote: usize = aff.workers.iter().map(|w| w.remote_bytes).sum();
        println!(
            "  {workers} workers: hit ratio with cache affinity {:.1}%  without {:.1}%  (remote bytes {remote})",
            100.0 * aff.hit_ratio(),
            100.0 * blind.hit_ratio()
        );
        if workers > 1 {
            assert!(
                aff.hit_ratio() >= last - 0.02,
                "more workers should not shrink the aggregate cache"
            );
        }
        last = aff.hit_ratio();
    }
    assert!(last > 0.8, "with 8 workers the whole hot set fits in cache");

    println!("== time travel, zero-copy clone, copy-on-write, reference counting");
    let mut shadow = sorted_rows.clone();
    let mut t = Table::new();
    t.insert(&mut store, &sorted_rows);
    let v_before = t.version();
    let files_before = store.parts.len();
    let mut clone = t.clone_table(&mut store);
    assert_eq!(
        store.parts.len(),
        files_before,
        "clone copies no data files"
    );
    // DML on the original after the clone point.
    let removed = t.delete_ts(&mut store, 100_000, 160_000);
    shadow.retain(|r| !(100_000..=160_000).contains(&r[0].unwrap()));
    assert!(removed > 0);
    let extra = gen_rows(700, &mut rng);
    t.insert(&mut store, &extra);
    shadow.extend(extra.iter().copied());
    // Time travel: version v_before still has the original rows.
    let mut hist = store.read_all(&t.versions[v_before]);
    hist.sort();
    let mut orig_sorted = sorted_rows.clone();
    orig_sorted.sort();
    assert_eq!(hist, orig_sorted, "time travel returns historical contents");
    // The clone, created at v_before, is unaffected by the original's later DML.
    let mut cl = store.read_all(clone.head());
    cl.sort();
    assert_eq!(
        cl, orig_sorted,
        "clone unaffected by changes to the original"
    );
    // Modify the clone; the original must not move.
    let head_before = t.head().clone();
    clone.delete_ts(&mut store, 500_000, 600_000);
    assert_eq!(t.head(), &head_before);
    let mut cur = store.read_all(t.head());
    let mut want = shadow.clone();
    cur.sort();
    want.sort();
    assert_eq!(cur, want, "original equals shadow model");
    let shared = clone.versions[0]
        .iter()
        .filter(|id| t.versions[v_before].contains(id))
        .count();
    let listed: usize = t
        .versions
        .iter()
        .chain(clone.versions.iter())
        .map(|v| v.len())
        .sum();
    let distinct: HashSet<u64> = t
        .versions
        .iter()
        .chain(clone.versions.iter())
        .flatten()
        .copied()
        .collect();
    println!(
        "  this table + clone: {} distinct files back {} listed entries; {} shared between original@v{} and clone",
        distinct.len(),
        listed,
        shared,
        v_before
    );
    assert!(distinct.len() < listed && shared == clone.versions[0].len());
    // Reference counts: refs equal the number of version lists mentioning each file.
    let mut expect: HashMap<u64, usize> = HashMap::new();
    for v in t
        .versions
        .iter()
        .chain(clone.versions.iter())
        .chain(unclustered.versions.iter())
        .chain(natural.versions.iter())
        .chain(reclustered.versions.iter())
    {
        for id in v {
            *expect.entry(*id).or_default() += 1;
        }
    }
    assert_eq!(expect, store.refs);
    let freed_clone = clone.drop_table(&mut store);
    let freed_old = t.expire(&mut store, 2);
    println!(
        "  dropped clone -> {freed_clone} files freed; expired old versions -> {freed_old} files freed; original head still readable"
    );
    let mut cur = store.read_all(t.head());
    cur.sort();
    assert_eq!(cur, want);
    for id in t.head() {
        assert!(store.refs[id] >= 1);
    }

    println!("== result cache with invalidation on table change");
    let mut rc = ResultCache::default();
    let mut wh = Warehouse::new(4, 8, true);
    let q = Query {
        preds: vec![(0, 200_000, 260_000)],
    };
    let key = format!("{q:?}");
    let run = |rc: &mut ResultCache, wh: &mut Warehouse, store: &Store, t: &Table| -> (Res, bool) {
        if let Some(r) = rc.get(&key, t.version()) {
            return (r, true);
        }
        let (r, _) = wh.run(store, t.head(), &q);
        rc.put(key.clone(), t.version(), r);
        (r, false)
    };
    let (r1, hit1) = run(&mut rc, &mut wh, &store, &t);
    let (r2, hit2) = run(&mut rc, &mut wh, &store, &t);
    assert!(!hit1 && hit2 && r1 == r2);
    t.insert(&mut store, &[[Some(210_000), Some(1), Some(77), None]]);
    shadow.push([Some(210_000), Some(1), Some(77), None]);
    let (r3, hit3) = run(&mut rc, &mut wh, &store, &t);
    assert!(!hit3, "entry must be invalidated by the insert");
    assert_eq!(r3, oracle(&shadow, &q));
    assert_eq!(r3.0, r1.0 + 1);
    println!(
        "  hits {} misses {} invalidations {}; answer after insert reflects the new row ({} -> {})",
        rc.hits, rc.misses, rc.invalidated, r1.0, r3.0
    );

    println!("== multi-cluster autoscaling on queue depth");
    let mut arrivals = vec![];
    for tick in 0..400 {
        let burst = (100..160).contains(&tick) || (300..330).contains(&tick);
        arrivals.push(if burst {
            12 + rng.below(6) as usize
        } else {
            1 + rng.below(3) as usize
        });
    }
    let total: usize = arrivals.iter().sum();
    let fixed = simulate_load(&arrivals, false, 1);
    let auto = simulate_load(&arrivals, true, 6);
    println!(
        "  fixed 1 cluster : p95 wait {:>3} ticks, max {:>3}, cluster-ticks {}",
        fixed.p95_wait, fixed.max_wait, fixed.cluster_ticks
    );
    println!(
        "  autoscaled (<=6): p95 wait {:>3} ticks, max {:>3}, cluster-ticks {}, peak clusters {}",
        auto.p95_wait, auto.max_wait, auto.cluster_ticks, auto.max_clusters
    );
    assert_eq!((fixed.served, auto.served), (total, total), "no query lost");
    assert!(auto.p95_wait < fixed.p95_wait && auto.max_clusters <= 6 && auto.max_clusters > 1);
    assert!(
        auto.cluster_ticks < 6 * (400 + auto.max_wait),
        "cheaper than always running the maximum"
    );

    println!(
        "cloud_warehouse_micro_partitions: all checks passed (store has {} files, {} puts)",
        store.parts.len(),
        store.puts
    );
}
