use solution::*;

#[test]
fn case_accents_and_spaces() {
    check!("key", search_key("  CAFÉ\t Crème "), "cafe creme".to_string());
    check!("two letters from one", search_key("Straße"), "strasse".to_string());
    check!("é sent as two characters", search_key("cafe\u{301}"), "cafe".to_string());
}

#[test]
fn the_typed_part_is_lit_up() {
    let shown = "Crème Brûlée";
    let end = typed_end(shown, "creme br");
    check!("end", end, Some("Crème Br".len()));
    check!("not a beginning", typed_end(shown, "brul"), None);
    check!("nothing typed", typed_end(shown, "  "), Some(0));
}

#[test]
fn a_letter_keeps_its_mark() {
    check!("e + mark", typed_end("Re\u{301}sume\u{301}", "re"), Some("Re\u{301}".len()));
}

#[test]
fn nothing_but_spaces_has_an_empty_key() {
    check!("spaces", search_key(" \t\n "), String::new());
    check!("empty", search_key(""), String::new());
}

#[test]
fn the_answer_counts_bytes() {
    check!("é is two bytes", typed_end("été", "ét"), Some(3));
}
