use solution::*;

#[test]
fn inserts_at_different_positions() {
    check!(r#"left stays, right shifts"#, transform(&Op::Insert { pos: 2, ch: 'x', site: 1 }, &Op::Insert { pos: 5, ch: 'y', site: 2 }), (Op::Insert { pos: 2, ch: 'x', site: 1 }, Op::Insert { pos: 6, ch: 'y', site: 2 }));
}

#[test]
fn inserts_at_the_same_position_lower_site_first() {
    check!(r#"tie on index"#, (transform(&Op::Insert { pos: 3, ch: 'x', site: 1 }, &Op::Insert { pos: 3, ch: 'y', site: 2 }), transform(&Op::Insert { pos: 3, ch: 'y', site: 2 }, &Op::Insert { pos: 3, ch: 'x', site: 1 })), ((Op::Insert { pos: 3, ch: 'x', site: 1 }, Op::Insert { pos: 4, ch: 'y', site: 2 }), (Op::Insert { pos: 4, ch: 'y', site: 2 }, Op::Insert { pos: 3, ch: 'x', site: 1 })));
}

#[test]
fn insert_before_a_delete() {
    check!(r#"the deleted character moves right"#, transform(&Op::Insert { pos: 2, ch: 'x', site: 1 }, &Op::Delete { pos: 2 }), (Op::Insert { pos: 2, ch: 'x', site: 1 }, Op::Delete { pos: 3 }));
}

#[test]
fn insert_after_a_delete() {
    check!(r#"the insert point moves left"#, transform(&Op::Insert { pos: 5, ch: 'x', site: 1 }, &Op::Delete { pos: 2 }), (Op::Insert { pos: 4, ch: 'x', site: 1 }, Op::Delete { pos: 2 }));
}

#[test]
fn delete_the_same_character() {
    check!(r#"both become Nop"#, transform(&Op::Delete { pos: 4 }, &Op::Delete { pos: 4 }), (Op::Nop, Op::Nop));
}

#[test]
fn deletes_at_different_positions() {
    check!(r#"the later one moves left"#, (transform(&Op::Delete { pos: 1 }, &Op::Delete { pos: 4 }), transform(&Op::Delete { pos: 4 }, &Op::Delete { pos: 1 })), ((Op::Delete { pos: 1 }, Op::Delete { pos: 3 }), (Op::Delete { pos: 3 }, Op::Delete { pos: 1 })));
}

#[test]
fn a_concrete_session() {
    check!(r#"abcdef, both converge"#, { let s = "abcdef"; let a = Op::Insert { pos: 2, ch: 'X', site: 1 }; let b = Op::Delete { pos: 3 }; let (a2, b2) = transform(&a, &b); (apply(&apply(s, &a), &b2), apply(&apply(s, &b), &a2)) }, ("abXcef".to_string(), "abXcef".to_string()));
}
