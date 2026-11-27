use solution::Expr as E;
use solution::*;

#[allow(dead_code)]
fn col(name: &str) -> E {
    E::Column(name.to_string())
}

#[allow(dead_code)]
fn int(n: i64) -> E {
    E::Lit(Value::Int(n))
}

#[allow(dead_code)]
fn text(s: &str) -> E {
    E::Lit(Value::Text(s.to_string()))
}

#[allow(dead_code)]
fn bin(op: Op, a: E, b: E) -> E {
    E::Binary(op, Box::new(a), Box::new(b))
}

#[allow(dead_code)]
fn item(name: &str) -> Item {
    Item::Expr { expr: col(name), name: name.to_string() }
}

#[allow(dead_code)]
fn select(items: Vec<Item>) -> Select {
    Select { items, table: "emp".into(), filter: None, group_by: None, order_by: None, limit: None }
}

#[allow(dead_code)]
fn staff() -> Database {
    let mut db = Database::new();
    let columns = vec![("name".to_string(), Type::Text), ("pay".to_string(), Type::Int)];
    check!("create", db.run(&Statement::CreateTable { table: "emp".into(), columns }), Ok(Output::Created));
    let rows = vec![vec![text("ann"), int(70)], vec![text("bob"), E::Lit(Value::Null)], vec![text("cy"), int(50)], vec![text("di"), int(70)]];
    check!("insert", db.run(&Statement::Insert { table: "emp".into(), rows }), Ok(Output::Inserted(4)));
    db
}

/// The first column of the answer, as text.
#[allow(dead_code)]
fn names(db: &mut Database, query: Select) -> Result<Vec<String>, Fault> {
    match db.run(&Statement::Select(query))? {
        Output::Rows { rows, .. } => Ok(rows
            .into_iter()
            .map(|r| match &r[0] {
                Value::Text(s) => s.clone(),
                other => format!("{other:?}"),
            })
            .collect()),
        other => panic!("not rows: {other:?}"),
    }
}

#[test]
fn star_and_column_names() {
    let mut db = staff();
    let mut query = select(vec![Item::Star, Item::Expr { expr: bin(Op::Mul, col("pay"), int(2)), name: "pay * 2".into() }]);
    query.limit = Some(1);
    let want = Output::Rows {
        columns: vec!["name".into(), "pay".into(), "pay * 2".into()],
        rows: vec![vec![Value::Text("ann".into()), Value::Int(70), Value::Int(140)]],
    };
    check!("SELECT *, pay * 2 FROM emp LIMIT 1", db.run(&Statement::Select(query)), Ok(want));
}

#[test]
fn only_true_keeps_a_row() {
    let mut db = staff();
    let mut query = select(vec![item("name")]);
    query.filter = Some(bin(Op::Gt, col("pay"), int(60)));
    check!("pay > 60: bob's pay is unknown", names(&mut db, query), Ok(vec!["ann".into(), "di".into()]));
}

#[test]
fn order_by_a_column_that_is_not_selected() {
    let mut db = staff();
    let mut query = select(vec![item("name")]);
    query.order_by = Some(OrderBy { column: "pay".into(), descending: true });
    check!("highest first, NULL last, ties as inserted", names(&mut db, query.clone()), Ok(["ann", "di", "cy", "bob"].map(String::from).to_vec()));
    query.order_by = Some(OrderBy { column: "pay".into(), descending: false });
    check!("lowest first, NULL first", names(&mut db, query), Ok(["bob", "cy", "ann", "di"].map(String::from).to_vec()));
}

#[test]
fn a_failed_insert_adds_nothing() {
    let mut db = staff();
    let rows = vec![vec![text("ed"), int(1)], vec![text("fi"), bin(Op::Div, int(1), int(0))]];
    check!("the second row fails", db.run(&Statement::Insert { table: "emp".into(), rows }), Err(Fault::DivisionByZero));
    check!("still four", names(&mut db, select(vec![item("name")])).map(|v| v.len()), Ok(4));
}

#[test]
fn checks_come_before_rows() {
    let mut db = staff();
    let mut query = select(vec![item("bonus")]);
    query.limit = Some(0);
    check!("an unknown column, although nothing would run", db.run(&Statement::Select(query)), Err(Fault::UnknownColumn("bonus".into())));
}

#[test]
fn limit_stops_the_work() {
    let mut db = staff();
    // 100 / (pay - 50) fails on cy, the third row
    let share = bin(Op::Div, int(100), bin(Op::Sub, col("pay"), int(50)));
    let mut query = select(vec![Item::Expr { expr: share, name: "share".into() }]);
    query.limit = Some(2);
    check!("two rows are enough", db.run(&Statement::Select(query.clone())).map(|_| ()), Ok(()));
    query.limit = Some(3);
    check!("the third is needed", db.run(&Statement::Select(query)), Err(Fault::DivisionByZero));
}
