//! RECIPE 862 (database): TimescaleDB-style hypertables: time/space chunking, chunk exclusion, columnar chunk compression, retention, continuous aggregates, gapfill.
//! LEVEL: advanced
//! Use in: understanding how a relational engine is turned into a time-series store by partitioning on time and pre-aggregating.   Run: cargo run --release --example hypertables_continuous_aggregates
//! COMMON USES: TimescaleDB hypertables and continuous aggregates; the same ideas appear in PostgreSQL declarative partitioning with pg_partman,
//! ClickHouse materialized views over MergeTree partitions, Druid/Pinot segment pruning and rollups, and BigQuery/Snowflake partition pruning.
//! Course: CMU 15-445 (partitioning, materialized views), CMU Database Group time-series talks (TimescaleDB).
#![allow(dead_code)]
//!
//! WHAT: a HYPERTABLE looks like one table `(ts, device, value)` but is stored as many CHUNKS keyed by (time slot, space partition). Queries with a
//! time (or device) predicate only touch chunks whose range can match: CHUNK EXCLUSION. Old chunks are cheap to drop as a whole (RETENTION)
//! and cheap to store if converted to compressed columnar SEGMENTS. A CONTINUOUS AGGREGATE (cagg) keeps a materialised rollup per
//! (time_bucket, device) so dashboards do not rescan raw data.
//! HOW:
//! * CHUNKS: slot = ts.div_euclid(interval); space = hash(device) % S. `scan(lo, hi, device)` uses a BTreeMap range over (slot, space) so
//!   excluded chunks are never even visited; with a device equality predicate the space dimension prunes too.
//! * COMPRESSION: a chunk older than the policy age becomes SEGMENTS: rows grouped by the SEGMENT-BY column (device), sorted by the ORDER-BY column
//!   (ts), each segment holding delta+varint timestamps, XOR+varint values and min/max metadata that lets a query skip it without decoding.
//!   Late inserts into a compressed chunk land in an uncompressed staging area (as in TimescaleDB); the next policy run recompresses it.
//!   Deleting from compressed data first decompresses the chunk.
//! * RETENTION: `drop_chunks(cutoff)` removes whole chunks with end <= cutoff (partial chunks stay: the guarantee is chunk-granular), advances a
//!   floor (older inserts are rejected) and FIRST refreshes caggs up to that floor, so the rollup keeps the history the raw data loses.
//! * CAGG: `mat[(bucket, device)] -> (count, sum, min, max, last)` for buckets below a WATERMARK. Any write below the watermark appends its time range to
//!   an INVALIDATION LOG (inserts at or above it are the tail, found by the real-time part). `refresh(end)` re-materialises invalidated buckets from raw
//!   data and extends the watermark. `RealTime` read = materialised part UNION raw tail aggregated on the fly (TimescaleDB's default); it is stale
//!   only in buckets with pending invalidations. `Fresh` read additionally recomputes those buckets.
//! * GAPFILL: `time_bucket_gapfill` semantics over sparse bucket values: NULL, zero, LOCF (last observation carried forward, may come from before
//!   the window) and linear interpolation between the neighbours on both sides.
//! TESTS: 2000 random insert / late arrival / delete / compress / retention / refresh / query sequences compared with a brute-force oracle that keeps
//! all rows and recomputes every aggregate; plus 3000 random sparse series checking gapfill against an O(n*m) scan. Pruning statistics are printed.
//!
//! Read: TimescaleDB docs, hypertables  https://docs.timescale.com/use-timescale/latest/hypertables/
//! Read: TimescaleDB docs, continuous aggregates  https://docs.timescale.com/use-timescale/latest/continuous-aggregates/
//! Read: Freedman et al., "Timescale: SQL made scalable for time-series data" (TimescaleDB design paper; cited by title, link not checked)
//!
//! TRY NEXT:
//!   - [S] Change `time_bucket` to take an origin (like `time_bucket(width, ts, origin)`) and fix the one place the oracle and the engine must agree on it.
//!     Predict which test breaks first if only the engine uses the origin.
//!   - [S] In `drop_chunks`, remove the "refresh before drop" step. Which assertion in `run_workload` fails and what exactly is lost from the rollup?
//!   - [M] Add a `Fill::Previous` variant to `gapfill` that, unlike LOCF, never crosses a gap longer than N buckets; extend `gapfill_oracle` and the random test.
//!   - [M] The invalidation log is unbounded under heavy late data. Coalesce ranges on insert (keep it sorted and merged) and measure how many ranges the
//!     workload keeps pending at its peak before and after.
//!   - [A] PROJECT: drive this hypertable from the `tsm_time_series_engine` write path (WAL, crash recovery) so chunks and the invalidation log survive a crash, then
//!     compare chunk-exclusion pruning with that recipe's block pruning on the same data.

use std::collections::BTreeMap;

const NEG: i64 = -(1 << 40);

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
}

