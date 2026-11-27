//! A small model of SQL for property tests: random expressions over a table `t(a, b, c)` of integers, printed as SQL with the fewest
//! parentheses, and evaluated on plain Rust rows. The database must agree with the model. Not part of the course.
#![allow(dead_code)]

use bustub::sql::ast::Expr;
use proptest::prelude::*;

/// One row of `t(a, b, c)`; `None` is NULL.
pub type Row = Vec<Option<i64>>;

pub const COLUMNS: [&str; 3] = ["a", "b", "c"];

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Int(Option<i64>),
    Bool(Option<bool>),
}

pub fn bin(op: &str, l: Expr, r: Expr) -> Expr {
    Expr::Binary { op: op.into(), left: Box::new(l), right: Box::new(r) }
}

pub fn col(name: &str) -> Expr {
    Expr::Column(vec![name.to_string()])
}

fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Binary { op, .. } => match op.as_str() {
            "or" => 1,
            "and" => 2,
            "+" | "-" => 5,
            _ => 4,
        },
        _ => 9,
    }
}

fn paren(e: &Expr, needed: bool) -> String {
    if needed {
        format!("({})", show(e))
    } else {
        show(e)
    }
}

/// SQL text for `e` (integers, NULL, columns, `+ - = <> < <= > >= and or`).
pub fn show(e: &Expr) -> String {
    match e {
        Expr::Integer(v) => v.to_string(),
        Expr::Null => "null".into(),
        Expr::Bool(b) => b.to_string(),
        Expr::Column(parts) => parts.join("."),
        Expr::Binary { op, left, right } => {
            let p = prec(e);
            format!("{} {op} {}", paren(left, prec(left) < p), paren(right, prec(right) <= p))
        }
        other => panic!("the model prints only integers, NULL, columns and binary operators: {other:?}"),
    }
}

/// What `e` is on `row` (columns `a`, `b`, `c`). Arithmetic does not overflow for the small numbers the strategies generate.
pub fn eval(e: &Expr, row: &Row) -> Val {
    eval_in(e, row, &COLUMNS)
}

/// What `e` is on `row`, whose columns have the (possibly dotted, `l.k`) names `cols`.
pub fn eval_in(e: &Expr, row: &Row, cols: &[&str]) -> Val {
    match e {
        Expr::Integer(v) => Val::Int(Some(*v)),
        Expr::Null => Val::Int(None),
        Expr::Bool(b) => Val::Bool(Some(*b)),
        Expr::Column(parts) => Val::Int(row[cols.iter().position(|c| *c == parts.join(".")).expect("a column of the row")]),
        Expr::Binary { op, left, right } => match (op.as_str(), eval_in(left, row, cols), eval_in(right, row, cols)) {
            ("and", Val::Bool(l), Val::Bool(r)) => Val::Bool(match (l, r) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            }),
            ("or", Val::Bool(l), Val::Bool(r)) => Val::Bool(match (l, r) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            }),
            ("+", Val::Int(l), Val::Int(r)) => Val::Int(l.zip(r).map(|(a, b)| a + b)),
            ("-", Val::Int(l), Val::Int(r)) => Val::Int(l.zip(r).map(|(a, b)| a - b)),
            (cmp, Val::Int(l), Val::Int(r)) => Val::Bool(l.zip(r).map(|(a, b)| match cmp {
                "=" => a == b,
                "<>" => a != b,
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                _ => a >= b,
            })),
            other => panic!("ill-typed model expression: {other:?}"),
        },
        other => panic!("not in the model: {other:?}"),
    }
}

/// True when the predicate keeps the row: only TRUE keeps it, FALSE and unknown do not.
pub fn keeps(pred: &Expr, row: &Row) -> bool {
    keeps_in(pred, row, &COLUMNS)
}

pub fn keeps_in(pred: &Expr, row: &Row, cols: &[&str]) -> bool {
    eval_in(pred, row, cols) == Val::Bool(Some(true))
}

pub fn as_int(v: Val) -> Option<i64> {
    match v {
        Val::Int(i) => i,
        other => panic!("expected an integer, got {other:?}"),
    }
}

/// A row as the shell prints it: cells separated by a space, NULL as `integer_null`.
pub fn line(row: &Row) -> String {
    row.iter().map(|c| c.map_or("integer_null".to_string(), |v| v.to_string())).collect::<Vec<_>>().join(" ")
}

/// A row as SQL values: `(1, null, 3)`.
pub fn values(row: &Row) -> String {
    format!("({})", row.iter().map(|c| c.map_or("null".to_string(), |v| v.to_string())).collect::<Vec<_>>().join(", "))
}

pub fn sorted_lines(rows: &[Row]) -> Vec<String> {
    let mut v: Vec<String> = rows.iter().map(line).collect();
    v.sort();
    v
}

/// A small integer, a column or NULL; sums and differences of them (to the given depth).
pub fn arith(columns: &'static [&'static str], depth: u32) -> impl Strategy<Value = Expr> {
    let leaf = prop_oneof![
        3 => prop::sample::select(columns.to_vec()).prop_map(col),
        3 => (-9i64..=9).prop_map(Expr::Integer),
        1 => Just(Expr::Null),
    ];
    leaf.prop_recursive(depth, 8, 2, |inner| {
        (prop::sample::select(vec!["+", "-"]), inner.clone(), inner).prop_map(|(op, l, r)| bin(op, l, r))
    })
}

/// Comparisons of arithmetic, combined with `and` / `or`.
pub fn predicate(columns: &'static [&'static str]) -> impl Strategy<Value = Expr> {
    let cmp = (prop::sample::select(vec!["=", "<>", "<", "<=", ">", ">="]), arith(columns, 1), arith(columns, 1))
        .prop_map(|(op, l, r)| bin(op, l, r));
    cmp.prop_recursive(2, 6, 2, |inner| {
        (prop::sample::select(vec!["and", "or"]), inner.clone(), inner).prop_map(|(op, l, r)| bin(op, l, r))
    })
}

/// A cell: usually a small integer, sometimes NULL.
pub fn cell() -> impl Strategy<Value = Option<i64>> {
    prop_oneof![1 => Just(None), 8 => (-12i64..=12).prop_map(Some)]
}

/// The naive join: every pair, the ON predicate on the pair; a left join adds the unmatched left rows padded with NULLs. Sorted lines.
pub fn naive_join(l: &[Row], r: &[Row], right_width: usize, left: bool, on: impl Fn(&Row) -> bool) -> Vec<String> {
    let mut out = vec![];
    for lr in l {
        let mut matched = false;
        for rr in r {
            let pair: Row = lr.iter().chain(rr.iter()).copied().collect();
            if on(&pair) {
                matched = true;
                out.push(line(&pair));
            }
        }
        if left && !matched {
            out.push(line(&lr.iter().copied().chain(std::iter::repeat(None).take(right_width)).collect()));
        }
    }
    out.sort();
    out
}
