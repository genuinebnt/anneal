use anneal_prelude::Rng;
use solution::*;

/// The set of the single item the pattern parses to, asked about each character of `probe`.
#[allow(dead_code)]
fn members(pattern: &str, probe: &str) -> Result<String, ParseError> {
    match parse(pattern)? {
        Ast::Class(class) => Ok(probe.chars().filter(|&c| class.matches(c)).collect()),
        other => panic!("{pattern:?} should be one set of characters, not {other:?}"),
    }
}

#[allow(dead_code)]
const PROBE: &str = "abcxyzAZ059_ \t\n\r\u{b}\u{c}-^]\\[.*dswé";

#[allow(dead_code)]
fn of(chars: &str) -> Result<String, ParseError> {
    Ok(PROBE.chars().filter(|c| chars.contains(*c)).collect())
}

#[allow(dead_code)]
fn all_but(chars: &str) -> Result<String, ParseError> {
    Ok(PROBE.chars().filter(|c| !chars.contains(*c)).collect())
}

#[test]
fn escapes_as_range_ends() {
    check!(r"[\t-\r]", members(r"[\t-\r]", PROBE), of("\t\n\r\u{b}\u{c}"));
    check!(r"[\[-\]]", members(r"[\[-\]]", PROBE), of("[\\]"));
    check!("[+--]", members("[+--]", ",-.+"), Ok(",-+".to_string()));
}

#[test]
fn a_shorthand_cannot_be_a_range_end() {
    check!(r"[\d-z]", parse(r"[\d-z]"), Err(ParseError::BadRange(1)));
    check!(r"[a-\w]", parse(r"[a-\w]"), Err(ParseError::BadRange(1)));
    check!(r"xy[0a-\s]", parse(r"xy[0a-\s]"), Err(ParseError::BadRange(4)));
    check!(r"[\d-]", members(r"[\d-]", PROBE), of("059-"));
}

#[test]
fn special_characters_are_ordinary_inside_sets() {
    check!("[.*+?|()$[]", members("[.*+?|()$[]", ".*+?|()$[a"), Ok(".*+?|()$[".to_string()));
    check!("[]a is an empty set and then a", match parse("[]a") {
        Ok(Ast::Cat(items)) => match items.as_slice() {
            [Ast::Class(none), Ast::Class(a)] => Some((none.matches('a'), none.matches(']'), a.matches('a'))),
            _ => None,
        },
        _ => None,
    }, Some((false, false, true)));
    check!("] alone is ordinary", members("]", PROBE), of("]"));
}

#[test]
fn error_positions() {
    check!("[", parse("["), Err(ParseError::UnclosedClass(0)));
    check!("[^", parse("[^"), Err(ParseError::UnclosedClass(0)));
    check!("[a-", parse("[a-"), Err(ParseError::UnclosedClass(0)));
    check!(r"[a\]", parse(r"[a\]"), Err(ParseError::UnclosedClass(0)));
    check!("([a)", parse("([a)"), Err(ParseError::UnclosedClass(1)));
    check!("[a\\", parse("[a\\"), Err(ParseError::TrailingEscape(2)));
    check!(r"[ab\q]", parse(r"[ab\q]"), Err(ParseError::BadEscape(3)));
    check!(r"[a-\Q]", parse(r"[a-\Q]"), Err(ParseError::BadEscape(3)));
    check!("é[é-a]", parse("é[é-a]"), Err(ParseError::BadRange(2)));
    check!("[z-a][", parse("[z-a]["), Err(ParseError::BadRange(1)));
    check!(r"\1", parse(r"\1"), Err(ParseError::BadEscape(0)));
}

#[test]
fn any_other_symbol_may_be_escaped() {
    for c in "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~ é€".chars() {
        let pattern = format!("\\{c}");
        let only_itself = match parse(&pattern) {
            Ok(Ast::Class(set)) => set.matches(c) && !"a\\é1".chars().any(|other| other != c && set.matches(other)),
            _ => false,
        };
        check!(format!("{pattern:?} is just {c:?}"), only_itself, true);
    }
}

#[test]
fn quantifiers_and_groups_work_on_sets() {
    check!("[ab]+", matches!(parse("[ab]+"), Ok(Ast::Plus(_))), true);
    check!(r"(\d|x)*", matches!(parse(r"(\d|x)*"), Ok(Ast::Star(_))), true);
    check!("brackets inside a set do not open groups", parse(&("[".to_string() + &"(".repeat(500) + "]")).is_ok(), true);
}

#[test]
fn step_one_still_holds() {
    check!("(ab", parse("(ab"), Err(ParseError::UnclosedGroup(0)));
    check!("a)", parse("a)"), Err(ParseError::UnmatchedClose(1)));
    check!("[a]|*", parse("[a]|*"), Err(ParseError::NothingToRepeat(4)));
}

#[test]
fn random_sets_agree_with_a_list_of_members() {
    let mut rng = Rng::new(1602);
    let pieces: &[(&str, &str)] = &[
        ("a", "a"), ("c", "c"), ("x", "x"), ("a-c", "abc"), ("x-z", "xyz"), ("0-5", "012345"),
        (r"\d", "0123456789"), (r"\n", "\n"), (r"\]", "]"), (r"\-", "-"), (".", "."), ("[", "["),
    ];
    let probe = "abcdwxyz0569\n]-.[^";
    for _ in 0..1500 {
        let negated = rng.bool();
        let mut text = String::from(if negated { "[^" } else { "[" });
        let mut inside = String::new();
        for _ in 0..rng.below(5) {
            let &(source, set) = rng.pick(pieces);
            text.push_str(source);
            inside.push_str(set);
        }
        text.push(']');
        let want: String = probe.chars().filter(|&c| inside.contains(c) != negated).collect();
        check!(format!("{text:?}"), members(&text, probe), Ok(want));
    }
}
