use solution::*;

use std::cell::Cell;
use std::rc::Rc;

/// A clock the test moves by hand. Clones share the time.
#[derive(Clone, Default)]
struct FakeClock(Rc<Cell<u64>>);

impl FakeClock {
    fn set(&self, ms: u64) {
        self.0.set(ms);
    }
    fn advance(&self, ms: u64) {
        self.0.set(self.0.get() + ms);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.get()
    }
}

#[test]
fn core_progress_survives_frequent_polling() {
    let clock = FakeClock::default();
    let mut b = TokenBucket::new(clock.clone(), 1, 100);
    b.try_acquire(1);
    let mut granted = 0;
    for _ in 0..1000 {
        clock.advance(7);
        granted += u32::from(b.try_acquire(1));
    }
    // 7000 ms, polled every 7 ms: a token is ready 100 ms after each grant, noticed at the next poll (105 ms).
    check!("grants", granted, 66);
}

#[test]
fn core_leftover_time_counts_towards_the_next_token() {
    let clock = FakeClock::default();
    let mut b = TokenBucket::new(clock.clone(), 3, 100);
    b.try_acquire(3);
    clock.set(150);
    check!("one at 150", (b.try_acquire(1), b.try_acquire(1)), (true, false));
    clock.set(200);
    check!("the second at 200", b.try_acquire(1), true);
}

#[test]
fn core_idle_time_beyond_full_is_lost() {
    let clock = FakeClock::default();
    let mut b = TokenBucket::new(clock.clone(), 2, 100);
    clock.set(1_000_050);
    check!("burst is still 2", [b.try_acquire(1), b.try_acquire(1), b.try_acquire(1)], [true, true, false]);
    clock.advance(99);
    check!("no head start from the time spent full", b.try_acquire(1), false);
    clock.advance(1);
    check!("after a whole interval", b.try_acquire(1), true);
}

#[test]
fn core_a_clock_that_steps_back() {
    let clock = FakeClock::default();
    clock.set(10_000);
    let mut b = TokenBucket::new(clock.clone(), 2, 100);
    b.try_acquire(2);
    clock.set(5_000);
    check!("in the past", b.try_acquire(1), false);
    clock.set(10_000);
    check!("back where it was: nothing was earned", b.try_acquire(1), false);
    clock.set(10_100);
    check!("100 ms after the latest time seen", (b.try_acquire(1), b.try_acquire(1)), (true, false));
}

#[test]
fn core_starts_at_the_current_time() {
    let clock = FakeClock::default();
    clock.set(1_700_000_000_000);
    let mut b = TokenBucket::new(clock.clone(), 1, 1000);
    check!("(first, second)", (b.try_acquire(1), b.try_acquire(1)), (true, false));
}

#[test]
fn core_extreme_values() {
    let clock = FakeClock::default();
    let mut b = TokenBucket::new(clock.clone(), u32::MAX, u64::MAX);
    check!("drain", b.try_acquire(u32::MAX), true);
    clock.set(u64::MAX);
    check!("after the largest time: one token", (b.try_acquire(1), b.try_acquire(1)), (true, false));
    let mut none = TokenBucket::new(clock.clone(), 0, 1);
    check!("capacity 0", (none.try_acquire(1), none.try_acquire(0)), (false, true));
}

#[test]
fn part2_extreme_values() {
    let clock = FakeClock::default();
    let mut none = TokenBucket::new(clock.clone(), 0, 1);
    check!("capacity 0 can never serve one", none.retry_after_ms(1), None);
    check!("capacity 0, nothing asked", none.retry_after_ms(0), Some(0));
    let mut slow = TokenBucket::new(clock.clone(), 2, u64::MAX);
    slow.try_acquire(2);
    check!("a wait that does not fit", slow.retry_after_ms(2), Some(u64::MAX));
}

