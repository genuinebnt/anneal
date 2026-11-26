use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// The only source of time.
pub trait Clock {
    fn now_ms(&self) -> u64;
}

/// One decision about one request.
pub trait Limiter {
    fn allow(&mut self) -> bool;
}

pub struct TokenBucket<C: Clock> {
    clock: C,
    /// Credit in milliseconds: one token costs `interval`, one millisecond earns 1.
    credit: u128,
    full: u128,
    interval: u128,
    latest: u64,
}

impl<C: Clock> TokenBucket<C> {
    pub fn new(clock: C, capacity: u32, refill_every_ms: u64) -> Self {
        let interval = u128::from(refill_every_ms);
        let full = u128::from(capacity) * interval;
        let latest = clock.now_ms();
        TokenBucket {
            clock,
            credit: full,
            full,
            interval,
            latest,
        }
    }

    fn refill(&mut self) {
        let now = self.clock.now_ms();
        self.credit = (self.credit + u128::from(now.saturating_sub(self.latest))).min(self.full);
        self.latest = now;
    }

    pub fn try_acquire(&mut self, n: u32) -> bool {
        self.refill();
        let cost = u128::from(n) * self.interval;
        if cost > self.credit {
            return false;
        }
        self.credit -= cost;
        true
    }

    pub fn retry_after_ms(&mut self, n: u32) -> Option<u64> {
        self.refill();
        let cost = u128::from(n) * self.interval;
        if cost > self.full {
            return None;
        }
        let missing = cost.saturating_sub(self.credit);
        // A clock that went backwards must first catch up with the latest time seen.
        let behind = if missing == 0 {
            0
        } else {
            self.latest.saturating_sub(self.clock.now_ms())
        };
        Some(u64::try_from(missing + u128::from(behind)).unwrap_or(u64::MAX))
    }
}

impl<C: Clock> Limiter for TokenBucket<C> {
    fn allow(&mut self) -> bool {
        self.try_acquire(1)
    }
}

pub struct SlidingWindow<C: Clock> {
    clock: C,
    limit: usize,
    window_ms: u64,
    latest: u64,
    admitted: VecDeque<u64>,
}

impl<C: Clock> SlidingWindow<C> {
    pub fn new(clock: C, limit: usize, window_ms: u64) -> Self {
        SlidingWindow {
            clock,
            limit,
            window_ms,
            latest: 0,
            admitted: VecDeque::new(),
        }
    }
}

impl<C: Clock> Limiter for SlidingWindow<C> {
    fn allow(&mut self) -> bool {
        self.latest = self.latest.max(self.clock.now_ms());
        let now = self.latest;
        while self
            .admitted
            .front()
            .is_some_and(|&t| now - t >= self.window_ms)
        {
            self.admitted.pop_front();
        }
        if self.admitted.len() >= self.limit {
            return false;
        }
        self.admitted.push_back(now);
        true
    }
}

pub struct PerKey<K, L, C, F> {
    clock: C,
    idle_ms: u64,
    make: F,
    latest: u64,
    /// Each key's limiter and the time it was last used.
    limiters: HashMap<K, (L, u64)>,
    /// One entry per call, oldest first. An entry is stale if the key was used again later.
    uses: VecDeque<(u64, K)>,
}

impl<K: Hash + Eq + Clone, L: Limiter, C: Clock, F: Fn() -> L> PerKey<K, L, C, F> {
    pub fn new(clock: C, idle_ms: u64, make: F) -> Self {
        PerKey {
            clock,
            idle_ms,
            make,
            latest: 0,
            limiters: HashMap::new(),
            uses: VecDeque::new(),
        }
    }

    fn forget_idle(&mut self) -> u64 {
        self.latest = self.latest.max(self.clock.now_ms());
        while let Some((at, key)) = self.uses.front() {
            if self.latest - at < self.idle_ms {
                break;
            }
            if self.limiters.get(key).is_some_and(|(_, last)| last == at) {
                self.limiters.remove(key);
            }
            self.uses.pop_front();
        }
        self.latest
    }

    pub fn allow(&mut self, key: &K) -> bool {
        let now = self.forget_idle();
        self.uses.push_back((now, key.clone()));
        let entry = self
            .limiters
            .entry(key.clone())
            .or_insert_with(|| ((self.make)(), now));
        entry.1 = now;
        entry.0.allow()
    }

    pub fn tracked(&mut self) -> usize {
        self.forget_idle();
        self.limiters.len()
    }
}
