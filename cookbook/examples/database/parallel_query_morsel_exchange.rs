//! RECIPE 720 (database): parallel query execution: morsel-driven pipelines, exchange operators, parallel build/probe, sample sort.
//! LEVEL: advanced
//! Use in: analytical engines, parallel dataframe libraries, shuffle layers of distributed SQL, anything that must use all cores on one query.   Run: cargo run --release --example parallel_query_morsel_exchange
//! COMMON USES: HyPer / Umbra (the morsel-driven design below), DuckDB's pipeline executor, Spark SQL and other shuffle based
//! engines (hash-partition / broadcast / range exchange between stages), the Volcano "exchange operator" of Graefe, partitioned
//! parallel aggregation in PostgreSQL's Parallel Hash and ClickHouse's two-level aggregation.
//! Course: CMU 15-445 Lecture 14 (parallel query execution) and CMU 15-721 (parallel databases: morsels, exchange, parallel joins).
#![allow(dead_code)]
//!
//! WHAT: a miniature parallel executor running two TPC-H flavoured queries over generated tables (all integer arithmetic, so
//! parallel and serial answers are EXACTLY equal, not just close):
//! * Q1-like: scan lineitem, filter on shipdate, GROUP BY (flag, status), SUM/COUNT. A scan-filter-aggregate pipeline.
//! * Q3-like: customer -> orders -> lineitem three-way join, GROUP BY orderkey, SUM(revenue), top-10. Three pipelines separated
//!   by PIPELINE BREAKERS (the hash table builds), the last one probes both tables and feeds an aggregation.
//!
//! INTER- vs INTRA-operator parallelism. Inter-operator: different operators (or pipelines) run at once on different threads,
//! connected by queues; parallelism is capped by the plan's depth and one slow stage throttles the rest. Intra-operator: the SAME
//! operator runs on many threads over different slices of its input; parallelism scales with data. The exchange section below
//! shows the inter-operator style (producer threads feed consumer threads through channels), everything else is intra-operator.
//!
//! MORSEL-DRIVEN PARALLELISM (Leis et al.): the input of a pipeline is cut into small fixed-size MORSELS (here 1024 rows, in the
//! paper ~100k). Worker threads pull the next morsel from a shared dispatcher (one atomic `fetch_add`), run the WHOLE pipeline
//! on it (filter, probe, partial aggregate) and keep their results in THREAD-LOCAL state, so the hot loop touches no locks.
//! Because morsels are claimed dynamically, a fast or lucky thread simply takes more of them: load balancing is automatic and
//! skew (a few morsels that cost 100x more) cannot idle the other cores. `StealQueues` is the NUMA-flavoured variant: every
//! worker first drains its OWN contiguous lane (locality) and only then steals single morsels from the others.
//! Static partitioning (one slice per thread, decided up front) is the contrast: it is only as fast as its unluckiest slice.
//!
//! PIPELINE BREAKERS: a hash-join build must finish before probing starts, an aggregation must see all input before emitting. A
//! pipeline is the maximal chain of operators between breakers; the scheduler runs pipelines one after another, each one
//! morsel-parallel. Three ways to build the shared table (`Mode`):
//! * Serial: workers scan in parallel but ONE thread inserts everything. Simple, and the textbook Amdahl bottleneck.
//! * Striped: all workers insert concurrently into one logical table made of 64 lock-protected stripes (lock striping stands
//!   in for the lock-free CAS table of the paper; no unsafe needed). Contention is low because stripes are many.
//! * Partitioned: workers first hash-partition rows into thread-local buffers, then phase two builds each partition's table
//!   in parallel with NO sharing at all. Costs one extra pass, scales best. The same trick parallelises the final aggregate
//!   merge (a thread-local table per worker merged serially vs. partition-wise merge in parallel).
//!
//! EXCHANGE operators move rows between pipelines running on different threads: HASH-PARTITION (equal keys meet at one
//! consumer, so a GROUP BY needs no merge), BROADCAST (a small table is copied to every consumer so the big side never moves),
//! GATHER / MERGE (many sorted streams become one; a k-way merge keeps the order). PARALLEL SORT is a sample sort: sample,
//! pick splitters, range-partition exchange, sort each range independently, concatenate: no serial merge at all, compared here
//! against local sort + gather-merge whose final merge is serial.
//!
//! AMDAHL: speed-up <= 1 / (s + (1 - s)/n) where s is the serial fraction. The scaling table prints the serial seconds that were
//! measured inside the 1-worker run, the Amdahl prediction from them, and the observed speed-up. Serial build and serial merge
//! are exactly the `s`. (Timings are printed, never asserted: they depend on the machine and its core count.)
//!
//! CHECKS: 200 random queries (Q1 with random cutoffs, Q3 with random segment / date / build mode) on skewed tables; each answer
//! for 1, 2, 4 and 8 workers must equal a row-at-a-time oracle written independently of the parallel code. Exchanges, sorts and
//! the stealing dispatcher are checked the same way (every morsel processed exactly once).
//! Read: Morsel-Driven Parallelism https://db.in.tum.de/~leis/papers/morsels.pdf   CMU 15-445 https://15445.courses.cs.cmu.edu/fall2024/
//!       CMU 15-721 https://15721.courses.cs.cmu.edu/spring2024/

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::{Mutex, mpsc};
use std::thread;
use std::time::Instant;

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

