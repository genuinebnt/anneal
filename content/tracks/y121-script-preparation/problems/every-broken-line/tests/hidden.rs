use solution::*;

#[allow(dead_code)]
fn set(name: &str, n: u64) -> Item {
    Item::Setting(name.into(), Value::Number(n))
}

#[allow(dead_code)]
fn section(name: &str, items: Vec<Item>) -> Item {
    Item::Section(name.into(), items)
}

#[allow(dead_code)]
fn lines(found: &[Mistake]) -> Vec<(u32, Problem)> {
    found.iter().map(|m| (m.line, m.problem)).collect()
}

#[test]
fn empty_files() {
    check!("empty", read(""), (vec![], vec![]));
    check!("blank", read(" \n\n "), (vec![], vec![]));
    check!(
        "an empty section",
        read("s {}"),
        (vec![section("s", vec![])], vec![])
    );
}

#[test]
fn each_problem() {
    for (text, line, problem) in [
        ("= 1;", 1, Problem::ExpectedName),
        ("7 = 1;", 1, Problem::ExpectedName),
        ("a\n+ 1;", 2, Problem::ExpectedEqualsOrBrace),
        ("a =\n;", 2, Problem::ExpectedValue),
        ("a = [,];", 1, Problem::ExpectedValue),
        ("a = [1,];", 1, Problem::ExpectedValue),
        ("a = [1\n\n;", 3, Problem::ExpectedCommaOrBracket),
        ("a = 1 2;", 1, Problem::ExpectedSemi),
        ("a", 1, Problem::ExpectedEqualsOrBrace),
        ("a =\n", 1, Problem::ExpectedValue),
        ("a = 1\n\n\n", 1, Problem::ExpectedSemi),
        ("a = [1", 1, Problem::ExpectedCommaOrBracket),
        ("$;", 1, Problem::ExpectedName),
        ("}", 1, Problem::StrayClose),
    ] {
        let (items, found) = read(text);
        check!(
            text,
            (items.len(), lines(&found)),
            (0, vec![(line, problem)])
        );
    }
}

#[test]
fn a_broken_item_with_a_block_in_it_is_passed_over_whole() {
    let text = "bad + {\n x = 1; y { z = 2; } ;\n}\nafter = 5;";
    let (items, found) = read(text);
    check!(
        text,
        (items, lines(&found)),
        (
            vec![set("after", 5)],
            vec![(1, Problem::ExpectedEqualsOrBrace)]
        )
    );
}

#[test]
fn a_semicolon_inside_braces_does_not_end_the_skip() {
    let text = "a = { ; ; } ; b = 1;";
    let (items, found) = read(text);
    check!(
        text,
        (items, lines(&found)),
        (
            vec![set("b", 1)],
            vec![(1, Problem::ExpectedValue), (1, Problem::ExpectedName)]
        )
    );
}

#[test]
fn the_wrong_token_may_be_the_closer_of_the_section() {
    let text = "s { a = } t = 1;";
    let (items, found) = read(text);
    check!(
        text,
        (items, lines(&found)),
        (
            vec![section("s", vec![]), set("t", 1)],
            vec![(1, Problem::ExpectedValue)]
        )
    );
    let text = "s { a }";
    let (items, found) = read(text);
    check!(
        text,
        (items, lines(&found)),
        (
            vec![section("s", vec![])],
            vec![(1, Problem::ExpectedEqualsOrBrace)]
        )
    );
}

#[test]
fn stray_closers_take_only_themselves() {
    let text = "} } a = 1;\n} b = 2;";
    let (items, found) = read(text);
    check!(
        text,
        (items, lines(&found)),
        (
            vec![set("a", 1), set("b", 2)],
            vec![
                (1, Problem::StrayClose),
                (1, Problem::StrayClose),
                (2, Problem::StrayClose)
            ]
        )
    );
}

#[test]
fn several_unclosed_sections() {
    let text = "a {\n b {\n  c = 1;\n  d = ;\n";
    let (items, found) = read(text);
    let want = vec![section("a", vec![section("b", vec![set("c", 1)])])];
    check!(
        text,
        (items, lines(&found)),
        (
            want,
            vec![
                (1, Problem::Unclosed),
                (2, Problem::Unclosed),
                (4, Problem::ExpectedValue)
            ]
        )
    );
}