// ---------------------------------------------------------------- rows, buckets
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Row {
    ts: i64,
    dev: u32,
    bits: u64, // f64 bits
}
impl Row {
    fn val(&self) -> f64 {
        f64::from_bits(self.bits)
    }
}
fn time_bucket(w: i64, ts: i64) -> i64 {
    ts.div_euclid(w) * w
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AggRow {
    count: u64,
    sum: f64,
    min: f64,
    max: f64,
    last: (i64, u64), // (ts, bits) with the largest (ts, bits): deterministic even with equal timestamps
}
impl AggRow {
    fn first(r: &Row) -> AggRow {
        AggRow {
            count: 1,
            sum: r.val(),
            min: r.val(),
            max: r.val(),
            last: (r.ts, r.bits),
        }
    }
    fn add(&mut self, r: &Row) {
        self.count += 1;
        self.sum += r.val();
        self.min = self.min.min(r.val());
        self.max = self.max.max(r.val());
        self.last = self.last.max((r.ts, r.bits));
    }
    fn avg(&self) -> f64 {
        self.sum / self.count as f64
    }
}
type Buckets = BTreeMap<(i64, u32), AggRow>;
fn accumulate(b: &mut Buckets, w: i64, r: &Row) {
    match b.get_mut(&(time_bucket(w, r.ts), r.dev)) {
        Some(a) => a.add(r),
        None => {
            b.insert((time_bucket(w, r.ts), r.dev), AggRow::first(r));
        }
    }
}

// ---------------------------------------------------------------- segment codec (segment-by device, order-by ts)
fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}
fn get_varint(b: &[u8], pos: &mut usize) -> u64 {
    let (mut v, mut shift) = (0u64, 0);
    loop {
        let byte = b[*pos];
        *pos += 1;
        v |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return v;
        }
        shift += 7;
    }
}
#[derive(Clone)]
struct Segment {
    dev: u32,
    mint: i64,
    maxt: i64,
    n: usize,
    ts_col: Vec<u8>,  // first ts (zigzag varint) then non-negative deltas
    val_col: Vec<u8>, // per value: 0 (same) or trailing_zeros+1 then varint of the XOR with the previous value, shifted
}
impl Segment {
    fn encode(dev: u32, sorted: &[(i64, u64)]) -> Segment {
        let (mut ts_col, mut val_col) = (vec![], vec![]);
        let (mut pt, mut pv) = (0i64, 0u64);
        for (i, &(t, v)) in sorted.iter().enumerate() {
            if i == 0 {
                put_varint(&mut ts_col, ((t << 1) ^ (t >> 63)) as u64);
            } else {
                put_varint(&mut ts_col, (t - pt) as u64);
            }
            let x = v ^ pv;
            if x == 0 {
                val_col.push(0);
            } else {
                // trailing zeros are shifted away (decimal-ish floats have many), so the varint stays short
                val_col.push(x.trailing_zeros() as u8 + 1);
                put_varint(&mut val_col, x >> x.trailing_zeros());
            }
            (pt, pv) = (t, v);
        }
        Segment {
            dev,
            mint: sorted[0].0,
            maxt: sorted[sorted.len() - 1].0,
            n: sorted.len(),
            ts_col,
            val_col,
        }
    }
    fn decode(&self) -> Vec<Row> {
        let (mut tp, mut vp) = (0, 0);
        let (mut t, mut v) = (0i64, 0u64);
        (0..self.n)
            .map(|i| {
                let d = get_varint(&self.ts_col, &mut tp);
                t = if i == 0 {
                    ((d >> 1) as i64) ^ -((d & 1) as i64)
                } else {
                    t + d as i64
                };
                let c = self.val_col[vp];
                vp += 1;
                if c != 0 {
                    v ^= get_varint(&self.val_col, &mut vp) << (c - 1);
                }
                Row {
                    ts: t,
                    dev: self.dev,
                    bits: v,
                }
            })
            .collect()
    }
    fn bytes(&self) -> usize {
        self.ts_col.len() + self.val_col.len() + 24 // + per-segment metadata (dev, min, max, n)
    }
}
const SEGMENT_ROWS: usize = 64;

// ---------------------------------------------------------------- hypertable
#[derive(Default, Clone)]
struct Chunk {
    rows: Vec<Row>, // uncompressed rows (all rows, or the staging area of a compressed chunk)
    segments: Vec<Segment>,
}
impl Chunk {
    fn row_count(&self) -> usize {
        self.rows.len() + self.segments.iter().map(|s| s.n).sum::<usize>()
    }
    fn decompress(&mut self) {
        for s in self.segments.drain(..) {
            self.rows.extend(s.decode());
        }
    }
    fn compress(&mut self) {
        self.decompress();
        let mut by_dev: BTreeMap<u32, Vec<(i64, u64)>> = BTreeMap::new();
        for r in self.rows.drain(..) {
            by_dev.entry(r.dev).or_default().push((r.ts, r.bits));
        }
        for (dev, mut pts) in by_dev {
            pts.sort();
            self.segments
                .extend(pts.chunks(SEGMENT_ROWS).map(|c| Segment::encode(dev, c)));
        }
    }
}

#[derive(Default, Debug, Clone)]
struct ScanStats {
    scans: u64,
    chunks_total: u64,
    chunks_scanned: u64,
    segments_seen: u64,
    segments_skipped: u64,
}

