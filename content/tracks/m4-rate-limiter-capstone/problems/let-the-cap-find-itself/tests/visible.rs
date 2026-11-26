use solution::*;

fn take_all(a: &Adaptive) -> Vec<Ticket> {
    std::iter::from_fn(|| a.try_acquire()).take(10_000).collect()
}

#[test]
fn a_full_round_raises_the_limit_by_one() {
    let a = Adaptive::new(4, 1, 100);
    for ticket in take_all(&a) {
        ticket.success();
    }
    check!("(limit, in flight)", (a.limit(), a.in_flight()), (5, 0));
}

#[test]
fn an_idle_gate_learns_nothing() {
    let a = Adaptive::new(4, 1, 100);
    for _ in 0..100 {
        a.try_acquire().unwrap().success();
    }
    check!("never full", a.limit(), 4);
}

#[test]
fn many_timeouts_at_once_cut_once() {
    let a = Adaptive::new(40, 1, 100);
    for ticket in take_all(&a) {
        ticket.overload();
    }
    check!("halved once", a.limit(), 20);
    a.try_acquire().unwrap().overload();
    check!("a later one cuts again", a.limit(), 10);
}

#[test]
fn old_successes_do_not_undo_a_cut() {
    let a = Adaptive::new(4, 1, 100);
    let mut tickets = take_all(&a);
    let filled = tickets.pop().unwrap();
    tickets.pop().unwrap().overload();
    filled.success();
    check!("limit", a.limit(), 2);
}

#[test]
fn a_dropped_ticket_reports_nothing() {
    let a = Adaptive::new(3, 1, 100);
    drop(take_all(&a));
    check!("(limit, in flight)", (a.limit(), a.in_flight()), (3, 0));
}

#[test]
fn the_bounds_hold() {
    check!("initial too low", Adaptive::new(0, 2, 8).limit(), 2);
    check!("initial too high", Adaptive::new(99, 2, 8).limit(), 8);
    let a = Adaptive::new(8, 3, 8);
    for _ in 0..5 {
        a.try_acquire().unwrap().overload();
    }
    check!("not below min", a.limit(), 3);
}

#[test]
fn it_follows_a_service_up_and_down() {
    let a = Adaptive::new(2, 1, 100);
    let mut limits = Vec::new();
    for round in 0..300 {
        let capacity = if round < 150 { 40 } else { 10 };
        let held = take_all(&a);
        let too_many = held.len() > capacity;
        for ticket in held {
            if too_many {
                ticket.overload()
            } else {
                ticket.success()
            }
        }
        limits.push(a.limit());
    }
    let range = |r: std::ops::Range<usize>| (*limits[r.clone()].iter().min().unwrap(), *limits[r].iter().max().unwrap());
    check!("while the service could take 40", range(100..150), (20, 41));
    check!("after it fell to 10", range(200..300), (5, 11));
}
