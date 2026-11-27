use solution::*;


#[test]
fn the_longest_waiting_gets_the_item() {
    let mut w = Waiting::new();
    check!("park 7", w.park(7, &["book"], None, "seven"), true);
    check!("park 3", w.park(3, &["book"], None, "three"), true);
    check!("first", w.wake("book"), Some((7, "seven")));
    check!("second", w.wake("book"), Some((3, "three")));
    check!("nobody left", w.wake("book"), None);
}

#[test]
fn whoever_is_woken_stops_waiting_for_everything() {
    let mut w = Waiting::new();
    w.park(1, &["pen", "ink"], None, ());
    w.park(2, &["ink"], None, ());
    check!("a pen wakes 1", w.wake("pen"), Some((1, ())));
    check!("so ink goes to 2", w.wake("ink"), Some((2, ())));
    check!("empty", w.len(), 0);
}

#[test]
fn deadlines() {
    let mut w = Waiting::new();
    w.park(1, &["a"], Some(30), 'x');
    w.park(2, &["a"], Some(10), 'y');
    w.park(3, &["a"], None, 'z');
    check!("the next one", w.next_deadline(), Some(10));
    check!("nothing at 9", w.expire(9), vec![]);
    check!("both at 30, earliest first", w.expire(30), vec![(2, 'y'), (1, 'x')]);
    check!("3 waits for ever", (w.next_deadline(), w.len()), (None, 1));
}

#[test]
fn cancel() {
    let mut w = Waiting::new();
    w.park(1, &["a", "b"], Some(5), 10);
    check!("cancel returns the ticket", w.cancel(1), Some(10));
    check!("once", w.cancel(1), None);
    check!("not woken", w.wake("a"), None);
    check!("not expired", w.expire(100), vec![]);
}

#[test]
fn one_wait_per_id() {
    let mut w = Waiting::new();
    w.park(1, &["a"], None, 1);
    check!("a second park is refused", w.park(1, &["b"], Some(0), 2), false);
    check!("nothing changed", (w.wake("b"), w.expire(10), w.len()), (None, vec![], 1));
}
