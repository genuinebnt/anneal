use solution::*;

#[allow(dead_code)]
fn ranges(spec: &str) -> Result<Vec<(char, char)>, SetError> {
    CharSet::parse(spec).map(|s| s.ranges().to_vec())
}

#[test]
fn ranges_and_single_characters_come_out_sorted() {
    check!("a-z0-9_", ranges("a-z0-9_"), Ok(vec![('0', '9'), ('_', '_'), ('a', 'z')]));
    check!("the empty text", ranges(""), Ok(vec![]));
}

#[test]
fn overlapping_and_touching_ranges_become_one() {
    check!("d-fa-c", ranges("d-fa-c"), Ok(vec![('a', 'f')]));
    check!("a-mc-zz", ranges("a-mc-zz"), Ok(vec![('a', 'z')]));
    check!("cab", ranges("cab"), Ok(vec![('a', 'c')]));
    check!("a-cc-a: the second range is reversed", ranges("a-cc-a"), Err(SetError::Reversed(3)));
}

#[test]
fn a_dash_that_cannot_make_a_range_is_ordinary() {
    check!("-a", ranges("-a"), Ok(vec![('-', '-'), ('a', 'a')]));
    check!("a-", ranges("a-"), Ok(vec![('-', '-'), ('a', 'a')]));
    check!("a-c-e", ranges("a-c-e"), Ok(vec![('-', '-'), ('a', 'c'), ('e', 'e')]));
}

#[test]
fn a_leading_caret_takes_the_rest_away() {
    check!(
        "^b",
        ranges("^b"),
        Ok(vec![('\0', 'a'), ('c', '\u{10FFFF}')])
    );
    check!("^ alone", ranges("^"), Ok(vec![('\0', '\u{10FFFF}')]));
    check!("a^", ranges("a^"), Ok(vec![('^', '^'), ('a', 'a')]));
}

#[test]
fn escapes() {
    check!(r"\n\t\r", ranges(r"\n\t\r"), Ok(vec![('\t', '\n'), ('\r', '\r')]));
    check!(r"a\-c", ranges(r"a\-c"), Ok(vec![('-', '-'), ('a', 'a'), ('c', 'c')]));
    check!(r"\^\\", ranges(r"\^\\"), Ok(vec![('\\', '\\'), ('^', '^')]));
}

#[test]
fn contains_answers_from_the_set() {
    let set = CharSet::parse("^a-y0").unwrap();
    check!("^a-y0 on z, a, m, 0, é", ['z', 'a', 'm', '0', 'é'].map(|c| set.contains(c)), [true, false, false, false, true]);
}

#[test]
fn errors_name_a_position() {
    check!(r"ab\q", ranges(r"ab\q"), Err(SetError::UnknownEscape(2)));
    check!(r"ab\", ranges(r"ab\"), Err(SetError::TrailingEscape(2)));
    check!("xz-a", ranges("xz-a"), Err(SetError::Reversed(1)));
}
