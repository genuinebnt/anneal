#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stretch {
    /// The time of the first event in the stretch.
    pub start_ms: u64,
    /// The sum of the sizes in it.
    pub total: u128,
    /// How many events it holds.
    pub events: usize,
}

/// Calls `see(first, last, total)` for every event `last`, where `first..=last` are the events less than `span_ms`
/// older than it (and not after it in the log), and `total` is the sum of their sizes.
fn walk(log: &[(u64, u64)], span_ms: u64, mut see: impl FnMut(usize, usize, u128)) {
    let (mut first, mut total) = (0, 0u128);
    for (last, &(now, size)) in log.iter().enumerate() {
        total += u128::from(size);
        while log[first].0.saturating_add(span_ms) <= now {
            total -= u128::from(log[first].1);
            first += 1;
        }
        see(first, last, total);
    }
}

pub fn busiest(log: &[(u64, u64)], span_ms: u64) -> Option<Stretch> {
    let mut best: Option<Stretch> = None;
    walk(log, span_ms, |first, last, total| {
        let here = Stretch {
            start_ms: log[first].0,
            total,
            events: last - first + 1,
        };
        match &mut best {
            Some(b) if total < b.total => {}
            // A later event with the same total only counts if it extends the same stretch (a size of zero).
            Some(b) if total == b.total => {
                if here.start_ms == b.start_ms {
                    b.events = here.events;
                }
            }
            _ => best = Some(here),
        }
    });
    best
}

pub fn first_over(log: &[(u64, u64)], span_ms: u64, budget: u64) -> Option<usize> {
    let mut found = None;
    walk(log, span_ms, |_, last, total| {
        if total > u128::from(budget) && found.is_none() {
            found = Some(last);
        }
    });
    found
}
