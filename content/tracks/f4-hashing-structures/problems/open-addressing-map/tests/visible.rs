use solution::*;

#[test]
fn insert_get_remove() {
    check!(r#"basic"#, { let mut m = OaMap::new(); m.insert("a", 1); m.insert("b", 2); (m.get(&"a").copied(), m.get(&"c").copied(), m.remove(&"a"), m.get(&"a").copied(), m.len()) }, (Some(1), None, Some(1), None, 1));
}

#[test]
fn insert_replaces_and_returns_the_old_value() {
    check!(r#"same key twice"#, { let mut m = OaMap::new(); let a = m.insert(1, "x"); let b = m.insert(1, "y"); (a, b, m.len(), m.get(&1).copied()) }, (None, Some("x"), 1, Some("y")));
}

#[test]
fn grows_and_keeps_everything() {
    check!(r#"a thousand keys"#, { let mut m = OaMap::new(); for i in 0..1000u32 { m.insert(i, i * 2); } ((0..1000u32).all(|i| m.get(&i) == Some(&(i * 2))), m.len(), m.capacity().is_power_of_two(), m.capacity() >= 1000) }, (true, 1000, true, true));
}

#[test]
fn get_mut_edits_in_place() {
    check!(r#"push to a value"#, { let mut m = OaMap::new(); m.insert("k", vec![1]); m.get_mut(&"k").unwrap().push(2); m.get(&"k").cloned() }, Some(vec![1, 2]));
}

#[test]
fn iter_visits_every_entry() {
    check!(r#"sum"#, { let mut m = OaMap::new(); for i in 1..=10u32 { m.insert(i, i); } (m.iter().count(), m.iter().map(|(k, v)| k + v).sum::<u32>()) }, (10, 110));
}
