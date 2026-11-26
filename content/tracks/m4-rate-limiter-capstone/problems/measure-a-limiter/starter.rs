use std::collections::VecDeque;

/// Decides one request at a time. The time is passed in: no limiter ever reads a clock.
pub trait Limiter {
    fn check(&mut self, now_ms: u64) -> bool;
}

/// At most `limit` admissions in each aligned window `[k * window, (k + 1) * window)`.
#[derive(Debug, Clone)]
pub struct FixedWindow {
    limit: u32,
    window: u64,
    latest: u64,
    index: u64,
    count: u32,
}

impl FixedWindow {
    /// `window_ms` is at least 1.
    pub fn new(limit: u32, window_ms: u64) -> Self {
        todo!()
    }
}

impl Limiter for FixedWindow {
    fn check(&mut self, now_ms: u64) -> bool {
        todo!()
    }
}

/// At most `limit` admissions in any `window` milliseconds. (**Given**: you built one in the preparation.)
#[derive(Debug, Clone)]
pub struct SlidingLog {
    limit: u32,
    window: u64,
    latest: u64,
    admitted: VecDeque<u64>,
}

impl SlidingLog {
    /// `window_ms` is at least 1.
    pub fn new(limit: u32, window_ms: u64) -> Self {
        SlidingLog {
            limit,
            window: window_ms,
            latest: 0,
            admitted: VecDeque::new(),
        }
    }
}

impl Limiter for SlidingLog {
    fn check(&mut self, now_ms: u64) -> bool {
        self.latest = self.latest.max(now_ms);
        while self
            .admitted
            .front()
            .is_some_and(|&t| self.latest - t >= self.window)
        {
            self.admitted.pop_front();
        }
        if self.admitted.len() >= self.limit as usize {
            return false;
        }
        self.admitted.push_back(self.latest);
        true
    }
}

/// What one limiter did with a list of arrivals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// One answer per arrival, in order.
    pub decisions: Vec<bool>,
    pub allowed: usize,
    pub denied: usize,
    /// The most admitted requests found in any stretch of `span_ms`.
    pub busiest: usize,
}

/// Offers every arrival to `limiter`, in order, and reports what happened. `span_ms` is at least 1.
pub fn replay<L: Limiter + ?Sized>(limiter: &mut L, arrivals: &[u64], span_ms: u64) -> Report {
    todo!()
}
