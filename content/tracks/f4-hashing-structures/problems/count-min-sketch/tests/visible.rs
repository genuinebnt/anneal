use solution::*;

#[test]
fn sizing() {
    check!(r#"eps 0.01, delta 0.01"#, { let c = CountMin::with_error(0.01, 0.01); (c.width(), c.depth()) }, (272, 5));
}

#[test]
fn exact_when_there_is_room() {
    check!(r#"few keys, wide sketch"#, { let mut c = CountMin::new(1000, 4); c.add("a", 3); c.add("b", 5); c.add("a", 2); (c.estimate("a"), c.estimate("b"), c.estimate("never"), c.total()) }, (5, 5, 0, 10));
}

#[test]
fn never_undercounts_in_a_tiny_sketch() {
    check!(r#"width 3 forces collisions"#, { let mut c = CountMin::new(3, 2); for i in 0..50u32 { c.add(&i, 1 + (i % 3) as u64); } (0..50u32).all(|i| c.estimate(&i) >= 1 + (i % 3) as u64) }, true);
}

#[test]
fn merge_adds() {
    check!(r#"two streams"#, { let mut a = CountMin::new(200, 4); let mut b = CountMin::new(200, 4); a.add("x", 4); b.add("x", 6); b.add("y", 1); a.merge(&b).unwrap(); (a.estimate("x"), a.estimate("y"), a.total()) }, (10, 1, 11));
}

#[test]
fn merge_needs_equal_shape() {
    check!(r#"different width"#, { let mut a = CountMin::new(10, 3); let b = CountMin::new(11, 3); a.merge(&b) }, Err(()));
}
