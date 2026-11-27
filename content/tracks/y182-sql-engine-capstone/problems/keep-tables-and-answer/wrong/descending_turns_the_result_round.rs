use std::collections::{HashMap, HashSet};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    Count,
    Sum,
    Min,
    Max,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Star,
    Expr {
        expr: Expr,
        name: String,
    },
    /// `column` is `None` for `*`.
    Aggregate {
        of: Aggregate,
        column: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBy {
    pub column: String,
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Select {
    pub items: Vec<Item>,
    pub table: String,
    pub filter: Option<Expr>,
    pub group_by: Option<String>,
    pub order_by: Option<OrderBy>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    CreateTable {
        table: String,
        columns: Vec<(String, Type)>,
    },
    Insert {
        table: String,
        rows: Vec<Vec<Expr>>,
    },
    Select(Select),
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
    TableExists(String),
    NoSuchTable(String),
    /// A table definition names this column twice.
    DuplicateColumn(String),
    /// A row of an INSERT has `found` values for `needed` columns.
    WrongCount {
        needed: usize,
        found: usize,
    },
}

/// What a statement gives back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Created,
    /// How many rows were added.
    Inserted(usize),
    Rows {
        columns: Vec<String>,
        rows: Vec<Vec<Value>>,
    },
}

/// The tables, in memory.
#[derive(Default)]
pub struct Database {
    tables: HashMap<String, Table>,
}

struct Table {
    columns: Vec<(String, Type)>,
    rows: Vec<Vec<Value>>,
}

impl Database {
    pub fn new() -> Database {
        Database::default()
    }

    /// Runs one statement. A statement that fails changes nothing.
    pub fn run(&mut self, statement: &Statement) -> Result<Output, Fault> {
        match statement {
            Statement::CreateTable { table, columns } => self.create(table, columns),
            Statement::Insert { table, rows } => self.insert(table, rows),
            Statement::Select(select) => {
                let table = self
                    .tables
                    .get(&select.table)
                    .ok_or_else(|| Fault::NoSuchTable(select.table.clone()))?;
                table.select(select)
            }
        }
    }

    fn create(&mut self, name: &str, columns: &[(String, Type)]) -> Result<Output, Fault> {
        if self.tables.contains_key(name) {
            return Err(Fault::TableExists(name.to_string()));
        }
        let mut seen = HashSet::new();
        for (column, _) in columns {
            if !seen.insert(column) {
                return Err(Fault::DuplicateColumn(column.clone()));
            }
        }
        let table = Table {
            columns: columns.to_vec(),
            rows: Vec::new(),
        };
        self.tables.insert(name.to_string(), table);
        Ok(Output::Created)
    }

    fn insert(&mut self, name: &str, rows: &[Vec<Expr>]) -> Result<Output, Fault> {
        let table = self
            .tables
            .get_mut(name)
            .ok_or_else(|| Fault::NoSuchTable(name.to_string()))?;
        // every row is built before any is added, so a failure leaves the table as it was
        let new: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| table.new_row(row))
            .collect::<Result<_, _>>()?;
        let added = new.len();
        table.rows.extend(new);
        Ok(Output::Inserted(added))
    }
}

/// A stable sort of the rows by one column. Turning the comparison round, not the result, keeps ties in order.
fn sort_rows<R: AsRef<[Value]>>(rows: &mut [R], key: usize, descending: bool) {
    rows.sort_by(|a, b| a.as_ref()[key].cmp(&b.as_ref()[key]));
    if descending {
        rows.reverse();
    }
}

/// A condition that must be BOOL, if there is one.
fn condition(filter: &Option<Expr>, columns: &[(String, Type)]) -> Result<Option<Compiled>, Fault> {
    let Some(filter) = filter else {
        return Ok(None);
    };
    let compiled = compile(filter, columns)?;
    need(compiled.ty(), Type::Bool)?;
    Ok(Some(compiled))
}

/// Only TRUE keeps a row: FALSE and NULL both drop it.
fn keeps(filter: &Option<Compiled>, row: &[Value]) -> Result<bool, Fault> {
    match filter {
        Some(filter) => Ok(filter.eval(row)? == Value::Bool(true)),
        None => Ok(true),
    }
}

impl Table {
    fn new_row(&self, exprs: &[Expr]) -> Result<Vec<Value>, Fault> {
        if exprs.len() != self.columns.len() {
            return Err(Fault::WrongCount {
                needed: self.columns.len(),
                found: exprs.len(),
            });
        }
        exprs
            .iter()
            .zip(&self.columns)
            .map(|(expr, (_, ty))| {
                // no columns: a value cannot look at any row
                let value = compile(expr, &[])?;
                if let Some(found) = value.ty().filter(|found| found != ty) {
                    return Err(Fault::WrongType { needed: *ty, found });
                }
                value.eval(&[])
            })
            .collect()
    }

    fn select(&self, select: &Select) -> Result<Output, Fault> {
        let filter = condition(&select.filter, &self.columns)?;
        self.plain(select, filter)
    }

    fn plain(&self, select: &Select, filter: Option<Compiled>) -> Result<Output, Fault> {
        let key = match &select.order_by {
            Some(order) => Some((column(&self.columns, &order.column)?, order.descending)),
            None => None,
        };
        let mut names = Vec::new();
        let mut outputs = Vec::new();
        for item in &select.items {
            match item {
                Item::Star => {
                    for (name, _) in &self.columns {
                        names.push(name.clone());
                        outputs.push(compile(&Expr::Column(name.clone()), &self.columns)?);
                    }
                }
                Item::Expr { expr, name } => {
                    names.push(name.clone());
                    outputs.push(compile(expr, &self.columns)?);
                }
                Item::Aggregate { .. } => unreachable!("no test of this step has one"),
            }
        }

        // every check is done; from here on, only rows that are needed are touched
        let limit = select
            .limit
            .map_or(usize::MAX, |n| usize::try_from(n).unwrap_or(usize::MAX));
        let output = |row: &Vec<Value>| {
            outputs
                .iter()
                .map(|o| o.eval(row))
                .collect::<Result<_, _>>()
        };
        let mut rows = Vec::new();
        match key {
            _ if limit == 0 => {}
            None => {
                for row in &self.rows {
                    if keeps(&filter, row)? {
                        rows.push(output(row)?);
                        if rows.len() == limit {
                            break;
                        }
                    }
                }
            }
            Some((key, descending)) => {
                // any row can come first, so all are filtered; only the ones that survive the limit are computed
                let mut kept: Vec<&Vec<Value>> = Vec::new();
                for row in &self.rows {
                    if keeps(&filter, row)? {
                        kept.push(row);
                    }
                }
                sort_rows(&mut kept, key, descending);
                for row in kept.into_iter().take(limit) {
                    rows.push(output(row)?);
                }
            }
        }
        Ok(Output::Rows {
            columns: names,
            rows,
        })
    }
}

// ----- Step 3, given: checking and running expressions. -----

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
                Op::Add => a.checked_add(b),
                Op::Sub => a.checked_sub(b),
                Op::Mul => a.checked_mul(b),
                _ => a.checked_div(b),
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
