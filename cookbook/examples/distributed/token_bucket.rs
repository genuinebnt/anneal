//! RECIPE 15 (rate-limiting): token-bucket rate limiter with an injectable clock.
//! LEVEL: intermediate
//! Use in: rate-limiting track, API stage.   Run: cargo run --example token_bucket
//! COMMON USES: API gateways, per-tenant quotas, login throttling, outbound call budgets.
//!
//! Pattern: tokens refill continuously up to `capacity`; each request spends one. Bursts up to `capacity`
//! are allowed, the long-run rate is `refill_per_sec`. Take time from a `Clock` trait so tests control it
//! (no sleeping, no flakiness).
//! Compare: https://github.com/boinkor-net/governor and https://docs.rs/tower/latest/tower/limit/index.html
#![allow(dead_code)]
use std::cell::Cell;

trait Clock {
    fn now_ms(&self) -> u64;
}
struct FakeClock(Cell<u64>);
impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.get()
    }
}
impl<C: Clock> Clock for &C {
    fn now_ms(&self) -> u64 {
        (**self).now_ms()
    }
}

struct TokenBucket<C: Clock> {
    clock: C,
    capacity: f64,
    refill_per_sec: f64,
    tokens: f64,
    last_ms: u64,
}

impl<C: Clock> TokenBucket<C> {
    fn new(clock: C, capacity: u32, refill_per_sec: f64) -> Self {
        let now = clock.now_ms();
        TokenBucket {
            clock,
            capacity: capacity as f64,
            refill_per_sec,
            tokens: capacity as f64,
            last_ms: now,
        }
    }
    fn try_acquire(&mut self) -> bool {
        let now = self.clock.now_ms();
        let elapsed = (now - self.last_ms) as f64 / 1000.0;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_ms = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

fn main() {
    let clock = FakeClock(Cell::new(0));
    let mut bucket = TokenBucket::new(&clock, 3, 1.0);

    let burst: Vec<bool> = (0..5).map(|_| bucket.try_acquire()).collect();
    println!("burst of 5: {burst:?}");
    assert_eq!(burst, [true, true, true, false, false]);

    clock.0.set(1_000);
    assert!(bucket.try_acquire(), "one token refilled after 1s");
    assert!(!bucket.try_acquire());

    clock.0.set(60_000);
    let after_idle: usize = (0..10).filter(|_| bucket.try_acquire()).count();
    println!("after a long idle only {after_idle} pass (capped at capacity)");
    assert_eq!(after_idle, 3);
}
