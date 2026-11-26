//! RECIPE 863 (database): time-series joins and alignment: ASOF/window/interval joins, resampling and gap filling, ordering and duplicate policies, DST-safe integer time math.
//! LEVEL: intermediate
//! Use in: joining trades to the latest quote, sensor streams to reference data, aligning irregular series to a regular grid, daily rollups in a local time zone.   Run: cargo run --release --example asof_joins_gap_filling
//! COMMON USES: ASOF JOIN in kdb+/q (`aj`), DuckDB, ClickHouse, QuestDB and Snowflake; pandas `merge_asof` and `resample().ffill()`; TimescaleDB
//! `time_bucket_gapfill` + `locf`/`interpolate`; Flink/Beam interval joins; finance (trade/quote alignment), IoT (sensor fusion), observability (aligning metrics).
//! Course: CMU 15-445/721 (join algorithms: sort-merge, index/binary-search joins); kdb+ `aj` documentation for the trade/quote use case.
#![allow(dead_code)]
//!
//! WHAT: ordinary equi-joins need equal keys; time series almost never have equal timestamps. The ASOF join matches each LEFT row to the right row
//! "as of" its time. Everything hinges on precise semantics for ties, tolerance and direction, so this file pins them and tests each against a
//! brute-force O(n*m) oracle.
//! SEMANTICS (per join key `by`): BACKWARD = latest right row with ts <= t (ts < t when `allow_exact` is off); FORWARD = earliest with ts >= t;
//! NEAREST = smaller |dt|, ties go BACKWARD. TOLERANCE bounds |dt|; a match farther away is "no match" (None), not the next best. Several right rows
//! with the SAME timestamp: `DupPolicy::First`/`DupPolicy::Last` picks the earliest/latest in input order. `skip_nan` treats right rows whose value is NaN as absent
//! (so the join returns the last VALID observation).
//! HOW: right rows are grouped per key into runs of equal timestamps. MERGE join: per-key pointers only move forward while the (sorted) left advances,
//! O(n+m) overall. BINARY-SEARCH join: `partition_point` per left row, O(n log m), and the left side need not be sorted. Both feed the same `choose`.
//! WINDOW JOIN: all right rows within [t-a, t+b] (sliding pointers). INTERVAL JOIN: pairs of half-open intervals that overlap, by a sweep over interval starts
//! with an active set per side. RESAMPLE: bucket mean (NaN ignored) then a fill rule: null, zero, forward-fill buckets, last raw observation (an asof join of the
//! grid onto the data), or linear interpolation between in-window neighbours.
//! ORDER POLICIES: `Reject` out-of-order input or stable-`Sort` it; duplicates (same ts and key) are kept, first/last wins, averaged or an error.
//! TIME ZONES: a calendar-free integer implementation of civil dates (days <-> y/m/d), a US-style DST rule, and local-to-UTC conversion that reports GAPS
//! (nonexistent local times) and FOLDS (ambiguous ones). Local days are 23/24/25 hours long; bucketing by `ts - ts % 86400` is wrong for local days.
//!
//! Read: pandas `merge_asof` and ClickHouse/DuckDB ASOF JOIN docs (semantics of direction, tolerance, allow_exact_matches; cited by title)
//! Read: Howard Hinnant, "chrono-Compatible Low-Level Date Algorithms"  https://howardhinnant.github.io/date_algorithms.html
//! Read: TimescaleDB docs, time_bucket_gapfill, locf, interpolate  https://docs.timescale.com/use-timescale/latest/hyperfunctions/
//!
//! TRY NEXT:
//!   - [S] Flip the NEAREST tie-break in `choose` to prefer FORWARD. Predict which of the three random tests fail (merge, bsearch, oracle?) and why only the
//!     oracle should need a one-line change as well.
//!   - [S] Make tolerance exclusive (`dt < tol`). Which entries of the "match rate vs tolerance" table move, and does the monotonicity assert still hold?
//!   - [M] Make `DupPolicy::Mean` meaningful in the asof join (`choose` currently treats it like `Last`): return the average of the equal-timestamp run, which means
//!     returning a value instead of a row index. Extend `asof_oracle`; what does the mean imply for `skip_nan`?
//!   - [M] Add `local_day_buckets(ts_list)` using `local_day_start_utc` and show that hourly data on the fall-back day lands 25 samples in one bucket; compare with UTC bucketing.
//!   - [A] PROJECT: use `asof_bsearch` to join the downsampled output of `tsm_time_series_engine` (series A) to another series B by tolerance, then run the anomaly
//!     detectors of `timeseries_sketches_downsampling_anomaly` on the ratio A/B.

use std::collections::{BTreeMap, HashMap};

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

#[derive(Clone, Copy, Debug)]
struct Ev {
    ts: i64,
    key: u32,
    val: f64,
}
impl PartialEq for Ev {
    fn eq(&self, o: &Ev) -> bool {
        (self.ts, self.key, self.val.to_bits()) == (o.ts, o.key, o.val.to_bits())
    }
}

