use solution::*;
use std::cell::Cell;

#[allow(dead_code)]
fn gather(plan: &Logical, own: &mut Vec<(usize, Cond)>, between: &mut Vec<Cond>) {
    match plan {
        Logical::Scan(_) => {}
        Logical::Filter(c, x) => match &**x {
            Logical::Scan(t) => {
                let mut parts = Vec::new();
                conjuncts(c, &mut parts);
                own.extend(parts.into_iter().map(|p| (*t, p)));
            }
            other => gather(other, own, between),
        },
        Logical::Cross(a, b) | Logical::Join(_, a, b) => {
            if let Logical::Join(c, ..) = plan {
                conjuncts(c, between);
            }
            gather(a, own, between);
            gather(b, own, between);
        }
    }
}

/// A planner that makes fixed choices: tables in the order written, an index whenever there is one, and always
/// the same kind of join (0 nested loop, 1 hash, 2 merge) where it can. Every choice is correct.
#[allow(dead_code)]
fn forced(kind: usize, query: &Query, catalog: &Catalog) -> Plan {
    let (mut own, mut between) = (Vec::new(), Vec::new());
    gather(&push_down(&naive_plan(query)), &mut own, &mut between);
    let leaf = |t: usize| {
        let conditions: Vec<Cond> = own.iter().filter(|(x, _)| *x == t).map(|(_, c)| c.clone()).collect();
        leaf_plans(catalog, t, &conditions).pop().unwrap()
    };
    let mut plan = leaf(query.tables[0]);
    for &t in &query.tables[1..] {
        let (seen, now) = (plan_tables(&plan), plan_tables(&plan) | (1 << t));
        let on: Vec<Cond> = between.iter().filter(|c| tables_of(c) & !now == 0 && tables_of(c) & !seen != 0 && tables_of(c) & seen != 0).cloned().collect();
        let mut ways = join_plans(&plan, &leaf(t), &on, 1.0);
        let pick = kind.min(ways.len() - 1);
        plan = ways.swap_remove(pick);
    }
    Plan::new(Node::Project(query.output.clone()), vec![plan])
}

/// Apply a change to every node, bottom up.
#[allow(dead_code)]
fn change(mut plan: Plan, edit: &dyn Fn(Plan) -> Plan) -> Plan {
    plan.inputs = plan.inputs.into_iter().map(|p| change(p, edit)).collect();
    edit(plan)
}

/// Apply a change to every condition inside a plan's filters.
#[allow(dead_code)]
fn rewrite(c: &Cond, edit: &dyn Fn(Cond) -> Cond) -> Cond {
    let inner = match c {
        Cond::All(p) => Cond::All(p.iter().map(|x| rewrite(x, edit)).collect()),
        Cond::Any(p) => Cond::Any(p.iter().map(|x| rewrite(x, edit)).collect()),
        other => other.clone(),
    };
    edit(inner)
}

#[allow(dead_code)]
fn faulty(fault: usize, query: &Query, catalog: &Catalog) -> Plan {
    let kind = match fault {
        1 => 1,
        2 => 2,
        _ => 0,
    };
    let good = forced(kind, query, catalog);
    let conditions = |plan: Plan, edit: &dyn Fn(Cond) -> Cond| {
        change(plan, &|mut p| {
            if let Node::Filter(c) = &p.node {
                p.node = Node::Filter(rewrite(c, edit));
            }
            p
        })
    };
    match fault {
        // the index range stops one short
        0 => change(good, &|mut p| {
            if let Node::IndexScan { high, .. } = &mut p.node {
                *high = high.saturating_sub(1);
            }
            p
        }),
        // a hash join drops its extra condition
        1 => change(good, &|mut p| {
            if let Node::HashJoin { rest, .. } = &mut p.node {
                *rest = None;
            }
            p
        }),
        // a merge join over inputs nobody sorted
        2 => change(good, &|mut p| if matches!(p.node, Node::Sort(_)) { p.inputs.remove(0) } else { p }),
        // "<=" becomes "<"
        3 => conditions(good, &|c| match c {
            Cond::Compare(x, Cmp::Le, v) => Cond::Compare(x, Cmp::Lt, v),
            other => other,
        }),
        // "or" is treated as "and"
        4 => conditions(good, &|c| match c {
            Cond::Any(p) => Cond::All(p),
            other => other,
        }),
        // only the first value of a list counts
        5 => conditions(good, &|c| match c {
            Cond::OneOf(x, mut v) => {
                v.truncate(1);
                Cond::OneOf(x, v)
            }
            other => other,
        }),
        // an index is read in place of a table even when no condition asks for it: rows with a missing value vanish
        6 => {
            let indexed: Vec<Option<usize>> = catalog.tables.iter().map(|t| t.indexed.first().copied()).collect();
            change(good, &|mut p| {
                if let Node::Scan(t) = p.node {
                    if let Some(column) = indexed[t] {
                        p.node = Node::IndexScan { table: t, column, low: i64::MIN, high: i64::MAX };
                    }
                }
                p
            })
        }
        // a nested loop checks only the first of its conditions
        7 => change(good, &|mut p| {
            if let Node::NestedLoop(Some(Cond::All(parts))) = &p.node {
                p.node = Node::NestedLoop(parts.first().cloned());
            }
            p
        }),
        // the last two output columns trade places
        8 => change(good, &|mut p| {
            if let Node::Project(c) = &mut p.node {
                let n = c.len();
                if n > 1 {
                    c.swap(n - 1, n - 2);
                }
            }
            p
        }),
        // a table is joined in once more than asked: the same rows, repeated
        _ => {
            let first = query.tables[0];
            let mut p = good;
            let body = p.inputs.remove(0);
            let again = Plan::new(Node::NestedLoop(None), vec![body, Plan::new(Node::Scan(first), vec![])]);
            Plan::new(p.node, vec![again])
        }
    }
}

