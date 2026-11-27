use solution::*;


#[allow(dead_code)]
fn table(cases: &[(&str, &str, bool)]) {
    for &(pattern, name, want) in cases {
        check!(format!("pattern {pattern:?}, name {name:?}"), matches(pattern.as_bytes(), name.as_bytes()), want);
    }
}

#[test]
fn stars_at_every_place() {
    table(&[
        ("**", "", true), ("a**b", "ab", true), ("*a", "a", true), ("a*", "a", true), ("*a*", "bab", true), ("*ab", "aab", true),
        ("a*ab", "aab", true), ("a*ab", "ab", false), ("*aab", "aaab", true), ("a?*", "a", false), ("*?", "", false), ("?*?", "ab", true),
        ("*x*y*", "xy", true), ("*x*y*", "yx", false), ("ab*", "a", false),
    ]);
}

#[test]
fn the_inside_of_a_set() {
    table(&[
        ("[]", "a", false), ("[]", "", false), ("[^]", "a", true), ("[^]", "", false), ("x[]y", "xy", false),
        ("[a]", "a", true), ("[a-a]", "a", true), ("[a-c]", "d", false), ("[a-ce]", "e", true), ("[-a]", "-", true),
        ("[a-]", "-", true), ("[a-]", "a", true), ("[a-]", "b", false), ("[*]", "*", true), ("[*]", "x", false), ("[?]", "x", false),
        (r"[\]]", "]", true), (r"[\]]", "\\", false), (r"[\^a]", "^", true), (r"[\-]", "-", true), (r"[a\-c]", "b", false),
        ("[a^]", "^", true), ("[^^]", "^", false), ("[[]", "[", true), ("a]", "a]", true), ("[ab][cd]", "ad", true),
    ]);
}

#[test]
fn what_is_left_unfinished() {
    table(&[
        // a set with no `]` runs to the end of the pattern
        ("[abc", "b", true), ("[abc", "d", false), ("x[ab", "xa", true), ("[^ab", "c", true), ("[a-", "-", true), ("[", "a", false),
        ("[^", "a", true),
        // a lone backslash at the end is itself
        ("a\\", "a\\", true), ("a\\", "a", false),
        ("[ab\\", "\\", true),
    ]);
}

#[test]
fn the_whole_name_must_fit() {
    table(&[("a", "ab", false), ("a", "ba", false), ("?", "", false), ("[a]", "aa", false), ("", "x", false), ("abc", "abc", true), ("a?c", "abbc", false)]);
}

#[test]
fn bytes_that_are_not_text() {
    check!("any byte", matches(b"?", &[0xff]), true);
    check!("a range of bytes", matches(&[b'[', 0x80, b'-', 0xff, b']'], &[0xc0]), true);
    check!("a zero byte", matches(b"a*", &[b'a', 0, 0]), true);
    check!("one character of two bytes is two bytes", matches(b"?", "é".as_bytes()), false);
}

/// The slow and obvious matcher over parsed tokens.
#[allow(dead_code)]
#[derive(Debug, Clone)]
enum Piece {
    Star,
    Any,
    Literal(u8),
    Set(bool, Vec<(u8, u8)>),
}

#[allow(dead_code)]
fn slow(pieces: &[Piece], name: &[u8]) -> bool {
    match pieces.first() {
        None => name.is_empty(),
        Some(Piece::Star) => (0..=name.len()).any(|i| slow(&pieces[1..], &name[i..])),
        Some(piece) => {
            let Some(&c) = name.first() else { return false };
            let fits = match piece {
                Piece::Literal(l) => *l == c,
                Piece::Set(negated, ranges) => ranges.iter().any(|&(a, b)| a <= c && c <= b) != *negated,
                _ => true,
            };
            fits && slow(&pieces[1..], &name[1..])
        }
    }
}

#[test]
fn random_patterns_match_a_slow_matcher() {
    let mut rng = anneal_prelude::Rng::new(0x3104);
    for _ in 0..20_000 {
        let (mut text, mut pieces) = (String::new(), Vec::new());
        for _ in 0..rng.below(6) {
            match rng.below(6) {
                0 => {
                    text.push('*');
                    pieces.push(Piece::Star);
                }
                1 => {
                    text.push('?');
                    pieces.push(Piece::Any);
                }
                2 => {
                    let negated = rng.bool();
                    text.push('[');
                    if negated {
                        text.push('^');
                    }
                    let mut ranges = Vec::new();
                    for _ in 0..rng.below(3) {
                        let a = *rng.pick(b"abc");
                        if rng.bool() {
                            let b = *rng.pick(b"abc");
                            text.push(a as char);
                            text.push('-');
                            text.push(b as char);
                            ranges.push((a.min(b), a.max(b)));
                        } else {
                            text.push(a as char);
                            ranges.push((a, a));
                        }
                    }
                    text.push(']');
                    pieces.push(Piece::Set(negated, ranges));
                }
                3 => {
                    let c = *rng.pick(b"*?[a\\");
                    text.push('\\');
                    text.push(c as char);
                    pieces.push(Piece::Literal(c));
                }
                _ => {
                    let c = *rng.pick(b"abc");
                    text.push(c as char);
                    pieces.push(Piece::Literal(c));
                }
            }
        }
        let len = rng.below(7);
        let name = rng.string(len, "abc*?[\\");
        check!(format!("pattern {text:?}, name {name:?}"), matches(text.as_bytes(), name.as_bytes()), slow(&pieces, name.as_bytes()));
    }
}

#[test]
fn many_stars_and_a_long_name() {
    // trying every way to split the name among the stars cannot finish
    let name = vec![b'a'; 100_000];
    for pattern in ["a*".repeat(30) + "b", "*a".repeat(30) + "b", "*?*?*?*?*?*?*?*?*b".to_string(), "*[ab]".repeat(20) + "c"] {
        check!(format!("{pattern:?} against 100000 letters"), matches(pattern.as_bytes(), &name), false);
    }
    let pattern = "*a".repeat(1000);
    check!("a thousand stars that do match", matches(pattern.as_bytes(), &name), true);
}

#[test]
fn a_long_literal_tail_after_a_star() {
    // retrying from each position is quadratic in the worst case, which is still fine at these sizes
    let pattern = "*".to_string() + &"a".repeat(100) + "b";
    let name = "a".repeat(50_000);
    check!("no b anywhere", matches(pattern.as_bytes(), name.as_bytes()), false);
}
