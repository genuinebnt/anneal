//! RECIPE 413 (database): column-store encodings, zone maps and vectorised scans over encoded data.
//! LEVEL: advanced
//! Use in: analytical (OLAP) engines, columnar file formats, time-series stores, anything scanning few columns of many rows.   Run: cargo run --release --example columnar_encodings
#![allow(dead_code)]
//!
//! WHAT: a tiny column store. A column is cut into BLOCKS of 1024 values; every block is encoded independently with the
//! cheapest of six encodings and carries a ZONE MAP (min, max, null presence). Encodings (all exact, lossless for the full i64
//! range thanks to wrapping arithmetic and zigzag):
//! * PLAIN      8 bytes/value, the fallback when nothing else wins.
//! * RLE        (value, run length) pairs: wins on sorted or clustered low-cardinality data.
//! * DICT       sorted (order-preserving) dictionary + bit-packed codes. Because code order == value order, `lo <= v <= hi`
//!              becomes `clo <= code < chi`: the predicate is rewritten ONCE per block (two binary searches) and then evaluated
//!              on the packed codes without ever materialising a value. A first-appearance dictionary only supports equality.
//! * FOR        frame of reference: store `v - min` bit-packed; a predicate is shifted into offset space.
//! * DELTA      first value + zigzag-encoded differences, bit-packed: sorted ids, counters.
//! * DELTA2     delta of delta: timestamps with a steady tick have all-zero second differences, i.e. a width of 0 bits.
//! Bit packing uses a `BitWriter` that lets a value straddle two u64 words, so any width 0..=64 works.
//! NULLS live in a separate validity bitmap; null slots hold the previous value so they do not break runs or deltas, and the
//! scan ANDs the selection with the bitmap afterwards (SQL: NULL never satisfies a comparison).
//!
//! WHY: analytics touches few columns of many rows. Column layout reads only the bytes it needs, same-typed neighbours compress
//! far better than mixed rows, and queries can run on the compressed form: less memory traffic is the whole speedup.
//! A ZONE MAP lets a whole block be skipped (no match possible) or accepted wholesale (every row matches) from 16 bytes.
//!
//! HOW the scan works: `Segment::scan(lo, hi)` returns a SELECTION VECTOR (sorted row ids), the currency of vectorised engines:
//! the next operator (project, aggregate, join probe) consumes row ids instead of re-evaluating predicates. Per block:
//! skip / accept-all / scan-encoded-form. RLE filters per RUN (one comparison covers thousands of rows), DICT and FOR compare
//! packed integers, DELTA streams a running sum without allocating. A heuristic chooser computes one-pass statistics
//! (runs, distinct count, min/max, delta widths) and evaluates the exact size formula of each encoding; it never encodes
//! all six, and the test asserts its estimate equals the real encoded size.
//!
//! COMMON USES: Parquet/ORC-style files (dictionary, RLE/bit-packed hybrid, delta encodings, min/max statistics per page and row
//! group), ClickHouse and DuckDB column segments (zone maps / min-max indexes), Gorilla/Prometheus-style timestamp compression
//! (delta-of-delta), Vertica and C-Store projections sorted for RLE, late materialisation via selection vectors.
//! Read: C-Store (Stonebraker et al.) https://people.csail.mit.edu/tdanford/6830papers/stonebraker-cstore.pdf
//!       Parquet file format https://parquet.apache.org/docs/file-format/
//!       Roaring bitmaps (compressed selection sets) https://arxiv.org/abs/1402.6407

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

const BLOCK: usize = 1024;

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
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

// ---------- bit packing ----------
fn bits_needed(x: u64) -> u32 {
    64 - x.leading_zeros()
}
/// Zigzag maps small-magnitude signed numbers to small unsigned ones: 0,-1,1,-2 -> 0,1,2,3.
fn zigzag(x: i64) -> u64 {
    ((x << 1) ^ (x >> 63)) as u64
}
fn unzigzag(z: u64) -> i64 {
    ((z >> 1) as i64) ^ -((z & 1) as i64)
}

/// Appends `width`-bit values back to back; a value may straddle a word boundary.
#[derive(Default)]
struct BitWriter {
    words: Vec<u64>,
    cur: u64,
    nbits: u32,
}
impl BitWriter {
    fn write(&mut self, v: u64, width: u32) {
        if width == 0 {
            return;
        }
        debug_assert!(width == 64 || v >> width == 0);
        if self.nbits < 64 {
            self.cur |= v << self.nbits;
        }
        let total = self.nbits + width;
        if total >= 64 {
            self.words.push(self.cur);
            // The bits that did not fit start the next word (nothing spilled if we were word aligned).
            self.cur = if self.nbits == 0 {
                0
            } else {
                v >> (64 - self.nbits)
            };
            self.nbits = total - 64;
        } else {
            self.nbits = total;
        }
    }
    fn finish(mut self) -> Vec<u64> {
        if self.nbits > 0 {
            self.words.push(self.cur);
        }
        self.words
    }
}

