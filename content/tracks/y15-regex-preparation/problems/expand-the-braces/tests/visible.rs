use solution::*;

#[allow(dead_code)]
fn words(list: &[&str]) -> Result<Vec<String>, BraceError> {
    Ok(list.iter().map(|w| w.to_string()).collect())
}

#[test]
fn two_groups_the_left_one_changes_slowest() {
    check!("{a,b}{1,2}", expand("{a,b}{1,2}"), words(&["a1", "a2", "b1", "b2"]));
    check!(
        "img_{small,large}.{png,jpg}",
        expand("img_{small,large}.{png,jpg}"),
        words(&["img_small.png", "img_small.jpg", "img_large.png", "img_large.jpg"])
    );
}

#[test]
fn plain_text_and_commas_outside_groups() {
    check!("the empty pattern", expand(""), words(&[""]));
    check!("a,b", expand("a,b"), words(&["a,b"]));
    check!("x,{y,z},w", expand("x,{y,z},w"), words(&["x,y,w", "x,z,w"]));
}

#[test]
fn nesting() {
    check!("a{b,c{d,e}}f", expand("a{b,c{d,e}}f"), words(&["abf", "acdf", "acef"]));
}

#[test]
fn empty_choices_single_choices_and_repeats() {
    check!("a{,b}", expand("a{,b}"), words(&["a", "ab"]));
    check!("{}", expand("{}"), words(&[""]));
    check!("{a}b", expand("{a}b"), words(&["ab"]));
    check!("{x,x}", expand("{x,x}"), words(&["x", "x"]));
}

#[test]
fn escapes_make_a_character_ordinary() {
    check!(r"\{a,b\}", expand(r"\{a,b\}"), words(&["{a,b}"]));
    check!(r"{a\,b,c\}d}", expand(r"{a\,b,c\}d}"), words(&["a,b", "c}d"]));
    check!(r"\\\q", expand(r"\\\q"), words(&[r"\q"]));
}

#[test]
fn errors_name_a_position() {
    check!("ab}", expand("ab}"), Err(BraceError::UnmatchedClose(2)));
    check!("x{ab", expand("x{ab"), Err(BraceError::Unclosed(1)));
    check!("{a,{b}", expand("{a,{b}"), Err(BraceError::Unclosed(0)));
    check!(r"ab\", expand(r"ab\"), Err(BraceError::TrailingEscape(2)));
}
