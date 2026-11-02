use solution::*;

#[test]
fn get_n_is_capped_by_membership() {
    check!(r#"ask for more than exist"#, { let mut r = Ring::new(20); r.add_node("a"); r.add_node("b"); (r.get_n("k", 5).len(), r.get_n("k", 0).len()) }, (2, 0));
}

#[test]
fn adding_twice_is_a_no_op() {
    check!(r#"idempotent"#, { let mut r = Ring::new(20); r.add_node("a"); let before: Vec<_> = (0..20).map(|i| r.get(&format!("k{i}")).map(String::from)).collect(); r.add_node("a"); let after: Vec<_> = (0..20).map(|i| r.get(&format!("k{i}")).map(String::from)).collect(); (before == after, r.node_count()) }, (true, 1));
}

#[test]
fn wraps_around_the_top() {
    check!(r#"every key has an owner even past the last point"#, { let mut r = Ring::new(1); r.add_node("only"); (0..2000).all(|i| r.get(&format!("wrap-{i}")).is_some()) }, true);
}

#[test]
fn removing_an_unknown_node() {
    check!(r#"no change"#, { let mut r = Ring::new(10); r.add_node("a"); (r.remove_node("zzz"), r.node_count()) }, (false, 1));
}

fn assignment(r: &Ring, keys: usize) -> Vec<String> {
    (0..keys).map(|i| r.get(&format!("key-{i}")).unwrap().to_string()).collect()
}

#[test]
fn load_is_balanced_with_virtual_nodes() {
    let mut r = Ring::new(200);
    for i in 0..10 {
        r.add_node(&format!("node-{i}"));
    }
    let keys = 20_000;
    let mut counts = std::collections::HashMap::new();
    for owner in assignment(&r, keys) {
        *counts.entry(owner).or_insert(0usize) += 1;
    }
    check!("all ten nodes own keys", counts.len(), 10);
    let fair = keys / 10;
    for (n, c) in &counts {
        assert!(*c > fair * 6 / 10 && *c < fair * 14 / 10, "{n} owns {c} keys; fair share is {fair}");
    }
}

#[test]
fn adding_a_node_moves_only_a_fair_share_to_the_new_node() {
    let mut r = Ring::new(200);
    for i in 0..10 {
        r.add_node(&format!("node-{i}"));
    }
    let keys = 20_000;
    let before = assignment(&r, keys);
    r.add_node("node-new");
    let after = assignment(&r, keys);
    let mut moved = 0;
    for (b, a) in before.iter().zip(&after) {
        if b != a {
            moved += 1;
            assert_eq!(a, "node-new", "a key moved between two old nodes");
        }
    }
    let fair = keys / 11;
    assert!(moved > fair / 2 && moved < fair * 3 / 2, "{moved} keys moved; about {fair} expected");
}

#[test]
fn removing_a_node_moves_only_its_own_keys() {
    let mut r = Ring::new(200);
    for i in 0..10 {
        r.add_node(&format!("node-{i}"));
    }
    let before = assignment(&r, 20_000);
    r.remove_node("node-3");
    let after = assignment(&r, 20_000);
    for (b, a) in before.iter().zip(&after) {
        if b != "node-3" {
            assert_eq!(b, a, "a key that did not belong to the removed node moved");
        } else {
            assert_ne!(a, "node-3");
        }
    }
}

#[test]
fn replicas_are_distinct_and_start_with_the_primary() {
    let mut r = Ring::new(100);
    for i in 0..6 {
        r.add_node(&format!("n{i}"));
    }
    for i in 0..500 {
        let key = format!("key-{i}");
        let reps = r.get_n(&key, 3);
        assert_eq!(reps.len(), 3);
        assert_eq!(reps[0], r.get(&key).unwrap());
        let mut s = reps.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), 3, "duplicate replicas {reps:?}");
    }
}