/// Fixed-width bit-packed integer array with random access.
#[derive(Clone, Debug, PartialEq)]
struct Packed {
    words: Vec<u64>,
    width: u32,
    len: usize,
}
impl Packed {
    fn pack(vals: &[u64]) -> Self {
        let width = bits_needed(vals.iter().copied().max().unwrap_or(0));
        let mut w = BitWriter::default();
        vals.iter().for_each(|&v| w.write(v, width));
        Packed {
            words: w.finish(),
            width,
            len: vals.len(),
        }
    }
    fn get(&self, i: usize) -> u64 {
        if self.width == 0 {
            return 0;
        }
        let pos = i * self.width as usize;
        let (word, off) = (pos / 64, (pos % 64) as u32);
        let mut v = self.words[word] >> off;
        if off + self.width > 64 {
            v |= self.words[word + 1] << (64 - off);
        }
        if self.width == 64 {
            v
        } else {
            v & ((1u64 << self.width) - 1)
        }
    }
    fn bytes(&self) -> usize {
        self.words.len() * 8 + 1 // +1: the width byte
    }
    /// Selection-vector scan: row ids whose packed value is in `lo..=hi`, evaluated on the packed form.
    fn scan_range(&self, lo: u64, hi: u64, base: u32, out: &mut Vec<u32>) {
        for i in 0..self.len {
            let v = self.get(i);
            if lo <= v && v <= hi {
                out.push(base + i as u32);
            }
        }
    }
}
fn packed_bytes(items: usize, width: u32) -> usize {
    (items * width as usize).div_ceil(64) * 8 + 1
}

// ---------- encodings ----------
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Plain,
    Rle,
    Dict,
    For,
    Delta,
    Delta2,
}
const ALL: [Kind; 6] = [
    Kind::Plain,
    Kind::Rle,
    Kind::Dict,
    Kind::For,
    Kind::Delta,
    Kind::Delta2,
];

#[derive(Clone, Debug)]
enum Encoded {
    Plain(Vec<i64>),
    Rle {
        vals: Vec<i64>,
        lens: Vec<u32>,
    },
    Dict {
        dict: Vec<i64>,
        codes: Packed,
    },
    For {
        base: i64,
        offs: Packed,
    },
    Delta {
        n: usize,
        first: i64,
        d: Packed,
    },
    Delta2 {
        n: usize,
        first: i64,
        d0: i64,
        dd: Packed,
    },
}

fn deltas(vals: &[i64]) -> Vec<i64> {
    vals.windows(2).map(|w| w[1].wrapping_sub(w[0])).collect()
}

impl Encoded {
    fn encode(kind: Kind, vals: &[i64]) -> Encoded {
        match kind {
            Kind::Plain => Encoded::Plain(vals.to_vec()),
            Kind::Rle => {
                let (mut v, mut l): (Vec<i64>, Vec<u32>) = (vec![], vec![]);
                for &x in vals {
                    if v.last() == Some(&x) {
                        *l.last_mut().unwrap() += 1;
                    } else {
                        v.push(x);
                        l.push(1);
                    }
                }
                Encoded::Rle { vals: v, lens: l }
            }
            Kind::Dict => {
                let mut dict = vals.to_vec();
                dict.sort_unstable();
                dict.dedup();
                let codes: Vec<u64> = vals
                    .iter()
                    .map(|v| dict.binary_search(v).unwrap() as u64)
                    .collect();
                Encoded::Dict {
                    dict,
                    codes: Packed::pack(&codes),
                }
            }
            Kind::For => {
                let base = vals.iter().copied().min().unwrap_or(0);
                let offs: Vec<u64> = vals.iter().map(|v| v.wrapping_sub(base) as u64).collect();
                Encoded::For {
                    base,
                    offs: Packed::pack(&offs),
                }
            }
            Kind::Delta => {
                let zz: Vec<u64> = deltas(vals).into_iter().map(zigzag).collect();
                Encoded::Delta {
                    n: vals.len(),
                    first: vals.first().copied().unwrap_or(0),
                    d: Packed::pack(&zz),
                }
            }
            Kind::Delta2 => {
                let d = deltas(vals);
                let zz: Vec<u64> = d
                    .windows(2)
                    .map(|w| zigzag(w[1].wrapping_sub(w[0])))
                    .collect();
                Encoded::Delta2 {
                    n: vals.len(),
                    first: vals.first().copied().unwrap_or(0),
                    d0: d.first().copied().unwrap_or(0),
                    dd: Packed::pack(&zz),
                }
            }
        }
    }

    fn len(&self) -> usize {
        match self {
            Encoded::Plain(v) => v.len(),
            Encoded::Rle { lens, .. } => lens.iter().map(|&l| l as usize).sum(),
            Encoded::Dict { codes, .. } => codes.len,
            Encoded::For { offs, .. } => offs.len,
            Encoded::Delta { n, .. } | Encoded::Delta2 { n, .. } => *n,
        }
    }

