use solution::*;

#[test]
fn none() {
    check!(r#"words = [], sep = "-""#, join_words(&[], "-"), String::new());
}

#[test]
fn empty_sep() {
    check!(r#"words = ["a", "b"], sep = """#, join_words(&["a", "b"], ""), "ab".to_string());
}

#[test]
fn unicode_sep() {
    check!(r#"words = ["x", "y"], sep = " → ""#, join_words(&["x", "y"], " → "), "x → y".to_string());
}

#[test]
fn single_empty_word() {
    check!(r#"words = [""], sep = "-""#, join_words(&[""], "-"), String::new());
}

#[test]
fn unicode_words() {
    check!(r#"words = ["日本", "é"], sep = "/""#, join_words(&["日本", "é"], "/"), "日本/é".to_string());
}

#[test]
fn long_sep() {
    check!(r#"words = ["a", "b", "c"], sep = "<->""#, join_words(&["a", "b", "c"], "<->"), "a<->b<->c".to_string());
}

#[test]
fn one_allocation() {
    check!(r#"words = ["abc"; 10], sep = ", ": (length, capacity, heap allocations)"#, { let (s, seen) = anneal_prelude::allocs(|| join_words(&["abc"; 10], ", ")); (s.len(), s.capacity(), seen.count) }, (48, 48, 1));
}

#[test]
fn exact_capacity_unicode() {
    check!(r#"words = ["é", "日"], sep = " → ": capacity is exactly the length"#, { let s = join_words(&["é", "日"], " → "); (s.len(), s.capacity()) }, (10, 10));
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(2206);
    for _ in 0..300 {
        let n = rng.below(6);
        let mut words = Vec::new();
        for _ in 0..n {
            let len = rng.below(4);
            words.push(rng.string(len, "ab日"));
        }
        let len = rng.below(3);
        let sep = rng.string(len, ",→");
        let refs: Vec<&str> = words.iter().map(|w| w.as_str()).collect();
        let mut want = String::new();
        for (i, w) in words.iter().enumerate() {
            if i > 0 {
                want += &sep;
            }
            want += w;
        }
        let got = join_words(&refs, &sep);
        check!(format!("words = {words:?}, sep = {sep:?}"), (got.capacity(), got), (want.len(), want));
    }
}

#[test]
fn scale_500k_words() {
    let words = vec!["ab"; 500_000];
    let s = join_words(&words, ",");
    check!("words = [\"ab\"; 500000], sep = \",\"", (s.len(), &s[..5], &s[s.len() - 5..]), (1_499_999, "ab,ab", "ab,ab"));
}
