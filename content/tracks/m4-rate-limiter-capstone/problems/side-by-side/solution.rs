use std::collections::{BTreeMap, HashMap, VecDeque};
use std::hash::Hash;

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
        u128::from(previous) * (window - offset) + (u128::from(current) + 1) * window
            <= u128::from(self.limit) * window
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

/// Like `Limiter`, for requests that cost `n` units, and saying when to come back.
pub trait Meter {
    /// `Ok(())`: `n` units taken. `Err(Some(d))`: nothing taken; the same call `d` ms later would succeed.
    /// `Err(None)`: it never can.
    fn take(&mut self, now_ms: u64, n: u32) -> Result<(), Option<u64>>;
}

/// **Given**, from the preparation: starts with `burst` tokens and earns one every `interval_ms`, never holding more.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    burst: u32,
    interval: u128,
    /// One token is `interval` credit; one millisecond earns 1.
    credit: u128,
    latest: u64,
}

impl TokenBucket {
    /// `interval_ms` is at least 1.
    pub fn new(burst: u32, interval_ms: u64) -> Self {
        TokenBucket {
            burst,
            interval: interval_ms.into(),
            credit: u128::from(burst) * u128::from(interval_ms),
            latest: 0,
        }
    }
}

impl Meter for TokenBucket {
    fn take(&mut self, now_ms: u64, n: u32) -> Result<(), Option<u64>> {
        let now = self.latest.max(now_ms);
        let full = u128::from(self.burst) * self.interval;
        self.credit = (self.credit + u128::from(now - self.latest)).min(full);
        self.latest = now;
        if n > self.burst {
            return Err(None);
        }
        let cost = u128::from(n) * self.interval;
        if cost > self.credit {
            return Err(Some(capped(cost - self.credit)));
        }
        self.credit -= cost;
        Ok(())
    }
}

/// A wait that does not fit in a `u64` is reported as `u64::MAX`.
fn capped(wait: u128) -> u64 {
    u64::try_from(wait).unwrap_or(u64::MAX)
}

impl Limiter for TokenBucket {
    fn check(&mut self, now_ms: u64) -> bool {
        self.take(now_ms, 1).is_ok()
    }
}

/// A level that drains by 1 each millisecond. A request of `n` units adds `n * interval_ms`, if that stays within
/// `burst * interval_ms`.
#[derive(Debug, Clone)]
pub struct LeakyMeter {
    burst: u32,
    interval: u128,
    level: u128,
    latest: u64,
}

impl LeakyMeter {
    /// `interval_ms` is at least 1.
    pub fn new(burst: u32, interval_ms: u64) -> Self {
        LeakyMeter {
            burst,
            interval: interval_ms.into(),
            level: 0,
            latest: 0,
        }
    }
}

impl Meter for LeakyMeter {
    fn take(&mut self, now_ms: u64, n: u32) -> Result<(), Option<u64>> {
        let now = self.latest.max(now_ms);
        self.level = self.level.saturating_sub(u128::from(now - self.latest));
        self.latest = now;
        if n > self.burst {
            return Err(None);
        }
        let after = self.level + u128::from(n) * self.interval;
        let full = u128::from(self.burst) * self.interval;
        if after > full {
            return Err(Some(capped(after - full)));
        }
        self.level = after;
        Ok(())
    }
}

impl Limiter for LeakyMeter {
    fn check(&mut self, now_ms: u64) -> bool {
        self.take(now_ms, 1).is_ok()
    }
}

/// The same promise as the token bucket, with one number of changing state.
#[derive(Debug, Clone)]
pub struct Gcra {
    burst: u32,
    interval: u128,
    /// The time at which everything admitted so far would be paid off.
    paid_off_at: u128,
}

impl Gcra {
    /// `interval_ms` is at least 1.
    pub fn new(burst: u32, interval_ms: u64) -> Self {
        Self::restore(burst, interval_ms, 0)
    }

    /// The whole changing state, as it would be kept in a shared store.
    pub fn save(&self) -> u128 {
        self.paid_off_at
    }

