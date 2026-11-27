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

/// The catalog that describes the tables of a store. Table `i` is called `t{i}`, column `j` is `c{j}`.
#[allow(dead_code)]
fn catalog_for(tables: &[(usize, Vec<Row>)], indexed: &[Vec<usize>]) -> Catalog {
    let tables = tables
        .iter()
        .zip(indexed)
        .enumerate()
        .map(|(t, ((width, rows), indexed))| Table {
            name: format!("t{t}"),
            column_names: (0..*width).map(|c| format!("c{c}")).collect(),
            rows: rows.len(),
            stats: (0..*width).map(|c| summarise(&rows.iter().map(|r| r[c]).collect::<Vec<_>>())).collect(),
            indexed: indexed.clone(),
        })
        .collect();
    Catalog { tables }
}

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * a.abs().max(1.0)
}

#[allow(dead_code)]
fn col(column: usize) -> Column {
    Column { table: 0, column }
}

#[allow(dead_code)]
fn value(rng: &mut Rng) -> i64 {
    match rng.below(14) {
        0 => i64::MIN,
        1 => i64::MAX,
        _ => rng.int(-2, 30),
    }
}

#[allow(dead_code)]
fn condition(rng: &mut Rng) -> Cond {
    let c = col(rng.below(3));
    match rng.below(10) {
        0 => Cond::OneOf(c, vec![rng.int(0, 20), rng.int(0, 20)]),
        1 => Cond::Same(c, col((c.column + 1) % 3)),
        2 => Cond::Any(vec![Cond::Compare(c, Cmp::Lt, value(rng)), Cond::Compare(col(rng.below(3)), Cmp::Eq, value(rng))]),
        _ => Cond::Compare(c, *rng.pick(&[Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge]), value(rng)),
    }
}

#[allow(dead_code)]
fn is_range(c: &Cond, column: usize) -> bool {
    matches!(c, Cond::Compare(x, cmp, _) if *x == col(column) && *cmp != Cmp::Ne)
}

/// How many rows have a value in `column` that satisfies every range condition on it.
#[allow(dead_code)]
fn in_range(rows: &[Row], conditions: &[Cond], column: usize) -> usize {
    let schema = [col(0), col(1), col(2)];
    rows.iter()
        .filter(|r| r[column].is_some() && conditions.iter().filter(|c| is_range(c, column)).all(|c| holds(c, r, &schema)))
        .count()
}

/// The conditions of a filter, as the filter writes them.
#[allow(dead_code)]
fn written(c: &Cond) -> Vec<Cond> {
    match c {
        Cond::All(parts) => parts.clone(),
        other => vec![other.clone()],
    }
}

#[allow(dead_code)]
fn combine(list: &[Cond]) -> Cond {
    if list.len() == 1 { list[0].clone() } else { Cond::All(list.to_vec()) }
}

