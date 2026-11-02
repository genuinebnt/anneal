use solution::*;

#[test]
fn sizing_for_1000_at_one_percent() {
    check!(r#"m and k"#, { let b = Bloom::new(1000, 0.01); (b.bit_count(), b.hash_count()) }, (9586, 7));
}

#[test]
fn sizing_for_100_at_a_tenth() {
    check!(r#"smaller"#, { let b = Bloom::new(100, 0.1); (b.bit_count(), b.hash_count()) }, (480, 3));
}

#[test]
fn inserted_items_are_found() {
    check!(r#"no false negatives on a few"#, { let mut b = Bloom::new(100, 0.01); for w in ["a", "b", "c"] { b.insert(w); } (b.contains("a"), b.contains("b"), b.contains("c"), b.len()) }, (true, true, true, 3));
}

#[test]
fn empty_filter_contains_nothing() {
    check!(r#"fresh"#, { let b = Bloom::new(100, 0.01); (b.contains("x"), b.contains(&42u32), b.is_empty(), b.estimated_fp_rate()) }, (false, false, true, 0.0));
}

#[test]
fn insert_reports_novelty() {
    check!(r#"second insert changes no bit"#, { let mut b = Bloom::new(100, 0.01); let first = b.insert("dup"); let again = b.insert("dup"); (first, again, b.len()) }, (true, false, 2));
}