// ---------------------------------------------------------------- tables
/// Columnar lineitem: parallel scans read only the columns a pipeline needs.
struct Lineitem {
    orderkey: Vec<u32>,
    price: Vec<u64>,
    disc: Vec<u64>,
    flag: Vec<u8>,
    status: Vec<u8>,
    shipdate: Vec<u32>,
}
struct Orders {
    cust: Vec<u32>,
    date: Vec<u32>,
} // orderkey = row index
struct Customer {
    seg: Vec<u8>,
} // custkey = row index
struct Db {
    li: Lineitem,
    ord: Orders,
    cust: Customer,
}

/// `skew`: the first 1% of orders own ~300 lineitems each, so their morsels (and group sizes) are much heavier.
fn gen_db(rng: &mut Rng, n_orders: usize, skew: bool) -> Db {
    let n_cust = (n_orders / 8).max(4);
    let cust = Customer {
        seg: (0..n_cust).map(|_| rng.below(5) as u8).collect(),
    };
    let mut ord = Orders {
        cust: vec![],
        date: vec![],
    };
    let mut li = Lineitem {
        orderkey: vec![],
        price: vec![],
        disc: vec![],
        flag: vec![],
        status: vec![],
        shipdate: vec![],
    };
    for o in 0..n_orders {
        ord.cust.push(rng.below(n_cust as u64) as u32);
        let d = rng.below(2400) as u32;
        ord.date.push(d);
        let lines = if skew && o < n_orders / 100 {
            300
        } else {
            1 + rng.below(7)
        };
        for _ in 0..lines {
            li.orderkey.push(o as u32);
            li.price.push(100 + rng.below(100_000));
            li.disc.push(rng.below(11));
            li.flag.push(rng.below(3) as u8);
            li.status.push(rng.below(2) as u8);
            li.shipdate.push(d + 1 + rng.below(120) as u32);
        }
    }
    Db { li, ord, cust }
}

// ---------------------------------------------------------------- morsel dispatch
/// Shared morsel dispatcher: ONE atomic counter is the whole scheduler. `take` is wait-free.
struct Morsels {
    next: AtomicUsize,
    total: usize,
    size: usize,
}
impl Morsels {
    fn new(total: usize, size: usize) -> Self {
        Morsels {
            next: AtomicUsize::new(0),
            total,
            size,
        }
    }
    fn take(&self) -> Option<Range<usize>> {
        let s = self.next.fetch_add(self.size, Relaxed);
        (s < self.total).then(|| s..(s + self.size).min(self.total))
    }
}

/// Per-worker lanes with stealing: each lane is a contiguous run of morsel indices claimed one at a time.
struct StealQueues {
    lanes: Vec<(AtomicUsize, usize)>, // (next morsel index, end index)
    total: usize,
    size: usize,
}
impl StealQueues {
    fn new(workers: usize, total: usize, size: usize) -> Self {
        let n_morsels = total.div_ceil(size);
        let per = n_morsels.div_ceil(workers);
        let lanes = (0..workers)
            .map(|w| {
                (
                    AtomicUsize::new((w * per).min(n_morsels)),
                    ((w + 1) * per).min(n_morsels),
                )
            })
            .collect();
        StealQueues { lanes, total, size }
    }
    /// Returns (rows, stolen?). Own lane first (cache/NUMA locality), then scan the other lanes round-robin.
    fn take(&self, me: usize) -> Option<(Range<usize>, bool)> {
        let n = self.lanes.len();
        for k in 0..n {
            let (next, end) = &self.lanes[(me + k) % n];
            let m = next.fetch_add(1, Relaxed);
            if m < *end {
                return Some((m * self.size..((m + 1) * self.size).min(self.total), k > 0));
            }
        }
        None
    }
}

