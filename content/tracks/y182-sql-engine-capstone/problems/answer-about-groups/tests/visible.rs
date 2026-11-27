use solution::Expr as E;
use solution::*;

#[allow(dead_code)]
fn text(s: &str) -> E {
    E::Lit(Value::Text(s.to_string()))
}

#[allow(dead_code)]
fn int(n: i64) -> E {
    E::Lit(Value::Int(n))
}

#[allow(dead_code)]
fn key(name: &str) -> Item {
    Item::Expr { expr: E::Column(name.to_string()), name: name.to_string() }
}

#[allow(dead_code)]
fn agg(of: Aggregate, column: &str) -> Item {
    Item::Aggregate { of, column: (column != "*").then(|| column.to_string()) }
}

/// `emp(dept TEXT, pay INT)`.
#[allow(dead_code)]
fn staff() -> Database {
    let mut db = Database::new();
    let columns = vec![("dept".to_string(), Type::Text), ("pay".to_string(), Type::Int)];
    check!("create", db.run(&Statement::CreateTable { table: "emp".into(), columns }), Ok(Output::Created));
    let rows = [("ops", Some(70)), ("eng", Some(100)), ("ops", None), ("eng", Some(80)), ("hr", Some(60))];
    let rows = rows.map(|(d, p)| vec![text(d), p.map_or(E::Lit(Value::Null), int)]).to_vec();
    check!("insert", db.run(&Statement::Insert { table: "emp".into(), rows }), Ok(Output::Inserted(5)));
    db
}

#[allow(dead_code)]
fn select(items: Vec<Item>, group_by: Option<&str>) -> Select {
    Select { items, table: "emp".into(), filter: None, group_by: group_by.map(String::from), order_by: None, limit: None }
}

/// The answer as one line per row, cells joined by `|`.
#[allow(dead_code)]
fn lines(db: &mut Database, query: Select) -> Result<Vec<String>, Fault> {
    let cell = |v: &Value| match v {
        Value::Null => "NULL".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Text(s) => s.clone(),
    };
    match db.run(&Statement::Select(query))? {
        Output::Rows { rows, .. } => Ok(rows.iter().map(|r| r.iter().map(cell).collect::<Vec<_>>().join("|")).collect()),
        other => panic!("not rows: {other:?}"),
    }
}

#[test]
fn one_row_per_group_in_key_order() {
    let query = select(vec![key("dept"), agg(Aggregate::Count, "*"), agg(Aggregate::Count, "pay"), agg(Aggregate::Sum, "pay"), agg(Aggregate::Max, "pay")], Some("dept"));
    let want = ["eng|2|2|180|100", "hr|1|1|60|60", "ops|2|1|70|70"].map(String::from).to_vec();
    check!("by department", lines(&mut staff(), query), Ok(want));
}

#[test]
fn titles() {
    let query = select(vec![agg(Aggregate::Count, "*"), agg(Aggregate::Min, "dept")], None);
    let want = Output::Rows { columns: vec!["count(*)".into(), "min(dept)".into()], rows: vec![vec![Value::Int(5), Value::Text("eng".into())]] };
    check!("count(*), min(dept)", staff().run(&Statement::Select(query)), Ok(want));
}

#[test]
fn nothing_to_add_up() {
    let mut query = select(vec![agg(Aggregate::Count, "*"), agg(Aggregate::Sum, "pay")], None);
    query.filter = Some(E::Lit(Value::Bool(false)));
    check!("one row without GROUP BY", lines(&mut staff(), query.clone()), Ok(vec!["0|NULL".to_string()]));
    query.group_by = Some("dept".into());
    check!("none with it", lines(&mut staff(), query), Ok(vec![]));
}

#[test]
fn order_by_an_output_title() {
    let mut query = select(vec![key("dept"), agg(Aggregate::Sum, "pay")], Some("dept"));
    query.order_by = Some(OrderBy { column: "sum(pay)".into(), descending: false });
    query.limit = Some(2);
    check!("the two smallest sums", lines(&mut staff(), query.clone()), Ok(["hr|60", "ops|70"].map(String::from).to_vec()));
    query.order_by = Some(OrderBy { column: "pay".into(), descending: false });
    check!("`pay` is not in the output", lines(&mut staff(), query), Err(Fault::UnknownColumn("pay".into())));
}

#[test]
fn what_may_be_selected() {
    let mut db = staff();
    check!("pay is not grouped", lines(&mut db, select(vec![key("pay"), agg(Aggregate::Count, "*")], Some("dept"))), Err(Fault::NotGrouped("pay".into())));
    check!("a star", lines(&mut db, select(vec![Item::Star], Some("dept"))), Err(Fault::StarInGroups));
    check!("sum(*)", lines(&mut db, select(vec![agg(Aggregate::Sum, "*")], None)), Err(Fault::NeedsColumn(Aggregate::Sum)));
    check!("sum of texts", lines(&mut db, select(vec![agg(Aggregate::Sum, "dept")], None)), Err(Fault::WrongType { needed: Type::Int, found: Type::Text }));
}
