use solution::*;

#[test]
fn a_fixed_window_counts_per_window() {
    let mut w = FixedWindow::new(2, 100);
    check!("window 0", [w.check(0), w.check(50), w.check(99)], [true, true, false]);
    check!("window 1", [w.check(100), w.check(100), w.check(150)], [true, true, false]);
}

#[test]
fn the_edge_lets_through_twice_the_limit() {
    let arrivals = [999, 999, 999, 1000, 1000, 1000];
    let r = replay(&mut FixedWindow::new(3, 1000), &arrivals, 1000);
    check!("(allowed, busiest)", (r.allowed, r.busiest), (6, 6));
    let r = replay(&mut SlidingLog::new(3, 1000), &arrivals, 1000);
    check!("sliding log (allowed, busiest)", (r.allowed, r.busiest), (3, 3));
}

#[test]
fn a_report() {
    let r = replay(&mut FixedWindow::new(1, 10), &[1, 2, 11, 12], 10);
    check!("report", r, Report { decisions: vec![true, false, true, false], allowed: 2, denied: 2, busiest: 1 });
}

#[test]
fn no_arrivals() {
    let r = replay(&mut FixedWindow::new(1, 10), &[], 10);
    check!("report", r, Report { decisions: vec![], allowed: 0, denied: 0, busiest: 0 });
}

#[test]
fn through_a_trait_object() {
    let mut limiter: Box<dyn Limiter> = Box::new(FixedWindow::new(1, 5));
    let r = replay(limiter.as_mut(), &[0, 0, 5], 100);
    check!("decisions", r.decisions, vec![true, false, true]);
}

#[test]
fn an_earlier_arrival_counts_as_the_latest() {
    let r = replay(&mut FixedWindow::new(2, 100), &[150, 20, 160], 1);
    check!("20 is treated as 150", (r.decisions, r.busiest), (vec![true, true, false], 2));
}
