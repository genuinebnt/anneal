use solution::*;
use std::cell::Cell;
use anneal_prelude::Rng;

/// A store that counts how many rows have been read from each table.
#[allow(dead_code)]
struct Counting {
    tables: Vec<(usize, Vec<Row>)>,
    indexes: Vec<Vec<Vec<(i64, usize)>>>,
    reads: Vec<Cell<usize>>,
}

#[allow(dead_code)]
impl Counting {
    fn new(tables: Vec<(usize, Vec<Row>)>) -> Counting {
        let indexes = tables
            .iter()
            .map(|(width, rows)| {
                (0..*width)
                    .map(|c| {
                        let mut ix: Vec<(i64, usize)> = rows.iter().enumerate().filter_map(|(i, r)| r[c].map(|v| (v, i))).collect();
                        ix.sort();
                        ix
                    })
                    .collect()
            })
            .collect();
        let reads = tables.iter().map(|_| Cell::new(0)).collect();
        Counting { tables, indexes, reads }
    }
    fn reads(&self) -> Vec<usize> {
        self.reads.iter().map(Cell::get).collect()
    }
}

impl Store for Counting {
    fn width(&self, t: usize) -> usize {
        self.tables[t].0
    }
    fn row_count(&self, t: usize) -> usize {
        self.tables[t].1.len()
    }
    fn row(&self, t: usize, i: usize) -> Row {
        self.reads[t].set(self.reads[t].get() + 1);
        self.tables[t].1[i].clone()
    }
    fn index(&self, t: usize, c: usize) -> &[(i64, usize)] {
        &self.indexes[t][c]
    }
}

#[allow(dead_code)]
fn col(table: usize, column: usize) -> Column {
    Column { table, column }
}

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * a.abs().max(1.0)
}

/// A two-table plan to join with a third.
#[allow(dead_code)]
fn random_side(rng: &mut Rng, tables: &[usize]) -> Plan {
    let mut parts: Vec<Plan> = tables.iter().map(|&t| Plan::new(Node::Scan(t), vec![])).collect();
    let mut plan = parts.remove(0);
    for p in parts {
        plan = Plan::new(Node::NestedLoop(None), vec![plan, p]);
    }
    plan.rows = [0.0, 0.5, 1.0, 2.0, 40.0, 1000.0, 123456.0][rng.below(7)];
    plan.cost = rng.int(0, 10_000) as f64 / 8.0;
    let t = *rng.pick(tables);
    plan.order = (rng.below(3) == 0).then(|| col(t, rng.below(2)));
    plan
}

#[allow(dead_code)]
fn side_of(c: &Column, tables: &[usize]) -> bool {
    tables.contains(&c.table)
}

#[allow(dead_code)]
fn combine(mut list: Vec<Cond>) -> Cond {
    if list.len() == 1 { list.remove(0) } else { Cond::All(list) }
}

#[test]
fn tables_of_a_plan() {
    let a = Plan::new(Node::IndexScan { table: 5, column: 0, low: 0, high: 1 }, vec![]);
    let b = Plan::new(Node::Filter(Cond::All(vec![])), vec![Plan::new(Node::Scan(63), vec![])]);
    let both = Plan::new(Node::NestedLoop(None), vec![a, Plan::new(Node::Sort(col(63, 0)), vec![b])]);
    check!("tables 5 and 63", plan_tables(&both), (1u64 << 5) | (1u64 << 63));
}