#[test]
fn core_random_calls_match_a_millisecond_simulation() {
    let mut rng = anneal_prelude::Rng::new(5601);
    for _ in 0..300 {
        let capacity = rng.below(5) as u32;
        let interval = 1 + rng.below(9) as u64;
        let clock = FakeClock::default();
        let mut bucket = TokenBucket::new(clock.clone(), capacity, interval);
        let (mut tokens, mut progress) = (capacity, 0u64);
        let mut trace = vec![format!("capacity {capacity}, every {interval} ms")];
        for _ in 0..40 {
            let wait = rng.below(15) as u64;
            for _ in 0..wait {
                clock.advance(1);
                if tokens < capacity {
                    progress += 1;
                    if progress == interval {
                        tokens += 1;
                        progress = 0;
                    }
                }
            }
            // At full, partial progress is discarded.
            if tokens == capacity {
                progress = 0;
            }
            let n = rng.below(4) as u32;
            let want = n <= tokens;
            if want {
                tokens -= n;
            }
            trace.push(format!("+{wait} ms, take {n}"));
            check!(format!("{trace:?}"), bucket.try_acquire(n), want);
        }
    }
}

#[test]
fn part2_exact_to_the_millisecond() {
    let mut rng = anneal_prelude::Rng::new(5602);
    for _ in 0..300 {
        let capacity = 1 + rng.below(5) as u32;
        let interval = 1 + rng.below(20) as u64;
        let clock = FakeClock::default();
        let mut b = TokenBucket::new(clock.clone(), capacity, interval);
        for _ in 0..20 {
            clock.advance(rng.below(30) as u64);
            let take = rng.below(capacity as usize + 1) as u32;
            b.try_acquire(take);
            let n = rng.below(capacity as usize + 1) as u32;
            let what = format!("capacity {capacity}, every {interval} ms, retry_after_ms({n}) at {}", clock.now_ms());
            let wait = b.retry_after_ms(n);
            check!(format!("{what}: asking twice"), b.retry_after_ms(n), wait);
            let wait = wait.unwrap_or(u64::MAX);
            if wait > 0 {
                clock.advance(wait - 1);
                check!(format!("{what} = {wait}: one ms early"), b.try_acquire(n), false);
                clock.advance(1);
            }
            check!(format!("{what} = {wait}: on time"), b.try_acquire(n), true);
        }
    }
}

#[test]
fn part3_one_interface() {
    let clock = FakeClock::default();
    let mut all: Vec<Box<dyn Limiter>> = vec![Box::new(TokenBucket::new(clock.clone(), 1, 50)), Box::new(SlidingWindow::new(clock.clone(), 1, 50))];
    for limiter in &mut all {
        check!("(first, second)", (limiter.allow(), limiter.allow()), (true, false));
    }
}

#[test]
fn part3_rejections_do_not_count() {
    let clock = FakeClock::default();
    let mut w = SlidingWindow::new(clock.clone(), 1, 100);
    w.allow();
    for t in 1..100 {
        clock.set(t);
        w.allow();
    }
    clock.set(100);
    check!("after 99 rejected calls", w.allow(), true);
}

#[test]
fn part3_no_double_burst_at_a_boundary() {
    let clock = FakeClock::default();
    let mut w = SlidingWindow::new(clock.clone(), 3, 1000);
    clock.set(999);
    let first = (0..3).filter(|_| w.allow()).count();
    clock.set(1000);
    let second = (0..3).filter(|_| w.allow()).count();
    check!("admitted around the 1000 ms mark", (first, second), (3, 0));
    let mut zero = SlidingWindow::new(clock.clone(), 0, 10);
    check!("limit 0", zero.allow(), false);
}

