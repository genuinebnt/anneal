use anneal_prelude::Rng;
use solution::Expr as E;
use solution::*;

fn col(name: &str) -> E {
    E::Column(name.to_string())
}

fn int(n: i64) -> E {
    E::Lit(Value::Int(n))
}

fn null() -> E {
    E::Lit(Value::Null)
}

fn bin(op: Op, a: E, b: E) -> E {
    E::Binary(op, Box::new(a), Box::new(b))
}

fn item(name: &str) -> Item {
    Item::Expr { expr: col(name), name: name.to_string() }
}

fn expr(e: E) -> Item {
    Item::Expr { expr: e, name: "x".into() }
}

fn select(table: &str, items: Vec<Item>) -> Select {
    Select { items, table: table.into(), filter: None, group_by: None, order_by: None, limit: None }
}

fn create(db: &mut Database, table: &str, columns: &[(&str, Type)]) -> Result<Output, Fault> {
    let columns = columns.iter().map(|&(n, t)| (n.to_string(), t)).collect();
    db.run(&Statement::CreateTable { table: table.into(), columns })
}

fn insert(db: &mut Database, table: &str, rows: Vec<Vec<E>>) -> Result<Output, Fault> {
    db.run(&Statement::Insert { table: table.into(), rows })
}

fn rows(db: &mut Database, query: &Select) -> Result<Vec<Vec<Value>>, Fault> {
    match db.run(&Statement::Select(query.clone()))? {
        Output::Rows { rows, .. } => Ok(rows),
        other => panic!("not rows: {other:?}"),
    }
}

/// `t(k INT, d INT)` holding these pairs.
fn pairs(values: &[(i64, i64)]) -> Database {
    let mut db = Database::new();
    assert!(create(&mut db, "t", &[("k", Type::Int), ("d", Type::Int)]).is_ok());
    if !values.is_empty() {
        assert!(insert(&mut db, "t", values.iter().map(|&(k, d)| vec![int(k), int(d)]).collect()).is_ok());
    }
    db
}

#[test]
fn table_names_and_column_names() {
    let mut db = Database::new();
    check!("a repeated column", create(&mut db, "t", &[("a", Type::Int), ("b", Type::Int), ("b", Type::Text), ("a", Type::Int)]), Err(Fault::DuplicateColumn("b".into())));
    check!("no table was left behind", rows(&mut db, &select("t", vec![Item::Star])), Err(Fault::NoSuchTable("t".into())));
    check!("create", create(&mut db, "t", &[("a", Type::Int)]), Ok(Output::Created));
    check!("again, with a repeated column: the name is checked first", create(&mut db, "t", &[("x", Type::Int), ("x", Type::Int)]), Err(Fault::TableExists("t".into())));
    check!("the old table is still there", rows(&mut db, &select("t", vec![item("a")])), Ok(vec![]));
    check!("insert into nothing", insert(&mut db, "ghost", vec![vec![int(1)]]), Err(Fault::NoSuchTable("ghost".into())));
}

#[test]
fn what_an_insert_refuses() {
    let mut db = Database::new();
    assert!(create(&mut db, "t", &[("a", Type::Int), ("b", Type::Text)]).is_ok());
    let text = || E::Lit(Value::Text("x".into()));
    let good = || vec![int(1), text()];
    let cases = [
        ("too few", vec![good(), vec![int(1)]], Fault::WrongCount { needed: 2, found: 1 }),
        ("too many", vec![vec![int(1), text(), int(2)]], Fault::WrongCount { needed: 2, found: 3 }),
        ("the count comes before the values", vec![vec![text()]], Fault::WrongCount { needed: 2, found: 1 }),
        ("a column name", vec![good(), vec![col("a"), text()]], Fault::UnknownColumn("a".into())),
        ("a text for a number", vec![vec![text(), text()]], Fault::WrongType { needed: Type::Int, found: Type::Text }),
        ("the type decides, not the value", vec![good(), vec![bin(Op::Eq, null(), int(1)), text()]], Fault::WrongType { needed: Type::Int, found: Type::Bool }),
        ("the type is checked before computing", vec![vec![int(1), bin(Op::Div, int(1), int(0))]], Fault::WrongType { needed: Type::Text, found: Type::Int }),
        ("the first value first", vec![vec![bin(Op::Div, int(1), int(0)), int(5)]], Fault::DivisionByZero),
        ("the first row first", vec![vec![int(1), int(2)], vec![int(1)]], Fault::WrongType { needed: Type::Text, found: Type::Int }),
        ("late overflow", vec![good(), good(), vec![bin(Op::Add, int(i64::MAX), int(1)), text()]], Fault::Overflow),
    ];
    for (what, values, fault) in cases {
        check!(what, insert(&mut db, "t", values), Err(fault));
        check!(format!("{what}: nothing was added"), rows(&mut db, &select("t", vec![Item::Star])), Ok(vec![]));
    }
    check!("NULL and computed values", insert(&mut db, "t", vec![vec![null(), null()], vec![bin(Op::Add, int(2), int(3)), text()]]), Ok(Output::Inserted(2)));
    let want = vec![vec![Value::Null, Value::Null], vec![Value::Int(5), Value::Text("x".into())]];
    check!("stored", rows(&mut db, &select("t", vec![Item::Star])), Ok(want));
}