struct Hypertable {
    interval: i64,
    space: u32,
    chunks: BTreeMap<(i64, u32), Chunk>,
    floor: i64,
    stats: ScanStats,
    decompress_on_delete: u64,
}
impl Hypertable {
    fn new(interval: i64, space: u32) -> Self {
        Hypertable {
            interval,
            space,
            chunks: BTreeMap::new(),
            floor: NEG,
            stats: ScanStats::default(),
            decompress_on_delete: 0,
        }
    }
    fn space_of(&self, dev: u32) -> u32 {
        (((dev as u64).wrapping_mul(0x9E3779B97F4A7C15) >> 33) % self.space as u64) as u32
    }
    fn insert(&mut self, r: Row) {
        let key = (r.ts.div_euclid(self.interval), self.space_of(r.dev));
        self.chunks.entry(key).or_default().rows.push(r);
    }
    /// Rows with lo <= ts < hi (and device, when given). Chunk exclusion by BTreeMap range + space hash; segment skipping by metadata.
    fn scan(&mut self, lo: i64, hi: i64, dev: Option<u32>) -> Vec<Row> {
        self.stats.scans += 1;
        self.stats.chunks_total += self.chunks.len() as u64;
        let mut out = vec![];
        if lo >= hi {
            return out;
        }
        let (s_lo, s_hi) = (
            lo.div_euclid(self.interval),
            (hi - 1).div_euclid(self.interval),
        );
        let want_space = dev.map(|d| self.space_of(d));
        for (&(_, sp), chunk) in self.chunks.range((s_lo, 0)..=(s_hi, u32::MAX)) {
            if want_space.is_some_and(|w| w != sp) {
                continue;
            }
            self.stats.chunks_scanned += 1;
            out.extend(
                chunk
                    .rows
                    .iter()
                    .filter(|r| r.ts >= lo && r.ts < hi && dev.is_none_or(|d| d == r.dev)),
            );
            for s in &chunk.segments {
                self.stats.segments_seen += 1;
                if s.maxt < lo || s.mint >= hi || dev.is_some_and(|d| d != s.dev) {
                    self.stats.segments_skipped += 1;
                    continue;
                }
                out.extend(s.decode().into_iter().filter(|r| r.ts >= lo && r.ts < hi));
            }
        }
        out
    }
    /// Delete rows in [lo, hi) (optionally one device). Returns the number removed. Compressed chunks are decompressed first.
    fn delete(&mut self, lo: i64, hi: i64, dev: Option<u32>) -> usize {
        let lo = lo.max(self.floor);
        if lo >= hi {
            return 0;
        }
        let (s_lo, s_hi) = (
            lo.div_euclid(self.interval),
            (hi - 1).div_euclid(self.interval),
        );
        let mut removed = 0;
        for (_, chunk) in self.chunks.range_mut((s_lo, 0)..=(s_hi, u32::MAX)) {
            if chunk
                .segments
                .iter()
                .any(|s| s.maxt >= lo && s.mint < hi && dev.is_none_or(|d| d == s.dev))
            {
                chunk.decompress();
                self.decompress_on_delete += 1;
            }
            let before = chunk.rows.len();
            chunk
                .rows
                .retain(|r| !(r.ts >= lo && r.ts < hi && dev.is_none_or(|d| d == r.dev)));
            removed += before - chunk.rows.len();
        }
        removed
    }
    /// Compression policy: (re)compress every chunk that ended at or before `older_than` and has uncompressed rows.
    fn compress_policy(&mut self, older_than: i64) -> usize {
        let interval = self.interval;
        let mut n = 0;
        for (&(slot, _), chunk) in self.chunks.iter_mut() {
            if (slot + 1) * interval <= older_than && !chunk.rows.is_empty() {
                chunk.compress();
                n += 1;
            }
        }
        n
    }
    fn drop_chunks_below(&mut self, floor: i64) -> usize {
        let slot = floor.div_euclid(self.interval);
        let before = self.chunks.len();
        self.chunks.retain(|&(s, _), _| s >= slot);
        self.floor = self.floor.max(floor);
        before - self.chunks.len()
    }
    fn storage(&self) -> (usize, usize) {
        let raw = self.chunks.values().map(|c| c.row_count() * 20).sum();
        let stored = self
            .chunks
            .values()
            .map(|c| c.rows.len() * 20 + c.segments.iter().map(|s| s.bytes()).sum::<usize>())
            .sum();
        (raw, stored)
    }
}

