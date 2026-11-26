use solution::*;

#[test]
fn the_previous_window_fades_out() {
    let mut c = SlidingCounter::new(4, 100);
    for _ in 0..4 {
        c.check(50);
    }
    check!("at 100 all four are in view", c.check(100), false);
    check!("at 124 the estimate is 3.04", c.check(124), false);
    check!("at 125 it is exactly 3", [c.check(125), c.check(125)], [true, false]);
}

#[test]
fn retry_later_in_this_window() {
    let mut c = SlidingCounter::new(4, 100);
    for _ in 0..4 {
        c.check(50);
    }
    check!("from 100", c.retry_after_ms(100), Some(25));
    check!("asking changes nothing", c.retry_after_ms(110), Some(15));
}

#[test]
fn retry_in_the_next_window() {
    let mut c = SlidingCounter::new(2, 100);
    c.check(10);
    c.check(10);
    check!("not before 150", c.retry_after_ms(10), Some(140));
}

#[test]
fn retry_after_both_windows() {
    let mut c = SlidingCounter::new(1, 100);
    c.check(30);
    check!("the next window is blocked entirely", c.retry_after_ms(30), Some(170));
}

#[test]
fn a_gap_forgets_everything() {
    let mut c = SlidingCounter::new(2, 100);
    c.check(0);
    c.check(0);
    check!("two windows later", [c.check(200), c.check(200), c.check(200)], [true, true, false]);
}

#[test]
fn it_sits_between_the_two_others() {
    let arrivals = [999, 999, 999, 1000, 1000, 1000, 1500, 1500, 1500];
    let r = replay(&mut SlidingCounter::new(3, 1000), &arrivals, 1000);
    check!("(allowed, busiest)", (r.allowed, r.busiest), (4, 4));
}