    pub fn restore(burst: u32, interval_ms: u64, saved: u128) -> Self {
        Gcra {
            burst,
            interval: interval_ms.into(),
            paid_off_at: saved,
        }
    }
}

impl Meter for Gcra {
    fn take(&mut self, now_ms: u64, n: u32) -> Result<(), Option<u64>> {
        if n > self.burst {
            return Err(None);
        }
        if n == 0 {
            return Ok(());
        }
        let now = u128::from(now_ms);
        let after = self.paid_off_at.max(now) + u128::from(n) * self.interval;
        let furthest = now + u128::from(self.burst) * self.interval;
        if after > furthest {
            return Err(Some(capped(after - furthest)));
        }
        self.paid_off_at = after;
        Ok(())
    }
}

impl Limiter for Gcra {
    fn check(&mut self, now_ms: u64) -> bool {
        self.take(now_ms, 1).is_ok()
    }
}

/// A limiter that can tell when it no longer remembers anything that matters.
pub trait Forget: Limiter {
    /// True when, at every time from `now_ms` on, this limiter answers exactly as a newly made one would.
    /// A time earlier than one it has seen counts as that latest time.
    fn is_fresh(&self, now_ms: u64) -> bool;
}

impl Forget for FixedWindow {
    fn is_fresh(&self, now_ms: u64) -> bool {
        self.count == 0 || self.latest.max(now_ms) / self.window != self.index
    }
}

impl Forget for SlidingLog {
    fn is_fresh(&self, now_ms: u64) -> bool {
        let now = self.latest.max(now_ms);
        self.admitted.back().is_none_or(|&t| now - t >= self.window)
    }
}

impl Forget for SlidingCounter {
    fn is_fresh(&self, now_ms: u64) -> bool {
        let index = self.latest.max(now_ms) / self.window;
        match index - self.index {
            0 => self.previous == 0 && self.current == 0,
            1 => self.current == 0,
            _ => true,
        }
    }
}

impl Forget for Gcra {
    fn is_fresh(&self, now_ms: u64) -> bool {
        self.paid_off_at <= u128::from(now_ms)
    }
}

/// One limiter per key.
pub struct Keyed<K, L, F> {
    make: F,
    limiters: HashMap<K, L>,
    latest: u64,
    /// Sweep before holding more than this many.
    sweep_at: usize,
}

impl<K: Hash + Eq + Clone, L: Forget, F: Fn() -> L> Keyed<K, L, F> {
    pub fn new(make: F) -> Self {
        Keyed {
            make,
            limiters: HashMap::new(),
            latest: 0,
            sweep_at: 64,
        }
    }

    pub fn check(&mut self, key: &K, now_ms: u64) -> bool {
        self.latest = self.latest.max(now_ms);
        if self.limiters.len() >= self.sweep_at {
            self.sweep(now_ms);
        }
        self.limiters
            .entry(key.clone())
            .or_insert_with(&self.make)
            .check(self.latest)
    }

    pub fn sweep(&mut self, now_ms: u64) -> usize {
        self.latest = self.latest.max(now_ms);
        let now = self.latest;
        let before = self.limiters.len();
        self.limiters.retain(|_, limiter| !limiter.is_fresh(now));
        // a full pass again only after as many new keys as it has just looked at
        self.sweep_at = (2 * self.limiters.len()).max(64);
        before - self.limiters.len()
    }

    pub fn len(&self) -> usize {
        self.limiters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.limiters.is_empty()
    }
}

