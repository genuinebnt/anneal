//! RECIPE 721 (database): in-memory hash joins: no-partition join vs radix-partitioned join vs sort-merge.
//! LEVEL: advanced
//! Use in: analytical engines and dataframe libraries whose hash joins outgrow the CPU caches.   Run: cargo run --release --example radix_partitioned_hash_join
//! COMMON USES: main-memory OLAP engines (MonetDB's radix-cluster join, the research lineage behind HyPer/Umbra and DuckDB's
//! partitioned hash join), the "partition both sides first" step of GRACE/hybrid hash joins (on disk the same idea is used
//! with pages instead of cache lines), shuffle joins in distributed engines, radix sort.
//! Course: CMU 15-721 (Advanced Database Systems): "To partition or not to partition" hash-join lecture (parallel/in-memory joins).
#![allow(dead_code)]
//!
//! WHAT: the equi-join R(key, payload) JOIN S(key, payload) with three in-memory algorithms, all emitting (key, r_payload,
//! s_payload) and all compared on identical inputs:
//! * NO-PARTITION join (NPJ): build ONE hash table over R, probe it with every tuple of S. Minimal work: two passes over the data.
//! * RADIX-PARTITIONED join: first split BOTH inputs by the low `bits` bits of a hash of the key into 2^bits partitions, then
//!   join partition i of R with partition i of S using a small private hash table. Equal keys have equal hashes, so matching
//!   tuples always land in the same partition pair.
//! * SORT-MERGE: sort both inputs on the key and merge equal-key groups (cross product per group).
//!
//! HOW the partitioning works (one pass): (1) HISTOGRAM: count tuples per digit; (2) PREFIX SUM turns counts into start offsets,
//! so every partition owns an exact slice of the output and no resizing or linked lists are needed; (3) SCATTER: each tuple is
//! written to the next free slot of its partition. Multi-pass: a second pass refines every partition of the first by the NEXT
//! bits of the hash. `Cfg { bits_per_pass }` parameterises radix bits and passes; the final partition order is identical for R and S.
//! The table inside a partition indexes buckets with hash bits ABOVE the radix bits; reusing the radix bits would put every
//! tuple of a partition into the same bucket (they all share them), a classic bug.
//! SOFTWARE WRITE-COMBINING (`wc = true`): the scatter writes to `fanout` different places, i.e. touches `fanout` cache lines
//! and as many TLB pages at once. With a few hundred partitions that exceeds the L1 data cache and the L1 TLB, and every store
//! becomes a miss. Instead, each partition owns a small buffer of one cache line (8 tuples of 8 bytes); tuples are collected
//! there and flushed to the destination as a full line. In C this is combined with non-temporal stores that bypass the cache;
//! here the flush is a slice copy, which keeps the same access pattern.
//!
//! WHY partition at all (cost model, N = |R| build tuples, 8 bytes each, plus a 4-byte chain link and a 4-byte bucket head):
//! * NPJ probes a table of ~16*N bytes at RANDOM positions. While the table fits the L2 (a few hundred KB to some MB) a probe
//!   costs a few ns. Once it exceeds the last-level cache every probe is a cache miss (~80-100 ns) AND a TLB miss (the table
//!   spans far more 4 KB pages than the TLB holds), so cost per probe jumps by an order of magnitude. Out-of-order execution
//!   hides some of it by overlapping independent probes.
//! * Radix join pays up to `passes` extra sequential read+write passes over BOTH inputs (cheap: bandwidth-bound, prefetcher
//!   friendly), then every partition's table (<= ~64 KB) is cache resident, so build and probe run at cache speed.
//! * Fan-out per pass is bounded by the TLB/L1 (rule of thumb <= 2^6..2^8): more bits are reached with several passes
//!   (cost: one more full pass each) rather than one huge fan-out.
//! * Crossover: small inputs -> NPJ wins (partitioning is pure overhead); beyond the cache -> radix wins, in the original
//!   studies by 2x or more. On modern hardware with big caches, hardware prefetchers and huge pages, NPJ is much more
//!   competitive than the early papers suggested (that is the "to partition or not" debate), so the printed table is
//!   machine-specific and the example only asserts results, never times.
//! * Skew: a hot key piles into one partition; hashing cannot split equal keys, so that partition (and its join output) stays big,
//!   but the algorithm remains correct. Sort-merge is immune to hash collisions but pays O(n log n) sorting.
//!
//! CHECKS: 300 random input pairs (empty, tiny, skewed hot key, heavy duplicates, tiny and wide key domains), random radix
//! settings (0..9 bits, 1..3 passes, write-combining on/off) must give multiset-identical results for NPJ, radix and sort-merge
//! against an independent BTreeMap oracle; partition invariants (permutation of the input, one hash-digit value per partition,
//! distinct per partition) are checked; plain and write-combined scatter must produce byte-identical partitions.
//! Read: Multi-Core, Main-Memory Joins: Sort vs. Hash Revisited https://www.vldb.org/pvldb/vol7/p85-balkesen.pdf
//!       Hash join https://en.wikipedia.org/wiki/Hash_join   CMU 15-721 https://15721.courses.cs.cmu.edu/spring2024/

