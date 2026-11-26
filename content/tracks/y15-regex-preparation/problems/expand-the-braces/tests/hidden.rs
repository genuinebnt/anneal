use solution::*;
use anneal_prelude::Rng;

#[allow(dead_code)]
fn words(list: &[&str]) -> Result<Vec<String>, BraceError> {
    Ok(list.iter().map(|w| w.to_string()).collect())
}

#[test]
fn the_innermost_open_group_is_reported() {
    check!("{a,{b,{c", expand("{a,{b,{c"), Err(BraceError::Unclosed(6)));
    check!("{{x},{", expand("{{x},{"), Err(BraceError::Unclosed(5)));
    check!("a{b{c}", expand("a{b{c}"), Err(BraceError::Unclosed(1)));
}

#[test]
fn the_first_problem_wins() {
    check!("}{", expand("}{"), Err(BraceError::UnmatchedClose(0)));
    check!("{a}}{", expand("{a}}{"), Err(BraceError::UnmatchedClose(3)));
    check!(r"{a\", expand(r"{a\"), Err(BraceError::TrailingEscape(2)));
}

#[test]
fn positions_count_characters() {
    check!("é€}", expand("é€}"), Err(BraceError::UnmatchedClose(2)));
    check!("né{x", expand("né{x"), Err(BraceError::Unclosed(2)));
    check!("{é,ü}n", expand("{é,ü}n"), words(&["én", "ün"]));
}

#[test]
fn depth_limit_is_exact() {
    let ok = format!("{}x{}", "{".repeat(MAX_OPEN), "}".repeat(MAX_OPEN));
    check!("50 groups inside each other", expand(&ok), words(&["x"]));
    let deep = format!("{}x{}", "{".repeat(MAX_OPEN + 1), "}".repeat(MAX_OPEN + 1));
    check!("51 groups inside each other", expand(&deep), Err(BraceError::TooDeep(MAX_OPEN)));
    let flat = "{a}".repeat(500);
    check!("500 groups side by side", expand(&flat).map(|w| w.len()), Ok(1));
}

#[test]
fn a_hundred_thousand_opening_braces_do_not_overflow_the_stack() {
    let text = "{".repeat(100_000);
    check!("100 000 opening braces", expand(&text), Err(BraceError::TooDeep(MAX_OPEN)));
}

#[test]
fn the_word_limit_is_exact() {
    let ten = "{0,1,2,3,4,5,6,7,8,9}";
    check!("10 000 words are fine", expand(&ten.repeat(4)).map(|w| (w.len(), w[1234].clone())), Ok((10_000, "1234".to_string())));
    let over = format!("{{{},x}}", ten.repeat(4));
    check!("10 001 words are too many", expand(&over), Err(BraceError::TooManyWords));
    check!("100 000 words are too many", expand(&ten.repeat(5)).map(|w| w.len()), Err(BraceError::TooManyWords));
}

#[test]
fn an_astronomical_pattern_returns_at_once() {
    check!("{a,b} sixty-four times", expand(&"{a,b}".repeat(64)), Err(BraceError::TooManyWords));
    check!("{a,b} two hundred times", expand(&"{a,b}".repeat(200)), Err(BraceError::TooManyWords));
    let nested = format!("{{{},z}}", "{a,b}".repeat(70));
    check!("the same, inside a group", expand(&nested), Err(BraceError::TooManyWords));
}

#[test]
fn a_syntax_error_beats_the_size_limit() {
    let text = format!("{}}}", "{a,b}".repeat(40));
    check!("too many words, then a stray brace", expand(&text), Err(BraceError::UnmatchedClose(200)));
}

#[test]
fn a_long_plain_pattern_is_one_word() {
    let text = "a,".repeat(50_000);
    check!("100 000 characters of text", expand(&text).map(|w| (w.len(), w[0].len())), Ok((1, 100_000)));
}

/// A random pattern together with the words it must give, built side by side.
#[allow(dead_code)]
fn build(rng: &mut Rng, depth: u32) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut out = vec![String::new()];
    for _ in 0..rng.below(4) {
        let (piece, tails) = if depth > 0 && rng.below(3) == 0 {
            let mut inner = Vec::new();
            let mut tails = Vec::new();
            for _ in 0..rng.int(1, 3) {
                let (t, w) = build(rng, depth - 1);
                inner.push(t);
                tails.extend(w);
            }
            (format!("{{{}}}", inner.join(",")), tails)
        } else {
            let (src, lit) = *rng.pick(&[("a", "a"), ("b", "b"), ("é", "é"), (r"\{", "{"), (r"\,", ","), (r"\\", "\\")]);
            (src.to_string(), vec![lit.to_string()])
        };
        text.push_str(&piece);
        out = out.iter().flat_map(|h| tails.iter().map(move |t| format!("{h}{t}"))).collect();
    }
    (text, out)
}

#[test]
fn random_patterns_give_the_words_they_were_built_from() {
    let mut rng = Rng::new(1501);
    for _ in 0..600 {
        let (text, want) = build(&mut rng, 3);
        if want.len() > MAX_WORDS {
            check!(format!("{text:?}"), expand(&text), Err(BraceError::TooManyWords));
        } else {
            check!(format!("{text:?}"), expand(&text), Ok(want));
        }
    }
}
