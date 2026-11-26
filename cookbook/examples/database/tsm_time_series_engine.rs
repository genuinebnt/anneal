//! RECIPE 861 (database): a TSM/Prometheus-TSDB-style time-series engine: tag index with postings, WAL + head, immutable blocks, compaction, retention, tombstones, downsampling queries.
//! LEVEL: advanced
//! Use in: understanding how InfluxDB (TSM), Prometheus (TSDB) and VictoriaMetrics-like engines ingest, index, compact and query metrics.   Run: cargo run --release --example tsm_time_series_engine
//! COMMON USES: metrics backends (Prometheus, InfluxDB, M3, Mimir/Cortex ingesters), observability platforms where every unique label set is a
//! "series", cardinality limits (InfluxDB max-series-per-database, Prometheus series limits), retention-by-dropping-blocks.
//! Course: CMU Database Group time-series talks, Fall 2017 (InfluxDB storage engine).
#![allow(dead_code)]
//!
//! WHAT: samples (timestamp, f64) belong to a SERIES identified by a sorted label set (measurement is the label `__name__`). Reads select series
//! by label matchers, then read their samples in a time range, then optionally downsample by a step with an aggregate.
//! HOW (one struct `Engine` over a simulated `Disk { wal, blocks, floor }` so a crash is "drop memory, keep Disk"):
//! * INDEX: every `(label, value)` has a sorted POSTINGS list of series ids. `=` is one list; `=~`/`!=`/`!~` evaluate the predicate on the values
//!   of that label and union/complement lists (a series lacking the label counts as value ""). Several matchers INTERSECT their lists with a
//!   GALLOPING (exponential + binary) search so a short list against a huge one costs O(small * log big), not O(big).
//! * WRITE PATH: validate (floor, series limit) -> append ONE checksummed WAL record (new series definitions + samples) -> insert into the HEAD (a
//!   per-series ordered map; same timestamp = last write wins; out-of-order is simply accepted, and counted).
//! * FLUSH: head samples older than `upto` become an immutable BLOCK (per series a compressed chunk: delta-of-delta varint timestamps, byte-trimmed
//!   XOR values), then the WAL is rewritten from what remains in the head. A flush after old data arrived makes OVERLAPPING blocks;
//!   newer block id wins on a conflicting timestamp.
//! * COMPACTION ("vertical"): connected groups of time-overlapping blocks are merged into one new block that records its `parents`; tombstones and
//!   data below the retention floor are applied physically. Publishing = add the new block, then remove parents; a crash between leaves both, and
//!   recovery deletes any block that is a parent of another (Prometheus does the same via `parents` in meta.json).
//! * RETENTION: a durable `floor`; whole blocks below it are dropped, the rest are filtered at read and trimmed at the next compaction; writes below
//!   the floor are rejected. TOMBSTONES: `delete` removes head samples now and records (series, range) on each overlapping block (not on blocks
//!   flushed later, so later writes in the range are visible).
//! * QUERY: `query_raw` skips blocks by [mint,maxt]; `downsample` buckets by step and aggregates (sum/avg/min/max/count/last/rate; rate sums
//!   increases and treats a drop as a counter RESET). STALENESS: a special NaN bit pattern (Prometheus's) marks "series ended"; aggregates ignore it
//!   and `last` returns nothing when the newest sample in a bucket is stale; `eval_at` implements the lookback rule.
//! * CARDINALITY: series count multiplies across label dimensions; a high-cardinality label (user id, request id) makes the index (not the samples)
//!   the bottleneck. The demo prints growth and enforces a series limit.
//! TESTS: 2000 random workloads of write/query/flush/compact/delete/retention/crash+recover (clean crash, torn WAL tail, crash after a flush wrote
//! its block, crash after a compaction wrote its block) checked against a brute-force oracle after every op batch; no acknowledged sample is ever lost.
//!
//! Read: Prometheus TSDB blog series by Ganesh Vernekar  https://ganeshvernekar.com/blog/prometheus-tsdb-the-head-block/
//! Read: Prometheus storage docs  https://prometheus.io/docs/prometheus/latest/storage/
//! Read: InfluxDB 1.x storage engine (TSM, WAL, cache)  https://docs.influxdata.com/influxdb/v1/concepts/storage_engine/
//! Read: Pelkonen et al., "Gorilla" (VLDB 2015)  https://www.vldb.org/pvldb/vol8/p1816-teller.pdf
//!
//! TRY NEXT:
//!   - [S] `intersect` gallops from the smaller list. Replace it with a plain two-pointer merge and re-run the demo: predict the comparison ratio for a
//!     list of 10 against 1,000,000 ids, then measure with the counters in `gallop_demo`.
//!   - [S] Change `Failpoint::AfterNewBlock` so recovery does NOT remove parent blocks. Which workload assertion fails first, and why is the duplicate data
//!     not simply harmless?
//!   - [M] Add leveled compaction (merge the oldest 4 adjacent non-overlapping blocks). The overlap-only rule relies on "a merged block overlaps no other
//!     block" for id precedence; what invariant must you add so newer data still wins?
//!   - [M] Port the `Matcher` set to a `label_names(matchers)` / `label_values(name, matchers)` API like Prometheus's /api/v1/labels and check it against the oracle.
//!   - [A] PROJECT: replace `encode_chunk` with the real Gorilla bit stream from the `timeseries_gorilla` recipe and measure bytes/sample per block; then feed the
//!     `downsample` output into `timeseries_sketches_downsampling_anomaly`'s anomaly detectors for a mini alerting pipeline.

use std::collections::{BTreeMap, BTreeSet, HashMap};

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
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

// ---------------------------------------------------------------- byte helpers
const STALE: u64 = 0x7ff0_0000_0000_0002; // Prometheus's staleness NaN payload
fn is_stale(bits: u64) -> bool {
    bits == STALE
}
fn zigzag(x: i64) -> u64 {
    ((x << 1) ^ (x >> 63)) as u64
}
fn unzigzag(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}
fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}
fn put_str(out: &mut Vec<u8>, s: &str) {
    put_varint(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}
struct Cur<'a> {
    b: &'a [u8],
    pos: usize,
}
impl<'a> Cur<'a> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }
    fn varint(&mut self) -> Option<u64> {
        let (mut v, mut shift) = (0u64, 0u32);
        loop {
            let byte = self.u8()?;
            v |= ((byte & 0x7f) as u64).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                return Some(v);
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
    }
    fn zz(&mut self) -> Option<i64> {
        self.varint().map(unzigzag)
    }
    fn u64le(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.bytes(8)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<String> {
        let n = self.varint()? as usize;
        String::from_utf8(self.bytes(n)?.to_vec()).ok()
    }
}
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB88320 & (!(crc & 1)).wrapping_add(1));
        }
    }
    !crc
}

