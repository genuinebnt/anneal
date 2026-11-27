use solution::*;

#[allow(dead_code)]
fn col(table: usize) -> Column {
    Column { table, column: 0 }
}
#[allow(dead_code)]
fn small(table: usize) -> Cond {
    Cond::Compare(col(table), Cmp::Lt, 10)
}
#[allow(dead_code)]
fn same(a: usize, b: usize) -> Cond {
    Cond::Same(col(a), col(b))
}
#[allow(dead_code)]
fn scan(t: usize) -> Logical {
    Logical::Scan(t)
}
#[allow(dead_code)]
fn filter(c: Cond, input: Logical) -> Logical {
    Logical::Filter(c, Box::new(input))
}
#[allow(dead_code)]
fn cross(a: Logical, b: Logical) -> Logical {
    Logical::Cross(Box::new(a), Box::new(b))
}
#[allow(dead_code)]
fn join(c: Cond, a: Logical, b: Logical) -> Logical {
    Logical::Join(c, Box::new(a), Box::new(b))
}

#[test]
fn tables_a_condition_reads() {
    check!("t0 = t3", tables_of(&same(0, 3)), 0b1001);
    check!("any(t1, t2)", tables_of(&Cond::Any(vec![small(1), small(2)])), 0b110);
    check!("all of nothing", tables_of(&Cond::All(vec![])), 0);
}

#[test]
fn each_condition_sinks_to_its_table_and_equalities_become_joins() {
    let plan = filter(Cond::All(vec![same(0, 1), small(0), small(1)]), cross(scan(0), scan(1)));
    let want = join(same(0, 1), filter(small(0), scan(0)), filter(small(1), scan(1)));
    check!(format!("{plan:?}"), push_down(&plan), want);
}

#[test]
fn a_condition_stops_at_the_lowest_node_with_all_its_tables() {
    let plan = filter(same(1, 0), cross(cross(scan(0), scan(1)), scan(2)));
    let want = cross(join(same(1, 0), scan(0), scan(1)), scan(2));
    check!(format!("{plan:?}"), push_down(&plan), want);
}

#[test]
fn nothing_to_move() {
    check!("one table", push_down(&scan(5)), scan(5));
    check!("a cross product", push_down(&cross(scan(0), scan(1))), cross(scan(0), scan(1)));
}

#[test]
fn two_parts_for_one_table_share_a_filter() {
    let other = Cond::Compare(col(0), Cmp::Gt, 2);
    let plan = filter(small(0), filter(other.clone(), scan(0)));
    let pushed = push_down(&plan);
    let ok = matches!(&pushed, Logical::Filter(Cond::All(parts), input)
        if parts.len() == 2 && parts.contains(&small(0)) && parts.contains(&other) && **input == scan(0));
    check!(format!("{pushed:?}: one Filter with an All of the two parts"), ok, true);
}
