use solution::*;
use std::collections::BTreeMap;

#[allow(dead_code)]
fn rule(field: u32, test: Test) -> Rule {
    Rule { field, test }
}

#[test]
fn several_rules_on_one_field_all_hold() {
    let rules = [rule(1, Test::AtLeast(18)), rule(1, Test::Below(30)), rule(2, Test::Exactly(4)), rule(1, Test::Above(20))];
    check!("ages 21 to 29, room 4", allowed(&rules), Some(BTreeMap::from([(1, (21, 29)), (2, (4, 4))])));
}

#[test]
fn a_contradiction() {
    check!("below 5 and above 5", allowed(&[rule(0, Test::Below(5)), rule(0, Test::Above(5))]), None);
}

#[test]
fn no_rules() {
    check!("no field is mentioned", allowed(&[]), Some(BTreeMap::new()));
}

#[test]
fn the_entries_in_a_range() {
    let entries = [(1, 7), (3, 2), (3, 9), (8, 1)];
    check!("2 to 3", between(&entries, 2, 3), &entries[1..3]);
    check!("everything", between(&entries, 0, 100), &entries[..]);
    check!("a gap", between(&entries, 4, 7).len(), 0);
}

#[test]
fn equal_bounds_leave_one_value() {
    let rules = [rule(3, Test::AtLeast(7)), rule(3, Test::AtMost(7))];
    check!("only 7", allowed(&rules), Some(BTreeMap::from([(3, (7, 7))])));
}
