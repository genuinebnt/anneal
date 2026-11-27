use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Expr {
    Num(i64),
    Var(String),
    Add(Box<Expr>, Box<Expr>),
    /// 1 if the first is less than the second, else 0.
    Less(Box<Expr>, Box<Expr>),
    /// The second if the first is not 0, else the third.
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `let name = value; body`
    Let(String, Box<Expr>, Box<Expr>),
    /// Gives an existing variable a new value, which is also the result.
    Set(String, Box<Expr>),
    /// The first, then the second. The result is the second's.
    Then(Box<Expr>, Box<Expr>),
    /// A new function: parameter names and body.
    Function(Vec<String>, Rc<Expr>),
    /// `name` is a function that can call itself by that name. It exists in its own body and in the last expression.
    Recursive(String, Vec<String>, Rc<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    Unknown(String),
    NotANumber,
    NotAFunction,
    WrongCount,
    TooDeep,
}

/// Calls that may be active at once.
pub const MAX_CALLS: usize = 200;

/// Evaluates a whole program, which must give a number.
pub fn run(program: &Expr) -> Result<i64, EvalError> {
    todo!()
}