use std::collections::BTreeMap;
use std::time::Instant;

type T = (u32, u32); // (join key, payload)
type Out = (u32, u32, u32); // (key, r payload, s payload)

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

/// 64-bit finalizer-style mixer. Radix partitioning consumes LOW hash bits, so they must be well mixed: with an identity hash,
/// keys that are multiples of 1024 would all share digit 0 and one partition would receive everything.
fn hash(k: u32) -> u64 {
    let mut x = k as u64 ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

// ---------------------------------------------------------------- hash table (chained, array based)
/// Bucket heads + `next` links in two flat arrays (no per-node allocation): the layout a cache-conscious join uses.
/// Entries are tuple index + 1 so that 0 means "empty".
struct Table<'a> {
    t: &'a [T],
    head: Vec<u32>,
    next: Vec<u32>,
    mask: usize,
    shift: u32,
}
impl<'a> Table<'a> {
    /// `shift` = number of hash bits already consumed by radix partitioning (0 for the no-partition join).
    fn build(t: &'a [T], shift: u32) -> Self {
        let cap = t.len().next_power_of_two().max(1);
        let mut tab = Table {
            t,
            head: vec![0; cap],
            next: vec![0; t.len()],
            mask: cap - 1,
            shift,
        };
        for (i, &(k, _)) in t.iter().enumerate() {
            let b = tab.bucket(k);
            tab.next[i] = tab.head[b];
            tab.head[b] = i as u32 + 1;
        }
        tab
    }
    fn bucket(&self, k: u32) -> usize {
        (hash(k) >> self.shift) as usize & self.mask
    }
    fn probe(&self, k: u32, mut f: impl FnMut(u32)) {
        let mut p = self.head[self.bucket(k)];
        while p != 0 {
            let (rk, rp) = self.t[p as usize - 1];
            if rk == k {
                f(rp);
            }
            p = self.next[p as usize - 1];
        }
    }
}

// ---------------------------------------------------------------- partitioning
const WC: usize = 8; // tuples per write-combining buffer = one 64-byte cache line

