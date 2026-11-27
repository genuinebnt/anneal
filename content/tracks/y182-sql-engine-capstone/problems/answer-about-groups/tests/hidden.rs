use anneal_prelude::Rng;
use solution::Expr as E;
use solution::*;

fn int(n: i64) -> E {
    E::Lit(Value::Int(n))
}

fn maybe(n: Option<i64>) -> E {
    E::Lit(n.map_or(Value::Null, Value::Int))
}

fn col(name: &str) -> E {
    E::Column(name.to_string())
}

fn bin(op: Op, a: E, b: E) -> E {
    E::Binary(op, Box::new(a), Box::new(b))
}

fn key(name: &str) -> Item {
    Item::Expr { expr: col(name), name: name.to_string() }
}

fn agg(of: Aggregate, column: &str) -> Item {
    Item::Aggregate { of, column: (column != "*").then(|| column.to_string()) }
}

fn select(items: Vec<Item>, group_by: Option<&str>) -> Select {
    Select { items, table: "t".into(), filter: None, group_by: group_by.map(String::from), order_by: None, limit: None }
}

/// `t(g INT, v INT)` holding these rows.
fn table(values: &[(Option<i64>, Option<i64>)]) -> Database {
    let mut db = Database::new();
    let columns = vec![("g".to_string(), Type::Int), ("v".to_string(), Type::Int)];
    assert!(db.run(&Statement::CreateTable { table: "t".into(), columns }).is_ok());
    if !values.is_empty() {
        let rows = values.iter().map(|&(g, v)| vec![maybe(g), maybe(v)]).collect();
        assert!(db.run(&Statement::Insert { table: "t".into(), rows }).is_ok());
    }
    db
}

fn rows(db: &mut Database, query: &Select) -> Result<Vec<Vec<Value>>, Fault> {
    match db.run(&Statement::Select(query.clone()))? {
        Output::Rows { rows, .. } => Ok(rows),
        other => panic!("not rows: {other:?}"),
    }
}

fn values(row: &[Option<i64>]) -> Vec<Value> {
    row.iter().map(|n| n.map_or(Value::Null, Value::Int)).collect()
}

#[test]
fn nulls_form_one_group_that_comes_first() {
    let mut db = table(&[(Some(2), Some(1)), (None, Some(5)), (Some(-1), None), (None, None)]);
    let query = select(vec![key("g"), agg(Aggregate::Count, "*"), agg(Aggregate::Count, "v"), agg(Aggregate::Sum, "v"), agg(Aggregate::Min, "v")], Some("g"));
    let want = vec![
        values(&[None, Some(2), Some(1), Some(5), Some(5)]),
        values(&[Some(-1), Some(1), Some(0), None, None]),
        values(&[Some(2), Some(1), Some(1), Some(1), Some(1)]),
    ];
    check!("groups", rows(&mut db, &query), Ok(want));
}

#[test]
fn group_by_without_aggregates_and_aggregates_without_the_key() {
    let mut db = table(&[(Some(2), None), (Some(1), None), (Some(2), None)]);
    check!("distinct keys", rows(&mut db, &select(vec![key("g")], Some("g"))), Ok(vec![values(&[Some(1)]), values(&[Some(2)])]));
    check!("counts only", rows(&mut db, &select(vec![agg(Aggregate::Count, "*")], Some("g"))), Ok(vec![values(&[Some(1)]), values(&[Some(2)])]));
    check!("the key twice", rows(&mut db, &select(vec![key("g"), key("g")], Some("g"))).map(|r| r.len()), Ok(2));
}

#[test]
fn an_empty_table() {
    let mut db = table(&[]);
    let all = vec![agg(Aggregate::Count, "*"), agg(Aggregate::Count, "v"), agg(Aggregate::Sum, "v"), agg(Aggregate::Min, "v"), agg(Aggregate::Max, "g")];
    check!("one group", rows(&mut db, &select(all.clone(), None)), Ok(vec![values(&[Some(0), Some(0), None, None, None])]));
    check!("no groups", rows(&mut db, &select(all, Some("g"))), Ok(vec![]));
}

#[test]
fn the_order_of_the_checks() {
    let mut db = table(&[]);
    let mut query = select(vec![Item::Star, agg(Aggregate::Sum, "zz")], Some("gone"));
    query.order_by = Some(OrderBy { column: "ww".into(), descending: false });
    query.filter = Some(col("yy"));
    check!("WHERE first", rows(&mut db, &query), Err(Fault::UnknownColumn("yy".into())));
    query.filter = None;
    check!("then GROUP BY", rows(&mut db, &query), Err(Fault::UnknownColumn("gone".into())));
    query.group_by = Some("g".into());
    check!("then the items from the left", rows(&mut db, &query), Err(Fault::StarInGroups));
    query.items.remove(0);
    check!("the column of an aggregate", rows(&mut db, &query), Err(Fault::UnknownColumn("zz".into())));
    query.items = vec![agg(Aggregate::Count, "g")];
    check!("then ORDER BY", rows(&mut db, &query), Err(Fault::UnknownColumn("ww".into())));
}

