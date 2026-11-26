use solution::*;

#[allow(dead_code)]
fn pair() -> (OrSet<&'static str>, OrSet<&'static str>) {
    (OrSet::new(1), OrSet::new(2))
}

#[test]
fn local_add_and_remove() {
    let (mut a, _) = pair();
    a.add("milk");
    a.add("eggs");
    check!("remove eggs, then again", (a.remove(&"eggs"), a.remove(&"eggs"), a.items()), (true, false, vec!["milk"]));
}

#[test]
fn merge_is_a_union_of_additions() {
    let (mut a, mut b) = pair();
    a.add("x");
    b.add("y");
    a.merge(&b);
    check!("a after merging b; b unchanged", (a.items(), b.items()), (vec!["x", "y"], vec!["y"]));
}

#[test]
fn a_remove_travels() {
    let (mut a, mut b) = pair();
    a.add("tea");
    b.merge(&a);
    b.remove(&"tea");
    a.merge(&b);
    check!("a learns of b's remove", a.contains(&"tea"), false);
}

#[test]
fn concurrent_add_wins() {
    let (mut a, mut b) = pair();
    a.add("jam");
    b.merge(&a);
    a.remove(&"jam");
    b.add("jam");
    a.merge(&b);
    b.merge(&a);
    check!("a removes while b adds again", (a.items(), b.items()), (vec!["jam"], vec!["jam"]));
}

#[test]
fn can_be_added_again() {
    let (mut a, _) = pair();
    a.add("a");
    a.remove(&"a");
    a.add("a");
    check!("add, remove, add", a.items(), vec!["a"]);
}
