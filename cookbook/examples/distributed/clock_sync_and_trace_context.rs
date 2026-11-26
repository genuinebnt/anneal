//! RECIPE 406 (distributed): clock synchronisation (Cristian, NTP, Berkeley, slew vs step) and W3C Trace Context
//! LEVEL: intermediate
//! Use in: timestamps you can compare across machines; propagating trace ids through service calls.   Run: cargo run --release --example clock_sync_and_trace_context
#![allow(dead_code)]
//!
//! Two halves of "what happened when, across machines":
//!
//! (a) CLOCKS. Machines disagree about the time (offset) and the disagreement grows (drift, tens of ppm).
//!  * Cristian: ask a time server, add half the round trip to its answer. Error is exactly (d_req - d_resp)/2
//!    and never more than RTT/2 - min_one_way_delay.
//!  * NTP's four timestamps t1 (client send), t2 (server receive), t3 (server send), t4 (client receive):
//!    offset = ((t2-t1)+(t3-t4))/2 = O + (d_req-d_resp)/2 and delay = (t4-t1)-(t3-t2) = d_req+d_resp. Server
//!    processing time cancels. The error is HALF THE ASYMMETRY and is bounded by delay/2; it cannot be
//!    detected from the timestamps (two different worlds give identical t1..t4), which is why NTP keeps the
//!    sample with the smallest delay out of the last 8.
//!  * Berkeley: no reference clock; a coordinator polls everyone, drops outliers, averages the rest and tells
//!    each node a RELATIVE adjustment (so request delay never enters absolute time).
//!  * Applying a correction: STEPPING sets the clock at once and can make time run backwards (breaks
//!    timers, ordering, "elapsed = now - start"); SLEWING changes the clock rate by at most 500 ppm until the
//!    error is gone (monotonic, but 100 ms takes 200 s). NTP steps only above ~128 ms.
//!
//! (b) TRACE CONTEXT. The W3C `traceparent` header `version-traceid-parentid-flags` (`00-<32 hex>-<16 hex>-<2 hex>`)
//! carries the trace id and the caller's span id across process boundaries; `tracestate` carries vendor data
//! (comma separated `key=value`, newest first, max 32 members). Rules implemented and tested: lowercase hex
//! only, exact length for version 00, version ff forbidden, all-zero trace or parent ids invalid, unknown
//! higher versions accepted if the first four fields are well-formed (forward compatibility), and an invalid
//! header means the callee starts a NEW trace. A 5-service simulation propagates headers, each service
//! creates a child span, spans arrive at the collector in completion order (out of order), and the tree is
//! rebuilt from (span id, parent id) only.
//!
//! (c) CRITICAL PATH: the chain of spans that actually gated the end-to-end latency. Walk backwards from the
//! parent's end: the last-finishing child gates it; time between children is the parent's own (self) time;
//! then continue from that child's start. Parallel siblings that finished earlier are off the critical path.
//! The segments tile the root's duration exactly, which the simulation asserts for every trace.
//!
//! COMMON USES: OpenTelemetry, Jaeger and Zipkin propagate exactly this header (W3C Trace Context); distributed
//! databases use bounded clock error for timestamps (Spanner's TrueTime exposes the error interval; hybrid
//! logical clocks tolerate it); chrony/ntpd/PTP keep servers in sync; log correlation across microservices;
//! critical-path analysis is how "which service should I optimise" is answered in tracing UIs.
//! Read: Trace Context (W3C Recommendation)   https://www.w3.org/TR/trace-context/
//! Read: RFC 5905 Network Time Protocol Version 4   https://datatracker.ietf.org/doc/html/rfc5905
//! Read: Time, Clocks, and the Ordering of Events in a Distributed System (Lamport, 1978)
//!   https://lamport.azurewebsites.net/pubs/time-clocks.pdf

use std::collections::{BTreeMap, HashMap, HashSet};

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
    fn irange(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn nonzero_u64(&mut self) -> u64 {
        loop {
            let x = self.next();
            if x != 0 {
                return x;
            }
        }
    }
    fn nonzero_u128(&mut self) -> u128 {
        loop {
            let x = (self.next() as u128) << 64 | self.next() as u128;
            if x != 0 {
                return x;
            }
        }
    }
}

// =============================================================== (a) clock synchronisation
// All times are integer microseconds so the algebra below is exact (no float rounding in the asserts).

/// One NTP-style exchange. `co`/`so` are client/server clock offsets from true time, `d1`/`d2` the one-way
/// delays, `p` the server's processing time. Returns (2*theta, delta) so there is no halving.
fn ntp_exchange(co: i64, so: i64, d1: i64, d2: i64, p: i64) -> (i64, i64) {
    let t0 = 1_000_000_000; // true time of the request
    let t1 = t0 + co;
    let t2 = t0 + d1 + so;
    let t3 = t2 + p;
    let t4 = t0 + d1 + p + d2 + co;
    ((t2 - t1) + (t3 - t4), (t4 - t1) - (t3 - t2))
}

