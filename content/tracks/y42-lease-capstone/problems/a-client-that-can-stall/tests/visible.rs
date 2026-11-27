use solution::*;
use Action::{Acquire, Backoff, Passed, Renew, Rest, Work, Write as Send};
use ClientEvent::{Denied, Granted, PauseOver, Renewed, Wake, WorkDone, WriteDone};

#[allow(dead_code)]
fn grant(token: u64, expires_at: u64) -> ClientEvent {
    Granted(Lease { token, expires_at })
}

/// Feeds each event and checks the action it gets.
#[allow(dead_code)]
fn script(client: &mut Client, steps: &[(ClientEvent, Option<Action>)]) {
    for (i, &(event, want)) in steps.iter().enumerate() {
        check!(format!("step {i}: {event:?}"), client.handle(event), want);
    }
}

const OK: ClientEvent = WriteDone { accepted: true };

#[test]
fn two_writes_under_one_lease() {
    let mut client = Client::new(1, 2);
    script(&mut client, &[
        (Wake, Some(Acquire)),
        (grant(1, 100), Some(Work)),
        (WorkDone { now: 20 }, Some(Passed)),
        (PauseOver, Some(Send { token: 1 })),
        (OK, Some(Renew { token: 1 })),
        (Renewed { token: 1, expires_at: Some(140) }, Some(Work)),
        (WorkDone { now: 139 }, Some(Passed)),
        (PauseOver, Some(Send { token: 1 })),
        (OK, Some(Rest)),
        (Wake, None),
    ]);
    check!("done", client.leases_left(), 0);
}

#[test]
fn the_check_refuses_a_lease_that_has_run_out() {
    let mut client = Client::new(2, 3);
    script(&mut client, &[
        (Wake, Some(Acquire)),
        (grant(1, 100), Some(Work)),
        (WorkDone { now: 100 }, Some(Rest)),
        (PauseOver, None),
        (Wake, Some(Acquire)),
    ]);
    check!("one lease was used up", client.leases_left(), 1);
}

#[test]
fn denied_means_try_again_later() {
    let mut client = Client::new(1, 1);
    script(&mut client, &[(Wake, Some(Acquire)), (Denied, Some(Backoff)), (Wake, Some(Acquire))]);
    check!("nothing used", client.leases_left(), 1);
}

#[test]
fn a_rejected_write_changes_nothing() {
    let mut client = Client::new(1, 2);
    script(&mut client, &[
        (Wake, Some(Acquire)),
        (grant(1, 100), Some(Work)),
        (WorkDone { now: 5 }, Some(Passed)),
        (PauseOver, Some(Send { token: 1 })),
        (WriteDone { accepted: false }, Some(Renew { token: 1 })),
    ]);
}

#[test]
fn a_refused_renewal_ends_the_lease() {
    let mut client = Client::new(1, 5);
    script(&mut client, &[
        (Wake, Some(Acquire)),
        (grant(1, 100), Some(Work)),
        (WorkDone { now: 5 }, Some(Passed)),
        (PauseOver, Some(Send { token: 1 })),
        (OK, Some(Renew { token: 1 })),
        (Renewed { token: 1, expires_at: None }, Some(Rest)),
        (Wake, None),
    ]);
}

#[test]
fn a_grant_delivered_twice_is_used_once() {
    let mut client = Client::new(2, 1);
    script(&mut client, &[
        (Wake, Some(Acquire)),
        (grant(4, 100), Some(Work)),
        (grant(4, 100), None),
        (WorkDone { now: 5 }, Some(Passed)),
        (PauseOver, Some(Send { token: 4 })),
        (OK, Some(Rest)),
        (Wake, Some(Acquire)),
        (grant(4, 100), None),
        (grant(9, 500), Some(Work)),
    ]);
}
