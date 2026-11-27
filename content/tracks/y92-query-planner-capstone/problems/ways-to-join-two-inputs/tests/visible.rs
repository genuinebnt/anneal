use solution::*;

#[allow(dead_code)]
fn col(table: usize, column: usize) -> Column {
    Column { table, column }
}

#[allow(dead_code)]
fn input(table: usize, rows: f64, cost: f64, order: Option<Column>) -> Plan {
    let mut plan = Plan::new(Node::Scan(table), vec![]);
    (plan.rows, plan.cost, plan.order) = (rows, cost, order);
    plan
}

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn without_an_equality_only_the_nested_loop() {
    let (a, b) = (input(0, 100.0, 10.0, None), input(1, 50.0, 5.0, None));
    let plans = join_plans(&a, &b, &[], 5000.0);
    check!("one plan", plans.len(), 1);
    check!("nested loop", plans[0].node.clone(), Node::NestedLoop(None));
    check!("cost: 10 + 5 + 100 * 50 * 0.0025 + 5000 * 0.01", close(plans[0].cost, 77.5), true);
}

#[test]
fn with_an_equality_three_ways() {
    let (a, b) = (input(0, 1000.0, 20.0, None), input(1, 500.0, 10.0, None));
    let on = [Cond::Same(col(1, 0), col(0, 1))];
    let plans = join_plans(&a, &b, &on, 1000.0);
    check!("three plans", plans.len(), 3);
    check!(
        "the hash join names the left key first",
        plans[1].node.clone(),
        Node::HashJoin { left_key: col(0, 1), right_key: col(1, 0), rest: None }
    );
    check!("hash cost: 30 + 500 * 0.02 + 1000 * 0.01 + 1000 * 0.01", close(plans[1].cost, 60.0), true);
    check!("the merge join sorts both inputs", plans[2].inputs.iter().map(|p| p.node.clone()).collect::<Vec<_>>(), vec![Node::Sort(col(0, 1)), Node::Sort(col(1, 0))]);
    check!("and its output is sorted by the left key", plans[2].order, Some(col(0, 1)));
}

#[test]
fn a_plan_set_keeps_the_cheapest_for_each_order() {
    let mut set = PlanSet::new();
    check!("empty", set.cheapest().cloned(), None);
    set.insert(input(0, 1.0, 10.0, None));
    set.insert(input(0, 1.0, 12.0, Some(col(0, 0))));
    set.insert(input(0, 1.0, 8.0, None));
    set.insert(input(0, 1.0, 9.0, None));
    check!("two plans are kept", set.plans().len(), 2);
    check!("the cheapest", set.cheapest().map(|p| p.cost), Some(8.0));
}

#[test]
fn a_sorted_input_is_not_sorted_again() {
    let (a, b) = (input(0, 1000.0, 20.0, Some(col(0, 0))), input(1, 500.0, 10.0, None));
    let plans = join_plans(&a, &b, &[Cond::Same(col(0, 0), col(1, 0))], 100.0);
    check!("the first input is used as it is", plans[2].inputs[0].clone(), a);
    check!("the second is sorted", plans[2].inputs[1].node.clone(), Node::Sort(col(1, 0)));
}

#[test]
fn the_other_conditions_stay_on_the_join() {
    let (a, b) = (input(0, 10.0, 1.0, None), input(1, 10.0, 1.0, None));
    let extra = Cond::Compare(col(1, 1), Cmp::Lt, 3);
    let plans = join_plans(&a, &b, &[extra.clone(), Cond::Same(col(0, 0), col(1, 0))], 5.0);
    check!("hash join", plans[1].node.clone(), Node::HashJoin { left_key: col(0, 0), right_key: col(1, 0), rest: Some(extra) });
}
