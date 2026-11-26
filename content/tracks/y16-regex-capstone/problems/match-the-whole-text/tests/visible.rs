use solution::*;

#[allow(dead_code)]
fn m(pattern: &str, text: &str) -> bool {
    is_match(&compile(&parse(pattern).expect("a valid pattern")), text)
}

#[test]
fn the_whole_text_or_nothing() {
    check!("a(b|c)*d", (m("a(b|c)*d", "abcbcd"), m("a(b|c)*d", "abcbc"), m("a(b|c)*d", "xabd"), m("a(b|c)*d", "abdx")), (true, false, false, false));
}

#[test]
fn classes_and_escapes() {
    check!("a name", (m(r"[a-z_]\w*", "snake_case9"), m(r"[a-z_]\w*", "9lives")), (true, false));
    check!("a number", (m(r"\d+\.\d+", "3.14"), m(r"\d+\.\d+", "3x14")), (true, false));
}

#[test]
fn the_empty_text() {
    check!("what matches nothing", (m("", ""), m("a|", ""), m("a*", ""), m("a+", ""), m(".", "")), (true, true, true, false, false));
}

#[test]
fn anchors() {
    check!("^ab?$", (m("^ab?$", "a"), m("^ab?$", "ab")), (true, true));
    check!("anchors in the wrong place", (m("a^b", "ab"), m("a$b", "ab")), (false, false));
    check!("both anchors on the empty text", (m("^$", ""), m("$^", ""), m("^$", "a")), (true, true, false));
}

#[test]
fn loops_around_nothing_end() {
    check!("(a*)*", (m("(a*)*", "aaaa"), m("(a*)*", "b"), m("(a|)*b", "aab")), (true, false, true));
}