#[test]
fn the_order_of_the_checks() {
    let mut db = pairs(&[]);
    let mut query = select("nope", vec![item("zz")]);
    query.filter = Some(col("yy"));
    check!("the table first", rows(&mut db, &query), Err(Fault::NoSuchTable("nope".into())));
    query.table = "t".into();
    query.order_by = Some(OrderBy { column: "ww".into(), descending: false });
    check!("then WHERE", rows(&mut db, &query), Err(Fault::UnknownColumn("yy".into())));
    query.filter = Some(col("k"));
    check!("WHERE must be a Bool, on an empty table too", rows(&mut db, &query), Err(Fault::WrongType { needed: Type::Bool, found: Type::Int }));
    query.filter = Some(null());
    check!("then ORDER BY", rows(&mut db, &query), Err(Fault::UnknownColumn("ww".into())));
    query.order_by = None;
    query.items = vec![item("k"), expr(bin(Op::Add, col("k"), E::Lit(Value::Bool(true)))), item("zz")];
    check!("then the items, left to right", rows(&mut db, &query), Err(Fault::WrongType { needed: Type::Int, found: Type::Bool }));
    let mut by_title = select("t", vec![Item::Expr { expr: col("k"), name: "title".into() }]);
    by_title.order_by = Some(OrderBy { column: "title".into(), descending: false });
    check!("ORDER BY looks at the table, not at the titles", rows(&mut db, &by_title), Err(Fault::UnknownColumn("title".into())));
}

/// 1 / d fails on a row whose d is 0.
fn inverse(column: &str) -> E {
    bin(Op::Div, int(1), col(column))
}

#[test]
fn only_what_is_needed_runs() {
    let mut db = pairs(&[(3, 1), (2, 1), (1, 1), (4, 0)]);
    let mut where_fails = select("t", vec![item("k")]);
    where_fails.filter = Some(bin(Op::Eq, inverse("d"), int(1)));
    check!("the condition reaches the last row", rows(&mut db, &where_fails), Err(Fault::DivisionByZero));
    where_fails.limit = Some(3);
    check!("three rows are enough", rows(&mut db, &where_fails).map(|r| r.len()), Ok(3));
    where_fails.order_by = Some(OrderBy { column: "k".into(), descending: false });
    check!("sorting needs the condition on every row", rows(&mut db, &where_fails), Err(Fault::DivisionByZero));
    for order_by in [None, Some(OrderBy { column: "k".into(), descending: false })] {
        where_fails.order_by = order_by;
        where_fails.limit = Some(0);
        check!("LIMIT 0 runs nothing", rows(&mut db, &where_fails), Ok(vec![]));
    }

    let mut item_fails = select("t", vec![expr(inverse("d"))]);
    item_fails.order_by = Some(OrderBy { column: "k".into(), descending: false });
    item_fails.limit = Some(3);
    check!("the failing row sorts last, so it is never computed", rows(&mut db, &item_fails).map(|r| r.len()), Ok(3));
    item_fails.order_by = Some(OrderBy { column: "k".into(), descending: true });
    item_fails.limit = Some(1);
    check!("now it sorts first", rows(&mut db, &item_fails), Err(Fault::DivisionByZero));
}

#[test]
fn items_run_with_their_row() {
    // without ORDER BY, row 1's items run before row 2's condition
    let mut db = pairs(&[(i64::MAX, 1), (1, 0)]);
    let mut query = select("t", vec![expr(bin(Op::Add, col("k"), int(1)))]);
    query.filter = Some(bin(Op::Eq, inverse("d"), int(1)));
    check!("the overflow of the first row comes first", rows(&mut db, &query), Err(Fault::Overflow));
    query.order_by = Some(OrderBy { column: "k".into(), descending: true });
    check!("with ORDER BY every condition runs first", rows(&mut db, &query), Err(Fault::DivisionByZero));
    let two = select("t", vec![expr(inverse("d")), expr(bin(Op::Mul, col("k"), int(2)))]);
    check!("row 1's second item before row 2's first", rows(&mut db, &two), Err(Fault::Overflow));
}

