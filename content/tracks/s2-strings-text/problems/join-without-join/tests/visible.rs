use solution::*;

#[test]
fn three() {
    check!(r#"words = ["a", "bc", "d"], sep = ", ""#, join_words(&["a", "bc", "d"], ", "), "a, bc, d".to_string());
}

#[test]
fn one() {
    check!(r#"words = ["solo"], sep = "-""#, join_words(&["solo"], "-"), "solo".to_string());
}

#[test]
fn no_words() {
    check!(r#"words = [], sep = ", ""#, join_words(&[], ", "), String::new());
}

#[test]
fn no_trailing_sep() {
    check!(r#"words = ["a", "b"], sep = "+""#, join_words(&["a", "b"], "+"), "a+b".to_string());
}

#[test]
fn empty_words_keep_seps() {
    check!(r#"words = ["", "", ""], sep = "-""#, join_words(&["", "", ""], "-"), "--".to_string());
}

#[test]
fn allocates_once() {
    check!(r#"words = ["a", "bc", "d"], sep = ", ": heap allocations"#, anneal_prelude::allocs(|| join_words(&["a", "bc", "d"], ", ")).1.count, 1);
}
