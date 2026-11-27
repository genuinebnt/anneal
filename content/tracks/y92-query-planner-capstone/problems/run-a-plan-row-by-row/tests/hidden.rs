use solution::*;
use anneal_prelude::Rng;
use std::cell::Cell;

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
fn scan(t: usize) -> Plan {
    Plan::new(Node::Scan(t), vec![])
}

#[allow(dead_code)]
fn numbers(n: i64) -> (usize, Vec<Row>) {
    (1, (0..n).map(|i| vec![Some(i)]).collect())
}

#[test]
fn a_missing_value_satisfies_no_comparison() {
    let schema = [col(0, 0), col(0, 1)];
    let row = [None, None];
    for cmp in [Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge] {
        for v in [i64::MIN, 0, i64::MAX] {
            check!(format!("missing {cmp:?} {v}"), holds(&Cond::Compare(col(0, 0), cmp, v), &row, &schema), false);
        }
    }
    check!("missing = missing", holds(&Cond::Same(col(0, 0), col(0, 1)), &row, &schema), false);
    check!("missing in (0)", holds(&Cond::OneOf(col(0, 0), vec![0]), &row, &schema), false);
    check!("all of nothing", holds(&Cond::All(vec![]), &row, &schema), true);
    check!("any of nothing", holds(&Cond::Any(vec![]), &row, &schema), false);
}

#[test]
fn the_schema_decides_where_a_column_is() {
    let schema = [col(2, 1), col(0, 0)];
    check!("columns in a different order", holds(&Cond::Compare(col(0, 0), Cmp::Eq, 8), &[Some(7), Some(8)], &schema), true);
}

#[test]
fn a_finished_plan_stays_finished_and_reads_no_more() {
    let store = Counting::new(vec![numbers(3), numbers(2)]);
    let plans = [
        scan(0),
        Plan::new(Node::Filter(Cond::Compare(col(0, 0), Cmp::Gt, 99)), vec![scan(0)]),
        Plan::new(Node::NestedLoop(None), vec![scan(0), scan(1)]),
    ];
    for plan in &plans {
        let mut running = open(plan, &store);
        while running.next().is_some() {}
        let before = store.reads();
        for _ in 0..3 {
            check!(format!("after the end of {:?}", plan.node), running.next(), None);
        }
        check!(format!("no more reads after the end of {:?}", plan.node), store.reads(), before);
    }
}

#[test]
fn scans_and_filters_read_no_further_than_they_must() {
    let store = Counting::new(vec![numbers(10_000)]);
    let plan = Plan::new(
        Node::Project(vec![col(0, 0), col(0, 0)]),
        vec![Plan::new(Node::Filter(Cond::OneOf(col(0, 0), vec![4, 7, 9000])), vec![scan(0)])],
    );
    let mut running = open(&plan, &store);
    check!("open reads nothing", store.reads(), vec![0]);
    check!("first", running.next(), Some(vec![Some(4), Some(4)]));
    check!("reads", store.reads(), vec![5]);
    check!("second", running.next(), Some(vec![Some(7), Some(7)]));
    check!("reads", store.reads(), vec![8]);
}

#[test]
fn a_nested_loop_reads_the_inner_side_once_and_the_outer_as_needed() {
    let store = Counting::new(vec![numbers(1000), numbers(5)]);
    let on = Cond::All(vec![Cond::Compare(col(0, 0), Cmp::Ge, 3), Cond::Compare(col(1, 0), Cmp::Ge, 3)]);
    let plan = Plan::new(Node::NestedLoop(Some(on)), vec![scan(0), scan(1)]);
    let mut running = open(&plan, &store);
    check!("open reads nothing", store.reads(), vec![0, 0]);
    for (k, (outer, inner)) in [(3, 3), (3, 4), (4, 3)].into_iter().enumerate() {
        check!(format!("row {k}"), running.next(), Some(vec![Some(outer), Some(inner)]));
        check!(format!("reads after row {k}"), store.reads(), vec![outer as usize + 1, 5]);
    }
    while running.next().is_some() {}
    check!("every row of each side was read exactly once", store.reads(), vec![1000, 5]);
}

#[test]
fn a_nested_loop_with_an_empty_side() {
    let store = Counting::new(vec![numbers(0), numbers(3)]);
    for (a, b) in [(0, 1), (1, 0)] {
        let plan = Plan::new(Node::NestedLoop(None), vec![scan(a), scan(b)]);
        check!(format!("table {a} with table {b}"), run(&plan, &store), Vec::<Row>::new());
    }
}