/// One partitioning pass over `src` into `dst` (same length) by hash bits [shift, shift+bits).
/// Returns offsets (len fanout+1) of the partitions inside `dst`. Both variants are STABLE, hence produce identical output.
fn partition_pass(src: &[T], dst: &mut [T], shift: u32, bits: u32, wc: bool) -> Vec<usize> {
    let fan = 1usize << bits;
    let digit = |k: u32| (hash(k) >> shift) as usize & (fan - 1);
    // 1. histogram
    let mut offsets = vec![0usize; fan + 1];
    for &(k, _) in src {
        offsets[digit(k) + 1] += 1;
    }
    // 2. prefix sum: counts -> start offsets
    for i in 0..fan {
        offsets[i + 1] += offsets[i];
    }
    // 3. scatter
    let mut pos = offsets[..fan].to_vec();
    if !wc {
        for &t in src {
            let d = digit(t.0);
            dst[pos[d]] = t;
            pos[d] += 1;
        }
    } else {
        let mut buf = vec![[(0u32, 0u32); WC]; fan];
        let mut fill = vec![0usize; fan];
        for &t in src {
            let d = digit(t.0);
            buf[d][fill[d]] = t;
            fill[d] += 1;
            if fill[d] == WC {
                dst[pos[d]..pos[d] + WC].copy_from_slice(&buf[d]); // one full-line write instead of 8 scattered ones
                pos[d] += WC;
                fill[d] = 0;
            }
        }
        for d in 0..fan {
            dst[pos[d]..pos[d] + fill[d]].copy_from_slice(&buf[d][..fill[d]]);
        }
    }
    offsets
}

#[derive(Clone, Debug)]
struct Cfg {
    bits_per_pass: Vec<u32>, // empty = no partitioning (one partition)
    wc: bool,
}
impl Cfg {
    fn total_bits(&self) -> u32 {
        self.bits_per_pass.iter().sum()
    }
    /// Spread `bits` over `passes` passes as evenly as possible.
    fn spread(bits: u32, passes: u32, wc: bool) -> Cfg {
        if bits == 0 {
            return Cfg {
                bits_per_pass: vec![],
                wc,
            };
        }
        let passes = passes.clamp(1, bits);
        let v = (0..passes)
            .map(|i| bits / passes + u32::from(i < bits % passes))
            .collect();
        Cfg {
            bits_per_pass: v,
            wc,
        }
    }
}

/// Multi-pass radix partitioning. Returns the reordered tuples and partition boundaries (len = partitions + 1).
/// Pass p refines every partition from pass p-1 using the next hash bits; the bounds list is ordered the same way for any input.
fn radix_partition(src: &[T], cfg: &Cfg) -> (Vec<T>, Vec<usize>) {
    let n = src.len();
    let mut cur = src.to_vec();
    let mut tmp = vec![(0u32, 0u32); n];
    let mut bounds = vec![0, n];
    let mut shift = 0;
    for &bits in &cfg.bits_per_pass {
        let mut nb = vec![0];
        for w in bounds.windows(2) {
            let (lo, hi) = (w[0], w[1]);
            let offs = partition_pass(&cur[lo..hi], &mut tmp[lo..hi], shift, bits, cfg.wc);
            nb.extend(offs[1..].iter().map(|o| lo + o));
        }
        std::mem::swap(&mut cur, &mut tmp);
        bounds = nb;
        shift += bits;
    }
    (cur, bounds)
}

// ---------------------------------------------------------------- the three joins
fn no_partition_join(r: &[T], s: &[T], mut sink: impl FnMut(u32, u32, u32)) {
    let tab = Table::build(r, 0);
    for &(k, sp) in s {
        tab.probe(k, |rp| sink(k, rp, sp));
    }
}

fn radix_join(r: &[T], s: &[T], cfg: &Cfg, mut sink: impl FnMut(u32, u32, u32)) {
    let (rp, rb) = radix_partition(r, cfg);
    let (sp, sb) = radix_partition(s, cfg);
    assert_eq!(rb.len(), sb.len());
    let shift = cfg.total_bits();
    for p in 0..rb.len() - 1 {
        let (rpart, spart) = (&rp[rb[p]..rb[p + 1]], &sp[sb[p]..sb[p + 1]]);
        if rpart.is_empty() || spart.is_empty() {
            continue; // nothing can match: skip building
        }
        let tab = Table::build(rpart, shift);
        for &(k, sv) in spart {
            tab.probe(k, |rv| sink(k, rv, sv));
        }
    }
}

