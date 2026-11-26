use solution::*;

fn model_busiest(log: &[(u64, u64)], span: u64) -> Option<Stretch> {
    let mut best: Option<Stretch> = None;
    for &(start, _) in log {
        let inside: Vec<u64> = log.iter().filter(|e| e.0 >= start && e.0 - start < span).map(|e| e.1).collect();
        let total: u128 = inside.iter().map(|&s| u128::from(s)).sum();
        if best.is_none_or(|b| total > b.total) {
            best = Some(Stretch { start_ms: start, total, events: inside.len() });
        }
    }
    best
}

fn model_first_over(log: &[(u64, u64)], span: u64, budget: u64) -> Option<usize> {
    (0..log.len()).find(|&i| {
        let now = log[i].0;
        let total: u128 = log[..=i].iter().filter(|e| now - e.0 < span).map(|e| u128::from(e.1)).sum();
        total > u128::from(budget)
    })
}

#[test]
fn many_old_events_leave_at_once() {
    let log = [(0, 1), (1, 1), (2, 1), (100, 10), (101, 1)];
    check!("span 3", busiest(&log, 3), Some(Stretch { start_ms: 100, total: 11, events: 2 }));
    check!("over 10 only at the last", first_over(&log, 3, 10), Some(4));
}

#[test]
fn sizes_of_zero_belong_to_their_stretch() {
    let log = [(0, 0), (1, 5), (2, 0), (50, 5)];
    check!("span 10", busiest(&log, 10), Some(Stretch { start_ms: 0, total: 5, events: 3 }));
    check!("only zeros", busiest(&[(4, 0), (4, 0)], 1), Some(Stretch { start_ms: 4, total: 0, events: 2 }));
}

#[test]
fn the_largest_times_and_sizes() {
    let m = u64::MAX;
    let log = [(m - 5, m), (m - 3, m), (m, 7)];
    check!("span 3", busiest(&log, 3), Some(Stretch { start_ms: m - 5, total: 2 * u128::from(m), events: 2 }));
    check!("a span as long as time", busiest(&log, m), Some(Stretch { start_ms: m - 5, total: 2 * u128::from(m) + 7, events: 3 }));
    check!("budget u64::MAX", first_over(&log, 3, m), Some(1));
}

#[test]
fn one_event_over_budget_by_itself() {
    check!("one large event", first_over(&[(3, 1), (500, 9)], 1, 8), Some(1));
    check!("equal to the budget is allowed", first_over(&[(3, 8)], 1, 8), None);
}

#[test]
fn a_span_of_one_millisecond() {
    let log = [(1, 2), (2, 2), (2, 3), (3, 4)];
    check!("busiest", busiest(&log, 1), Some(Stretch { start_ms: 2, total: 5, events: 2 }));
    check!("budget 4", first_over(&log, 1, 4), Some(2));
}

#[test]
fn the_best_stretch_comes_after_a_long_quiet() {
    let log = [(0, 9), (1_000_000, 5), (1_000_001, 5)];
    check!("busiest", busiest(&log, 2), Some(Stretch { start_ms: 1_000_000, total: 10, events: 2 }));
    check!("budget 9", first_over(&log, 2, 9), Some(2));
}

#[test]
fn matches_trying_every_start() {
    let mut rng = anneal_prelude::Rng::new(9102);
    for _ in 0..600 {
        let n = rng.below(14);
        let mut now = 0u64;
        let log: Vec<(u64, u64)> = (0..n)
            .map(|_| {
                now += if rng.bool() { 0 } else { rng.below(8) as u64 };
                (now, rng.below(5) as u64)
            })
            .collect();
        let span = 1 + rng.below(12) as u64;
        let budget = rng.below(14) as u64;
        check!(format!("busiest({log:?}, {span})"), busiest(&log, span), model_busiest(&log, span));
        check!(format!("first_over({log:?}, {span}, {budget})"), first_over(&log, span, budget), model_first_over(&log, span, budget));
    }
}

#[test]
fn a_long_log_with_a_wide_span() {
    // Every stretch is large: looking back through it for each event is far too slow.
    let log: Vec<(u64, u64)> = (0..200_000u64).map(|i| (i, 1)).collect();
    check!("busiest", busiest(&log, 150_000), Some(Stretch { start_ms: 0, total: 150_000, events: 150_000 }));
    check!("first_over", first_over(&log, 150_000, 149_999), Some(149_999));
}