// ---------------------------------------------------------------- continuous aggregate
#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    RealTime,
    Fresh,
}
#[derive(Default, Debug)]
struct CaggStats {
    invalidations: u64,
    rematerialised_ranges: u64,
    refreshes: u64,
}
struct Cagg {
    w: i64,
    mat: Buckets,
    wm: i64,                  // buckets starting below this are materialised
    pending: Vec<(i64, i64)>, // bucket-aligned half-open ranges awaiting re-materialisation
    stats: CaggStats,
}
impl Cagg {
    fn new(w: i64) -> Self {
        Cagg {
            w,
            mat: BTreeMap::new(),
            wm: NEG,
            pending: vec![],
            stats: CaggStats::default(),
        }
    }
    fn compute(&self, ht: &mut Hypertable, lo: i64, hi: i64, dev: Option<u32>) -> Buckets {
        let mut b = Buckets::new();
        for r in ht.scan(lo, hi, dev) {
            accumulate(&mut b, self.w, &r);
        }
        b
    }
    /// Record that raw rows in the time range [lo, hi_incl] changed. Only the already-materialised part (< wm) needs logging.
    fn invalidate(&mut self, lo: i64, hi_incl: i64) {
        let hi_incl = hi_incl.min(self.wm - 1);
        if lo > hi_incl {
            return;
        }
        self.pending.push((
            time_bucket(self.w, lo),
            time_bucket(self.w, hi_incl) + self.w,
        ));
        self.stats.invalidations += 1;
    }
    fn merged_pending(&self) -> Vec<(i64, i64)> {
        let mut v = self.pending.clone();
        v.sort();
        let mut out: Vec<(i64, i64)> = vec![];
        for (a, b) in v {
            match out.last_mut() {
                Some(l) if a <= l.1 => l.1 = l.1.max(b),
                _ => out.push((a, b)),
            }
        }
        out
    }
    /// Re-materialise invalidated ranges, then extend the watermark to `end` (rounded down to a bucket boundary).
    fn refresh(&mut self, ht: &mut Hypertable, end: i64) {
        self.stats.refreshes += 1;
        let lo_limit = ht.floor;
        for (a, b) in self.merged_pending() {
            self.rematerialise(ht, a.max(lo_limit), b);
        }
        self.pending.clear();
        let new_wm = time_bucket(self.w, end);
        if new_wm > self.wm {
            let from = self.wm.max(lo_limit);
            self.rematerialise(ht, from, new_wm);
            self.wm = new_wm;
        }
    }
    fn rematerialise(&mut self, ht: &mut Hypertable, lo: i64, hi: i64) {
        if lo >= hi {
            return;
        }
        self.stats.rematerialised_ranges += 1;
        let keys: Vec<_> = self.mat.range((lo, 0)..(hi, 0)).map(|(k, _)| *k).collect();
        keys.iter().for_each(|k| {
            self.mat.remove(k);
        });
        let fresh = self.compute(ht, lo, hi, None);
        self.mat.extend(fresh);
    }
    /// Read buckets with start in [lo, hi) (both multiples of w).
    fn read(&self, ht: &mut Hypertable, lo: i64, hi: i64, dev: Option<u32>, mode: Mode) -> Buckets {
        assert!(lo.rem_euclid(self.w) == 0 && hi.rem_euclid(self.w) == 0);
        let keep = |k: &(i64, u32)| dev.is_none_or(|d| d == k.1);
        let upto = hi.min(self.wm);
        let mut out: Buckets = if lo < upto {
            self.mat
                .range((lo, 0)..(upto, 0))
                .filter(|(k, _)| keep(k))
                .map(|(k, v)| (*k, *v))
                .collect()
        } else {
            Buckets::new()
        };
        if mode == Mode::Fresh {
            for (a, b) in self.merged_pending() {
                let (a, b) = (a.max(lo).max(ht.floor), b.min(hi).min(self.wm));
                if a < b {
                    out.retain(|k, _| k.0 < a || k.0 >= b);
                    out.extend(self.compute(ht, a, b, dev));
                }
            }
        }
        let tail_lo = lo.max(self.wm);
        if tail_lo < hi {
            out.extend(self.compute(ht, tail_lo.max(ht.floor), hi, dev)); // real-time part: raw rows above the watermark
        }
        out
    }
}

// ---------------------------------------------------------------- database facade
#[derive(Debug, PartialEq)]
enum DbError {
    TooOld { ts: i64, floor: i64 },
}
struct Db {
    ht: Hypertable,
    caggs: Vec<Cagg>,
}
impl Db {
    fn new(interval: i64, space: u32, widths: &[i64]) -> Db {
        assert!(
            widths.iter().all(|w| interval % w == 0),
            "a bucket must not straddle a chunk boundary"
        );
        Db {
            ht: Hypertable::new(interval, space),
            caggs: widths.iter().map(|&w| Cagg::new(w)).collect(),
        }
    }
    fn insert(&mut self, rows: &[Row]) -> Result<(), DbError> {
        if let Some(r) = rows.iter().find(|r| r.ts < self.ht.floor) {
            return Err(DbError::TooOld {
                ts: r.ts,
                floor: self.ht.floor,
            });
        }
        for r in rows {
            self.ht.insert(*r);
            self.caggs.iter_mut().for_each(|c| c.invalidate(r.ts, r.ts));
        }
        Ok(())
    }
    fn delete(&mut self, lo: i64, hi: i64, dev: Option<u32>) -> usize {
        let lo = lo.max(self.ht.floor);
        let n = self.ht.delete(lo, hi, dev);
        self.caggs.iter_mut().for_each(|c| c.invalidate(lo, hi - 1));
        n
    }
    fn refresh_all(&mut self, end: i64) {
        for c in &mut self.caggs {
            c.refresh(&mut self.ht, end);
        }
    }
    /// Retention. Materialise first: the rollup must keep what the raw table is about to lose.
    fn drop_chunks(&mut self, cutoff: i64) -> usize {
        let floor = time_bucket(self.ht.interval, cutoff);
        if floor <= self.ht.floor {
            return 0;
        }
        self.refresh_all(floor);
        self.ht.drop_chunks_below(floor)
    }
}

