use solution::*;

#[allow(dead_code)]
fn forgets_the_filters(query: &Query, catalog: &Catalog) -> Plan {
    fn strip(plan: Plan) -> Plan {
        let Plan { node, mut inputs, rows, cost, order } = plan;
        if matches!(node, Node::Filter(_)) {
            return strip(inputs.remove(0));
        }
        Plan { node, inputs: inputs.into_iter().map(strip).collect(), rows, cost, order }
    }
    strip(optimize(query, catalog).expect("a valid query"))
}

/// The counterexample really is one, and it is small.
#[allow(dead_code)]
fn proves_it(planner: &dyn Fn(&Query, &Catalog) -> Plan, case: &Case) -> bool {
    let small = case.tables.len() <= CASE_TABLES && case.tables.iter().all(|t| t.rows.len() <= CASE_ROWS && t.width <= CASE_WIDTH);
    let plan = planner(&case.query, &catalog_of(&case.tables));
    let mut got = run(&plan, &MemStore::new(case.tables.clone()));
    let mut want = plain_answer(&case.query, &case.tables);
    got.sort();
    want.sort();
    small && got != want
}

#[test]
fn a_planner_that_forgets_filters_is_caught() {
    let found = find_wrong_answer(&forgets_the_filters, 1, 2000);
    check!("a counterexample", found.is_some_and(|c| proves_it(&forgets_the_filters, &c)), true);
}

#[test]
fn a_correct_planner_is_not_accused() {
    let correct = |q: &Query, c: &Catalog| optimize(q, c).expect("a valid query");
    check!("no counterexample", find_wrong_answer(&correct, 2, 40).is_none(), true);
}

#[test]
fn no_tries_find_nothing() {
    check!("a budget of 0", find_wrong_answer(&forgets_the_filters, 3, 0).is_none(), true);
}

#[test]
fn the_case_is_found_for_other_seeds_too() {
    for seed in [7, 8] {
        let found = find_wrong_answer(&forgets_the_filters, seed, 2000);
        check!(format!("seed {seed}"), found.is_some_and(|c| proves_it(&forgets_the_filters, &c)), true);
    }
}

#[test]
fn the_planner_is_asked_no_more_than_allowed() {
    let asked = std::cell::Cell::new(0);
    let planner = |q: &Query, c: &Catalog| {
        asked.set(asked.get() + 1);
        optimize(q, c).expect("a valid query")
    };
    find_wrong_answer(&planner, 4, 25);
    check!("at most 25 questions", asked.get() <= 25, true);
}