/// Cristian: server stamps its reply immediately (p = 0). Returns 2*estimated offset.
fn cristian_exchange(co: i64, so: i64, d1: i64, d2: i64) -> i64 {
    let t0 = 1_000_000_000;
    let t1 = t0 + co;
    let server_ts = t0 + d1 + so;
    let t4 = t0 + d1 + d2 + co;
    // estimate of the server clock when the reply lands = server_ts + RTT/2; offset = that - t4
    2 * server_ts + (t4 - t1) - 2 * t4
}

fn demo_cristian_and_ntp() {
    let mut rng = Rng::new(11);
    let min_delay = 300;
    let mut worst_ratio: f64 = 0.0;
    for _ in 0..5000 {
        let (co, so) = (
            rng.irange(-5_000_000, 5_000_000),
            rng.irange(-5_000_000, 5_000_000),
        );
        let (d1, d2) = (rng.irange(min_delay, 20_000), rng.irange(min_delay, 20_000));
        let o = so - co;

        // Cristian: error = (d1 - d2)/2 exactly, and |error| <= RTT/2 - min_delay.
        let est2 = cristian_exchange(co, so, d1, d2);
        assert_eq!(est2 - 2 * o, d1 - d2);
        assert!((est2 - 2 * o).abs() <= (d1 + d2) - 2 * min_delay);

        // NTP with processing time: p cancels; error is half the asymmetry; |error| <= delay/2.
        let p = rng.irange(0, 3_000);
        let (theta2, delta) = ntp_exchange(co, so, d1, d2, p);
        assert_eq!(delta, d1 + d2, "processing time is excluded from the delay");
        assert_eq!(theta2 - 2 * o, d1 - d2, "error = (d_req - d_resp)/2");
        assert!((theta2 - 2 * o).abs() <= delta);
        worst_ratio = worst_ratio.max((theta2 - 2 * o).abs() as f64 / delta as f64);
    }
    println!(
        "Cristian + NTP: 5000 random exchanges, error == half the asymmetry, never above delay/2 (worst ratio {worst_ratio:.3} of the bound)"
    );

    // Indistinguishable worlds: same four timestamps up to the asymmetry, different true offsets.
    let a = ntp_exchange(0, 0, 1_000, 9_000, 0); // symmetric offset 0, request fast, response slow
    let b = ntp_exchange(0, -4_000, 5_000, 5_000, 0); // true offset -4 ms, symmetric network
    assert_eq!(
        a, b,
        "identical (offset, delay): asymmetry cannot be seen from the timestamps"
    );
    println!(
        "  asymmetric 1ms/9ms path with offset 0 looks identical to a symmetric path with offset -4ms (theta*2 = {})",
        a.0
    );

    // NTP clock filter: keep the lowest-delay of 8 samples.
    let (mut single, mut filtered) = (0i64, 0i64);
    let trials = 2000;
    for _ in 0..trials {
        let so = rng.irange(-1_000_000, 1_000_000);
        let mut best: Option<(i64, i64)> = None;
        for i in 0..8 {
            // Queueing delay is mostly one-sided and bursty: large delay <=> large asymmetry.
            let (d1, d2) = (rng.irange(500, 30_000), rng.irange(500, 30_000));
            let (t2x, delta) = ntp_exchange(0, so, d1, d2, 100);
            if i == 0 {
                single += (t2x - 2 * so).abs();
            }
            if best.is_none_or(|b| delta < b.1) {
                best = Some((t2x, delta));
            }
        }
        filtered += (best.unwrap().0 - 2 * so).abs();
    }
    println!(
        "  clock filter: mean |error| single sample {:.0} us vs min-delay of 8 {:.0} us",
        single as f64 / trials as f64 / 2.0,
        filtered as f64 / trials as f64 / 2.0
    );
    assert!(
        filtered * 10 < single * 8,
        "min-delay selection should clearly beat a single sample"
    );
}

/// Berkeley algorithm. offsets[0] is the coordinator (0 by definition: everyone is measured relative to it).
/// Returns (robust_avg, naive_avg, max spread of clocks after adjustment).
fn berkeley_round(
    true_offsets: &[i64],
    rng: &mut Rng,
    reject_us: i64,
) -> (i64, i64, i64, Vec<i64>) {
    // 1. Poll: measured difference = truth + half-asymmetry error (|e| <= 1 ms).
    let measured: Vec<i64> = true_offsets
        .iter()
        .enumerate()
        .map(|(i, &o)| {
            if i == 0 {
                0
            } else {
                o + (rng.irange(200, 2200) - rng.irange(200, 2200)) / 2
            }
        })
        .collect();
    // 2. Outlier rejection around the median.
    let mut sorted = measured.clone();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    let good: Vec<i64> = measured
        .iter()
        .copied()
        .filter(|m| (m - median).abs() <= reject_us)
        .collect();
    let robust = good.iter().sum::<i64>() / good.len() as i64;
    let naive = measured.iter().sum::<i64>() / measured.len() as i64;
    // 3. Send each node a RELATIVE adjustment: robust_avg - its measured difference.
    let after: Vec<i64> = true_offsets
        .iter()
        .zip(&measured)
        .map(|(&o, &m)| o + (robust - m))
        .collect();
    let spread = after.iter().max().unwrap() - after.iter().min().unwrap();
    (robust, naive, spread, after)
}

