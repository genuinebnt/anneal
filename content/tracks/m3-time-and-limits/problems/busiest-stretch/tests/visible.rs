use solution::*;

#[test]
fn a_burst_across_a_round_number() {
    let log = [(100, 1), (950, 4), (990, 4), (1010, 4), (1900, 1)];
    check!("span 100", busiest(&log, 100), Some(Stretch { start_ms: 950, total: 12, events: 3 }));
}

#[test]
fn exactly_one_span_apart_is_outside() {
    let log = [(0, 5), (10, 5)];
    check!("span 10", busiest(&log, 10), Some(Stretch { start_ms: 0, total: 5, events: 1 }));
    check!("span 11", busiest(&log, 11), Some(Stretch { start_ms: 0, total: 10, events: 2 }));
}

#[test]
fn an_empty_log() {
    check!("busiest", busiest(&[], 5), None);
    check!("first_over", first_over(&[], 5, 0), None);
}

#[test]
fn ties_go_to_the_earliest() {
    let log = [(0, 3), (100, 1), (101, 2)];
    check!("span 5", busiest(&log, 5), Some(Stretch { start_ms: 0, total: 3, events: 1 }));
}

#[test]
fn the_first_event_over_budget() {
    let log = [(0, 4), (3, 4), (9, 4), (10, 4), (12, 4)];
    check!("budget 8", first_over(&log, 10, 8), Some(2));
    check!("budget 12", first_over(&log, 10, 12), Some(4));
    check!("budget 16", first_over(&log, 10, 16), None);
}

#[test]
fn events_at_the_same_time() {
    let log = [(7, 1), (7, 1), (7, 1)];
    check!("span 1", busiest(&log, 1), Some(Stretch { start_ms: 7, total: 3, events: 3 }));
    check!("the second one breaks a budget of 1", first_over(&log, 1, 1), Some(1));
}
