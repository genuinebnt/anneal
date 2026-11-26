use solution::*;

#[test]
fn none() {
    check!(r#"parts = [], sep = ",""#, join_with(&[], ","), "");
}

#[test]
fn many() {
    check!(r#"100 parts of "xyz", sep = "/""#, { let parts = vec!["xyz"; 100]; let s = join_with(&parts, "/"); (s.len(), s.capacity() == s.len()) }, (399, true));
}

#[test]
fn single_empty_part() {
    check!(r#"parts = [""], sep = "-""#, join_with(&[""], "-"), String::new());
}

#[test]
fn unicode() {
    check!(r#"parts = ["日本", "é"], sep = " → ""#, { let s = join_with(&["日本", "é"], " → "); (s.clone(), s.capacity()) }, ("日本 → é".to_string(), 13));
}

#[test]
fn long_sep() {
    check!(r#"parts = ["a", "b", "c"], sep = "<->""#, join_with(&["a", "b", "c"], "<->"), "a<->b<->c".to_string());
}

#[test]
fn capacity_ten_parts() {
    check!(r#"10 parts of "abc", sep = ", ": (length, capacity, heap allocations)"#, { let (s, seen) = anneal_prelude::allocs(|| join_with(&["abc"; 10], ", ")); (s.len(), s.capacity(), seen.count) }, (48, 48, 1));
}

#[test]
fn no_trailing_sep() {
    check!(r#"parts = ["x", "y"], sep = ";""#, join_with(&["x", "y"], ";"), "x;y".to_string());
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(2308);
    for _ in 0..300 {
        let n = rng.below(6);
        let mut parts = Vec::new();
        for _ in 0..n {
            let len = rng.below(4);
            parts.push(rng.string(len, "ab日"));
        }
        let len = rng.below(3);
        let sep = rng.string(len, ",→");
        let refs: Vec<&str> = parts.iter().map(|p| p.as_str()).collect();
        let mut want = String::new();
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                want += &sep;
            }
            want += p;
        }
        let got = join_with(&refs, &sep);
        check!(format!("parts = {parts:?}, sep = {sep:?}"), (got.capacity(), got), (want.len(), want));
    }
}

#[test]
fn scale_500k_parts() {
    let parts = vec!["ab"; 500_000];
    let s = join_with(&parts, ",");
    check!("parts = [\"ab\"; 500000], sep = \",\"", (s.len(), s.capacity()), (1_499_999, 1_499_999));
}