/// Run one pipeline: every worker builds a thread-local state with `mk`, then repeatedly takes a morsel and calls `body`.
/// The per-worker states are returned for the (serial or parallel) merge phase.
fn run_pipeline<L: Send>(
    workers: usize,
    total: usize,
    morsel: usize,
    mk: impl Fn() -> L + Sync,
    body: impl Fn(&mut L, Range<usize>) + Sync,
) -> Vec<L> {
    let q = Morsels::new(total, morsel);
    thread::scope(|s| {
        let hs: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut local = mk();
                    while let Some(r) = q.take() {
                        body(&mut local, r);
                    }
                    local
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

// ---------------------------------------------------------------- hash tables (build modes)
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Serial,
    Striped,
    Partitioned,
}

fn h(k: u32) -> usize {
    ((k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) as usize
}

/// A hash table split into parts; part = h(key) % parts. Serial has 1 part, Striped 64, Partitioned 16.
struct PartTable<V> {
    parts: Vec<HashMap<u32, V>>,
}
impl<V: Copy> PartTable<V> {
    fn get(&self, k: u32) -> Option<V> {
        self.parts[h(k) % self.parts.len()].get(&k).copied()
    }
}

const STRIPES: usize = 64;
const PARTS: usize = 16;

/// Pipeline-breaker build. `scan` is the pipeline body: it turns a morsel into (key, value) rows via the `emit` callback.
/// Returns the table and the SERIAL seconds spent (the Amdahl term).
fn build_table<V: Copy + Send + Sync, F>(
    workers: usize,
    total: usize,
    morsel: usize,
    mode: Mode,
    scan: F,
) -> (PartTable<V>, f64)
where
    F: Fn(Range<usize>, &mut dyn FnMut(u32, V)) + Sync,
{
    match mode {
        Mode::Serial => {
            let locals = run_pipeline(workers, total, morsel, Vec::<(u32, V)>::new, |l, r| {
                scan(r, &mut |k, v| l.push((k, v)))
            });
            let t = Instant::now(); // everything below runs on a single thread
            let mut m = HashMap::with_capacity(locals.iter().map(Vec::len).sum());
            for (k, v) in locals.into_iter().flatten() {
                m.insert(k, v);
            }
            (PartTable { parts: vec![m] }, t.elapsed().as_secs_f64())
        }
        Mode::Striped => {
            let stripes: Vec<Mutex<HashMap<u32, V>>> =
                (0..STRIPES).map(|_| Mutex::new(HashMap::new())).collect();
            run_pipeline(
                workers,
                total,
                morsel,
                || (),
                |_, r| {
                    scan(r, &mut |k, v| {
                        stripes[h(k) % STRIPES].lock().unwrap().insert(k, v);
                    })
                },
            );
            (
                PartTable {
                    parts: stripes
                        .into_iter()
                        .map(|m| m.into_inner().unwrap())
                        .collect(),
                },
                0.0,
            )
        }
        Mode::Partitioned => {
            // Phase 1: thread-local partition buffers. Phase 2: one task per partition, built without any sharing.
            let locals = run_pipeline(
                workers,
                total,
                morsel,
                || {
                    (0..PARTS)
                        .map(|_| Vec::<(u32, V)>::new())
                        .collect::<Vec<_>>()
                },
                |l, r| scan(r, &mut |k, v| l[h(k) % PARTS].push((k, v))),
            );
            let tasks = Morsels::new(PARTS, 1);
            let built: Vec<Vec<(usize, HashMap<u32, V>)>> = thread::scope(|s| {
                let hs: Vec<_> = (0..workers)
                    .map(|_| {
                        s.spawn(|| {
                            let mut mine = vec![];
                            while let Some(p) = tasks.take() {
                                let p = p.start;
                                let mut m = HashMap::new();
                                for l in &locals {
                                    m.extend(l[p].iter().copied());
                                }
                                mine.push((p, m));
                            }
                            mine
                        })
                    })
                    .collect();
                hs.into_iter().map(|x| x.join().unwrap()).collect()
            });
            let mut parts: Vec<HashMap<u32, V>> = (0..PARTS).map(|_| HashMap::new()).collect();
            for (p, m) in built.into_iter().flatten() {
                parts[p] = m;
            }
            (PartTable { parts }, 0.0)
        }
    }
}

// ---------------------------------------------------------------- Q1: scan -> filter -> aggregate
/// [flag][status] -> [sum_qty-like (rows), sum_price, sum_disc_price (price*(100-disc)), count]
type Q1Agg = [[[u64; 4]; 2]; 3];

fn q1_scan(li: &Lineitem, r: Range<usize>, cut: u32, agg: &mut Q1Agg) {
    for i in r {
        if li.shipdate[i] <= cut {
            let a = &mut agg[li.flag[i] as usize][li.status[i] as usize];
            a[0] += 1;
            a[1] += li.price[i];
            a[2] += li.price[i] * (100 - li.disc[i]);
            a[3] += 1;
        }
    }
}

fn q1_parallel(li: &Lineitem, cut: u32, workers: usize) -> (Q1Agg, f64) {
    let locals = run_pipeline(
        workers,
        li.shipdate.len(),
        1024,
        || [[[0u64; 4]; 2]; 3],
        |a, r| q1_scan(li, r, cut, a),
    );
    let t = Instant::now(); // merging 6 groups per worker: trivial, but still serial
    let mut out = [[[0u64; 4]; 2]; 3];
    for l in locals {
        for (f, row) in l.iter().enumerate() {
            for (s, cell) in row.iter().enumerate() {
                (0..4).for_each(|k| out[f][s][k] += cell[k]);
            }
        }
    }
    (out, t.elapsed().as_secs_f64())
}

/// Oracle: independent row-at-a-time implementation with a BTreeMap.
fn q1_oracle(db: &Db, cut: u32) -> Q1Agg {
    let mut m: BTreeMap<(u8, u8), [u64; 4]> = BTreeMap::new();
    for i in 0..db.li.shipdate.len() {
        if db.li.shipdate[i] <= cut {
            let e = m.entry((db.li.flag[i], db.li.status[i])).or_default();
            *e = [
                e[0] + 1,
                e[1] + db.li.price[i],
                e[2] + db.li.price[i] * (100 - db.li.disc[i]),
                e[3] + 1,
            ];
        }
    }
    let mut out = [[[0u64; 4]; 2]; 3];
    for ((f, s), v) in m {
        out[f as usize][s as usize] = v;
    }
    out
}

// ---------------------------------------------------------------- Q3: customer x orders x lineitem
type Q3Row = (u32, u64, u32); // (orderkey, revenue, orderdate)

fn top10(mut v: Vec<(u32, u64)>) -> Vec<(u32, u64)> {
    v.sort_by_key(|&(k, rev)| (Reverse(rev), k)); // revenue desc, orderkey asc: a total order, so results are unique
    v.truncate(10);
    v
}

#[derive(Default, Clone, Copy, Debug)]
struct Serial {
    build: f64, // serial seconds inside table builds
    merge: f64, // serial seconds inside aggregate merges
}

fn q3_parallel(db: &Db, seg: u8, date: u32, mode: Mode, workers: usize) -> (Vec<Q3Row>, Serial) {
    let mut ser = Serial::default();
    // Pipeline 1: customer scan + segment filter -> build set of custkeys.
    let (cset, s1) = build_table::<(), _>(workers, db.cust.seg.len(), 1024, mode, |r, emit| {
        for c in r {
            if db.cust.seg[c] == seg {
                emit(c as u32, ());
            }
        }
    });
    // Pipeline 2: orders scan + date filter + probe customer set -> build orders table (orderkey -> orderdate).
    let (otab, s2) = build_table::<u32, _>(workers, db.ord.date.len(), 1024, mode, |r, emit| {
        for o in r {
            if db.ord.date[o] < date && cset.get(db.ord.cust[o]).is_some() {
                emit(o as u32, db.ord.date[o]);
            }
        }
    });
    ser.build = s1 + s2;
    // Pipeline 3: lineitem scan + filter + probe orders table + aggregate by orderkey.
    let li = &db.li;
    let probe = |r: Range<usize>, emit: &mut dyn FnMut(u32, u64)| {
        for i in r {
            if li.shipdate[i] > date && otab.get(li.orderkey[i]).is_some() {
                emit(li.orderkey[i], li.price[i] * (100 - li.disc[i]));
            }
        }
    };
    let n = li.orderkey.len();
    let top: Vec<(u32, u64)> = if mode == Mode::Partitioned {
        // Partition-wise aggregation: rows go to the buffer of h(key)%PARTS, so each key lives in exactly one partition and the
        // merge phase is embarrassingly parallel; only the final top-10 of top-10s is serial (<= 10*PARTS rows).
        let locals = run_pipeline(
            workers,
            n,
            1024,
            || {
                (0..PARTS)
                    .map(|_| Vec::<(u32, u64)>::new())
                    .collect::<Vec<_>>()
            },
            |l, r| probe(r, &mut |k, v| l[h(k) % PARTS].push((k, v))),
        );
        let tasks = Morsels::new(PARTS, 1);
        let tops: Vec<Vec<(u32, u64)>> = thread::scope(|s| {
            let hs: Vec<_> = (0..workers)
                .map(|_| {
                    s.spawn(|| {
                        let mut mine = vec![];
                        while let Some(p) = tasks.take() {
                            let mut m: HashMap<u32, u64> = HashMap::new();
                            for l in &locals {
                                for &(k, v) in &l[p.start] {
                                    *m.entry(k).or_insert(0) += v;
                                }
                            }
                            mine.extend(top10(m.into_iter().collect()));
                        }
                        mine
                    })
                })
                .collect();
            hs.into_iter().map(|x| x.join().unwrap()).collect()
        });
        let t = Instant::now();
        let r = top10(tops.into_iter().flatten().collect());
        ser.merge = t.elapsed().as_secs_f64();
        r
    } else {
        // Thread-local tables merged by ONE thread: the merge cost grows with groups x workers.
        let locals = run_pipeline(workers, n, 1024, HashMap::<u32, u64>::new, |m, r| {
            probe(r, &mut |k, v| *m.entry(k).or_insert(0) += v)
        });
        let t = Instant::now();
        let mut all: HashMap<u32, u64> = HashMap::new();
        for l in locals {
            for (k, v) in l {
                *all.entry(k).or_insert(0) += v;
            }
        }
        let r = top10(all.into_iter().collect());
        ser.merge = t.elapsed().as_secs_f64();
        r
    };
    (
        top.into_iter()
            .map(|(k, rev)| (k, rev, otab.get(k).unwrap()))
            .collect(),
        ser,
    )
}

fn q3_oracle(db: &Db, seg: u8, date: u32) -> Vec<Q3Row> {
    let custs: HashSet<usize> = (0..db.cust.seg.len())
        .filter(|&c| db.cust.seg[c] == seg)
        .collect();
    let mut rev: BTreeMap<u32, u64> = BTreeMap::new();
    for i in 0..db.li.orderkey.len() {
        let o = db.li.orderkey[i] as usize;
        if db.li.shipdate[i] > date
            && db.ord.date[o] < date
            && custs.contains(&(db.ord.cust[o] as usize))
        {
            *rev.entry(o as u32).or_insert(0) += db.li.price[i] * (100 - db.li.disc[i]);
        }
    }
    top10(rev.into_iter().collect())
        .into_iter()
        .map(|(k, r)| (k, r, db.ord.date[k as usize]))
        .collect()
}

#[derive(Clone, Copy, Debug)]
enum Query {
    Q1 { cut: u32 },
    Q3 { seg: u8, date: u32, mode: Mode },
}
#[derive(PartialEq, Debug)]
enum Answer {
    Q1(Q1Agg),
    Q3(Vec<Q3Row>),
}

fn run_parallel(db: &Db, q: Query, workers: usize) -> (Answer, Serial) {
    match q {
        Query::Q1 { cut } => {
            let (a, merge) = q1_parallel(&db.li, cut, workers);
            (Answer::Q1(a), Serial { build: 0.0, merge })
        }
        Query::Q3 { seg, date, mode } => {
            let (r, s) = q3_parallel(db, seg, date, mode, workers);
            (Answer::Q3(r), s)
        }
    }
}
fn run_oracle(db: &Db, q: Query) -> Answer {
    match q {
        Query::Q1 { cut } => Answer::Q1(q1_oracle(db, cut)),
        Query::Q3 { seg, date, .. } => Answer::Q3(q3_oracle(db, seg, date)),
    }
}

fn random_query(rng: &mut Rng) -> Query {
    if rng.below(2) == 0 {
        Query::Q1 {
            cut: rng.below(2700) as u32,
        }
    } else {
        let mode = [Mode::Serial, Mode::Striped, Mode::Partitioned][rng.below(3) as usize];
        Query::Q3 {
            seg: rng.below(5) as u8,
            date: 600 + rng.below(1500) as u32,
            mode,
        }
    }
}

// ---------------------------------------------------------------- exchange operators
const BATCH: usize = 512;

/// HASH-PARTITION exchange feeding an aggregation. Producers scan morsels and route (orderkey, revenue) by hash to consumer
/// channels (bounded: a slow consumer back-pressures producers). Equal keys always reach the SAME consumer, so consumers
/// aggregate privately and the final "merge" is a mere union of disjoint maps. Returns the result and rows moved.
fn exchange_hash_agg(
    li: &Lineitem,
    producers: usize,
    consumers: usize,
) -> (BTreeMap<u32, u64>, usize) {
    let (txs, rxs): (Vec<_>, Vec<_>) = (0..consumers)
        .map(|_| mpsc::sync_channel::<Vec<(u32, u64)>>(4))
        .unzip();
    let q = Morsels::new(li.orderkey.len(), 1024);
    let (qr, lr) = (&q, li);
    thread::scope(|s| {
        let cons: Vec<_> = rxs
            .into_iter()
            .map(|rx| {
                s.spawn(move || {
                    let mut m: HashMap<u32, u64> = HashMap::new();
                    for batch in rx {
                        for (k, v) in batch {
                            *m.entry(k).or_insert(0) += v;
                        }
                    }
                    m
                })
            })
            .collect();
        let prods: Vec<_> = (0..producers)
            .map(|_| {
                let txs = txs.clone();
                s.spawn(move || {
                    let mut bufs: Vec<Vec<(u32, u64)>> = vec![Vec::new(); consumers];
                    let mut moved = 0;
                    while let Some(r) = qr.take() {
                        for i in r {
                            let d = h(lr.orderkey[i]) % consumers;
                            bufs[d].push((lr.orderkey[i], lr.price[i] * (100 - lr.disc[i])));
                            if bufs[d].len() == BATCH {
                                moved += BATCH;
                                txs[d].send(std::mem::take(&mut bufs[d])).unwrap();
                            }
                        }
                    }
                    for (d, b) in bufs.into_iter().enumerate() {
                        moved += b.len();
                        if !b.is_empty() {
                            txs[d].send(b).unwrap();
                        }
                    }
                    moved
                })
            })
            .collect();
        drop(txs); // consumers see end-of-stream only when EVERY sender (including this original) is gone
        let moved = prods.into_iter().map(|p| p.join().unwrap()).sum();
        let mut out = BTreeMap::new();
        for c in cons {
            for (k, v) in c.join().unwrap() {
                assert!(
                    out.insert(k, v).is_none(),
                    "hash exchange must keep each key at a single consumer"
                );
            }
        }
        (out, moved)
    })
}

/// BROADCAST exchange: the small side is copied to every consumer; the large side is NOT moved, each consumer probes its own
/// morsels. Rows moved = |small| * consumers, versus |small| + |large| for a shuffle (hash both sides).
fn broadcast_join_sum(li: &Lineitem, small: &[(u32, u32)], consumers: usize) -> (u64, usize) {
    let (txs, rxs): (Vec<_>, Vec<_>) = (0..consumers)
        .map(|_| mpsc::sync_channel::<Vec<(u32, u32)>>(4))
        .unzip();
    let q = Morsels::new(li.orderkey.len(), 1024);
    let qr = &q;
    thread::scope(|s| {
        let cons: Vec<_> = rxs
            .into_iter()
            .map(|rx| {
                s.spawn(move || {
                    let mut tab: HashMap<u32, u32> = HashMap::new();
                    for b in rx {
                        tab.extend(b);
                    }
                    let mut sum = 0u64;
                    while let Some(r) = qr.take() {
                        for i in r {
                            if tab.contains_key(&li.orderkey[i]) {
                                sum += li.price[i] * (100 - li.disc[i]);
                            }
                        }
                    }
                    sum
                })
            })
            .collect();
        for chunk in small.chunks(BATCH) {
            for tx in &txs {
                tx.send(chunk.to_vec()).unwrap();
            }
        }
        drop(txs);
        (
            cons.into_iter().map(|c| c.join().unwrap()).sum(),
            small.len() * consumers,
        )
    })
}

/// GATHER-MERGE exchange: each worker sorts its slice locally and ships the sorted run; the gatherer k-way merges them with a
/// heap. The order inside every stream is preserved, but this final merge is a single thread (Amdahl again).
fn gather_merge_sort(data: &[u64], workers: usize) -> Vec<u64> {
    let (tx, rx) = mpsc::channel::<Vec<u64>>();
    let per = data.len().div_ceil(workers.max(1)).max(1);
    thread::scope(|s| {
        for chunk in data.chunks(per) {
            let tx = tx.clone();
            s.spawn(move || {
                let mut run = chunk.to_vec();
                run.sort_unstable();
                tx.send(run).unwrap();
            });
        }
        drop(tx);
    });
    let runs: Vec<Vec<u64>> = rx.into_iter().collect();
    let mut heap: BinaryHeap<Reverse<(u64, usize, usize)>> = BinaryHeap::new();
    for (r, run) in runs.iter().enumerate() {
        if let Some(&v) = run.first() {
            heap.push(Reverse((v, r, 0)));
        }
    }
    let mut out = Vec::with_capacity(data.len());
    while let Some(Reverse((v, r, i))) = heap.pop() {
        out.push(v);
        if let Some(&nx) = runs[r].get(i + 1) {
            heap.push(Reverse((nx, r, i + 1)));
        }
    }
    out
}

/// SAMPLE SORT: (1) sample and pick workers-1 splitters, (2) every worker range-partitions its slice into `workers` buckets
/// (a range exchange), (3) worker b sorts bucket b gathered from all workers, (4) concatenate in bucket order. Any monotone
/// bucket function keeps the output sorted; good splitters only decide how EVEN the buckets are (heavy duplicates unbalance
/// them but never break correctness).
fn sample_sort(data: &[u64], workers: usize) -> Vec<u64> {
    if workers <= 1 || data.len() < 64 {
        let mut v = data.to_vec();
        v.sort_unstable();
        return v;
    }
    let step = (data.len() / (workers * 32)).max(1);
    let mut sample: Vec<u64> = data.iter().step_by(step).copied().collect();
    sample.sort_unstable();
    let splitters: Vec<u64> = (1..workers)
        .map(|i| sample[i * sample.len() / workers])
        .collect();
    let bucket = |x: u64| splitters.partition_point(|&s| s <= x);
    let per = data.len().div_ceil(workers);
    // Phase 1: scatter. scattered[w][b] = rows of worker w's slice that belong to bucket b.
    let scattered: Vec<Vec<Vec<u64>>> = thread::scope(|s| {
        let hs: Vec<_> = data
            .chunks(per)
            .map(|chunk| {
                let bucket = &bucket;
                s.spawn(move || {
                    let mut b = vec![Vec::new(); workers];
                    for &x in chunk {
                        b[bucket(x)].push(x);
                    }
                    b
                })
            })
            .collect();
        hs.into_iter().map(|x| x.join().unwrap()).collect()
    });
    // Phase 2: each bucket is gathered and sorted independently, in parallel.
    let sorted: Vec<Vec<u64>> = thread::scope(|s| {
        let hs: Vec<_> = (0..workers)
            .map(|b| {
                let scattered = &scattered;
                s.spawn(move || {
                    let mut v: Vec<u64> = scattered
                        .iter()
                        .flat_map(|w| w[b].iter().copied())
                        .collect();
                    v.sort_unstable();
                    v
                })
            })
            .collect();
        hs.into_iter().map(|x| x.join().unwrap()).collect()
    });
    sorted.concat()
}

// ---------------------------------------------------------------- skew: static slices vs dynamic morsels vs stealing
/// Deterministic busy-work whose cost depends on the row: the first 1/16 of the rows are 100x heavier ("hot keys").
fn row_cost(i: usize, n: usize) -> u64 {
    let rounds = if i < n / 16 { 200 } else { 2 };
    let mut x = i as u64 + 1;
    for _ in 0..rounds {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407)
            ^ (x >> 29);
    }
    x
}

struct SkewRun {
    result: u64,
    wall: f64,
    imbalance: f64, // max worker busy time / mean worker busy time (1.0 = perfect balance)
    steals: usize,
    morsels: usize,
}

fn skew_run(kind: &str, workers: usize, n: usize) -> SkewRun {
    let t0 = Instant::now();
    let stats: Vec<(u64, f64, usize, usize)> = match kind {
        "static" => thread::scope(|s| {
            let per = n.div_ceil(workers);
            let hs: Vec<_> = (0..workers)
                .map(|w| {
                    s.spawn(move || {
                        let t = Instant::now();
                        let sum = (w * per..((w + 1) * per).min(n))
                            .fold(0u64, |a, i| a.wrapping_add(row_cost(i, n)));
                        (sum, t.elapsed().as_secs_f64(), 0, 1)
                    })
                })
                .collect();
            hs.into_iter().map(|x| x.join().unwrap()).collect()
        }),
        "dynamic" => {
            let q = Morsels::new(n, 256);
            thread::scope(|s| {
                let hs: Vec<_> = (0..workers)
                    .map(|_| {
                        s.spawn(|| {
                            let (mut sum, mut cnt, t) = (0u64, 0, Instant::now());
                            while let Some(r) = q.take() {
                                cnt += 1;
                                sum = r.fold(sum, |a, i| a.wrapping_add(row_cost(i, n)));
                            }
                            (sum, t.elapsed().as_secs_f64(), 0, cnt)
                        })
                    })
                    .collect();
                hs.into_iter().map(|x| x.join().unwrap()).collect()
            })
        }
        _ => {
            let q = StealQueues::new(workers, n, 256);
            thread::scope(|s| {
                let hs: Vec<_> = (0..workers)
                    .map(|w| {
                        let q = &q;
                        s.spawn(move || {
                            let (mut sum, mut steals, mut cnt, t) = (0u64, 0, 0, Instant::now());
                            while let Some((r, stolen)) = q.take(w) {
                                cnt += 1;
                                steals += stolen as usize;
                                sum = r.fold(sum, |a, i| a.wrapping_add(row_cost(i, n)));
                            }
                            (sum, t.elapsed().as_secs_f64(), steals, cnt)
                        })
                    })
                    .collect();
                hs.into_iter().map(|x| x.join().unwrap()).collect()
            })
        }
    };
    let wall = t0.elapsed().as_secs_f64();
    let busy: Vec<f64> = stats.iter().map(|s| s.1).collect();
    let mean = busy.iter().sum::<f64>() / busy.len() as f64;
    SkewRun {
        result: stats.iter().fold(0u64, |a, s| a.wrapping_add(s.0)),
        wall,
        imbalance: busy.iter().cloned().fold(0.0, f64::max) / mean.max(1e-9),
        steals: stats.iter().map(|s| s.2).sum(),
        morsels: stats.iter().map(|s| s.3).sum(),
    }
}

// ---------------------------------------------------------------- tests and demos
fn differential_queries() {
    let mut rng = Rng(0xA17E_5EED_0720_0001);
    // Small skewed database + tiny morsels => many morsel boundaries; plus a degenerate tiny one.
    let dbs = [gen_db(&mut rng, 3000, true), gen_db(&mut rng, 5, false)];
    let mut counts = HashMap::new();
    for qi in 0..200 {
        let db = &dbs[if qi % 25 == 0 { 1 } else { 0 }];
        let q = random_query(&mut rng);
        let want = run_oracle(db, q);
        for workers in [1, 2, 4, 8] {
            let (got, _) = run_parallel(db, q, workers);
            assert_eq!(got, want, "query {qi} {q:?} with {workers} workers");
        }
        *counts
            .entry(match q {
                Query::Q1 { .. } => "Q1",
                Query::Q3 {
                    mode: Mode::Serial, ..
                } => "Q3/serial",
                Query::Q3 {
                    mode: Mode::Striped,
                    ..
                } => "Q3/striped",
                _ => "Q3/partitioned",
            })
            .or_insert(0) += 1;
    }
    let mut c: Vec<_> = counts.into_iter().collect();
    c.sort();
    println!("200 random queries x workers {{1,2,4,8}} == oracle   mix: {c:?}");
}

fn exchange_tests() {
    let mut rng = Rng(0xE8C4_A46E_0720_0002);
    let db = gen_db(&mut rng, 4000, true);
    let li = &db.li;
    let mut want: BTreeMap<u32, u64> = BTreeMap::new();
    for i in 0..li.orderkey.len() {
        *want.entry(li.orderkey[i]).or_insert(0) += li.price[i] * (100 - li.disc[i]);
    }
    for (p, c) in [(1, 1), (2, 3), (4, 4), (3, 8)] {
        let (got, moved) = exchange_hash_agg(li, p, c);
        assert_eq!(got, want, "hash exchange {p}x{c}");
        assert_eq!(
            moved,
            li.orderkey.len(),
            "every row crosses the exchange exactly once"
        );
    }
    // Broadcast join: orders with an even key are the "small side".
    let small: Vec<(u32, u32)> = (0..db.ord.date.len() as u32)
        .filter(|k| k % 7 == 0)
        .map(|k| (k, db.ord.date[k as usize]))
        .collect();
    let want_sum: u64 = (0..li.orderkey.len())
        .filter(|&i| li.orderkey[i] % 7 == 0)
        .map(|i| li.price[i] * (100 - li.disc[i]))
        .sum();
    for c in [1, 2, 5] {
        let (got, moved) = broadcast_join_sum(li, &small, c);
        assert_eq!(got, want_sum, "broadcast join with {c} consumers");
        assert_eq!(moved, small.len() * c);
    }
    // Sorting: random, heavy duplicates, all-equal, tiny and empty inputs.
    for case in 0..60 {
        let n = [0, 1, 7, 63, 64, 500, 5000][case % 7];
        let modulus = [u64::MAX, 10, 1][case % 3];
        let data: Vec<u64> = (0..n).map(|_| rng.next() % modulus).collect();
        let mut want = data.clone();
        want.sort_unstable();
        for w in [1, 2, 4, 8] {
            assert_eq!(
                sample_sort(&data, w),
                want,
                "sample sort n={n} mod={modulus} w={w}"
            );
            assert_eq!(
                gather_merge_sort(&data, w),
                want,
                "gather-merge n={n} mod={modulus} w={w}"
            );
        }
    }
    println!(
        "exchange: hash-agg, broadcast join, gather-merge and sample sort == serial for all shapes"
    );
}

fn stealing_tests() {
    // Every morsel is processed exactly once: no loss, no duplication, whatever the lane layout.
    for (workers, total, size) in [
        (1, 10, 3),
        (4, 1000, 7),
        (8, 5, 1),
        (3, 0, 4),
        (8, 12345, 100),
    ] {
        let q = StealQueues::new(workers, total, size);
        let hit: Vec<AtomicUsize> = (0..total).map(|_| AtomicUsize::new(0)).collect();
        thread::scope(|s| {
            for w in 0..workers {
                let (q, hit) = (&q, &hit);
                s.spawn(move || {
                    while let Some((r, _)) = q.take(w) {
                        r.for_each(|i| {
                            hit[i].fetch_add(1, Relaxed);
                        });
                    }
                });
            }
        });
        assert!(
            hit.iter().all(|c| c.load(Relaxed) == 1),
            "w={workers} total={total} size={size}"
        );
    }
    println!("stealing dispatcher: every row processed exactly once");
}

fn best_of<T>(n: usize, mut f: impl FnMut() -> (T, f64)) -> (T, f64, f64) {
    let mut best: Option<(T, f64, f64)> = None;
    for _ in 0..n {
        let t = Instant::now();
        let (v, serial) = f();
        let el = t.elapsed().as_secs_f64();
        if best.as_ref().is_none_or(|b| el < b.1) {
            best = Some((v, el, serial));
        }
    }
    best.unwrap()
}

fn amdahl(s: f64, n: usize) -> f64 {
    1.0 / (s + (1.0 - s) / n as f64)
}

fn scaling_tables() {
    let mut rng = Rng(0x5CA1_AB1E_0720_0003);
    let db = gen_db(&mut rng, 600_000, false);
    println!(
        "\nscaling on {} lineitem rows ({} orders), best of 3; serial = seconds spent single-threaded inside the run",
        db.li.orderkey.len(),
        db.ord.date.len()
    );
    let cores = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    println!(
        "available cores: {cores}  (speed-ups cap there; the oracle checks above do not depend on it)"
    );
    let cases = [
        ("Q1 scan+agg      ", Query::Q1 { cut: 2000 }),
        (
            "Q3 build=serial  ",
            Query::Q3 {
                seg: 1,
                date: 1200,
                mode: Mode::Serial,
            },
        ),
        (
            "Q3 build=striped ",
            Query::Q3 {
                seg: 1,
                date: 1200,
                mode: Mode::Striped,
            },
        ),
        (
            "Q3 build=partitnd",
            Query::Q3 {
                seg: 1,
                date: 1200,
                mode: Mode::Partitioned,
            },
        ),
    ];
    for (name, q) in cases {
        let want = run_oracle(&db, q);
        println!("{name}  workers  time_ms  speedup  serial_ms  amdahl_pred");
        let mut t1 = 0.0;
        let mut s_frac = 0.0;
        for w in [1, 2, 4, 8] {
            let (ans, t, ser) = best_of(3, || {
                let (a, s) = run_parallel(&db, q, w);
                (a, s.build + s.merge)
            });
            assert_eq!(ans, want);
            if w == 1 {
                t1 = t;
                s_frac = (ser / t).min(1.0); // serial fraction measured at 1 worker
            }
            println!(
                "{:>19}  {w:>5}  {:>7.1}  {:>6.2}x  {:>9.2}  {:>9.2}x",
                "",
                t * 1e3,
                t1 / t,
                ser * 1e3,
                amdahl(s_frac, w)
            );
        }
    }
}

fn skew_table() {
    let n = 400_000;
    println!(
        "\nskew: first 1/16 of rows cost 100x more; imbalance = max busy / mean busy per worker"
    );
    println!(
        "{:<8} {:>7} {:>9} {:>10} {:>8} {:>8}",
        "kind", "workers", "wall_ms", "imbalance", "morsels", "steals"
    );
    let want = skew_run("static", 1, n).result;
    for w in [4, 8] {
        for kind in ["static", "dynamic", "steal"] {
            let r = skew_run(kind, w, n);
            assert_eq!(r.result, want, "{kind} must compute the same answer");
            println!(
                "{kind:<8} {w:>7} {:>9.1} {:>10.2} {:>8} {:>8}",
                r.wall * 1e3,
                r.imbalance,
                r.morsels,
                r.steals
            );
        }
    }
}

fn sort_and_exchange_demo() {
    let mut rng = Rng(0x1234_5678_0720_0004);
    let data: Vec<u64> = (0..2_000_000).map(|_| rng.next() % 1_000_000_007).collect();
    let mut want = data.clone();
    let t = Instant::now();
    want.sort_unstable();
    println!(
        "\nparallel sort of {} u64 (serial sort_unstable {:.1} ms)",
        data.len(),
        t.elapsed().as_secs_f64() * 1e3
    );
    println!("{:<14} {:>7} {:>9}", "algorithm", "workers", "time_ms");
    for w in [2, 4, 8] {
        for (name, f) in [
            ("sample sort", sample_sort as fn(&[u64], usize) -> Vec<u64>),
            ("local+gather", gather_merge_sort),
        ] {
            let t = Instant::now();
            let got = f(&data, w);
            let el = t.elapsed().as_secs_f64();
            assert_eq!(got, want);
            println!("{name:<14} {w:>7} {:>9.1}", el * 1e3);
        }
    }
    // Exchange traffic: rows moved by broadcast vs shuffle for a small dimension and a large fact table.
    let db = gen_db(&mut rng, 100_000, false);
    let small: Vec<(u32, u32)> = (0..1000u32).map(|k| (k, 0)).collect();
    let consumers = 8;
    let (_, bm) = broadcast_join_sum(&db.li, &small, consumers);
    println!(
        "\nexchange traffic joining {} fact rows with {} dimension rows over {consumers} consumers:",
        db.li.orderkey.len(),
        small.len()
    );
    println!(
        "  broadcast: {bm} rows moved   shuffle both sides: {} rows moved",
        small.len() + db.li.orderkey.len()
    );
    assert!(
        bm < small.len() + db.li.orderkey.len(),
        "broadcasting the small side must be cheaper here"
    );
}

fn main() {
    // Tiny worked example: one Q1 group computed by hand, then the same through the parallel engine.
    let mut rng = Rng(7);
    let db = gen_db(&mut rng, 50, false);
    let (q1, _) = q1_parallel(&db.li, 10_000, 4);
    let rows: u64 = q1.iter().flatten().map(|c| c[3]).sum();
    assert_eq!(
        rows as usize,
        db.li.shipdate.len(),
        "a cutoff past every shipdate keeps every row"
    );
    println!(
        "Q1 groups (flag,status) -> count: {:?}",
        q1.iter()
            .enumerate()
            .flat_map(|(f, r)| r.iter().enumerate().map(move |(s, c)| ((f, s), c[3])))
            .collect::<Vec<_>>()
    );

    differential_queries();
    exchange_tests();
    stealing_tests();
    scaling_tables();
    skew_table();
    sort_and_exchange_demo();
    println!("ok");
}
