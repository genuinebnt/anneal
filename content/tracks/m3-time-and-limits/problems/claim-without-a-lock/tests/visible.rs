use solution::*;

use std::ops::Range;

#[test]
fn pieces_do_not_overlap() {
    let r = Region::new(10);
    check!("4", r.claim(4), Some(0..4));
    check!("3", r.claim(3), Some(4..7));
    check!("free", r.free(), 3);
}

#[test]
fn a_refused_claim_changes_nothing() {
    let r = Region::new(5);
    check!("too big", r.claim(6), None);
    check!("exactly all", r.claim(5), Some(0..5));
    check!("empty", r.claim(0), Some(5..5));
    check!("one more", r.claim(1), None);
}

#[test]
fn choosing_a_size() {
    let r = Region::new(10);
    check!("half of what is free", r.claim_with(|free| free / 2), Some(0..5));
    check!("half again", r.claim_with(|free| free / 2), Some(5..7));
    check!("more than offered", r.claim_with(|free| free + 1), None);
}

#[test]
fn close_reports_what_was_handed_out() {
    let r = Region::new(10);
    let _: Option<Range<usize>> = r.claim(3);
    check!("close", r.close(), 3);
    check!("close again", r.close(), 3);
    check!("claim after close", r.claim(1), None);
    check!("even nothing", r.claim(0), None);
    check!("free", r.free(), 0);
}

#[test]
fn somebody_claims_in_the_middle() {
    let r = Region::new(10);
    let mut inner = None;
    let outer = r.claim_with(|free| {
        if inner.is_none() {
            inner = r.claim(4);
        }
        free / 2
    });
    check!("the claim made inside", inner, Some(0..4));
    check!("half of the 6 that were then free", outer, Some(4..7));
}

#[test]
fn somebody_closes_in_the_middle() {
    let r = Region::new(10);
    r.claim(2).unwrap();
    let mut total = None;
    let got = r.claim_with(|_| {
        total.get_or_insert_with(|| r.close());
        3
    });
    check!("closed first, so the claim fails", (total, got), (Some(2), None));
}
