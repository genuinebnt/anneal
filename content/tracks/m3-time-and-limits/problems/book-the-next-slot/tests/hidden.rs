use solution::*;

#[test]
fn no_waiting_at_all() {
    let mut p = Pacer::new(0);
    check!("free", p.book(7, 3), Ok(7));
    check!("busy until 10", p.book(8, 3), Err(2));
    check!("free again", p.book(10, 3), Ok(10));
}

#[test]
fn a_cost_of_zero() {
    let mut p = Pacer::new(0);
    check!("first", p.book(4, 0), Ok(4));
    check!("second", p.book(4, 0), Ok(4));
}

#[test]
fn a_refusal_books_nothing() {
    let mut p = Pacer::new(10);
    p.book(0, 20).unwrap();
    for _ in 0..50 {
        check!("refused", p.book(0, 1000), Err(10));
    }
    check!("the line is free at 20", p.book(20, 1), Ok(20));
}

#[test]
fn a_pause_in_the_past() {
    let mut p = Pacer::new(10);
    p.pause_until(100, 50);
    check!("nothing to wait for", p.book(100, 1), Ok(100));
}

#[test]
fn send_times_beyond_the_largest_time() {
    let mut p = Pacer::new(u64::MAX);
    check!("a huge cost", p.book(10, u64::MAX), Ok(10));
    check!("no send time exists", p.book(10, 1), Err(u64::MAX));
    check!("backlog still fits", p.backlog_ms(11), u64::MAX - 1);
    check!("later too", p.book(u64::MAX, 1), Err(u64::MAX));
    let mut q = Pacer::new(5);
    check!("the last millisecond", q.book(u64::MAX, u64::MAX), Ok(u64::MAX));
    check!("backlog is capped", q.backlog_ms(u64::MAX), u64::MAX);
}

#[test]
fn a_pause_beyond_the_wait_limit() {
    let mut p = Pacer::new(100);
    p.pause_until(0, 1000);
    check!("come back at 900", p.book(0, 5), Err(900));
    check!("then", p.book(900, 5), Ok(1000));
}

#[test]
fn reading_the_backlog_moves_time() {
    let mut p = Pacer::new(100);
    check!("idle", p.backlog_ms(500), 0);
    check!("an earlier booking counts as 500", p.book(10, 1), Ok(500));
}

#[test]
fn matches_a_plan_that_keeps_every_message() {
    let mut rng = anneal_prelude::Rng::new(9103);
    for _ in 0..500 {
        let max_wait = rng.below(40) as u64;
        let mut p = Pacer::new(max_wait);
        // every busy stretch, as (start, end)
        let mut plan: Vec<(u64, u64)> = Vec::new();
        let mut paused = 0u64;
        let (mut now, mut latest) = (0u64, 0u64);
        let mut calls = format!("max_wait {max_wait}");
        for _ in 0..40 {
            now = if rng.below(8) == 0 { now.saturating_sub(rng.below(20) as u64) } else { now + rng.below(12) as u64 };
            latest = latest.max(now);
            let free = plan.iter().map(|m| m.1).fold(paused, u64::max);
            let start = free.max(latest);
            match rng.below(6) {
                0 => {
                    let until = now + rng.below(60) as u64;
                    calls += &format!(", pause_until({now}, {until})");
                    p.pause_until(now, until);
                    paused = paused.max(until);
                }
                1 => {
                    calls += &format!(", backlog_ms({now})");
                    check!(&calls, p.backlog_ms(now), start - latest);
                }
                _ => {
                    let cost = rng.below(25) as u64;
                    calls += &format!(", book({now}, {cost})");
                    let want = if start - latest <= max_wait {
                        plan.push((start, start + cost));
                        Ok(start)
                    } else {
                        Err(start - latest - max_wait)
                    };
                    check!(&calls, p.book(now, cost), want);
                }
            }
        }
    }
}