    fn bytes(&self) -> usize {
        match self {
            Encoded::Plain(v) => 8 * v.len(),
            Encoded::Rle { vals, .. } => 12 * vals.len(),
            Encoded::Dict { dict, codes } => 8 * dict.len() + codes.bytes(),
            Encoded::For { offs, .. } => 8 + offs.bytes(),
            Encoded::Delta { d, .. } => 8 + d.bytes(),
            Encoded::Delta2 { dd, .. } => 16 + dd.bytes(),
        }
    }

    /// Visits (row, value) for the two delta encodings by integrating differences on the fly (no allocation).
    fn walk(&self, mut f: impl FnMut(usize, i64)) {
        match self {
            Encoded::Delta { n, first, d } => {
                if *n == 0 {
                    return;
                }
                let mut prev = *first;
                f(0, prev);
                for i in 0..d.len {
                    prev = prev.wrapping_add(unzigzag(d.get(i)));
                    f(i + 1, prev);
                }
            }
            Encoded::Delta2 { n, first, d0, dd } => {
                if *n == 0 {
                    return;
                }
                f(0, *first);
                if *n == 1 {
                    return;
                }
                let mut prev = first.wrapping_add(*d0);
                f(1, prev);
                let mut delta = *d0;
                for i in 0..dd.len {
                    delta = delta.wrapping_add(unzigzag(dd.get(i)));
                    prev = prev.wrapping_add(delta);
                    f(i + 2, prev);
                }
            }
            _ => unreachable!("walk is for delta encodings"),
        }
    }

    fn decode(&self) -> Vec<i64> {
        match self {
            Encoded::Plain(v) => v.clone(),
            Encoded::Rle { vals, lens } => vals
                .iter()
                .zip(lens)
                .flat_map(|(&v, &l)| std::iter::repeat_n(v, l as usize))
                .collect(),
            Encoded::Dict { dict, codes } => (0..codes.len)
                .map(|i| dict[codes.get(i) as usize])
                .collect(),
            Encoded::For { base, offs } => (0..offs.len)
                .map(|i| base.wrapping_add(offs.get(i) as i64))
                .collect(),
            Encoded::Delta { .. } | Encoded::Delta2 { .. } => {
                let mut out = Vec::with_capacity(self.len());
                self.walk(|_, v| out.push(v));
                out
            }
        }
    }

    /// Appends the row ids (offset by `base`) whose value lies in `lo..=hi`, WITHOUT decoding to a value vector.
    fn scan(&self, lo: i64, hi: i64, base: u32, out: &mut Vec<u32>) {
        if lo > hi {
            return;
        }
        match self {
            Encoded::Plain(v) => {
                for (i, &x) in v.iter().enumerate() {
                    if lo <= x && x <= hi {
                        out.push(base + i as u32);
                    }
                }
            }
            // Run-level filtering: one comparison decides a whole run.
            Encoded::Rle { vals, lens } => {
                let mut pos = base;
                for (&v, &l) in vals.iter().zip(lens) {
                    if lo <= v && v <= hi {
                        out.extend(pos..pos + l);
                    }
                    pos += l;
                }
            }
            // Order-preserving dictionary: rewrite the predicate into code space once, compare codes only.
            Encoded::Dict { dict, codes } => {
                let clo = dict.partition_point(|&x| x < lo);
                let chi = dict.partition_point(|&x| x <= hi); // exclusive
                if clo < chi {
                    codes.scan_range(clo as u64, chi as u64 - 1, base, out);
                }
            }
            // FOR: shift the predicate by the frame base (i128 avoids overflow), clamp to the offset domain.
            Encoded::For { base: fb, offs } => {
                let (olo, ohi) = (lo as i128 - *fb as i128, hi as i128 - *fb as i128);
                if ohi >= 0 {
                    offs.scan_range(
                        olo.max(0) as u64,
                        ohi.min(u64::MAX as i128) as u64,
                        base,
                        out,
                    );
                }
            }
            Encoded::Delta { .. } | Encoded::Delta2 { .. } => {
                self.walk(|i, v| {
                    if lo <= v && v <= hi {
                        out.push(base + i as u32);
                    }
                });
            }
        }
    }
}

