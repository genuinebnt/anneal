//! BLOCK 7: per-principal token-bucket rate limiting.
//!
//! A bucket holds up to `capacity` tokens and refills continuously at `refill_per_sec`. This permits
//! bursts (up to capacity) while bounding the long-run rate. Time comes from the caller (the injected
//! clock), so tests move time instead of sleeping.
//! Pitfalls: this state is per process, so N replicas give N times the limit (use a Redis script or an
//! edge limiter for a global one); idle buckets must be evicted or the map grows forever; clocks can step
//! backwards, which must never mint tokens.
//! Production alternatives: `governor` (GCRA), the gateway/proxy's limiter.

use std::collections::HashMap;
use std::sync::Mutex;

use axum::http::{HeaderMap, HeaderName, HeaderValue};

const SWEEP_AT: usize = 10_000;

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Decision {
    pub allowed: bool,
    pub limit: u32,
    pub remaining: u32,
    /// Seconds until the bucket is full again.
    pub reset_secs: u64,
    /// Seconds until one token is available. 0 when allowed.
    pub retry_after_secs: u64,
}

impl Decision {
    /// Headers in the style of the IETF `RateLimit` header drafts (the three-field form).
    pub fn apply(&self, headers: &mut HeaderMap) {
        let fields = [
            ("ratelimit-limit", u64::from(self.limit)),
            ("ratelimit-remaining", u64::from(self.remaining)),
            ("ratelimit-reset", self.reset_secs),
        ];
        for (name, v) in fields {
            headers.insert(HeaderName::from_static(name), HeaderValue::from(v));
        }
    }
}

#[derive(Debug)]
pub struct RateLimiter {
    capacity: u32,
    per_sec: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    #[must_use]
    pub fn new(capacity: u32, per_sec: f64) -> Self {
        Self { capacity, per_sec, buckets: Mutex::default() }
    }

    fn refill(&self, b: Bucket, now_ms: i64) -> f64 {
        let elapsed = (now_ms - b.at_ms).max(0) as f64 / 1000.0;
        (b.tokens + elapsed * self.per_sec).min(f64::from(self.capacity))
    }

    pub fn check(&self, key: &str, now_ms: i64) -> Decision {
        let cap = f64::from(self.capacity);
        let mut map = self.buckets.lock().unwrap();
        if map.len() >= SWEEP_AT {
            // A full bucket is indistinguishable from an absent one, so dropping it is free.
            map.retain(|_, b| self.refill(*b, now_ms) < cap);
        }
        let b = map.entry(key.to_string()).or_insert(Bucket { tokens: cap, at_ms: now_ms });
        let mut tokens = self.refill(*b, now_ms);
        let allowed = tokens >= 1.0;
        if allowed {
            tokens -= 1.0;
        }
        // `max` keeps the high-water mark: a clock that steps back and forth must not refill twice.
        *b = Bucket { tokens, at_ms: b.at_ms.max(now_ms) };
        let secs_for = |need: f64| (need.max(0.0) / self.per_sec).ceil() as u64;
        Decision {
            allowed,
            limit: self.capacity,
            remaining: tokens.floor() as u32,
            reset_secs: secs_for(cap - tokens),
            retry_after_secs: if allowed { 0 } else { secs_for(1.0 - tokens).max(1) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_then_deny_then_refill() {
        let l = RateLimiter::new(3, 1.0);
        let got: Vec<bool> = (0..4).map(|_| l.check("k", 0).allowed).collect();
        assert_eq!(got, [true, true, true, false]);
        let d = l.check("k", 500);
        assert!(!d.allowed);
        assert_eq!(d.retry_after_secs, 1);
        assert!(l.check("k", 1_000).allowed);
        assert!(!l.check("k", 1_000).allowed);
    }

    #[test]
    fn keys_are_independent_and_refill_is_capped() {
        let l = RateLimiter::new(2, 10.0);
        assert!(l.check("a", 0).allowed && l.check("a", 0).allowed && !l.check("a", 0).allowed);
        assert!(l.check("b", 0).allowed);
        assert_eq!(l.check("a", 1_000_000).remaining, 1, "never exceeds capacity");
    }

    #[test]
    fn clock_stepping_backwards_mints_nothing() {
        let l = RateLimiter::new(1, 1.0);
        assert!(l.check("k", 10_000).allowed);
        assert!(!l.check("k", 0).allowed);
        assert!(!l.check("k", 10_000).allowed, "returning to the old time is not elapsed time");
    }

    /// Invariant over random arrivals: allowed <= capacity + rate * elapsed.
    #[test]
    fn long_run_rate_is_bounded_for_random_arrivals() {
        let mut x = 0x1234_5678_u64;
        for _ in 0..50 {
            let l = RateLimiter::new(5, 4.0);
            let (mut now, mut allowed) = (0_i64, 0_u64);
            for _ in 0..500 {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                now += (x % 300) as i64;
                allowed += u64::from(l.check("k", now).allowed);
            }
            let bound = 5.0 + 4.0 * now as f64 / 1000.0;
            assert!((allowed as f64) <= bound + 1e-6, "{allowed} > {bound}");
        }
    }
}