#[test]
fn random_tables_and_conditions() {
    let mut rng = Rng::new(9601);
    for case in 0..600 {
        let n = rng.below(600);
        let rows: Vec<Row> = (0..n)
            .map(|_| (0..3).map(|c| (rng.below(9) > 0).then(|| rng.int(0, [3, 25, 5000][c]))).collect())
            .collect();
        let data = vec![(3, rows.clone())];
        let mut indexed: Vec<usize> = (0..3).filter(|_| rng.bool()).collect();
        rng.shuffle(&mut indexed);
        let catalog = catalog_for(&data, &[indexed.clone()]);
        let count = rng.below(5);
        let conditions: Vec<Cond> = (0..count).map(|_| condition(&mut rng)).collect();
        let input = format!("case {case}: {n} rows, indexes {indexed:?}, conditions {conditions:?}");
        let plans = leaf_plans(&catalog, 0, &conditions);

        let usable: Vec<usize> = indexed.iter().copied().filter(|&i| conditions.iter().any(|c| is_range(c, i))).collect();
        check!(format!("{input}: number of plans"), plans.len(), 1 + usable.len());
        let total = n as f64 * selectivity(&Cond::All(conditions.clone()), &catalog);

        let store = Counting::new(data.clone());
        let schema = [col(0), col(1), col(2)];
        let truth: Vec<Row> = rows.iter().filter(|r| conditions.iter().all(|c| holds(c, r, &schema))).cloned().collect();
        let mut truth = truth;
        truth.sort();

        let pages = (n.div_ceil(100)).max(1) as f64;
        let scan = pages + n as f64 * 0.01;
        let want_node = if conditions.is_empty() { Node::Scan(0) } else { Node::Filter(combine(&conditions)) };
        check!(format!("{input}: the first plan"), plans[0].node.clone(), want_node);
        check!(format!("{input}: scan cost {}", plans[0].cost), close(plans[0].cost, scan + n as f64 * 0.0025 * conditions.len() as f64), true);
        check!(format!("{input}: scan order"), plans[0].order, None);
        if !conditions.is_empty() {
            let below = &plans[0].inputs[0];
            check!(format!("{input}: under the filter"), (below.node.clone(), below.rows, close(below.cost, scan)), (Node::Scan(0), n as f64, true));
        }

        for (k, plan) in plans.iter().enumerate() {
            check!(format!("{input}: plan {k} says {} rows, the whole condition gives {total}", plan.rows), close(plan.rows, total), true);
            let before = store.reads()[0];
            let mut got = run(plan, &store);
            let reads = store.reads()[0] - before;
            got.sort();
            check!(format!("{input}: rows of plan {k}: {plan:?}"), got, truth.clone());
            if k == 0 {
                continue;
            }
            let column = usable[k - 1];
            check!(format!("{input}: reads of the index plan on column {column}"), reads, in_range(&rows, &conditions, column));
            check!(format!("{input}: order of plan {k}"), plan.order, Some(col(column)));
            let rest: Vec<Cond> = conditions.iter().filter(|c| !is_range(c, column)).cloned().collect();
            let index = if rest.is_empty() {
                plan
            } else {
                check!(format!("{input}: the filter over index {column} keeps the other conditions"), match &plan.node {
                    Node::Filter(c) => written(c),
                    _ => vec![],
                }, written(&combine(&rest)));
                &plan.inputs[0]
            };
            let (low, high) = match index.node {
                Node::IndexScan { table: 0, column: c, low, high } if c == column => (low, high),
                _ => {
                    check!(format!("{input}: expected an index scan on column {column}"), format!("{:?}", index.node), "IndexScan".to_string());
                    unreachable!()
                }
            };
            let matched = catalog.tables[0].stats[column].between(low, high);
            let cost = (3.0 + matched.min(pages)) * 4.0 + matched * 0.01;
            check!(format!("{input}: index scan cost {} wanted {cost}", index.cost), close(index.cost, cost), true);
            check!(format!("{input}: index scan order"), index.order, Some(col(column)));
            if !rest.is_empty() {
                check!(format!("{input}: the index scan's own rows"), close(index.rows, matched), true);
                let want = cost + matched * 0.0025 * rest.len() as f64;
                check!(format!("{input}: filtered index cost"), close(plan.cost, want), true);
            }
        }
    }
}

#[test]
fn contradictions_read_nothing_through_the_index() {
    let rows: Vec<Row> = (0..300).map(|i| vec![Some(i), None, Some(0)]).collect();
    let data = vec![(3, rows)];
    let catalog = catalog_for(&data, &[vec![0]]);
    for (name, conditions) in [
        ("x > MAX", vec![Cond::Compare(col(0), Cmp::Gt, i64::MAX)]),
        ("x < MIN", vec![Cond::Compare(col(0), Cmp::Lt, i64::MIN)]),
        ("x > MAX and x <= 3", vec![Cond::Compare(col(0), Cmp::Gt, i64::MAX), Cond::Compare(col(0), Cmp::Le, 3)]),
        ("x < MIN and x >= 5", vec![Cond::Compare(col(0), Cmp::Lt, i64::MIN), Cond::Compare(col(0), Cmp::Ge, 5)]),
        ("x > 9 and x < 3", vec![Cond::Compare(col(0), Cmp::Gt, 9), Cond::Compare(col(0), Cmp::Lt, 3)]),
        ("x = 4 and x = 5", vec![Cond::Compare(col(0), Cmp::Eq, 4), Cond::Compare(col(0), Cmp::Eq, 5)]),
    ] {
        let plans = leaf_plans(&catalog, 0, &conditions);
        let store = Counting::new(data.clone());
        check!(format!("{name}: two plans"), plans.len(), 2);
        check!(format!("{name}: no rows, no reads"), (run(&plans[1], &store).len(), store.reads()[0]), (0, 0));
    }
}