fn sort_merge_join(r: &[T], s: &[T], mut sink: impl FnMut(u32, u32, u32)) {
    let (mut r, mut s) = (r.to_vec(), s.to_vec());
    r.sort_unstable_by_key(|t| t.0);
    s.sort_unstable_by_key(|t| t.0);
    let (mut i, mut j) = (0, 0);
    while i < r.len() && j < s.len() {
        match r[i].0.cmp(&s[j].0) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                let k = r[i].0;
                let ie = i + r[i..].iter().take_while(|t| t.0 == k).count();
                let je = j + s[j..].iter().take_while(|t| t.0 == k).count();
                for a in &r[i..ie] {
                    for b in &s[j..je] {
                        sink(k, a.1, b.1);
                    }
                }
                (i, j) = (ie, je);
            }
        }
    }
}

fn collect(f: impl FnOnce(&mut dyn FnMut(u32, u32, u32))) -> Vec<Out> {
    let mut v = vec![];
    f(&mut |k, a, b| v.push((k, a, b)));
    v.sort_unstable();
    v
}

/// Independent oracle: group R by key in a BTreeMap, then look every S tuple up.
fn oracle(r: &[T], s: &[T]) -> Vec<Out> {
    let mut m: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &(k, p) in r {
        m.entry(k).or_default().push(p);
    }
    let mut v: Vec<Out> = vec![];
    for &(k, sp) in s {
        if let Some(rs) = m.get(&k) {
            v.extend(rs.iter().map(|&rp| (k, rp, sp)));
        }
    }
    v.sort_unstable();
    v
}

// ---------------------------------------------------------------- tests
fn gen_rel(rng: &mut Rng, n: usize, domain: u64, hot: bool) -> Vec<T> {
    (0..n)
        .map(|i| {
            let k = if hot && rng.below(10) < 3 {
                7
            } else {
                rng.below(domain) as u32
            };
            (k, i as u32) // payload = row id, so a lost or duplicated tuple changes the multiset
        })
        .collect()
}

fn check_partition(src: &[T], cfg: &Cfg) {
    let (out, bounds) = radix_partition(src, cfg);
    let mut a = src.to_vec();
    let mut b = out.clone();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "partitioning must permute the input");
    let mask = (1u64 << cfg.total_bits()) - 1;
    assert_eq!(bounds.len(), (1usize << cfg.total_bits()) + 1);
    let mut seen = std::collections::HashSet::new();
    for w in bounds.windows(2) {
        let digits: std::collections::HashSet<u64> =
            out[w[0]..w[1]].iter().map(|t| hash(t.0) & mask).collect();
        assert!(
            digits.len() <= 1,
            "one partition holds a single radix value"
        );
        for d in digits {
            assert!(seen.insert(d), "two partitions share a radix value");
        }
    }
    // Write-combining changes the memory access pattern, never the result.
    let plain = radix_partition(
        src,
        &Cfg {
            wc: false,
            ..cfg.clone()
        },
    );
    let comb = radix_partition(
        src,
        &Cfg {
            wc: true,
            ..cfg.clone()
        },
    );
    assert_eq!(plain, comb);
}

fn differential_test() {
    let mut rng = Rng(0x7A11_ED00_0721_0001);
    let mut total_out = 0usize;
    for case in 0..300 {
        let (nr, ns) = [
            (0, 0),
            (0, 5),
            (5, 0),
            (1, 1),
            (37, 211),
            (400, 400),
            (700, 90),
            (90, 700),
        ][case % 8];
        let domain = [1u64, 8, 100, 100_000, 1 << 20][rng.below(5) as usize];
        let hot = rng.below(3) == 0;
        let r = gen_rel(&mut rng, nr, domain, hot);
        let s = gen_rel(&mut rng, ns, domain, hot);
        let want = oracle(&r, &s);
        total_out += want.len();
        assert_eq!(
            collect(|k| no_partition_join(&r, &s, k)),
            want,
            "npj case {case}"
        );
        assert_eq!(
            collect(|k| sort_merge_join(&r, &s, k)),
            want,
            "smj case {case}"
        );
        let cfg = Cfg::spread(
            rng.below(10) as u32,
            1 + rng.below(3) as u32,
            rng.below(2) == 0,
        );
        assert_eq!(
            collect(|k| radix_join(&r, &s, &cfg, k)),
            want,
            "radix {cfg:?} case {case}"
        );
        check_partition(&r, &cfg);
    }
    println!(
        "300 random pairs: NPJ == radix == sort-merge == oracle ({total_out} output tuples in total)"
    );
}

