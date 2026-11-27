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
fn keyed(keys: &[Option<i64>]) -> (usize, Vec<Row>) {
    (2, keys.iter().enumerate().map(|(i, k)| vec![*k, Some(i as i64)]).collect())
}

#[allow(dead_code)]
fn join(merge: bool, a: usize, b: usize) -> Plan {
    let (left_key, right_key, rest) = (col(a, 0), col(b, 0), None);
    let node = if merge { Node::MergeJoin { left_key, right_key, rest } } else { Node::HashJoin { left_key, right_key, rest } };
    Plan::new(node, vec![scan(a), scan(b)])
}

#[test]
fn missing_keys_never_match_each_other() {
    let store = Counting::new(vec![keyed(&[None, None]), keyed(&[None, None, None])]);
    for merge in [false, true] {
        check!(format!("all keys missing, merge = {merge}"), run(&join(merge, 0, 1), &store).len(), 0);
    }
}

#[test]
fn empty_sides() {
    let store = Counting::new(vec![keyed(&[]), keyed(&[Some(1)])]);
    for merge in [false, true] {
        for (a, b) in [(0, 1), (1, 0)] {
            check!(format!("table {a} with {b}, merge = {merge}"), run(&join(merge, a, b), &store).len(), 0);
        }
    }
}

#[test]
fn extreme_keys() {
    let keys = [Some(i64::MIN), Some(0), Some(i64::MAX), Some(i64::MAX)];
    let store = Counting::new(vec![keyed(&keys), keyed(&keys)]);
    for merge in [false, true] {
        check!(format!("MIN and MAX as keys, merge = {merge}"), run(&join(merge, 0, 1), &store).len(), 6);
    }
}

#[test]
fn a_long_run_of_one_key_on_both_sides() {
    let store = Counting::new(vec![keyed(&[Some(4); 100]), keyed(&[Some(4); 80])]);
    for merge in [false, true] {
        let rows = run(&join(merge, 0, 1), &store);
        let order: Vec<i64> = rows.iter().map(|r| r[1].unwrap() * 80 + r[3].unwrap()).collect();
        check!(format!("100 x 80 rows in left-then-right order, merge = {merge}"), order, (0..8000).collect::<Vec<i64>>());
    }
}

#[test]
fn a_merge_join_reads_each_side_only_as_far_as_it_has_to() {
    let mut rng = Rng::new(9501);
    for case in 0..200 {
        let make = |rng: &mut Rng| {
            let n = rng.below(40);
            let mut keys: Vec<Option<i64>> = (0..n).map(|_| (rng.below(8) > 0).then(|| rng.int(0, 15))).collect();
            keys.sort();
            keys
        };
        let (a, b) = (make(&mut rng), make(&mut rng));
        let store = Counting::new(vec![keyed(&a), keyed(&b)]);
        let plan = join(true, 0, 1);
        let mut running = open(&plan, &store);
        check!("open reads nothing", store.reads(), vec![0, 0]);
        while let Some(row) = running.next() {
            let (i, key) = (row[1].unwrap() as usize, row[0]);
            let reads = store.reads();
            let right_limit = b.iter().filter(|&&k| k <= key).count() + 1;
            let input = format!("case {case}: keys {a:?} and {b:?}, at a row with left row {i}");
            check!(format!("{input}: first-input reads, at most {}", i + 1), reads[0] <= i + 1, true);
            check!(format!("{input}: second-input reads {} at most {right_limit}", reads[1]), reads[1] <= right_limit, true);
        }
        let reads = store.reads();
        check!("no row is read twice", (reads[0] <= a.len(), reads[1] <= b.len()), (true, true));
    }
}

#[test]
fn a_hash_join_reads_the_second_input_once_and_the_first_as_needed() {
    let a: Vec<Option<i64>> = (0..500).map(|i| Some(i % 7)).collect();
    let b = [Some(3), Some(5), Some(3), None];
    let store = Counting::new(vec![keyed(&a), keyed(&b)]);
    let plan = join(false, 0, 1);
    let mut running = open(&plan, &store);
    check!("open reads nothing", store.reads(), vec![0, 0]);
    while let Some(row) = running.next() {
        let i = row[1].unwrap() as usize;
        check!(format!("reads at a row with left row {i}"), store.reads(), vec![i + 1, 4]);
    }
    check!("totals", store.reads(), vec![500, 4]);
    check!("after the end", (running.next(), running.next()), (None, None));
    check!("no more reads", store.reads(), vec![500, 4]);
}

