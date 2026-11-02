use solution::*;

#[test]
fn insert_get() {
    check!(r#"two values"#, { let mut s = Slab::new(); let a = s.insert("a"); let b = s.insert("b"); (s.get(a).copied(), s.get(b).copied(), s.len()) }, (Some("a"), Some("b"), 2));
}

#[test]
fn remove_returns_value() {
    check!(r#"remove once, twice"#, { let mut s = Slab::new(); let a = s.insert(10); (s.remove(a), s.remove(a), s.get(a).copied(), s.len()) }, (Some(10), None, None, 0));
}

#[test]
fn slot_is_reused() {
    check!(r#"remove then insert"#, { let mut s = Slab::new(); let a = s.insert(1); s.remove(a); let b = s.insert(2); (a.idx == b.idx, s.slots()) }, (true, 1));
}

#[test]
fn stale_key_is_dead_after_reuse() {
    check!(r#"old key must not see the new value"#, { let mut s = Slab::new(); let a = s.insert(1); s.remove(a); let b = s.insert(2); (a == b, s.get(a).copied(), s.get(b).copied(), s.remove(a)) }, (false, None, Some(2), None));
}

#[test]
fn get_mut() {
    check!(r#"modify in place"#, { let mut s = Slab::new(); let a = s.insert(vec![1]); s.get_mut(a).unwrap().push(2); s.get(a).cloned() }, Some(vec![1, 2]));
}