// ---------------------------------------------------------------- gapfill
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fill {
    Null,
    Zero,
    Locf,
    Linear,
}
fn lerp(vp: f64, vn: f64, bp: i64, bn: i64, b: i64) -> f64 {
    vp + (vn - vp) * ((b - bp) as f64 / (bn - bp) as f64)
}
/// `sparse` = (bucket, value) sorted by bucket with unique buckets and may include buckets outside [start, end) (the neighbours
/// that LOCF and interpolation need). Output one entry per bucket start in [start, end).
fn gapfill(
    sparse: &[(i64, f64)],
    start: i64,
    end: i64,
    w: i64,
    fill: Fill,
) -> Vec<(i64, Option<f64>)> {
    let mut out = vec![];
    let mut b = start;
    while b < end {
        let i = sparse.partition_point(|&(t, _)| t < b); // first >= b
        let v = if i < sparse.len() && sparse[i].0 == b {
            Some(sparse[i].1)
        } else {
            let prev = i.checked_sub(1).map(|j| sparse[j]);
            let next = sparse.get(i).copied();
            match fill {
                Fill::Null => None,
                Fill::Zero => Some(0.0),
                Fill::Locf => prev.map(|p| p.1),
                Fill::Linear => prev.zip(next).map(|(p, n)| lerp(p.1, n.1, p.0, n.0, b)),
            }
        };
        out.push((b, v));
        b += w;
    }
    out
}
fn gapfill_oracle(
    sparse: &[(i64, f64)],
    start: i64,
    end: i64,
    w: i64,
    fill: Fill,
) -> Vec<(i64, Option<f64>)> {
    (0..)
        .map(|k| start + k * w)
        .take_while(|&b| b < end)
        .map(|b| {
            let exact = sparse.iter().find(|p| p.0 == b).map(|p| p.1);
            let prev = sparse.iter().filter(|p| p.0 < b).last();
            let next = sparse.iter().find(|p| p.0 > b);
            let v = exact.or(match fill {
                Fill::Null => None,
                Fill::Zero => Some(0.0),
                Fill::Locf => prev.map(|p| p.1),
                Fill::Linear => match (prev, next) {
                    (Some(p), Some(n)) => Some(lerp(p.1, n.1, p.0, n.0, b)),
                    _ => None,
                },
            });
            (b, v)
        })
        .collect()
}

// ---------------------------------------------------------------- oracle
struct Oracle {
    hist: Vec<Row>, // every row ever inserted and not deleted (rows below `floor` stay: they live on in the rollups)
    floor: i64,
    interval: i64,
}
impl Oracle {
    fn insert(&mut self, rows: &[Row]) -> Result<(), DbError> {
        if let Some(r) = rows.iter().find(|r| r.ts < self.floor) {
            return Err(DbError::TooOld {
                ts: r.ts,
                floor: self.floor,
            });
        }
        self.hist.extend_from_slice(rows);
        Ok(())
    }
    fn delete(&mut self, lo: i64, hi: i64, dev: Option<u32>) -> usize {
        let lo = lo.max(self.floor);
        let before = self.hist.len();
        self.hist
            .retain(|r| !(r.ts >= lo && r.ts < hi && dev.is_none_or(|d| d == r.dev)));
        before - self.hist.len()
    }
    fn drop_chunks(&mut self, cutoff: i64) {
        self.floor = self.floor.max(time_bucket(self.interval, cutoff));
    }
    fn raw(&self, lo: i64, hi: i64, dev: Option<u32>) -> Vec<Row> {
        let mut v: Vec<Row> = self
            .hist
            .iter()
            .copied()
            .filter(|r| r.ts >= lo.max(self.floor) && r.ts < hi && dev.is_none_or(|d| d == r.dev))
            .collect();
        v.sort();
        v
    }
    fn cagg(&self, w: i64, lo: i64, hi: i64, dev: Option<u32>) -> Buckets {
        let mut b = Buckets::new();
        for r in self.hist.iter().filter(|r| {
            time_bucket(w, r.ts) >= lo
                && time_bucket(w, r.ts) < hi
                && dev.is_none_or(|d| d == r.dev)
        }) {
            accumulate(&mut b, w, r);
        }
        b
    }
}

// ---------------------------------------------------------------- workloads
fn val(rng: &mut Rng) -> u64 {
    (rng.range(-40, 40) as f64 * 0.25).to_bits() // multiples of 1/4: sums are exact in any order
}
#[derive(Default, Debug)]
struct Totals {
    ops: u64,
    inserts_rejected: u64,
    late_inserts: u64,
    deletes: u64,
    refreshes: u64,
    compress_runs: u64,
    chunks_dropped: u64,
    realtime_checks: u64,
    stale_buckets_masked: u64,
    fresh_checks: u64,
    invalidations: u64,
    rematerialised: u64,
    decompress_on_delete: u64,
    scan: ScanStats,
}

fn sorted_rows(mut v: Vec<Row>) -> Vec<Row> {
    v.sort();
    v
}

