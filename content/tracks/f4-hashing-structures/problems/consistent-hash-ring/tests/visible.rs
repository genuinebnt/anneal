use solution::*;

#[test]
fn empty_ring() {
    check!(r#"no nodes"#, { let r = Ring::new(10); let n = r.get_n("k", 3).len(); (r.get("k").is_none(), n, r.node_count()) }, (true, 0, 0));
}

#[test]
fn single_node_owns_everything() {
    check!(r#"one node"#, { let mut r = Ring::new(10); r.add_node("a"); (0..20).all(|i| r.get(&format!("k{i}")) == Some("a")) }, true);
}

#[test]
fn lookups_are_deterministic() {
    check!(r#"same ring, same answer"#, { let mut r = Ring::new(50); for n in ["a", "b", "c"] { r.add_node(n); } let x: Vec<_> = (0..30).map(|i| r.get(&format!("k{i}"))).collect(); let y: Vec<_> = (0..30).map(|i| r.get(&format!("k{i}"))).collect(); x == y }, true);
}

#[test]
fn remove_node() {
    check!(r#"its keys go elsewhere"#, { let mut r = Ring::new(50); for n in ["a", "b"] { r.add_node(n); } let removed = r.remove_node("a"); let again = r.remove_node("a"); (removed, again, r.node_count(), (0..50).all(|i| r.get(&format!("k{i}")) == Some("b"))) }, (true, false, 1, true));
}

#[test]
fn get_n_gives_distinct_nodes() {
    check!(r#"three replicas of three nodes"#, { let mut r = Ring::new(20); for n in ["a", "b", "c"] { r.add_node(n); } let mut v: Vec<String> = r.get_n("some-key", 3).into_iter().map(String::from).collect(); let first = v[0].clone(); v.sort(); let same = first == r.get("some-key").unwrap(); (v, same) }, (vec!["a".to_string(), "b".to_string(), "c".to_string()], true));
}