#[test]
fn sorting_texts_and_booleans() {
    let mut db = Database::new();
    assert!(create(&mut db, "t", &[("s", Type::Text), ("b", Type::Bool)]).is_ok());
    let row = |s: &str, b: Option<bool>| vec![E::Lit(Value::Text(s.into())), E::Lit(b.map_or(Value::Null, Value::Bool))];
    assert!(insert(&mut db, "t", vec![row("b", Some(true)), row("B", None), row("é", Some(false)), row("a", Some(true))]).is_ok());
    let mut by = |column: &str, descending: bool| {
        let mut query = select("t", vec![item("s")]);
        query.order_by = Some(OrderBy { column: column.into(), descending });
        rows(&mut db, &query).map(|r| r.into_iter().map(|r| format!("{:?}", r[0])).collect::<Vec<_>>().join(" "))
    };
    check!("by bytes", by("s", false), Ok(r#"Text("B") Text("a") Text("b") Text("é")"#.to_string()));
    check!("NULL, FALSE, TRUE", by("b", false), Ok(r#"Text("B") Text("é") Text("b") Text("a")"#.to_string()));
    check!("TRUE, FALSE, NULL, ties as inserted", by("b", true), Ok(r#"Text("b") Text("a") Text("é") Text("B")"#.to_string()));
}

#[test]
fn random_queries() {
    let mut rng = Rng::new(18221);
    for _ in 0..400 {
        let mut db = Database::new();
        assert!(create(&mut db, "t", &[("id", Type::Int), ("a", Type::Int), ("b", Type::Int)]).is_ok());
        // rows arrive in several inserts
        let mut model: Vec<(i64, Option<i64>, i64)> = Vec::new();
        for _ in 0..rng.below(4) {
            let batch: Vec<(i64, Option<i64>, i64)> = (0..rng.int(1, 8)).map(|i| (model.len() as i64 + i, (rng.below(5) != 0).then(|| rng.int(-3, 3)), rng.int(0, 3))).collect();
            let values = batch.iter().map(|r| vec![int(r.0), r.1.map_or_else(null, int), int(r.2)]).collect();
            check!("insert", insert(&mut db, "t", values), Ok(Output::Inserted(batch.len())));
            model.extend(batch);
        }
        let bound = rng.int(-3, 3);
        let filtered = rng.bool();
        let order = rng.bool().then(|| (*rng.pick(&["a", "b"]), rng.bool()));
        let limit = (rng.below(3) != 0).then(|| rng.below(10) as u64);
        let mut query = select("t", vec![item("id")]);
        query.filter = filtered.then(|| bin(Op::Lt, col("a"), int(bound)));
        query.order_by = order.map(|(c, descending)| OrderBy { column: c.into(), descending });
        query.limit = limit;

        let mut kept: Vec<&(i64, Option<i64>, i64)> = model.iter().filter(|r| !filtered || r.1.is_some_and(|a| a < bound)).collect();
        if let Some((column, descending)) = order {
            let key = |r: &(i64, Option<i64>, i64)| if column == "a" { r.1 } else { Some(r.2) };
            // the insertion order is the id, so it can break ties
            kept.sort_unstable_by(|x, y| {
                let by_key = if descending { key(y).cmp(&key(x)) } else { key(x).cmp(&key(y)) };
                by_key.then(x.0.cmp(&y.0))
            });
        }
        let want: Vec<Vec<Value>> = kept.into_iter().take(limit.map_or(usize::MAX, |n| n as usize)).map(|r| vec![Value::Int(r.0)]).collect();
        check!(format!("{query:?} over {model:?}"), rows(&mut db, &query), Ok(want));
    }
}

#[test]
fn a_large_table() {
    let mut db = pairs(&(0..100_000).map(|i| (i % 1000, i)).collect::<Vec<_>>());
    let mut query = select("t", vec![item("d")]);
    query.order_by = Some(OrderBy { column: "k".into(), descending: true });
    query.limit = Some(2);
    check!("the first two rows with the largest key", rows(&mut db, &query), Ok(vec![vec![Value::Int(999)], vec![Value::Int(1999)]]));
    query.limit = Some(u64::MAX);
    check!("a huge limit", rows(&mut db, &query).map(|r| r.len()), Ok(100_000));
}