#[allow(dead_code)]
fn proves_it(planner: &dyn Fn(&Query, &Catalog) -> Plan, case: &Case) -> Result<(), String> {
    if case.tables.len() > CASE_TABLES || case.tables.iter().any(|t| t.rows.len() > CASE_ROWS || t.width > CASE_WIDTH) {
        return Err("the case is bigger than the limits".to_string());
    }
    let catalog = catalog_of(&case.tables);
    if case.query.tables.iter().any(|&t| t >= case.tables.len()) || optimize(&case.query, &catalog).is_err() {
        return Err(format!("not a valid query: {:?}", case.query));
    }
    let plan = planner(&case.query, &catalog);
    let mut got = run(&plan, &MemStore::new(case.tables.clone()));
    let mut want = plain_answer(&case.query, &case.tables);
    got.sort();
    want.sort();
    if got == want {
        return Err(format!("the planner is right on {case:?}"));
    }
    Ok(())
}

#[allow(dead_code)]
fn caught(fault: usize) {
    let planner = move |q: &Query, c: &Catalog| faulty(fault, q, c);
    for seed in [0, u64::MAX] {
        let calls = Cell::new(0usize);
        let counted = |q: &Query, c: &Catalog| {
            calls.set(calls.get() + 1);
            planner(q, c)
        };
        let found = find_wrong_answer(&counted, seed, 3000);
        check!(format!("fault {fault}, seed {seed}: at most 3000 questions"), calls.get() <= 3000, true);
        check!(format!("fault {fault}, seed {seed}: found"), found.is_some(), true);
        check!(format!("fault {fault}, seed {seed}: the case proves it"), proves_it(&planner, &found.unwrap()), Ok(()));
    }
}

#[test]
fn an_index_range_that_stops_short() {
    caught(0);
}
#[test]
fn a_hash_join_that_drops_a_condition() {
    caught(1);
}
#[test]
fn a_merge_join_without_sorting() {
    caught(2);
}
#[test]
fn less_or_equal_read_as_less() {
    caught(3);
}
#[test]
fn or_read_as_and() {
    caught(4);
}
#[test]
fn a_list_cut_to_one_value() {
    caught(5);
}
#[test]
fn an_index_that_loses_missing_values() {
    caught(6);
}
#[test]
fn a_loop_that_checks_one_condition() {
    caught(7);
}
#[test]
fn output_columns_that_trade_places() {
    caught(8);
}
#[test]
fn rows_that_come_out_too_often() {
    caught(9);
}

#[test]
fn correct_planners_with_unusual_choices_are_not_accused() {
    for kind in 0..3 {
        let calls = Cell::new(0usize);
        let planner = |q: &Query, c: &Catalog| {
            calls.set(calls.get() + 1);
            forced(kind, q, c)
        };
        let found = find_wrong_answer(&planner, 5 + kind as u64, 60);
        check!(format!("forced join kind {kind}: {found:?}"), found.is_none(), true);
        check!(format!("forced join kind {kind}: asked at most 60 times, and at least once"), (1..=60).contains(&calls.get()), true);
    }
}

#[test]
fn the_same_seed_finds_the_same_case() {
    let planner = |q: &Query, c: &Catalog| faulty(3, q, c);
    let show = |c: Option<Case>| format!("{c:?}");
    check!("seed 9 twice", show(find_wrong_answer(&planner, 9, 3000)), show(find_wrong_answer(&planner, 9, 3000)));
}

#[test]
fn no_tries_means_no_questions() {
    let planner = |_: &Query, _: &Catalog| -> Plan { panic!("asked with a budget of 0") };
    check!("0 tries", find_wrong_answer(&planner, 1, 0).is_none(), true);
}