fn run_workload(seed: u64, t: &mut Totals) {
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
    let interval = 100;
    let widths_pool = [5i64, 10, 20, 25, 50, 100];
    let widths = [
        widths_pool[rng.below(6) as usize],
        widths_pool[rng.below(6) as usize],
    ];
    let space = [1u32, 3][rng.below(2) as usize];
    let mut db = Db::new(interval, space, &widths);
    let mut o = Oracle {
        hist: vec![],
        floor: NEG,
        interval,
    };
    let mut now = 0i64;
    for i in 0..rng.range(30, 120) {
        t.ops += 1;
        let ctx = format!("seed {seed} op {i}");
        now += rng.range(0, 25);
        match rng.below(100) {
            0..=44 => {
                let rows: Vec<Row> = (0..rng.range(1, 8))
                    .map(|_| {
                        let late = rng.chance(25);
                        t.late_inserts += late as u64;
                        let ts = if late {
                            now - rng.range(0, 500)
                        } else {
                            now + rng.range(0, 10)
                        };
                        Row {
                            ts,
                            dev: rng.below(8) as u32,
                            bits: val(&mut rng),
                        }
                    })
                    .collect();
                let (a, b) = (db.insert(&rows), o.insert(&rows));
                assert_eq!(a, b, "{ctx}: insert verdict");
                t.inserts_rejected += a.is_err() as u64;
            }
            45..=50 => {
                let (a, b) = (now - rng.range(0, 500), now - rng.range(-20, 400));
                let dev = rng.chance(50).then(|| rng.below(8) as u32);
                assert_eq!(
                    db.delete(a.min(b), a.max(b), dev),
                    o.delete(a.min(b), a.max(b), dev),
                    "{ctx}: rows deleted"
                );
                t.deletes += 1;
            }
            51..=60 => {
                db.refresh_all(now - rng.range(0, 120));
                t.refreshes += 1;
                assert!(db.caggs.iter().all(|c| c.pending.is_empty()));
                for c in &db.caggs {
                    let (lo, hi) = (time_bucket(c.w, NEG), time_bucket(c.w, now + 1000) + c.w);
                    let got = c.read(&mut db.ht, lo, hi, None, Mode::RealTime);
                    assert_eq!(
                        got,
                        o.cagg(c.w, lo, hi, None),
                        "{ctx}: after refresh RealTime must equal the oracle exactly"
                    );
                }
            }
            61..=68 => {
                db.ht.compress_policy(now - rng.range(0, 300));
                t.compress_runs += 1;
            }
            69..=72 => {
                let cutoff = (now - 150 - rng.range(0, 250)).max(o.floor);
                t.chunks_dropped += db.drop_chunks(cutoff) as u64;
                o.drop_chunks(cutoff);
                assert_eq!(db.ht.floor.max(NEG), o.floor.max(NEG));
            }
            _ => {
                let (a, b) = (now - rng.range(0, 600), now + rng.range(-100, 30));
                let (lo, hi) = (a.min(b), a.max(b));
                let dev = rng.chance(40).then(|| rng.below(8) as u32);
                assert_eq!(
                    sorted_rows(db.ht.scan(lo, hi, dev)),
                    o.raw(lo, hi, dev),
                    "{ctx}: raw scan [{lo},{hi}) {dev:?}"
                );
                for c in &db.caggs {
                    let (blo, bhi) = (time_bucket(c.w, lo), time_bucket(c.w, hi) + c.w);
                    let want = o.cagg(c.w, blo, bhi, dev);
                    assert_eq!(
                        c.read(&mut db.ht, blo, bhi, dev, Mode::Fresh),
                        want,
                        "{ctx}: Fresh read w={}",
                        c.w
                    );
                    t.fresh_checks += 1;
                    // RealTime may only differ inside buckets with pending invalidations
                    let pend = c.merged_pending();
                    let masked = |m: &Buckets| -> Buckets {
                        m.iter()
                            .filter(|(k, _)| !pend.iter().any(|&(a, b)| k.0 >= a && k.0 < b))
                            .map(|(k, v)| (*k, *v))
                            .collect()
                    };
                    let rt = c.read(&mut db.ht, blo, bhi, dev, Mode::RealTime);
                    assert_eq!(
                        masked(&rt),
                        masked(&want),
                        "{ctx}: RealTime read outside pending ranges, w={}",
                        c.w
                    );
                    t.stale_buckets_masked +=
                        (want.len() as i64 - masked(&want).len() as i64).max(0) as u64;
                    t.realtime_checks += 1;
                }
            }
        }
    }
    // final: everything refreshed must match; compression + decompression of every chunk must preserve rows
    db.refresh_all(now + 500);
    db.ht.compress_policy(now + 10_000);
    let all = sorted_rows(db.ht.scan(NEG, now + 10_000, None));
    assert_eq!(
        all,
        o.raw(NEG, now + 10_000, None),
        "seed {seed}: final full scan after compressing everything"
    );
    t.invalidations += db.caggs.iter().map(|c| c.stats.invalidations).sum::<u64>();
    t.rematerialised += db
        .caggs
        .iter()
        .map(|c| c.stats.rematerialised_ranges)
        .sum::<u64>();
    t.decompress_on_delete += db.ht.decompress_on_delete;
    let s = &db.ht.stats;
    t.scan.scans += s.scans;
    t.scan.chunks_total += s.chunks_total;
    t.scan.chunks_scanned += s.chunks_scanned;
    t.scan.segments_seen += s.segments_seen;
    t.scan.segments_skipped += s.segments_skipped;
}

