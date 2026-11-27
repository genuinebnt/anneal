use solution::*;

fn found(i: &Index, typed: &str) -> Vec<(String, Kind, u8)> {
    i.search(typed, 5, 0).into_iter().map(|s| (s.term, s.kind, s.dist)).collect()
}

fn sample() -> Index {
    let mut i = Index::new();
    i.add(&[("javascript", 15), ("typescript", 10), ("python", 12), ("pytorch", 4), ("rust", 9), ("rustacean", 1)], 0);
    i
}

#[test]
fn inside_a_term() {
    check!("script", found(&sample(), "Script"), vec![("javascript".to_string(), Kind::Infix, 0), ("typescript".to_string(), Kind::Infix, 0)]);
}

#[test]
fn a_swap_is_one_slip() {
    check!("pyhton", found(&sample(), "pyhton"), vec![("python".to_string(), Kind::Typo, 1)]);
}

#[test]
fn what_starts_the_term_comes_first() {
    check!("rust", found(&sample(), "rust"), vec![("rust".to_string(), Kind::Prefix, 0), ("rustacean".to_string(), Kind::Prefix, 0)]);
}

#[test]
fn two_characters_only_start_terms() {
    check!("th", found(&sample(), "th"), vec![]);
}

#[test]
fn the_rule_by_itself() {
    check!("prefix", classify("ru", "rust"), Some((Kind::Prefix, 0)));
    check!("infix", classify("ust", "rust"), Some((Kind::Infix, 0)));
    check!("typo", classify("pyton", "python"), Some((Kind::Typo, 1)));
    check!("the second test", classify("rsut", "rustacean"), None);
    check!("nothing", classify("zzz", "rust"), None);
}
