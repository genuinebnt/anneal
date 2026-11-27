use solution::*;

#[allow(dead_code)]
fn lease(token: u64, expires_at: u64) -> Option<Lease> {
    Some(Lease { token, expires_at })
}

#[test]
#[should_panic]
fn no_time_to_live_is_refused() {
    LeaseService::new(0);
}

#[test]
fn the_same_client_with_an_old_token() {
    let mut service = LeaseService::new(10);
    service.acquire(0, 4);
    check!("its second lease", service.acquire(10, 4), lease(2, 20));
    check!("the first token no longer renews", service.renew(11, 4, 1), None);
    check!("a token not yet granted", service.renew(11, 4, 3), None);
    check!("the current one does", service.renew(11, 4, 2), lease(2, 21));
}

#[test]
fn another_client_cannot_renew_with_the_right_token() {
    let mut service = LeaseService::new(10);
    service.acquire(0, 1);
    check!("stolen number", service.renew(5, 2, 1), None);
    check!("end unchanged", service.holder(10), None);
}

#[test]
fn refused_calls_use_no_token() {
    let mut service = LeaseService::new(10);
    service.acquire(0, 1);
    for t in 1..10 {
        service.acquire(t, 2);
    }
    check!("the second grant", service.acquire(10, 2), lease(2, 20));
}

#[test]
fn an_earlier_time_counts_as_the_latest_seen() {
    let mut service = LeaseService::new(100);
    service.acquire(500, 1);
    check!("renew 'at 0' is at 500", service.renew(0, 1, 1), lease(1, 600));
    check!("holder at 900", service.holder(900), None);
    check!("'at 0' is now at 900: free", service.holder(0), None);
    check!("and cannot be renewed", service.renew(550, 1, 1), None);
    check!("grant 'at 3' starts at 900", service.acquire(3, 2), lease(2, 1000));
}

#[test]
fn a_refused_call_also_moves_the_clock() {
    let mut service = LeaseService::new(100);
    service.acquire(0, 1);
    check!("wrong client, late", service.renew(300, 9, 9), None);
    check!("so the lease is gone", service.renew(50, 1, 1), None);
    check!("holder", service.holder(50), None);
}

#[test]
fn a_refused_acquire_moves_the_clock_too() {
    let mut service = LeaseService::new(100);
    service.acquire(0, 1);
    service.acquire(90, 2);
    check!("renewal 'at 10' is at 90", service.renew(10, 1, 1), lease(1, 190));
}

#[test]
fn end_times_stop_at_the_largest_number() {
    let mut service = LeaseService::new(u64::MAX);
    check!("grant", service.acquire(5, 1), lease(1, u64::MAX));
    check!("live just before", service.holder(u64::MAX - 1), Some(1));
    check!("run out at the end time", service.holder(u64::MAX), None);
}

#[test]
fn matches_a_history_of_every_extension() {
    let mut rng = anneal_prelude::Rng::new(42_001);
    for round in 0..300 {
        let ttl = rng.int(1, 12) as u64;
        let mut service = LeaseService::new(ttl);
        // every grant so far, as (client, end time); grant number i has token i + 1
        let mut grants: Vec<(u32, u64)> = Vec::new();
        let (mut base, mut seen) = (0i64, 0u64);
        for op in 0..80 {
            base += rng.int(0, 5);
            let now = (base + rng.int(-6, 3)).max(0) as u64;
            seen = seen.max(now);
            let client = rng.below(3) as u32;
            let live = grants.last().copied().filter(|g| seen < g.1);
            let label = format!("round {round}, op {op}, ttl {ttl}, time {now}, client {client}");
            match rng.below(3) {
                0 => {
                    let want = if live.is_some() {
                        None
                    } else {
                        grants.push((client, seen + ttl));
                        lease(grants.len() as u64, seen + ttl)
                    };
                    check!(format!("{label}: acquire"), service.acquire(now, client), want);
                }
                1 => {
                    let token = (grants.len() as i64 - rng.int(-1, 1)).max(0) as u64;
                    let mut want = None;
                    if live.is_some_and(|g| g.0 == client) && token == grants.len() as u64 {
                        grants.last_mut().unwrap().1 = seen + ttl;
                        want = lease(token, seen + ttl);
                    }
                    check!(format!("{label}: renew {token}"), service.renew(now, client, token), want);
                }
                _ => check!(format!("{label}: holder"), service.holder(now), live.map(|g| g.0)),
            }
        }
    }
}