fn demo_berkeley() {
    let mut rng = Rng::new(21);
    for trial in 0..1000 {
        let n = rng.range(4, 8) as usize;
        let mut offs: Vec<i64> = (0..n)
            .map(|i| {
                if i == 0 {
                    0
                } else {
                    rng.irange(-30_000, 30_000)
                }
            })
            .collect();
        let bad = 1 + rng.next() as usize % (n - 1);
        offs[bad] = if rng.chance(50) {
            5_000_000
        } else {
            -3_000_000
        }; // wildly wrong clock
        let truth: Vec<i64> = offs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != bad)
            .map(|(_, o)| *o)
            .collect();
        let true_avg = truth.iter().sum::<i64>() / truth.len() as i64;
        let (robust, naive, spread, after) = berkeley_round(&offs, &mut rng, 200_000);
        assert!(
            (robust - true_avg).abs() <= 2_200,
            "trial {trial}: outlier must not move the average"
        );
        assert!(
            (naive - true_avg).abs() > 100_000,
            "the naive mean IS wrecked by the outlier"
        );
        assert!(
            spread <= 4_500,
            "after one round everybody (even the faulty clock) agrees within measurement error, got {spread}"
        );
        assert!(after.iter().all(|a| (a - robust).abs() <= 2_200));
    }
    println!(
        "Berkeley: 1000 rounds, 1 faulty clock (+5s / -3s): robust average within 2.2 ms of the good clocks' mean, naive mean off by >100 ms, all nodes within 4.5 ms afterwards"
    );
}

/// A software clock ticking in true-time steps. `pending` is correction still to be slewed in.
struct SoftClock {
    local: f64, // microseconds
    drift_ppm: f64,
    pending: f64,
}

const SLEW_MAX: f64 = 500e-6; // NTP slews at most 500 ppm

impl SoftClock {
    fn tick(&mut self, dt: f64) {
        let max = dt * SLEW_MAX;
        let applied = self.pending.clamp(-max, max);
        self.pending -= applied;
        self.local += dt * (1.0 + self.drift_ppm * 1e-6) + applied;
    }
}

fn demo_step_vs_slew() {
    // Clock is 100 ms fast. Step: instant, but time goes backwards.
    let dt = 1_000.0; // 1 ms of true time
    let mut c = SoftClock {
        local: 100_000.0,
        drift_ppm: 0.0,
        pending: 0.0,
    };
    let mut backwards = 0;
    for tick in 0..20_000 {
        let before = c.local;
        if tick == 10_000 {
            c.local -= 100_000.0; // STEP
        }
        c.tick(dt);
        if c.local < before {
            backwards += 1;
        }
    }
    assert_eq!(backwards, 1);
    println!(
        "step: clock 100 ms fast corrected instantly -> time went backwards {backwards} time(s)"
    );

    // Slew: same error, never backwards, converges in offset / 500ppm = 200 s.
    let mut c = SoftClock {
        local: 100_000.0,
        drift_ppm: 0.0,
        pending: -100_000.0,
    };
    let mut t_true = 0.0;
    let mut converged_at = None;
    let mut non_monotonic = 0;
    for _ in 0..400_000 {
        let before = c.local;
        c.tick(dt);
        t_true += dt;
        if c.local <= before {
            non_monotonic += 1;
        }
        if converged_at.is_none() && (c.local - t_true).abs() < 1.0 {
            converged_at = Some(t_true / 1e6);
        }
    }
    let at = converged_at.expect("slew converges");
    assert_eq!(non_monotonic, 0);
    assert!(
        (at - 200.0).abs() < 0.1,
        "100 ms / 500 ppm = 200 s, got {at}"
    );
    println!(
        "slew: same 100 ms error removed monotonically, converged after {at:.1} s (= 100 ms / 500 ppm)"
    );

    // One hour with 50 ppm drift, NTP-like poll every 64 s with +-1 ms measurement error, slew-only corrections.
    let mut rng = Rng::new(5);
    let step = 100_000.0; // 100 ms ticks
    let (mut synced, mut free) = (
        SoftClock {
            local: 0.0,
            drift_ppm: 50.0,
            pending: 0.0,
        },
        SoftClock {
            local: 0.0,
            drift_ppm: 50.0,
            pending: 0.0,
        },
    );
    let (mut t, mut max_err, mut last_local) = (0.0f64, 0.0f64, 0.0f64);
    let mut next_poll = 64e6;
    while t < 3600e6 {
        synced.tick(step);
        free.tick(step);
        t += step;
        assert!(synced.local > last_local, "slewed clock stays monotonic");
        last_local = synced.local;
        if t >= next_poll {
            let noise = rng.irange(-1000, 1000) as f64;
            // Replace (not add to) the schedule: the new measurement already reflects what was slewed so far.
            synced.pending = -((synced.local - t) + noise);
            next_poll += 64e6;
        }
        if t > 300e6 {
            max_err = max_err.max((synced.local - t).abs());
        }
    }
    let free_err = (free.local - t).abs();
    println!(
        "one hour at 50 ppm drift: free-running error {:.1} ms, polled+slewed max error {:.2} ms",
        free_err / 1e3,
        max_err / 1e3
    );
    assert!(free_err > 150_000.0);
    assert!(max_err < 6_000.0);
}