// ---------------------------------------------------------------- labels, regex, matchers
type Labels = Vec<(String, String)>; // sorted by key, includes ("__name__", measurement)
fn labels(name: &str, tags: &[(&str, &str)]) -> Labels {
    let mut l: Labels = tags
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    l.push(("__name__".into(), name.into()));
    l.sort();
    l
}
fn label_value<'a>(l: &'a Labels, name: &str) -> &'a str {
    l.iter()
        .find(|(k, _)| k == name)
        .map_or("", |(_, v)| v.as_str())
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Quant {
    One,
    Star,
    Plus,
    Opt,
}
/// A tiny fully-anchored regex: literals, `.`, `*` `+` `?` on one atom, top-level `|`, `\` escapes. No groups or classes
/// (a real engine uses the `regex` crate; the point here is the label-matcher semantics, not regex theory).
#[derive(Clone, Debug)]
struct Regex {
    alts: Vec<Vec<(Option<char>, Quant)>>,
}
impl Regex {
    fn new(pat: &str) -> Result<Regex, String> {
        let mut alts = vec![vec![]];
        let mut it = pat.chars();
        while let Some(c) = it.next() {
            let cur = alts.last_mut().unwrap();
            match c {
                '|' => alts.push(vec![]),
                '.' => cur.push((None, Quant::One)),
                '\\' => cur.push((Some(it.next().ok_or("dangling backslash")?), Quant::One)),
                '*' | '+' | '?' => {
                    let last = cur.last_mut().ok_or("quantifier without atom")?;
                    if last.1 != Quant::One {
                        return Err("stacked quantifier".into());
                    }
                    last.1 = match c {
                        '*' => Quant::Star,
                        '+' => Quant::Plus,
                        _ => Quant::Opt,
                    };
                }
                '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' => {
                    return Err(format!("unsupported {c:?}"));
                }
                c => cur.push((Some(c), Quant::One)),
            }
        }
        Ok(Regex { alts })
    }
    fn is_match(&self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        self.alts.iter().any(|a| Self::m(a, &chars))
    }
    fn m(seq: &[(Option<char>, Quant)], s: &[char]) -> bool {
        let Some(&(atom, q)) = seq.first() else {
            return s.is_empty();
        };
        let hit = !s.is_empty() && atom.is_none_or(|a| a == s[0]);
        match q {
            Quant::One => hit && Self::m(&seq[1..], &s[1..]),
            Quant::Opt => (hit && Self::m(&seq[1..], &s[1..])) || Self::m(&seq[1..], s),
            Quant::Star => Self::m(&seq[1..], s) || (hit && Self::m(seq, &s[1..])),
            Quant::Plus => hit && (Self::m(&seq[1..], &s[1..]) || Self::m(seq, &s[1..])),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Eq,
    Neq,
    Re,
    Nre,
}
#[derive(Clone, Debug)]
struct Matcher {
    name: String,
    op: Op,
    value: String,
    re: Option<Regex>,
}
impl Matcher {
    fn new(name: &str, op: Op, value: &str) -> Result<Matcher, String> {
        let re = matches!(op, Op::Re | Op::Nre)
            .then(|| Regex::new(value))
            .transpose()?;
        Ok(Matcher {
            name: name.into(),
            op,
            value: value.into(),
            re,
        })
    }
    /// A missing label has the value "" (so `dc!="a"` also selects series without `dc`), as in Prometheus.
    fn matches(&self, v: &str) -> bool {
        match self.op {
            Op::Eq => v == self.value,
            Op::Neq => v != self.value,
            Op::Re => self.re.as_ref().unwrap().is_match(v),
            Op::Nre => !self.re.as_ref().unwrap().is_match(v),
        }
    }
}

// ---------------------------------------------------------------- postings algebra
/// First index >= `from` with `s[idx] >= target` (or s.len()), by exponential then binary search. Counts probes.
fn gallop(s: &[u32], from: usize, target: u32, probes: &mut u64) -> usize {
    let mut step = 1;
    let mut hi = from;
    while hi < s.len() && s[hi] < target {
        *probes += 1;
        hi = from + step;
        step *= 2;
    }
    let (mut lo, mut hi) = (from + step / 4, hi.min(s.len()));
    lo = lo.max(from);
    while lo < hi {
        *probes += 1;
        let mid = (lo + hi) / 2;
        if s[mid] < target {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}
fn intersect_counted(a: &[u32], b: &[u32], probes: &mut u64) -> Vec<u32> {
    let (small, big) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let (mut out, mut pos) = (Vec::new(), 0);
    for &x in small {
        pos = gallop(big, pos, x, probes);
        if pos == big.len() {
            break;
        }
        if big[pos] == x {
            out.push(x);
        }
    }
    out
}
fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    intersect_counted(a, b, &mut 0)
}
fn union(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (mut out, mut i, mut j) = (Vec::with_capacity(a.len() + b.len()), 0, 0);
    while i < a.len() || j < b.len() {
        let take_a = j == b.len() || (i < a.len() && a[i] <= b[j]);
        let x = if take_a { a[i] } else { b[j] };
        if out.last() != Some(&x) {
            out.push(x);
        }
        if take_a { i += 1 } else { j += 1 }
    }
    out
}
fn difference(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut probes = 0;
    let both = intersect_counted(a, b, &mut probes);
    let mut it = both.iter().peekable();
    a.iter()
        .copied()
        .filter(|x| {
            if it.peek() == Some(&x) {
                it.next();
                false
            } else {
                true
            }
        })
        .collect()
}

// ---------------------------------------------------------------- chunk encoding (byte-aligned Gorilla-ish)
fn encode_chunk(pts: &[(i64, u64)]) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(&mut out, pts.len() as u64);
    let (mut prev_t, mut prev_d, mut prev_v) = (0i64, 0i64, 0u64);
    for (i, &(t, v)) in pts.iter().enumerate() {
        if i == 0 {
            put_varint(&mut out, zigzag(t));
            out.extend_from_slice(&v.to_le_bytes());
        } else {
            let d = t.wrapping_sub(prev_t);
            put_varint(&mut out, zigzag(d.wrapping_sub(prev_d)));
            prev_d = d;
            let x = v ^ prev_v;
            if x == 0 {
                out.push(0xFF);
            } else {
                let (lzb, tzb) = (x.leading_zeros() / 8, x.trailing_zeros() / 8);
                out.push((lzb << 4 | tzb) as u8);
                out.extend_from_slice(&(x >> (8 * tzb)).to_le_bytes()[..(8 - lzb - tzb) as usize]);
            }
        }
        prev_t = t;
        prev_v = v;
    }
    out
}
fn decode_chunk(b: &[u8]) -> Option<Vec<(i64, u64)>> {
    let mut c = Cur { b, pos: 0 };
    let n = c.varint()? as usize;
    let mut out = Vec::with_capacity(n.min(1 << 16));
    let (mut t, mut d, mut v) = (0i64, 0i64, 0u64);
    for i in 0..n {
        if i == 0 {
            t = c.zz()?;
            v = c.u64le()?;
        } else {
            d = d.wrapping_add(c.zz()?);
            t = t.wrapping_add(d);
            let ctl = c.u8()?;
            if ctl != 0xFF {
                let (lzb, tzb) = ((ctl >> 4) as u32, (ctl & 15) as u32);
                if lzb + tzb > 7 {
                    return None;
                }
                let mut buf = [0u8; 8];
                buf[..(8 - lzb - tzb) as usize].copy_from_slice(c.bytes((8 - lzb - tzb) as usize)?);
                v ^= u64::from_le_bytes(buf) << (8 * tzb);
            }
        }
        out.push((t, v));
    }
    Some(out)
}

// ---------------------------------------------------------------- WAL
enum Record {
    Write {
        series: Vec<(u32, Labels)>,
        samples: Vec<(u32, i64, u64)>,
    },
    Delete {
        ids: Vec<u32>,
        mint: i64,
        maxt: i64,
    },
}
fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = (payload.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&crc32(payload).to_le_bytes());
    out.extend_from_slice(payload);
    out
}
fn encode_record(r: &Record) -> Vec<u8> {
    let mut p = Vec::new();
    match r {
        Record::Write { series, samples } => {
            p.push(1);
            put_varint(&mut p, series.len() as u64);
            for (id, l) in series {
                put_varint(&mut p, *id as u64);
                put_varint(&mut p, l.len() as u64);
                for (k, v) in l {
                    put_str(&mut p, k);
                    put_str(&mut p, v);
                }
            }
            put_varint(&mut p, samples.len() as u64);
            for &(id, t, bits) in samples {
                put_varint(&mut p, id as u64);
                put_varint(&mut p, zigzag(t));
                p.extend_from_slice(&bits.to_le_bytes());
            }
        }
        Record::Delete { ids, mint, maxt } => {
            p.push(2);
            put_varint(&mut p, ids.len() as u64);
            ids.iter().for_each(|&i| put_varint(&mut p, i as u64));
            put_varint(&mut p, zigzag(*mint));
            put_varint(&mut p, zigzag(*maxt));
        }
    }
    frame(&p)
}
fn decode_record(p: &[u8]) -> Option<Record> {
    let mut c = Cur { b: p, pos: 0 };
    match c.u8()? {
        1 => {
            let mut series = Vec::new();
            for _ in 0..c.varint()? {
                let id = c.varint()? as u32;
                let mut l = Vec::new();
                for _ in 0..c.varint()? {
                    l.push((c.string()?, c.string()?));
                }
                series.push((id, l));
            }
            let mut samples = Vec::new();
            for _ in 0..c.varint()? {
                samples.push((c.varint()? as u32, c.zz()?, c.u64le()?));
            }
            Some(Record::Write { series, samples })
        }
        2 => {
            let ids = (0..c.varint()?)
                .map(|_| c.varint().map(|v| v as u32))
                .collect::<Option<_>>()?;
            Some(Record::Delete {
                ids,
                mint: c.zz()?,
                maxt: c.zz()?,
            })
        }
        _ => None,
    }
}
/// Replays framed records; stops at the first short or corrupt one. Returns (records, length of the valid prefix).
fn scan_wal(wal: &[u8]) -> (Vec<Record>, usize) {
    let (mut recs, mut pos) = (Vec::new(), 0);
    while pos + 8 <= wal.len() {
        let len = u32::from_le_bytes(wal[pos..pos + 4].try_into().unwrap()) as usize;
        let crc = u32::from_le_bytes(wal[pos + 4..pos + 8].try_into().unwrap());
        let Some(payload) = wal.get(pos + 8..pos + 8 + len) else {
            break;
        };
        if crc32(payload) != crc {
            break;
        }
        let Some(r) = decode_record(payload) else {
            break;
        };
        recs.push(r);
        pos += 8 + len;
    }
    (recs, pos)
}

// ---------------------------------------------------------------- disk model
#[derive(Clone)]
struct Block {
    id: u64,
    mint: i64,
    maxt: i64,
    parents: Vec<u64>,
    chunks: BTreeMap<u32, Vec<u8>>,
    tombstones: Vec<(u32, i64, i64)>,
}
impl Block {
    fn series_points(&self, sid: u32) -> Vec<(i64, u64)> {
        let Some(chunk) = self.chunks.get(&sid) else {
            return vec![];
        };
        let pts = decode_chunk(chunk).expect("block chunk is valid");
        pts.into_iter()
            .filter(|&(t, _)| {
                !self
                    .tombstones
                    .iter()
                    .any(|&(s, a, b)| s == sid && t >= a && t <= b)
            })
            .collect()
    }
}
#[derive(Clone)]
struct Disk {
    wal: Vec<u8>,
    blocks: Vec<Block>, // ascending id
    floor: i64,
    next_block: u64,
}
impl Disk {
    fn new() -> Self {
        Disk {
            wal: vec![],
            blocks: vec![],
            floor: i64::MIN,
            next_block: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Failpoint {
    TornWal(u64),
    AfterFlushBlock,
    AfterNewBlock,
}
#[derive(Debug, PartialEq)]
enum EngineError {
    TooOld { ts: i64, floor: i64 },
    SeriesLimit { limit: usize },
    Crashed,
}

#[derive(Default, Debug, Clone)]
struct Stats {
    ooo_writes: u64,
    blocks_scanned: u64,
    blocks_total_seen: u64,
    flushes: u64,
    compactions: u64,
}

struct Engine {
    disk: Disk,
    max_series: usize,
    series: Vec<Labels>,
    by_labels: HashMap<Labels, u32>,
    postings: HashMap<(String, String), Vec<u32>>,
    values: BTreeMap<String, BTreeSet<String>>,
    head: BTreeMap<u32, BTreeMap<i64, u64>>,
    fail: Option<Failpoint>,
    stats: Stats,
}

impl Engine {
    fn new(max_series: usize) -> Engine {
        Self::from_parts(Disk::new(), max_series)
    }
    fn from_parts(disk: Disk, max_series: usize) -> Engine {
        Engine {
            disk,
            max_series,
            series: vec![],
            by_labels: HashMap::new(),
            postings: HashMap::new(),
            values: BTreeMap::new(),
            head: BTreeMap::new(),
            fail: None,
            stats: Stats::default(),
        }
    }
    fn register(&mut self, l: &Labels) -> u32 {
        let id = self.series.len() as u32;
        for (k, v) in l {
            self.postings
                .entry((k.clone(), v.clone()))
                .or_default()
                .push(id); // ids grow, so lists stay sorted
            self.values.entry(k.clone()).or_default().insert(v.clone());
        }
        self.by_labels.insert(l.clone(), id);
        self.series.push(l.clone());
        id
    }
    fn head_insert(&mut self, id: u32, t: i64, bits: u64) {
        let m = self.head.entry(id).or_default();
        if m.range(t + 1..).next().is_some() {
            self.stats.ooo_writes += 1;
        }
        m.insert(t, bits);
    }

    // ---- write path
    fn write(&mut self, batch: &[(Labels, i64, f64)]) -> Result<(), EngineError> {
        // 1. validate everything first so a rejected batch changes nothing (atomic batches)
        let mut fresh: Vec<Labels> = Vec::new();
        for (l, t, _) in batch {
            if *t < self.disk.floor {
                return Err(EngineError::TooOld {
                    ts: *t,
                    floor: self.disk.floor,
                });
            }
            if !self.by_labels.contains_key(l) && !fresh.contains(l) {
                fresh.push(l.clone());
            }
        }
        if self.series.len() + fresh.len() > self.max_series {
            return Err(EngineError::SeriesLimit {
                limit: self.max_series,
            });
        }
        if batch.is_empty() {
            return Ok(());
        }
        // 2. durable log record: new series defs + samples
        let new_series: Vec<(u32, Labels)> = fresh
            .iter()
            .enumerate()
            .map(|(i, l)| ((self.series.len() + i) as u32, l.clone()))
            .collect();
        let id_of = |l: &Labels| {
            new_series
                .iter()
                .find(|(_, x)| x == l)
                .map(|(i, _)| *i)
                .or_else(|| self.by_labels.get(l).copied())
                .unwrap()
        };
        let samples: Vec<(u32, i64, u64)> = batch
            .iter()
            .map(|(l, t, v)| (id_of(l), *t, v.to_bits()))
            .collect();
        let rec = encode_record(&Record::Write {
            series: new_series.clone(),
            samples: samples.clone(),
        });
        if let Some(Failpoint::TornWal(seed)) = self.fail.take() {
            let cut = (seed % rec.len() as u64) as usize; // strictly inside the record: never acknowledged
            self.disk.wal.extend_from_slice(&rec[..cut]);
            return Err(EngineError::Crashed);
        }
        self.disk.wal.extend_from_slice(&rec); // <- acknowledgement point (an fsync in a real engine)
        // 3. in-memory state
        for (_, l) in &new_series {
            self.register(l);
        }
        for (id, t, bits) in samples {
            self.head_insert(id, t, bits);
        }
        Ok(())
    }
    fn write_stale(&mut self, l: &Labels, t: i64) -> Result<(), EngineError> {
        self.write(&[(l.clone(), t, f64::from_bits(STALE))])
    }

    // ---- series selection
    fn all_ids(&self) -> Vec<u32> {
        (0..self.series.len() as u32).collect()
    }
    fn ids_for(&self, m: &Matcher) -> Vec<u32> {
        let empty = BTreeSet::new();
        let vals = self.values.get(&m.name).unwrap_or(&empty);
        let mut ids: Vec<u32> = vec![];
        let mut present: Vec<u32> = vec![];
        for v in vals {
            let list = &self.postings[&(m.name.clone(), v.clone())];
            if m.matches(v) {
                ids = union(&ids, list);
            }
            present = union(&present, list);
        }
        if m.matches("") {
            ids = union(&ids, &difference(&self.all_ids(), &present)); // series that lack the label
        }
        ids
    }
    fn select_ids(&self, ms: &[Matcher]) -> Vec<u32> {
        let mut acc: Option<Vec<u32>> = None;
        for m in ms {
            let ids = self.ids_for(m);
            acc = Some(match acc {
                None => ids,
                Some(a) => intersect(&a, &ids),
            });
        }
        acc.unwrap_or_else(|| self.all_ids())
    }

    // ---- read path
    fn query_raw(
        &mut self,
        ms: &[Matcher],
        mint: i64,
        maxt: i64,
    ) -> Vec<(Labels, Vec<(i64, u64)>)> {
        let lo = mint.max(self.disk.floor);
        let ids = self.select_ids(ms);
        let blocks: Vec<&Block> = self
            .disk
            .blocks
            .iter()
            .filter(|b| b.maxt >= lo && b.mint <= maxt)
            .collect();
        self.stats.blocks_total_seen += self.disk.blocks.len() as u64;
        self.stats.blocks_scanned += blocks.len() as u64;
        let mut out = vec![];
        for id in ids {
            let mut merged: BTreeMap<i64, u64> = BTreeMap::new();
            for b in &blocks {
                merged.extend(b.series_points(id)); // ascending block id: newer overwrites older
            }
            if let Some(h) = self.head.get(&id) {
                merged.extend(h.iter().map(|(&t, &v)| (t, v)));
            }
            let pts: Vec<(i64, u64)> = merged
                .into_iter()
                .filter(|&(t, _)| t >= lo && t <= maxt)
                .collect();
            if !pts.is_empty() {
                out.push((self.series[id as usize].clone(), pts));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
    fn query_range(
        &mut self,
        ms: &[Matcher],
        start: i64,
        end: i64,
        step: i64,
        agg: Agg,
    ) -> Vec<(Labels, Vec<(i64, f64)>)> {
        self.query_raw(ms, start, end - 1)
            .into_iter()
            .map(|(l, pts)| (l, downsample(&pts, start, end, step, agg)))
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }

    // ---- maintenance
    fn rewrite_wal(&mut self) {
        let series: Vec<(u32, Labels)> = self
            .series
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, l)| (i as u32, l))
            .collect();
        let samples: Vec<(u32, i64, u64)> = self
            .head
            .iter()
            .flat_map(|(&id, m)| m.iter().map(move |(&t, &v)| (id, t, v)))
            .collect();
        self.disk.wal = if series.is_empty() {
            vec![]
        } else {
            encode_record(&Record::Write { series, samples })
        };
    }
    fn build_block(
        &mut self,
        data: BTreeMap<u32, BTreeMap<i64, u64>>,
        parents: Vec<u64>,
    ) -> Option<Block> {
        let data: BTreeMap<_, _> = data.into_iter().filter(|(_, m)| !m.is_empty()).collect();
        let mint = data
            .values()
            .filter_map(|m| m.keys().next())
            .min()
            .copied()?;
        let maxt = data
            .values()
            .filter_map(|m| m.keys().next_back())
            .max()
            .copied()?;
        let chunks = data
            .iter()
            .map(|(&id, m)| {
                (
                    id,
                    encode_chunk(&m.iter().map(|(&t, &v)| (t, v)).collect::<Vec<_>>()),
                )
            })
            .collect();
        let id = self.disk.next_block;
        self.disk.next_block += 1;
        Some(Block {
            id,
            mint,
            maxt,
            parents,
            chunks,
            tombstones: vec![],
        })
    }
    /// Move head samples with ts < `upto` into a new immutable block.
    fn flush(&mut self, upto: i64) -> Result<(), EngineError> {
        let mut taken: BTreeMap<u32, BTreeMap<i64, u64>> = BTreeMap::new();
        for (&id, m) in self.head.iter_mut() {
            let tail = m.split_off(&upto);
            taken.insert(id, std::mem::replace(m, tail));
        }
        self.head.retain(|_, m| !m.is_empty());
        if let Some(b) = self.build_block(taken, vec![]) {
            self.disk.blocks.push(b);
            self.stats.flushes += 1;
            if self.fail.take() == Some(Failpoint::AfterFlushBlock) {
                return Err(EngineError::Crashed); // block is durable, WAL still holds the same samples
            }
        }
        self.rewrite_wal();
        Ok(())
    }
    /// Vertical compaction of overlapping blocks (and cleanup of blocks with tombstones or data under the floor).
    fn compact(&mut self) -> Result<(), EngineError> {
        let mut order: Vec<usize> = (0..self.disk.blocks.len()).collect();
        order.sort_by_key(|&i| self.disk.blocks[i].mint);
        let mut groups: Vec<Vec<u64>> = vec![];
        let mut cur_max = i64::MIN;
        for i in order {
            let b = &self.disk.blocks[i];
            match groups.last_mut() {
                Some(g) if b.mint <= cur_max => {
                    g.push(b.id);
                    cur_max = cur_max.max(b.maxt);
                }
                _ => {
                    groups.push(vec![b.id]);
                    cur_max = b.maxt;
                }
            }
        }
        for g in groups {
            let blocks: Vec<Block> = {
                let mut v: Vec<Block> = self
                    .disk
                    .blocks
                    .iter()
                    .filter(|b| g.contains(&b.id))
                    .cloned()
                    .collect();
                v.sort_by_key(|b| b.id);
                v
            };
            let dirty = blocks.len() > 1
                || !blocks[0].tombstones.is_empty()
                || blocks[0].mint < self.disk.floor;
            if !dirty {
                continue;
            }
            let mut merged: BTreeMap<u32, BTreeMap<i64, u64>> = BTreeMap::new();
            for b in &blocks {
                for &sid in b.chunks.keys() {
                    let m = merged.entry(sid).or_default();
                    m.extend(
                        b.series_points(sid)
                            .into_iter()
                            .filter(|&(t, _)| t >= self.disk.floor),
                    );
                }
            }
            let parents: Vec<u64> = blocks.iter().map(|b| b.id).collect();
            if let Some(nb) = self.build_block(merged, parents.clone()) {
                self.disk.blocks.push(nb);
            }
            if self.fail.take() == Some(Failpoint::AfterNewBlock) {
                return Err(EngineError::Crashed); // new block AND parents are on disk
            }
            self.disk.blocks.retain(|b| !parents.contains(&b.id));
            self.stats.compactions += 1;
        }
        self.disk.blocks.sort_by_key(|b| b.id);
        Ok(())
    }
    fn apply_retention(&mut self, cutoff: i64) {
        if cutoff <= self.disk.floor {
            return;
        }
        self.disk.floor = cutoff;
        self.disk.blocks.retain(|b| b.maxt >= cutoff);
        for m in self.head.values_mut() {
            *m = m.split_off(&cutoff);
        }
        self.head.retain(|_, m| !m.is_empty());
    }
    fn delete(&mut self, ms: &[Matcher], mint: i64, maxt: i64) {
        let ids = self.select_ids(ms);
        if ids.is_empty() {
            return;
        }
        for b in self
            .disk
            .blocks
            .iter_mut()
            .filter(|b| b.maxt >= mint && b.mint <= maxt)
        {
            for &id in &ids {
                if b.chunks.contains_key(&id) {
                    b.tombstones.push((id, mint, maxt));
                }
            }
        }
        self.disk
            .wal
            .extend_from_slice(&encode_record(&Record::Delete {
                ids: ids.clone(),
                mint,
                maxt,
            }));
        self.apply_delete_head(&ids, mint, maxt);
    }
    fn apply_delete_head(&mut self, ids: &[u32], mint: i64, maxt: i64) {
        for id in ids {
            if let Some(m) = self.head.get_mut(id) {
                m.retain(|&t, _| t < mint || t > maxt);
            }
        }
        self.head.retain(|_, m| !m.is_empty());
    }

    // ---- crash recovery
    fn into_disk(self) -> Disk {
        self.disk
    }
    fn recover(mut disk: Disk, max_series: usize) -> Engine {
        let (records, valid) = scan_wal(&disk.wal);
        disk.wal.truncate(valid); // a torn tail must go, or new appends would sit behind garbage
        let parents: BTreeSet<u64> = disk
            .blocks
            .iter()
            .flat_map(|b| b.parents.iter().copied())
            .collect();
        disk.blocks.retain(|b| !parents.contains(&b.id)); // finish an interrupted compaction
        let mut e = Engine::from_parts(disk, max_series);
        for r in records {
            match r {
                Record::Write { series, samples } => {
                    for (id, l) in series {
                        assert_eq!(
                            id as usize,
                            e.series.len(),
                            "series ids are dense and replayed in order"
                        );
                        e.register(&l);
                    }
                    for (id, t, bits) in samples {
                        e.head_insert(id, t, bits);
                    }
                }
                Record::Delete { ids, mint, maxt } => e.apply_delete_head(&ids, mint, maxt),
            }
        }
        let floor = e.disk.floor;
        for m in e.head.values_mut() {
            *m = m.split_off(&floor);
        }
        e.head.retain(|_, m| !m.is_empty());
        e.stats.ooo_writes = 0;
        e
    }

    fn cardinality_report(&self) -> Vec<(String, usize)> {
        let mut v: Vec<(String, usize)> = self
            .values
            .iter()
            .map(|(k, s)| (k.clone(), s.len()))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }
    fn index_entries(&self) -> usize {
        self.postings.values().map(|p| p.len()).sum()
    }
}

// ---------------------------------------------------------------- aggregation
#[derive(Clone, Copy, Debug, PartialEq)]
enum Agg {
    Sum,
    Avg,
    Min,
    Max,
    Count,
    Last,
    Rate,
}
const AGGS: [Agg; 7] = [
    Agg::Sum,
    Agg::Avg,
    Agg::Min,
    Agg::Max,
    Agg::Count,
    Agg::Last,
    Agg::Rate,
];

/// Aggregate non-stale, time-ordered points. Rate = counter-reset-aware increase / elapsed time (needs 2 points).
fn aggregate(agg: Agg, pts: &[(i64, f64)]) -> Option<f64> {
    let (first, last) = (pts.first()?, pts.last()?);
    Some(match agg {
        Agg::Sum => pts.iter().fold(0.0, |a, p| a + p.1),
        Agg::Avg => pts.iter().fold(0.0, |a, p| a + p.1) / pts.len() as f64,
        Agg::Min => pts.iter().fold(f64::INFINITY, |a, p| a.min(p.1)),
        Agg::Max => pts.iter().fold(f64::NEG_INFINITY, |a, p| a.max(p.1)),
        Agg::Count => pts.len() as f64,
        Agg::Last => last.1,
        Agg::Rate => {
            if pts.len() < 2 || last.0 == first.0 {
                return None;
            }
            let inc: f64 = pts
                .windows(2)
                .map(|w| {
                    if w[1].1 >= w[0].1 {
                        w[1].1 - w[0].1
                    } else {
                        w[1].1
                    }
                })
                .sum(); // drop = reset to 0
            inc / (last.0 - first.0) as f64
        }
    })
}
/// Buckets [start + k*step, start + (k+1)*step) for k while the bucket start < end.
fn downsample(raw: &[(i64, u64)], start: i64, end: i64, step: i64, agg: Agg) -> Vec<(i64, f64)> {
    let mut out = vec![];
    let mut b = start;
    while b < end {
        let in_bucket: Vec<(i64, u64)> = raw
            .iter()
            .copied()
            .filter(|&(t, _)| t >= b && t < b + step)
            .collect();
        let went_stale = agg == Agg::Last && in_bucket.last().is_some_and(|&(_, v)| is_stale(v));
        let live: Vec<(i64, f64)> = in_bucket
            .iter()
            .filter(|&&(_, v)| !is_stale(v))
            .map(|&(t, v)| (t, f64::from_bits(v)))
            .collect();
        if !went_stale {
            if let Some(x) = aggregate(agg, &live) {
                out.push((b, x));
            }
        }
        b += step;
    }
    out
}
/// Instant-vector lookback: newest sample in (t - lookback, t]; none if it is a stale marker.
fn eval_at(raw: &[(i64, u64)], t: i64, lookback: i64) -> Option<f64> {
    let &(_, v) = raw
        .iter()
        .rev()
        .find(|&&(ts, _)| ts <= t && ts > t - lookback)?;
    (!is_stale(v)).then(|| f64::from_bits(v))
}

// ---------------------------------------------------------------- oracle
struct Oracle {
    data: BTreeMap<Labels, BTreeMap<i64, u64>>,
    floor: i64,
}
impl Oracle {
    fn new() -> Self {
        Oracle {
            data: BTreeMap::new(),
            floor: i64::MIN,
        }
    }
    fn write(&mut self, batch: &[(Labels, i64, f64)]) -> Result<(), EngineError> {
        if let Some((_, t, _)) = batch.iter().find(|(_, t, _)| *t < self.floor) {
            return Err(EngineError::TooOld {
                ts: *t,
                floor: self.floor,
            });
        }
        for (l, t, v) in batch {
            self.data
                .entry(l.clone())
                .or_default()
                .insert(*t, v.to_bits());
        }
        Ok(())
    }
    fn selected(&self, ms: &[Matcher]) -> Vec<Labels> {
        self.data
            .keys()
            .filter(|l| ms.iter().all(|m| m.matches(label_value(l, &m.name))))
            .cloned()
            .collect()
    }
    fn delete(&mut self, ms: &[Matcher], mint: i64, maxt: i64) {
        for l in self.selected(ms) {
            self.data
                .get_mut(&l)
                .unwrap()
                .retain(|&t, _| t < mint || t > maxt);
        }
    }
    fn retention(&mut self, cutoff: i64) {
        if cutoff > self.floor {
            self.floor = cutoff;
            self.data
                .values_mut()
                .for_each(|m| m.retain(|&t, _| t >= cutoff));
        }
    }
    fn query_raw(&self, ms: &[Matcher], mint: i64, maxt: i64) -> Vec<(Labels, Vec<(i64, u64)>)> {
        let lo = mint.max(self.floor);
        let mut out: Vec<_> = self
            .selected(ms)
            .into_iter()
            .map(|l| {
                let pts: Vec<(i64, u64)> = self.data[&l]
                    .iter()
                    .filter(|&(&t, _)| t >= lo && t <= maxt)
                    .map(|(&t, &v)| (t, v))
                    .collect();
                (l, pts)
            })
            .filter(|(_, p)| !p.is_empty())
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

// ---------------------------------------------------------------- workloads
const NAMES: [&str; 3] = ["cpu", "mem", "disk"];
const HOSTS: [&str; 6] = ["h0", "h1", "h2", "h3", "h4", "h5"];

fn rand_labels(rng: &mut Rng) -> Labels {
    let mut tags = vec![("host", *rng.pick(&HOSTS))];
    if rng.chance(50) {
        tags.push(("dc", *rng.pick(&["a", "b"])));
    }
    labels(rng.pick(&NAMES), &tags)
}
fn rand_matchers(rng: &mut Rng) -> Vec<Matcher> {
    let mut out = vec![];
    for _ in 0..rng.below(3) {
        let (name, pool): (&str, &[&str]) = match rng.below(4) {
            0 => ("__name__", &["cpu", "mem", "disk", "net"]),
            1 => ("host", &["h0", "h1", "h5", "h9", ""]),
            2 => ("dc", &["a", "b", "c", ""]),
            _ => ("zone", &["z", ""]), // label no series has
        };
        let re_pool = [
            "h.", "h1|h2", "c.*", ".*", "a|", "", "d?c?", ".+", "m.m|cpu", "h0|h3|",
        ];
        let m = match rng.below(4) {
            0 => Matcher::new(name, Op::Eq, rng.pick(pool)),
            1 => Matcher::new(name, Op::Neq, rng.pick(pool)),
            2 => Matcher::new(name, Op::Re, rng.pick(&re_pool)),
            _ => Matcher::new(name, Op::Nre, rng.pick(&re_pool)),
        };
        out.push(m.unwrap());
    }
    out
}
fn rand_batch(rng: &mut Rng, base: i64) -> Vec<(Labels, i64, f64)> {
    (0..1 + rng.below(6))
        .map(|_| {
            let t = if rng.chance(60) {
                base + rng.range(0, 10)
            } else {
                rng.range(0, 300)
            }; // mostly "now", sometimes far in the past
            let v = match rng.below(40) {
                0 => f64::NAN,
                1 => f64::from_bits(STALE),
                _ => rng.range(0, 100) as f64,
            };
            (rand_labels(rng), t, v)
        })
        .collect()
}

#[derive(Default, Debug)]
struct WorkloadStats {
    ops: u64,
    writes: u64,
    rejected: u64,
    clean_crashes: u64,
    torn: u64,
    flush_crashes: u64,
    compact_crashes: u64,
    checks: u64,
    max_blocks: usize,
}

fn same_raw(a: &[(Labels, Vec<(i64, u64)>)], b: &[(Labels, Vec<(i64, u64)>)]) -> bool {
    a == b
}
fn full_check(e: &mut Engine, o: &Oracle, rng: &mut Rng, ctx: &str, st: &mut WorkloadStats) {
    let all = [Matcher::new("__name__", Op::Re, ".*").unwrap()];
    assert!(
        same_raw(
            &e.query_raw(&all, i64::MIN, i64::MAX),
            &o.query_raw(&all, i64::MIN, i64::MAX)
        ),
        "{ctx}: full scan differs"
    );
    for _ in 0..3 {
        let ms = rand_matchers(rng);
        let (a, b) = (rng.range(0, 300), rng.range(0, 300));
        let (mint, maxt) = (a.min(b), a.max(b));
        let (got, want) = (e.query_raw(&ms, mint, maxt), o.query_raw(&ms, mint, maxt));
        assert!(
            same_raw(&got, &want),
            "{ctx}: raw query {ms:?} [{mint},{maxt}] differs\n got {got:?}\nwant {want:?}"
        );
        let step = rng.range(1, 40);
        let agg = *rng.pick(&AGGS);
        let (g, w) = (e.query_range(&ms, mint, maxt + 1, step, agg), {
            o.query_raw(&ms, mint, maxt)
                .into_iter()
                .map(|(l, p)| (l, downsample(&p, mint, maxt + 1, step, agg)))
                .filter(|(_, v)| !v.is_empty())
                .collect::<Vec<_>>()
        });
        let bits = |x: &[(Labels, Vec<(i64, f64)>)]| {
            x.iter()
                .map(|(l, v)| {
                    (
                        l.clone(),
                        v.iter().map(|&(t, f)| (t, f.to_bits())).collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(&g), bits(&w), "{ctx}: {agg:?} step {step} differs");
        st.checks += 1;
    }
}

fn run_workload(seed: u64, st: &mut WorkloadStats, ooo: &mut u64) {
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let (mut e, mut o) = (Engine::new(usize::MAX), Oracle::new());
    let mut base = 0i64;
    let ctx_of = |i: usize| format!("seed {seed} op {i}");
    for i in 0..rng.range(20, 90) as usize {
        st.ops += 1;
        let ctx = ctx_of(i);
        match rng.below(100) {
            0..=44 => {
                base += rng.range(0, 6);
                let batch = rand_batch(&mut rng, base);
                let (r_e, r_o) = (e.write(&batch), o.write(&batch));
                assert_eq!(r_e, r_o, "{ctx}: write verdict");
                st.writes += 1;
                st.rejected += r_e.is_err() as u64;
            }
            45..=54 => {
                let upto = rng.range(0, 320);
                e.flush(upto).unwrap();
            }
            55..=62 => {
                e.compact().unwrap();
                let mut spans: Vec<(i64, i64)> =
                    e.disk.blocks.iter().map(|b| (b.mint, b.maxt)).collect();
                spans.sort();
                assert!(
                    spans.windows(2).all(|w| w[0].1 < w[1].0),
                    "{ctx}: blocks overlap after compaction {spans:?}"
                );
                assert!(
                    e.disk.blocks.iter().all(|b| b.tombstones.is_empty()),
                    "{ctx}: tombstones remain"
                );
            }
            63..=70 => {
                let ms = rand_matchers(&mut rng);
                let (a, b) = (rng.range(0, 300), rng.range(0, 300));
                e.delete(&ms, a.min(b), a.max(b));
                o.delete(&ms, a.min(b), a.max(b));
            }
            71..=73 => {
                let cutoff = o.floor.max(0) + rng.range(0, 40);
                e.apply_retention(cutoff);
                o.retention(cutoff);
            }
            74..=89 => full_check(&mut e, &o, &mut rng, &ctx, st),
            _ => {
                // crash at a random point
                *ooo += e.stats.ooo_writes;
                let kind = rng.below(4);
                match kind {
                    0 => st.clean_crashes += 1,
                    1 => {
                        e.fail = Some(Failpoint::TornWal(rng.next()));
                        let batch = rand_batch(&mut rng, base);
                        assert_eq!(
                            e.write(&batch),
                            e_result(&batch, o.floor),
                            "{ctx}: torn write verdict"
                        );
                        st.torn += 1;
                    }
                    2 => {
                        e.fail = Some(Failpoint::AfterFlushBlock);
                        let r = e.flush(rng.range(0, 320));
                        // with nothing to flush there is no block and no crash point: still a valid clean state
                        assert!(r.is_ok() || r == Err(EngineError::Crashed));
                        st.flush_crashes += 1;
                    }
                    _ => {
                        e.fail = Some(Failpoint::AfterNewBlock);
                        let r = e.compact();
                        assert!(r.is_ok() || r == Err(EngineError::Crashed));
                        st.compact_crashes += 1;
                    }
                }
                let wal_before = e.disk.wal.len();
                let disk = e.into_disk();
                e = Engine::recover(disk, usize::MAX);
                assert!(e.disk.wal.len() <= wal_before + 1_000_000);
                let parents: BTreeSet<u64> = e
                    .disk
                    .blocks
                    .iter()
                    .flat_map(|b| b.parents.iter().copied())
                    .collect();
                assert!(
                    e.disk.blocks.iter().all(|b| !parents.contains(&b.id)),
                    "{ctx}: parent block survived recovery"
                );
                full_check(
                    &mut e,
                    &o,
                    &mut rng,
                    &format!("{ctx} after crash kind {kind}"),
                    st,
                );
            }
        }
        st.max_blocks = st.max_blocks.max(e.disk.blocks.len());
    }
    *ooo += e.stats.ooo_writes;
    // idempotent recovery: crashing again right away changes nothing
    let disk = e.into_disk();
    let mut e = Engine::recover(disk, usize::MAX);
    full_check(&mut e, &o, &mut rng, &format!("seed {seed} final"), st);
    let disk2 = e.into_disk();
    let mut e2 = Engine::recover(disk2, usize::MAX);
    full_check(&mut e2, &o, &mut rng, &format!("seed {seed} final x2"), st);
}

/// Torn write helper: the batch is invisible to the oracle unless the engine rejected it before touching the log.
fn e_result(batch: &[(Labels, i64, f64)], floor: i64) -> Result<(), EngineError> {
    match batch.iter().find(|(_, t, _)| *t < floor) {
        Some((_, t, _)) => Err(EngineError::TooOld { ts: *t, floor }),
        None => Err(EngineError::Crashed),
    }
}

// ---------------------------------------------------------------- unit tests and demos
fn test_regex_and_matchers() {
    let re = |p: &str, s: &str| Regex::new(p).unwrap().is_match(s);
    assert!(
        re("h.", "h1")
            && !re("h.", "h12")
            && re("h.*", "h12")
            && re("a|b", "b")
            && !re("a|b", "ab")
    );
    assert!(
        re("", "")
            && !re("", "x")
            && re("a|", "")
            && re("d?c?", "")
            && re("d?c?", "dc")
            && !re("d?c?", "cd")
    );
    assert!(re("a+b", "aaab") && !re("a+b", "b") && re(r"a\.b", "a.b") && !re(r"a\.b", "axb"));
    assert!(Regex::new("(a)").is_err() && Regex::new("*a").is_err() && Regex::new("a**").is_err());
    let m = Matcher::new("dc", Op::Neq, "a").unwrap();
    assert!(
        m.matches("") && m.matches("b") && !m.matches("a"),
        "absent label matches !="
    );
    println!("regex + matcher semantics: ok");
}

fn test_postings(rng: &mut Rng) {
    for _ in 0..2000 {
        let gen_list = |rng: &mut Rng, max_len: u64| -> Vec<u32> {
            let s: BTreeSet<u32> = (0..rng.below(max_len))
                .map(|_| rng.below(300) as u32)
                .collect();
            s.into_iter().collect()
        };
        let (a, b) = (gen_list(rng, 40), gen_list(rng, 250));
        let (sa, sb): (BTreeSet<u32>, BTreeSet<u32>) =
            (a.iter().copied().collect(), b.iter().copied().collect());
        assert_eq!(
            intersect(&a, &b),
            sa.intersection(&sb).copied().collect::<Vec<_>>()
        );
        assert_eq!(
            intersect(&b, &a),
            sa.intersection(&sb).copied().collect::<Vec<_>>()
        );
        assert_eq!(union(&a, &b), sa.union(&sb).copied().collect::<Vec<_>>());
        assert_eq!(
            difference(&a, &b),
            sa.difference(&sb).copied().collect::<Vec<_>>()
        );
    }
    println!("postings intersect/union/difference vs BTreeSet on 2000 pairs: ok");
}

fn gallop_demo() {
    println!("\ngalloping intersect vs linear merge (probe counts, not time):");
    println!(
        "{:>10} {:>10} {:>14} {:>14}",
        "small", "big", "gallop probes", "linear steps"
    );
    for (s, b) in [
        (10usize, 1_000_000usize),
        (1000, 1_000_000),
        (100_000, 1_000_000),
        (500_000, 1_000_000),
    ] {
        let big: Vec<u32> = (0..b as u32).collect();
        let small: Vec<u32> = (0..s as u32).map(|i| i * (b / s) as u32 + 1).collect();
        let mut probes = 0;
        let r = intersect_counted(&small, &big, &mut probes);
        assert_eq!(r, small);
        let linear = (s + b) as u64;
        println!("{s:>10} {b:>10} {probes:>14} {linear:>14}");
        if s == 10 {
            assert!(
                probes < 1000 && probes * 1000 < linear,
                "galloping must beat the merge by orders of magnitude on skew"
            );
        }
    }
}

fn test_chunk_codec(rng: &mut Rng) {
    for _ in 0..500 {
        let n = rng.below(60) as usize;
        let mut t = rng.range(-1000, 1000);
        let pts: Vec<(i64, u64)> = (0..n)
            .map(|_| {
                t += rng.range(1, 20);
                let v = match rng.below(5) {
                    0 => rng.next(),
                    1 => STALE,
                    _ => (rng.range(0, 50) as f64).to_bits(),
                };
                (t, v)
            })
            .collect();
        assert_eq!(decode_chunk(&encode_chunk(&pts)), Some(pts));
    }
    let regular: Vec<(i64, u64)> = (0..1000).map(|i| (i * 10, 42.0f64.to_bits())).collect();
    let bytes = encode_chunk(&regular).len();
    println!(
        "chunk codec round trips; 1000 regular constant samples = {bytes} bytes ({:.2} bytes/sample vs 16 raw)",
        bytes as f64 / 1000.0
    );
    assert!(bytes < 2200);
    assert!(
        decode_chunk(&[5, 0]).is_none(),
        "truncated chunk is an error"
    );
}

fn test_semantics_demo() {
    let mut e = Engine::new(usize::MAX);
    let l = labels("reqs_total", &[("host", "h0")]);
    // counter that resets at t=30 (restart): 10 20 30 | 5 15
    for (t, v) in [(0, 10.0), (10, 20.0), (20, 30.0), (30, 5.0), (40, 15.0)] {
        e.write(&[(l.clone(), t, v)]).unwrap();
    }
    let ms = [Matcher::new("__name__", Op::Eq, "reqs_total").unwrap()];
    let raw = e.query_raw(&ms, 0, 100).remove(0).1;
    let rate = downsample(&raw, 0, 50, 50, Agg::Rate);
    // increases: 10 + 10 + 5 (reset: counts the new value from zero) + 10 = 35 over 40 time units
    assert_eq!(rate, vec![(0, 35.0 / 40.0)]);
    // staleness: the series disappears at t=50
    e.write_stale(&l, 50).unwrap();
    let raw = e.query_raw(&ms, 0, 100).remove(0).1;
    assert_eq!(eval_at(&raw, 45, 30), Some(15.0));
    assert_eq!(
        eval_at(&raw, 55, 30),
        None,
        "stale marker hides the series immediately, not after the lookback"
    );
    assert_eq!(eval_at(&raw, 100, 30), None);
    assert_eq!(
        downsample(&raw, 40, 60, 20, Agg::Last),
        vec![],
        "newest sample in the bucket is stale"
    );
    assert_eq!(
        downsample(&raw, 40, 60, 20, Agg::Count),
        vec![(40, 1.0)],
        "other aggregates just ignore the marker"
    );
    // a real NaN is NOT a stale marker
    e.write(&[(l.clone(), 60, f64::NAN)]).unwrap();
    let raw = e.query_raw(&ms, 0, 100).remove(0).1;
    assert!(eval_at(&raw, 61, 10).is_some_and(f64::is_nan));
    // rejected writes below the floor change nothing
    e.flush(100).unwrap();
    e.apply_retention(35);
    assert_eq!(
        e.write(&[(l.clone(), 10, 1.0)]),
        Err(EngineError::TooOld { ts: 10, floor: 35 })
    );
    let kept = e.query_raw(&ms, 0, 100).remove(0).1;
    assert!(kept.iter().all(|&(t, _)| t >= 35));
    println!("semantics demo (counter reset rate, staleness, retention floor): ok");
}

fn test_series_limit_and_cardinality() {
    println!("\ncardinality: series count multiplies across label dimensions");
    let mut e = Engine::new(usize::MAX);
    let step = |e: &mut Engine, extra: Option<usize>, label: &str| {
        let before = e.series.len();
        for h in 0..20 {
            for r in 0..4 {
                for ep in 0..5 {
                    let (hs, rs, es) = (format!("host-{h}"), format!("r{r}"), format!("/api/{ep}"));
                    let mut tags = vec![
                        ("host", hs.as_str()),
                        ("region", rs.as_str()),
                        ("endpoint", es.as_str()),
                    ];
                    let us;
                    if let Some(u) = extra {
                        // 1 user id per series would be one series per request: the classic explosion
                        us = format!("user-{}", (h * 20 + r * 5 + ep) % u);
                        tags.push(("user_id", us.as_str()));
                    }
                    e.write(&[(labels("http_requests", &tags), 1, 1.0)])
                        .unwrap();
                }
            }
        }
        println!(
            "  {label:<34} series {:>6} (+{:>5})  index entries {:>7}",
            e.series.len(),
            e.series.len() - before,
            e.index_entries()
        );
    };
    step(&mut e, None, "host(20) x region(4) x endpoint(5)");
    step(&mut e, Some(10), "+ user_id with 10 values");
    step(&mut e, Some(400), "+ user_id with 400 values");
    println!("  per-label distinct values: {:?}", e.cardinality_report());
    assert_eq!(e.cardinality_report()[0].0, "user_id");
    let mut limited = Engine::new(450);
    let mut rejected = 0;
    for u in 0..1000 {
        let us = format!("u{u}");
        if limited.write(&[(labels("m", &[("user", &us)]), 1, 1.0)])
            == Err(EngineError::SeriesLimit { limit: 450 })
        {
            rejected += 1;
        }
    }
    assert_eq!((limited.series.len(), rejected), (450, 550));
    // existing series still accept samples at the limit
    assert!(
        limited
            .write(&[(labels("m", &[("user", "u3")]), 2, 2.0)])
            .is_ok()
    );
    println!(
        "  max_series=450: accepted 450 series, rejected 550 new ones, existing series still writable"
    );
}

fn test_block_pruning() {
    let mut e = Engine::new(usize::MAX);
    let l = labels("cpu", &[("host", "h0")]);
    for day in 0..20 {
        for k in 0..10 {
            e.write(&[(l.clone(), day * 1000 + k, k as f64)]).unwrap();
        }
        e.flush((day + 1) * 1000).unwrap();
    }
    let ms = [Matcher::new("host", Op::Eq, "h0").unwrap()];
    let (s0, t0) = (e.stats.blocks_scanned, e.stats.blocks_total_seen);
    let got = e.query_raw(&ms, 5000, 6009);
    assert_eq!(got[0].1.len(), 20);
    let (scanned, total) = (e.stats.blocks_scanned - s0, e.stats.blocks_total_seen - t0);
    println!("\nblock index pruning: query over 2 of 20 blocks scanned {scanned}/{total} blocks");
    assert_eq!((scanned, total), (2, 20));
}

fn main() {
    let mut rng = Rng(0x7511_E5E7_0861);
    test_regex_and_matchers();
    test_postings(&mut rng);
    test_chunk_codec(&mut rng);
    test_semantics_demo();
    test_block_pruning();
    test_series_limit_and_cardinality();
    gallop_demo();

    let mut st = WorkloadStats::default();
    let mut ooo = 0;
    for seed in 0..2000 {
        run_workload(seed, &mut st, &mut ooo);
    }
    println!("\n2000 random workloads vs brute-force oracle: all equal");
    println!("  {st:?}");
    println!("  out-of-order head writes absorbed: {ooo}");
    assert!(
        st.torn > 300
            && st.clean_crashes > 300
            && st.flush_crashes > 300
            && st.compact_crashes > 300,
        "every crash kind must be exercised"
    );
    assert!(st.checks > 5000 && ooo > 1000);
    println!("\nall tsm_time_series_engine checks passed");
}
