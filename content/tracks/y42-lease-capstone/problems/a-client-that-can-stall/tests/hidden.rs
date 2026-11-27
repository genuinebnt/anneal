use solution::*;
use Action::{Acquire, Backoff, Passed, Renew, Rest, Work, Write as Send};
use ClientEvent::{Denied, Granted, PauseOver, Renewed, Wake, WorkDone, WriteDone};

#[allow(dead_code)]
fn grant(token: u64, expires_at: u64) -> ClientEvent {
    Granted(Lease { token, expires_at })
}

#[allow(dead_code)]
fn script(client: &mut Client, steps: &[(ClientEvent, Option<Action>)]) {
    for (i, &(event, want)) in steps.iter().enumerate() {
        check!(format!("step {i}: {event:?}"), client.handle(event), want);
    }
}

const OK: ClientEvent = WriteDone { accepted: true };

#[test]
fn no_writes_per_lease() {
    let mut client = Client::new(2, 0);
    script(&mut client, &[
        (Wake, Some(Acquire)),
        (grant(1, 100), Some(Rest)),
        (WorkDone { now: 0 }, None),
        (PauseOver, None),
        (Wake, Some(Acquire)),
        (grant(2, 200), Some(Rest)),
        (Wake, None),
    ]);
    check!("both used", client.leases_left(), 0);
}

#[test]
fn a_client_that_wants_nothing() {
    let mut client = Client::new(0, 3);
    script(&mut client, &[(Wake, None), (grant(1, 100), None), (Denied, None)]);
}

#[test]
fn the_end_is_taken_from_the_latest_renewal() {
    let mut client = Client::new(1, 3);
    script(&mut client, &[
        (Wake, Some(Acquire)),
        (grant(2, 100), Some(Work)),
        (WorkDone { now: 90 }, Some(Passed)),
        (PauseOver, Some(Send { token: 2 })),
        (OK, Some(Renew { token: 2 })),
        (Renewed { token: 2, expires_at: Some(180) }, Some(Work)),
        (WorkDone { now: 180 }, Some(Rest)),
    ]);
}

#[test]
fn the_answer_to_an_older_renewal_is_ignored() {
    let mut client = Client::new(2, 2);
    script(&mut client, &[
        (Wake, Some(Acquire)),
        (grant(1, 100), Some(Work)),
        (WorkDone { now: 200 }, Some(Rest)),
        (Wake, Some(Acquire)),
        (grant(5, 400), Some(Work)),
        (WorkDone { now: 300 }, Some(Passed)),
        (PauseOver, Some(Send { token: 5 })),
        (OK, Some(Renew { token: 5 })),
        (Renewed { token: 1, expires_at: Some(900) }, None),
        (Renewed { token: 1, expires_at: None }, None),
        (Renewed { token: 6, expires_at: None }, None),
        (Renewed { token: 5, expires_at: Some(450) }, Some(Work)),
        (WorkDone { now: 450 }, Some(Rest)),
    ]);
}

#[test]
fn a_late_grant_from_an_earlier_lease() {
    let mut client = Client::new(3, 1);
    script(&mut client, &[
        (Wake, Some(Acquire)),
        (grant(3, 100), Some(Work)),
        (WorkDone { now: 100 }, Some(Rest)),
        (Wake, Some(Acquire)),
        (grant(2, 999), None),
        (Denied, Some(Backoff)),
        (grant(7, 999), None),
    ]);
    check!("only one grant counted", client.leases_left(), 2);
}

#[test]
fn out_of_place_events_do_nothing() {
    // after each real step, every other kind of event is thrown at the client
    let real = [
        (Wake, Some(Acquire)),
        (grant(1, 100), Some(Work)),
        (WorkDone { now: 1 }, Some(Passed)),
        (PauseOver, Some(Send { token: 1 })),
        (OK, Some(Renew { token: 1 })),
        (Renewed { token: 1, expires_at: Some(200) }, Some(Work)),
    ];
    let noise = [Wake, Denied, WorkDone { now: 0 }, PauseOver, OK, Renewed { token: 1, expires_at: None }, grant(1, 5000)];
    // the events that the next real step is waiting for must be left out
    let awaited: [&[usize]; 6] = [&[1, 6], &[2], &[3], &[4], &[5], &[2]];
    let mut client = Client::new(1, 2);
    for (i, &(event, want)) in real.iter().enumerate() {
        check!(format!("real step {i}: {event:?}"), client.handle(event), want);
        for (kind, &extra) in noise.iter().enumerate() {
            if !awaited[i].contains(&kind) {
                check!(format!("after real step {i}: {extra:?}"), client.handle(extra), None);
            }
        }
    }
    check!("it goes on", client.handle(WorkDone { now: 2 }), Some(Passed));
}

