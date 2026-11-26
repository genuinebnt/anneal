use solution::*;

#[allow(dead_code)]
fn find(pattern: &str, text: &str) -> Option<(usize, usize)> {
    Regex::new(pattern).expect("a valid pattern").find(text)
}

#[test]
fn leftmost_then_longest() {
    check!(r"\d+ in 'ab 123 45'", find(r"\d+", "ab 123 45"), Some((3, 6)));
    check!("a|ab|abc in xabcx", find("a|ab|abc", "xabcx"), Some((1, 4)));
    check!("leftmost beats longer", find("a+|b+", "xabbbbb"), Some((1, 2)));
}

#[test]
fn no_match() {
    check!("x in abc", find("x", "abc"), None);
    check!("an empty text", find("a", ""), None);
}

#[test]
fn empty_matches() {
    check!("x* in abc", find("x*", "abc"), Some((0, 0)));
    check!("$ in abc", find("$", "abc"), Some((3, 3)));
    check!("the empty pattern in the empty text", find("", ""), Some((0, 0)));
}

#[test]
fn anchors_mean_the_whole_text() {
    check!("^b in ab", find("^b", "ab"), None);
    check!("c$ in abc", find("c$", "abc"), Some((2, 3)));
    check!("a$ in aab a", find("a$", "aaba"), Some((3, 4)));
}

#[test]
fn positions_count_characters() {
    check!("é+ in naééz", find("é+", "naééz"), Some((2, 4)));
}

#[test]
fn the_whole_text_still_works() {
    let re = Regex::new("(ab)+").unwrap();
    check!("is_match", (re.is_match("abab"), re.is_match("xabab"), re.find("xabab")), (true, false, Some((1, 5))));
    check!("errors pass through", Regex::new("a(").err(), Some(ParseError::UnclosedGroup(1)));
}
