use solution::*;

#[test]
fn messages_line_up() {
    let mut p = Pacer::new(1000);
    check!("first", p.book(5000, 100), Ok(5000));
    check!("second waits for the first", p.book(5000, 100), Ok(5100));
    check!("third", p.book(5040, 30), Ok(5200));
    check!("backlog", p.backlog_ms(5040), 190);
}

#[test]
fn idle_time_is_not_saved_up() {
    let mut p = Pacer::new(1000);
    p.book(0, 100).unwrap();
    check!("after a long rest", p.book(9000, 100), Ok(9000));
    check!("the next one still waits", p.book(9000, 100), Ok(9100));
}

#[test]
fn too_long_a_wait_is_refused_with_the_time_to_return() {
    let mut p = Pacer::new(150);
    check!("a", p.book(0, 100), Ok(0));
    check!("b", p.book(0, 100), Ok(100));
    check!("c would wait 200", p.book(0, 100), Err(50));
    check!("still refused 1 ms early", p.book(49, 100), Err(1));
    check!("on time", p.book(50, 100), Ok(200));
}

#[test]
fn a_wait_equal_to_the_limit_is_allowed() {
    let mut p = Pacer::new(100);
    p.book(0, 100).unwrap();
    check!("exactly the limit", p.book(0, 1), Ok(100));
}

#[test]
fn a_pause_holds_later_messages() {
    let mut p = Pacer::new(500);
    p.pause_until(10, 300);
    check!("after the pause", p.book(10, 50), Ok(300));
    p.pause_until(10, 200);
    check!("a shorter pause changes nothing", p.book(10, 50), Ok(350));
}

#[test]
fn the_clock_may_step_back() {
    let mut p = Pacer::new(10);
    check!("at 1000", p.book(1000, 5), Ok(1000));
    check!("an earlier time counts as 1000", p.book(400, 5), Ok(1005));
}