// ---------- statistics and the heuristic chooser ----------
struct Stats {
    n: usize,
    runs: usize,
    distinct: usize,
    min: i64,
    max: i64,
    delta_w: u32,
    delta2_w: u32,
}
impl Stats {
    fn of(vals: &[i64]) -> Stats {
        let mut sorted = vals.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let d = deltas(vals);
        let max_zz = |it: &mut dyn Iterator<Item = i64>| it.map(zigzag).max().unwrap_or(0);
        let dd_max = max_zz(&mut d.windows(2).map(|w| w[1].wrapping_sub(w[0])));
        Stats {
            n: vals.len(),
            runs: vals.windows(2).filter(|w| w[0] != w[1]).count() + usize::from(!vals.is_empty()),
            distinct: sorted.len(),
            min: vals.iter().copied().min().unwrap_or(0),
            max: vals.iter().copied().max().unwrap_or(0),
            delta_w: bits_needed(max_zz(&mut d.iter().copied())),
            delta2_w: bits_needed(dd_max),
        }
    }
    /// Exact encoded size of `kind` for these statistics (mirrors `Encoded::bytes`).
    fn estimate(&self, kind: Kind) -> usize {
        match kind {
            Kind::Plain => 8 * self.n,
            Kind::Rle => 12 * self.runs,
            Kind::Dict => {
                8 * self.distinct
                    + packed_bytes(self.n, bits_needed(self.distinct.saturating_sub(1) as u64))
            }
            Kind::For => {
                8 + packed_bytes(self.n, bits_needed(self.max.wrapping_sub(self.min) as u64))
            }
            Kind::Delta => 8 + packed_bytes(self.n.saturating_sub(1), self.delta_w),
            Kind::Delta2 => 16 + packed_bytes(self.n.saturating_sub(2), self.delta2_w),
        }
    }
}
/// Smallest estimated size wins; ties go to the earlier (simpler, faster to decode) kind in `ALL`.
fn choose(st: &Stats) -> Kind {
    *ALL.iter().min_by_key(|&&k| st.estimate(k)).unwrap()
}

// ---------- blocks, zone maps, segments ----------
#[derive(Clone, Copy)]
enum Policy {
    Auto,
    Force(Kind),
}

struct Block {
    n: usize,
    min: i64, // zone map over VALID values; min > max means the block is all NULL
    max: i64,
    valid: Option<Vec<u64>>, // None = no nulls
    enc: Encoded,
}
impl Block {
    fn build(chunk: &[Option<i64>], policy: Policy) -> Block {
        let first_valid = chunk.iter().flatten().next().copied().unwrap_or(0);
        let mut prev = first_valid;
        // Null slots repeat the previous value so runs and deltas stay cheap.
        let dense: Vec<i64> = chunk
            .iter()
            .map(|v| {
                prev = v.unwrap_or(prev);
                prev
            })
            .collect();
        let valid_vals = chunk.iter().flatten();
        let (min, max) = valid_vals
            .clone()
            .fold((i64::MAX, i64::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        let has_null = chunk.iter().any(Option::is_none);
        let valid = has_null.then(|| {
            let mut bm = vec![0u64; chunk.len().div_ceil(64)];
            chunk
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_some())
                .for_each(|(i, _)| bm[i / 64] |= 1 << (i % 64));
            bm
        });
        let kind = match policy {
            Policy::Auto => choose(&Stats::of(&dense)),
            Policy::Force(k) => k,
        };
        Block {
            n: chunk.len(),
            min,
            max,
            valid,
            enc: Encoded::encode(kind, &dense),
        }
    }
    fn is_valid(&self, i: usize) -> bool {
        self.valid
            .as_ref()
            .is_none_or(|bm| bm[i / 64] >> (i % 64) & 1 == 1)
    }
    /// Zone-map test: false means NO row of the block can match.
    fn may_match(&self, lo: i64, hi: i64) -> bool {
        lo <= hi && self.min <= self.max && self.min <= hi && lo <= self.max
    }
    fn bytes(&self) -> usize {
        self.enc.bytes() + 16 + self.valid.as_ref().map_or(0, |v| 8 * v.len())
    }
}

#[derive(Default, Debug, Clone, Copy)]
struct ScanStats {
    skipped: usize,
    accepted_whole: usize,
    scanned: usize,
}

struct Segment {
    blocks: Vec<Block>,
    rows: usize,
}
impl Segment {
    fn build(col: &[Option<i64>], policy: Policy) -> Segment {
        Segment {
            blocks: col.chunks(BLOCK).map(|c| Block::build(c, policy)).collect(),
            rows: col.len(),
        }
    }
    fn bytes(&self) -> usize {
        self.blocks.iter().map(Block::bytes).sum()
    }
    fn decode(&self) -> Vec<Option<i64>> {
        let mut out = Vec::with_capacity(self.rows);
        for b in &self.blocks {
            let dense = b.enc.decode();
            out.extend(
                dense
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| b.is_valid(i).then_some(v)),
            );
        }
        out
    }
    /// Selection vector of rows with `lo <= value <= hi` (NULLs never match).
    fn scan(&self, lo: i64, hi: i64) -> (Vec<u32>, ScanStats) {
        let (mut out, mut st) = (Vec::new(), ScanStats::default());
        for (bi, b) in self.blocks.iter().enumerate() {
            let base = (bi * BLOCK) as u32;
            if !b.may_match(lo, hi) {
                st.skipped += 1;
            } else if lo <= b.min && b.max <= hi {
                // Every valid value matches: no need to touch the data at all.
                st.accepted_whole += 1;
                out.extend((0..b.n).filter(|&i| b.is_valid(i)).map(|i| base + i as u32));
            } else {
                st.scanned += 1;
                let start = out.len();
                b.enc.scan(lo, hi, base, &mut out);
                if b.valid.is_some() {
                    let mut keep = out.split_off(start);
                    keep.retain(|&r| b.is_valid((r - base) as usize));
                    out.extend(keep);
                }
            }
        }
        (out, st)
    }
}

