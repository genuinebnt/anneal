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
fn bin(op: Op, a: E, b: E) -> E {
    E::Binary(op, Box::new(a), Box::new(b))
}

/// The WHERE condition of `SELECT * FROM t WHERE <text>`.
#[allow(dead_code)]
fn condition(text: &str) -> Result<E, SyntaxError> {
    match parse(&format!("SELECT * FROM t WHERE {text}"))? {
        Statement::Select(s) => Ok(s.filter.expect("a filter")),
        other => panic!("not a select: {other:?}"),
    }
}

#[allow(dead_code)]
fn bad(problem: Problem, start: usize, end: usize) -> Result<Statement, SyntaxError> {
    Err(SyntaxError { problem, start, end })
}

#[test]
fn create_and_insert() {
    let columns = vec![("id".to_string(), Type::Int), ("text".to_string(), Type::Text), ("done".to_string(), Type::Bool)];
    check!("create", parse("create table Notes (Id int, Text TEXT, done Bool);"), Ok(Statement::CreateTable { table: "notes".into(), columns }));
    let rows = vec![vec![int(1), E::Lit(Value::Text("it's Ok".into()))], vec![E::Neg(Box::new(int(2))), E::Lit(Value::Null)]];
    check!("insert", parse("INSERT INTO notes VALUES (1, 'it''s Ok'), (-2, NULL)"), Ok(Statement::Insert { table: "notes".into(), rows }));
}

#[test]
fn a_select_with_every_clause() {
    let got = parse("SELECT Dept, COUNT(*), sum(pay), pay*2  +1, (id) FROM staff WHERE pay > 10 GROUP BY dept ORDER BY dept DESC LIMIT 3");
    let items = vec![
        Item::Expr { expr: col("dept"), name: "dept".into() },
        Item::Aggregate { of: Aggregate::Count, column: None },
        Item::Aggregate { of: Aggregate::Sum, column: Some("pay".into()) },
        Item::Expr { expr: bin(Op::Add, bin(Op::Mul, col("pay"), int(2)), int(1)), name: "pay*2  +1".into() },
        Item::Expr { expr: col("id"), name: "id".into() },
    ];
    let select = Select {
        items,
        table: "staff".into(),
        filter: Some(bin(Op::Gt, col("pay"), int(10))),
        group_by: Some("dept".into()),
        order_by: Some(OrderBy { column: "dept".into(), descending: true }),
        limit: Some(3),
    };
    check!("the tree", got, Ok(Statement::Select(select)));
}

#[test]
fn stronger_operators_take_their_operands_first() {
    let want = bin(Op::Or, col("a"), bin(Op::And, col("b"), E::Not(Box::new(bin(Op::Eq, col("c"), bin(Op::Add, int(1), bin(Op::Mul, int(2), int(3))))))));
    check!("a OR b AND NOT c = 1 + 2 * 3", condition("a OR b AND NOT c = 1 + 2 * 3"), Ok(want));
    check!("10 - 4 - 3", condition("10 - 4 - 3"), Ok(bin(Op::Sub, bin(Op::Sub, int(10), int(4)), int(3))));
    let missing = E::IsNull { of: Box::new(bin(Op::Eq, col("a"), col("b"))), negated: true };
    check!("a = b IS NOT NULL", condition("a = b IS NOT NULL"), Ok(missing));
}

#[test]
fn a_wrong_token_is_pointed_at() {
    check!("no item", parse("SELECT FROM emp"), bad(Problem::Unexpected, 7, 11));
    check!("a = NOT b", parse("SELECT a = NOT b FROM t"), bad(Problem::Unexpected, 11, 14));
    check!("it stops too early", parse("SELECT * FROM emp LIMIT"), bad(Problem::Unexpected, 23, 23));
}

#[test]
fn bad_tokens() {
    check!("a character", parse("SELECT # FROM t"), bad(Problem::BadCharacter, 7, 8));
    check!("an open text", parse("SELECT 'oops FROM t"), bad(Problem::UnclosedText, 7, 19));
    check!("a huge number", parse("SELECT 9223372036854775808 FROM t"), bad(Problem::NumberTooLarge, 7, 26));
}