#[test]
fn index_ranges_at_the_edges() {
    let rows: Vec<Row> = [Some(i64::MIN), Some(5), None, Some(i64::MAX), Some(5)].iter().map(|v| vec![*v]).collect();
    let store = Counting::new(vec![(1, rows)]);
    let pick = |low, high| {
        let plan = Plan::new(Node::IndexScan { table: 0, column: 0, low, high }, vec![]);
        run(&plan, &store).into_iter().map(|r| r[0].unwrap()).collect::<Vec<i64>>()
    };
    check!("everything but the missing value", pick(i64::MIN, i64::MAX), vec![i64::MIN, 5, 5, i64::MAX]);
    check!("only the largest", pick(i64::MAX, i64::MAX), vec![i64::MAX]);
    check!("only the smallest", pick(i64::MIN, i64::MIN), vec![i64::MIN]);
    let before = store.reads();
    check!("low above high", pick(6, 5), Vec::<i64>::new());
    check!("far above high", pick(i64::MAX, i64::MIN), Vec::<i64>::new());
    check!("a gap", pick(6, 100), Vec::<i64>::new());
    check!("empty ranges read no row", store.reads(), before);
}

#[test]
fn an_index_scan_is_pulled_one_row_at_a_time() {
    let store = Counting::new(vec![numbers(5000)]);
    let plan = Plan::new(Node::IndexScan { table: 0, column: 0, low: 100, high: 4000 }, vec![]);
    let mut running = open(&plan, &store);
    check!("open reads nothing", store.reads(), vec![0]);
    check!("first", running.next(), Some(vec![Some(100)]));
    check!("one read", store.reads(), vec![1]);
}

/// Small tables with few distinct values, so that duplicates and missing values are common.
#[allow(dead_code)]
fn random_tables(rng: &mut Rng) -> Vec<(usize, Vec<Row>)> {
    (0..4)
        .map(|_| {
            let n = rng.below(9);
            let rows = (0..n)
                .map(|_| (0..2).map(|_| (rng.below(5) > 0).then(|| rng.int(0, 3))).collect())
                .collect();
            (2, rows)
        })
        .collect()
}

#[allow(dead_code)]
fn random_cond(rng: &mut Rng, columns: &[Column], depth: usize) -> Cond {
    let cmps = [Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge];
    match rng.below(if depth == 0 { 3 } else { 5 }) {
        1 => {
            let a = *rng.pick(columns);
            Cond::Same(a, *rng.pick(columns))
        }
        2 => Cond::OneOf(*rng.pick(columns), vec![rng.int(0, 3), rng.int(0, 3)]),
        3 => {
            let n = rng.below(3);
            Cond::All((0..n).map(|_| random_cond(rng, columns, depth - 1)).collect())
        }
        4 => {
            let n = rng.below(3);
            Cond::Any((0..n).map(|_| random_cond(rng, columns, depth - 1)).collect())
        }
        _ => Cond::Compare(*rng.pick(columns), *rng.pick(&cmps), rng.int(0, 3)),
    }
}

#[allow(dead_code)]
fn model_holds(c: &Cond, row: &Row, columns: &[Column]) -> bool {
    let value = |c: &Column| (0..columns.len()).find(|&i| columns[i] == *c).and_then(|i| row[i]);
    match c {
        Cond::Compare(c, cmp, w) => match value(c) {
            None => false,
            Some(v) => [v == *w, v != *w, v < *w, v <= *w, v > *w, v >= *w][*cmp as usize],
        },
        Cond::Same(a, b) => matches!((value(a), value(b)), (Some(x), Some(y)) if x == y),
        Cond::OneOf(c, list) => matches!(value(c), Some(v) if list.contains(&v)),
        Cond::All(parts) => !parts.iter().any(|p| !model_holds(p, row, columns)),
        Cond::Any(parts) => parts.iter().any(|p| model_holds(p, row, columns)),
    }
}

/// Every pair of rows, left side first, that passes `keep`.
#[allow(dead_code)]
fn model_pairs(left: &[Row], right: &[Row], keep: impl Fn(&Row) -> bool) -> Vec<Row> {
    let mut out = Vec::new();
    for a in left {
        for b in right {
            let mut row = a.clone();
            row.extend(b.clone());
            if keep(&row) {
                out.push(row);
            }
        }
    }
    out
}