#[test]
fn thirty_two_levels_fit_and_one_more_does_not() {
    let text = format!("{}x = 1;{}", "s { ".repeat(32), " }".repeat(32));
    check!("32 sections", read(&text).1, vec![]);
    let text = format!("{}x = 1;{}\nok = 2;", "s { ".repeat(33), " }".repeat(33));
    let (items, found) = read(&text);
    check!(
        "33 sections",
        (items.len(), lines(&found)),
        (2, vec![(1, Problem::TooDeep)])
    );
    let text = format!("a = {}{};", "[".repeat(32), "]".repeat(32));
    check!("32 lists", read(&text).1, vec![]);
    let text = format!("a = {}{};\nok = 2;", "[".repeat(33), "]".repeat(33));
    let (items, found) = read(&text);
    check!(
        "33 lists",
        (items, lines(&found)),
        (vec![set("ok", 2)], vec![(1, Problem::TooDeep)])
    );
}

#[test]
fn huge_hostile_files() {
    for opener in ["s { ", "[", "a = [", "{ ", "} ", "a { b = [ { "] {
        let text = opener.repeat(100_000);
        let (_, found) = read(&text);
        check!(format!("{opener:?} 100 000 times"), found.is_empty(), false);
    }
}

#[test]
fn many_mistakes_are_all_reported() {
    let text = "a = ;\n".repeat(5_000);
    let (items, found) = read(&text);
    check!(
        "5000 broken lines",
        (items.len(), found.len(), found[4999].line),
        (0, 5000, 5000)
    );
}

/// Good items with broken ones put between them: every good one comes back, every broken one is reported once.
#[test]
fn random_files_with_broken_items() {
    let broken = [
        ("a = ;", Problem::ExpectedValue),
        ("= 1;", Problem::ExpectedName),
        ("a b;", Problem::ExpectedEqualsOrBrace),
        ("a = [1 2];", Problem::ExpectedCommaOrBracket),
        ("a = 1 2;", Problem::ExpectedSemi),
        ("a + { x = 1; }", Problem::ExpectedEqualsOrBrace),
        ("a = { };", Problem::ExpectedValue),
        ("a = [[], [,]];", Problem::ExpectedValue),
    ];
    let mut rng = anneal_prelude::Rng::new(6101);
    for _ in 0..800 {
        let mut text = String::new();
        let mut line = 1;
        // One list of good items for each open section.
        let mut open: Vec<(String, Vec<Item>)> = vec![(String::new(), Vec::new())];
        let mut want = Vec::new();
        for i in 0..rng.below(14) {
            match rng.below(4) {
                0 => {
                    let name = format!("s{i}");
                    text.push_str(&format!("{name} {{\n"));
                    open.push((name, Vec::new()));
                }
                1 if open.len() > 1 => {
                    text.push_str("}\n");
                    let (name, items) = open.pop().unwrap();
                    open.last_mut().unwrap().1.push(Item::Section(name, items));
                }
                2 => {
                    let (bad, problem) = *rng.pick(&broken);
                    // `a = { };` leaves its `;` behind, which is a second broken item.
                    text.push_str(bad);
                    text.push('\n');
                    want.push((line, problem));
                    if bad == "a = { };" {
                        want.push((line, Problem::ExpectedName));
                    }
                }
                _ => {
                    let name = format!("k{i}");
                    text.push_str(&format!("{name} = {i};\n"));
                    open.last_mut().unwrap().1.push(set(&name, i as u64));
                }
            }
            line += 1;
        }
        while open.len() > 1 {
            text.push_str("}\n");
            let (name, items) = open.pop().unwrap();
            open.last_mut().unwrap().1.push(Item::Section(name, items));
        }
        let (items, found) = read(&text);
        check!(
            text.clone(),
            (items, lines(&found)),
            (open.pop().unwrap().1, want)
        );
    }
}
