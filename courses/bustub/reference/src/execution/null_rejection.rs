//! Does a condition fail on a row that an outer join padded with NULLs?

/// An integer-valued expression.
#[derive(Debug, Clone, PartialEq)]
pub enum S {
    Col(usize),
    Lit(i64),
    Coalesce(Box<S>, Box<S>),
}

/// A condition.
#[derive(Debug, Clone, PartialEq)]
pub enum B {
    Gt(S, S),
    Eq(S, S),
    IsNull(S),
    IsNotNull(S),
    And(Box<B>, Box<B>),
    Or(Box<B>, Box<B>),
    Not(Box<B>),
}

pub fn eval_s(s: &S, row: &[Option<i64>]) -> Option<i64> {
    match s {
        S::Col(i) => row[*i],
        S::Lit(v) => Some(*v),
        S::Coalesce(a, b) => eval_s(a, row).or_else(|| eval_s(b, row)),
    }
}

/// Three-valued evaluation: `None` is unknown.
pub fn eval(b: &B, row: &[Option<i64>]) -> Option<bool> {
    match b {
        B::Gt(x, y) => Some(eval_s(x, row)? > eval_s(y, row)?),
        B::Eq(x, y) => Some(eval_s(x, row)? == eval_s(y, row)?),
        B::IsNull(x) => Some(eval_s(x, row).is_none()),
        B::IsNotNull(x) => Some(eval_s(x, row).is_some()),
        B::And(x, y) => match (eval(x, row), eval(y, row)) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        },
        B::Or(x, y) => match (eval(x, row), eval(y, row)) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        },
        B::Not(x) => eval(x, row).map(|v| !v),
    }
}

/// Is `cond` certainly not TRUE when the columns in `nulls` are NULL, whatever the other columns hold?
pub fn rejects_nulls(cond: &B, nulls: &[usize]) -> bool {
    // @begin 3j-c5
    // what is known about an integer expression
    #[derive(Clone, Copy, PartialEq)]
    enum A {
        Null,
        Const(i64),
        Any,
        AnyOrNull,
    }
    // truth values that may occur: bit 1 = true, 2 = false, 4 = unknown
    fn abs_s(s: &S, nulls: &[usize]) -> A {
        match s {
            S::Col(i) => {
                if nulls.contains(i) {
                    A::Null
                } else {
                    A::AnyOrNull
                }
            }
            S::Lit(v) => A::Const(*v),
            S::Coalesce(a, b) => match (abs_s(a, nulls), abs_s(b, nulls)) {
                (A::Null, b) => b,
                (a @ (A::Const(_) | A::Any), _) => a,
                (A::AnyOrNull, A::AnyOrNull | A::Null) => A::AnyOrNull,
                (A::AnyOrNull, _) => A::Any,
            },
        }
    }
    fn cmp(a: A, b: A, eq: bool) -> u8 {
        match (a, b) {
            (A::Null, _) | (_, A::Null) => 4,
            (A::Const(x), A::Const(y)) => {
                if (eq && x == y) || (!eq && x > y) {
                    1
                } else {
                    2
                }
            }
            _ => 3 | if a == A::AnyOrNull || b == A::AnyOrNull { 4 } else { 0 },
        }
    }
    fn lift(x: u8, y: u8, f: impl Fn(u8, u8) -> u8) -> u8 {
        let mut out = 0;
        for a in [1u8, 2, 4] {
            for b in [1u8, 2, 4] {
                if x & a != 0 && y & b != 0 {
                    out |= f(a, b);
                }
            }
        }
        out
    }
    fn go(b: &B, nulls: &[usize]) -> u8 {
        match b {
            B::Gt(x, y) => cmp(abs_s(x, nulls), abs_s(y, nulls), false),
            B::Eq(x, y) => cmp(abs_s(x, nulls), abs_s(y, nulls), true),
            B::IsNull(x) => match abs_s(x, nulls) {
                A::Null => 1,
                A::AnyOrNull => 3,
                _ => 2,
            },
            B::IsNotNull(x) => match abs_s(x, nulls) {
                A::Null => 2,
                A::AnyOrNull => 3,
                _ => 1,
            },
            B::And(x, y) => lift(go(x, nulls), go(y, nulls), |a, b| {
                if a == 2 || b == 2 {
                    2
                } else if a == 4 || b == 4 {
                    4
                } else {
                    1
                }
            }),
            B::Or(x, y) => lift(go(x, nulls), go(y, nulls), |a, b| {
                if a == 1 || b == 1 {
                    1
                } else if a == 4 || b == 4 {
                    4
                } else {
                    2
                }
            }),
            B::Not(x) => {
                let v = go(x, nulls);
                (if v & 1 != 0 { 2 } else { 0 }) | (if v & 2 != 0 { 1 } else { 0 }) | (v & 4)
            }
        }
    }
    go(cond, nulls) & 1 == 0
    //~ let _ = (cond, nulls);
    //~ false
    // @end
}
