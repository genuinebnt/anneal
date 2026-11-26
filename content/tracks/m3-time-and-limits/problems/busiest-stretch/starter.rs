#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stretch {
    /// The time of the first event in the stretch.
    pub start_ms: u64,
    /// The sum of the sizes in it.
    pub total: u128,
    /// How many events it holds.
    pub events: usize,
}

/// `log` holds `(time_ms, size)` pairs in non-decreasing order of time. `span_ms` is at least 1.
pub fn busiest(log: &[(u64, u64)], span_ms: u64) -> Option<Stretch> {
    todo!()
}

/// The index of the first event that makes some stretch of `span_ms` hold more than `budget`.
pub fn first_over(log: &[(u64, u64)], span_ms: u64, budget: u64) -> Option<usize> {
    todo!()
}
