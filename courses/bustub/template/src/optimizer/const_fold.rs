//! Constant folding for a small expression language.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Int(i64),
    Bool(bool),
    Col(usize),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Val {
    Int(i64),
    Bool(bool),
}

/// The value of `e` on `row`; `None` for a type error or an overflow (such trees are not folded away).
pub fn eval(e: &Expr, row: &[i64]) -> Option<Val> {
    use Expr::*;
    Some(match e {
        Int(v) => Val::Int(*v),
        Bool(b) => Val::Bool(*b),
        Col(i) => Val::Int(*row.get(*i)?),
        Add(a, b) => match (eval(a, row)?, eval(b, row)?) {
            (Val::Int(x), Val::Int(y)) => Val::Int(x.checked_add(y)?),
            _ => return None,
        },
        Mul(a, b) => match (eval(a, row)?, eval(b, row)?) {
            (Val::Int(x), Val::Int(y)) => Val::Int(x.checked_mul(y)?),
            _ => return None,
        },
        Lt(a, b) => match (eval(a, row)?, eval(b, row)?) {
            (Val::Int(x), Val::Int(y)) => Val::Bool(x < y),
            _ => return None,
        },
        And(a, b) => match (eval(a, row)?, eval(b, row)?) {
            (Val::Bool(x), Val::Bool(y)) => Val::Bool(x && y),
            _ => return None,
        },
        Or(a, b) => match (eval(a, row)?, eval(b, row)?) {
            (Val::Bool(x), Val::Bool(y)) => Val::Bool(x || y),
            _ => return None,
        },
        Not(a) => match eval(a, row)? {
            Val::Bool(x) => Val::Bool(!x),
            _ => return None,
        },
    })
}

/// Simplifies `e` without changing its value: subtrees with no column are evaluated, and the identities `x + 0`, `x * 1`, `x * 0`, `x AND TRUE`,
/// `x AND FALSE`, `x OR FALSE`, `x OR TRUE` and `NOT NOT x` are applied bottom-up. A subtree whose evaluation fails (a type error, an overflow)
/// is left as it is.
pub fn fold(e: &Expr) -> Expr {
    todo!("3h-c1: fold the children, evaluate what has no column, then apply the identities")
}
