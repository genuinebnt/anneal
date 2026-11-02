use solution::*;

#[test]
fn alternation_binds_loosest() {
    check!(r#"ab|cd is (ab)|(cd)"#, (Regex::new("ab|cd").unwrap().is_match("ab"), Regex::new("ab|cd").unwrap().is_match("cd"), Regex::new("ab|cd").unwrap().is_match("abcd"), Regex::new("ab|cd").unwrap().is_match("ad")), (true, true, false, false));
}

#[test]
fn postfix_binds_tightest() {
    check!(r#"ab* is a(b*)"#, (Regex::new("ab*").unwrap().is_match("abbb"), Regex::new("ab*").unwrap().is_match("abab"), Regex::new("(ab)*").unwrap().is_match("abab")), (true, false, true));
}

#[test]
fn nested_groups_and_alternatives() {
    check!(r#"(a|b)*abb"#, (Regex::new("(a|b)*abb").unwrap().is_match("aababb"), Regex::new("(a|b)*abb").unwrap().is_match("abab"), Regex::new("(ab|a)(bc|c)?").unwrap().is_match("abc"), Regex::new("(ab|a)(bc|c)?").unwrap().is_match("ac")), (true, false, true, true));
}

#[test]
fn the_longer_alternative_is_found() {
    check!(r#"needs real nondeterminism"#, (Regex::new("(foo|foobar)baz").unwrap().is_match("foobarbaz"), Regex::new("(a|ab)(c|bcd)").unwrap().is_match("abcd")), (true, true));
}

#[test]
fn empty_pieces() {
    check!(r#"a||b, (), (a*)*, (a*)+, (|a)+"#, (Regex::new("a||b").unwrap().is_match(""), Regex::new("()").unwrap().is_match(""), Regex::new("(a*)*").unwrap().is_match("aaa"), Regex::new("(a*)+").unwrap().is_match(""), Regex::new("(|a)+").unwrap().is_match("aa"), Regex::new("a()b").unwrap().is_match("ab")), (true, true, true, true, true, true));
}

#[test]
fn escapes() {
    check!(r#"a\.c, \*, \\"#, (Regex::new("a\\.c").unwrap().is_match("a.c"), Regex::new("a\\.c").unwrap().is_match("abc"), Regex::new("\\*").unwrap().is_match("*"), Regex::new("\\\\").unwrap().is_match("\\"), Regex::new("\\(a\\)").unwrap().is_match("(a)")), (true, false, true, true, true));
}

#[test]
fn every_parse_error() {
    check!(r#"paren and quantifier mistakes"#, (Regex::new("a**").err(), Regex::new("a|*b").err(), Regex::new("(*)").err(), Regex::new(")").err(), Regex::new("a)b").err(), Regex::new("((a)").err(), Regex::new("a+?").err(), Regex::new("").err()), (Some(ParseError::NothingToRepeat), Some(ParseError::NothingToRepeat), Some(ParseError::NothingToRepeat), Some(ParseError::UnbalancedParen), Some(ParseError::UnbalancedParen), Some(ParseError::UnbalancedParen), Some(ParseError::NothingToRepeat), None));
}

#[test]
fn find_leftmost_then_longest() {
    check!(r#"positions"#, (Regex::new("a*").unwrap().find("baaa"), Regex::new("a+").unwrap().find("baaa"), Regex::new("cat|category").unwrap().find("my category"), Regex::new("x").unwrap().find("abc"), Regex::new("a*").unwrap().find("")), (Some((0, 0)), Some((1, 4)), Some((3, 11)), None, Some((0, 0))));
}

#[test]
fn unicode_is_by_char() {
    check!(r#"indices are char indices"#, (Regex::new("caf.").unwrap().is_match("caf\u{e9}"), Regex::new("\u{e9}+").unwrap().find("a\u{e9}\u{e9}b")), (true, Some((1, 3))));
}

fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}

        fn m(pat: &str, text: &str) -> bool {
            Regex::new(pat).unwrap().is_match(text)
        }

        #[test]
        fn no_catastrophic_backtracking() {
            let ok = within(5000, || {
                let text = "a".repeat(3000);
                let nested = Regex::new("(a*)*b").unwrap();
                let alt = Regex::new("(a|aa)+$").unwrap();
                let many = Regex::new("(a?){30}a{30}").unwrap();
                (nested.is_match(&text), alt.is_match(&text), many.is_match(&text), Regex::new("(x+x+)+y").unwrap().is_match(&"x".repeat(3000)))
            });
            check!("finishes, and the answers are right", ok, Some((false, false, false, false)));
        }

        #[test]
        fn long_inputs_and_programs() {
            let ok = within(5000, || {
                let pat = "a".repeat(500) + &"b?".repeat(500);
                let re = Regex::new(&pat).unwrap();
                (re.is_match(&"a".repeat(500)), re.is_match(&"a".repeat(499)), re.is_match(&("a".repeat(500) + &"b".repeat(500))), re.find(&"a".repeat(600)))
            });
            check!("a 1000-instruction program", ok, Some((true, false, true, Some((0, 500)))));
        }

        #[test]
        fn matches_a_hand_written_model_on_small_inputs() {
            // every string over {a, b} up to length 6 against patterns whose language is easy to state
            let mut all: Vec<String> = vec![String::new()];
            for len in 1..=6 {
                for n in 0..(1u32 << len) {
                    all.push((0..len).map(|i| if n >> i & 1 == 0 { 'a' } else { 'b' }).collect());
                }
            }
            for s in &all {
                check!(format!("(a|b)*abb on {s:?}"), m("(a|b)*abb", s), s.ends_with("abb"));
                check!(format!("a*b* on {s:?}"), m("a*b*", s), { let t = s.trim_start_matches('a'); t.chars().all(|c| c == 'b') });
                check!(format!("(ab)*|b+ on {s:?}"), m("(ab)*|b+", s), { (s.len() % 2 == 0 && s.as_bytes().chunks(2).all(|c| c == b"ab")) || (!s.is_empty() && s.chars().all(|c| c == 'b')) });
                check!(format!("(a|b)(a|b)? on {s:?}"), m("(a|b)(a|b)?", s), s.len() == 1 || s.len() == 2);
            }
        }
