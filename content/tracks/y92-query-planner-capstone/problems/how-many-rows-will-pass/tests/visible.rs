use solution::*;

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// One table of 100 rows: column 0 counts 0..100, column 1 is that number's last digit, and column 2 is
/// missing in every second row.
#[allow(dead_code)]
fn catalog() -> Catalog {
    let columns: Vec<Vec<Option<i64>>> = vec![
        (0..100).map(Some).collect(),
        (0..100).map(|i| Some(i % 10)).collect(),
        (0..100).map(|i| (i % 2 == 0).then_some(1)).collect(),
    ];
    Catalog {
        tables: vec![Table {
            name: "t".to_string(),
            column_names: vec!["id".to_string(), "digit".to_string(), "flag".to_string()],
            rows: 100,
            stats: columns.iter().map(|c| summarise(c)).collect(),
            indexed: vec![],
        }],
    }
}

#[allow(dead_code)]
fn col(column: usize) -> Column {
    Column { table: 0, column }
}

#[test]
fn single_comparisons() {
    let c = catalog();
    check!("digit = 3", close(selectivity(&Cond::Compare(col(1), Cmp::Eq, 3), &c), 0.1), true);
    check!("digit <> 3", close(selectivity(&Cond::Compare(col(1), Cmp::Ne, 3), &c), 0.9), true);
    check!("flag <> 1: missing values satisfy nothing", selectivity(&Cond::Compare(col(2), Cmp::Ne, 1), &c), 0.0);
}

#[test]
fn and_multiplies_or_adds_without_counting_twice() {
    let c = catalog();
    let a = Cond::Compare(col(1), Cmp::Eq, 3);
    let b = Cond::Compare(col(2), Cmp::Eq, 1);
    check!("and", close(selectivity(&Cond::All(vec![a.clone(), b.clone()]), &c), 0.05), true);
    check!("or", close(selectivity(&Cond::Any(vec![a, b]), &c), 1.0 - 0.9 * 0.5), true);
}

#[test]
fn two_bounds_on_one_column_are_one_range() {
    let c = catalog();
    let both = Cond::All(vec![Cond::Compare(col(0), Cmp::Ge, 20), Cond::Compare(col(0), Cmp::Lt, 30)]);
    let range = summarise(&(0..100).map(Some).collect::<Vec<_>>()).between(20, 29) / 100.0;
    check!("id >= 20 and id < 30", close(selectivity(&both, &c), range), true);
}

#[test]
fn ranges_lists_and_the_error_factor() {
    check!("x < 5", range_of(Cmp::Lt, 5), Some((i64::MIN, 4)));
    check!("x <> 5 is not one range", range_of(Cmp::Ne, 5), None);
    let c = catalog();
    let one_of = Cond::OneOf(col(1), vec![1, 2, 2, 1]);
    check!("digit in (1, 2, 2, 1)", close(selectivity(&one_of, &c), 0.2), true);
    check!("digit = id", close(selectivity(&Cond::Same(col(1), col(0)), &c), 0.01), true);
    check!("q_error(10, 40)", q_error(10.0, 40.0), 4.0);
    check!("q_error(40, 10)", q_error(40.0, 10.0), 4.0);
}

#[test]
fn empty_lists() {
    let c = catalog();
    check!("all of nothing", selectivity(&Cond::All(vec![]), &c), 1.0);
    check!("any of nothing", selectivity(&Cond::Any(vec![]), &c), 0.0);
}
