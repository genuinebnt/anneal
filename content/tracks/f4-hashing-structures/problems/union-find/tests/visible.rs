use solution::*;

#[test]
fn core_links_join_groups() {
    let mut g = Groups::new(4);
    check!("0-1", (g.union(0, 1), g.count()), (true, 3));
    check!("2-3", (g.union(2, 3), g.count()), (true, 2));
    check!("(0~3, 0~1)", (g.connected(0, 3), g.connected(0, 1)), (false, true));
    check!("1-2 joins everything", (g.union(1, 2), g.count(), g.connected(0, 3)), (true, 1, true));
}

#[test]
fn core_a_redundant_link_changes_nothing() {
    let mut g = Groups::new(3);
    g.union(0, 1);
    check!("again, reversed, and to itself", (g.union(1, 0), g.union(2, 2), g.count()), (false, false, 2));
}

#[test]
fn core_no_machines() {
    let g = Groups::new(0);
    check!("count", g.count(), 0);
}

#[test]
fn part2_sizes() {
    let mut g = Groups::new(5);
    check!("largest at the start", g.largest(), 1);
    g.union(0, 1);
    g.union(1, 2);
    check!("(size of 2, size of 4, largest)", (g.group_size(2), g.group_size(4), g.largest()), (3, 1, 3));
}

#[test]
fn part3_add_a_machine() {
    let mut g = Groups::new(2);
    let id = g.add();
    check!("(id, count)", (id, g.count()), (2, 3));
    g.union(id, 0);
    check!("linked", (g.connected(2, 0), g.group_size(0)), (true, 2));
}

#[test]
fn part4_undo() {
    let mut g = Groups::new(4);
    g.union(0, 1);
    let mark = g.checkpoint();
    g.union(2, 3);
    g.union(1, 2);
    g.rollback(mark);
    check!("(count, 0~1, 2~3, largest)", (g.count(), g.connected(0, 1), g.connected(2, 3), g.largest()), (3, true, false, 2));
}
