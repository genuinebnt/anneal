use solution::*;
use anneal_prelude::Rng;

#[allow(dead_code)]
fn ranges(spec: &str) -> Result<Vec<(char, char)>, SetError> {
    CharSet::parse(spec).map(|s| s.ranges().to_vec())
}

#[test]
fn escapes_work_as_either_end_of_a_range() {
    check!(r"\t-\r", ranges(r"\t-\r"), Ok(vec![('\t', '\r')]));
    check!(r"\--0", ranges(r"\--0"), Ok(vec![('-', '0')]));
    check!(r"+-\-", ranges(r"+-\-"), Ok(vec![('+', '-')]));
    check!("+-- (a range ending at a dash)", ranges("+--"), Ok(vec![('+', '-')]));
}

#[test]
fn errors_come_in_reading_order() {
    check!(r"z-a\q", ranges(r"z-a\q"), Err(SetError::Reversed(0)));
    check!(r"\qz-a", ranges(r"\qz-a"), Err(SetError::UnknownEscape(0)));
    check!(r"a-\", ranges(r"a-\"), Err(SetError::TrailingEscape(2)));
    check!(r"^z-\n", ranges(r"^z-\n"), Err(SetError::Reversed(1)));
    check!(r"é\né-a", ranges(r"é\né-a"), Err(SetError::Reversed(3)));
    check!(r"a\é", ranges(r"a\é"), Err(SetError::UnknownEscape(1)));
}

#[test]
fn the_ends_of_unicode() {
    let all = "\0-\u{10FFFF}";
    check!("every character", ranges(all), Ok(vec![('\0', '\u{10FFFF}')]));
    check!("everything but everything", ranges(&format!("^{all}")), Ok(vec![]));
    check!("without the first", ranges("^\0"), Ok(vec![('\u{1}', '\u{10FFFF}')]));
    check!("without the last", ranges("^\u{10FFFF}"), Ok(vec![('\0', '\u{10FFFE}')]));
    check!("the last twice and more", ranges("\u{10FFFF}a\u{10FFFF}"), Ok(vec![('a', 'a'), ('\u{10FFFF}', '\u{10FFFF}')]));
}

#[test]
fn the_hole_in_char_is_stepped_over() {
    check!("ranges meeting across the hole join", ranges("\u{E000}-\u{E002}\u{D700}-\u{D7FF}"), Ok(vec![('\u{D700}', '\u{E002}')]));
    check!("taking away U+E000", ranges("^\u{E000}"), Ok(vec![('\0', '\u{D7FF}'), ('\u{E001}', '\u{10FFFF}')]));
    check!("taking away U+D7FF", ranges("^\u{D7FF}"), Ok(vec![('\0', '\u{D7FE}'), ('\u{E000}', '\u{10FFFF}')]));
    check!("taking away both sides", ranges("^\0-\u{D7FF}\u{E000}-\u{10FFFF}"), Ok(vec![]));
}

#[test]
fn random_sets_agree_with_a_list_of_members() {
    let alphabet = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
    let mut rng = Rng::new(1502);
    for _ in 0..500 {
        let negated = rng.bool();
        let mut spec = String::from(if negated { "^" } else { "" });
        let mut picked = [false; 8];
        for _ in 0..rng.below(5) {
            let a = rng.below(8);
            let b = rng.below(8);
            let (lo, hi) = (a.min(b), a.max(b));
            if rng.bool() {
                spec.push(alphabet[lo]);
                spec.push('-');
                spec.push(alphabet[hi]);
                (lo..=hi).for_each(|i| picked[i] = true);
            } else {
                spec.push(alphabet[a]);
                picked[a] = true;
            }
        }
        let set = CharSet::parse(&spec).unwrap();
        for (i, &c) in alphabet.iter().enumerate() {
            check!(format!("{spec:?} contains {c:?}"), set.contains(c), picked[i] != negated);
        }
        check!(format!("{spec:?} contains 'z'"), set.contains('z'), negated);
        let r = set.ranges();
        check!(format!("{spec:?}: each pair is in order: {r:?}"), r.iter().all(|&(l, h)| l <= h), true);
        check!(
            format!("{spec:?}: pairs are sorted, apart and not touching: {r:?}"),
            r.windows(2).all(|w| (w[0].1 as u32) + 1 < w[1].0 as u32),
            true
        );
    }
}

#[test]
fn many_ranges_and_many_lookups() {
    // 60 000 separate characters (every second one), then 300 000 lookups.
    let spec: String = (0..60_000u32).rev().map(|i| char::from_u32(0x2_0000 + 2 * i).unwrap()).collect();
    let set = CharSet::parse(&spec).unwrap();
    check!("60 000 pairs", set.ranges().len(), 60_000);
    let mut hits = 0u32;
    for i in 0..300_000u32 {
        hits += u32::from(set.contains(char::from_u32(0x2_0000 + (i % 120_000)).unwrap()));
    }
    check!("every second lookup is a member", hits, 150_000);
}

#[test]
fn lone_symbols() {
    check!("a dash alone", ranges("-"), Ok(vec![('-', '-')]));
    check!("two carets", ranges("^^"), Ok(vec![('\0', ']'), ('_', '\u{10FFFF}')]));
    check!("an escaped caret first does not negate", ranges(r"\^a"), Ok(vec![('^', '^'), ('a', 'a')]));
}

#[test]
fn equal_sets_compare_equal() {
    check!("two spellings", CharSet::parse("a-cd-f").unwrap() == CharSet::parse("fedcba").unwrap(), true);
    check!("complement twice over", CharSet::parse("^\0-`g-\u{10FFFF}").unwrap() == CharSet::parse("a-f").unwrap(), true);
}
