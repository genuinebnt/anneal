use solution::*;

#[test]
fn reading_also_moves_time_forward() {
    let mut m = PaceMeter::new(100);
    m.record(10, 5);
    let _ = m.pace(350);
    m.record(20, 7);
    check!("the second record counts at 350", m.totals(350), (0, 7));
}

#[test]
fn rates_are_exact_at_large_values() {
    // previous / period and current / observed differ by one part in 2^53 or less
    let period = 1u64 << 40;
    let mut m = PaceMeter::new(period);
    m.record(0, u64::MAX - 1);
    m.record(2 * period - 1, u64::MAX - 1);
    check!("equal totals over equal lengths", m.pace(2 * period - 1), Pace::Level);
    m.record(2 * period - 1, 1);
    check!("one unit more", m.pace(2 * period - 1), Pace::Faster);
    let mut s = PaceMeter::new(period);
    s.record(0, u64::MAX);
    s.record(2 * period - 1, u64::MAX - 1);
    check!("one unit fewer", s.pace(2 * period - 1), Pace::Slower);
}

#[test]
fn totals_stop_at_the_largest_number() {
    let mut m = PaceMeter::new(10);
    m.record(1, u64::MAX);
    m.record(2, 5);
    check!("saturates", m.totals(3), (0, u64::MAX));
}

#[test]
fn the_largest_times() {
    let mut m = PaceMeter::new(u64::MAX);
    m.record(0, 3);
    check!("the first period", m.totals(u64::MAX - 1), (0, 3));
    check!("the second period has only the last millisecond", m.totals(u64::MAX), (3, 0));
    let mut one = PaceMeter::new(1);
    one.record(u64::MAX - 1, 8);
    one.record(u64::MAX, 9);
    check!("periods of one millisecond", one.totals(u64::MAX), (8, 9));
}

#[test]
fn the_first_millisecond_of_a_period() {
    let mut m = PaceMeter::new(1000);
    m.record(10, 1000);
    m.record(1000, 1);
    check!("1 in 1 ms against 1000 in 1000 ms", m.pace(1000), Pace::Level);
}

#[test]
fn only_the_directly_previous_period_counts() {
    let mut m = PaceMeter::new(10);
    m.record(5, 100);
    m.record(25, 1);
    check!("period 1 was empty", m.totals(25), (0, 1));
    check!("so anything is faster", m.pace(25), Pace::Faster);
}

#[test]
fn recording_nothing_still_moves_time() {
    let mut m = PaceMeter::new(10);
    m.record(1, 4);
    m.record(15, 0);
    check!("rolled over", m.totals(15), (4, 0));
    check!("slower", m.pace(15), Pace::Slower);
}

#[test]
fn matches_a_log_of_every_record() {
    let mut rng = anneal_prelude::Rng::new(9101);
    for _ in 0..400 {
        let period = 1 + rng.below(12) as u64;
        let mut m = PaceMeter::new(period);
        let mut log: Vec<(u64, u64)> = Vec::new();
        let mut latest = 0u64;
        let mut now = 0u64;
        let mut calls = format!("period {period}");
        for _ in 0..40 {
            now = if rng.below(8) == 0 { now.saturating_sub(rng.below(20) as u64) } else { now + rng.below(9) as u64 };
            latest = latest.max(now);
            let units = rng.below(6) as u64;
            let writing = rng.bool();
            if writing {
                calls += &format!(", record({now}, {units})");
                m.record(now, units);
                log.push((latest, units));
            } else {
                let index = latest / period;
                let sum = |i: u64| log.iter().filter(|&&(t, _)| t / period == i).map(|&(_, u)| u).sum::<u64>();
                let current = sum(index);
                let previous = if index == 0 { 0 } else { sum(index - 1) };
                let observed = latest % period + 1;
                let want = match (current * period).cmp(&(previous * observed)) {
                    std::cmp::Ordering::Less => Pace::Slower,
                    std::cmp::Ordering::Equal => Pace::Level,
                    std::cmp::Ordering::Greater => Pace::Faster,
                };
                calls += &format!(", totals/pace({now})");
                check!(&calls, m.totals(now), (previous, current));
                check!(&calls, m.pace(now), want);
            }
        }
    }
}
