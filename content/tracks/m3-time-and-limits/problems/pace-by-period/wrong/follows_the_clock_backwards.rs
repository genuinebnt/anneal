#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Slower,
    Level,
    Faster,
}

pub struct PaceMeter {
    period: u64,
    /// The latest time seen: earlier times count as this.
    latest: u64,
    /// Which period `latest` is in.
    index: u64,
    previous: u64,
    current: u64,
}

impl PaceMeter {
    pub fn new(period_ms: u64) -> Self {
        PaceMeter {
            period: period_ms,
            latest: 0,
            index: 0,
            previous: 0,
            current: 0,
        }
    }

    /// Moves to `now_ms` (never backwards) and returns the time to use.
    fn advance(&mut self, now_ms: u64) -> u64 {
        self.latest = now_ms;
        let index = self.latest / self.period;
        if index != self.index {
            self.previous = if index.wrapping_sub(self.index) == 1 {
                self.current
            } else {
                0
            };
            self.current = 0;
            self.index = index;
        }
        self.latest
    }

    pub fn record(&mut self, now_ms: u64, units: u64) {
        self.advance(now_ms);
        self.current = self.current.saturating_add(units);
    }

    pub fn totals(&mut self, now_ms: u64) -> (u64, u64) {
        self.advance(now_ms);
        (self.previous, self.current)
    }

    pub fn pace(&mut self, now_ms: u64) -> Pace {
        let now = self.advance(now_ms);
        let observed = u128::from(now % self.period) + 1;
        // current / observed against previous / period, without dividing
        let here = u128::from(self.current) * u128::from(self.period);
        let there = u128::from(self.previous) * observed;
        match here.cmp(&there) {
            std::cmp::Ordering::Less => Pace::Slower,
            std::cmp::Ordering::Equal => Pace::Level,
            std::cmp::Ordering::Greater => Pace::Faster,
        }
    }
}