// =============================================================== (b) W3C Trace Context

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct TraceParent {
    version: u8,
    trace_id: u128,
    parent_id: u64,
    flags: u8,
}

#[derive(Debug, PartialEq, Eq)]
enum TpError {
    TooShort,
    BadLength,
    BadSeparator,
    ForbiddenVersion,
    NotLowerHex(&'static str),
    ZeroTraceId,
    ZeroParentId,
}

/// Lowercase hex only: `u128::from_str_radix` would also accept uppercase and a leading '+'.
fn hex_exact(s: &str, what: &'static str) -> Result<u128, TpError> {
    if s.is_empty() || !s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(TpError::NotLowerHex(what));
    }
    Ok(u128::from_str_radix(s, 16).expect("validated hex"))
}

impl TraceParent {
    fn parse(h: &str) -> Result<TraceParent, TpError> {
        if h.len() < 55 {
            return Err(TpError::TooShort);
        }
        if !h.is_ascii() {
            return Err(TpError::NotLowerHex("non-ascii"));
        }
        let b = h.as_bytes();
        if b[2] != b'-' || b[35] != b'-' || b[52] != b'-' {
            return Err(TpError::BadSeparator);
        }
        let version = hex_exact(&h[0..2], "version")? as u8;
        if version == 0xff {
            return Err(TpError::ForbiddenVersion);
        }
        if version == 0 && h.len() != 55 {
            return Err(TpError::BadLength);
        }
        // Higher versions may append fields, but only after a '-' (forward compatibility).
        if version != 0 && h.len() > 55 && b[55] != b'-' {
            return Err(TpError::BadLength);
        }
        let trace_id = hex_exact(&h[3..35], "trace-id")?;
        let parent_id = hex_exact(&h[36..52], "parent-id")? as u64;
        let flags = hex_exact(&h[53..55], "flags")? as u8;
        if trace_id == 0 {
            return Err(TpError::ZeroTraceId);
        }
        if parent_id == 0 {
            return Err(TpError::ZeroParentId);
        }
        Ok(TraceParent {
            version,
            trace_id,
            parent_id,
            flags,
        })
    }

    fn sampled(&self) -> bool {
        self.flags & 1 == 1
    }
}

impl std::fmt::Display for TraceParent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02x}-{:032x}-{:016x}-{:02x}",
            self.version, self.trace_id, self.parent_id, self.flags
        )
    }
}

/// `tracestate`: ordered list, most recently updated first, at most 32 members, unique keys.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TraceState(Vec<(String, String)>);

fn key_chars_ok(s: &str, first_digit_ok: bool) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(c) if c.is_ascii_lowercase() || (first_digit_ok && c.is_ascii_digit()) => {}
        _ => return false,
    }
    it.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '*' | '/'))
}

fn valid_key(k: &str) -> bool {
    match k.split_once('@') {
        Some((tenant, system)) => {
            (1..=241).contains(&tenant.len())
                && (1..=14).contains(&system.len())
                && key_chars_ok(tenant, true)
                && key_chars_ok(system, false)
        }
        None => (1..=256).contains(&k.len()) && key_chars_ok(k, false),
    }
}

fn valid_value(v: &str) -> bool {
    (1..=256).contains(&v.len())
        && v.bytes()
            .all(|b| (0x20..=0x7e).contains(&b) && b != b',' && b != b'=')
        && !v.ends_with(' ')
}

