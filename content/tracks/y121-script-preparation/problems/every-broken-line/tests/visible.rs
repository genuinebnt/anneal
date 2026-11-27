use solution::*;

#[allow(dead_code)]
fn set(name: &str, value: Value) -> Item {
    Item::Setting(name.into(), value)
}

#[allow(dead_code)]
fn num(n: u64) -> Value {
    Value::Number(n)
}

#[allow(dead_code)]
fn word(w: &str) -> Value {
    Value::Word(w.into())
}

#[allow(dead_code)]
fn section(name: &str, items: Vec<Item>) -> Item {
    Item::Section(name.into(), items)
}

#[allow(dead_code)]
fn wrong(line: u32, problem: Problem) -> Mistake {
    Mistake { line, problem }
}

#[test]
fn a_good_file() {
    let text = "name = web;\nports = [80, [], [x]];\nlimits { clients = 100; }";
    let want = vec![
        set("name", word("web")),
        set(
            "ports",
            Value::List(vec![
                num(80),
                Value::List(vec![]),
                Value::List(vec![word("x")]),
            ]),
        ),
        section("limits", vec![set("clients", num(100))]),
    ];
    check!(text, read(text), (want, vec![]));
}

#[test]
fn one_bad_line_does_not_hide_the_next() {
    let text = "a = ;\nb = 2;\nc 3;\nd = 4;";
    check!(
        text,
        read(text),
        (
            vec![set("b", num(2)), set("d", num(4))],
            vec![
                wrong(1, Problem::ExpectedValue),
                wrong(3, Problem::ExpectedEqualsOrBrace)
            ]
        )
    );
}

#[test]
fn a_missing_semicolon_takes_the_next_item_with_it() {
    let text = "a = 1\nb = 2;\nc = 3;";
    check!(
        text,
        read(text),
        (
            vec![set("c", num(3))],
            vec![wrong(2, Problem::ExpectedSemi)]
        )
    );
}

#[test]
fn a_mistake_inside_a_section_stays_inside() {
    let text = "s {\n  x = [1 2];\n  y = 3\n}\nz = 4;";
    check!(
        text,
        read(text),
        (
            vec![section("s", vec![]), set("z", num(4))],
            vec![
                wrong(2, Problem::ExpectedCommaOrBracket),
                wrong(4, Problem::ExpectedSemi)
            ]
        )
    );
}

#[test]
fn braces_that_do_not_match() {
    let text = "}\na = 1;\ns {\n b = 2;";
    check!(
        text,
        read(text),
        (
            vec![set("a", num(1)), section("s", vec![set("b", num(2))])],
            vec![wrong(1, Problem::StrayClose), wrong(3, Problem::Unclosed)]
        )
    );
}
