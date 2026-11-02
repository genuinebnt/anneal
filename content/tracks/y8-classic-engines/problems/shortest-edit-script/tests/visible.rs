use solution::*;

#[test]
fn identical_files() {
    check!(r#"all keeps"#, diff(&["a", "b"], &["a", "b"]), vec![Op::Keep(0, 0), Op::Keep(1, 1)]);
}

#[test]
fn delete_the_middle_line() {
    check!(r#"a b c -> a c"#, diff(&["a", "b", "c"], &["a", "c"]), vec![Op::Keep(0, 0), Op::Delete(1), Op::Keep(2, 1)]);
}

#[test]
fn insert_a_line() {
    check!(r#"a c -> a b c"#, diff(&["a", "c"], &["a", "b", "c"]), vec![Op::Keep(0, 0), Op::Insert(1), Op::Keep(1, 2)]);
}

#[test]
fn replacement_is_delete_then_insert() {
    check!(r#"x -> y"#, diff(&["x"], &["y"]), vec![Op::Delete(0), Op::Insert(0)]);
}

#[test]
fn empty_sides() {
    check!(r#"from nothing and to nothing"#, (diff(&[], &["a", "b"]), diff(&["a"], &[]), diff(&[], &[])), (vec![Op::Insert(0), Op::Insert(1)], vec![Op::Delete(0)], vec![]));
}