#[test]
fn random_pairs_get_the_right_plans_and_costs() {
    let mut rng = Rng::new(9701);
    for case in 0..2000 {
        let (left_tables, right_tables) = if rng.bool() { (vec![0, 3], vec![1]) } else { (vec![2], vec![0, 4]) };
        let (a, b) = (random_side(&mut rng, &left_tables), random_side(&mut rng, &right_tables));
        let n = rng.below(4);
        let conditions: Vec<Cond> = (0..n)
            .map(|_| {
                let l = col(*rng.pick(&left_tables), rng.below(2));
                let r = col(*rng.pick(&right_tables), rng.below(2));
                match rng.below(6) {
                    0 => Cond::Any(vec![Cond::Same(l, r), Cond::Compare(l, Cmp::Eq, 1)]),
                    1 => Cond::Compare(l, Cmp::Lt, 3),
                    2 => Cond::Same(r, l),
                    _ => Cond::Same(l, r),
                }
            })
            .collect();
        let rows = [0.0, 1.0, 77.0, 5000.0][rng.below(4)];
        let input = format!("case {case}: left (tables {left_tables:?}, rows {}, cost {}, order {:?}), right (tables {right_tables:?}, rows {}, cost {}, order {:?}), {rows} rows out, conditions {conditions:?}",
            a.rows, a.cost, a.order, b.rows, b.cost, b.order);
        let plans = join_plans(&a, &b, &conditions, rows);
        let key = conditions.iter().enumerate().find_map(|(i, c)| match c {
            Cond::Same(x, y) if side_of(x, &left_tables) && side_of(y, &right_tables) => Some((i, *x, *y)),
            Cond::Same(x, y) if side_of(y, &left_tables) && side_of(x, &right_tables) => Some((i, *y, *x)),
            _ => None,
        });
        check!(format!("{input}: number of plans"), plans.len(), if key.is_some() { 3 } else { 1 });
        for p in &plans {
            check!(format!("{input}: rows of {:?}", p.node), p.rows, rows);
        }
        let all = (!conditions.is_empty()).then(|| combine(conditions.clone()));
        check!(format!("{input}: nested loop node"), plans[0].node.clone(), Node::NestedLoop(all));
        check!(format!("{input}: nested loop inputs and order"), (plans[0].inputs.clone(), plans[0].order), (vec![a.clone(), b.clone()], a.order));
        let nested = a.cost + b.cost + a.rows * b.rows * 0.0025 + rows * 0.01;
        check!(format!("{input}: nested loop cost {} wanted {nested}", plans[0].cost), close(plans[0].cost, nested), true);
        let Some((used, left_key, right_key)) = key else { continue };
        let mut others = conditions.clone();
        others.remove(used);
        let extra = if others.is_empty() { 0.0 } else { rows * 0.0025 };
        let rest = (!others.is_empty()).then(|| combine(others));
        check!(format!("{input}: hash join node"), plans[1].node.clone(), Node::HashJoin { left_key, right_key, rest: rest.clone() });
        check!(format!("{input}: hash join inputs and order"), (plans[1].inputs.clone(), plans[1].order), (vec![a.clone(), b.clone()], None));
        let hash = a.cost + b.cost + b.rows * 0.02 + a.rows * 0.01 + rows * 0.01 + extra;
        check!(format!("{input}: hash cost {} wanted {hash}", plans[1].cost), close(plans[1].cost, hash), true);
        check!(format!("{input}: merge join node"), plans[2].node.clone(), Node::MergeJoin { left_key, right_key, rest });
        check!(format!("{input}: merge order"), plans[2].order, Some(left_key));
        let mut merge = (a.rows + b.rows) * 0.0025 + rows * 0.01 + extra;
        for (side, key, got) in [(&a, left_key, &plans[2].inputs[0]), (&b, right_key, &plans[2].inputs[1])] {
            if side.order == Some(key) {
                check!(format!("{input}: an input already in order {key:?} is used as it is"), got.clone(), side.clone());
                merge += side.cost;
            } else {
                let n = side.rows.max(2.0);
                let cost = side.cost + n * n.log2() * 0.005;
                check!(format!("{input}: a sort on {key:?}"), (got.node.clone(), got.order, got.rows, got.inputs.clone()), (Node::Sort(key), Some(key), side.rows, vec![side.clone()]));
                check!(format!("{input}: sort cost {} wanted {cost}", got.cost), close(got.cost, cost), true);
                merge += cost;
            }
        }
        check!(format!("{input}: merge cost {} wanted {merge}", plans[2].cost), close(plans[2].cost, merge), true);
    }
}

#[test]
fn an_equality_inside_one_side_is_not_a_key() {
    let a = Plan::new(Node::NestedLoop(None), vec![Plan::new(Node::Scan(0), vec![]), Plan::new(Node::Scan(1), vec![])]);
    let b = Plan::new(Node::Scan(2), vec![]);
    let plans = join_plans(&a, &b, &[Cond::Same(col(0, 0), col(1, 0))], 1.0);
    check!("only the nested loop", plans.len(), 1);
}