// ---------------------------------------------------------------- timing
fn time_join(
    r: &[T],
    s: &[T],
    reps: usize,
    f: impl Fn(&mut dyn FnMut(u32, u32, u32)),
) -> (f64, u64, u64) {
    let mut best = f64::MAX;
    let (mut count, mut sum) = (0u64, 0u64);
    for _ in 0..reps {
        let t = Instant::now();
        (count, sum) = (0, 0);
        f(&mut |k, a, b| {
            count += 1;
            sum = sum.wrapping_add(k as u64 * 31 + a as u64 * 17 + b as u64);
        });
        best = best.min(t.elapsed().as_secs_f64());
    }
    let _ = (r, s);
    (best, count, sum)
}

/// FK-style workload: R has unique keys (shuffled), S draws keys from R uniformly, so output = |S|.
fn fk_workload(rng: &mut Rng, n: usize) -> (Vec<T>, Vec<T>) {
    let mut keys: Vec<u32> = (0..n as u32)
        .map(|k| k.wrapping_mul(2_654_435_761))
        .collect();
    for i in (1..n).rev() {
        keys.swap(i, rng.below(i as u64 + 1) as usize);
    }
    let r: Vec<T> = keys
        .iter()
        .enumerate()
        .map(|(i, &k)| (k, i as u32))
        .collect();
    let s: Vec<T> = (0..n)
        .map(|i| (keys[rng.below(n as u64) as usize], i as u32))
        .collect();
    (r, s)
}

fn crossover_table() {
    let mut rng = Rng(0xC0FF_EE00_0721_0002);
    println!(
        "\ntime per probe tuple (ns), |R| = |S| = 2^k, unique build keys; radix aims at ~2^12 build tuples per partition"
    );
    println!(
        "{:>3} {:>10} {:>9} {:>8} {:>10} {:>10} {:>10}",
        "k", "R bytes", "radix cfg", "NPJ", "radix", "radix+wc", "sort-merge"
    );
    for k in [10u32, 12, 14, 16, 18, 20, 22] {
        let n = 1usize << k;
        let (r, s) = fk_workload(&mut rng, n);
        let bits = k.saturating_sub(12);
        let passes = bits.div_ceil(7).max(1); // fan-out <= 2^7 per pass keeps the scatter inside the TLB
        let cfg = Cfg::spread(bits, passes, false);
        let cfgw = Cfg {
            wc: true,
            ..cfg.clone()
        };
        let reps = if k <= 18 { 5 } else { 2 };
        let (t0, c0, h0) = time_join(&r, &s, reps, |f| no_partition_join(&r, &s, f));
        let (t1, c1, h1) = time_join(&r, &s, reps, |f| radix_join(&r, &s, &cfg, f));
        let (t2, c2, h2) = time_join(&r, &s, reps, |f| radix_join(&r, &s, &cfgw, f));
        let (t3, c3, h3) = if k <= 20 {
            time_join(&r, &s, reps, |f| sort_merge_join(&r, &s, f))
        } else {
            (f64::NAN, c0, h0)
        };
        assert!(
            c0 == n as u64 && c1 == c0 && c2 == c0 && c3 == c0 && h1 == h0 && h2 == h0 && h3 == h0,
            "all algorithms agree at k={k}"
        );
        let ns = |t: f64| t * 1e9 / n as f64;
        let label = format!("{}b/{}p", cfg.total_bits(), cfg.bits_per_pass.len());
        println!(
            "{k:>3} {:>10} {label:>9} {:>8.1} {:>10.1} {:>10.1} {:>10.1}",
            n * 8,
            ns(t0),
            ns(t1),
            ns(t2),
            ns(t3)
        );
    }
}

