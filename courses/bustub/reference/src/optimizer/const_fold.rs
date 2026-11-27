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
    // @begin 3h-c1
    use Expr::*;
    let lit = |v: Val| match v {
        Val::Int(i) => Int(i),
        Val::Bool(b) => Bool(b),
    };
    let has_col = |e: &Expr| -> bool {
        fn go(e: &Expr) -> bool {
            match e {
                Int(_) | Bool(_) => false,
                Col(_) => true,
                Add(a, b) | Mul(a, b) | Lt(a, b) | And(a, b) | Or(a, b) => go(a) || go(b),
                Not(a) => go(a),
            }
        }
        go(e)
    };
    // An identity may drop an operand only if that cannot hide a failure: `x + 0`, `x * 1`, `x AND TRUE`, `x OR FALSE` and
    // `NOT NOT x` keep `x`, so `x` only has to have the right type; `x * 0`, `x AND FALSE` and `x OR TRUE` drop `x` entirely, so `x`
    // must be unable to fail at all (no type error, no arithmetic that could overflow).
    #[derive(PartialEq)]
    enum Ty {
        I,
        B,
    }
    fn ty(e: &Expr) -> Option<Ty> {
        match e {
            Int(_) | Col(_) => Some(Ty::I),
            Bool(_) => Some(Ty::B),
            Add(a, b) | Mul(a, b) => (ty(a)? == Ty::I && ty(b)? == Ty::I).then_some(Ty::I),
            Lt(a, b) => (ty(a)? == Ty::I && ty(b)? == Ty::I).then_some(Ty::B),
            And(a, b) | Or(a, b) => (ty(a)? == Ty::B && ty(b)? == Ty::B).then_some(Ty::B),
            Not(a) => (ty(a)? == Ty::B).then_some(Ty::B),
        }
    }
    fn cannot_fail(e: &Expr) -> bool {
        match e {
            Int(_) | Bool(_) | Col(_) => true,
            Add(..) | Mul(..) => false,
            Lt(a, b) => ty(a) == Some(Ty::I) && ty(b) == Some(Ty::I) && cannot_fail(a) && cannot_fail(b),
            And(a, b) | Or(a, b) => ty(a) == Some(Ty::B) && ty(b) == Some(Ty::B) && cannot_fail(a) && cannot_fail(b),
            Not(a) => ty(a) == Some(Ty::B) && cannot_fail(a),
        }
    }
    let is = |x: &Expr, t: Ty| ty(x) == Some(t);
    let out = match e {
        Int(_) | Bool(_) | Col(_) => return e.clone(),
        Add(a, b) => {
            let (a, b) = (fold(a), fold(b));
            match (&a, &b) {
                (Int(0), x) | (x, Int(0)) if is(x, Ty::I) => x.clone(),
                _ => Add(Box::new(a), Box::new(b)),
            }
        }
        Mul(a, b) => {
            let (a, b) = (fold(a), fold(b));
            match (&a, &b) {
                (Int(1), x) | (x, Int(1)) if is(x, Ty::I) => x.clone(),
                (Int(0), x) | (x, Int(0)) if cannot_fail(x) && is(x, Ty::I) => Int(0),
                _ => Mul(Box::new(a), Box::new(b)),
            }
        }
        Lt(a, b) => Lt(Box::new(fold(a)), Box::new(fold(b))),
        And(a, b) => {
            let (a, b) = (fold(a), fold(b));
            match (&a, &b) {
                (Bool(true), x) | (x, Bool(true)) if is(x, Ty::B) => x.clone(),
                (Bool(false), x) | (x, Bool(false)) if cannot_fail(x) && is(x, Ty::B) => Bool(false),
                _ => And(Box::new(a), Box::new(b)),
            }
        }
        Or(a, b) => {
            let (a, b) = (fold(a), fold(b));
            match (&a, &b) {
                (Bool(false), x) | (x, Bool(false)) if is(x, Ty::B) => x.clone(),
                (Bool(true), x) | (x, Bool(true)) if cannot_fail(x) && is(x, Ty::B) => Bool(true),
                _ => Or(Box::new(a), Box::new(b)),
            }
        }
        Not(a) => match fold(a) {
            Not(inner) if is(&inner, Ty::B) => *inner,
            Bool(b) => Bool(!b),
            x => Not(Box::new(x)),
        },
    };
    if !has_col(&out) {
        if let Some(v) = eval(&out, &[]) {
            return lit(v);
        }
    }
    out
    //~ todo!("3h-c1: fold the children, evaluate what has no column, then apply the identities")
    // @end
}