fn plain_scan(col: &[Option<i64>], lo: i64, hi: i64) -> Vec<u32> {
    col.iter()
        .enumerate()
        .filter(|(_, v)| v.is_some_and(|x| lo <= x && x <= hi))
        .map(|(i, _)| i as u32)
        .collect()
}

// ---------- data generators ----------
fn gen_dense(rng: &mut Rng, n: usize, shape: u64) -> Vec<i64> {
    let mut v = Vec::with_capacity(n);
    let base = rng.next() as i64 >> rng.below(60);
    match shape {
        0 => v.resize(n, base),
        1 => {
            let pool: Vec<i64> = (0..1 + rng.below(30))
                .map(|_| base.wrapping_add(rng.below(1 << 20) as i64))
                .collect();
            v.extend((0..n).map(|_| pool[rng.below(pool.len() as u64) as usize]));
        }
        2 => {
            let mut cur = base;
            while v.len() < n {
                let run = 1 + rng.below(200) as usize;
                v.extend(std::iter::repeat_n(cur, run.min(n - v.len())));
                cur = cur.wrapping_add(rng.below(5) as i64 - 2);
            }
        }
        3 => {
            let mut cur = base;
            v.extend((0..n).map(|_| {
                cur = cur.wrapping_add(rng.below(10) as i64);
                cur
            }));
        }
        4 => {
            let (step, jitter) = (1 + rng.below(1000) as i64, rng.below(4));
            v.extend(
                (0..n as i64).map(|i| base.wrapping_add(i * step + rng.below(jitter + 1) as i64)),
            );
        }
        5 => v.extend((0..n).map(|_| base.wrapping_add(rng.below(1000) as i64))),
        6 => v.extend((0..n).map(|_| rng.next() as i64)),
        7 => v.extend((0..n).map(|i| if i % 2 == 0 { i64::MIN } else { i64::MAX })), // wrapping stress
        _ => v.extend((0..n).map(|_| [i64::MIN, i64::MAX, 0, -1, 1][rng.below(5) as usize])),
    }
    v
}

fn gen_nullable(rng: &mut Rng) -> Vec<Option<i64>> {
    let n = match rng.below(8) {
        0 => rng.below(4) as usize,
        1 => BLOCK + rng.below(3) as usize - 1, // around a block boundary
        _ => rng.below(3500) as usize,
    };
    let dense = {
        let s = rng.below(9);
        gen_dense(rng, n, s)
    };
    let null_pct = [0, 0, 5, 50, 100][rng.below(5) as usize];
    dense
        .into_iter()
        .map(|v| (!rng.chance(null_pct)).then_some(v))
        .collect()
}

fn random_pred(rng: &mut Rng, col: &[Option<i64>]) -> (i64, i64) {
    let pick = |rng: &mut Rng| {
        col.iter()
            .flatten()
            .nth(rng.below(col.len() as u64) as usize)
            .copied()
    };
    let (a, b) = (pick(rng).unwrap_or(0), pick(rng).unwrap_or(1));
    match rng.below(7) {
        0 => (a, a),                                        // equality
        1 => (a.min(b), a.max(b)),                          // range between two values
        2 => (i64::MIN, a),                                 // <=
        3 => (a, i64::MAX),                                 // >=
        4 => (a.max(b), a.min(b).saturating_sub(1)),        // empty (lo > hi) most of the time
        5 => (i64::MIN, i64::MAX),                          // everything non-null
        _ => (a.saturating_add(1), a.saturating_add(1000)), // just beside a value
    }
}