fn sweep_table() {
    let mut rng = Rng(0x5EED_0000_0721_0003);
    let n = 1usize << 21;
    let (r, s) = fk_workload(&mut rng, n);
    let (tb, cb, hb) = time_join(&r, &s, 1, |f| no_partition_join(&r, &s, f));
    println!(
        "\nradix bits x passes sweep at |R| = |S| = 2^21 (NPJ baseline {:.1} ns/probe tuple); ns per probe tuple:",
        tb * 1e9 / n as f64
    );
    println!(
        "{:>5} {:>9} {:>9} {:>9}",
        "bits", "1 pass", "2 passes", "3 passes"
    );
    for bits in [4u32, 8, 10, 12, 14] {
        let mut row = vec![];
        for passes in 1..=3 {
            let cfg = Cfg::spread(bits, passes, true);
            let (t, c, h) = time_join(&r, &s, 1, |f| radix_join(&r, &s, &cfg, f));
            assert!(c == cb && h == hb);
            row.push(format!("{:>9.1}", t * 1e9 / n as f64));
        }
        println!("{bits:>5} {}", row.join(" "));
    }
    println!(
        "(many partitions in one pass scatter into many streams; on small-TLB machines extra passes pay off, on this one compare the columns yourself)"
    );
}

fn skew_demo() {
    let mut rng = Rng(0xBAD5_EED0_0721_0004);
    // Hot key: 50% of both inputs share key 7. Output is quadratic in the hot group, so keep it moderate.
    let n = 6000usize;
    let mk = |rng: &mut Rng| -> Vec<T> {
        (0..n)
            .map(|i| {
                (
                    if rng.below(2) == 0 {
                        7
                    } else {
                        rng.below(1_000_000) as u32
                    },
                    i as u32,
                )
            })
            .collect()
    };
    let (r, s) = (mk(&mut rng), mk(&mut rng));
    let cfg = Cfg::spread(8, 2, true);
    let (rp, rb) = radix_partition(&r, &cfg);
    let sizes: Vec<usize> = rb.windows(2).map(|w| w[1] - w[0]).collect();
    let biggest = *sizes.iter().max().unwrap();
    println!(
        "\nskew: 256 partitions over {} tuples, largest partition {} (a fair share would be ~{}): the hot key cannot be split",
        rp.len(),
        biggest,
        n / 256
    );
    assert!(
        biggest >= n / 2 - 200,
        "the hot key keeps its tuples together"
    );
    let want = oracle(&r, &s);
    assert_eq!(collect(|k| radix_join(&r, &s, &cfg, k)), want);
    println!(
        "join output {} tuples, still identical to the oracle",
        want.len()
    );
}

fn main() {
    // Worked example: the hash digits decide partitions, matching keys meet in the same partition pair.
    let r: Vec<T> = vec![(1, 10), (2, 20), (2, 21), (4, 40)];
    let s: Vec<T> = vec![(2, 200), (2, 201), (3, 300), (4, 400)];
    let cfg = Cfg::spread(2, 1, true);
    let (_, rb) = radix_partition(&r, &cfg);
    println!(
        "R split into {} partitions with sizes {:?}",
        rb.len() - 1,
        rb.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>()
    );
    let out = collect(|k| radix_join(&r, &s, &cfg, k));
    println!("join: {out:?}");
    assert_eq!(
        out.len(),
        2 * 2 + 1,
        "key 2 is duplicated on both sides (2x2), key 4 matches once"
    );

    differential_test();
    skew_demo();
    crossover_table();
    sweep_table();
    println!("ok");
}