#[test]
fn a_sort_reads_nothing_until_asked_then_everything_once() {
    let store = Counting::new(vec![keyed(&(0..300).rev().map(Some).collect::<Vec<_>>())]);
    let plan = Plan::new(Node::Sort(col(0, 0)), vec![scan(0)]);
    let mut running = open(&plan, &store);
    check!("open reads nothing", store.reads(), vec![0]);
    check!("first", running.next(), Some(vec![Some(0), Some(299)]));
    check!("all read", store.reads(), vec![300]);
    while running.next().is_some() {}
    check!("after the end", (running.next(), store.reads()), (None, vec![300]));
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

/// What a plan means, computed the plain way: every join is a pair-everything loop.
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
        Node::Sort(by) => {
            let (columns, mut rows) = model(&plan.inputs[0], tables);
            let at = columns.iter().position(|c| c == by).unwrap();
            rows.sort_by_key(|r| r[at]);
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
        Node::HashJoin { left_key, right_key, rest } | Node::MergeJoin { left_key, right_key, rest } => {
            let (mut columns, left) = model(&plan.inputs[0], tables);
            let (more, right) = model(&plan.inputs[1], tables);
            columns.extend(more);
            let equal = Cond::Same(*left_key, *right_key);
            let rows = model_pairs(&left, &right, |r| {
                model_holds(&equal, r, &columns) && rest.as_ref().is_none_or(|c| model_holds(c, r, &columns))
            });
            (columns, rows)
        }
    }
}

/// A random tree of joins over some of the four tables, with filters and sorts mixed in. Merge joins get the
/// sorts they need.
#[allow(dead_code)]
fn random_plan(rng: &mut Rng, next_table: &mut usize) -> (Plan, Vec<Column>) {
    let (mut plan, mut columns) = random_leaf(rng, next_table);
    while *next_table < 4 && rng.below(4) > 0 {
        let (mut other, mut more) = random_leaf(rng, next_table);
        if rng.bool() {
            std::mem::swap(&mut plan, &mut other);
            std::mem::swap(&mut columns, &mut more);
        }
        let left_key = *rng.pick(&columns);
        let right_key = *rng.pick(&more);
        columns.extend(more);
        let rest = rng.bool().then(|| random_cond(rng, &columns, 2));
        plan = match rng.below(3) {
            0 => Plan::new(Node::HashJoin { left_key, right_key, rest }, vec![plan, other]),
            1 => {
                let a = Plan::new(Node::Sort(left_key), vec![plan]);
                let b = Plan::new(Node::Sort(right_key), vec![other]);
                Plan::new(Node::MergeJoin { left_key, right_key, rest }, vec![a, b])
            }
            _ => Plan::new(Node::NestedLoop(rest), vec![plan, other]),
        };
        if rng.below(4) == 0 {
            let c = random_cond(rng, &columns, 1);
            plan = Plan::new(Node::Filter(c), vec![plan]);
        }
    }
    (plan, columns)
}

#[test]
fn random_plans_give_the_rows_of_the_plain_model_in_the_same_order() {
    let mut rng = Rng::new(9502);
    for case in 0..800 {
        let tables = random_tables(&mut rng);
        let store = Counting::new(tables.clone());
        let mut next_table = 0;
        let (plan, _) = random_plan(&mut rng, &mut next_table);
        let (columns, want) = model(&plan, &tables);
        check!(format!("case {case}: schema of {plan:?}"), schema(&plan, &store), columns);
        check!(format!("case {case}: rows of {plan:?} over {tables:?}"), run(&plan, &store), want);
    }
}

#[test]
fn a_larger_join_of_each_kind() {
    let keys: Vec<Option<i64>> = (0..3000).map(Some).collect();
    let store = Counting::new(vec![keyed(&keys), keyed(&keys)]);
    for merge in [false, true] {
        check!(format!("3000 matches, merge = {merge}"), run(&join(merge, 0, 1), &store).len(), 3000);
    }
}