fn test_gapfill(rng: &mut Rng) {
    for case in 0..3000 {
        let w = [1i64, 5, 10][rng.below(3) as usize];
        let n = rng.below(12);
        let mut present: Vec<i64> = (0..n).map(|_| rng.range(-5, 40) * w).collect();
        present.sort();
        present.dedup();
        let sparse: Vec<(i64, f64)> = present
            .into_iter()
            .map(|b| (b, rng.range(-20, 20) as f64 * 0.5))
            .collect();
        let (start, end) = (rng.range(-3, 20) * w, rng.range(0, 45) * w);
        for fill in [Fill::Null, Fill::Zero, Fill::Locf, Fill::Linear] {
            assert_eq!(
                gapfill(&sparse, start, end, w, fill),
                gapfill_oracle(&sparse, start, end, w, fill),
                "case {case} {fill:?}"
            );
        }
    }
    // worked example: values at buckets 10 and 40 (width 10), window [0, 60)
    let sparse = [(10, 1.0), (40, 4.0)];
    let show = |f| {
        gapfill(&sparse, 0, 60, 10, f)
            .iter()
            .map(|&(_, v)| v)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        show(Fill::Null),
        [None, Some(1.0), None, None, Some(4.0), None]
    );
    assert_eq!(
        show(Fill::Zero),
        [
            Some(0.0),
            Some(1.0),
            Some(0.0),
            Some(0.0),
            Some(4.0),
            Some(0.0)
        ]
    );
    assert_eq!(
        show(Fill::Locf),
        [None, Some(1.0), Some(1.0), Some(1.0), Some(4.0), Some(4.0)]
    );
    assert_eq!(
        show(Fill::Linear),
        [None, Some(1.0), Some(2.0), Some(3.0), Some(4.0), None]
    );
    // LOCF may reach before the window: the carried value comes from outside it
    assert_eq!(
        gapfill(&sparse, 20, 40, 10, Fill::Locf),
        vec![(20, Some(1.0)), (30, Some(1.0))]
    );
    println!("gapfill (null/zero/locf/linear) vs O(n*m) oracle on 3000 random sparse series: ok");
    println!(
        "  window [0,60) over buckets {{10: 1.0, 40: 4.0}}: null {:?}\n  locf {:?}\n  linear {:?}",
        show(Fill::Null),
        show(Fill::Locf),
        show(Fill::Linear)
    );
    assert_eq!(
        (
            time_bucket(10, -1),
            time_bucket(10, -10),
            time_bucket(10, -11),
            time_bucket(10, 9),
            time_bucket(10, 10)
        ),
        (-10, -10, -20, 0, 10)
    );
}

fn demo_pruning_and_compression(rng: &mut Rng) {
    println!("\npruning and compression demo: 1 device-day per chunk, 30 devices, 200 chunks");
    let mut ht = Hypertable::new(1000, 4);
    let n_chunks = 200;
    for slot in 0..n_chunks {
        for dev in 0..30u32 {
            for k in 0..40 {
                // slowly varying sensor with two decimals-ish steps (Gorilla-friendly)
                let v = 20.0 + dev as f64 * 0.5 + ((slot * 40 + k) as f64 / 7.0).sin() * 0.25;
                ht.insert(Row {
                    ts: slot * 1000 + k * 25,
                    dev,
                    bits: ((v * 4.0).round() / 4.0).to_bits(),
                });
            }
        }
    }
    let (raw0, stored0) = ht.storage();
    assert_eq!(raw0, stored0);
    let rows_total: usize = ht.chunks.values().map(|c| c.row_count()).sum();
    // time predicate: 1 hour of 200
    ht.stats = ScanStats::default();
    let a = ht.scan(50_000, 52_000, None);
    println!(
        "  time range covering 2 slots:   chunks scanned {}/{}",
        ht.stats.chunks_scanned, ht.stats.chunks_total
    );
    assert_eq!(
        (ht.stats.chunks_scanned, ht.stats.chunks_total),
        (8, 800),
        "2 slots x 4 space partitions of 200 slots x 4"
    );
    // time + device predicate: space partition prunes too
    ht.stats = ScanStats::default();
    let b = ht.scan(50_000, 52_000, Some(7));
    println!(
        "  + device = 7 (space prune):    chunks scanned {}/{}",
        ht.stats.chunks_scanned, ht.stats.chunks_total
    );
    assert_eq!(ht.stats.chunks_scanned, 2);
    assert!(b.iter().all(|r| r.dev == 7) && b.len() == 80 && a.len() == 30 * 80);
    // compress everything older than slot 190
    let n = ht.compress_policy(190_000);
    let (raw, stored) = ht.storage();
    println!(
        "  compressed {n} chunks: {raw} B raw-equivalent -> {stored} B stored ({:.1}x); {rows_total} rows",
        raw as f64 / stored as f64
    );
    assert!(raw as f64 / stored as f64 > 1.8);
    // segment-by / segment metadata: a device + narrow time predicate on a compressed chunk skips most segments
    ht.stats = ScanStats::default();
    let c = ht.scan(10_000, 10_100, Some(3));
    println!(
        "  device=3, 100ms window on a compressed chunk: segments skipped {}/{}",
        ht.stats.segments_skipped, ht.stats.segments_seen
    );
    assert_eq!(c.len(), 4);
    assert!(ht.stats.segments_skipped * 2 >= ht.stats.segments_seen);
    // retention
    let before = ht.chunks.len();
    let dropped = ht.drop_chunks_below(150_000);
    println!(
        "  drop_chunks below slot 150: {dropped} of {before} chunks removed in O(chunks), no per-row work"
    );
    assert_eq!(dropped, 150 * 4);
    let _ = rng.next();
}

