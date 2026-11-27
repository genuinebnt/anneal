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
fn col(table: usize, column: usize) -> Column {
    Column { table, column }
}

#[allow(dead_code)]
fn data() -> Vec<(usize, Vec<Row>)> {
    vec![
        (2, (0..1000).map(|i| vec![Some(i), Some(i % 100)]).collect()),
        (2, (0..100).map(|i| vec![Some(i), Some(i % 5)]).collect()),
    ]
}

#[test]
fn the_plan_gives_the_rows_of_the_query() {
    let catalog = catalog_for(&data(), &[vec![0], vec![0]]);
    let query = Query {
        tables: vec![0, 1],
        condition: Cond::All(vec![Cond::Same(col(0, 1), col(1, 0)), Cond::Compare(col(0, 0), Cmp::Lt, 3)]),
        output: vec![col(1, 0), col(0, 0)],
    };
    let plan = optimize(&query, &catalog).expect("a valid query");
    check!("the top is the projection", plan.node.clone(), Node::Project(query.output.clone()));
    let mut rows = run(&plan, &Counting::new(data()));
    rows.sort();
    check!("rows", rows, (0..3).map(|i| vec![Some(i), Some(i)]).collect::<Vec<_>>());
}

#[test]
fn explaining_a_plan() {
    let catalog = catalog_for(&data(), &[vec![0], vec![0]]);
    let mut scan = Plan::new(Node::Scan(0), vec![]);
    (scan.rows, scan.cost) = (1000.0, 20.0);
    let mut index = Plan::new(Node::IndexScan { table: 1, column: 0, low: i64::MIN, high: 4 }, vec![]);
    (index.rows, index.cost) = (5.0, 32.05);
    let join = Node::HashJoin { left_key: col(0, 1), right_key: col(1, 0), rest: Some(Cond::Compare(col(1, 1), Cmp::Ne, 2)) };
    let mut top = Plan::new(join, vec![scan, index]);
    (top.rows, top.cost) = (49.6, 62.51);
    let want = "HashJoin t0.c1 = t1.c0 where t1.c1 <> 2 (rows=50 cost=62.5)\n  Scan t0 (rows=1000 cost=20.0)\n  IndexScan t1.c0 [-inf..4] (rows=5 cost=32.0)\n";
    check!("a hash join over two readers", explain(&top, &catalog), want.to_string());
}

#[test]
fn a_query_that_cannot_be_planned_says_why() {
    let catalog = catalog_for(&data(), &[vec![], vec![]]);
    let query = |tables: Vec<usize>| Query { tables, condition: Cond::All(vec![]), output: vec![] };
    check!("no tables", optimize(&query(vec![]), &catalog).err(), Some(PlanError::NoTables));
    check!("unknown table", optimize(&query(vec![0, 2]), &catalog).err(), Some(PlanError::UnknownTable(2)));
    check!("twice", optimize(&query(vec![1, 1]), &catalog).err(), Some(PlanError::RepeatedTable(1)));
}

#[test]
fn one_table_without_conditions() {
    let catalog = catalog_for(&data(), &[vec![], vec![]]);
    let query = Query { tables: vec![1], condition: Cond::All(vec![]), output: vec![col(1, 1)] };
    let plan = optimize(&query, &catalog).expect("a valid query");
    check!("its plan", explain(&plan, &catalog), "Project t1.c1 (rows=100 cost=2.2)\n  Scan t1 (rows=100 cost=2.0)\n".to_string());
}

#[test]
fn an_unknown_column() {
    let catalog = catalog_for(&data(), &[vec![], vec![]]);
    let query = Query { tables: vec![0], condition: Cond::All(vec![]), output: vec![col(0, 5)] };
    check!("column 5", optimize(&query, &catalog).err(), Some(PlanError::UnknownColumn(col(0, 5))));
}