// ---------- tests ----------
fn test_round_trips(rng: &mut Rng) {
    let mut checked = 0;
    for _ in 0..2000 {
        let n = [0, 1, 2, 3][rng.below(4) as usize].max(if rng.chance(70) {
            rng.below(3000) as usize
        } else {
            0
        });
        let vals = {
            let s = rng.below(9);
            gen_dense(rng, n, s)
        };
        let st = Stats::of(&vals);
        for kind in ALL {
            let enc = Encoded::encode(kind, &vals);
            assert_eq!(enc.decode(), vals, "{kind:?} round trip, n={n}");
            assert_eq!(enc.len(), n);
            assert_eq!(
                enc.bytes(),
                st.estimate(kind),
                "{kind:?} estimate must equal real size"
            );
        }
        let best = ALL
            .iter()
            .map(|&k| Encoded::encode(k, &vals).bytes())
            .min()
            .unwrap();
        assert_eq!(
            st.estimate(choose(&st)),
            best,
            "chooser picks a minimal encoding"
        );
        checked += 1;
    }
    // Segments with nulls and blocks, under every policy.
    for _ in 0..600 {
        let col = gen_nullable(rng);
        for policy in std::iter::once(Policy::Auto).chain(ALL.iter().map(|&k| Policy::Force(k))) {
            assert_eq!(
                Segment::build(&col, policy).decode(),
                col,
                "segment round trip"
            );
        }
    }
    println!(
        "round trips: {checked} random dense columns x 6 encodings, 600 nullable segments x 7 policies: exact"
    );
}

fn test_scans(rng: &mut Rng) {
    let (mut preds, mut skipped, mut whole, mut scanned, mut hits) = (0u64, 0, 0, 0, 0usize);
    for _ in 0..2000 {
        let col = gen_nullable(rng);
        let policy = if rng.chance(50) {
            Policy::Auto
        } else {
            Policy::Force(ALL[rng.below(6) as usize])
        };
        let seg = Segment::build(&col, policy);
        for _ in 0..4 {
            let (lo, hi) = if col.is_empty() {
                (0, 0)
            } else {
                random_pred(rng, &col)
            };
            let expect = plain_scan(&col, lo, hi);
            let (got, st) = seg.scan(lo, hi);
            assert_eq!(
                got, expect,
                "encoded scan must equal plain scan ({policy:?}-ish, {lo}..={hi})"
            );
            // Block skipping never loses a match: a skipped block has no matching row in the oracle.
            for (bi, b) in seg.blocks.iter().enumerate() {
                if !b.may_match(lo, hi) {
                    let range = bi * BLOCK..bi * BLOCK + b.n;
                    assert!(
                        expect.iter().all(|&r| !range.contains(&(r as usize))),
                        "zone map lost a match"
                    );
                }
            }
            (preds, skipped, whole, scanned, hits) = (
                preds + 1,
                skipped + st.skipped,
                whole + st.accepted_whole,
                scanned + st.scanned,
                hits + got.len(),
            );
        }
    }
    println!(
        "scans: {preds} predicates match the plain scan ({hits} rows selected); blocks skipped {skipped}, accepted whole {whole}, scanned {scanned}"
    );
}

impl std::fmt::Debug for Policy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Policy::Auto => write!(f, "auto"),
            Policy::Force(k) => write!(f, "{k:?}"),
        }
    }
}

// ---------- order-preserving dictionary: why order matters ----------
fn demo_dictionaries(rng: &mut Rng) {
    // First-appearance dictionary: equality works in code space, ranges do not.
    let vals = [30i64, 10, 20, 10, 30, 20];
    let mut code_of: HashMap<i64, u32> = HashMap::new();
    let codes: Vec<u32> = vals
        .iter()
        .map(|&v| {
            let next = code_of.len() as u32;
            *code_of.entry(v).or_insert(next)
        })
        .collect();
    let naive: Vec<usize> = (0..vals.len())
        .filter(|&i| codes[i] <= code_of[&20])
        .collect(); // "v <= 20" as "code <= code(20)"
    let truth: Vec<usize> = (0..vals.len()).filter(|&i| vals[i] <= 20).collect();
    assert_ne!(naive, truth, "unordered codes break range predicates");
    println!(
        "unordered dict: `v <= 20` as code<=code(20) selects {naive:?}, truth is {truth:?} (so ranges need an ordered dict)"
    );

    // Order-preserving string dictionary: a range over strings becomes a code range.
    let vocab: Vec<String> = (0..60)
        .map(|i| format!("item-{:02}", (i * 37) % 60))
        .collect();
    let col: Vec<&str> = (0..5000)
        .map(|_| vocab[rng.below(60) as usize].as_str())
        .collect();
    let mut dict: Vec<&str> = col.clone();
    dict.sort_unstable();
    dict.dedup();
    let codes: Vec<u32> = col
        .iter()
        .map(|s| dict.binary_search(s).unwrap() as u32)
        .collect();
    for (lo, hi) in [
        ("item-10", "item-20"),
        ("item-00", "item-01"),
        ("zzz", "zzzz"),
        ("a", "item-05"),
    ] {
        let (clo, chi) = (
            dict.partition_point(|&s| s < lo) as u32,
            dict.partition_point(|&s| s < hi) as u32,
        );
        let got: Vec<usize> = (0..col.len())
            .filter(|&i| clo <= codes[i] && codes[i] < chi)
            .collect();
        let expect: Vec<usize> = (0..col.len())
            .filter(|&i| lo <= col[i] && col[i] < hi)
            .collect();
        assert_eq!(got, expect);
    }
    println!(
        "string dict: {} distinct of {} rows; 4 string range predicates answered in code space, all equal to the string scan",
        dict.len(),
        col.len()
    );
}