fn demo_cagg() {
    println!("\ncontinuous aggregate demo (bucket 10, chunk 100):");
    let mut db = Db::new(100, 1, &[10]);
    let ins = |db: &mut Db, ts: i64, v: f64| {
        db.insert(&[Row {
            ts,
            dev: 1,
            bits: v.to_bits(),
        }])
        .unwrap()
    };
    for ts in (0..100).step_by(5) {
        ins(&mut db, ts, 1.0);
    }
    db.refresh_all(60); // materialises buckets 0..60
    let read = |db: &mut Db, mode| db.caggs[0].read(&mut db.ht, 0, 100, None, mode);
    let m = read(&mut db, Mode::RealTime);
    assert_eq!(m[&(30, 1)].count, 2);
    assert_eq!(db.caggs[0].wm, 60);
    // late arrival into bucket 30 (materialised): invalidation logged, RealTime still stale, Fresh and refresh correct
    ins(&mut db, 33, 100.0);
    assert_eq!(db.caggs[0].merged_pending(), vec![(30, 40)]);
    assert_eq!(
        read(&mut db, Mode::RealTime)[&(30, 1)].count,
        2,
        "stale until refresh (materialised part only)"
    );
    assert_eq!(read(&mut db, Mode::Fresh)[&(30, 1)].count, 3);
    // arrival in the tail (>= watermark): needs no invalidation, real-time part sees it at once
    ins(&mut db, 71, 7.0);
    assert_eq!(db.caggs[0].merged_pending(), vec![(30, 40)]);
    assert_eq!(read(&mut db, Mode::RealTime)[&(70, 1)].count, 3);
    db.refresh_all(60);
    assert!(db.caggs[0].pending.is_empty());
    assert_eq!(
        read(&mut db, Mode::RealTime)[&(30, 1)],
        read(&mut db, Mode::Fresh)[&(30, 1)]
    );
    println!(
        "  late row at t=33 -> invalidation (30,40); RealTime stale (count 2), Fresh/refresh see count 3; tail row t=71 needs no log entry"
    );
    // retention keeps the rollup: drop raw chunks, buckets stay
    let kept = read(&mut db, Mode::Fresh)[&(0, 1)];
    db.drop_chunks(100);
    assert_eq!(db.ht.scan(0, 100, None).len(), 0, "raw rows are gone");
    assert_eq!(
        read(&mut db, Mode::Fresh)[&(0, 1)],
        kept,
        "rollup survives retention"
    );
    println!(
        "  after drop_chunks(100): raw rows 0 but bucket 0 still {{count {}, sum {}}}",
        kept.count, kept.sum
    );
}

fn main() {
    let mut rng = Rng(0x7157_E5A6_0862);
    test_gapfill(&mut rng);
    demo_pruning_and_compression(&mut rng);
    demo_cagg();

    let mut t = Totals::default();
    for seed in 0..2000 {
        run_workload(seed, &mut t);
    }
    println!("\n2000 random hypertable workloads vs brute-force oracle: all equal");
    println!(
        "  ops {} (late inserts {}, rejected inserts {}, deletes {}, refreshes {}, compress runs {}, chunks dropped {})",
        t.ops,
        t.late_inserts,
        t.inserts_rejected,
        t.deletes,
        t.refreshes,
        t.compress_runs,
        t.chunks_dropped
    );
    println!(
        "  cagg: fresh checks {}, real-time checks {}, stale buckets masked {}, invalidations {}, re-materialised ranges {}",
        t.fresh_checks,
        t.realtime_checks,
        t.stale_buckets_masked,
        t.invalidations,
        t.rematerialised
    );
    println!("  decompress-on-delete events {}", t.decompress_on_delete);
    println!(
        "  pruning over all scans: chunks scanned {} of {} visited-or-skipped ({:.1}%), segments skipped {} of {}",
        t.scan.chunks_scanned,
        t.scan.chunks_total,
        100.0 * t.scan.chunks_scanned as f64 / t.scan.chunks_total.max(1) as f64,
        t.scan.segments_skipped,
        t.scan.segments_seen
    );
    assert!(
        t.scan.chunks_scanned < t.scan.chunks_total,
        "time predicates must exclude chunks"
    );
    assert!(
        t.stale_buckets_masked > 0
            && t.invalidations > 1000
            && t.decompress_on_delete > 0
            && t.chunks_dropped > 0
    );
    println!("\nall hypertables_continuous_aggregates checks passed");
}
