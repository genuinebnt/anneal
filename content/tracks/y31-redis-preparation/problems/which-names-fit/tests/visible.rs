use solution::*;


#[allow(dead_code)]
fn table(cases: &[(&str, &str, bool)]) {
    for &(pattern, name, want) in cases {
        check!(format!("pattern {pattern:?}, name {name:?}"), matches(pattern.as_bytes(), name.as_bytes()), want);
    }
}

#[test]
fn plain_names_and_question_marks() {
    table(&[("cat", "cat", true), ("cat", "cats", false), ("cat", "ca", false), ("c?t", "cut", true), ("c?t", "ct", false), ("", "", true), ("", "a", false)]);
}

#[test]
fn stars() {
    table(&[
        ("*", "", true), ("*", "anything", true), ("user:*", "user:42", true), ("user:*", "users", false),
        ("*.log", "a.b.log", true), ("a*b*c", "a-b-b-c", true), ("a*b*c", "acb", false),
    ]);
}

#[test]
fn sets_of_characters() {
    table(&[
        ("[abc]1", "b1", true), ("[abc]1", "d1", false), ("v[0-9]", "v7", true), ("v[0-9]", "vx", false),
        ("[^0-9]", "x", true), ("[^0-9]", "5", false), ("[z-a]", "m", true),
    ]);
}

#[test]
fn escapes() {
    table(&[(r"a\*b", "a*b", true), (r"a\*b", "axxb", false), (r"\?", "?", true), (r"\?", "x", false), (r"\\", r"\", true)]);
}

#[test]
fn a_hard_pattern_answers_quickly() {
    let pattern = "a*".repeat(40) + "b";
    let name = "a".repeat(5000);
    check!("40 stars against 5000 letters with no b", matches(pattern.as_bytes(), name.as_bytes()), false);
}