// ---------------------------------------------------------------- ordering and duplicate policies
#[derive(Clone, Copy, Debug, PartialEq)]
enum OrderPolicy {
    Reject,
    Sort,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum DupPolicy {
    Keep,
    First,
    Last,
    Mean,
    Error,
}
#[derive(Debug, PartialEq)]
enum NormErr {
    OutOfOrder { index: usize },
    Duplicate { ts: i64, key: u32 },
}
/// Returns rows sorted by ts (stable) with the duplicate policy applied to equal (ts, key) pairs.
fn normalize(rows: &[Ev], order: OrderPolicy, dup: DupPolicy) -> Result<Vec<Ev>, NormErr> {
    let mut v = rows.to_vec();
    match order {
        OrderPolicy::Reject => {
            if let Some(i) = (1..v.len()).find(|&i| v[i].ts < v[i - 1].ts) {
                return Err(NormErr::OutOfOrder { index: i });
            }
        }
        OrderPolicy::Sort => v.sort_by_key(|e| e.ts), // stable: equal timestamps keep arrival order
    }
    if dup == DupPolicy::Keep {
        return Ok(v);
    }
    let mut out: Vec<Ev> = Vec::with_capacity(v.len());
    let mut i = 0;
    while i < v.len() {
        let j = v[i..]
            .iter()
            .position(|e| e.ts != v[i].ts)
            .map_or(v.len(), |p| i + p);
        let mut slot: HashMap<u32, Vec<f64>> = HashMap::new();
        let mut order_of_keys = vec![];
        for e in &v[i..j] {
            let list = slot.entry(e.key).or_default();
            if list.is_empty() {
                order_of_keys.push(e.key);
            }
            list.push(e.val);
        }
        for k in order_of_keys {
            let vals = &slot[&k];
            let val = match dup {
                DupPolicy::Error if vals.len() > 1 => {
                    return Err(NormErr::Duplicate {
                        ts: v[i].ts,
                        key: k,
                    });
                }
                DupPolicy::First | DupPolicy::Error => vals[0],
                DupPolicy::Last => *vals.last().unwrap(),
                DupPolicy::Mean => {
                    let good: Vec<f64> = vals.iter().copied().filter(|x| !x.is_nan()).collect();
                    if good.is_empty() {
                        f64::NAN
                    } else {
                        good.iter().sum::<f64>() / good.len() as f64
                    }
                }
                DupPolicy::Keep => unreachable!(),
            };
            out.push(Ev {
                ts: v[i].ts,
                key: k,
                val,
            });
        }
        i = j;
    }
    Ok(out)
}

// ---------------------------------------------------------------- ASOF join
#[derive(Clone, Copy, Debug, PartialEq)]
enum Dir {
    Backward,
    Forward,
    Nearest,
}
#[derive(Clone, Copy, Debug)]
struct AsofCfg {
    dir: Dir,
    tolerance: Option<i64>,
    allow_exact: bool,
    dup: DupPolicy, // only First/Last are meaningful: which row of an equal-timestamp run to return
    skip_nan: bool,
}
/// Rows of one key with equal timestamps collapse to a run remembering the first and last row index.
#[derive(Clone, Copy, Debug)]
struct Run {
    ts: i64,
    first: usize,
    last: usize,
}
fn build_runs(right: &[Ev], skip_nan: bool) -> HashMap<u32, Vec<Run>> {
    let mut m: HashMap<u32, Vec<Run>> = HashMap::new();
    for (i, e) in right.iter().enumerate() {
        if skip_nan && e.val.is_nan() {
            continue;
        }
        let runs = m.entry(e.key).or_default();
        match runs.last_mut() {
            Some(r) if r.ts == e.ts => r.last = i,
            _ => runs.push(Run {
                ts: e.ts,
                first: i,
                last: i,
            }),
        }
    }
    m
}
/// Shared decision once the position is known: `lo` = runs with ts < t, `hi` = runs with ts <= t.
fn choose(runs: &[Run], lo: usize, hi: usize, t: i64, c: &AsofCfg) -> Option<usize> {
    let within = |d: i64| c.tolerance.is_none_or(|tol| d <= tol);
    let back = if c.allow_exact { hi } else { lo }
        .checked_sub(1)
        .map(|i| runs[i])
        .filter(|r| within(t - r.ts));
    let fwd = runs
        .get(if c.allow_exact { lo } else { hi })
        .copied()
        .filter(|r| within(r.ts - t));
    let run = match c.dir {
        Dir::Backward => back,
        Dir::Forward => fwd,
        Dir::Nearest => match (back, fwd) {
            (Some(b), Some(f)) => Some(if t - b.ts <= f.ts - t { b } else { f }),
            (b, f) => b.or(f),
        },
    }?;
    Some(if c.dup == DupPolicy::First {
        run.first
    } else {
        run.last
    })
}
/// O(n + m): `left` must be sorted by ts; per-key pointers only advance.
fn asof_merge(left: &[Ev], right: &[Ev], c: &AsofCfg) -> Vec<Option<usize>> {
    assert!(
        left.windows(2).all(|w| w[0].ts <= w[1].ts),
        "merge join needs a sorted left side"
    );
    let runs = build_runs(right, c.skip_nan);
    let mut ptr: HashMap<u32, (usize, usize)> = HashMap::new();
    left.iter()
        .map(|l| {
            let rs = runs.get(&l.key)?;
            let (lo, hi) = ptr.entry(l.key).or_insert((0, 0));
            while *lo < rs.len() && rs[*lo].ts < l.ts {
                *lo += 1;
            }
            *hi = (*hi).max(*lo);
            while *hi < rs.len() && rs[*hi].ts <= l.ts {
                *hi += 1;
            }
            choose(rs, *lo, *hi, l.ts, c)
        })
        .collect()
}
/// O(n log m): any left order.
fn asof_bsearch(left: &[Ev], right: &[Ev], c: &AsofCfg) -> Vec<Option<usize>> {
    let runs = build_runs(right, c.skip_nan);
    left.iter()
        .map(|l| {
            let rs = runs.get(&l.key)?;
            let lo = rs.partition_point(|r| r.ts < l.ts);
            let hi = rs.partition_point(|r| r.ts <= l.ts);
            choose(rs, lo, hi, l.ts, c)
        })
        .collect()
}
/// Independent brute force straight from the definitions.
fn asof_oracle(left: &[Ev], right: &[Ev], c: &AsofCfg) -> Vec<Option<usize>> {
    left.iter()
        .map(|l| {
            let cands: Vec<usize> = (0..right.len())
                .filter(|&j| {
                    let r = &right[j];
                    let d = r.ts - l.ts; // negative = right is earlier
                    r.key == l.key
                        && !(c.skip_nan && r.val.is_nan())
                        && c.tolerance.is_none_or(|t| d.abs() <= t)
                        && (c.allow_exact || d != 0)
                        && match c.dir {
                            Dir::Backward => d <= 0,
                            Dir::Forward => d >= 0,
                            Dir::Nearest => true,
                        }
                })
                .collect();
            // rank: smallest distance, then earlier side (BACKWARD wins ties), then First/Last by index
            let best_ts = cands
                .iter()
                .map(|&j| ((right[j].ts - l.ts).abs(), right[j].ts > l.ts))
                .min()?;
            let same: Vec<usize> = cands
                .into_iter()
                .filter(|&j| ((right[j].ts - l.ts).abs(), right[j].ts > l.ts) == best_ts)
                .collect();
            Some(if c.dup == DupPolicy::First {
                *same.first().unwrap()
            } else {
                *same.last().unwrap()
            })
        })
        .collect()
}

// ---------------------------------------------------------------- window and interval joins
/// For each left row, indices of right rows (same key) with t-a <= ts <= t+b, in right order. Left sorted.
fn window_join(left: &[Ev], right: &[Ev], a: i64, b: i64) -> Vec<Vec<usize>> {
    let mut by_key: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, r) in right.iter().enumerate() {
        by_key.entry(r.key).or_default().push(i);
    }
    let mut ptr: HashMap<u32, (usize, usize)> = HashMap::new();
    left.iter()
        .map(|l| {
            let Some(idx) = by_key.get(&l.key) else {
                return vec![];
            };
            let (lo, hi) = ptr.entry(l.key).or_insert((0, 0));
            while *lo < idx.len() && right[idx[*lo]].ts < l.ts - a {
                *lo += 1;
            }
            *hi = (*hi).max(*lo);
            while *hi < idx.len() && right[idx[*hi]].ts <= l.ts + b {
                *hi += 1;
            }
            idx[*lo..*hi].to_vec()
        })
        .collect()
}
fn window_oracle(left: &[Ev], right: &[Ev], a: i64, b: i64) -> Vec<Vec<usize>> {
    left.iter()
        .map(|l| {
            (0..right.len())
                .filter(|&j| {
                    right[j].key == l.key && right[j].ts >= l.ts - a && right[j].ts <= l.ts + b
                })
                .collect()
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
struct Iv {
    key: u32,
    start: i64,
    end: i64, // half-open [start, end)
}
/// All (left, right) index pairs with the same key whose half-open intervals overlap. Sweep by start; active sets drop expired intervals.
fn interval_join(left: &[Iv], right: &[Iv]) -> Vec<(usize, usize)> {
    #[derive(Clone, Copy)]
    struct Item {
        side: u8,
        idx: usize,
        iv: Iv,
    }
    let mut items: Vec<Item> = left
        .iter()
        .enumerate()
        .map(|(i, &iv)| Item {
            side: 0,
            idx: i,
            iv,
        })
        .collect();
    items.extend(right.iter().enumerate().map(|(i, &iv)| Item {
        side: 1,
        idx: i,
        iv,
    }));
    items.retain(|it| it.iv.start < it.iv.end); // empty intervals overlap nothing
    items.sort_by_key(|it| (it.iv.start, it.side, it.idx));
    let mut active: [HashMap<u32, Vec<Item>>; 2] = [HashMap::new(), HashMap::new()];
    let mut out = vec![];
    for it in items {
        let other = &mut active[1 - it.side as usize];
        if let Some(list) = other.get_mut(&it.iv.key) {
            list.retain(|o| o.iv.end > it.iv.start);
            for o in list.iter() {
                out.push(if it.side == 0 {
                    (it.idx, o.idx)
                } else {
                    (o.idx, it.idx)
                });
            }
        }
        active[it.side as usize]
            .entry(it.iv.key)
            .or_default()
            .push(it);
    }
    out.sort();
    out
}
fn interval_oracle(left: &[Iv], right: &[Iv]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    for (i, l) in left.iter().enumerate() {
        for (j, r) in right.iter().enumerate() {
            if l.key == r.key
                && l.start < r.end
                && r.start < l.end
                && l.start < l.end
                && r.start < r.end
            {
                out.push((i, j));
            }
        }
    }
    out
}

// ---------------------------------------------------------------- resampling
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fill {
    Null,
    Zero,
    Ffill,   // carry the last non-empty BUCKET value forward
    LastObs, // last raw observation before the bucket (asof backward)
    Linear,  // interpolate between non-empty buckets inside the window
}
/// `series` is (ts, value) sorted by ts. Buckets [start + k*step, start + (k+1)*step) for bucket start < end.
fn resample(
    series: &[(i64, f64)],
    start: i64,
    end: i64,
    step: i64,
    fill: Fill,
) -> Vec<(i64, Option<f64>)> {
    let mut buckets: Vec<(i64, Option<f64>)> = vec![];
    let mut b = start;
    while b < end {
        let lo = series.partition_point(|p| p.0 < b);
        let hi = series.partition_point(|p| p.0 < b + step);
        let good: Vec<f64> = series[lo..hi]
            .iter()
            .map(|p| p.1)
            .filter(|v| !v.is_nan())
            .collect();
        buckets.push((
            b,
            (!good.is_empty()).then(|| good.iter().sum::<f64>() / good.len() as f64),
        ));
        b += step;
    }
    let snapshot = buckets.clone();
    let mut last: Option<f64> = None;
    for (i, (b, v)) in buckets.iter_mut().enumerate() {
        if v.is_some() {
            last = *v;
            continue;
        }
        *v = match fill {
            Fill::Null => None,
            Fill::Zero => Some(0.0),
            Fill::Ffill => last,
            Fill::LastObs => {
                let lo = series.partition_point(|p| p.0 < *b);
                series[..lo].iter().rev().map(|p| p.1).find(|x| !x.is_nan())
            }
            Fill::Linear => {
                let prev = snapshot[..i]
                    .iter()
                    .rev()
                    .find_map(|&(t, x)| x.map(|x| (t, x)));
                let next = snapshot[i + 1..]
                    .iter()
                    .find_map(|&(t, x)| x.map(|x| (t, x)));
                prev.zip(next).map(|((bp, vp), (bn, vn))| {
                    vp + (vn - vp) * ((*b - bp) as f64 / (bn - bp) as f64)
                })
            }
        };
    }
    buckets
}
fn resample_oracle(
    series: &[(i64, f64)],
    start: i64,
    end: i64,
    step: i64,
    fill: Fill,
) -> Vec<(i64, Option<f64>)> {
    let n = (0..).take_while(|k| start + k * step < end).count() as i64;
    let mean_of = |k: i64| {
        let vals: Vec<f64> = series
            .iter()
            .filter(|p| p.0 >= start + k * step && p.0 < start + (k + 1) * step && !p.1.is_nan())
            .map(|p| p.1)
            .collect();
        (!vals.is_empty()).then(|| vals.iter().sum::<f64>() / vals.len() as f64)
    };
    (0..n)
        .map(|k| {
            let t = start + k * step;
            let v = mean_of(k).or_else(|| match fill {
                Fill::Null => None,
                Fill::Zero => Some(0.0),
                Fill::Ffill => (0..k).rev().find_map(mean_of),
                Fill::LastObs => series
                    .iter()
                    .filter(|p| p.0 < t && !p.1.is_nan())
                    .last()
                    .map(|p| p.1),
                Fill::Linear => {
                    let p = (0..k)
                        .rev()
                        .find_map(|j| mean_of(j).map(|v| (start + j * step, v)));
                    let q = (k + 1..n).find_map(|j| mean_of(j).map(|v| (start + j * step, v)));
                    p.zip(q).map(|((bp, vp), (bn, vn))| {
                        vp + (vn - vp) * ((t - bp) as f64 / (bn - bp) as f64)
                    })
                }
            });
            (t, v)
        })
        .collect()
}

// ---------------------------------------------------------------- civil time with integers
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}
fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7) // 0 = Sunday (1970-01-01 was a Thursday)
}
fn nth_sunday(y: i64, m: i64, n: i64) -> i64 {
    let first = days_from_civil(y, m, 1);
    first + (7 - weekday(first)) % 7 + 7 * (n - 1)
}
const STD: i64 = -5 * 3600;
const DST: i64 = -4 * 3600;
/// US-style rule: DST from the 2nd Sunday of March 02:00 standard (= 07:00 UTC) to the 1st Sunday of November 02:00 daylight (= 06:00 UTC).
fn dst_bounds_utc(y: i64) -> (i64, i64) {
    (
        nth_sunday(y, 3, 2) * 86400 + 7 * 3600,
        nth_sunday(y, 11, 1) * 86400 + 6 * 3600,
    )
}
fn offset_at(utc: i64) -> i64 {
    let (y, _, _) = civil_from_days(utc.div_euclid(86400));
    let (s, e) = dst_bounds_utc(y);
    if utc >= s && utc < e { DST } else { STD }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Resolve {
    Earliest,
    Latest,
}
#[derive(Debug, PartialEq)]
enum LocalError {
    Gap,
}
/// UTC instants whose local wall-clock reading is `local` (0 = gap, 1 = normal, 2 = fold), ascending.
fn local_candidates(local: i64) -> Vec<i64> {
    let mut v: Vec<i64> = [STD, DST]
        .into_iter()
        .map(|off| local - off)
        .filter(|&u| offset_at(u) == local - u)
        .collect();
    v.sort();
    v
}
fn local_to_utc(local: i64, r: Resolve) -> Result<i64, LocalError> {
    let c = local_candidates(local);
    match (c.first(), c.last(), r) {
        (None, ..) => Err(LocalError::Gap),
        (Some(&a), _, Resolve::Earliest) => Ok(a),
        (_, Some(&b), Resolve::Latest) => Ok(b),
        _ => unreachable!(),
    }
}
fn local_day_start_utc(day: i64) -> i64 {
    local_to_utc(day * 86400, Resolve::Earliest).expect("midnight exists in this zone")
}

// ---------------------------------------------------------------- random generators
fn rand_events(rng: &mut Rng, max_n: u64, span: i64, keys: u64) -> Vec<Ev> {
    (0..rng.below(max_n))
        .map(|_| Ev {
            ts: rng.range(0, span),
            key: rng.below(keys) as u32,
            val: if rng.chance(15) {
                f64::NAN
            } else {
                rng.range(-9, 9) as f64
            },
        })
        .collect()
}

// ---------------------------------------------------------------- tests
fn test_asof(rng: &mut Rng) {
    let (mut total, mut matched) = (0u64, 0u64);
    for case in 0..3000 {
        let left = normalize(
            &rand_events(rng, 25, 40, 3),
            OrderPolicy::Sort,
            DupPolicy::Keep,
        )
        .unwrap();
        let right = normalize(
            &rand_events(rng, 25, 40, 3),
            OrderPolicy::Sort,
            DupPolicy::Keep,
        )
        .unwrap();
        let cfg = AsofCfg {
            dir: [Dir::Backward, Dir::Forward, Dir::Nearest][rng.below(3) as usize],
            tolerance: [None, Some(0), Some(1), Some(3), Some(10)][rng.below(5) as usize],
            allow_exact: rng.chance(70),
            dup: [DupPolicy::First, DupPolicy::Last][rng.below(2) as usize],
            skip_nan: rng.chance(50),
        };
        let want = asof_oracle(&left, &right, &cfg);
        assert_eq!(
            asof_merge(&left, &right, &cfg),
            want,
            "case {case} merge {cfg:?}\nL {left:?}\nR {right:?}"
        );
        assert_eq!(
            asof_bsearch(&left, &right, &cfg),
            want,
            "case {case} bsearch {cfg:?}"
        );
        // the binary-search join does not care about left order
        let mut shuffled = left.clone();
        for i in (1..shuffled.len()).rev() {
            shuffled.swap(i, rng.below(i as u64 + 1) as usize);
        }
        assert_eq!(
            asof_bsearch(&shuffled, &right, &cfg),
            asof_oracle(&shuffled, &right, &cfg),
            "case {case} unsorted left"
        );
        total += want.len() as u64;
        matched += want.iter().flatten().count() as u64;
    }
    // hand-written example: trades joined to the latest quote of the same symbol
    let quotes = [
        Ev {
            ts: 100,
            key: 1,
            val: 10.0,
        },
        Ev {
            ts: 200,
            key: 1,
            val: 11.0,
        },
        Ev {
            ts: 150,
            key: 2,
            val: 50.0,
        },
    ];
    let quotes = normalize(&quotes, OrderPolicy::Sort, DupPolicy::Keep).unwrap();
    let trades = [
        Ev {
            ts: 99,
            key: 1,
            val: 0.0,
        },
        Ev {
            ts: 100,
            key: 1,
            val: 0.0,
        },
        Ev {
            ts: 199,
            key: 1,
            val: 0.0,
        },
        Ev {
            ts: 205,
            key: 2,
            val: 0.0,
        },
        Ev {
            ts: 260,
            key: 1,
            val: 0.0,
        },
    ];
    let base = AsofCfg {
        dir: Dir::Backward,
        tolerance: None,
        allow_exact: true,
        dup: DupPolicy::Last,
        skip_nan: false,
    };
    let show = |cfg: &AsofCfg| {
        asof_merge(&trades, &quotes, cfg)
            .iter()
            .map(|m| m.map(|i| quotes[i].val))
            .collect::<Vec<_>>()
    };
    println!(
        "trades at [99,100,199 (sym1), 205 (sym2), 260 (sym1)] joined to quotes sym1@100=10 sym1@200=11 sym2@150=50:"
    );
    println!("  backward                : {:?}", show(&base));
    println!(
        "  backward, exact excluded: {:?}",
        show(&AsofCfg {
            allow_exact: false,
            ..base
        })
    );
    println!(
        "  backward, tolerance 60  : {:?}",
        show(&AsofCfg {
            tolerance: Some(60),
            ..base
        })
    );
    println!(
        "  forward                 : {:?}",
        show(&AsofCfg {
            dir: Dir::Forward,
            ..base
        })
    );
    println!(
        "  nearest                 : {:?}",
        show(&AsofCfg {
            dir: Dir::Nearest,
            ..base
        })
    );
    assert_eq!(
        show(&base),
        vec![None, Some(10.0), Some(10.0), Some(50.0), Some(11.0)]
    );
    assert_eq!(
        show(&AsofCfg {
            tolerance: Some(60),
            ..base
        }),
        vec![None, Some(10.0), None, Some(50.0), Some(11.0)]
    );
    assert_eq!(
        show(&AsofCfg {
            dir: Dir::Nearest,
            ..base
        })[2],
        Some(11.0),
        "199 is closer to 200 than to 100"
    );
    println!(
        "asof merge == bsearch == oracle on 3000 random cases ({matched}/{total} left rows matched)"
    );
}

fn test_window_and_interval(rng: &mut Rng) {
    for case in 0..3000 {
        let left = normalize(
            &rand_events(rng, 20, 40, 3),
            OrderPolicy::Sort,
            DupPolicy::Keep,
        )
        .unwrap();
        let right = normalize(
            &rand_events(rng, 25, 40, 3),
            OrderPolicy::Sort,
            DupPolicy::Keep,
        )
        .unwrap();
        let (a, b) = (rng.range(0, 6), rng.range(0, 6));
        assert_eq!(
            window_join(&left, &right, a, b),
            window_oracle(&left, &right, a, b),
            "window case {case} [-{a},+{b}]"
        );
        let mk = |rng: &mut Rng| -> Vec<Iv> {
            (0..rng.below(14))
                .map(|_| {
                    let s = rng.range(0, 30);
                    Iv {
                        key: rng.below(3) as u32,
                        start: s,
                        end: s + rng.range(0, 8),
                    } // length 0 allowed: overlaps nothing
                })
                .collect()
        };
        let (l, r) = (mk(rng), mk(rng));
        assert_eq!(
            interval_join(&l, &r),
            interval_oracle(&l, &r),
            "interval case {case}"
        );
    }
    println!(
        "window join and interval join (incl. empty and equal-start intervals) vs O(n*m) oracle on 3000 cases: ok"
    );
}

fn test_resample(rng: &mut Rng) {
    for case in 0..3000 {
        let mut series: Vec<(i64, f64)> = (0..rng.below(20))
            .map(|_| {
                (
                    rng.range(-5, 55),
                    if rng.chance(20) {
                        f64::NAN
                    } else {
                        rng.range(-8, 8) as f64
                    },
                )
            })
            .collect();
        series.sort_by_key(|p| p.0);
        let step = rng.range(1, 9);
        let start = rng.range(-3, 10);
        let end = start + rng.range(0, 60);
        for fill in [
            Fill::Null,
            Fill::Zero,
            Fill::Ffill,
            Fill::LastObs,
            Fill::Linear,
        ] {
            let (got, want) = (
                resample(&series, start, end, step, fill),
                resample_oracle(&series, start, end, step, fill),
            );
            let bits = |v: &[(i64, Option<f64>)]| {
                v.iter()
                    .map(|&(t, x)| (t, x.map(f64::to_bits)))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                bits(&got),
                bits(&want),
                "resample case {case} {fill:?} step {step} [{start},{end})"
            );
        }
    }
    let s = [(1, 10.0), (2, 20.0), (14, 40.0), (15, f64::NAN)];
    let show = |f| {
        resample(&s, 0, 20, 5, f)
            .iter()
            .map(|p| p.1)
            .collect::<Vec<_>>()
    };
    println!("resample [(1,10),(2,20),(14,40),(15,NaN)] into 4 buckets of 5:");
    for f in [
        Fill::Null,
        Fill::Zero,
        Fill::Ffill,
        Fill::LastObs,
        Fill::Linear,
    ] {
        println!("  {f:<8?} {:?}", show(f));
    }
    assert_eq!(show(Fill::Null), vec![Some(15.0), None, Some(40.0), None]);
    assert_eq!(
        show(Fill::Ffill),
        vec![Some(15.0), Some(15.0), Some(40.0), Some(40.0)]
    );
    assert_eq!(
        show(Fill::LastObs),
        vec![Some(15.0), Some(20.0), Some(40.0), Some(40.0)],
        "last RAW observation (20), not the bucket mean (15)"
    );
    assert_eq!(
        show(Fill::Linear),
        vec![Some(15.0), Some(27.5), Some(40.0), None]
    );
    println!("resample (5 fill rules, NaN ignored) vs oracle on 3000 cases: ok");
}

fn test_policies(rng: &mut Rng) {
    let rows = vec![
        Ev {
            ts: 5,
            key: 1,
            val: 1.0,
        },
        Ev {
            ts: 3,
            key: 1,
            val: 2.0,
        },
        Ev {
            ts: 5,
            key: 1,
            val: 3.0,
        },
        Ev {
            ts: 5,
            key: 2,
            val: 4.0,
        },
    ];
    assert_eq!(
        normalize(&rows, OrderPolicy::Reject, DupPolicy::Keep),
        Err(NormErr::OutOfOrder { index: 1 })
    );
    assert_eq!(
        normalize(&rows, OrderPolicy::Sort, DupPolicy::Error),
        Err(NormErr::Duplicate { ts: 5, key: 1 })
    );
    let last = normalize(&rows, OrderPolicy::Sort, DupPolicy::Last).unwrap();
    assert_eq!(
        last,
        vec![
            Ev {
                ts: 3,
                key: 1,
                val: 2.0
            },
            Ev {
                ts: 5,
                key: 1,
                val: 3.0
            },
            Ev {
                ts: 5,
                key: 2,
                val: 4.0
            }
        ]
    );
    let mean = normalize(&rows, OrderPolicy::Sort, DupPolicy::Mean).unwrap();
    assert_eq!(
        mean[1],
        Ev {
            ts: 5,
            key: 1,
            val: 2.0
        }
    );
    for _ in 0..2000 {
        let rows = rand_events(rng, 30, 10, 3);
        let kept = normalize(&rows, OrderPolicy::Sort, DupPolicy::Keep).unwrap();
        assert_eq!(kept.len(), rows.len());
        assert!(kept.windows(2).all(|w| w[0].ts <= w[1].ts));
        // stability: for equal ts the original relative order is preserved
        for ts in 0..=10 {
            let orig: Vec<Ev> = rows.iter().copied().filter(|e| e.ts == ts).collect();
            let now: Vec<Ev> = kept.iter().copied().filter(|e| e.ts == ts).collect();
            assert_eq!(orig, now);
        }
        for d in [DupPolicy::First, DupPolicy::Last, DupPolicy::Mean] {
            let n = normalize(&rows, OrderPolicy::Sort, d).unwrap();
            let mut seen = std::collections::HashSet::new();
            assert!(
                n.iter().all(|e| seen.insert((e.ts, e.key))),
                "{d:?}: duplicates remain"
            );
            let distinct: std::collections::HashSet<_> =
                rows.iter().map(|e| (e.ts, e.key)).collect();
            assert_eq!(n.len(), distinct.len());
        }
        let naive_last: BTreeMap<(i64, u32), u64> = kept
            .iter()
            .map(|e| ((e.ts, e.key), e.val.to_bits()))
            .collect();
        let got: BTreeMap<(i64, u32), u64> = normalize(&rows, OrderPolicy::Sort, DupPolicy::Last)
            .unwrap()
            .iter()
            .map(|e| ((e.ts, e.key), e.val.to_bits()))
            .collect();
        assert_eq!(got, naive_last);
    }
    println!("order/duplicate policies (reject, stable sort, first/last/mean/error): ok");
}

fn test_tolerance_table(rng: &mut Rng) {
    // irregular sensor (jittered 1s sampling with dropouts) aligned to a regular 1s grid by a backward asof with tolerance
    let sensor: Vec<Ev> = {
        let mut t = 0;
        (0..2000)
            .map(|_| {
                t += if rng.chance(8) { rng.range(2, 30) } else { 1 } * 1000 + rng.range(-120, 120);
                Ev {
                    ts: t,
                    key: 0,
                    val: 1.0,
                }
            })
            .collect()
    };
    let grid: Vec<Ev> = (1..=(sensor.last().unwrap().ts / 1000))
        .map(|s| Ev {
            ts: s * 1000,
            key: 0,
            val: 0.0,
        })
        .collect();
    println!("\nirregular sensor -> regular 1 s grid, backward asof: match rate vs tolerance");
    println!("  {:>10} {:>10}", "tolerance", "matched");
    let mut prev = 0;
    for tol in [
        Some(0),
        Some(100),
        Some(500),
        Some(1000),
        Some(2000),
        Some(10_000),
        None,
    ] {
        let cfg = AsofCfg {
            dir: Dir::Backward,
            tolerance: tol,
            allow_exact: true,
            dup: DupPolicy::Last,
            skip_nan: false,
        };
        let m = asof_merge(&grid, &sensor, &cfg).iter().flatten().count();
        println!(
            "  {:>10} {:>9.1}%",
            tol.map_or("inf".into(), |t| format!("{t} ms")),
            100.0 * m as f64 / grid.len() as f64
        );
        assert!(m >= prev, "a larger tolerance can only add matches");
        prev = m;
    }
    assert_eq!(
        prev,
        grid.len()
            - asof_merge(
                &grid,
                &sensor,
                &AsofCfg {
                    dir: Dir::Backward,
                    tolerance: None,
                    allow_exact: true,
                    dup: DupPolicy::Last,
                    skip_nan: false
                }
            )
            .iter()
            .filter(|m| m.is_none())
            .count()
    );
}

fn test_time_zones() {
    assert_eq!(days_from_civil(1970, 1, 1), 0);
    assert_eq!(days_from_civil(2000, 3, 1), 11017);
    assert_eq!(civil_from_days(-1), (1969, 12, 31));
    for z in (-800_000..800_000).step_by(37) {
        let (y, m, d) = civil_from_days(z);
        assert_eq!(days_from_civil(y, m, d), z);
    }
    assert_eq!(
        weekday(days_from_civil(2024, 3, 10)),
        0,
        "2024-03-10 is a Sunday"
    );
    let (s, e) = dst_bounds_utc(2024);
    assert_eq!(
        (s, e),
        (
            days_from_civil(2024, 3, 10) * 86400 + 7 * 3600,
            days_from_civil(2024, 11, 3) * 86400 + 6 * 3600
        )
    );
    // exhaustive over two years of hours/half-hours: local round trip is always among the candidates
    let (mut gaps, mut folds) = (0, 0);
    let (from, to) = (
        days_from_civil(2023, 1, 1) * 86400,
        days_from_civil(2025, 1, 1) * 86400,
    );
    let mut u = from;
    while u < to {
        let local = u + offset_at(u);
        let c = local_candidates(local);
        assert!(
            c.contains(&u),
            "{u} not recoverable from its own local time"
        );
        folds += (c.len() == 2) as u32 * (u % 3600 == 0) as u32;
        u += 1800;
    }
    // gap: local 02:30 on the spring-forward day does not exist; fold: 01:30 on the fall-back day happens twice
    let spring_gap = days_from_civil(2024, 3, 10) * 86400 + 2 * 3600 + 1800;
    let fall_fold = days_from_civil(2024, 11, 3) * 86400 + 3600 + 1800;
    assert_eq!(
        local_to_utc(spring_gap, Resolve::Earliest),
        Err(LocalError::Gap)
    );
    let (a, b) = (
        local_to_utc(fall_fold, Resolve::Earliest).unwrap(),
        local_to_utc(fall_fold, Resolve::Latest).unwrap(),
    );
    assert_eq!(b - a, 3600, "the two readings of 01:30 are one hour apart");
    gaps += (0..86400)
        .step_by(600)
        .filter(|s| local_candidates(days_from_civil(2024, 3, 10) * 86400 + s).is_empty())
        .count();
    assert_eq!(gaps, 6, "one hour of gap, sampled every 10 minutes");
    // local-day lengths over 2024
    let d0 = days_from_civil(2024, 1, 1);
    let lens: Vec<i64> = (d0..d0 + 366)
        .map(|d| local_day_start_utc(d + 1) - local_day_start_utc(d))
        .collect();
    let (short, long) = (
        lens.iter().filter(|&&l| l == 23 * 3600).count(),
        lens.iter().filter(|&&l| l == 25 * 3600).count(),
    );
    assert_eq!((short, long), (1, 1));
    assert_eq!(
        lens.iter().sum::<i64>(),
        366 * 86400,
        "23h + 25h cancel out over a year"
    );
    assert_eq!(
        lens[(days_from_civil(2024, 3, 10) - d0) as usize],
        23 * 3600
    );
    assert_eq!(
        lens[(days_from_civil(2024, 11, 3) - d0) as usize],
        25 * 3600
    );
    // pitfall 1: bucketing hourly data by UTC day vs by local day on the fall-back date
    let day = days_from_civil(2024, 11, 3);
    let (ls, le) = (local_day_start_utc(day), local_day_start_utc(day + 1));
    let hourly: Vec<i64> = (ls - 86400..le + 86400).step_by(3600).collect();
    let in_local = hourly.iter().filter(|&&t| t >= ls && t < le).count();
    let in_utc = hourly
        .iter()
        .filter(|&&t| t.div_euclid(86400) == day)
        .count();
    println!(
        "\ntime zones: on 2024-11-03 hourly samples per bucket: local day {in_local}, naive UTC day {in_utc}"
    );
    assert_eq!((in_local, in_utc), (25, 24));
    // pitfall 2: wall-clock time is not monotonic across the fold, so a series sorted by UTC looks out-of-order in local time
    let utc: Vec<i64> = (from..to).step_by(1800).collect();
    let local: Vec<i64> = utc.iter().map(|&u| u + offset_at(u)).collect();
    let backwards = local.windows(2).filter(|w| w[1] < w[0]).count();
    println!(
        "  wall-clock readings that go backwards in two years of half-hourly UTC samples: {backwards} (one per fall-back)"
    );
    assert_eq!(backwards, 2);
    println!(
        "  local-day lengths in 2024: 23h x{short}, 25h x{long}, 24h x{}; fold hours seen {folds}",
        lens.len() - short - long
    );
    println!("civil-date math, DST gap/fold resolution and local-day buckets: ok");
}

fn main() {
    let mut rng = Rng(0xA50F_6A95_0863);
    test_asof(&mut rng);
    test_window_and_interval(&mut rng);
    test_resample(&mut rng);
    test_policies(&mut rng);
    test_tolerance_table(&mut rng);
    test_time_zones();
    println!("\nall asof_joins_gap_filling checks passed");
}
