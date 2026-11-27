use solution::*;

#[allow(dead_code)]
fn col(table: usize) -> Column {
    Column { table, column: 0 }
}

#[allow(dead_code)]
fn leaf(table: usize, rows: f64) -> PlanSet {
    let mut plan = Plan::new(Node::Scan(table), vec![]);
    (plan.rows, plan.cost) = (rows, rows * 0.01);
    let mut set = PlanSet::new();
    set.insert(plan);
    set
}

#[allow(dead_code)]
fn edge(a: usize, b: usize, selectivity: f64) -> Edge {
    Edge { leaves: (1 << a) | (1 << b), condition: Cond::Same(col(a), col(b)), selectivity }
}

/// Three tables in a row: 0 - 1 - 2.
#[allow(dead_code)]
fn chain() -> JoinGraph {
    let sizes = [1000.0, 100.0, 10.0];
    JoinGraph {
        leaves: (0..3).map(|i| leaf(i, sizes[i])).collect(),
        leaf_rows: sizes.to_vec(),
        edges: vec![edge(0, 1, 0.01), edge(1, 2, 0.1)],
    }
}

#[test]
fn rows_of_a_set_of_tables() {
    let graph = chain();
    check!("one table", rows_of(&graph, 0b001), 1000.0);
    check!("0 and 1: 1000 * 100 * 0.01", rows_of(&graph, 0b011).round(), 1000.0);
    check!("0 and 2: no condition", rows_of(&graph, 0b101), 10_000.0);
    check!("all three", rows_of(&graph, 0b111).round(), 1000.0);
}

#[test]
fn tables_without_a_condition_are_not_paired() {
    let graph = chain();
    let mut pairs = Vec::new();
    let mut join = |a: &Plan, b: &Plan, on: &[Cond], rows: f64| {
        pairs.push((plan_tables(a), plan_tables(b), on.len()));
        join_plans(a, b, on, rows)
    };
    let best = best_order(&graph, &mut join);
    check!("a plan was found", best.cheapest().map(plan_tables), Some(0b111));
    check!("tables 0 and 2 were never paired alone", pairs.iter().any(|&(a, b, _)| a | b == 0b101), false);
    check!("every pairing had a condition", pairs.iter().all(|p| p.2 > 0), true);
}

#[test]
fn unconnected_tables_are_paired_when_there_is_no_other_way() {
    let graph = JoinGraph { leaves: vec![leaf(0, 3.0), leaf(1, 4.0)], leaf_rows: vec![3.0, 4.0], edges: vec![] };
    let mut join = |a: &Plan, b: &Plan, on: &[Cond], rows: f64| join_plans(a, b, on, rows);
    let best = best_order(&graph, &mut join);
    check!("12 rows", best.cheapest().map(|p| p.rows), Some(12.0));
}

#[test]
fn one_table_is_its_own_answer() {
    let graph = JoinGraph { leaves: vec![leaf(0, 5.0)], leaf_rows: vec![5.0], edges: vec![] };
    let mut join = |_: &Plan, _: &Plan, _: &[Cond], _: f64| -> Vec<Plan> { panic!("nothing to join") };
    check!("the leaf", best_order(&graph, &mut join).cheapest().map(|p| p.node.clone()), Some(Node::Scan(0)));
}

#[test]
fn a_pair_is_tried_in_both_directions() {
    let graph = chain();
    let mut pairs = Vec::new();
    let mut join = |a: &Plan, b: &Plan, on: &[Cond], rows: f64| {
        pairs.push((plan_tables(a), plan_tables(b)));
        join_plans(a, b, on, rows)
    };
    best_order(&graph, &mut join);
    check!("0 outside, 1 inside", pairs.contains(&(0b001, 0b010)), true);
    check!("1 outside, 0 inside", pairs.contains(&(0b010, 0b001)), true);
}
