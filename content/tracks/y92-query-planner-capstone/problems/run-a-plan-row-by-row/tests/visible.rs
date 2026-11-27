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

/// Table 0 is people (id, city); table 1 is cities (id).
#[allow(dead_code)]
fn people() -> Counting {
    Counting::new(vec![
        (2, vec![vec![Some(1), Some(20)], vec![Some(2), Some(10)], vec![Some(3), None], vec![Some(4), Some(10)]]),
        (1, vec![vec![Some(10)], vec![Some(20)]]),
    ])
}

#[allow(dead_code)]
fn col(table: usize, column: usize) -> Column {
    Column { table, column }
}

#[allow(dead_code)]
fn scan(t: usize) -> Plan {
    Plan::new(Node::Scan(t), vec![])
}

#[test]
fn filter_then_project() {
    let store = people();
    let in_ten = Plan::new(Node::Filter(Cond::Compare(col(0, 1), Cmp::Eq, 10)), vec![scan(0)]);
    let ids = Plan::new(Node::Project(vec![col(0, 0)]), vec![in_ten]);
    check!("ids of the people in city 10", run(&ids, &store), vec![vec![Some(2)], vec![Some(4)]]);
    check!("schema", schema(&ids, &store), vec![col(0, 0)]);
}

#[test]
fn a_nested_loop_keeps_the_outer_order() {
    let store = people();
    let on = Cond::Same(col(0, 1), col(1, 0));
    let joined = Plan::new(Node::NestedLoop(Some(on)), vec![scan(0), scan(1)]);
    let want = vec![
        vec![Some(1), Some(20), Some(20)],
        vec![Some(2), Some(10), Some(10)],
        vec![Some(4), Some(10), Some(10)],
    ];
    check!("people with their city", run(&joined, &store), want);
    check!("each city row was read once", store.reads()[1], 2);
}

#[test]
fn rows_are_read_only_when_asked_for() {
    let store = people();
    let plan = Plan::new(Node::Filter(Cond::Compare(col(0, 1), Cmp::Eq, 10)), vec![scan(0)]);
    let mut running = open(&plan, &store);
    check!("open reads nothing", store.reads(), vec![0, 0]);
    check!("the first row", running.next(), Some(vec![Some(2), Some(10)]));
    check!("two rows were read to find it", store.reads(), vec![2, 0]);
}

#[test]
fn an_index_scan_reads_only_what_it_returns() {
    let store = people();
    let plan = Plan::new(Node::IndexScan { table: 0, column: 1, low: 10, high: 15 }, vec![]);
    check!("cities 10 to 15, in index order", run(&plan, &store), vec![vec![Some(2), Some(10)], vec![Some(4), Some(10)]]);
    check!("two rows read", store.reads()[0], 2);
}

#[test]
fn without_a_condition_everything_is_paired() {
    let store = people();
    let all = Plan::new(Node::NestedLoop(None), vec![scan(1), scan(1)]);
    check!("2 x 2 rows, outer first", run(&all, &store), vec![
        vec![Some(10), Some(10)],
        vec![Some(10), Some(20)],
        vec![Some(20), Some(10)],
        vec![Some(20), Some(20)],
    ]);
}
