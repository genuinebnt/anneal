use solution::*;

#[test]
fn push_then_pop() {
    check!(r#"capacity 4, push 1, 2, 3, pop three"#, { let (mut p, mut c) = channel(4); p.push(1).unwrap(); p.push(2).unwrap(); p.push(3).unwrap(); (c.pop(), c.pop(), c.pop(), c.pop()) }, (Some(1), Some(2), Some(3), None));
}

#[test]
fn full_gives_the_value_back() {
    check!(r#"capacity 2, third push"#, { let (mut p, _c) = channel(2); p.push("a".to_string()).unwrap(); p.push("b".to_string()).unwrap(); p.push("c".to_string()) }, Err("c".to_string()));
}

#[test]
fn exactly_capacity_fit() {
    check!(r#"capacity 3, three pushes succeed"#, { let (mut p, _c) = channel(3); (p.push(1).is_ok(), p.push(2).is_ok(), p.push(3).is_ok(), p.push(4).is_ok()) }, (true, true, true, false));
}

#[test]
fn space_comes_back_after_pop() {
    check!(r#"capacity 1, push pop push"#, { let (mut p, mut c) = channel(1); p.push(1).unwrap(); let full = p.push(2).is_err(); let a = c.pop(); let ok = p.push(3).is_ok(); (full, a, ok, c.pop()) }, (true, Some(1), true, Some(3)));
}

#[test]
fn empty_pop() {
    check!(r#"nothing pushed"#, { let (_p, mut c) = channel::<u8>(2); c.pop() }, None);
}
