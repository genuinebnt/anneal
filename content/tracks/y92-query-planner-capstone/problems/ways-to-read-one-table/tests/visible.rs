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
fn col(column: usize) -> Column {
    Column { table: 0, column }
}

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// 1000 rows: column 0 counts, column 1 is the last digit. Both are indexed.
#[allow(dead_code)]
fn data() -> Vec<(usize, Vec<Row>)> {
    vec![(2, (0..1000).map(|i| vec![Some(i), Some(i % 10)]).collect())]
}

#[test]
fn with_no_condition_there_is_one_way() {
    let catalog = catalog_for(&data(), &[vec![0, 1]]);
    let plans = leaf_plans(&catalog, 0, &[]);
    check!("number of plans", plans.len(), 1);
    check!("a bare scan", plans[0].node.clone(), Node::Scan(0));
    // 10 pages of 100 rows, plus 0.01 for each row
    check!("rows and cost", (plans[0].rows, close(plans[0].cost, 20.0)), (1000.0, true));
}

#[test]
fn an_indexed_column_with_a_range_adds_a_second_way() {
    let catalog = catalog_for(&data(), &[vec![0, 1]]);
    let conditions = [Cond::Compare(col(0), Cmp::Lt, 5)];
    let plans = leaf_plans(&catalog, 0, &conditions);
    check!("number of plans", plans.len(), 2);
    check!("first the scan, with a filter", plans[0].node.clone(), Node::Filter(conditions[0].clone()));
    check!("scan cost: 20 plus 1000 checks", close(plans[0].cost, 22.5), true);
    check!("then the index", matches!(plans[1].node, Node::IndexScan { table: 0, column: 0, .. }), true);
    check!("sorted by the indexed column", plans[1].order, Some(col(0)));
    let matched = catalog.tables[0].stats[0].between(i64::MIN, 4);
    check!("index cost", close(plans[1].cost, (3.0 + matched) * 4.0 + matched * 0.01), true);
    let store = Counting::new(data());
    check!("it reads exactly the five rows", (run(&plans[1], &store).len(), store.reads()[0]), (5, 5));
}

#[test]
fn other_conditions_stay_in_a_filter() {
    let catalog = catalog_for(&data(), &[vec![1]]);
    let low = Cond::Compare(col(0), Cmp::Lt, 500);
    let digit = Cond::Compare(col(1), Cmp::Eq, 3);
    let plans = leaf_plans(&catalog, 0, &[low.clone(), digit]);
    check!("two ways", plans.len(), 2);
    check!("the filter over the index keeps only the other condition", plans[1].node.clone(), Node::Filter(low));
    check!("over an index scan of the digit", plans[1].inputs[0].node.clone(), Node::IndexScan { table: 0, column: 1, low: 3, high: 3 });
}

#[test]
fn every_way_gives_the_same_rows() {
    let catalog = catalog_for(&data(), &[vec![0, 1]]);
    let conditions = [Cond::Compare(col(0), Cmp::Ge, 990), Cond::Compare(col(1), Cmp::Ne, 5)];
    let store = Counting::new(data());
    let counts: Vec<usize> = leaf_plans(&catalog, 0, &conditions).iter().map(|p| run(p, &store).len()).collect();
    check!("nine rows each way", counts, vec![9, 9]);
}

#[test]
fn not_equal_cannot_use_an_index() {
    let catalog = catalog_for(&data(), &[vec![0, 1]]);
    check!("only the scan", leaf_plans(&catalog, 0, &[Cond::Compare(col(1), Cmp::Ne, 3)]).len(), 1);
}
