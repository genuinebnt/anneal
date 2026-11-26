use solution::*;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// How many of `limit + 1` requests made at `at` pass.
fn burst_at<L: Limiter>(mut l: L, at: u64) -> usize {
    (0..12).filter(|_| l.check(at)).count()
}

/// `is_fresh` by what can be observed: after a random history, a burst at `t` tells old from new.
fn exact<L: Forget + Clone>(name: &str, seed: u64, make: impl Fn(u32, u64) -> L) {
    let mut rng = anneal_prelude::Rng::new(seed);
    for _ in 0..1500 {
        let limit = rng.below(5) as u32;
        let window = 1 + rng.below(12) as u64;
        let mut l = make(limit, window);
        let mut now = 0u64;
        let mut calls = format!("{name}, limit {limit}, window or interval {window}, calls at");
        for _ in 0..rng.below(14) {
            now += rng.below(8) as u64;
            l.check(now);
            calls += &format!(" {now}");
        }
        for _ in 0..8 {
            let t = now + rng.below(4 * window as usize + 2) as u64;
            let want = burst_at(l.clone(), t) == burst_at(make(limit, window), t);
            check!(format!("{calls}; is_fresh({t})"), l.is_fresh(t), want);
        }
    }
}

#[test]
fn fixed_window_is_fresh_exactly_when_it_behaves_as_new() {
    exact("FixedWindow", 9205, FixedWindow::new);
}

#[test]
fn sliding_log_is_fresh_exactly_when_it_behaves_as_new() {
    exact("SlidingLog", 9206, SlidingLog::new);
}

#[test]
fn sliding_counter_is_fresh_exactly_when_it_behaves_as_new() {
    exact("SlidingCounter", 9207, SlidingCounter::new);
}

#[test]
fn gcra_is_fresh_exactly_when_it_behaves_as_new() {
    exact("Gcra", 9208, Gcra::new);
}

#[test]
fn an_earlier_time_asked_about_counts_as_the_latest() {
    let mut w = FixedWindow::new(1, 100);
    w.check(150);
    check!("asked about 20", w.is_fresh(20), false);
    let mut c = SlidingCounter::new(1, 100);
    c.check(150);
    check!("counter asked about 20", c.is_fresh(20), false);
    let mut s = SlidingLog::new(1, 100);
    s.check(150);
    check!("log asked about 20", s.is_fresh(20), false);
}

/// Decisions with any amount of sweeping must equal those of limiters that are never forgotten.
fn transparent<L: Forget>(name: &str, seed: u64, make: impl Fn(u32, u64) -> L) {
    let mut rng = anneal_prelude::Rng::new(seed);
    for _ in 0..200 {
        let limit = rng.below(5) as u32;
        let window = 1 + rng.below(30) as u64;
        let mut keyed = Keyed::new(|| make(limit, window));
        let mut kept: HashMap<u32, L> = HashMap::new();
        let (mut now, mut latest) = (0u64, 0u64);
        let keys = 1 + rng.below(150);
        for step in 0..600 {
            now = if rng.below(10) == 0 { now.saturating_sub(rng.below(40) as u64) } else { now + rng.below(4) as u64 };
            latest = latest.max(now);
            if rng.below(9) == 0 {
                let want = keyed.len();
                let removed = keyed.sweep(now);
                check!(format!("{name} step {step}: sweep says what it removed"), want - keyed.len(), removed);
            }
            let key = rng.below(keys) as u32;
            let want = kept.entry(key).or_insert_with(|| make(limit, window)).check(latest);
            check!(format!("{name} limit {limit}, window {window}, step {step}: key {key} at {now}"), keyed.check(&key, now), want);
        }
        keyed.sweep(latest);
        check!(format!("{name}: after a sweep only limiters that still matter are held"), keyed.len(), kept.values().filter(|l| !l.is_fresh(latest)).count());
    }
}

#[test]
fn forgetting_never_changes_an_answer() {
    transparent("FixedWindow", 9209, FixedWindow::new);
    transparent("SlidingLog", 9210, SlidingLog::new);
    transparent("SlidingCounter", 9211, SlidingCounter::new);
    transparent("Gcra", 9212, Gcra::new);
}

/// Counts the limiters that exist.
struct Counted {
    inner: FixedWindow,
    alive: Rc<Cell<usize>>,
}
impl Limiter for Counted {
    fn check(&mut self, now_ms: u64) -> bool {
        self.inner.check(now_ms)
    }
}
impl Forget for Counted {
    fn is_fresh(&self, now_ms: u64) -> bool {
        self.inner.is_fresh(now_ms)
    }
}
impl Drop for Counted {
    fn drop(&mut self) {
        self.alive.set(self.alive.get() - 1);
    }
}

#[test]
fn memory_follows_the_keys_that_still_matter() {
    let alive = Rc::new(Cell::new(0));
    let mut keyed = Keyed::new(|| {
        alive.set(alive.get() + 1);
        Counted { inner: FixedWindow::new(1, 10), alive: alive.clone() }
    });
    // 100 keys a millisecond window of 10 ms: never more than 1000 matter at once
    for i in 0..100_000u64 {
        keyed.check(&i, i / 100);
        let held = keyed.len();
        assert!(held <= 2 * 1000 + 64, "{held} limiters held after {i} calls");
        assert_eq!(alive.get(), held, "a forgotten limiter must be dropped");
    }
}

#[test]
fn limiters_that_never_matter_do_not_pile_up() {
    let mut keyed = Keyed::new(|| FixedWindow::new(0, 10));
    for i in 0..10_000u32 {
        check!("refused", keyed.check(&i, 0), false);
        assert!(keyed.len() <= 64, "{} limiters held", keyed.len());
    }
}

#[test]
fn many_keys_that_all_still_matter() {
    // None can be forgotten, so looking at every key on every call is far too slow.
    let mut keyed = Keyed::new(|| SlidingCounter::new(1, 1 << 40));
    for i in 0..200_000u64 {
        assert!(keyed.check(&i, i));
    }
    for i in 0..200_000u64 {
        assert!(!keyed.check(&i, 200_000));
    }
    check!("all held", keyed.len(), 200_000);
}