impl TraceState {
    fn parse(h: &str) -> Result<TraceState, &'static str> {
        let mut out: Vec<(String, String)> = Vec::new();
        for member in h.split(',').map(str::trim).filter(|m| !m.is_empty()) {
            let (k, v) = member.split_once('=').ok_or("member without '='")?;
            if !valid_key(k) || !valid_value(v) {
                return Err("invalid key or value");
            }
            if out.iter().any(|(ek, _)| ek == k) {
                return Err("duplicate key");
            }
            out.push((k.to_string(), v.to_string()));
        }
        if out.len() > 32 {
            return Err("more than 32 members");
        }
        Ok(TraceState(out))
    }

    /// Vendor update: remove the old entry, put the new one first, drop the rightmost beyond 32.
    fn set_front(&mut self, k: &str, v: &str) {
        assert!(valid_key(k) && valid_value(v));
        self.0.retain(|(ek, _)| ek != k);
        self.0.insert(0, (k.to_string(), v.to_string()));
        self.0.truncate(32);
    }

    fn header(&self) -> String {
        self.0
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn keys(&self) -> Vec<&str> {
        self.0.iter().map(|(k, _)| k.as_str()).collect()
    }
}

fn demo_traceparent_validation() {
    let good = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let tp = TraceParent::parse(good).unwrap();
    assert_eq!(tp.trace_id, 0x4bf92f3577b34da6a3ce929d0e0e4736);
    assert_eq!(tp.parent_id, 0x00f067aa0ba902b7);
    assert!(tp.sampled());
    assert_eq!(tp.to_string(), good, "format(parse(x)) == x");

    let cases: Vec<(&str, &str, Result<(), TpError>)> = vec![
        (
            "uppercase hex",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            Err(TpError::NotLowerHex("trace-id")),
        ),
        (
            "all-zero trace id",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            Err(TpError::ZeroTraceId),
        ),
        (
            "all-zero parent id",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
            Err(TpError::ZeroParentId),
        ),
        (
            "version ff",
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            Err(TpError::ForbiddenVersion),
        ),
        (
            "too short",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-1",
            Err(TpError::TooShort),
        ),
        (
            "v00 with trailing data",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-extra",
            Err(TpError::BadLength),
        ),
        (
            "wrong separator",
            "00_4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            Err(TpError::BadSeparator),
        ),
        (
            "non-hex flags",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-0g",
            Err(TpError::NotLowerHex("flags")),
        ),
        (
            "plus sign sneaks past from_str_radix",
            "00-+bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            Err(TpError::NotLowerHex("trace-id")),
        ),
        (
            "future version 01 + extra field",
            "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-what-the-future-holds",
            Ok(()),
        ),
        (
            "future version, junk not after '-'",
            "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01x",
            Err(TpError::BadLength),
        ),
        (
            "unsampled flag",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00",
            Ok(()),
        ),
    ];
    for (name, header, expect) in cases {
        let got = TraceParent::parse(header).map(|_| ());
        println!("  traceparent {name:40} -> {got:?}");
        assert_eq!(got, expect, "{name}");
    }

    // Round-trip and mutation fuzz: generated headers parse back; one mutated character never panics, and
    // whatever still parses must re-serialise to canonical form.
    let mut rng = Rng::new(77);
    let mut mutated_ok = 0;
    for _ in 0..3000 {
        let tp = TraceParent {
            version: 0,
            trace_id: rng.nonzero_u128(),
            parent_id: rng.nonzero_u64(),
            flags: rng.range(0, 3) as u8,
        };
        let s = tp.to_string();
        assert_eq!(TraceParent::parse(&s), Ok(tp));
        let mut bytes = s.into_bytes();
        let i = rng.next() as usize % bytes.len();
        bytes[i] = b" -_GgZ+0f"[rng.next() as usize % 9];
        let m = String::from_utf8(bytes).unwrap();
        if let Ok(p) = TraceParent::parse(&m) {
            mutated_ok += 1;
            assert_eq!(
                p.to_string(),
                m,
                "accepted mutation must already be canonical"
            );
        }
    }
    println!(
        "  fuzz: 3000 round trips ok; {mutated_ok} of 3000 single-char mutations still valid (e.g. 0 -> 0), rest rejected without panic"
    );

    // tracestate
    let mut ts = TraceState::parse("congo=t61rcWkgMzE,rojo=00f067aa0ba902b7").unwrap();
    ts.set_front("congo", "lZWRzIHRoNhcm5pbmc"); // update moves to the front, no duplicate
    assert_eq!(
        ts.header(),
        "congo=lZWRzIHRoNhcm5pbmc,rojo=00f067aa0ba902b7"
    );
    for bad in [
        "a=1,a=2",
        "UPPER=1",
        "k=",
        "k=a=b",
        "noequals",
        "k=trailing ",
    ] {
        assert!(
            TraceState::parse(bad).is_err() || bad == "k=trailing ",
            "{bad}"
        ); // trailing OWS is trimmed, so that one is fine
    }
    assert!(TraceState::parse("vendor@tenant=1,tenant@sys=2").is_ok());
    assert!(TraceState::parse("a@b@c=1").is_err());
    let many: String = (0..33)
        .map(|i| format!("k{i}=v"))
        .collect::<Vec<_>>()
        .join(",");
    assert!(TraceState::parse(&many).is_err(), "more than 32 members");
    let mut full = TraceState::parse(&many.rsplit_once(',').unwrap().0).unwrap();
    full.set_front("new", "x");
    assert_eq!(full.0.len(), 32);
    assert_eq!(full.keys()[0], "new");
    assert!(
        !full.keys().contains(&"k31"),
        "rightmost entry dropped at the 32-member cap"
    );
    println!(
        "  tracestate: update-moves-to-front, duplicate/invalid rejection and the 32-member cap verified"
    );
}