#[test]
fn random_storms_of_events() {
    let mut rng = anneal_prelude::Rng::new(42_201);
    let mut writes_seen = 0;
    for round in 0..600 {
        let (leases, per_lease) = (rng.int(0, 3) as u32, rng.int(0, 3) as u32);
        let mut client = Client::new(leases, per_lease);
        let mut useful = Vec::new();
        let mut log = Vec::new();
        // what an observer can tell from the actions alone
        let (mut last, mut token, mut end, mut newest, mut grants, mut writes) = (None, 0u64, 0u64, 0u64, 0u32, 0u32);
        for op in 0..150 {
            let t = rng.int(0, 12) as u64;
            let at = rng.int(0, 60) as u64;
            let event = match rng.below(7) {
                0 => Wake,
                // tokens start at 1: no grant has token 0
                1 => grant((newest + t.min(2)).max(1), at),
                2 => Denied,
                3 => WorkDone { now: at },
                4 => PauseOver,
                5 => WriteDone { accepted: rng.bool() },
                _ => Renewed {
                    token: if rng.bool() { token } else { t },
                    expires_at: rng.bool().then_some(at),
                },
            };
            let action = client.handle(event);
            let label = format!("round {round} ({leases} leases, {per_lease} writes), op {op}: {event:?} after {last:?}");
            if let Some(action) = action {
                useful.push(event);
                log.push(action);
                let fits = match action {
                    Acquire => event == Wake && matches!(last, None | Some(Rest | Backoff)) && grants < leases,
                    Work => match event {
                        Granted(l) => last == Some(Acquire) && l.token > newest && per_lease > 0,
                        Renewed { token: of, expires_at } => matches!(last, Some(Renew { .. })) && of == token && expires_at.is_some(),
                        _ => false,
                    },
                    Backoff => event == Denied && last == Some(Acquire),
                    Passed => matches!(event, WorkDone { now } if now < end) && last == Some(Work),
                    Send { token: sent } => event == PauseOver && last == Some(Passed) && sent == token && writes < per_lease,
                    Renew { token: of } => matches!(event, WriteDone { .. }) && matches!(last, Some(Send { .. })) && of == token && writes < per_lease,
                    Rest => match event {
                        Granted(l) => last == Some(Acquire) && l.token > newest && per_lease == 0,
                        WorkDone { now } => last == Some(Work) && now >= end,
                        WriteDone { .. } => matches!(last, Some(Send { .. })) && writes == per_lease,
                        Renewed { token: of, expires_at } => matches!(last, Some(Renew { .. })) && of == token && expires_at.is_none(),
                        _ => false,
                    },
                };
                check!(format!("{label}: {action:?} is allowed here"), fits, true);
                match (action, event) {
                    (Work | Rest, Granted(l)) => {
                        (token, end, newest, writes) = (l.token, l.expires_at, l.token, 0);
                        grants += 1;
                    }
                    (Work, Renewed { expires_at: Some(e), .. }) => end = e,
                    (Send { .. }, _) => {
                        writes += 1;
                        writes_seen += 1;
                    }
                    _ => {}
                }
                last = Some(action);
            } else {
                // the events that must never be ignored
                let must_act = match (last, event) {
                    (None | Some(Rest | Backoff), Wake) => grants < leases,
                    (Some(Acquire), Denied) => true,
                    (Some(Acquire), Granted(l)) => l.token > newest,
                    (Some(Work), WorkDone { .. }) | (Some(Passed), PauseOver) => true,
                    (Some(Send { .. }), WriteDone { .. }) => true,
                    (Some(Renew { .. }), Renewed { token: of, .. }) => of == token,
                    _ => false,
                };
                check!(format!("{label}: ignored"), must_act, false);
            }
            check!(format!("{label}: leases left"), client.leases_left(), leases - grants);
        }
        // ignored events left no mark: a new client fed only the others does the same
        let mut fresh = Client::new(leases, per_lease);
        let again: Vec<Action> = useful.iter().filter_map(|&e| fresh.handle(e)).collect();
        check!(format!("round {round}: without the ignored events"), again, log);
    }
    check!("the storms reached writes", writes_seen > 200, true);
}

#[test]
fn the_check_is_exact_at_the_edges() {
    for (now, passed) in [(0, true), (99, true), (100, false), (101, false), (u64::MAX, false)] {
        let mut client = Client::new(1, 1);
        client.handle(Wake);
        client.handle(grant(1, 100));
        let want = if passed { Passed } else { Rest };
        check!(format!("lease ends at 100, now {now}"), client.handle(WorkDone { now }), Some(want));
    }
}
