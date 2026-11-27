#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int,
    Text,
    Bool,
}

/// An operator with two operands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Lit(Value),
    /// A column, in small letters.
    Column(String),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    /// `x IS NULL`, or `x IS NOT NULL` when `negated`.
    IsNull {
        of: Box<Expr>,
        negated: bool,
    },
    Binary(Op, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    UnknownColumn(String),
    /// A part has the type `found` where only `needed` will do.
    WrongType {
        needed: Type,
        found: Type,
    },
    Overflow,
    DivisionByZero,
}

/// An expression that passed the check, ready to run on rows. Its insides are yours.
pub struct Compiled {}

/// Checks `expr` against the columns a row will have. No row exists yet.
pub fn compile(expr: &Expr, columns: &[(String, Type)]) -> Result<Compiled, Fault> {
    let _ = (expr, columns);
    todo!()
}

impl Compiled {
    /// The type of every value this gives, apart from NULL. `None` if it is the literal NULL.
    pub fn ty(&self) -> Option<Type> {
        todo!()
    }

    /// The value for one row, which matches the columns given to `compile`.
    pub fn eval(&self, row: &[Value]) -> Result<Value, Fault> {
        let _ = row;
        todo!()
    }
}
