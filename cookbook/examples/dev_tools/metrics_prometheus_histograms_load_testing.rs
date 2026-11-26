//! RECIPE 913 (dev_tools): a small metrics library with Prometheus text output, and a load-testing lab: coordinated omission, Little's law, M/M/c, the knee
//! LEVEL: advanced
//! Use in: instrumenting a service, reading dashboards correctly, and not being fooled by your own load test.   Run: cargo run --release --example metrics_prometheus_histograms_load_testing
//! COMMON USES: Prometheus client libraries, HdrHistogram and DDSketch latency recording, wrk2 / k6 / Vegeta constant-rate load generation, capacity planning from queueing formulas.
#![allow(dead_code)]
//!
//! METRIC TYPES: COUNTER only goes up (a restart resets it to 0, which `rate` must handle). GAUGE goes both ways. HISTOGRAM counts
//! observations into buckets with upper bounds `le`; buckets are CUMULATIVE in the exposition, and because they are counters they
//! can be summed across instances, then turned into a quantile. A SUMMARY computes quantiles in the client: percentiles of
//! different instances CANNOT be averaged. The summary here is a log-bucketed sketch (the DDSketch idea): bucket i holds
//! (g^(i-1), g^i] with g = (1+a)/(1-a), so the reported value is within relative error `a` of the true quantile VALUE, and sketches merge.
//! A fixed-bucket histogram guarantees only the right BUCKET; interpolation inside it is a guess.
//! LABELS multiply: every distinct label set is a new time series. A user id as a label is an outage of your monitoring system, so
//! the registry caps series per metric and folds the excess into one `__overflow__` series (and counts it).
//! LOAD GENERATORS: CLOSED loop = N users, each waits for the reply before sending again: when the server stalls, the tester STOPS
//! SENDING, so the stall yields N bad samples instead of hundreds. That is COORDINATED OMISSION. OPEN loop = requests arrive on a
//! schedule regardless of replies, and latency is measured from the INTENDED send time. If you only have closed-loop data taken at
//! a fixed intended rate, HdrHistogram's correction back-fills: a sample v > interval also records v-i, v-2i, ...; it is an
//! ESTIMATE that assumes the missing requests would have waited out the same stall; it ignores the time the backlog takes to drain,
//! so it errs low (quantified and asserted below).
//! QUEUEING: Little's law L = lambda x W holds for any stable system and any discipline. M/M/1: W = 1/(mu - lambda). M/M/c:
//! W = C/(c mu - lambda) + 1/mu with C the Erlang C probability of waiting. Latency explodes as utilisation -> 1.
//! Simulations below use a VIRTUAL clock in microseconds; arrivals and service times are seeded exponential draws.
//! Read: https://prometheus.io/docs/instrumenting/exposition_formats/ and https://prometheus.io/docs/practices/histograms/
//! Read: https://github.com/HdrHistogram/HdrHistogram and https://arxiv.org/abs/1908.10693 (DDSketch)
//!
//! TRY NEXT:
//!   - [S] Change `Sketch` accuracy from 1% to 5%. Predict how the bucket count changes, then print it.
//!   - [S] In `closed_loop` give the tester 100 users instead of 1. Predict whether coordinated omission goes away.
//!   - [M] Implement `histogram_quantile` over the SUM of two instances' bucket counters and show that averaging their two p99s gives a different (wrong) answer.
//!   - [M] Add a bounded server queue that rejects when full. How must Little's law be restated (which lambda)?
//!   - [A] PROJECT: put this registry behind an `axum` `/metrics` endpoint, drive it with an open-loop generator on tokio, and compare measured percentiles with the server's own histogram.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering::*};
use std::sync::{Arc, Mutex};
use std::thread;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in (0, 1].
    fn unit(&mut self) -> f64 {
        ((self.next() >> 11) + 1) as f64 / (1u64 << 53) as f64
    }
    fn exponential(&mut self, mean: f64) -> f64 {
        -mean * self.unit().ln()
    }
}

