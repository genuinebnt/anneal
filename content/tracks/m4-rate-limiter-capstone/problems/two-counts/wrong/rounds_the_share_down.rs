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
        FixedWindow {
            limit,
            window: window_ms,
            latest: 0,
            index: 0,
            count: 0,
        }
    }
}

impl Limiter for FixedWindow {
    fn check(&mut self, now_ms: u64) -> bool {
        self.latest = self.latest.max(now_ms);
        let index = self.latest / self.window;
        if index != self.index {
            self.index = index;
            self.count = 0;
        }
        if self.count >= self.limit {
            return false;
        }
        self.count += 1;
        true
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

/// The most of these ascending times that lie in one half-open stretch of `span_ms`.
fn most_in_a_stretch(times: &[u64], span_ms: u64) -> usize {
    let (mut first, mut most) = (0, 0);
    for (last, &t) in times.iter().enumerate() {
        while t - times[first] >= span_ms {
            first += 1;
        }
        most = most.max(last - first + 1);
    }
    most
}

pub fn replay<L: Limiter + ?Sized>(limiter: &mut L, arrivals: &[u64], span_ms: u64) -> Report {
    let mut latest = 0;
    let mut decisions = Vec::with_capacity(arrivals.len());
    let mut admitted = Vec::new();
    for &at in arrivals {
        latest = at.max(latest);
        let ok = limiter.check(latest);
        decisions.push(ok);
        if ok {
            admitted.push(latest);
        }
    }
    Report {
        allowed: admitted.len(),
        denied: decisions.len() - admitted.len(),
        busiest: most_in_a_stretch(&admitted, span_ms),
        decisions,
    }
}

/// An estimate of the last `window` milliseconds from two counts: this window's and the one before.
#[derive(Debug, Clone)]
pub struct SlidingCounter {
    limit: u32,
    window: u64,
    latest: u64,
    index: u64,
    previous: u32,
    current: u32,
}

impl SlidingCounter {
    /// `window_ms` is at least 1.
    pub fn new(limit: u32, window_ms: u64) -> Self {
        SlidingCounter {
            limit,
            window: window_ms,
            latest: 0,
            index: 0,
            previous: 0,
            current: 0,
        }
    }

    fn advance(&mut self, now_ms: u64) {
        self.latest = self.latest.max(now_ms);
        let index = self.latest / self.window;
        if index != self.index {
            self.previous = if index - self.index == 1 {
                self.current
            } else {
                0
            };
            self.current = 0;
            self.index = index;
        }
    }

    /// Is `previous * (window - offset) / window + current + 1 <= limit`, without dividing?
    fn fits(&self, previous: u32, current: u32, offset: u128) -> bool {
        let window = u128::from(self.window);
        u128::from(previous) * (window - offset) / window + u128::from(current) + 1
            <= u128::from(self.limit)
    }

    /// The first offset into a window with these counts at which a request fits.
    fn first_offset(&self, previous: u32, current: u32) -> Option<u128> {
        let window = u128::from(self.window);
        if current >= self.limit {
            return None;
        }
        if previous == 0 {
            return Some(0);
        }
        // the largest share of the previous window that may still be in view
        let room = u128::from(self.limit - current - 1) * window / u128::from(previous);
        (room >= 1).then(|| window - room.min(window))
    }

    pub fn retry_after_ms(&mut self, now_ms: u64) -> Option<u64> {
        self.advance(now_ms);
        if self.limit == 0 {
            return None;
        }
        let window = u128::from(self.window);
        let now = u128::from(self.latest);
        let offset = now % window;
        if self.fits(self.previous, self.current, offset) {
            return Some(0);
        }
        let start = now - offset;
        // later in this window, else in the next one, else once both counts are gone
        let at = match self.first_offset(self.previous, self.current) {
            Some(o) => start + o,
            None => match self.first_offset(self.current, 0) {
                Some(o) => start + window + o,
                None => start + 2 * window,
            },
        };
        u64::try_from(at).ok().map(|at| at - self.latest)
    }
}

impl Limiter for SlidingCounter {
    fn check(&mut self, now_ms: u64) -> bool {
        self.advance(now_ms);
        let offset = u128::from(self.latest % self.window);
        let ok = self.fits(self.previous, self.current, offset);
        if ok {
            self.current += 1;
        }
        ok
    }
}
