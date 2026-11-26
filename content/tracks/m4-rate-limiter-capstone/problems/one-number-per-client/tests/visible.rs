use solution::*;

fn all(burst: u32, interval: u64) -> [(&'static str, Box<dyn Meter>); 3] {
    [("TokenBucket", Box::new(TokenBucket::new(burst, interval))), ("LeakyMeter", Box::new(LeakyMeter::new(burst, interval))), ("Gcra", Box::new(Gcra::new(burst, interval)))]
}

#[test]
fn a_burst_then_the_steady_rate() {
    for (name, mut m) in all(3, 100) {
        check!(name, [m.take(0, 1), m.take(0, 1), m.take(0, 1), m.take(0, 1)], [Ok(()), Ok(()), Ok(()), Err(Some(100))]);
        check!(name, [m.take(99, 1), m.take(100, 1), m.take(100, 1)], [Err(Some(1)), Ok(()), Err(Some(100))]);
    }
}

#[test]
fn requests_that_cost_several_units() {
    for (name, mut m) in all(5, 10) {
        check!(name, [m.take(1000, 3), m.take(1000, 3), m.take(1010, 3), m.take(1010, 6)], [Ok(()), Err(Some(10)), Ok(()), Err(None)]);
    }
}

#[test]
fn idle_time_is_not_stored_up() {
    for (name, mut m) in all(2, 10) {
        check!(name, [m.take(1_000_000, 2), m.take(1_000_000, 1)], [Ok(()), Err(Some(10))]);
    }
}

#[test]
fn a_burst_of_zero() {
    for (name, mut m) in all(0, 5) {
        check!(name, [m.take(3, 0), m.take(3, 1)], [Ok(()), Err(None)]);
    }
}

#[test]
fn saved_and_restored() {
    let mut g = Gcra::new(2, 50);
    g.take(10, 2).unwrap();
    let mut h = Gcra::restore(2, 50, g.save());
    check!("after restoring", (h.take(10, 1), h.take(60, 1)), (Err(Some(50)), Ok(())));
}

#[test]
fn through_the_laboratory() {
    let arrivals = [0, 0, 0, 0, 100, 150, 200];
    let want = replay(&mut TokenBucket::new(3, 100), &arrivals, 1000);
    check!("LeakyMeter", replay(&mut LeakyMeter::new(3, 100), &arrivals, 1000), want.clone());
    check!("Gcra", replay(&mut Gcra::new(3, 100), &arrivals, 1000), want);
}
