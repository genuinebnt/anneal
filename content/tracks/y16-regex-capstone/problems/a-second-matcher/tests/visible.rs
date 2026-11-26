use solution::*;

#[allow(dead_code)]
const PLENTY: u64 = 2_000_000;

#[allow(dead_code)]
fn bm(pattern: &str, text: &str, limit: u64) -> Result<bool, StepLimit> {
    backtrack_is_match(&parse(pattern).expect("a valid pattern"), text, limit)
}

#[allow(dead_code)]
fn bf(pattern: &str, text: &str, limit: u64) -> Result<Option<(usize, usize)>, StepLimit> {
    backtrack_find(&parse(pattern).expect("a valid pattern"), text, limit)
}

#[test]
fn a_hand_table() {
    check!("a(b|c)*d", bm("a(b|c)*d", "abcbcd", PLENTY), Ok(true));
    check!("a(b|c)*d without d", bm("a(b|c)*d", "abcbc", PLENTY), Ok(false));
    check!("^ab?$", bm("^ab?$", "a", PLENTY), Ok(true));
    check!("a^b", bm("a^b", "ab", PLENTY), Ok(false));
}

#[test]
fn it_must_be_able_to_go_back() {
    check!("(a|ab)c", bm("(a|ab)c", "abc", PLENTY), Ok(true));
    check!("a*a", bm("a*a", "aaaa", PLENTY), Ok(true));
    check!("(a+b?)*c", bm("(a+b?)*c", "aababc", PLENTY), Ok(true));
}

#[test]
fn find_names_the_longest_from_the_leftmost_start() {
    check!("a|ab|abc in xabcx", bf("a|ab|abc", "xabcx", PLENTY), Ok(Some((1, 4))));
    check!("c$ in abc", bf("c$", "abc", PLENTY), Ok(Some((2, 3))));
    check!("x* in abc", bf("x*", "abc", PLENTY), Ok(Some((0, 0))));
    check!("é+ in naééz", bf("é+", "naééz", PLENTY), Ok(Some((2, 4))));
    check!("no match", bf("z", "abc", PLENTY), Ok(None));
}

#[test]
fn loops_around_nothing_end() {
    check!("(a*)*", bm("(a*)*", "aaa", PLENTY), Ok(true));
    check!("()*x", bm("()*x", "y", PLENTY), Ok(false));
    check!("(|a)+b", bm("(|a)+b", "aab", PLENTY), Ok(true));
}

#[test]
fn it_gives_up_on_the_famous_pattern() {
    check!("short input is fine", bm("(a*)*b", "aaaab", 1_000_000), Ok(true));
    check!("40 a's are too much", bm("(a*)*b", &"a".repeat(40), 1_000_000), Err(StepLimit));
}
