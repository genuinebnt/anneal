use solution::*;

#[test]
fn totals_of_two_periods() {
    let mut m = PaceMeter::new(1000);
    m.record(100, 5);
    m.record(900, 2);
    m.record(1000, 4);
    check!("totals at 1500", m.totals(1500), (7, 4));
}

#[test]
fn same_rate_is_level() {
    let mut m = PaceMeter::new(1000);
    m.record(0, 1000);
    m.record(1499, 500);
    check!("500 in the first 500 ms, after 1000 in 1000 ms", m.pace(1499), Pace::Level);
    check!("one ms later with nothing new", m.pace(1500), Pace::Slower);
    m.record(1500, 2);
    check!("502 in 501 ms", m.pace(1500), Pace::Faster);
}

#[test]
fn nothing_at_all_is_level() {
    let mut m = PaceMeter::new(10);
    check!("a new meter", m.pace(12345), Pace::Level);
    check!("its totals", m.totals(12345), (0, 0));
}

#[test]
fn a_skipped_period_is_empty() {
    let mut m = PaceMeter::new(100);
    m.record(50, 9);
    check!("one period later", m.totals(150), (9, 0));
    check!("two periods later", m.totals(250), (0, 0));
    m.record(260, 3);
    check!("after a long gap", m.totals(10_000), (0, 0));
}

#[test]
fn periods_are_aligned_to_zero() {
    let mut m = PaceMeter::new(1000);
    m.record(1999, 4);
    check!("the next millisecond starts a new period", m.totals(2000), (4, 0));
}

#[test]
fn an_earlier_time_counts_as_the_latest() {
    let mut m = PaceMeter::new(100);
    m.record(250, 1);
    m.record(30, 2);
    check!("both are in the period of 250", m.totals(0), (0, 3));
}
