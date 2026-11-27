use anneal_prelude::Rng;
use solution::Expr as E;
use solution::*;

fn col(name: &str) -> E {
    E::Column(name.to_string())
}

fn int(n: i64) -> E {
    E::Lit(Value::Int(n))
}

fn truth(t: Option<bool>) -> E {
    E::Lit(t.map_or(Value::Null, Value::Bool))
}

fn null() -> E {
    E::Lit(Value::Null)
}

fn bin(op: Op, a: E, b: E) -> E {
    E::Binary(op, Box::new(a), Box::new(b))
}

fn not(a: E) -> E {
    E::Not(Box::new(a))
}

fn neg(a: E) -> E {
    E::Neg(Box::new(a))
}

fn value(e: &E) -> Result<Value, Fault> {
    compile(e, &[])?.eval(&[])
}

fn refused(e: &E, columns: &[(String, Type)]) -> Option<Fault> {
    compile(e, columns).err()
}

fn wrong(needed: Type, found: Type) -> Option<Fault> {
    Some(Fault::WrongType { needed, found })
}

const TRUTHS: [Option<bool>; 3] = [Some(true), Some(false), None];

#[test]
fn the_truth_tables() {
    let out = |t: Option<bool>| Ok::<_, Fault>(t.map_or(Value::Null, Value::Bool));
    // rows and columns in the order TRUE, FALSE, NULL
    let (t, f, u) = (Some(true), Some(false), None);
    let and = [[t, f, u], [f, f, f], [u, f, u]];
    let or = [[t, t, t], [t, f, u], [t, u, u]];
    for (i, a) in TRUTHS.into_iter().enumerate() {
        for (j, b) in TRUTHS.into_iter().enumerate() {
            check!(format!("{a:?} AND {b:?}"), value(&bin(Op::And, truth(a), truth(b))), out(and[i][j]));
            check!(format!("{a:?} OR {b:?}"), value(&bin(Op::Or, truth(a), truth(b))), out(or[i][j]));
        }
        check!(format!("NOT {a:?}"), value(&not(truth(a))), out(a.map(|x| !x)));
        let missing = E::IsNull { of: Box::new(truth(a)), negated: false };
        check!(format!("{a:?} IS NULL"), value(&missing), Ok(Value::Bool(a.is_none())));
        let present = E::IsNull { of: Box::new(truth(a)), negated: true };
        check!(format!("{a:?} IS NOT NULL"), value(&present), Ok(Value::Bool(a.is_some())));
    }
}

#[test]
fn which_side_runs() {
    let boom = || bin(Op::Eq, bin(Op::Div, int(1), int(0)), int(0));
    check!("FALSE AND boom", value(&bin(Op::And, truth(Some(false)), boom())), Ok(Value::Bool(false)));
    check!("TRUE OR boom", value(&bin(Op::Or, truth(Some(true)), boom())), Ok(Value::Bool(true)));
    for (op, left) in [(Op::And, Some(true)), (Op::And, None), (Op::Or, Some(false)), (Op::Or, None), (Op::Eq, None)] {
        check!(format!("{left:?} {op:?} boom"), value(&bin(op, truth(left), boom())), Err(Fault::DivisionByZero));
    }
    check!("boom AND FALSE: the left runs first", value(&bin(Op::And, boom(), truth(Some(false)))), Err(Fault::DivisionByZero));
    let both = bin(Op::Add, bin(Op::Mul, int(i64::MAX), int(2)), bin(Op::Div, int(1), int(0)));
    check!("two faults: the left one", value(&both), Err(Fault::Overflow));
    let nothing = bin(Op::Add, null(), bin(Op::Div, int(1), int(0)));
    check!("NULL + 1/0", value(&nothing), Err(Fault::DivisionByZero));
    check!("NULL / 0", value(&bin(Op::Div, null(), int(0))), Ok(Value::Null));
    check!("1 / NULL", value(&bin(Op::Div, int(1), null())), Ok(Value::Null));
}

#[test]
fn the_ends_of_the_integers() {
    let overflow = Err(Fault::Overflow);
    check!("MIN - 1", value(&bin(Op::Sub, int(i64::MIN), int(1))), overflow.clone());
    check!("MIN * -1", value(&bin(Op::Mul, int(i64::MIN), int(-1))), overflow.clone());
    check!("MIN / -1", value(&bin(Op::Div, int(i64::MIN), int(-1))), overflow.clone());
    check!("- MIN", value(&neg(int(i64::MIN))), overflow);
    check!("- MAX", value(&neg(int(i64::MAX))), Ok(Value::Int(-i64::MAX)));
    check!("-7 / 2 rounds toward zero", value(&bin(Op::Div, int(-7), int(2))), Ok(Value::Int(-3)));
    check!("- NULL", value(&neg(null())), Ok(Value::Null));
}

