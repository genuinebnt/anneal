use solution::*;

#[test]
fn lifo_order() {
    check!(r#"push 1, 2, 3; pop three"#, { let s = Stack::new(4); s.push(1).unwrap(); s.push(2).unwrap(); s.push(3).unwrap(); (s.pop(), s.pop(), s.pop(), s.pop()) }, (Some(3), Some(2), Some(1), None));
}

#[test]
fn full_gives_value_back() {
    check!(r#"capacity 2, third push"#, { let s = Stack::new(2); s.push(10).unwrap(); s.push(20).unwrap(); s.push(30) }, Err(30));
}

#[test]
fn pop_frees_a_slot() {
    check!(r#"fill, pop, push again"#, { let s = Stack::new(2); s.push(1).unwrap(); s.push(2).unwrap(); let a = s.pop(); let ok = s.push(3).is_ok(); (a, ok, s.pop(), s.pop(), s.pop()) }, (Some(2), true, Some(3), Some(1), None));
}

#[test]
fn empty_pop() {
    check!(r#"new stack"#, Stack::new(3).pop(), None);
}

#[test]
fn pack_roundtrip() {
    check!(r#"tag and index survive packing"#, unpack(pack(7, 9)), (7, 9));
}