// ---------- size table ----------
fn size_table(rng: &mut Rng) {
    const N: usize = 200_000;
    let mut cur = 1_000_000i64;
    let shapes: Vec<(&str, Vec<i64>)> = vec![
        ("low-card random (40)", {
            let pool: Vec<i64> = (0..40).map(|_| rng.below(1_000_000_000) as i64).collect();
            (0..N).map(|_| pool[rng.below(40) as usize]).collect()
        }),
        (
            "sorted categories",
            (0..N).map(|i| (i / 5000) as i64 * 1000).collect(),
        ),
        (
            "sorted ids, gaps 1..8",
            (0..N)
                .map(|_| {
                    cur += 1 + rng.below(8) as i64;
                    cur
                })
                .collect(),
        ),
        (
            "timestamps, exact 1s tick",
            (0..N as i64)
                .map(|i| 1_700_000_000_000 + i * 1000)
                .collect(),
        ),
        (
            "timestamps, jitter 0..4ms",
            (0..N as i64)
                .map(|i| 1_700_000_000_000 + i * 1000 + rng.below(5) as i64)
                .collect(),
        ),
        (
            "random 0..1000",
            (0..N).map(|_| rng.below(1000) as i64).collect(),
        ),
        (
            "random u32",
            (0..N).map(|_| rng.below(1 << 32) as i64).collect(),
        ),
        ("random i64", (0..N).map(|_| rng.next() as i64).collect()),
    ];
    println!(
        "\ncompressed bytes/value ({N} rows, blocks of {BLOCK}, incl. 16 B zone map per block):"
    );
    println!(
        "{:<28}{:>7}{:>7}{:>7}{:>7}{:>7}{:>7}{:>8}  auto picks",
        "shape", "plain", "rle", "dict", "for", "delta", "delta2", "AUTO"
    );
    for (name, vals) in &shapes {
        let col: Vec<Option<i64>> = vals.iter().map(|&v| Some(v)).collect();
        let size = |p| Segment::build(&col, p).bytes();
        let fixed: Vec<usize> = ALL.iter().map(|&k| size(Policy::Force(k))).collect();
        let seg = Segment::build(&col, Policy::Auto);
        let auto = seg.bytes();
        assert!(
            fixed.iter().all(|&f| auto <= f),
            "per-block auto choice can never lose to one fixed encoding"
        );
        assert_eq!(seg.decode(), col);
        let mut picks: HashMap<String, usize> = HashMap::new();
        for b in &seg.blocks {
            let k = match b.enc {
                Encoded::Plain(_) => "plain",
                Encoded::Rle { .. } => "rle",
                Encoded::Dict { .. } => "dict",
                Encoded::For { .. } => "for",
                Encoded::Delta { .. } => "delta",
                Encoded::Delta2 { .. } => "delta2",
            };
            *picks.entry(k.to_string()).or_default() += 1;
        }
        let mut picks: Vec<_> = picks.into_iter().collect();
        picks.sort();
        let bpv = |b: usize| b as f64 / N as f64;
        println!(
            "{:<28}{:>7.2}{:>7.2}{:>7.2}{:>7.2}{:>7.2}{:>7.2}{:>8.3}  {:?}",
            name,
            bpv(fixed[0]),
            bpv(fixed[1]),
            bpv(fixed[2]),
            bpv(fixed[3]),
            bpv(fixed[4]),
            bpv(fixed[5]),
            bpv(auto),
            picks
        );
        match *name {
            "timestamps, exact 1s tick" => {
                assert!(bpv(auto) < 0.1, "steady tick collapses to ~0 bits")
            }
            "sorted categories" => assert!(bpv(fixed[1]) < 0.05, "RLE wins on sorted categories"),
            "low-card random (40)" => assert!(bpv(auto) < 1.3 && bpv(auto) < bpv(fixed[0]) / 5.0),
            "random i64" => assert!(bpv(auto) <= 8.1, "incompressible data costs ~plain"),
            "random 0..1000" => assert!(bpv(auto) < 1.4),
            _ => {}
        }
    }
}

// ---------- layout and speed demos ----------
#[derive(Clone, Copy)]
struct Row {
    id: i64,
    a: i64,
    b: i64,
    c: i64,
    d: i64,
    e: i64,
    f: i64,
    g: i64,
}

