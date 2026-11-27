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

/// An expression that passed the check, ready to run on rows: names are positions, and the type is known.
pub struct Compiled {
    node: Node,
    ty: Option<Type>,
}

enum Node {
    Lit(Value),
    Column(usize),
    Not(Box<Node>),
    Neg(Box<Node>),
    IsNull(Box<Node>, bool),
    Binary(Op, Box<Node>, Box<Node>),
}

/// `None` is the type of the literal NULL: it passes every check.
fn need(found: Option<Type>, needed: Type) -> Result<(), Fault> {
    match found {
        Some(found) if found != needed => Err(Fault::WrongType { needed, found }),
        _ => Ok(()),
    }
}

fn value_type(value: &Value) -> Option<Type> {
    match value {
        Value::Null => None,
        Value::Bool(_) => Some(Type::Bool),
        Value::Int(_) => Some(Type::Int),
        Value::Text(_) => Some(Type::Text),
    }
}

/// Checks `expr` against the columns a row will have. No row exists yet.
pub fn compile(expr: &Expr, columns: &[(String, Type)]) -> Result<Compiled, Fault> {
    let (node, ty) = match expr {
        Expr::Lit(value) => (Node::Lit(value.clone()), value_type(value)),
        Expr::Column(name) => {
            let at = column(columns, name)?;
            (Node::Column(at), Some(columns[at].1))
        }
        Expr::Not(of) => {
            let of = compile(of, columns)?;
            need(of.ty, Type::Bool)?;
            (Node::Not(Box::new(of.node)), Some(Type::Bool))
        }
        Expr::Neg(of) => {
            let of = compile(of, columns)?;
            need(of.ty, Type::Int)?;
            (Node::Neg(Box::new(of.node)), Some(Type::Int))
        }
        Expr::IsNull { of, negated } => {
            let of = compile(of, columns)?;
            (Node::IsNull(Box::new(of.node), *negated), Some(Type::Bool))
        }
        Expr::Binary(op, left, right) => {
            let (left, right) = (compile(left, columns)?, compile(right, columns)?);
            let ty = match op {
                Op::And | Op::Or => {
                    need(left.ty, Type::Bool)?;
                    need(right.ty, Type::Bool)?;
                    Type::Bool
                }
                Op::Add | Op::Sub | Op::Mul | Op::Div => {
                    need(left.ty, Type::Int)?;
                    need(right.ty, Type::Int)?;
                    Type::Int
                }
                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    if let Some(first) = left.ty {
                        need(right.ty, first)?;
                    }
                    Type::Bool
                }
            };
            let node = Node::Binary(*op, Box::new(left.node), Box::new(right.node));
            (node, Some(ty))
        }
    };
    Ok(Compiled { node, ty })
}

fn column(columns: &[(String, Type)], name: &str) -> Result<usize, Fault> {
    columns
        .iter()
        .position(|(column, _)| column == name)
        .ok_or_else(|| Fault::UnknownColumn(name.to_string()))
}

/// TRUE, FALSE, or `None` for NULL.
fn truth(value: Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(b),
        _ => None,
    }
}

fn from_truth(truth: Option<bool>) -> Value {
    truth.map_or(Value::Null, Value::Bool)
}

fn run(node: &Node, row: &[Value]) -> Result<Value, Fault> {
    Ok(match node {
        Node::Lit(value) => value.clone(),
        Node::Column(at) => row[*at].clone(),
        Node::Not(of) => from_truth(truth(run(of, row)?).map(|b| !b)),
        Node::Neg(of) => match run(of, row)? {
            Value::Int(n) => Value::Int(n.checked_neg().ok_or(Fault::Overflow)?),
            _ => Value::Null,
        },
        Node::IsNull(of, negated) => Value::Bool((run(of, row)? == Value::Null) != *negated),
        Node::Binary(op @ (Op::And | Op::Or), left, right) => {
            // the value that settles the answer by itself: FALSE for AND, TRUE for OR
            let settles = *op == Op::Or;
            let left = truth(run(left, row)?);
            if left == Some(settles) {
                return Ok(Value::Bool(settles));
            }
            let right = truth(run(right, row)?);
            from_truth(match (left, right) {
                (_, Some(b)) if b == settles => Some(settles),
                (Some(_), Some(_)) => Some(!settles),
                _ => None,
            })
        }
        Node::Binary(op, left, right) => {
            let (left, right) = (run(left, row)?, run(right, row)?);
            apply(*op, left, right)?
        }
    })
}

fn apply(op: Op, left: Value, right: Value) -> Result<Value, Fault> {
    if left == Value::Null || right == Value::Null {
        return Ok(Value::Null);
    }
    let order = left.cmp(&right);
    let result = match op {
        Op::Eq => order.is_eq(),
        Op::Ne => order.is_ne(),
        Op::Lt => order.is_lt(),
        Op::Le => order.is_le(),
        Op::Gt => order.is_gt(),
        Op::Ge => order.is_ge(),
        Op::And | Op::Or => return Ok(Value::Null),
        Op::Add | Op::Sub | Op::Mul | Op::Div => {
            let (Value::Int(a), Value::Int(b)) = (left, right) else {
                return Ok(Value::Null);
            };
            if op == Op::Div && b == 0 {
                return Err(Fault::DivisionByZero);
            }
            let n = match op {
                Op::Add => Some(a.wrapping_add(b)),
                Op::Sub => Some(a.wrapping_sub(b)),
                Op::Mul => Some(a.wrapping_mul(b)),
                _ => Some(a.wrapping_div(b)),
            };
            return n.map(Value::Int).ok_or(Fault::Overflow);
        }
    };
    Ok(Value::Bool(result))
}

impl Compiled {
    /// The type of every value this gives, apart from NULL. `None` if it is the literal NULL.
    pub fn ty(&self) -> Option<Type> {
        self.ty
    }

    /// The value for one row, which matches the columns given to `compile`.
    pub fn eval(&self, row: &[Value]) -> Result<Value, Fault> {
        run(&self.node, row)
    }
}