/// A leaf over a table no other part of the plan uses.
#[allow(dead_code)]
fn random_leaf(rng: &mut Rng, next_table: &mut usize) -> (Plan, Vec<Column>) {
    let t = *next_table;
    *next_table += 1;
    let node = if rng.bool() {
        Node::Scan(t)
    } else {
        Node::IndexScan { table: t, column: rng.below(2), low: rng.int(-1, 4), high: rng.int(-1, 4) }
    };
    (Plan::new(node, vec![]), vec![col(t, 0), col(t, 1)])
}

/// What a plan means, computed the plain way.
#[allow(dead_code)]
fn model(plan: &Plan, tables: &[(usize, Vec<Row>)]) -> (Vec<Column>, Vec<Row>) {
    match &plan.node {
        Node::Scan(t) => (vec![col(*t, 0), col(*t, 1)], tables[*t].1.clone()),
        Node::IndexScan { table, column, low, high } => {
            let mut rows: Vec<Row> = tables[*table].1.iter().filter(|r| r[*column].is_some_and(|v| v >= *low && v <= *high)).cloned().collect();
            rows.sort_by_key(|r| r[*column]);
            (vec![col(*table, 0), col(*table, 1)], rows)
        }
        Node::Filter(c) => {
            let (columns, mut rows) = model(&plan.inputs[0], tables);
            rows.retain(|r| model_holds(c, r, &columns));
            (columns, rows)
        }
        Node::Project(wanted) => {
            let (columns, rows) = model(&plan.inputs[0], tables);
            let rows = rows
                .iter()
                .map(|r| wanted.iter().map(|w| r[columns.iter().position(|c| c == w).unwrap()]).collect())
                .collect();
            (wanted.clone(), rows)
        }
        Node::NestedLoop(on) => {
            let (mut columns, left) = model(&plan.inputs[0], tables);
            let (more, right) = model(&plan.inputs[1], tables);
            columns.extend(more);
            let rows = model_pairs(&left, &right, |r| on.as_ref().is_none_or(|c| model_holds(c, r, &columns)));
            (columns, rows)
        }
    }
}

/// A random plan over at most four tables.
#[allow(dead_code)]
fn random_plan(rng: &mut Rng, next_table: &mut usize, depth: usize) -> Plan {
    let (mut plan, mut columns) = random_leaf(rng, next_table);
    for _ in 0..depth {
        match rng.below(4) {
            0 => {
                let c = random_cond(rng, &columns, 2);
                plan = Plan::new(Node::Filter(c), vec![plan]);
            }
            1 if *next_table < 4 => {
                let (other, more) = random_leaf(rng, next_table);
                columns.extend(more);
                let on = rng.bool().then(|| random_cond(rng, &columns, 2));
                let pair = if rng.bool() { vec![plan, other] } else {
                    let n = columns.len();
                    columns.rotate_left(n - 2);
                    vec![other, plan]
                };
                plan = Plan::new(Node::NestedLoop(on), pair);
            }
            2 => {
                let n = 1 + rng.below(3);
                let picks: Vec<Column> = (0..n).map(|_| *rng.pick(&columns)).collect();
                columns = picks.clone();
                columns.dedup();
                plan = Plan::new(Node::Project(picks), vec![plan]);
            }
            _ => {}
        }
    }
    plan
}

#[test]
fn random_plans_give_the_rows_of_the_plain_model() {
    let mut rng = Rng::new(9401);
    for case in 0..400 {
        let tables = random_tables(&mut rng);
        let store = Counting::new(tables.clone());
        let mut next_table = 0;
        let plan = random_plan(&mut rng, &mut next_table, 3);
        let (columns, want) = model(&plan, &tables);
        check!(format!("case {case}: schema of {plan:?}"), schema(&plan, &store), columns);
        check!(format!("case {case}: rows of {plan:?} over {tables:?}"), run(&plan, &store), want);
    }
}

#[test]
fn a_larger_join() {
    let store = Counting::new(vec![numbers(300), numbers(300)]);
    let plan = Plan::new(Node::NestedLoop(Some(Cond::Same(col(0, 0), col(1, 0)))), vec![scan(0), scan(1)]);
    check!("300 matches", run(&plan, &store).len(), 300);
    check!("reads", store.reads(), vec![300, 300]);
}