// =============================================================== simulated services + collector

#[derive(Debug, Clone)]
struct Span {
    trace_id: u128,
    span_id: u64,
    parent: Option<u64>,
    svc: &'static str,
    start: u64,
    end: u64,
    flags: u8,
    tracestate_in: Vec<String>,
}

struct World {
    rng: Rng,
    spans: Vec<Span>,
    restarts: u32,
}

impl World {
    fn stages(&mut self, svc: &str) -> Vec<Vec<&'static str>> {
        match svc {
            "frontend" => vec![vec!["auth", "orders"]], // auth runs in parallel with orders
            "orders" => {
                let n = self.rng.range(1, 2) as usize;
                vec![vec!["inventory"; n], vec!["payments"]]
            }
            "payments" => vec![vec!["fraud"]],
            _ => vec![],
        }
    }

    /// Handle one request. `traceparent`/`tracestate` are the incoming headers. Returns the end time.
    fn handle(
        &mut self,
        svc: &'static str,
        traceparent: Option<&str>,
        tracestate: Option<&str>,
        arrive: u64,
    ) -> u64 {
        let (trace_id, parent, flags) = match traceparent.map(TraceParent::parse) {
            Some(Ok(tp)) => (tp.trace_id, Some(tp.parent_id), tp.flags),
            invalid => {
                if invalid.is_some() {
                    self.restarts += 1; // spec: an invalid traceparent means "start a new trace"
                }
                (self.rng.nonzero_u128(), None, self.rng.chance(50) as u8) // head-based sampling decision
            }
        };
        let mut state = tracestate
            .and_then(|h| TraceState::parse(h).ok())
            .unwrap_or_default();
        let tracestate_in = state.keys().iter().map(|k| k.to_string()).collect();
        let span_id = self.rng.nonzero_u64();
        state.set_front(svc, &format!("{span_id:016x}"));

        let mut t = arrive + self.rng.range(1, 4);
        for stage in self.stages(svc) {
            let mut stage_end = t;
            for callee in stage {
                let out = TraceParent {
                    version: 0,
                    trace_id,
                    parent_id: span_id,
                    flags,
                }
                .to_string();
                let net = self.rng.range(1, 3);
                let done = self.handle(callee, Some(&out), Some(&state.header()), t + net);
                stage_end = stage_end.max(done + net);
            }
            t = stage_end + self.rng.range(0, 2);
        }
        let end = t + self.rng.range(1, 4);
        if flags & 1 == 1 {
            // Collector only keeps sampled traces; spans arrive in completion order, children before parents.
            self.spans.push(Span {
                trace_id,
                span_id,
                parent,
                svc,
                start: arrive,
                end,
                flags,
                tracestate_in,
            });
        }
        end
    }
}

// =============================================================== (c) tree reconstruction + critical path

struct Tree {
    spans: Vec<Span>,
    kids: Vec<Vec<usize>>,
    root: usize,
    external_parent: bool,
}

/// Rebuild the tree of one trace from (span id, parent id) alone and validate it.
fn build_tree(spans: Vec<Span>) -> Tree {
    let index: HashMap<u64, usize> = spans
        .iter()
        .enumerate()
        .map(|(i, s)| (s.span_id, i))
        .collect();
    assert_eq!(index.len(), spans.len(), "span ids unique within a trace");
    let mut kids = vec![Vec::new(); spans.len()];
    let mut roots = Vec::new();
    for (i, s) in spans.iter().enumerate() {
        match s.parent.and_then(|p| index.get(&p)) {
            Some(&p) => kids[p].push(i),
            None => roots.push(i), // no parent, or the parent lives outside our system (external caller)
        }
    }
    assert_eq!(roots.len(), 1, "exactly one root per trace");
    let root = roots[0];
    for (p, ks) in kids.iter().enumerate() {
        for &c in ks {
            assert_eq!(spans[c].trace_id, spans[p].trace_id);
            assert!(
                spans[c].start >= spans[p].start && spans[c].end <= spans[p].end,
                "child interval inside parent"
            );
        }
    }
    // Reachability: every span hangs off the root (no cycles / islands).
    let mut seen = HashSet::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        assert!(seen.insert(n), "cycle");
        stack.extend(&kids[n]);
    }
    assert_eq!(seen.len(), spans.len());
    let external_parent = spans[root].parent.is_some();
    Tree {
        spans,
        kids,
        root,
        external_parent,
    }
}

