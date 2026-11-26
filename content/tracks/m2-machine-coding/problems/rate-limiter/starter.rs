use std::hash::Hash;
use std::marker::PhantomData;

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
}

impl<C: Clock> TokenBucket<C> {
    pub fn new(clock: C, capacity: u32, refill_every_ms: u64) -> Self {
        todo!()
    }

    pub fn try_acquire(&mut self, n: u32) -> bool {
        todo!()
    }

    pub fn retry_after_ms(&mut self, n: u32) -> Option<u64> {
        todo!()
    }
}

impl<C: Clock> Limiter for TokenBucket<C> {
    fn allow(&mut self) -> bool {
        todo!()
    }
}

pub struct SlidingWindow<C: Clock> {
    clock: C,
}

impl<C: Clock> SlidingWindow<C> {
    pub fn new(clock: C, limit: usize, window_ms: u64) -> Self {
        todo!()
    }
}

impl<C: Clock> Limiter for SlidingWindow<C> {
    fn allow(&mut self) -> bool {
        todo!()
    }
}

pub struct PerKey<K, L, C, F> {
    parts: PhantomData<(K, L, C, F)>,
}

impl<K: Hash + Eq + Clone, L: Limiter, C: Clock, F: Fn() -> L> PerKey<K, L, C, F> {
    pub fn new(clock: C, idle_ms: u64, make: F) -> Self {
        todo!()
    }

    pub fn allow(&mut self, key: &K) -> bool {
        todo!()
    }

    pub fn tracked(&mut self) -> usize {
        todo!()
    }
}