/// A limiter to be built, by name and settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    FixedWindow { limit: u32, window_ms: u64 },
    SlidingLog { limit: u32, window_ms: u64 },
    SlidingCounter { limit: u32, window_ms: u64 },
    TokenBucket { burst: u32, interval_ms: u64 },
    LeakyMeter { burst: u32, interval_ms: u64 },
    Gcra { burst: u32, interval_ms: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// One limiter for everybody.
    Shared,
    /// A limiter for each client.
    PerClient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub at_ms: u64,
    pub client: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClientReport {
    pub allowed: usize,
    pub denied: usize,
    /// The most of this client's admitted requests in any stretch of the span.
    pub busiest: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub decisions: Vec<bool>,
    /// The most admitted requests, of all clients together, in any stretch of the span.
    pub busiest: usize,
    pub clients: BTreeMap<u32, ClientReport>,
    /// The first request this entry decided differently from the first entry of the lineup.
    pub differs_from_first_at: Option<usize>,
    /// Limiters that still matter at the time of the last request.
    pub held: usize,
}

// New in this step: the other two meters must also say when they are fresh.
impl Forget for TokenBucket {
    fn is_fresh(&self, now_ms: u64) -> bool {
        let idle = u128::from(now_ms.saturating_sub(self.latest));
        self.credit + idle >= u128::from(self.burst) * self.interval
    }
}

impl Forget for LeakyMeter {
    fn is_fresh(&self, now_ms: u64) -> bool {
        self.level <= u128::from(now_ms.saturating_sub(self.latest))
    }
}

/// So that `Keyed` and one shared limiter can hold any policy.
impl Limiter for Box<dyn Forget> {
    fn check(&mut self, now_ms: u64) -> bool {
        (**self).check(now_ms)
    }
}

impl Forget for Box<dyn Forget> {
    fn is_fresh(&self, now_ms: u64) -> bool {
        (**self).is_fresh(now_ms)
    }
}

fn build(policy: Policy) -> Box<dyn Forget> {
    match policy {
        Policy::FixedWindow { limit, window_ms } => Box::new(FixedWindow::new(limit, window_ms)),
        Policy::SlidingLog { limit, window_ms } => Box::new(SlidingLog::new(limit, window_ms)),
        Policy::SlidingCounter { limit, window_ms } => {
            Box::new(SlidingCounter::new(limit, window_ms))
        }
        Policy::TokenBucket { burst, interval_ms } => {
            Box::new(TokenBucket::new(burst, interval_ms))
        }
        Policy::LeakyMeter { burst, interval_ms } => Box::new(LeakyMeter::new(burst, interval_ms)),
        Policy::Gcra { burst, interval_ms } => Box::new(Gcra::new(burst, interval_ms)),
    }
}

fn run(policy: Policy, scope: Scope, traffic: &[Request], span_ms: u64) -> Outcome {
    // Shared is the keyed map with one key for everybody.
    let mut limiters = Keyed::new(move || build(policy));
    let mut latest = 0;
    let mut decisions = Vec::with_capacity(traffic.len());
    let mut admitted = Vec::new();
    let mut by_client: BTreeMap<u32, (ClientReport, Vec<u64>)> = BTreeMap::new();
    for request in traffic {
        latest = request.at_ms.max(latest);
        let key = match scope {
            Scope::Shared => 0,
            Scope::PerClient => request.client,
        };
        let ok = limiters.check(&key, latest);
        decisions.push(ok);
        let (report, times) = by_client.entry(request.client).or_default();
        if ok {
            admitted.push(latest);
            times.push(latest);
            report.allowed += 1;
        } else {
            report.denied += 1;
        }
    }
    limiters.sweep(latest);
    let clients = by_client
        .into_iter()
        .map(|(client, (mut report, times))| {
            report.busiest = most_in_a_stretch(&times, span_ms);
            (client, report)
        })
        .collect();
    Outcome {
        busiest: most_in_a_stretch(&admitted, span_ms),
        decisions,
        clients,
        differs_from_first_at: None,
        held: limiters.len(),
    }
}

pub fn compare(lineup: &[(Policy, Scope)], traffic: &[Request], span_ms: u64) -> Vec<Outcome> {
    let mut outcomes: Vec<Outcome> = lineup
        .iter()
        .map(|&(policy, scope)| run(policy, scope, traffic, span_ms))
        .collect();
    if let Some((first, rest)) = outcomes.split_first_mut() {
        for other in rest {
            other.differs_from_first_at = first
                .decisions
                .iter()
                .zip(&other.decisions)
                .position(|(a, b)| a != b);
        }
    }
    outcomes
}
