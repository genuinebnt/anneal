//! RECIPE 15 (rate-limiting): token-bucket rate limiter with an injectable clock.
//! LEVEL: intermediate
//! Use in: rate-limiting track, API stage.   Run: cargo run --example token_bucket
//! COMMON USES: API gateways, per-tenant quotas, login throttling, outbound call budgets.
//!
//! Pattern: tokens refill continuously up to `capacity`; each request spends one. Bursts up to `capacity`
//! are allowed, the long-run rate is `refill_per_sec`. Take time from a `Clock` trait so tests control it
//! (no sleeping, no flakiness). A clock that steps back earns nothing. The rate must be a finite number >= 0 (`f64::min`
//! ignores a NaN, so a NaN or infinite rate would keep the bucket full: no limit at all).
//! LIMITS: floating-point tokens round, so a request that lands exactly when a token is due may have to wait a moment longer.
//! One caller at a time (`&mut self`): share it behind a lock.
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
        assert!(
            refill_per_sec.is_finite() && refill_per_sec >= 0.0,
            "the rate must be a finite number >= 0"
        );
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
        let elapsed = now.saturating_sub(self.last_ms) as f64 / 1000.0;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_ms = self.last_ms.max(now); // never back: that time was already paid for
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

    clock.0.set(30_000); // the clock steps back
    assert!(!bucket.try_acquire(), "no tokens for going back");
    clock.0.set(60_000);
    assert!(
        !bucket.try_acquire(),
        "nor for covering the same time again"
    );
    clock.0.set(61_000);
    assert!(bucket.try_acquire() && !bucket.try_acquire());

    for rate in [f64::NAN, f64::INFINITY, -1.0] {
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let made =
            std::panic::catch_unwind(|| drop(TokenBucket::new(FakeClock(Cell::new(0)), 1, rate)));
        std::panic::set_hook(hook);
        assert!(made.is_err(), "rate {rate} was accepted");
    }
}
