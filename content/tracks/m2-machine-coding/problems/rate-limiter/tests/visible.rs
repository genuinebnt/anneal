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
fn core_a_full_bucket_allows_a_burst() {
    let clock = FakeClock::default();
    let mut b = TokenBucket::new(clock.clone(), 3, 100);
    check!("four takes", [(); 4].map(|_| b.try_acquire(1)), [true, true, true, false]);
}

#[test]
fn core_one_token_per_interval() {
    let clock = FakeClock::default();
    let mut b = TokenBucket::new(clock.clone(), 2, 100);
    b.try_acquire(2);
    clock.advance(99);
    check!("at 99 ms", b.try_acquire(1), false);
    clock.advance(1);
    check!("at 100 ms", (b.try_acquire(1), b.try_acquire(1)), (true, false));
}

#[test]
fn core_all_or_nothing() {
    let clock = FakeClock::default();
    let mut b = TokenBucket::new(clock.clone(), 5, 10);
    check!("(6, 4, 2, 1, 0)", [6, 4, 2, 1, 0].map(|n| b.try_acquire(n)), [false, true, false, true, true]);
}

#[test]
fn part2_retry_after() {
    let clock = FakeClock::default();
    let mut b = TokenBucket::new(clock.clone(), 3, 100);
    b.try_acquire(3);
    clock.advance(40);
    check!("(1 token, 3 tokens, 4 tokens)", (b.retry_after_ms(1), b.retry_after_ms(3), b.retry_after_ms(4)), (Some(60), Some(260), None));
}

#[test]
fn part3_sliding_window() {
    let clock = FakeClock::default();
    let mut w = SlidingWindow::new(clock.clone(), 2, 1000);
    check!("at 0", [w.allow(), w.allow(), w.allow()], [true, true, false]);
    clock.set(999);
    check!("at 999", w.allow(), false);
    clock.set(1000);
    check!("at 1000", [w.allow(), w.allow(), w.allow()], [true, true, false]);
}

#[test]
fn part4_keys_are_separate_and_expire() {
    let clock = FakeClock::default();
    let limiter_clock = clock.clone();
    let mut keyed = PerKey::new(clock.clone(), 500, move || SlidingWindow::new(limiter_clock.clone(), 1, 10_000));
    check!("(ann, ann, bob)", (keyed.allow(&"ann"), keyed.allow(&"ann"), keyed.allow(&"bob")), (true, false, true));
    check!("tracked", keyed.tracked(), 2);
    clock.set(500);
    check!("both idle", keyed.tracked(), 0);
    check!("ann starts afresh", keyed.allow(&"ann"), true);
}