type Seg = (usize, u64, u64); // (span index, from, to)

/// Walk backwards from the span's end: the last-finishing child that ended no later than the cursor owns the
/// time up to its end; anything uncovered is this span's own time.
fn critical_path(t: &Tree, i: usize, out: &mut Vec<Seg>) {
    let s = &t.spans[i];
    let mut cursor = s.end;
    let mut cands = t.kids[i].clone();
    loop {
        let next = cands
            .iter()
            .copied()
            .filter(|&c| t.spans[c].end <= cursor)
            .max_by_key(|&c| (t.spans[c].end, std::cmp::Reverse(t.spans[c].start)));
        let Some(c) = next else {
            if cursor > s.start {
                out.push((i, s.start, cursor));
            }
            return;
        };
        if t.spans[c].end < cursor {
            out.push((i, t.spans[c].end, cursor)); // gap after the child: the parent was working
        }
        critical_path(t, c, out);
        cursor = t.spans[c].start;
        cands.retain(|&x| x != c);
    }
}

fn path_of(t: &Tree) -> Vec<Seg> {
    let mut segs = Vec::new();
    critical_path(t, t.root, &mut segs);
    segs.sort_by_key(|s| (s.1, s.2));
    segs
}

fn check_tiling(t: &Tree, segs: &[Seg]) {
    let root = &t.spans[t.root];
    assert_eq!(segs.first().unwrap().1, root.start);
    assert_eq!(segs.last().unwrap().2, root.end);
    for w in segs.windows(2) {
        assert_eq!(
            w[0].2, w[1].1,
            "segments must tile the root interval without gap or overlap"
        );
    }
    for &(i, a, b) in segs {
        assert!(t.spans[i].start <= a && b <= t.spans[i].end && a < b);
    }
    let total: u64 = segs.iter().map(|s| s.2 - s.1).sum();
    assert_eq!(total, root.end - root.start);
}

fn mk(id: u64, parent: Option<u64>, svc: &'static str, start: u64, end: u64) -> Span {
    Span {
        trace_id: 1,
        span_id: id,
        parent,
        svc,
        start,
        end,
        flags: 1,
        tracestate_in: vec![],
    }
}

fn demo_critical_path_hand_built() {
    // R[0,100]; A[10,40] (with A1[15,30]) and B[20,90] run in parallel; C[92,98] follows.
    let spans = vec![
        mk(1, None, "frontend", 0, 100),
        mk(2, Some(1), "orders", 10, 40),
        mk(3, Some(2), "inventory", 15, 30),
        mk(4, Some(1), "payments", 20, 90),
        mk(5, Some(1), "audit", 92, 98),
    ];
    let tree = build_tree(spans);
    let segs = path_of(&tree);
    let named: Vec<(&str, u64, u64)> = segs
        .iter()
        .map(|&(i, a, b)| (tree.spans[i].svc, a, b))
        .collect();
    println!("hand-built critical path: {named:?}");
    assert_eq!(
        named,
        vec![
            ("frontend", 0, 20),
            ("payments", 20, 90),
            ("frontend", 90, 92),
            ("audit", 92, 98),
            ("frontend", 98, 100)
        ]
    );
    check_tiling(&tree, &segs);
    // orders (A) and inventory ran concurrently with payments and are NOT on the critical path.
    assert!(!named.iter().any(|s| s.0 == "orders" || s.0 == "inventory"));
}

fn print_tree(t: &Tree, i: usize, depth: usize, on_path: &HashSet<usize>) {
    let s = &t.spans[i];
    println!(
        "    {}{} [{:3},{:3}] {}{}",
        "  ".repeat(depth),
        s.svc,
        s.start,
        s.end,
        s.end - s.start,
        if on_path.contains(&i) {
            "  <- critical"
        } else {
            ""
        }
    );
    let mut ks = t.kids[i].clone();
    ks.sort_by_key(|&k| t.spans[k].start);
    for k in ks {
        print_tree(t, k, depth + 1, on_path);
    }
}

