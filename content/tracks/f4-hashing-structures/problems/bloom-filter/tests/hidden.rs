use solution::*;

#[test]
fn sizing_is_ceil_and_round() {
    check!(r#"n = 1, tiny"#, { let b = Bloom::new(1, 0.5); (b.bit_count(), b.hash_count()) }, (2, 1));
}

#[test]
fn zero_expected_does_not_panic() {
    check!(r#"n = 0 is treated as 1"#, { let mut b = Bloom::new(0, 0.01); b.insert(&1u8); b.contains(&1u8) }, true);
}

#[test]
fn str_and_string_agree() {
    check!(r#"same hash for both"#, { let mut b = Bloom::new(100, 0.01); b.insert("key"); b.contains(&String::from("key")) }, true);
}

#[test]
fn union_combines() {
    check!(r#"membership of both"#, { let mut a = Bloom::new(100, 0.01); let mut b = Bloom::new(100, 0.01); a.insert("x"); b.insert("y"); let r = a.union_with(&b); (r, a.contains("x"), a.contains("y"), a.len()) }, (Ok(()), true, true, 2));
}

#[test]
fn union_needs_the_same_shape() {
    check!(r#"different sizes"#, { let mut a = Bloom::new(100, 0.01); let b = Bloom::new(500, 0.01); a.union_with(&b) }, Err(()));
}

#[test]
fn fp_estimate_grows_with_load() {
    check!(r#"more items, more error"#, { let mut b = Bloom::new(1000, 0.01); for i in 0..100u32 { b.insert(&i); } let lo = b.estimated_fp_rate(); for i in 100..1000u32 { b.insert(&i); } (lo < b.estimated_fp_rate(), b.estimated_fp_rate() < 0.02) }, (true, true));
}

#[test]
fn no_false_negatives_ever() {
    let mut b = Bloom::new(5000, 0.01);
    for i in 0..5000u32 {
        b.insert(&format!("member-{i}"));
    }
    let missing = (0..5000u32).filter(|i| !b.contains(&format!("member-{i}"))).count();
    check!("every inserted key is reported present", missing, 0);
}

#[test]
fn measured_false_positive_rate_is_near_the_target() {
    let mut b = Bloom::new(1000, 0.01);
    for i in 0..1000u32 {
        b.insert(&format!("in-{i}"));
    }
    let trials = 50_000u32;
    let fps = (0..trials).filter(|i| b.contains(&format!("out-{i}"))).count();
    let rate = fps as f64 / trials as f64;
    assert!(rate < 0.02, "false positive rate {rate} is more than twice the 1% target");
    assert!(rate > 0.002, "false positive rate {rate} is suspiciously low: is contains checking all k bits?");
}

#[test]
fn union_keeps_every_member_of_both() {
    let mut a = Bloom::new(2000, 0.01);
    let mut b = Bloom::new(2000, 0.01);
    for i in 0..1000u32 {
        a.insert(&i);
        b.insert(&(i + 1000));
    }
    a.union_with(&b).unwrap();
    let missing = (0..2000u32).filter(|i| !a.contains(i)).count();
    check!("the union contains all 2000 keys", missing, 0);
}