// ===========================================================================
// 1. instruments
// ===========================================================================
#[derive(Default)]
struct Counter(AtomicU64);
impl Counter {
    fn add(&self, n: u64) {
        self.0.fetch_add(n, Relaxed); // a statistic: no other data is published through it
    }
    fn get(&self) -> u64 {
        self.0.load(Relaxed)
    }
}
/// f64 in an AtomicU64. `add` is a CAS loop because there is no atomic float add.
#[derive(Default)]
struct Gauge(AtomicU64);
impl Gauge {
    fn set(&self, v: f64) {
        self.0.store(v.to_bits(), Relaxed);
    }
    fn add(&self, d: f64) {
        let _ = self.0.fetch_update(Relaxed, Relaxed, |bits| {
            Some((f64::from_bits(bits) + d).to_bits())
        });
    }
    fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Relaxed))
    }
}
struct Histogram {
    bounds: Vec<f64>,        // strictly increasing upper bounds, without +Inf
    buckets: Vec<AtomicU64>, // NOT cumulative in memory; one extra for +Inf
    sum: Gauge,
}
impl Histogram {
    fn new(bounds: Vec<f64>) -> Self {
        assert!(bounds.windows(2).all(|w| w[0] < w[1]) && bounds.iter().all(|b| b.is_finite()));
        let buckets = (0..=bounds.len()).map(|_| AtomicU64::new(0)).collect();
        Histogram {
            bounds,
            buckets,
            sum: Gauge::default(),
        }
    }
    fn linear(start: f64, width: f64, n: usize) -> Self {
        Self::new((0..n).map(|i| start + width * i as f64).collect())
    }
    fn exponential(start: f64, factor: f64, n: usize) -> Self {
        Self::new((0..n).map(|i| start * factor.powi(i as i32)).collect())
    }
    fn observe(&self, v: f64) {
        let i = self.bounds.partition_point(|&b| b < v); // first bound >= v: `le` is inclusive
        self.buckets[i].fetch_add(1, Relaxed);
        self.sum.add(v);
    }
    fn cumulative(&self) -> Vec<u64> {
        self.buckets
            .iter()
            .scan(0, |acc, b| {
                *acc += b.load(Relaxed);
                Some(*acc)
            })
            .collect()
    }
    /// Like PromQL `histogram_quantile`: find the bucket by rank, interpolate linearly inside it.
    fn quantile(&self, q: f64) -> Option<f64> {
        let cum = self.cumulative();
        let total = *cum.last()?;
        if total == 0 {
            return None;
        }
        let rank = ((q * total as f64).ceil() as u64).max(1);
        let i = cum.partition_point(|&c| c < rank);
        if i == self.bounds.len() {
            return self.bounds.last().copied(); // the answer is somewhere above the last bound: all we can say
        }
        let (low, below) = if i == 0 {
            (0.0, 0)
        } else {
            (self.bounds[i - 1], cum[i - 1])
        };
        Some(low + (self.bounds[i] - low) * (rank - below) as f64 / (cum[i] - below) as f64)
    }
}
/// Mergeable quantile sketch with a relative-error guarantee, for positive values.
#[derive(Clone)]
struct Sketch {
    gamma: f64,
    buckets: BTreeMap<i32, u64>,
    count: u64,
}
impl Sketch {
    fn new(alpha: f64) -> Self {
        Sketch {
            gamma: (1.0 + alpha) / (1.0 - alpha),
            buckets: BTreeMap::new(),
            count: 0,
        }
    }
    fn record_n(&mut self, v: f64, n: u64) {
        assert!(v > 0.0 && v.is_finite());
        *self
            .buckets
            .entry((v.ln() / self.gamma.ln()).ceil() as i32)
            .or_default() += n;
        self.count += n;
    }
    fn record(&mut self, v: f64) {
        self.record_n(v, 1);
    }
    /// HdrHistogram's `recordValueWithExpectedInterval`.
    fn record_corrected(&mut self, v: f64, expected_interval: f64) {
        self.record(v);
        let mut missing = v - expected_interval;
        while missing >= expected_interval {
            self.record(missing);
            missing -= expected_interval;
        }
    }
    fn merge(&mut self, other: &Sketch) {
        assert_eq!(
            self.gamma, other.gamma,
            "only sketches with the same buckets merge exactly"
        );
        other
            .buckets
            .iter()
            .for_each(|(&k, &n)| *self.buckets.entry(k).or_default() += n);
        self.count += other.count;
    }
    /// The value of rank ceil(q n) (nearest-rank definition).
    fn quantile(&self, q: f64) -> Option<f64> {
        let rank = ((q * self.count as f64).ceil() as u64).clamp(1, self.count.max(1));
        let mut seen = 0;
        for (&i, &n) in &self.buckets {
            seen += n;
            if seen >= rank {
                return Some(2.0 * self.gamma.powi(i) / (self.gamma + 1.0));
            }
        }
        None
    }
}
fn exact_quantile(sorted: &[f64], q: f64) -> f64 {
    sorted[((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len()) - 1]
}
fn quantile_bounds_test() {
    let mut r = Rng(1);
    let mut worst_sketch: f64 = 0.0;
    let mut worst_fixed: f64 = 0.0;
    for trial in 0..60 {
        let mut values: Vec<f64> = (0..20_000)
            .map(|_| match trial % 3 {
                0 => r.exponential(20.0) + 0.001,
                1 => (r.unit() * 12.0).exp(), // log-uniform over five orders of magnitude
                _ => {
                    if r.unit() < 0.98 {
                        1.0 + r.unit()
                    } else {
                        500.0 + r.unit() * 100.0
                    }
                } // bimodal tail
            })
            .collect();
        let hist = Histogram::exponential(0.5, 2.0, 20);
        // two halves recorded separately, then merged
        let (mut a, mut b) = (Sketch::new(0.01), Sketch::new(0.01));
        for (i, &v) in values.iter().enumerate() {
            hist.observe(v);
            if i % 2 == 0 { a.record(v) } else { b.record(v) }
        }
        a.merge(&b);
        values.sort_by(f64::total_cmp);
        for q in [0.01, 0.5, 0.9, 0.99, 0.999, 1.0] {
            let truth = exact_quantile(&values, q);
            let error = (a.quantile(q).unwrap() - truth).abs() / truth;
            assert!(
                error <= 0.01 + 1e-9,
                "sketch bound violated: q={q} error={error}"
            );
            worst_sketch = worst_sketch.max(error);
            // fixed buckets: the estimate lies in the SAME bucket as the truth, and that is all
            let bucket = |x: f64| hist.bounds.partition_point(|&bound| bound < x);
            let estimate = hist.quantile(q).unwrap();
            assert_eq!(
                bucket(estimate),
                bucket(truth),
                "q={q}: {estimate} vs {truth}"
            );
            worst_fixed = worst_fixed.max((estimate - truth).abs() / truth);
        }
    }
    assert!(worst_fixed > 0.2);
    println!(
        "  60 datasets: sketch worst relative error {:.3}% (bound 1%); doubling buckets: right bucket always, worst error {:.0}%",
        worst_sketch * 100.0,
        worst_fixed * 100.0
    );
}

// ===========================================================================
// 2. registry and text exposition
// ===========================================================================
type Labels = Vec<(String, String)>;
const OVERFLOW: &str = "__overflow__";
struct Family {
    help: String,
    series: BTreeMap<Labels, Arc<Counter>>,
}
/// Counter families only: gauges and histograms expose the same way, one sample line per value.
struct Registry {
    families: Mutex<BTreeMap<String, Family>>,
    max_series: usize,
    overflowed: Counter,
}
fn valid_name(s: &str, colon: bool) -> bool {
    !s.is_empty()
        && !s.starts_with(|c: char| c.is_ascii_digit())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || (colon && c == ':'))
}
impl Registry {
    fn new(max_series: usize) -> Self {
        Registry {
            families: Mutex::default(),
            max_series,
            overflowed: Counter::default(),
        }
    }
    /// Get-or-create. Callers should keep the returned handle: the hot path is then one atomic add, no lock.
    fn counter(&self, name: &str, help: &str, labels: &[(&str, &str)]) -> Arc<Counter> {
        assert!(
            valid_name(name, true) && labels.iter().all(|(k, _)| valid_name(k, false)),
            "bad metric or label name"
        );
        let mut key: Labels = labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        key.sort(); // {a,b} and {b,a} are the same series
        let mut families = self.families.lock().unwrap();
        let family = families.entry(name.to_string()).or_insert_with(|| Family {
            help: help.to_string(),
            series: BTreeMap::new(),
        });
        if !family.series.contains_key(&key) && family.series.len() >= self.max_series {
            self.overflowed.add(1);
            key.iter_mut().for_each(|(_, v)| *v = OVERFLOW.to_string());
        }
        family.series.entry(key).or_default().clone()
    }
    fn expose(&self) -> String {
        let mut out = String::new();
        for (name, family) in self.families.lock().unwrap().iter() {
            out += &format!(
                "# HELP {name} {}\n# TYPE {name} counter\n",
                escape(&family.help, false)
            );
            for (labels, c) in &family.series {
                out += &sample_line(name, labels, c.get() as f64);
            }
        }
        out
    }
}
/// HELP text escapes backslash and newline; label VALUES also escape the double quote.
fn escape(s: &str, quote: bool) -> String {
    let s = s.replace('\\', "\\\\").replace('\n', "\\n");
    if quote { s.replace('"', "\\\"") } else { s }
}
fn sample_line(name: &str, labels: &Labels, value: f64) -> String {
    let inner: Vec<String> = labels
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", escape(v, true)))
        .collect();
    let braces = if inner.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", inner.join(","))
    };
    let value = if value == f64::INFINITY {
        "+Inf".to_string()
    } else {
        value.to_string()
    };
    format!("{name}{braces} {value}\n")
}
fn expose_histogram(name: &str, help: &str, h: &Histogram) -> String {
    let mut out = format!(
        "# HELP {name} {}\n# TYPE {name} histogram\n",
        escape(help, false)
    );
    let cum = h.cumulative();
    for (i, &c) in cum.iter().enumerate() {
        let le = h.bounds.get(i).map_or("+Inf".to_string(), f64::to_string);
        out += &sample_line(
            &format!("{name}_bucket"),
            &vec![("le".into(), le)],
            c as f64,
        );
    }
    out += &sample_line(&format!("{name}_sum"), &vec![], h.sum.get());
    out += &sample_line(
        &format!("{name}_count"),
        &vec![],
        *cum.last().unwrap() as f64,
    );
    out
}
/// A strict parser for the sample lines this file writes (no timestamps, no exemplars).
fn parse_sample(line: &str) -> Option<(String, Labels, f64)> {
    let end = line.find(|c: char| c == '{' || c == ' ')?;
    let (name, mut rest) = line.split_at(end);
    if !valid_name(name, true) {
        return None;
    }
    let mut labels = vec![];
    if let Some(mut body) = rest.strip_prefix('{') {
        loop {
            let (key, after) = body.split_once("=\"")?;
            if !valid_name(key, false) {
                return None;
            }
            let mut value = String::new();
            let mut chars = after.char_indices();
            let close = loop {
                match chars.next()? {
                    (i, '"') => break i,
                    (_, '\\') => value.push(match chars.next()?.1 {
                        'n' => '\n',
                        c @ ('\\' | '"') => c,
                        _ => return None,
                    }),
                    (_, c) => value.push(c),
                }
            };
            labels.push((key.to_string(), value));
            body = &after[close + 1..];
            if let Some(next) = body.strip_prefix(',') {
                body = next;
            } else {
                rest = body.strip_prefix('}')?;
                break;
            }
        }
    }
    let value = rest.strip_prefix(' ')?;
    let value = if value == "+Inf" {
        f64::INFINITY
    } else {
        value.parse().ok()?
    };
    Some((name.to_string(), labels, value))
}
fn parse_exposition(text: &str) -> BTreeMap<(String, Labels), f64> {
    let mut out = BTreeMap::new();
    // split on '\n' only: a raw '\r' is legal inside a label value
    for line in text
        .split('\n')
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (name, labels, v) =
            parse_sample(line).unwrap_or_else(|| panic!("unparseable line {line:?}"));
        assert!(
            out.insert((name, labels), v).is_none(),
            "duplicate series {line:?}"
        );
    }
    out
}
/// Increase over a series of scrapes of one counter. A drop means the process restarted and began again from 0.
fn increase(samples: &[u64]) -> u64 {
    samples
        .windows(2)
        .map(|w| if w[1] >= w[0] { w[1] - w[0] } else { w[1] })
        .sum()
}
fn registry_test() {
    // random hostile label values survive exposition and parsing
    let alphabet = [
        'a', ' ', '"', '\\', '\n', '\r', '{', '}', ',', '=', 'n', 'é', '#',
    ];
    let mut r = Rng(2);
    for _ in 0..500 {
        let reg = Registry::new(1000);
        let mut want = BTreeMap::new();
        for _ in 0..8 {
            let mut word = || {
                (0..r.next() % 9)
                    .map(|_| alphabet[(r.next() % alphabet.len() as u64) as usize])
                    .collect::<String>()
            };
            let (a, b) = (word(), word());
            let n = r.next() % 1000;
            reg.counter(
                "requests_total",
                "help with \\ and\nnewline \"quotes\"",
                &[("path", &a), ("agent", &b)],
            )
            .add(n);
            *want
                .entry((
                    "requests_total".to_string(),
                    vec![("agent".to_string(), b), ("path".to_string(), a)],
                ))
                .or_insert(0.0) += n as f64;
        }
        assert_eq!(parse_exposition(&reg.expose()), want);
    }
    assert_eq!(escape("a\\b\"c\nd", true), r#"a\\b\"c\nd"#);
    assert!(
        parse_sample("x{a=\"unterminated} 1").is_none()
            && parse_sample("1x 2").is_none()
            && parse_sample("x{a=\"\\q\"} 1").is_none()
    );

    // cardinality limit: a user-id label cannot grow the registry without bound, and no increment is lost
    let reg = Registry::new(50);
    (0..10_000).for_each(|u| {
        reg.counter("logins_total", "by user", &[("user", &u.to_string())])
            .add(1)
    });
    let parsed = parse_exposition(&reg.expose());
    assert_eq!(parsed.len(), 51);
    assert_eq!(parsed.values().sum::<f64>(), 10_000.0);
    assert_eq!(
        parsed[&(
            "logins_total".to_string(),
            vec![("user".to_string(), OVERFLOW.to_string())]
        )],
        9950.0
    );
    assert_eq!(reg.overflowed.get(), 9950);

    // histogram exposition: cumulative buckets, +Inf equals _count
    let h = Histogram::linear(10.0, 10.0, 3);
    [5.0, 10.0, 10.5, 29.0, 1000.0]
        .into_iter()
        .for_each(|v| h.observe(v));
    let parsed = parse_exposition(&expose_histogram("latency_ms", "request latency", &h));
    let bucket = |le: &str| {
        parsed[&(
            "latency_ms_bucket".to_string(),
            vec![("le".to_string(), le.to_string())],
        )]
    };
    assert_eq!(
        [bucket("10"), bucket("20"), bucket("30"), bucket("+Inf")],
        [2.0, 3.0, 4.0, 5.0],
        "le is inclusive"
    );
    assert_eq!(parsed[&("latency_ms_count".to_string(), vec![])], 5.0);
    assert_eq!(parsed[&("latency_ms_sum".to_string(), vec![])], 1054.5);
    assert_eq!(
        increase(&[100, 150, 170, 20, 60]),
        50 + 20 + 20 + 40,
        "a reset does not produce a negative rate"
    );

    // 8 threads on the shared instruments: exact totals
    let reg = Arc::new(Registry::new(10));
    let (hist, gauge) = (
        Arc::new(Histogram::exponential(1.0, 2.0, 12)),
        Arc::new(Gauge::default()),
    );
    let hs: Vec<_> = (0..8)
        .map(|t| {
            let (reg, hist, gauge) = (reg.clone(), hist.clone(), gauge.clone());
            thread::spawn(move || {
                let handle = reg.counter("hits_total", "hits", &[("shard", ["a", "b"][t % 2])]);
                for i in 0..20_000u64 {
                    handle.add(1);
                    hist.observe((i % 100) as f64);
                    gauge.add(if i % 2 == 0 { 3.0 } else { -1.0 });
                }
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(
        parse_exposition(&reg.expose()).values().sum::<f64>(),
        160_000.0
    );
    assert_eq!(*hist.cumulative().last().unwrap(), 160_000);
    assert_eq!(
        hist.sum.get(),
        (0..20_000u64).map(|i| (i % 100) as f64).sum::<f64>() * 8.0,
        "integers are exact in f64"
    );
    assert_eq!(gauge.get(), 160_000.0);
    println!(
        "  500 hostile label sets round-tripped; 10000 user labels capped at 50 series + overflow; 8-thread totals exact"
    );
}

// ===========================================================================
// 3. queueing simulation
// ===========================================================================
#[derive(Clone, Copy)]
struct Job {
    arrival: u64, // microseconds
    start: u64,
    end: u64,
}
/// FCFS with `c` identical servers. `service(i, start)` returns the service time of job i.
fn serve_fcfs(arrivals: &[u64], c: usize, mut service: impl FnMut(usize, u64) -> u64) -> Vec<Job> {
    let mut free: BinaryHeap<Reverse<u64>> = (0..c).map(|_| Reverse(0)).collect();
    arrivals
        .iter()
        .enumerate()
        .map(|(i, &arrival)| {
            let Reverse(available) = free.pop().unwrap();
            let start = arrival.max(available);
            let end = start + service(i, start);
            free.push(Reverse(end));
            Job {
                arrival,
                start,
                end,
            }
        })
        .collect()
}
fn poisson_arrivals(n: usize, per_second: f64, r: &mut Rng) -> Vec<u64> {
    let mut t = 0.0;
    (0..n)
        .map(|_| {
            t += r.exponential(1e6 / per_second);
            t as u64
        })
        .collect()
}
/// Integral of "number in system" over [from, to], computed from the arrive/leave events alone.
fn area_in_system(jobs: &[Job], from: u64, to: u64) -> u64 {
    let mut events: Vec<(u64, i64)> = jobs
        .iter()
        .flat_map(|j| [(j.arrival, 1), (j.end, -1)])
        .collect();
    events.sort_unstable();
    let (mut n, mut last, mut area) = (0i64, from, 0u64);
    for (t, d) in events {
        let clipped = t.clamp(from, to);
        area += n as u64 * (clipped - last);
        (n, last) = (n + d, clipped);
    }
    area + n as u64 * (to - last)
}
fn erlang_c(c: u32, offered: f64) -> f64 {
    // Erlang B by its stable recurrence, then convert
    let b = (1..=c).fold(1.0, |b, k| offered * b / (f64::from(k) + offered * b));
    let rho = offered / f64::from(c);
    b / (1.0 - rho + rho * b)
}
/// Mean time in system, seconds.
fn mmc_mean_sojourn(c: u32, lambda: f64, mu: f64) -> f64 {
    assert!(lambda < f64::from(c) * mu, "unstable: no steady state");
    erlang_c(c, lambda / mu) / (f64::from(c) * mu - lambda) + 1.0 / mu
}
fn queueing_test() {
    assert!(
        (erlang_c(1, 0.7) - 0.7).abs() < 1e-12,
        "for one server P(wait) = utilisation"
    );
    assert!(
        (erlang_c(2, 1.0) - 1.0 / 3.0).abs() < 1e-12,
        "textbook value"
    );
    assert!(
        (mmc_mean_sojourn(1, 80.0, 100.0) - 0.05).abs() < 1e-12,
        "M/M/1: 1/(mu - lambda)"
    );
    println!(
        "  {:<22}{:>8}{:>14}{:>14}{:>10}",
        "system", "rho", "W theory ms", "W sim ms", "diff"
    );
    let mu = 100.0; // per server per second: mean service 10 ms
    for (c, rho) in [(1u32, 0.5), (1, 0.8), (4, 0.7), (4, 0.9), (16, 0.8)] {
        let lambda = rho * f64::from(c) * mu;
        let mut r = Rng(100 + u64::from(c) + (rho * 10.0) as u64);
        let arrivals = poisson_arrivals(1_000_000, lambda, &mut r);
        let jobs = serve_fcfs(&arrivals, c as usize, |_, _| r.exponential(1e6 / mu) as u64);
        let total: u64 = jobs.iter().map(|j| j.end - j.arrival).sum();
        let w = total as f64 / jobs.len() as f64 / 1e6;
        let theory = mmc_mean_sojourn(c, lambda, mu);
        let diff = (w - theory).abs() / theory;
        println!(
            "  M/M/{c:<18}{rho:>8.2}{:>14.3}{:>14.3}{:>9.1}%",
            theory * 1e3,
            w * 1e3,
            diff * 100.0
        );
        assert!(diff < 0.05, "simulation disagrees with theory by {diff}");
        if c == 1 {
            let mut s: Vec<f64> = jobs
                .iter()
                .map(|j| (j.end - j.arrival) as f64 / 1e6)
                .collect();
            s.sort_by(f64::total_cmp);
            let p99 = 100.0_f64.ln() / (mu - lambda); // sojourn is exponential(mu - lambda)
            assert!((exact_quantile(&s, 0.99) - p99).abs() / p99 < 0.08);
        }
        // Little, exactly: over a window that starts and ends empty, area under N(t) IS the sum of sojourn times
        let end = jobs.iter().map(|j| j.end).max().unwrap();
        assert_eq!(area_in_system(&jobs, 0, end), total);
        // Little, statistically: a window cut out of the middle, using only arrivals inside it
        let (from, to) = (end / 4, end * 3 / 4);
        let inside: Vec<&Job> = jobs
            .iter()
            .filter(|j| (from..to).contains(&j.arrival))
            .collect();
        let l = area_in_system(&jobs, from, to) as f64 / (to - from) as f64;
        let rate = inside.len() as f64 / (to - from) as f64;
        let mean_w = inside
            .iter()
            .map(|j| (j.end - j.arrival) as f64)
            .sum::<f64>()
            / inside.len() as f64;
        assert!(
            (l - rate * mean_w).abs() / l < 0.01,
            "L={l} lambda*W={}",
            rate * mean_w
        );
    }
    println!("  Little's law: exact on empty-to-empty windows, within 1% on mid-run windows");
}

// ===========================================================================
// 4. coordinated omission
// ===========================================================================
const INTERVAL: u64 = 10_000; // intended: one request every 10 ms
const STALL_FROM: u64 = 20_000_000;
const STALL_TO: u64 = 22_000_000; // the planted fault: nothing completes for 2 s
const REQUESTS: u64 = 10_000; // 100 s of intended load
fn stalling_service(start: u64, normal: u64) -> u64 {
    if (STALL_FROM..STALL_TO).contains(&start) {
        STALL_TO - start + normal
    } else {
        normal
    }
}
/// One user, paced at INTERVAL, that waits for each reply. Returns latency measured from the ACTUAL send.
fn closed_loop(service: u64) -> Vec<u64> {
    let (mut now, mut out) = (0u64, vec![]);
    for i in 0..REQUESTS {
        now = now.max(i * INTERVAL);
        let end = now + stalling_service(now, service);
        out.push(end - now);
        now = end;
    }
    out
}
/// Requests leave on schedule whatever happens; latency is measured from the INTENDED time.
fn open_loop(service: u64) -> Vec<u64> {
    let arrivals: Vec<u64> = (0..REQUESTS).map(|i| i * INTERVAL).collect();
    serve_fcfs(&arrivals, 1, |_, start| stalling_service(start, service))
        .iter()
        .map(|j| j.end - j.arrival)
        .collect()
}
fn sketch_of(values: &[u64], correction: Option<u64>) -> Sketch {
    let mut s = Sketch::new(0.01);
    for &v in values {
        match correction {
            Some(interval) => s.record_corrected(v as f64, interval as f64),
            None => s.record(v as f64),
        }
    }
    s
}
fn coordinated_omission_test() {
    for service in [100u64, 1_000] {
        let (closed, open) = (closed_loop(service), open_loop(service));
        let truth = sketch_of(&open, None);
        let naive = sketch_of(&closed, None);
        let fixed = sketch_of(&closed, Some(INTERVAL));
        println!("  normal service time {:.1} ms", service as f64 / 1000.0);
        println!(
            "    {:<28}{:>9}{:>12}{:>12}{:>12}",
            "recording", "samples", "p98 ms", "p99 ms", "p99.9 ms"
        );
        for (name, s) in [
            ("open loop (what users see)", &truth),
            ("closed loop, naive", &naive),
            ("closed loop, corrected", &fixed),
        ] {
            let q = |q| s.quantile(q).unwrap() / 1000.0;
            println!(
                "    {name:<28}{:>9}{:>12.1}{:>12.1}{:>12.1}",
                s.count,
                q(0.98),
                q(0.99),
                q(0.999)
            );
        }
        let slow = |v: &[u64]| v.iter().filter(|&&l| l > 100_000).count();
        assert_eq!(
            slow(&closed),
            1,
            "the closed-loop tester saw the 2 s stall as ONE slow request"
        );
        assert!(
            slow(&open) > 150,
            "on schedule, the stall delays about 2% of all requests"
        );
        assert!(
            naive.quantile(0.999).unwrap() <= service as f64 * 1.02,
            "naive p99.9 claims the normal service time"
        );
        assert!(
            (fixed.count as i64 - truth.count as i64).abs() <= 200,
            "back-filling restores roughly the missing samples"
        );
        // THE PREDICTION. With j samples slower than the quantile, in a stall of length D:
        //   truth     : the j-th request queued behind the stall waits D - j (interval - service), because the backlog is still draining;
        //   corrected : back-filling pretends the j-th missing request waits D - j interval (it knows nothing about draining).
        // So the correction errs LOW, by about j x service: negligible when service << interval, visible otherwise.
        let stall = (STALL_TO - STALL_FROM) as f64;
        for q in [0.99, 0.995, 0.999] {
            let (t, c) = (truth.quantile(q).unwrap(), fixed.quantile(q).unwrap());
            let above = |s: &Sketch| s.count as f64 - (q * s.count as f64).ceil();
            let predicted_truth = stall - above(&truth) * (INTERVAL - service) as f64;
            let predicted_fixed = stall - above(&fixed) * INTERVAL as f64;
            assert!(
                (t - predicted_truth).abs() / predicted_truth < 0.03,
                "q={q}: truth {t} predicted {predicted_truth}"
            );
            assert!(
                (c - predicted_fixed).abs() / predicted_fixed < 0.03,
                "q={q}: corrected {c} predicted {predicted_fixed}"
            );
            assert!(
                c <= t * 1.02,
                "the correction must not overstate the tail here: {c} vs {t}"
            );
            if service == 100 {
                assert!(
                    (c - t).abs() / t < 0.03,
                    "fast drain: the corrected tail recovers the planted one, q={q}: {c} vs {t}"
                );
            }
        }
        if service == 1_000 {
            let (t, c) = (truth.quantile(0.98).unwrap(), fixed.quantile(0.98).unwrap());
            assert!(
                t > 100_000.0 && c < 2_000.0,
                "near the EDGE of the affected population the estimate can be completely wrong: {c} vs {t}"
            );
        }
    }
    println!(
        "  correction recovers the far tail when the backlog drains fast; it errs low otherwise. An open-loop generator has no such bias."
    );
}

// ===========================================================================
// 5. open vs closed loop under load, and finding the knee
// ===========================================================================
/// N users with exponential think time against one server; returns (throughput per s, mean latency s).
fn closed_users(users: usize, think: f64, service: f64, completions: usize) -> (f64, f64) {
    let mut r = Rng(users as u64);
    let mut sending: BinaryHeap<Reverse<u64>> = (0..users)
        .map(|_| Reverse(r.exponential(think) as u64))
        .collect();
    let (mut free_at, mut total, mut last) = (0u64, 0u64, 0u64);
    for _ in 0..completions {
        let Reverse(arrival) = sending.pop().unwrap(); // FCFS: the earliest sender is served first
        let end = arrival.max(free_at) + r.exponential(service) as u64;
        (free_at, last) = (end, end);
        total += end - arrival;
        sending.push(Reverse(end + r.exponential(think) as u64));
    }
    (
        completions as f64 / last as f64 * 1e6,
        total as f64 / completions as f64 / 1e6,
    )
}
/// Kneedle-style: after scaling both axes to [0, 1], the point furthest BELOW the chord from first to last point.
fn knee(points: &[(f64, f64)]) -> usize {
    let (x0, y0) = points[0];
    let (x1, y1) = points[points.len() - 1];
    let gap = |&(x, y): &(f64, f64)| (x - x0) / (x1 - x0) - (y - y0) / (y1 - y0);
    (0..points.len())
        .max_by(|&a, &b| gap(&points[a]).total_cmp(&gap(&points[b])))
        .unwrap()
}
fn load_curve_test() {
    // closed loop: throughput saturates at 1/service and latency grows only linearly with users: it can never "blow up"
    let (think, service) = (100_000.0, 10_000.0);
    let mut previous = 0.0;
    for users in [1usize, 5, 20, 100, 400] {
        let (x, w) = closed_users(users, think, service, 200_000);
        // interactive response time law (Little over users, think time included): N = X (W + Z)
        assert!((x * (w + think / 1e6) - users as f64).abs() / (users as f64) < 0.03);
        assert!(x <= 101.0 && x >= previous * 0.99, "throughput {x}");
        previous = x;
        if users == 400 {
            assert!(x > 98.0 && (w - (400.0 / x - 0.1)).abs() < 0.1);
        }
    }
    // open loop: sweep offered load and watch p99
    let mut curve = vec![];
    println!(
        "  {:>10}{:>12}{:>16}",
        "offered/s", "p99 ms", "theory p99 ms"
    );
    for lambda in [
        10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 85.0, 90.0, 95.0,
    ] {
        let mut r = Rng(lambda as u64);
        let arrivals = poisson_arrivals(300_000, lambda, &mut r);
        let jobs = serve_fcfs(&arrivals, 1, |_, _| r.exponential(10_000.0) as u64);
        let s = sketch_of(
            &jobs
                .iter()
                .map(|j| (j.end - j.arrival).max(1))
                .collect::<Vec<_>>(),
            None,
        );
        let p99 = s.quantile(0.99).unwrap() / 1000.0;
        println!(
            "  {lambda:>10.0}{p99:>12.1}{:>16.1}",
            100.0_f64.ln() / (100.0 - lambda) * 1000.0
        );
        curve.push((lambda, p99));
    }
    assert!(
        curve.windows(2).all(|w| w[1].1 > w[0].1 * 0.97),
        "p99 rises with load"
    );
    let k = curve[knee(&curve)].0;
    let short = curve[knee(&curve[..8])].0;
    assert!((70.0..=90.0).contains(&k), "knee at {k}");
    assert!(
        short < k,
        "the 'knee' moves when you change the swept range: {short} vs {k}"
    );
    let slo_capacity = curve.iter().take_while(|p| p.1 <= 200.0).last().unwrap().0;
    assert!((70.0..=80.0).contains(&slo_capacity), "{slo_capacity}");
    println!(
        "  knee: {k}/s on the full sweep, {short}/s on a sweep that stops at 80/s. Prefer an SLO: p99 <= 200 ms holds up to {slo_capacity}/s"
    );
}

/// A tiny queue-depth sampler: why an average can hide a burst.
fn gauge_sampling_demo() {
    let mut queue: VecDeque<u32> = VecDeque::new();
    let (mut peak, mut sampled_peak) = (0, 0);
    for t in 0..10_000u32 {
        if t % 1000 == 500 {
            queue.extend(0..300); // a burst between two scrapes
        }
        peak = peak.max(queue.len());
        if t % 1000 == 0 {
            sampled_peak = sampled_peak.max(queue.len());
        }
        queue.pop_front();
    }
    assert_eq!((peak, sampled_peak), (300, 0));
    println!(
        "  a gauge scraped every 1000 ticks reported peak depth {sampled_peak}; the real peak was {peak}: record maxima or histograms too"
    );
}

fn main() {
    println!("== 1. quantiles: sketch vs fixed buckets ==");
    quantile_bounds_test();
    println!("== 2. registry and exposition ==");
    registry_test();
    gauge_sampling_demo();
    println!("== 3. M/M/c and Little's law ==");
    queueing_test();
    println!("== 4. coordinated omission (2 s stall in a 100 s test) ==");
    coordinated_omission_test();
    println!("== 5. closed loop, open loop and the knee ==");
    load_curve_test();
    println!("all metrics and load-testing checks passed");
}
