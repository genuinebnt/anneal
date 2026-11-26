use solution::*;

#[test]
fn a_refusal_changes_nothing() {
    let mut g = Gcra::new(2, 100);
    g.take(0, 2).unwrap();
    for t in 0..99 {
        check!("refused", g.take(t, 1).is_err(), true);
    }
    check!("on time", g.take(100, 1), Ok(()));
    let mut l = LeakyMeter::new(2, 100);
    l.take(0, 2).unwrap();
    for t in 0..100 {
        let _ = l.take(t, 2);
    }
    check!("meter on time", l.take(100, 1), Ok(()));
}

#[test]
fn the_meter_ignores_a_clock_that_goes_back() {
    let mut l = LeakyMeter::new(1, 100);
    l.take(1000, 1).unwrap();
    check!("in the past", l.take(0, 1), Err(Some(100)));
    check!("back again: nothing was earned", l.take(1000, 1), Err(Some(100)));
    check!("later", l.take(1100, 1), Ok(()));
}

#[test]
fn the_single_number_never_pays_twice() {
    let mut g = Gcra::new(1, 100);
    g.take(1000, 1).unwrap();
    check!("in the past", g.take(0, 1).is_err(), true);
    check!("back again", g.take(1099, 1), Err(Some(1)));
}

#[test]
fn extreme_values() {
    let m = u64::MAX;
    let mut all: [Box<dyn Meter>; 3] = [Box::new(TokenBucket::new(u32::MAX, m)), Box::new(LeakyMeter::new(u32::MAX, m)), Box::new(Gcra::new(u32::MAX, m))];
    for meter in &mut all {
        check!("all at once", meter.take(0, u32::MAX), Ok(()));
        check!("a wait that does not fit", meter.take(0, 2), Err(Some(m)));
        check!("a wait that just fits", meter.take(0, 1), Err(Some(m)));
        check!("at the end of time", meter.take(m, 1), Ok(()));
        check!("and no more", meter.take(m, 1), Err(Some(m)));
    }
}

#[test]
fn asking_for_nothing_does_nothing() {
    let mut g = Gcra::new(2, 10);
    check!("zero", g.take(500, 0), Ok(()));
    check!("the whole burst is still there", g.take(500, 2), Ok(()));
}

#[test]
fn a_restored_limiter_near_the_largest_time() {
    let mut g = Gcra::new(3, u64::MAX);
    g.take(u64::MAX, 3).unwrap();
    let saved = g.save();
    let mut h = Gcra::restore(3, u64::MAX, saved);
    check!("nothing left", h.take(u64::MAX, 1), Err(Some(u64::MAX)));
}

#[test]
fn all_three_agree_on_random_traffic() {
    let mut rng = anneal_prelude::Rng::new(9203);
    for _ in 0..800 {
        let burst = rng.below(6) as u32;
        let interval = 1 + rng.below(25) as u64;
        let mut bucket = TokenBucket::new(burst, interval);
        let mut leaky = LeakyMeter::new(burst, interval);
        let mut gcra = Gcra::new(burst, interval);
        let mut now = rng.below(100) as u64;
        let mut calls = format!("burst {burst}, interval {interval}: (time, units)");
        for _ in 0..50 {
            now += if rng.below(4) == 0 { rng.below(150) as u64 } else { rng.below(10) as u64 };
            let n = if rng.bool() { 1 } else { rng.below(7) as u32 };
            calls += &format!(" ({now}, {n})");
            let want = bucket.take(now, n);
            check!(format!("LeakyMeter {calls}"), leaky.take(now, n), want);
            if rng.below(4) == 0 {
                gcra = Gcra::restore(burst, interval, gcra.save());
            }
            check!(format!("Gcra {calls}"), gcra.take(now, n), want);
        }
    }
}

#[test]
fn the_meter_agrees_when_the_clock_jumps_about() {
    let mut rng = anneal_prelude::Rng::new(9204);
    for _ in 0..400 {
        let burst = rng.below(5) as u32;
        let interval = 1 + rng.below(15) as u64;
        let (mut bucket, mut leaky) = (TokenBucket::new(burst, interval), LeakyMeter::new(burst, interval));
        let mut calls = format!("burst {burst}, interval {interval}:");
        for _ in 0..40 {
            let (now, n) = (rng.below(200) as u64, rng.below(5) as u32);
            calls += &format!(" ({now}, {n})");
            check!(&calls, leaky.take(now, n), bucket.take(now, n));
        }
    }
}