#[test]
fn only_the_group_column_may_stand_alone() {
    let mut db = table(&[]);
    let twice = Item::Expr { expr: bin(Op::Mul, col("g"), int(2)), name: "g*2".into() };
    check!("a calculation on the key", rows(&mut db, &select(vec![twice], Some("g"))), Err(Fault::NotGrouped("g*2".into())));
    let constant = Item::Expr { expr: int(1), name: "1".into() };
    check!("a constant", rows(&mut db, &select(vec![agg(Aggregate::Count, "*"), constant], None)), Err(Fault::NotGrouped("1".into())));
    check!("a column without GROUP BY", rows(&mut db, &select(vec![agg(Aggregate::Count, "*"), key("g")], None)), Err(Fault::NotGrouped("g".into())));
    let ghost = key("ghost");
    check!("an unknown column is simply not the key", rows(&mut db, &select(vec![ghost], Some("g"))), Err(Fault::NotGrouped("ghost".into())));
    for of in [Aggregate::Min, Aggregate::Max, Aggregate::Sum] {
        check!(format!("{of:?}(*)"), rows(&mut db, &select(vec![agg(of, "*")], None)), Err(Fault::NeedsColumn(of)));
    }
}

#[test]
fn smallest_and_largest_of_any_type() {
    let mut db = Database::new();
    let columns = vec![("s".to_string(), Type::Text), ("b".to_string(), Type::Bool)];
    assert!(db.run(&Statement::CreateTable { table: "t".into(), columns }).is_ok());
    let row = |s: &str, b: bool| vec![E::Lit(Value::Text(s.into())), E::Lit(Value::Bool(b))];
    let rows_in = vec![row("pear", true), row("Zoo", false), row("apple", true)];
    assert!(db.run(&Statement::Insert { table: "t".into(), rows: rows_in }).is_ok());
    let query = select(vec![agg(Aggregate::Min, "s"), agg(Aggregate::Max, "s"), agg(Aggregate::Min, "b"), agg(Aggregate::Max, "b"), agg(Aggregate::Count, "b")], None);
    let want = vec![Value::Text("Zoo".into()), Value::Text("pear".into()), Value::Bool(false), Value::Bool(true), Value::Int(3)];
    check!("texts and booleans", rows(&mut db, &query), Ok(vec![want]));
    check!("sum of booleans", rows(&mut db, &select(vec![agg(Aggregate::Sum, "b")], None)), Err(Fault::WrongType { needed: Type::Int, found: Type::Bool }));
}

#[test]
fn a_sum_that_leaves_the_range() {
    let query = select(vec![agg(Aggregate::Sum, "v")], None);
    let mut db = table(&[(None, Some(i64::MAX)), (None, Some(1)), (None, Some(-5))]);
    check!("the running total counts, not the final one", rows(&mut db, &query), Err(Fault::Overflow));
    let mut db = table(&[(None, Some(i64::MAX)), (None, Some(-5)), (None, Some(1))]);
    check!("another order stays inside", rows(&mut db, &query), Ok(vec![values(&[Some(i64::MAX - 4)])]));
    let mut db = table(&[(None, Some(i64::MIN)), (None, Some(-1))]);
    check!("below", rows(&mut db, &query), Err(Fault::Overflow));
    // two groups: each has its own total
    let mut db = table(&[(Some(1), Some(i64::MAX)), (Some(2), Some(i64::MAX))]);
    check!("apart they fit", rows(&mut db, &select(vec![agg(Aggregate::Sum, "v")], Some("g"))).map(|r| r.len()), Ok(2));
}

#[test]
fn what_runs() {
    let mut db = table(&[(Some(1), Some(1)), (Some(2), Some(i64::MAX)), (Some(2), Some(1))]);
    let mut query = select(vec![key("g"), agg(Aggregate::Sum, "v")], Some("g"));
    query.limit = Some(1);
    check!("every row is read, even for groups the limit drops", rows(&mut db, &query), Err(Fault::Overflow));
    query.limit = Some(0);
    check!("LIMIT 0 runs nothing", rows(&mut db, &query), Ok(vec![]));
    // the condition fails on the second row, the sum on the third: rows are handled one at a time
    let mut db = table(&[(Some(1), Some(i64::MAX)), (Some(1), Some(1)), (Some(0), Some(0))]);
    let mut query = select(vec![agg(Aggregate::Sum, "v")], None);
    query.filter = Some(bin(Op::Eq, bin(Op::Div, int(1), col("g")), int(1)));
    check!("row 2 overflows before row 3 divides by zero", rows(&mut db, &query), Err(Fault::Overflow));
}

