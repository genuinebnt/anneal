use solution::*;

#[test]
fn three() {
    check!(r#"parts = ["a", "bb", "ccc"], sep = ", ""#, { let s = join_with(&["a", "bb", "ccc"], ", "); (s.clone(), s.capacity() == s.len()) }, ("a, bb, ccc".to_string(), true));
}

#[test]
fn one() {
    check!(r#"parts = ["solo"], sep = "-""#, { let s = join_with(&["solo"], "-"); (s.clone(), s.capacity() == s.len()) }, ("solo".to_string(), true));
}

#[test]
fn empty_sep() {
    check!(r#"parts = ["a", "b"], sep = """#, { let s = join_with(&["a", "b"], ""); (s.clone(), s.capacity() == s.len()) }, ("ab".to_string(), true));
}

#[test]
fn no_parts() {
    check!(r#"parts = [], sep = ",""#, join_with(&[], ","), String::new());
}

#[test]
fn empty_parts_keep_seps() {
    check!(r#"parts = ["", ""], sep = ",""#, { let s = join_with(&["", ""], ","); (s.clone(), s.capacity() == s.len()) }, (",".to_string(), true));
}

#[test]
fn allocates_once() {
    check!(r#"parts = ["a", "bb", "ccc"], sep = ", ": heap allocations"#, anneal_prelude::allocs(|| join_with(&["a", "bb", "ccc"], ", ")).1.count, 1);
}
