use solution::*;
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

/// Two tables of (key, tag), each already sorted by key with the missing key first.
#[allow(dead_code)]
fn store() -> Counting {
    let rows = |list: &[(Option<i64>, i64)]| list.iter().map(|&(k, t)| vec![k, Some(t)]).collect::<Vec<Row>>();
    Counting::new(vec![
        (2, rows(&[(None, 1), (Some(2), 2), (Some(5), 3), (Some(5), 4), (Some(7), 5)])),
        (2, rows(&[(None, 10), (Some(1), 11), (Some(5), 12), (Some(5), 13), (Some(7), 14), (Some(9), 15)])),
    ])
}

#[allow(dead_code)]
fn tags(rows: Vec<Row>) -> Vec<(i64, i64)> {
    rows.iter().map(|r| (r[1].unwrap(), r[3].unwrap())).collect()
}

#[allow(dead_code)]
fn hash(rest: Option<Cond>) -> Plan {
    Plan::new(Node::HashJoin { left_key: col(0, 0), right_key: col(1, 0), rest }, vec![scan(0), scan(1)])
}

#[allow(dead_code)]
fn merge(rest: Option<Cond>) -> Plan {
    Plan::new(Node::MergeJoin { left_key: col(0, 0), right_key: col(1, 0), rest }, vec![scan(0), scan(1)])
}

#[test]
fn both_joins_pair_every_match_in_left_order() {
    let want = vec![(3, 12), (3, 13), (4, 12), (4, 13), (5, 14)];
    check!("hash join", tags(run(&hash(None), &store())), want.clone());
    check!("merge join", tags(run(&merge(None), &store())), want);
}

#[test]
fn the_extra_condition_filters_pairs() {
    let rest = Cond::Compare(col(1, 1), Cmp::Ne, 12);
    let want = vec![(3, 13), (4, 13), (5, 14)];
    check!("hash join", tags(run(&hash(Some(rest.clone())), &store())), want.clone());
    check!("merge join", tags(run(&merge(Some(rest)), &store())), want);
}

#[test]
fn a_sort_is_stable_with_missing_values_first() {
    let rows: Vec<Row> = [(Some(3), 0), (None, 1), (Some(1), 2), (Some(3), 3), (None, 4)].iter().map(|&(k, t)| vec![k, Some(t)]).collect();
    let store = Counting::new(vec![(2, rows)]);
    let sorted = run(&Plan::new(Node::Sort(col(0, 0)), vec![scan(0)]), &store);
    check!("order of the tags", sorted.iter().map(|r| r[1].unwrap()).collect::<Vec<_>>(), vec![1, 4, 2, 0, 3]);
}

#[test]
fn a_hash_join_streams_its_first_input() {
    let store = store();
    let plan = hash(None);
    let mut running = open(&plan, &store);
    check!("open reads nothing", store.reads(), vec![0, 0]);
    running.next();
    check!("the second input fully, the first up to the first match", store.reads(), vec![3, 6]);
}

#[test]
fn the_schema_of_a_join() {
    check!("first input then second", schema(&hash(None), &store()), vec![col(0, 0), col(0, 1), col(1, 0), col(1, 1)]);
    check!("a sort keeps its input's columns", schema(&Plan::new(Node::Sort(col(1, 0)), vec![scan(1)]), &store()), vec![col(1, 0), col(1, 1)]);
}
