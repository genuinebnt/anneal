pub struct Pacer {
    max_wait: u64,
    /// The latest time seen: earlier times count as this.
    latest: u64,
    /// When the line is next free. Wider than a time, because it may lie past the last one.
    free_at: u128,
}

impl Pacer {
    pub fn new(max_wait_ms: u64) -> Self {
        Pacer {
            max_wait: max_wait_ms,
            latest: 0,
            free_at: 0,
        }
    }

    fn advance(&mut self, now_ms: u64) -> u128 {
        self.latest = self.latest.max(now_ms);
        u128::from(self.latest)
    }

    pub fn book(&mut self, now_ms: u64, cost_ms: u64) -> Result<u64, u64> {
        let now = self.advance(now_ms);
        let start = self.free_at.max(now);
        let Ok(send_at) = u64::try_from(start) else {
            return Err(u64::MAX);
        };
        let wait = start - now;
        if wait > u128::from(self.max_wait) {
            // fits: it is less than `start`, which does
            return Err((wait - u128::from(self.max_wait)) as u64);
        }
        self.free_at = start + u128::from(cost_ms);
        Ok(send_at)
    }

    pub fn pause_until(&mut self, now_ms: u64, until_ms: u64) {
        self.advance(now_ms);
        self.free_at = u128::from(until_ms);
    }

    pub fn backlog_ms(&mut self, now_ms: u64) -> u64 {
        let now = self.advance(now_ms);
        u64::try_from(self.free_at.saturating_sub(now)).unwrap_or(u64::MAX)
    }
}