fn demo_simulation() {
    let mut w = World {
        rng: Rng::new(2024),
        spans: Vec::new(),
        restarts: 0,
    };
    let (traces, mut external, mut garbage, mut unsampled) = (600u32, 0u32, 0u32, 0u32);
    let mut external_ids = Vec::new();
    for _ in 0..traces {
        let mut r = Rng::new(w.rng.next());
        let arrive = r.range(0, 1000);
        match r.range(0, 3) {
            0 => {
                // Upstream caller already traces and sampled: we must continue ITS trace under ITS parent id.
                let tp = TraceParent {
                    version: 0,
                    trace_id: r.nonzero_u128(),
                    parent_id: r.nonzero_u64(),
                    flags: 1,
                };
                external_ids.push((tp.trace_id, tp.parent_id));
                w.handle(
                    "frontend",
                    Some(&tp.to_string()),
                    Some("upstream=abc"),
                    arrive,
                );
                external += 1;
            }
            1 => {
                // Garbage header: we must start a fresh trace and not crash.
                w.handle("frontend", Some("00-NOT-A-TRACE-01"), None, arrive);
                garbage += 1;
            }
            _ => {
                w.handle("frontend", None, None, arrive);
            }
        }
    }
    assert_eq!(w.restarts, garbage);

    // Group collected spans by trace id (they arrived interleaved in completion order).
    let mut by_trace: BTreeMap<u128, Vec<Span>> = BTreeMap::new();
    let mut all_ids = HashSet::new();
    for s in &w.spans {
        assert!(all_ids.insert(s.span_id), "span ids globally unique");
        assert!(
            s.span_id != 0 && s.flags & 1 == 1,
            "only sampled spans reach the collector"
        );
        by_trace.entry(s.trace_id).or_default().push(s.clone());
    }
    let sampled_traces = by_trace.len();
    unsampled += traces - sampled_traces as u32;
    println!(
        "\nsimulation: {traces} requests ({external} with upstream context, {garbage} with a garbage header) -> {sampled_traces} sampled traces, {unsampled} dropped by the sampled flag, {} spans, all ids unique",
        w.spans.len()
    );

    // The upstream-supplied trace ids survive and their root's parent is the external span id.
    for (tid, pid) in &external_ids {
        let spans = &by_trace[tid];
        let root: Vec<&Span> = spans.iter().filter(|s| s.parent == Some(*pid)).collect();
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].svc, "frontend");
    }

    let mut blame: BTreeMap<&str, u64> = BTreeMap::new();
    let mut total_ms = 0u64;
    let mut shown = false;
    for (_, spans) in by_trace {
        // The frontend span is the trace root; it appears last in completion order, never first.
        assert_eq!(spans.last().unwrap().svc, "frontend", "root completes last");
        let n = spans.len();
        let tree = build_tree(spans);
        assert_eq!(tree.spans[tree.root].svc, "frontend");
        assert!(
            (5..=9).contains(&n),
            "frontend + auth + orders + 1-2 inventory + payments + fraud = {n}"
        );
        // tracestate propagation: a callee sees its ancestors, newest first.
        for (i, s) in tree.spans.iter().enumerate() {
            let mut chain = Vec::new();
            let mut cur = Some(i);
            while let Some(c) = cur {
                chain.push(tree.spans[c].svc);
                cur = tree.spans[c]
                    .parent
                    .and_then(|p| tree.spans.iter().position(|x| x.span_id == p));
            }
            let expect: Vec<&str> = chain[1..].to_vec();
            let have: Vec<&str> = s
                .tracestate_in
                .iter()
                .map(String::as_str)
                .filter(|k| *k != "upstream")
                .collect();
            assert_eq!(have, expect, "tracestate lists ancestors newest first");
        }
        let segs = path_of(&tree);
        check_tiling(&tree, &segs);
        for &(i, a, b) in &segs {
            *blame.entry(tree.spans[i].svc).or_insert(0) += b - a;
        }
        total_ms += tree.spans[tree.root].end - tree.spans[tree.root].start;
        if !shown && n >= 7 && !tree.external_parent {
            shown = true;
            let on: HashSet<usize> = segs.iter().map(|s| s.0).collect();
            println!("  example trace ({n} spans):");
            print_tree(&tree, tree.root, 0, &on);
        }
    }
    let sum: u64 = blame.values().sum();
    assert_eq!(
        sum, total_ms,
        "per-service critical-path time adds up to total latency"
    );
    println!(
        "  critical-path blame over {sampled_traces} traces (ms of end-to-end latency attributed):"
    );
    for (svc, ms) in &blame {
        println!(
            "    {svc:10} {ms:7} ({:4.1}%)",
            *ms as f64 * 100.0 / total_ms as f64
        );
    }
    assert!(blame["frontend"] > 0 && blame["orders"] > 0 && blame["payments"] > 0);
}

fn main() {
    println!("== (a) clock synchronisation ==");
    demo_cristian_and_ntp();
    demo_berkeley();
    demo_step_vs_slew();
    println!("\n== (b) W3C Trace Context ==");
    demo_traceparent_validation();
    println!("\n== (c) spans, tree reconstruction, critical path ==");
    demo_critical_path_hand_built();
    demo_simulation();
    println!("\nall clock-sync and trace-context assertions passed");
}
