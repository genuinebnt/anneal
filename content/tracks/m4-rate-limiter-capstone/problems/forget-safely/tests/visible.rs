use solution::*;

#[test]
fn a_fixed_window_forgets_at_the_next_window() {
    let mut w = FixedWindow::new(2, 100);
    check!("new", w.is_fresh(0), true);
    w.check(10);
    check!("(99, 100)", (w.is_fresh(99), w.is_fresh(100)), (false, true));
}

#[test]
fn a_log_forgets_when_its_newest_entry_leaves() {
    let mut s = SlidingLog::new(5, 100);
    s.check(10);
    s.check(60);
    check!("(110, 159, 160)", (s.is_fresh(110), s.is_fresh(159), s.is_fresh(160)), (false, false, true));
}

#[test]
fn a_counter_remembers_one_window_longer() {
    let mut c = SlidingCounter::new(5, 100);
    c.check(10);
    check!("(100, 199, 200)", (c.is_fresh(100), c.is_fresh(199), c.is_fresh(200)), (false, false, true));
}

#[test]
fn one_number_forgets_when_it_is_paid_off() {
    let mut g = Gcra::new(3, 100);
    g.check(10);
    g.check(10);
    check!("(209, 210)", (g.is_fresh(209), g.is_fresh(210)), (false, true));
}

#[test]
fn keys_are_separate() {
    let mut keyed = Keyed::new(|| FixedWindow::new(1, 100));
    check!("(ann, ann, bob)", (keyed.check(&"ann", 0), keyed.check(&"ann", 1), keyed.check(&"bob", 2)), (true, false, true));
    check!("held", keyed.len(), 2);
    check!("nothing to drop yet", keyed.sweep(99), 0);
    check!("both", keyed.sweep(100), 2);
    check!("empty", keyed.is_empty(), true);
}

#[test]
fn an_earlier_time_counts_for_every_key() {
    let mut keyed = Keyed::new(|| FixedWindow::new(1, 100));
    keyed.check(&1, 250);
    check!("key 2 at 30 counts at 250", keyed.check(&2, 30), true);
    check!("so it is still used up at 260", keyed.check(&2, 260), false);
}
