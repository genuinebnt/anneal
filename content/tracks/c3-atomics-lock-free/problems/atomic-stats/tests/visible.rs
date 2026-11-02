use solution::*;

#[test]
fn empty() {
    check!(r#"nothing recorded"#, Stats::new().snapshot(), None);
}

#[test]
fn one_value() {
    check!(r#"record 7"#, { let s = Stats::new(); s.record(7); s.snapshot() }, Some(Snapshot { count: 1, sum: 7, min: 7, max: 7 }));
}

#[test]
fn several() {
    check!(r#"record 5, 2, 9"#, { let s = Stats::new(); for v in [5, 2, 9] { s.record(v); } s.snapshot() }, Some(Snapshot { count: 3, sum: 16, min: 2, max: 9 }));
}

#[test]
fn zero_is_a_value() {
    check!(r#"record 0, 4"#, { let s = Stats::new(); s.record(0); s.record(4); s.snapshot() }, Some(Snapshot { count: 2, sum: 4, min: 0, max: 4 }));
}

#[test]
fn take_resets() {
    check!(r#"record 3, take, take"#, { let s = Stats::new(); s.record(3); (s.take(), s.take(), s.snapshot()) }, (Some(Snapshot { count: 1, sum: 3, min: 3, max: 3 }), None, None));
}