#[test]
fn an_empty_table() {
    let data = vec![(3, Vec::new())];
    let catalog = catalog_for(&data, &[vec![0]]);
    let plans = leaf_plans(&catalog, 0, &[Cond::Compare(col(0), Cmp::Eq, 1)]);
    check!("two plans, no rows", plans.iter().map(|p| p.rows).collect::<Vec<_>>(), vec![0.0, 0.0]);
    check!("an empty table still has one page", close(plans[0].cost, 1.0), true);
    check!("the index still has to be entered", close(plans[1].cost, 12.0), true);
}

#[test]
fn which_way_is_cheaper_depends_on_how_many_rows_match() {
    let rows: Vec<Row> = (0..20_000).map(|i| vec![Some((i * 7919) % 20_000), Some(0), Some(0)]).collect();
    let catalog = catalog_for(&[(3, rows)], &[vec![0]]);
    let cheapest_is_index = |limit| {
        let plans = leaf_plans(&catalog, 0, &[Cond::Compare(col(0), Cmp::Lt, limit)]);
        plans[1].cost < plans[0].cost
    };
    check!("a few rows: the index", cheapest_is_index(20), true);
    check!("half the table: the scan", cheapest_is_index(10_000), false);
}

#[test]
fn other_tables_columns_are_not_confused() {
    let rows: Vec<Row> = (0..50).map(|i| vec![Some(i)]).collect();
    let data = vec![(1, rows.clone()), (1, rows)];
    let catalog = catalog_for(&data, &[vec![0], vec![0]]);
    let on_one = Column { table: 1, column: 0 };
    let plans = leaf_plans(&catalog, 1, &[Cond::Compare(on_one, Cmp::Eq, 3)]);
    let store = Counting::new(data);
    for plan in &plans {
        check!("rows of table 1", run(plan, &store), vec![vec![Some(3)]]);
    }
    check!("table 0 is never read", store.reads()[0], 0);
}

#[test]
fn indexes_come_in_the_listed_order() {
    let rows: Vec<Row> = (0..40).map(|i| vec![Some(i), Some(i), Some(i)]).collect();
    let catalog = catalog_for(&[(3, rows)], &[vec![2, 0, 1]]);
    let conditions: Vec<Cond> = (0..3).map(|c| Cond::Compare(col(c), Cmp::Ge, 1)).collect();
    let orders: Vec<Option<Column>> = leaf_plans(&catalog, 0, &conditions).iter().map(|p| p.order).collect();
    check!("scan, then columns 2, 0, 1", orders, vec![None, Some(col(2)), Some(col(0)), Some(col(1))]);
}

#[test]
fn one_condition_is_written_alone_and_more_as_a_list() {
    let catalog = catalog_for(&[(3, vec![])], &[vec![]]);
    let a = Cond::Compare(col(0), Cmp::Eq, 1);
    let b = Cond::All(vec![]);
    check!("one", leaf_plans(&catalog, 0, std::slice::from_ref(&b))[0].node.clone(), Node::Filter(b.clone()));
    check!("two, in order", leaf_plans(&catalog, 0, &[b.clone(), a.clone()])[0].node.clone(), Node::Filter(Cond::All(vec![b, a])));
}

#[test]
fn two_ranges_on_one_column_become_one_scan() {
    let rows: Vec<Row> = (0..1000).map(|i| vec![Some(i), None, None]).collect();
    let catalog = catalog_for(&[(3, rows)], &[vec![0]]);
    let conditions = [Cond::Compare(col(0), Cmp::Gt, 10), Cond::Compare(col(0), Cmp::Le, 20), Cond::Compare(col(0), Cmp::Ge, 5)];
    let plans = leaf_plans(&catalog, 0, &conditions);
    check!("no filter left", plans[1].node.clone(), Node::IndexScan { table: 0, column: 0, low: 11, high: 20 });
}
