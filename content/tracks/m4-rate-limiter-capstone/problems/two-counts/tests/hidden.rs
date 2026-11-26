use solution::*;

/// The rule by definition, over a log of every admitted time.
#[derive(Clone)]
struct Model {
    limit: u128,
    window: u64,
    latest: u64,
    admitted: Vec<u64>,
}
impl Model {
    fn would_pass(&self, now: u64) -> bool {
        let (w, k) = (self.window, now / self.window);
        let count = |i: u64| self.admitted.iter().filter(|&&t| t / w == i).count() as u128;
        let previous = if k == 0 { 0 } else { count(k - 1) };
        previous * u128::from(w - now % w) + (count(k) + 1) * u128::from(w) <= self.limit * u128::from(w)
    }
    fn check(&mut self, now: u64) -> bool {
        self.latest = self.latest.max(now);
        let ok = self.would_pass(self.latest);
        if ok {
            self.admitted.push(self.latest);
        }
        ok
    }
}

#[test]
fn limit_zero() {
    let mut c = SlidingCounter::new(0, 10);
    check!("never", (c.check(5), c.retry_after_ms(5)), (false, None));
}

#[test]
fn waiting_when_nothing_blocks() {
    let mut c = SlidingCounter::new(1, 10);
    check!("now", c.retry_after_ms(99), Some(0));
}

#[test]
fn exact_with_huge_windows() {
    let w = 1u64 << 60;
    let mut c = SlidingCounter::new(3, w);
    for _ in 0..3 {
        check!("fill", c.check(1), true);
    }
    // one third of the previous window must have left: offset >= ceil(w / 3)
    let first = w + w.div_ceil(3);
    check!("wait", c.retry_after_ms(w), Some(first - w));
    check!("one ms early", c.check(first - 1), false);
    check!("on time", c.check(first), true);
}

#[test]
fn no_time_will_do() {
    let mut c = SlidingCounter::new(1, u64::MAX);
    check!("first", c.check(5), true);
    check!("both windows would have to pass", c.retry_after_ms(6), None);
    let mut d = SlidingCounter::new(1, 10);
    check!("near the end of time", d.check(u64::MAX - 3), true);
    check!("nothing left", d.retry_after_ms(u64::MAX), None);
}

#[test]
fn asking_for_a_wait_also_moves_time() {
    let mut c = SlidingCounter::new(1, 10);
    let _ = c.retry_after_ms(100);
    check!("this counts at 100", c.check(3), true);
    check!("so it blocks until 120", c.retry_after_ms(0), Some(20));
}

#[test]
fn large_limits_do_not_overflow() {
    let mut c = SlidingCounter::new(u32::MAX, u64::MAX);
    check!("first", c.check(u64::MAX - 1), true);
    check!("last window", c.check(u64::MAX), true);
}

#[test]
fn random_calls_match_the_rule_over_a_full_log() {
    let mut rng = anneal_prelude::Rng::new(9202);
    for _ in 0..500 {
        let limit = rng.below(7) as u32;
        let window = 1 + rng.below(20) as u64;
        let mut c = SlidingCounter::new(limit, window);
        let mut m = Model { limit: limit.into(), window, latest: 0, admitted: Vec::new() };
        let mut now = 0u64;
        let mut calls = format!("limit {limit}, window {window}: check at");
        for _ in 0..40 {
            now = if rng.below(9) == 0 { now.saturating_sub(rng.below(25) as u64) } else { now + rng.below(7) as u64 };
            if rng.below(3) == 0 {
                m.latest = m.latest.max(now);
                // by definition: the first time at or after now that would pass (two whole windows always suffice)
                let want = (0..=2 * window).find(|d| m.would_pass(m.latest + d));
                check!(format!("{calls}; retry_after_ms({now})"), c.retry_after_ms(now), want);
            } else {
                calls += &format!(" {now}");
                check!(&calls, c.check(now), m.check(now));
            }
        }
    }
}

#[test]
fn the_wait_is_not_found_by_stepping() {
    let mut c = SlidingCounter::new(1, 1 << 50);
    c.check(0);
    for _ in 0..10_000 {
        check!("two windows", c.retry_after_ms(1), Some((1 << 51) - 1));
    }
}