#[test]
fn ties_after_grouping_stay_in_key_order() {
    let data: Vec<(Option<i64>, Option<i64>)> = (0..40).map(|i| (Some((i * 7) % 40), Some(i % 2))).collect();
    let mut db = table(&data);
    for descending in [false, true] {
        let mut query = select(vec![key("g"), agg(Aggregate::Sum, "v")], Some("g"));
        query.order_by = Some(OrderBy { column: "sum(v)".into(), descending });
        let mut want: Vec<(i64, i64)> = data.iter().map(|&(g, v)| (g.unwrap(), v.unwrap())).collect();
        want.sort_by_key(|&(g, v)| (if descending { -v } else { v }, g));
        let want: Vec<Vec<Value>> = want.iter().map(|&(g, v)| values(&[Some(g), Some(v)])).collect();
        check!(format!("descending = {descending}"), rows(&mut db, &query), Ok(want));
    }
}

#[test]
fn two_columns_with_one_title() {
    let mut db = table(&[(Some(1), Some(9)), (Some(2), Some(3))]);
    let titled = |name: &str| Item::Expr { expr: col("g"), name: name.into() };
    let mut query = select(vec![titled("min(v)"), agg(Aggregate::Min, "v")], Some("g"));
    query.order_by = Some(OrderBy { column: "min(v)".into(), descending: false });
    check!("the first column with the title sorts", rows(&mut db, &query), Ok(vec![values(&[Some(1), Some(9)]), values(&[Some(2), Some(3)])]));
}

#[test]
fn random_groups() {
    let mut rng = Rng::new(18231);
    for _ in 0..500 {
        let data: Vec<(Option<i64>, Option<i64>)> = (0..rng.below(14)).map(|_| ((rng.below(5) != 0).then(|| rng.int(-2, 2)), (rng.below(4) != 0).then(|| rng.int(-9, 9)))).collect();
        let mut db = table(&data);
        let grouped = rng.bool();
        let bound = rng.bool().then(|| rng.int(-5, 5));
        let aggregates = [agg(Aggregate::Count, "*"), agg(Aggregate::Count, "v"), agg(Aggregate::Sum, "v"), agg(Aggregate::Min, "v"), agg(Aggregate::Max, "v")];
        let mut items = aggregates.to_vec();
        if grouped {
            items.insert(0, key("g"));
        }
        let mut query = select(items, grouped.then_some("g"));
        query.filter = bound.map(|b| bin(Op::Gt, col("v"), int(b)));
        let order = rng.bool().then(|| (rng.below(5), rng.bool()));
        let titles = ["count(*)", "count(v)", "sum(v)", "min(v)", "max(v)"];
        query.order_by = order.map(|(i, descending)| OrderBy { column: titles[i].into(), descending });
        query.limit = rng.bool().then(|| rng.below(5) as u64);

        let passed: Vec<_> = data.iter().filter(|r| bound.is_none_or(|b| r.1.is_some_and(|v| v > b))).collect();
        let mut keys: Vec<Option<i64>> = if grouped { passed.iter().map(|r| r.0).collect() } else { vec![None] };
        keys.sort();
        keys.dedup();
        let mut want: Vec<Vec<Option<i64>>> = keys
            .into_iter()
            .map(|k| {
                let members: Vec<_> = passed.iter().filter(|r| !grouped || r.0 == k).collect();
                let present: Vec<i64> = members.iter().filter_map(|r| r.1).collect();
                let mut row = vec![Some(members.len() as i64), Some(present.len() as i64), (!present.is_empty()).then(|| present.iter().sum()), present.iter().min().copied(), present.iter().max().copied()];
                if grouped {
                    row.insert(0, k);
                }
                row
            })
            .collect();
        if let Some((i, descending)) = order {
            let at = i + usize::from(grouped);
            want.sort_by(|a, b| if descending { b[at].cmp(&a[at]) } else { a[at].cmp(&b[at]) });
        }
        if let Some(n) = query.limit {
            want.truncate(n as usize);
        }
        let want: Vec<Vec<Value>> = want.iter().map(|r| values(r)).collect();
        check!(format!("{query:?} over {data:?}"), rows(&mut db, &query), Ok(want));
    }
}

#[test]
fn many_groups() {
    let data: Vec<(Option<i64>, Option<i64>)> = (0..100_000).map(|i| (Some(i % 20_000), Some(1))).collect();
    let mut db = table(&data);
    let got = rows(&mut db, &select(vec![key("g"), agg(Aggregate::Sum, "v")], Some("g"))).map(|r| (r.len(), r[19_999].clone()));
    check!("20 000 groups of five", got, Ok((20_000, values(&[Some(19_999), Some(5)]))));
}

#[test]
fn plain_queries_still_work() {
    let mut db = table(&[(Some(1), Some(2))]);
    check!("step 4", rows(&mut db, &select(vec![Item::Star], None)), Ok(vec![values(&[Some(1), Some(2)])]));
}
