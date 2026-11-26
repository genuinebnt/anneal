#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Slower,
    Level,
    Faster,
}

pub struct PaceMeter {
    // your fields
}

impl PaceMeter {
    /// `period_ms` is at least 1.
    pub fn new(period_ms: u64) -> Self {
        todo!()
    }

    /// Counts `units` at time `now_ms`.
    pub fn record(&mut self, now_ms: u64, units: u64) {
        todo!()
    }

    /// `(previous, current)`: the totals of the period before the one containing `now_ms`, and of that one.
    pub fn totals(&mut self, now_ms: u64) -> (u64, u64) {
        todo!()
    }

    /// Is the current period running slower or faster, per millisecond, than the previous one did?
    pub fn pace(&mut self, now_ms: u64) -> Pace {
        todo!()
    }
}
