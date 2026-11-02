use solution::*;

#[test]
fn literals_must_match_whole_text() {
    check!(r#"full match, not a prefix"#, (Regex::new("abc").unwrap().is_match("abc"), Regex::new("abc").unwrap().is_match("abcd"), Regex::new("abc").unwrap().is_match("ab")), (true, false, false));
}

#[test]
fn dot_matches_one_char() {
    check!(r#"a.c"#, (Regex::new("a.c").unwrap().is_match("abc"), Regex::new("a.c").unwrap().is_match("ac"), Regex::new("a.c").unwrap().is_match("abbc")), (true, false, false));
}

#[test]
fn star_plus_quest() {
    check!(r#"the three quantifiers"#, (Regex::new("a*").unwrap().is_match(""), Regex::new("a+").unwrap().is_match(""), Regex::new("a+").unwrap().is_match("aaa"), Regex::new("colou?r").unwrap().is_match("color"), Regex::new("colou?r").unwrap().is_match("colouur")), (true, false, true, true, false));
}

#[test]
fn alternation_and_groups() {
    check!(r#"cat|dog and (ab)+"#, (Regex::new("cat|dog").unwrap().is_match("dog"), Regex::new("cat|dog").unwrap().is_match("cow"), Regex::new("(ab)+").unwrap().is_match("abab"), Regex::new("(ab)+").unwrap().is_match("aba")), (true, false, true, false));
}

#[test]
fn find_is_leftmost_longest() {
    check!(r#"b+ in aabbbc"#, Regex::new("b+").unwrap().find("aabbbc"), Some((2, 5)));
}

#[test]
fn empty_pattern() {
    check!(r#"matches the empty string only"#, (Regex::new("").unwrap().is_match(""), Regex::new("").unwrap().is_match("a")), (true, false));
}

#[test]
fn parse_errors() {
    check!(r#"the three kinds"#, (Regex::new("(a").err(), Regex::new("*a").err(), Regex::new("ab\\").err()), (Some(ParseError::UnbalancedParen), Some(ParseError::NothingToRepeat), Some(ParseError::DanglingEscape)));
}