fn demo_layout() {
    const N: usize = 2_000_000;
    let rows: Vec<Row> = (0..N as i64)
        .map(|i| Row {
            id: i,
            a: i,
            b: i,
            c: i % 97,
            d: i,
            e: i,
            f: i,
            g: i,
        })
        .collect();
    let col_c: Vec<i64> = rows.iter().map(|r| r.c).collect();
    let t = Instant::now();
    let s_row: i64 = black_box(&rows).iter().map(|r| r.c).sum();
    let t_row = t.elapsed();
    let t = Instant::now();
    let s_col: i64 = black_box(&col_c).iter().sum();
    let t_col = t.elapsed();
    assert_eq!(s_row, s_col);
    println!(
        "layout: SUM(c) over {N} rows: row store streams {} MB in {:.2} ms, column streams {} MB in {:.2} ms",
        N * 64 / 1_000_000,
        t_row.as_secs_f64() * 1e3,
        N * 8 / 1_000_000,
        t_col.as_secs_f64() * 1e3
    );
}

fn time_scan(label: &str, col: &[Option<i64>], seg: &Segment, lo: i64, hi: i64) {
    let reps = 5;
    let t = Instant::now();
    let mut n_plain = 0;
    for _ in 0..reps {
        n_plain = black_box(plain_scan(black_box(col), lo, hi)).len();
    }
    let t_plain = t.elapsed().as_secs_f64() / reps as f64;
    let t = Instant::now();
    let mut stats = ScanStats::default();
    let mut n_enc = 0;
    for _ in 0..reps {
        let (sel, st) = seg.scan(black_box(lo), black_box(hi));
        n_enc = black_box(sel).len();
        stats = st;
    }
    let t_enc = t.elapsed().as_secs_f64() / reps as f64;
    assert_eq!(n_plain, n_enc);
    let rows = col.len() as f64;
    println!(
        "  {label:<26} sel={n_enc:>8} plain {:>7.1} Mrows/s | encoded {:>8.1} Mrows/s | blocks skip/whole/scan {}/{}/{}  {:.2} B/val",
        rows / t_plain / 1e6,
        rows / t_enc / 1e6,
        stats.skipped,
        stats.accepted_whole,
        stats.scanned,
        seg.bytes() as f64 / rows
    );
}

fn demo_scan_speed(rng: &mut Rng) {
    const N: usize = 2_000_000;
    println!("\nscan speed over {N} rows (timings are machine dependent and only printed):");
    let lowcard: Vec<Option<i64>> = (0..N).map(|_| Some(rng.below(100) as i64 * 7)).collect();
    let runs: Vec<Option<i64>> = (0..N).map(|i| Some((i / 700) as i64 % 50)).collect();
    let sorted: Vec<Option<i64>> = (0..N as i64).map(|i| Some(i * 3)).collect();
    let nulls: Vec<Option<i64>> = (0..N)
        .map(|_| (!rng.chance(20)).then(|| rng.below(5000) as i64))
        .collect();
    for (label, col, lo, hi) in [
        ("low-card, 5% selective", &lowcard, 0, 28),
        ("clustered runs (RLE)", &runs, 10, 14),
        ("sorted, narrow range", &sorted, 3_000_000, 3_003_000),
        ("20% nulls, 10% range", &nulls, 1000, 1499),
    ] {
        let seg = Segment::build(col, Policy::Auto);
        time_scan(label, col, &seg, lo, hi);
    }
    // Zone maps on random vs sorted data: a narrow predicate prunes everything on sorted data, nothing on random.
    let seg_sorted = Segment::build(&sorted, Policy::Auto);
    let (_, st) = seg_sorted.scan(3_000_000, 3_003_000);
    assert!(
        st.skipped as f64 > 0.99 * seg_sorted.blocks.len() as f64,
        "sorted data prunes nearly all blocks"
    );
    let seg_rand = Segment::build(&lowcard, Policy::Auto);
    let (_, st2) = seg_rand.scan(0, 28);
    assert_eq!(st2.skipped, 0, "unclustered data defeats zone maps");
    println!(
        "zone maps: sorted column skips {}/{} blocks, random column skips {}/{}",
        st.skipped,
        seg_sorted.blocks.len(),
        st2.skipped,
        seg_rand.blocks.len()
    );
}

fn main() {
    let mut rng = Rng(0x9E3779B97F4A7C15);

    // Bit packing edge cases first: widths 0, 1, 63, 64 and straddling word boundaries.
    for vals in [
        vec![],
        vec![0u64; 10],
        vec![1, 0, 1, 1],
        vec![u64::MAX, 0, u64::MAX],
        vec![(1 << 63) - 1, 5, 1 << 62],
    ] {
        let p = Packed::pack(&vals);
        assert!(
            (0..vals.len()).all(|i| p.get(i) == vals[i]),
            "pack/get {vals:?} width {}",
            p.width
        );
    }
    assert!(
        (-5i64..5)
            .chain([i64::MIN, i64::MAX])
            .all(|x| unzigzag(zigzag(x)) == x)
    );
    println!("zigzag(0,-1,1,-2) = {:?}", [0, -1, 1, -2].map(zigzag));

    demo_layout();
    test_round_trips(&mut rng);
    test_scans(&mut rng);
    demo_dictionaries(&mut rng);
    size_table(&mut rng);
    demo_scan_speed(&mut rng);
    println!("ok");
}