#[test]
fn part3_random_calls_match_a_full_log() {
    let mut rng = anneal_prelude::Rng::new(5603);
    for _ in 0..400 {
        let limit = rng.below(5);
        let window = 1 + rng.below(20) as u64;
        let clock = FakeClock::default();
        let mut w = SlidingWindow::new(clock.clone(), limit, window);
        let mut admitted: Vec<u64> = Vec::new();
        let mut latest = 0u64;
        let mut trace = vec![format!("limit {limit}, window {window}")];
        let mut time = 100u64;
        for _ in 0..40 {
            // Mostly forwards, sometimes back.
            time = if rng.below(8) == 0 { time.saturating_sub(rng.below(30) as u64) } else { time + rng.below(8) as u64 };
            clock.set(time);
            latest = latest.max(time);
            let want = admitted.iter().filter(|&&t| latest < t + window).count() < limit;
            if want {
                admitted.push(latest);
            }
            trace.push(format!("at {time}"));
            check!(format!("{trace:?}"), w.allow(), want);
        }
    }
}

struct Open(Rc<Cell<i64>>);
impl Limiter for Open {
    fn allow(&mut self) -> bool {
        true
    }
}
impl Drop for Open {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

#[test]
fn part4_a_recent_use_keeps_a_key() {
    let clock = FakeClock::default();
    let inner = clock.clone();
    let mut keyed = PerKey::new(clock.clone(), 100, move || TokenBucket::new(inner.clone(), 1, 1_000_000));
    keyed.allow(&1u8);
    clock.set(60);
    check!("empty bucket", keyed.allow(&1), false);
    clock.set(159);
    check!("99 ms after the last use: the same bucket", keyed.allow(&1), false);
    clock.set(259);
    check!("100 ms after: a new one", keyed.allow(&1), true);
}

#[test]
fn part4_forgotten_limiters_are_dropped() {
    let clock = FakeClock::default();
    let alive = Rc::new(Cell::new(0i64));
    let counter = alive.clone();
    let mut keyed = PerKey::new(clock.clone(), 10, move || {
        counter.set(counter.get() + 1);
        Open(counter.clone())
    });
    for key in 0..100u32 {
        keyed.allow(&key);
        clock.advance(1);
    }
    check!("(tracked, alive)", (keyed.tracked(), alive.get()), (9, 9));
    clock.advance(1000);
    keyed.allow(&7);
    check!("after a long pause", alive.get(), 1);
}

#[test]
fn part4_random_keys_match_a_model() {
    let mut rng = anneal_prelude::Rng::new(5604);
    for _ in 0..300 {
        let idle = 1 + rng.below(20) as u64;
        let clock = FakeClock::default();
        let inner = clock.clone();
        // Each key may pass once per very long window, so `true` means "a new limiter".
        let mut keyed = PerKey::new(clock.clone(), idle, move || SlidingWindow::new(inner.clone(), 1, 1_000_000));
        let mut last: Vec<(u8, u64)> = Vec::new();
        let mut trace = vec![format!("idle {idle}")];
        for _ in 0..50 {
            clock.advance(rng.below(12) as u64);
            let now = clock.now_ms();
            last.retain(|e| now - e.1 < idle);
            let key = rng.below(4) as u8;
            let fresh = !last.iter().any(|e| e.0 == key);
            last.retain(|e| e.0 != key);
            last.push((key, now));
            trace.push(format!("at {now} key {key}"));
            check!(format!("{trace:?}"), keyed.allow(&key), fresh);
            check!(format!("{trace:?}: tracked"), keyed.tracked(), last.len());
        }
    }
}

#[test]
fn part4_scale_many_keys() {
    let clock = FakeClock::default();
    let alive = Rc::new(Cell::new(0i64));
    let counter = alive.clone();
    let mut keyed = PerKey::new(clock.clone(), 1_000_000, move || {
        counter.set(counter.get() + 1);
        Open(counter.clone())
    });
    for key in 0..200_000u32 {
        keyed.allow(&key);
        clock.advance(1);
    }
    check!("all still recent", keyed.tracked(), 200_000);
    for i in 0..100_000u32 {
        keyed.allow(&(i % 10));
        clock.advance(20);
    }
    check!("(tracked, alive)", (keyed.tracked(), alive.get()), (10, 10));
}