#[test]
fn comparing_texts_and_booleans() {
    let text = |s: &str| E::Lit(Value::Text(s.to_string()));
    let yes = Ok(Value::Bool(true));
    check!("bytes: Z before a", value(&bin(Op::Lt, text("Z"), text("a"))), yes.clone());
    check!("a prefix comes first", value(&bin(Op::Lt, text("ab"), text("abc"))), yes.clone());
    check!("FALSE < TRUE", value(&bin(Op::Lt, truth(Some(false)), truth(Some(true)))), yes.clone());
    check!("10 > 9", value(&bin(Op::Gt, int(10), int(9))), yes);
    check!("\"10\" < \"9\"", value(&bin(Op::Lt, text("10"), text("9"))), Ok(Value::Bool(true)));
}

#[test]
fn what_each_operator_asks_for() {
    let columns = [("i".to_string(), Type::Int), ("s".to_string(), Type::Text), ("b".to_string(), Type::Bool)];
    check!("NOT i", refused(&not(col("i")), &columns), wrong(Type::Bool, Type::Int));
    check!("- s", refused(&neg(col("s")), &columns), wrong(Type::Int, Type::Text));
    check!("b AND i", refused(&bin(Op::And, col("b"), col("i")), &columns), wrong(Type::Bool, Type::Int));
    check!("s OR i: the left first", refused(&bin(Op::Or, col("s"), col("i")), &columns), wrong(Type::Bool, Type::Text));
    check!("i * b", refused(&bin(Op::Mul, col("i"), col("b")), &columns), wrong(Type::Int, Type::Bool));
    check!("s < b", refused(&bin(Op::Lt, col("s"), col("b")), &columns), wrong(Type::Text, Type::Bool));
    check!("NULL >= s", refused(&bin(Op::Ge, null(), col("s")), &columns), None);
    check!("NOT (i + 1)", refused(&not(bin(Op::Add, col("i"), int(1))), &columns), wrong(Type::Bool, Type::Int));
    check!("(NULL = s) + 1: the comparison is a Bool", refused(&bin(Op::Add, bin(Op::Eq, null(), col("s")), int(1)), &columns), wrong(Type::Int, Type::Bool));
    check!("names match exactly", refused(&col("I"), &columns), Some(Fault::UnknownColumn("I".into())));
    check!("i AND ghost: the right side is checked before the left is judged", refused(&bin(Op::And, col("i"), col("ghost")), &columns), Some(Fault::UnknownColumn("ghost".into())));
    check!("(s + 1) = ghost: inside the left comes first", refused(&bin(Op::Eq, bin(Op::Add, col("s"), int(1)), col("ghost")), &columns), wrong(Type::Int, Type::Text));
    check!("FALSE AND (1 = b) is refused although the right side would not run", refused(&bin(Op::And, truth(Some(false)), bin(Op::Eq, int(1), col("b"))), &columns), wrong(Type::Int, Type::Bool));
    check!("1 / 0 compiles", refused(&bin(Op::Div, int(1), int(0)), &columns), None);
}

#[test]
fn the_type_of_the_whole() {
    let kind = |e: E| compile(&e, &[]).ok().map(|c| c.ty());
    check!("NULL", kind(null()), Some(None));
    check!("NULL + NULL", kind(bin(Op::Add, null(), null())), Some(Some(Type::Int)));
    check!("NOT NULL", kind(not(null())), Some(Some(Type::Bool)));
    check!("- NULL", kind(neg(null())), Some(Some(Type::Int)));
    check!("'a'", kind(E::Lit(Value::Text("a".into()))), Some(Some(Type::Text)));
}

/// What a part is worth, worked out while the part is made.
type Worth = Result<Value, Fault>;

