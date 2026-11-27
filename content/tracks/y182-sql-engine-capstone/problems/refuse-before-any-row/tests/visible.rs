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
fn null() -> E {
    E::Lit(Value::Null)
}

#[allow(dead_code)]
fn bin(op: Op, a: E, b: E) -> E {
    E::Binary(op, Box::new(a), Box::new(b))
}

#[allow(dead_code)]
fn columns() -> Vec<(String, Type)> {
    vec![("pay".to_string(), Type::Int), ("name".to_string(), Type::Text), ("active".to_string(), Type::Bool)]
}

#[allow(dead_code)]
fn value(e: &E, row: &[Value]) -> Result<Value, Fault> {
    compile(e, &columns())?.eval(row)
}

#[allow(dead_code)]
fn ann() -> Vec<Value> {
    vec![Value::Int(70), Value::Text("ann".into()), Value::Bool(true)]
}

#[test]
fn a_condition_on_a_row() {
    let e = bin(Op::And, bin(Op::Gt, col("pay"), int(50)), col("active"));
    let Ok(compiled) = compile(&e, &columns()) else {
        panic!("it compiles");
    };
    check!("its type", compiled.ty(), Some(Type::Bool));
    check!("pay > 50 AND active", compiled.eval(&ann()), Ok(Value::Bool(true)));
}

#[test]
fn faults_come_without_any_row() {
    let compared = bin(Op::Eq, col("pay"), col("name"));
    check!("pay = name", compile(&compared, &columns()).err(), Some(Fault::WrongType { needed: Type::Int, found: Type::Text }));
    let missing = bin(Op::Add, col("name"), col("bonus"));
    check!("name + bonus: both sides are looked at first", compile(&missing, &columns()).err(), Some(Fault::UnknownColumn("bonus".into())));
}

#[test]
fn unknown_values() {
    let row = [Value::Null, Value::Null, Value::Null];
    check!("pay = pay, pay unknown", value(&bin(Op::Eq, col("pay"), col("pay")), &row), Ok(Value::Null));
    check!("active OR TRUE", value(&bin(Op::Or, col("active"), E::Lit(Value::Bool(true))), &row), Ok(Value::Bool(true)));
    check!("active AND TRUE", value(&bin(Op::And, col("active"), E::Lit(Value::Bool(true))), &row), Ok(Value::Null));
}

#[test]
fn a_guard_protects_the_right_side() {
    let division = bin(Op::Eq, bin(Op::Div, int(1), int(0)), int(1));
    let guarded = bin(Op::And, E::Lit(Value::Bool(false)), division.clone());
    check!("FALSE AND 1/0 = 1", value(&guarded, &ann()), Ok(Value::Bool(false)));
    check!("1/0 = 1", value(&division, &ann()), Err(Fault::DivisionByZero));
}

#[test]
fn overflow_is_a_fault() {
    check!("MAX + 1", value(&bin(Op::Add, int(i64::MAX), int(1)), &ann()), Err(Fault::Overflow));
    check!("NULL fits every type", value(&bin(Op::Add, null(), col("pay")), &ann()), Ok(Value::Null));
}
