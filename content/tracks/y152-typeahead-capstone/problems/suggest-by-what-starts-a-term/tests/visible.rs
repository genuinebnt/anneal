use solution::*;

/// The suggested terms, joined by commas.
fn names(list: &[Suggestion]) -> String {
    list.iter().map(|s| s.term.as_str()).collect::<Vec<_>>().join(", ")
}

#[test]
fn chosen_more_often_comes_first() {
    let mut t = Terms::new();
    t.add(&[("rust", 9), ("ruby", 6), ("runtime", 2), ("python", 12)], 0);
    check!("ru", names(&t.suggest("ru", 5, 0)), "rust, ruby, runtime".to_string());
    check!("only two", names(&t.suggest("ru", 2, 0)), "rust, ruby".to_string());
    check!("everything", names(&t.suggest("", 1, 0)), "python".to_string());
}

#[test]
fn recent_choices_count_more() {
    let mut t = Terms::new();
    t.add(&[("rust", 4)], 0);
    t.add(&[("ruby", 3)], 3600);
    check!("rust is down to 2", names(&t.suggest("ru", 5, 3600)), "ruby, rust".to_string());
    check!("its score", (t.suggest("rust", 1, 3600)[0].score - 2.0).abs() < 1e-5, true);
}

#[test]
fn one_term_many_spellings() {
    let mut t = Terms::new();
    let report = t.add(&[("  Café ", 1), ("cafe", 1), ("CAFÉ", 1), ("  ", 9)], 0);
    check!("report", report, Added { added: 1, existing: 2 });
    check!("len", t.len(), 1);
    check!("first spelling", names(&t.suggest("CAF", 5, 0)), "Café".to_string());
}

#[test]
fn choosing_moves_a_term_up() {
    let mut t = Terms::new();
    t.add(&[("go", 1), ("gleam", 1)], 0);
    check!("tie by key", names(&t.suggest("g", 5, 0)), "gleam, go".to_string());
    check!("known", t.choose("GO", 0), true);
    check!("after", names(&t.suggest("g", 5, 0)), "go, gleam".to_string());
}

#[test]
fn an_unknown_choice_changes_nothing() {
    let mut t = Terms::new();
    check!("empty", t.is_empty(), true);
    check!("unknown", t.choose("zig", 5), false);
    check!("still empty", (t.len(), t.suggest("", 5, 5).len()), (0, 0));
}
