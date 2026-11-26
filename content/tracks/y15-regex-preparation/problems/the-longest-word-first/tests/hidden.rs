use solution::*;

/// Exactly this text.
#[allow(dead_code)]
struct Exact(&'static str);
impl Pattern for Exact {
    fn start(&self) -> u32 {
        0
    }
    fn step(&self, state: u32, c: char) -> Option<u32> {
        (self.0.chars().nth(state as usize) == Some(c)).then_some(state + 1)
    }
    fn accepts(&self, state: u32) -> bool {
        state as usize == self.0.chars().count()
    }
}

/// One or more characters that pass the test.
#[allow(dead_code)]
struct Many(fn(char) -> bool);
impl Pattern for Many {
    fn start(&self) -> u32 {
        0
    }
    fn step(&self, _: u32, c: char) -> Option<u32> {
        (self.0)(c).then_some(1)
    }
    fn accepts(&self, state: u32) -> bool {
        state == 1
    }
}

/// Digits, or digits, a dot and more digits. `1.` on its own is not a number.
#[allow(dead_code)]
struct Number;
impl Pattern for Number {
    fn start(&self) -> u32 {
        0
    }
    fn step(&self, state: u32, c: char) -> Option<u32> {
        match (state, c) {
            (0 | 1, '0'..='9') => Some(1),
            (1, '.') => Some(2),
            (2 | 3, '0'..='9') => Some(3),
            _ => None,
        }
    }
    fn accepts(&self, state: u32) -> bool {
        state == 1 || state == 3
    }
}

#[allow(dead_code)]
fn cut(patterns: &[(&dyn Pattern, bool)], text: &str) -> Result<Vec<(usize, String)>, LexError> {
    let rules: Vec<Rule> = patterns.iter().map(|&(pattern, skip)| Rule { pattern, skip }).collect();
    let chars: Vec<char> = text.chars().collect();
    lex(&rules, text).map(|tokens| tokens.iter().map(|t| (t.rule, chars[t.start..t.end].iter().collect())).collect())
}

#[allow(dead_code)]
fn words(list: &[(usize, &str)]) -> Result<Vec<(usize, String)>, LexError> {
    Ok(list.iter().map(|&(r, w)| (r, w.to_string())).collect())
}

use anneal_prelude::Rng;

/// Accepts the empty text and any run of `x`.
#[allow(dead_code)]
struct MaybeX;
impl Pattern for MaybeX {
    fn start(&self) -> u32 {
        0
    }
    fn step(&self, _: u32, c: char) -> Option<u32> {
        (c == 'x').then_some(0)
    }
    fn accepts(&self, _: u32) -> bool {
        true
    }
}

#[test]
fn an_empty_word_is_never_taken() {
    check!("xxy", cut(&[(&MaybeX, false)], "xxy"), Err(LexError::NoToken(2)));
    check!("no rules at all", cut(&[], "a"), Err(LexError::NoToken(0)));
    check!("no rules, no text", cut(&[], ""), words(&[]));
}

#[test]
fn a_longer_later_rule_beats_a_shorter_earlier_one() {
    let (a, b) = (Exact("="), Exact("=="));
    check!("===", cut(&[(&a, false), (&b, false)], "==="), words(&[(1, "=="), (0, "=")]));
}

#[test]
fn failing_after_reading_far_ahead() {
    let (long, a, b) = (Exact("aaaaab"), Exact("a"), Exact("c"));
    check!("aaaaac", cut(&[(&long, false), (&a, false), (&b, false)], "aaaaac"), words(&[(1, "a"), (1, "a"), (1, "a"), (1, "a"), (1, "a"), (2, "c")]));
    check!("aaaaax", cut(&[(&long, false), (&a, false)], "aaaaax"), Err(LexError::NoToken(5)));
}

#[test]
fn skipped_words_still_take_part_in_longest() {
    let (comment, minus) = (Exact("--"), Exact("-"));
    check!("---", cut(&[(&minus, false), (&comment, true)], "---"), words(&[(0, "-")]));
}

#[test]
fn random_rules_agree_with_trying_every_length() {
    let pool = ["a", "b", "ab", "ba", "aab", "abab", "bbb", "é", "aé"];
    let mut rng = Rng::new(1506);
    for _ in 0..800 {
        let k = rng.int(1, 4) as usize;
        let picked: Vec<&str> = (0..k).map(|_| *rng.pick(&pool)).collect();
        let exact: Vec<Exact> = picked.iter().map(|&s| Exact(s)).collect();
        let rules: Vec<(&dyn Pattern, bool)> = exact.iter().map(|e| (e as &dyn Pattern, false)).collect();
        let len = rng.below(10);
        let text = rng.string(len, "abé");
        let chars: Vec<char> = text.chars().collect();
        let mut want = Ok(Vec::new());
        let mut start = 0;
        while start < chars.len() {
            let found = (start + 1..=chars.len()).rev().find_map(|end| {
                let word: String = chars[start..end].iter().collect();
                picked.iter().position(|&p| p == word).map(|rule| (rule, word, end))
            });
            let Some((rule, word, end)) = found else {
                want = Err(LexError::NoToken(start));
                break;
            };
            if let Ok(list) = &mut want {
                list.push((rule, word));
            }
            start = end;
        }
        check!(format!("rules {picked:?} on {text:?}"), cut(&rules, &text), want);
    }
}

#[test]
fn a_long_text_of_short_words() {
    let (name, space) = (Many(|c| c.is_alphabetic()), Exact(" "));
    let text = "ab ".repeat(100_000);
    let got = cut(&[(&name, false), (&space, true)], &text);
    check!("100 000 words", got.map(|w| w.len()), Ok(100_000));
}

#[test]
fn everything_skipped_gives_no_words() {
    let space = Many(|c| c.is_whitespace());
    check!("only spaces", cut(&[(&space, true)], " \n\t "), words(&[]));
}

#[test]
fn an_unfinished_word_at_the_end() {
    let dot = Exact(".");
    check!("a number that stops after the dot", cut(&[(&Number, false), (&dot, false)], "12."), words(&[(0, "12"), (1, ".")]));
    check!("with no rule for the dot", cut(&[(&Number, false)], "12."), Err(LexError::NoToken(2)));
}

#[test]
fn the_error_position_counts_characters() {
    let name = Many(|c| c.is_alphabetic());
    check!("ééé!", cut(&[(&name, false)], "ééé!"), Err(LexError::NoToken(3)));
}
