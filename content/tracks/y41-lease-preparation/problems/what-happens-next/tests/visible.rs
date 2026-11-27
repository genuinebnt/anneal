use solution::*;

/// Takes everything out, in order.
#[allow(dead_code)]
fn drain<E>(agenda: &mut Agenda<E>) -> Vec<(u64, E)> {
    std::iter::from_fn(|| agenda.next()).collect()
}

#[test]
fn events_come_out_by_time() {
    let mut agenda = Agenda::new();
    agenda.after(30, "lunch");
    agenda.after(10, "coffee");
    agenda.after(20, "post");
    check!("pending", agenda.len(), 3);
    check!("order", drain(&mut agenda), vec![(10, "coffee"), (20, "post"), (30, "lunch")]);
    check!("clock", agenda.now(), 30);
}

#[test]
fn equal_times_keep_the_planning_order() {
    let mut agenda = Agenda::new();
    for name in ["zed", "ann", "mia", "bob"] {
        agenda.after(5, name);
    }
    let names: Vec<&str> = drain(&mut agenda).into_iter().map(|(_, n)| n).collect();
    check!("as planned", names, vec!["zed", "ann", "mia", "bob"]);
}

#[test]
fn delays_count_from_now() {
    let mut agenda = Agenda::new();
    agenda.after(10, 1);
    check!("first", agenda.next(), Some((10, 1)));
    agenda.after(5, 2);
    agenda.after(0, 3);
    check!("now itself", agenda.next(), Some((10, 3)));
    check!("five later", agenda.next(), Some((15, 2)));
    check!("empty", agenda.next(), None);
    check!("the clock stays", agenda.now(), 15);
}

#[test]
fn a_cancelled_event_never_happens() {
    let mut agenda = Agenda::new();
    let early = agenda.after(1, "early").unwrap();
    agenda.after(9, "late");
    check!("taken back", agenda.cancel(early), Some("early"));
    check!("only once", agenda.cancel(early), None);
    check!("one left", agenda.len(), 1);
    check!("the late one", agenda.next(), Some((9, "late")));
}

#[test]
fn run_lets_the_handler_plan_more() {
    let mut agenda = Agenda::new();
    agenda.after(1, 3u32);
    let mut seen = Vec::new();
    let stop = run(&mut agenda, 100, |agenda, time, left| {
        seen.push((time, left));
        if left > 0 {
            agenda.after(10, left - 1);
        }
    });
    check!("stop", stop, Stop::Finished);
    check!("seen", seen, vec![(1, 3), (11, 2), (21, 1), (31, 0)]);
}

#[test]
fn the_budget_stops_an_endless_system() {
    let mut agenda = Agenda::new();
    agenda.after(1, ());
    let mut handled = 0;
    let stop = run(&mut agenda, 50, |agenda, _, ()| {
        handled += 1;
        agenda.after(1, ());
    });
    check!("stop", stop, Stop::OutOfSteps);
    check!("handled", handled, 50);
    check!("the 51st is still pending", (agenda.len(), agenda.now()), (1, 50));
}
