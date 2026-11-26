use solution::*;

/// By definition: the most of `times` in any stretch of `span` that starts at one of them.
fn model_busiest(times: &[u64], span: u64) -> usize {
    times.iter().map(|&s| times.iter().filter(|&&t| t >= s && t - s < span).count()).max().unwrap_or(0)
}

/// Records what it is asked and answers from a script.
struct Script {
    answers: Vec<bool>,
    asked: Vec<u64>,
}
impl Limiter for Script {
    fn check(&mut self, now_ms: u64) -> bool {
        self.asked.push(now_ms);
        self.answers[self.asked.len() - 1]
    }
}

#[test]
fn replay_asks_once_per_arrival_with_the_effective_time() {
    let mut s = Script { answers: vec![false, true, false, true], asked: vec![] };
    let r = replay(&mut s, &[5, 3, 9, 1], 4);
    check!("times given to the limiter", s.asked, vec![5, 5, 9, 9]);
    check!("report", r, Report { decisions: vec![false, true, false, true], allowed: 2, denied: 2, busiest: 1 });
}

#[test]
fn refused_requests_do_not_count_in_the_busiest_stretch() {
    let mut s = Script { answers: vec![true, false, false, true], asked: vec![] };
    check!("busiest", replay(&mut s, &[0, 0, 0, 0], 1).busiest, 2);
}

#[test]
fn the_measured_span_is_its_own() {
    let r = replay(&mut FixedWindow::new(1, 10), &[0, 10, 20, 30], 31);
    check!("span 31", r.busiest, 4);
    let r = replay(&mut FixedWindow::new(1, 10), &[0, 10, 20, 30], 30);
    check!("span 30", r.busiest, 3);
}

#[test]
fn a_limit_of_zero_and_a_window_of_one() {
    let mut z = FixedWindow::new(0, 10);
    check!("nothing passes", [z.check(0), z.check(100)], [false, false]);
    let mut one = FixedWindow::new(1, 1);
    check!("one per millisecond", [one.check(7), one.check(7), one.check(8)], [true, false, true]);
}

#[test]
fn the_largest_times() {
    let m = u64::MAX;
    let r = replay(&mut FixedWindow::new(2, m), &[0, m - 1, m - 1, m, m], m);
    check!("report", (r.decisions, r.busiest), (vec![true, true, false, true, true], 3));
}

#[test]
fn an_earlier_time_does_not_reopen_a_window() {
    let mut w = FixedWindow::new(1, 100);
    check!("(at 150, at 20, at 160)", [w.check(150), w.check(20), w.check(160)], [true, false, false]);
}

#[test]
fn random_traffic_matches_a_model() {
    let mut rng = anneal_prelude::Rng::new(9201);
    for _ in 0..500 {
        let limit = rng.below(4) as u32;
        let window = 1 + rng.below(12) as u64;
        let span = 1 + rng.below(25) as u64;
        let mut now = 0u64;
        let arrivals: Vec<u64> = (0..rng.below(25))
            .map(|_| {
                now = if rng.below(8) == 0 { now.saturating_sub(rng.below(15) as u64) } else { now + rng.below(7) as u64 };
                now
            })
            .collect();
        let mut latest = 0;
        let (mut decisions, mut admitted) = (Vec::new(), Vec::<u64>::new());
        for &a in &arrivals {
            latest = a.max(latest);
            let ok = admitted.iter().filter(|&&t| t / window == latest / window).count() < limit as usize;
            decisions.push(ok);
            if ok {
                admitted.push(latest);
            }
        }
        let want = Report { allowed: admitted.len(), denied: arrivals.len() - admitted.len(), busiest: model_busiest(&admitted, span), decisions };
        check!(format!("limit {limit}, window {window}, span {span}, {arrivals:?}"), replay(&mut FixedWindow::new(limit, window), &arrivals, span), want);
    }
}

#[test]
fn a_long_replay_with_a_wide_span() {
    let arrivals: Vec<u64> = (0..200_000).collect();
    let r = replay(&mut FixedWindow::new(u32::MAX, 10), &arrivals, 150_000);
    check!("(allowed, busiest)", (r.allowed, r.busiest), (200_000, 150_000));
}