#[test]
fn every_way_returns_the_same_rows() {
    let mut rng = Rng::new(9702);
    for case in 0..300 {
        let tables: Vec<(usize, Vec<Row>)> = (0..2)
            .map(|_| {
                let n = rng.below(12);
                (2, (0..n).map(|_| (0..2).map(|_| (rng.below(6) > 0).then(|| rng.int(0, 3))).collect()).collect())
            })
            .collect();
        let store = Counting::new(tables.clone());
        let mut conditions = vec![Cond::Same(col(0, rng.below(2)), col(1, rng.below(2)))];
        for _ in 0..rng.below(3) {
            let c = col(rng.below(2), rng.below(2));
            conditions.push(if rng.bool() { Cond::Same(c, col(1 - c.table, rng.below(2))) } else { Cond::Compare(c, Cmp::Ne, rng.int(0, 3)) });
        }
        rng.shuffle(&mut conditions);
        let (a, b) = (Plan::new(Node::Scan(0), vec![]), Plan::new(Node::Scan(1), vec![]));
        let plans = join_plans(&a, &b, &conditions, 1.0);
        let mut want = run(&plans[0], &store);
        want.sort();
        for plan in &plans[1..] {
            let mut got = run(plan, &store);
            got.sort();
            check!(format!("case {case}: {:?} over {tables:?}", plan.node), got, want.clone());
        }
    }
}

#[test]
fn a_plan_set_against_a_plain_list() {
    let mut rng = Rng::new(9703);
    for case in 0..500 {
        let mut set = PlanSet::new();
        let mut all: Vec<Plan> = Vec::new();
        for serial in 0..rng.below(20) {
            let mut plan = Plan::new(Node::Scan(serial), vec![]);
            plan.cost = rng.int(0, 5) as f64;
            plan.order = [None, Some(col(0, 0)), Some(col(0, 1)), Some(col(1, 0))][rng.below(4)];
            all.push(plan.clone());
            set.insert(plan);
        }
        // the first of the cheapest for each order
        let mut want: Vec<Plan> = Vec::new();
        for p in &all {
            let best = all.iter().filter(|q| q.order == p.order).min_by(|a, b| a.cost.total_cmp(&b.cost)).unwrap();
            if !want.contains(best) {
                want.push(best.clone());
            }
        }
        let mut got = set.plans().to_vec();
        let key = |p: &Plan| format!("{:?}", p);
        got.sort_by_key(key);
        want.sort_by_key(key);
        check!(format!("case {case}: after inserting {all:?}"), got, want);
        let low = all.iter().map(|p| p.cost).min_by(f64::total_cmp);
        check!(format!("case {case}: cheapest"), set.cheapest().map(|p| p.cost), low);
    }
}

#[test]
fn the_first_of_two_keys_is_used() {
    let (a, b) = (Plan::new(Node::Scan(0), vec![]), Plan::new(Node::Scan(1), vec![]));
    let first = Cond::Same(col(0, 1), col(1, 1));
    let second = Cond::Same(col(0, 0), col(1, 0));
    let plans = join_plans(&a, &b, &[first, second.clone()], 1.0);
    check!("keys of the first, the second stays", plans[2].node.clone(), Node::MergeJoin { left_key: col(0, 1), right_key: col(1, 1), rest: Some(second) });
}

#[test]
fn tiny_inputs_are_sorted_at_the_price_of_two_rows() {
    let (mut a, mut b) = (Plan::new(Node::Scan(0), vec![]), Plan::new(Node::Scan(1), vec![]));
    (a.rows, b.rows) = (0.0, 1.0);
    let plans = join_plans(&a, &b, &[Cond::Same(col(0, 0), col(1, 0))], 0.0);
    check!("each sort costs 2 x 1 x 0.005", plans[2].inputs.iter().all(|p| close(p.cost, 0.01)), true);
}

#[test]
fn a_plan_set_never_keeps_two_of_an_order() {
    let mut set = PlanSet::new();
    for cost in [5.0, 4.0, 6.0, 4.0, 3.0] {
        let mut plan = Plan::new(Node::Scan(0), vec![]);
        (plan.cost, plan.order) = (cost, Some(col(0, 0)));
        set.insert(plan);
    }
    check!("one plan, the cheapest", set.plans().iter().map(|p| p.cost).collect::<Vec<_>>(), vec![3.0]);
}