/// A random well-typed expression over the row (i = 3, n = NULL, s = "m"), and what it is worth.
fn typed(rng: &mut Rng, want: Type, depth: u32) -> (E, Worth) {
    if depth == 0 || rng.below(4) == 0 {
        return match (want, rng.below(3)) {
            (_, 0) => (null(), Ok(Value::Null)),
            (Type::Int, 1) => {
                if rng.bool() {
                    (col("i"), Ok(Value::Int(3)))
                } else {
                    (col("n"), Ok(Value::Null))
                }
            }
            (Type::Int, _) => {
                let n = *rng.pick(&[0, 1, -1, 2, i64::MAX, i64::MIN]);
                (int(n), Ok(Value::Int(n)))
            }
            (Type::Text, 1) => (col("s"), Ok(Value::Text("m".into()))),
            (Type::Text, _) => {
                let s = *rng.pick(&["", "m", "n"]);
                (E::Lit(Value::Text(s.into())), Ok(Value::Text(s.into())))
            }
            (Type::Bool, _) => {
                let b = rng.bool();
                (truth(Some(b)), Ok(Value::Bool(b)))
            }
        };
    }
    let d = depth - 1;
    match want {
        Type::Text => typed(rng, want, 0),
        Type::Int if rng.below(5) == 0 => {
            let (a, x) = typed(rng, Type::Int, d);
            let y = x.and_then(|x| match x {
                Value::Int(n) => n.checked_neg().map(Value::Int).ok_or(Fault::Overflow),
                other => Ok(other),
            });
            (neg(a), y)
        }
        Type::Int => {
            let (a, x) = typed(rng, Type::Int, d);
            let (b, y) = typed(rng, Type::Int, d);
            let op = *rng.pick(&[Op::Add, Op::Sub, Op::Mul, Op::Div]);
            let worth = x.and_then(|x| {
                Ok(match (x, y?) {
                    (Value::Int(_), Value::Int(0)) if op == Op::Div => return Err(Fault::DivisionByZero),
                    (Value::Int(x), Value::Int(y)) => {
                        let (x, y) = (i128::from(x), i128::from(y));
                        let wide = match op {
                            Op::Add => x + y,
                            Op::Sub => x - y,
                            Op::Mul => x * y,
                            _ => x / y,
                        };
                        Value::Int(i64::try_from(wide).map_err(|_| Fault::Overflow)?)
                    }
                    _ => Value::Null,
                })
            });
            (bin(op, a, b), worth)
        }
        Type::Bool => match rng.below(4) {
            0 => {
                let (a, x) = typed(rng, Type::Bool, d);
                let y = x.map(|x| match x {
                    Value::Bool(b) => Value::Bool(!b),
                    other => other,
                });
                (not(a), y)
            }
            1 => {
                let of = *rng.pick(&[Type::Int, Type::Text, Type::Bool]);
                let (a, x) = typed(rng, of, d);
                let negated = rng.bool();
                let y = x.map(|x| Value::Bool((x == Value::Null) != negated));
                (E::IsNull { of: Box::new(a), negated }, y)
            }
            2 => {
                let of = *rng.pick(&[Type::Int, Type::Text, Type::Bool]);
                let (a, x) = typed(rng, of, d);
                let (b, y) = typed(rng, of, d);
                let all = [Op::Eq, Op::Ne, Op::Lt, Op::Le, Op::Gt, Op::Ge];
                let op = *rng.pick(&all);
                let worth = x.and_then(|x| {
                    let y = y?;
                    if x == Value::Null || y == Value::Null {
                        return Ok(Value::Null);
                    }
                    // for each operator: does it hold when the left is smaller, equal, larger?
                    let holds = [[false, true, false], [true, false, true], [true, false, false], [true, true, false], [false, false, true], [false, true, true]];
                    let at = all.iter().position(|&o| o == op).unwrap();
                    Ok(Value::Bool(holds[at][(x.cmp(&y) as i8 + 1) as usize]))
                });
                (bin(op, a, b), worth)
            }
            _ => {
                let (a, x) = typed(rng, Type::Bool, d);
                let (b, y) = typed(rng, Type::Bool, d);
                let op = if rng.bool() { Op::And } else { Op::Or };
                let settles = Value::Bool(op == Op::Or);
                let worth = x.and_then(|x| {
                    if x == settles {
                        return Ok(x);
                    }
                    let y = y?;
                    Ok(if y == settles {
                        y
                    } else if x == Value::Null || y == Value::Null {
                        Value::Null
                    } else {
                        x
                    })
                });
                (bin(op, a, b), worth)
            }
        },
    }
}

#[test]
fn random_expressions() {
    let columns = [("i".to_string(), Type::Int), ("n".to_string(), Type::Int), ("s".to_string(), Type::Text)];
    let row = [Value::Int(3), Value::Null, Value::Text("m".into())];
    let mut rng = Rng::new(18211);
    for _ in 0..4000 {
        let want = *rng.pick(&[Type::Int, Type::Bool]);
        let (e, worth) = typed(&mut rng, want, 4);
        let got = compile(&e, &columns).and_then(|c| c.eval(&row));
        check!(format!("{e:?}"), got, worth);
    }
}

#[test]
fn one_compiled_expression_serves_many_rows() {
    let columns = [("a".to_string(), Type::Int), ("b".to_string(), Type::Int)];
    let e = bin(Op::Lt, bin(Op::Add, col("a"), col("b")), int(100));
    let Ok(compiled) = compile(&e, &columns) else {
        panic!("it compiles");
    };
    for i in 0..20_000 {
        let got = compiled.eval(&[Value::Int(i % 150), Value::Int(i % 7)]);
        check!(format!("row {i}"), got, Ok(Value::Bool(i % 150 + i % 7 < 100)));
    }
}

#[test]
fn the_tallest_expression() {
    let mut e = int(0);
    for _ in 0..99 {
        e = bin(Op::Add, e, int(1));
    }
    check!("99 additions", value(&e), Ok(Value::Int(99)));
}
