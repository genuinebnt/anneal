use solution::*;


#[derive(Debug, PartialEq)]
struct NotCopy(String);

#[test]
fn tickets_are_moved_out_not_copied() {
    let mut w = Waiting::new();
    w.park(1, &["a"], None, NotCopy("mine".into()));
    check!("wake", w.wake("a"), Some((1, NotCopy("mine".into()))));
}

#[test]
fn an_item_named_twice_counts_once() {
    let mut w = Waiting::new();
    w.park(1, &["a", "a", "b", "a"], None, ());
    w.park(2, &["a"], None, ());
    check!("first a", w.wake("a"), Some((1, ())));
    check!("second a", w.wake("a"), Some((2, ())));
    check!("b has nobody", w.wake("b"), None);
}

#[test]
fn waiting_for_nothing_only_a_deadline_ends_it() {
    let mut w = Waiting::new();
    check!("park", w.park(1, &[], Some(4), ()), true);
    check!("contains", w.contains(1), true);
    check!("expire", w.expire(4), vec![(1, ())]);
}

#[test]
fn equal_deadlines_go_in_the_order_they_came() {
    let mut w = Waiting::new();
    for id in [9, 2, 5, 1] {
        w.park(id, &["a"], Some(100), id);
    }
    check!("order", w.expire(u64::MAX).into_iter().map(|(id, _)| id).collect::<Vec<_>>(), vec![9, 2, 5, 1]);
}

#[test]
fn an_id_that_comes_back_goes_to_the_end() {
    let mut w = Waiting::new();
    w.park(1, &["a"], None, ());
    w.park(2, &["a"], None, ());
    w.cancel(1);
    w.park(1, &["a"], None, ());
    check!("2 is ahead now", w.wake("a"), Some((2, ())));
}

#[test]
fn extreme_deadlines() {
    let mut w = Waiting::new();
    w.park(u64::MAX, &["a"], Some(u64::MAX), ());
    w.park(0, &["a"], Some(0), ());
    check!("at 0", w.expire(0), vec![(0, ())]);
    check!("just before the end", w.expire(u64::MAX - 1), vec![]);
    check!("at the end", w.expire(u64::MAX), vec![(u64::MAX, ())]);
}

#[test]
fn a_deadline_in_the_past_still_waits_for_expire() {
    let mut w = Waiting::new();
    w.expire(1000);
    w.park(1, &["a"], Some(5), ());
    check!("it can still be woken", w.wake("a"), Some((1, ())));
}

#[test]
fn random_use_matches_a_plain_list() {
    let mut rng = anneal_prelude::Rng::new(0x3108);
    let items = ["a", "b", "c", "d"];
    for run in 0..300 {
        let mut w: Waiting<usize> = Waiting::new();
        // in the order they started waiting
        let mut model: Vec<(u64, Vec<&str>, Option<u64>, usize)> = Vec::new();
        let mut history = Vec::new();
        let mut now = 0;
        for step in 0..80 {
            match rng.below(5) {
                0 | 1 => {
                    let id = rng.below(8) as u64;
                    let n = rng.below(4);
                    let wants: Vec<&str> = (0..n).map(|_| *rng.pick(&items)).collect();
                    let deadline = rng.bool().then(|| now + rng.below(20) as u64);
                    history.push(format!("park {id} {wants:?} {deadline:?}"));
                    let fresh = model.iter().all(|m| m.0 != id);
                    check!(format!("run {run}: {history:?}"), w.park(id, &wants, deadline, step), fresh);
                    if fresh {
                        model.push((id, wants, deadline, step));
                    }
                }
                2 => {
                    let item = *rng.pick(&items);
                    history.push(format!("wake {item}"));
                    let want = model.iter().position(|m| m.1.contains(&item)).map(|i| {
                        let m = model.remove(i);
                        (m.0, m.3)
                    });
                    check!(format!("run {run}: {history:?}"), w.wake(item), want);
                }
                3 => {
                    let id = rng.below(8) as u64;
                    history.push(format!("cancel {id}"));
                    let want = model.iter().position(|m| m.0 == id).map(|i| model.remove(i).3);
                    check!(format!("run {run}: {history:?}"), w.cancel(id), want);
                }
                _ => {
                    now += rng.below(10) as u64;
                    history.push(format!("expire {now}"));
                    let mut due: Vec<(u64, usize, u64, usize)> = model
                        .iter()
                        .enumerate()
                        .filter_map(|(i, m)| m.2.filter(|&d| d <= now).map(|d| (d, i, m.0, m.3)))
                        .collect();
                    due.sort();
                    model.retain(|m| !m.2.is_some_and(|d| d <= now));
                    let want: Vec<(u64, usize)> = due.into_iter().map(|d| (d.2, d.3)).collect();
                    check!(format!("run {run}: {history:?}"), w.expire(now), want);
                }
            }
            let ctx = || format!("run {run}: after {history:?}");
            check!(ctx(), w.len(), model.len());
            check!(ctx(), w.next_deadline(), model.iter().filter_map(|m| m.2).min());
            let probe = rng.below(8) as u64;
            check!(ctx(), w.contains(probe), model.iter().any(|m| m.0 == probe));
        }
    }
}

#[test]
fn many_waiters_stay_fast() {
    // walking a queue to remove someone, or looking at everyone on each call, cannot finish in time
    let mut w = Waiting::new();
    let n = 200_000u64;
    for id in 0..n {
        w.park(id, &["hot", "also"], Some(1_000_000 + id), id);
        if id % 2 == 0 {
            check!("nothing is due", w.expire(id).len(), 0);
            check!("the next deadline", w.next_deadline(), Some(1_000_000));
        }
    }
    // cancel from the back of the queue
    for id in (n / 2..n).rev() {
        check!("cancel", w.cancel(id), Some(id));
    }
    for id in 0..n / 4 {
        check!("wake", w.wake("also"), Some((id, id)));
    }
    for id in n / 4..n / 2 {
        check!("expire one at a time", w.expire(1_000_000 + id), vec![(id, id)]);
    }
    check!("empty", w.len(), 0);
}

#[test]
fn many_items_come_and_go() {
    let mut w = Waiting::new();
    for i in 0..100_000u64 {
        let item = format!("item{i}");
        w.park(i, &[&item], None, ());
        check!("woken at once", w.wake(&item), Some((i, ())));
    }
    check!("nothing is left", w.len(), 0);
}
