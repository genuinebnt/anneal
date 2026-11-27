use solution::*;

/// An event type that can do nothing but exist.
#[allow(dead_code)]
struct Plain(usize);

#[test]
fn the_event_needs_no_traits() {
    let mut agenda = Agenda::new();
    agenda.after(2, Plain(1));
    agenda.after(1, Plain(2));
    check!("second planned comes first", agenda.next().map(|(t, e)| (t, e.0)), Some((1, 2)));
}

#[test]
fn exactly_the_budget_is_enough() {
    let mut agenda = Agenda::new();
    for i in 0..4 {
        agenda.after(i, i);
    }
    check!("4 events, 4 steps", run(&mut agenda, 4, |_, _, _| {}), Stop::Finished);
    check!("empty, no steps", run(&mut agenda, 0, |_, _, _| {}), Stop::Finished);
    agenda.after(1, 9);
    let mut handled = 0;
    check!("no budget", run(&mut agenda, 0, |_, _, _| handled += 1), Stop::OutOfSteps);
    check!("nothing was touched", (handled, agenda.len()), (0, 1));
}

#[test]
fn only_cancelled_events_left_means_finished() {
    let mut agenda = Agenda::new();
    agenda.after(1, "a");
    let b = agenda.after(2, "b").unwrap();
    let mut cancel = Some(b);
    let stop = run(&mut agenda, 1, |agenda, _, _| {
        if let Some(ticket) = cancel.take() {
            agenda.cancel(ticket);
        }
    });
    check!("stop", stop, Stop::Finished);
    check!("the clock did not move to the cancelled one", agenda.now(), 1);
    check!("nothing", (agenda.next().is_none(), agenda.now(), agenda.is_empty()), (true, 1, true));
}

#[test]
fn a_time_beyond_u64_is_refused() {
    let mut agenda = Agenda::new();
    agenda.after(10, 'a');
    agenda.next();
    check!("too far", agenda.after(u64::MAX, 'b').is_none(), true);
    check!("nothing planned", agenda.len(), 0);
    check!("the last time that fits", agenda.after(u64::MAX - 10, 'c').is_some(), true);
    check!("comes out", agenda.next(), Some((u64::MAX, 'c')));
    check!("zero still fits", agenda.after(0, 'd').is_some(), true);
    check!("one does not", agenda.after(1, 'e').is_none(), true);
}

#[test]
fn a_used_ticket_cannot_cancel_a_newer_event() {
    let mut agenda = Agenda::new();
    let first = agenda.after(1, 1).unwrap();
    agenda.next();
    let second = agenda.after(1, 2).unwrap();
    check!("tickets differ", first == second, false);
    check!("old ticket", agenda.cancel(first), None);
    check!("new one still pending", agenda.len(), 1);
}

#[test]
fn matches_a_plain_list() {
    let mut rng = anneal_prelude::Rng::new(41_101);
    for round in 0..300 {
        let mut agenda = Agenda::new();
        // the model: (time, planning number, ticket), searched from the start every time
        let mut model: Vec<(u64, usize, Ticket)> = Vec::new();
        let mut tickets = Vec::new();
        let mut now = 0;
        for op in 0..80 {
            let label = format!("round {round}, op {op}");
            match rng.below(5) {
                0 | 1 => {
                    let delay = rng.below(4) as u64;
                    let ticket = agenda.after(delay, tickets.len()).expect("small times fit");
                    model.push((now + delay, tickets.len(), ticket));
                    tickets.push(ticket);
                }
                2 if !tickets.is_empty() => {
                    let which = rng.below(tickets.len());
                    let at = model.iter().position(|&(_, n, _)| n == which);
                    let want = at.map(|i| model.remove(i).1);
                    check!(format!("{label}: cancel {which}"), agenda.cancel(tickets[which]), want);
                }
                _ => {
                    let at = (0..model.len()).min_by_key(|&i| (model[i].0, model[i].1));
                    let want = at.map(|i| model.remove(i)).map(|(t, n, _)| (t, n));
                    if let Some((t, _)) = want {
                        now = t;
                    }
                    check!(format!("{label}: next"), agenda.next(), want);
                }
            }
            check!(format!("{label}: now and len"), (agenda.now(), agenda.len()), (now, model.len()));
        }
    }
}

#[test]
fn many_pending_events_stay_fast() {
    let mut rng = anneal_prelude::Rng::new(41_102);
    let mut agenda = Agenda::new();
    let tickets: Vec<Ticket> = (0..200_000u32)
        .map(|i| agenda.after(rng.below(1000) as u64, i).unwrap())
        .collect();
    let cancelled = tickets.iter().step_by(2).filter(|&&t| agenda.cancel(t).is_some()).count();
    check!("cancelled", cancelled, 100_000);
    let mut last = (0, 0);
    let mut count = 0;
    while let Some((time, i)) = agenda.next() {
        check!("in order of time, then of planning", (time, i) > last || count == 0, true);
        last = (time, i);
        count += 1;
    }
    check!("the rest came out", count, 100_000);
}

#[test]
fn a_handler_can_replace_a_timeout() {
    // each message pushes the timeout back: only the last timeout happens
    #[derive(Debug, PartialEq)]
    enum E {
        Message(u32),
        Quiet,
    }
    let mut agenda = Agenda::new();
    for i in 0..5 {
        agenda.after(u64::from(i) * 10, E::Message(i));
    }
    let mut timeout: Option<Ticket> = None;
    let mut quiet_at = Vec::new();
    run(&mut agenda, 1000, |agenda, time, event| match event {
        E::Message(_) => {
            if let Some(old) = timeout.take() {
                agenda.cancel(old);
            }
            timeout = agenda.after(15, E::Quiet);
        }
        E::Quiet => quiet_at.push(time),
    });
    check!("one quiet moment", quiet_at, vec![55]);
}
