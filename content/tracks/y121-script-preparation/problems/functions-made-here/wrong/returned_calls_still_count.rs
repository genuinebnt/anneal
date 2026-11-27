use std::cell::RefCell;
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

#[derive(Clone)]
enum Value {
    Number(i64),
    Function(Rc<Function>),
}

struct Function {
    parameters: Vec<String>,
    body: Rc<Expr>,
    scope: Scope,
}

/// A chain of variables, the newest first. A new variable never changes what an older chain means.
#[derive(Clone)]
struct Scope(Option<Rc<Variable>>);

struct Variable {
    name: String,
    value: RefCell<Value>,
    older: Scope,
}

impl Scope {
    fn with(&self, name: &str, value: Value) -> Scope {
        Scope(Some(Rc::new(Variable {
            name: name.to_string(),
            value: RefCell::new(value),
            older: self.clone(),
        })))
    }

    fn find(&self, name: &str) -> Result<&Variable, EvalError> {
        std::iter::successors(self.0.as_deref(), |v| v.older.0.as_deref())
            .find(|v| v.name == name)
            .ok_or_else(|| EvalError::Unknown(name.to_string()))
    }
}

fn number(value: Value) -> Result<i64, EvalError> {
    match value {
        Value::Number(n) => Ok(n),
        Value::Function(_) => Err(EvalError::NotANumber),
    }
}

fn evaluate(expr: &Expr, scope: &Scope, calls: usize) -> Result<Value, EvalError> {
    let go = |e: &Expr| evaluate(e, scope, calls);
    Ok(match expr {
        Expr::Num(n) => Value::Number(*n),
        Expr::Var(name) => scope.find(name)?.value.borrow().clone(),
        Expr::Add(a, b) => {
            let (a, b) = (go(a)?, go(b)?);
            Value::Number(number(a)?.wrapping_add(number(b)?))
        }
        Expr::Less(a, b) => {
            let (a, b) = (go(a)?, go(b)?);
            Value::Number(i64::from(number(a)? < number(b)?))
        }
        Expr::If(c, then, otherwise) => {
            if number(go(c)?)? != 0 {
                go(then)?
            } else {
                go(otherwise)?
            }
        }
        Expr::Let(name, value, body) => {
            let value = go(value)?;
            evaluate(body, &scope.with(name, value), calls)?
        }
        Expr::Set(name, value) => {
            let value = go(value)?;
            let variable = scope.find(name)?;
            let old = variable.value.replace(value.clone());
            drop(old);
            value
        }
        Expr::Then(a, b) => {
            go(a)?;
            go(b)?
        }
        Expr::Function(parameters, body) => Value::Function(Rc::new(Function {
            parameters: parameters.clone(),
            body: body.clone(),
            scope: scope.clone(),
        })),
        Expr::Recursive(name, parameters, body, rest) => {
            // The function's scope must contain the variable that will hold the function.
            let inner = scope.with(name, Value::Number(0));
            let function = Value::Function(Rc::new(Function {
                parameters: parameters.clone(),
                body: body.clone(),
                scope: inner.clone(),
            }));
            *inner.find(name)?.value.borrow_mut() = function;
            evaluate(rest, &inner, calls)?
        }
        Expr::Call(callee, arguments) => {
            let callee = go(callee)?;
            let values = arguments.iter().map(go).collect::<Result<Vec<_>, _>>()?;
            let Value::Function(function) = callee else {
                return Err(EvalError::NotAFunction);
            };
            if values.len() != function.parameters.len() {
                return Err(EvalError::WrongCount);
            }
            thread_local!(static TOTAL: std::cell::Cell<usize> = const { std::cell::Cell::new(0) });
            if calls == 0 {
                TOTAL.set(0);
            }
            TOTAL.set(TOTAL.get() + 1);
            if TOTAL.get() > MAX_CALLS {
                return Err(EvalError::TooDeep);
            }
            let mut inside = function.scope.clone();
            for (name, value) in function.parameters.iter().zip(values) {
                inside = inside.with(name, value);
            }
            evaluate(&function.body, &inside, calls + 1)?
        }
    })
}

/// Evaluates a whole program, which must give a number.
pub fn run(program: &Expr) -> Result<i64, EvalError> {
    number(evaluate(program, &Scope(None), 0)?)
}
