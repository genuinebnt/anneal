use solution::*;

#[allow(dead_code)]
fn lease(token: u64, expires_at: u64) -> Option<Lease> {
    Some(Lease { token, expires_at })
}

#[test]
fn a_free_service_grants_a_lease() {
    let mut service = LeaseService::new(100);
    check!("grant", service.acquire(10, 7), lease(1, 110));
    check!("holder", service.holder(50), Some(7));
}

#[test]
fn only_one_lease_is_live() {
    let mut service = LeaseService::new(100);
    service.acquire(0, 1);
    check!("another client", service.acquire(99, 2), None);
    check!("the holder itself", service.acquire(99, 1), None);
}

#[test]
fn at_the_end_time_it_has_run_out() {
    let mut service = LeaseService::new(100);
    service.acquire(0, 1);
    check!("at 99", service.holder(99), Some(1));
    check!("at 100", service.holder(100), None);
    check!("next grant", service.acquire(100, 2), lease(2, 200));
}

#[test]
fn renewing_keeps_the_token() {
    let mut service = LeaseService::new(100);
    service.acquire(0, 1);
    check!("renew at 60", service.renew(60, 1, 1), lease(1, 160));
    check!("still held at 150", service.holder(150), Some(1));
}

#[test]
fn too_late_to_renew() {
    let mut service = LeaseService::new(100);
    service.acquire(0, 1);
    check!("at the end time", service.renew(100, 1, 1), None);
    check!("and it stays free", service.holder(100), None);
}

#[test]
fn the_old_holder_cannot_renew() {
    let mut service = LeaseService::new(100);
    service.acquire(0, 1);
    service.acquire(100, 2);
    check!("client 1, old token", service.renew(101, 1, 1), None);
    check!("client 1, new token", service.renew(101, 1, 2), None);
    check!("client 2 is untouched", service.holder(199), Some(2));
}
