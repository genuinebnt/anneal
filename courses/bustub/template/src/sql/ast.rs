//! The syntax tree the parser builds, as Rust enums. Identifiers are lower-cased unless they were quoted, as PostgreSQL does.

#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    CreateTable { name: String, columns: Vec<ColumnDef>, primary_key: Vec<String> },
    CreateIndex { name: String, table: String, columns: Vec<String>, using: Option<String> },
    Insert { table: String, columns: Option<Vec<String>>, query: Box<Query> },
    Update { table: String, assignments: Vec<(String, Expr)>, filter: Option<Expr>, has_from: bool },
    Delete { table: String, filter: Option<Expr> },
    Select(Box<Query>),
    /// `EXPLAIN [(option, ...)] statement`.
    Explain { options: Vec<String>, statement: Box<Statement> },
    Set { name: String, value: Expr },
    Show { name: String },
    /// `BEGIN [TRANSACTION] [ISOLATION LEVEL level]`: the level's words as written, lower-case and joined by one space.
    Begin { isolation: Option<String> },
    Commit,
    Rollback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    /// The type's name as written (`int`, `varchar`, ...), lower-case.
    pub type_name: String,
    /// The `(n)` after the type, if any.
    pub type_args: Vec<Expr>,
    pub primary_key: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub ctes: Vec<Cte>,
    pub body: QueryBody,
    pub order_by: Vec<OrderItem>,
    pub limit: Option<Expr>,
    pub offset: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cte {
    pub name: String,
    pub query: Query,
    pub recursive: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetOperator {
    Union,
    Intersect,
    Except,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueryBody {
    Select(Box<SelectCore>),
    Values(Vec<Vec<Expr>>),
    /// `left UNION [ALL] right`, also `INTERSECT` and `EXCEPT`. Each side is a whole query: a parenthesised one keeps its own ORDER BY and LIMIT.
    SetOp { op: SetOperator, all: bool, left: Box<Query>, right: Box<Query> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectCore {
    pub distinct: bool,
    pub distinct_on: bool,
    pub items: Vec<SelectItem>,
    pub from: Vec<TableRef>,
    pub filter: Option<Expr>,
    pub group_by: Vec<Expr>,
    pub having: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SelectItem {
    Wildcard,
    Expr { expr: Expr, alias: Option<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub enum TableRef {
    Named { name: String, alias: Option<String> },
    Subquery { query: Box<Query>, alias: Option<String> },
    Join { kind: JoinKind, left: Box<TableRef>, right: Box<TableRef>, on: Option<Expr> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinKind {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDir {
    Default,
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortNulls {
    Default,
    First,
    Last,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderItem {
    pub expr: Expr,
    pub dir: SortDir,
    pub nulls: SortNulls,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Integer(i64),
    /// A number that is not a 32-bit integer (`1.5`, `1e3`, `3000000000`), as written.
    Float(String),
    Str(String),
    Bool(bool),
    Null,
    /// `a`, `t.a`.
    Column(Vec<String>),
    Binary { op: String, left: Box<Expr>, right: Box<Expr> },
    Unary { op: String, expr: Box<Expr> },
    IsNull { expr: Box<Expr>, negated: bool },
    /// `CASE WHEN c THEN r ... [ELSE e] END`; the form `CASE x WHEN v THEN r` is rewritten by the parser to `CASE WHEN x = v THEN r`.
    Case { branches: Vec<(Expr, Expr)>, otherwise: Option<Box<Expr>> },
    /// The `i`th `?` of a statement, counted from 0 in the order they appear (a prepared statement's placeholder).
    Param(usize),
    Function { name: String, args: Vec<Expr>, distinct: bool, over: Option<Box<WindowSpec>> },
    /// `count(*)` is `Function { name: "count", args: [] }`; a bare `*` in an argument list is this.
    Star,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowSpec {
    pub partition_by: Vec<Expr>,
    pub order_by: Vec<OrderItem>,
    pub frame: Option<WindowFrame>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowFrame {
    pub range: bool,
    pub start: FrameBound,
    pub end: FrameBound,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FrameBound {
    UnboundedPreceding,
    UnboundedFollowing,
    CurrentRow,
    Preceding(Box<Expr>),
    Following(Box<Expr>),
}

// ---- visiting every expression of a statement -------------------------------------------------------------------------------

/// Calls `f` on every expression of the statements, and on each expression's children after `f` has run on it: select items, WHERE, GROUP BY,
/// HAVING, ORDER BY, LIMIT, OFFSET, JOIN ... ON, VALUES rows, assignments, filters, window specifications, subqueries and CTEs.
pub fn visit_exprs_mut(statements: &mut [Statement], f: &mut dyn FnMut(&mut Expr)) {
    fn expr(e: &mut Expr, f: &mut dyn FnMut(&mut Expr)) {
        f(e);
        match e {
            Expr::Binary { left, right, .. } => {
                expr(left, f);
                expr(right, f);
            }
            Expr::Unary { expr: inner, .. } | Expr::IsNull { expr: inner, .. } => expr(inner, f),
            Expr::Function { args, over, .. } => {
                args.iter_mut().for_each(|a| expr(a, f));
                if let Some(w) = over {
                    w.partition_by.iter_mut().for_each(|p| expr(p, f));
                    w.order_by.iter_mut().for_each(|o| expr(&mut o.expr, f));
                    if let Some(frame) = &mut w.frame {
                        for b in [&mut frame.start, &mut frame.end] {
                            if let FrameBound::Preceding(x) | FrameBound::Following(x) = b {
                                expr(x, f);
                            }
                        }
                    }
                }
            }
            Expr::Case { branches, otherwise } => {
                for (c, r) in branches {
                    expr(c, f);
                    expr(r, f);
                }
                if let Some(o) = otherwise {
                    expr(o, f);
                }
            }
            Expr::Integer(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null | Expr::Column(_) | Expr::Star | Expr::Param(_) => {}
        }
    }
    fn table_ref(t: &mut TableRef, f: &mut dyn FnMut(&mut Expr)) {
        match t {
            TableRef::Named { .. } => {}
            TableRef::Subquery { query: q, .. } => query(q, f),
            TableRef::Join { left, right, on, .. } => {
                table_ref(left, f);
                table_ref(right, f);
                if let Some(on) = on {
                    expr(on, f);
                }
            }
        }
    }
    fn query(q: &mut Query, f: &mut dyn FnMut(&mut Expr)) {
        for cte in &mut q.ctes {
            query(&mut cte.query, f);
        }
        match &mut q.body {
            QueryBody::Values(rows) => rows.iter_mut().flatten().for_each(|e| expr(e, f)),
            QueryBody::SetOp { left, right, .. } => {
                query(left, f);
                query(right, f);
            }
            QueryBody::Select(core) => {
                for item in &mut core.items {
                    if let SelectItem::Expr { expr: e, .. } = item {
                        expr(e, f);
                    }
                }
                core.from.iter_mut().for_each(|t| table_ref(t, f));
                if let Some(e) = &mut core.filter {
                    expr(e, f);
                }
                core.group_by.iter_mut().for_each(|e| expr(e, f));
                if let Some(e) = &mut core.having {
                    expr(e, f);
                }
            }
        }
        q.order_by.iter_mut().for_each(|o| expr(&mut o.expr, f));
        for e in [&mut q.limit, &mut q.offset].into_iter().flatten() {
            expr(e, f);
        }
    }
    fn statement(s: &mut Statement, f: &mut dyn FnMut(&mut Expr)) {
        match s {
            Statement::Insert { query: q, .. } => query(q, f),
            Statement::Update { assignments, filter, .. } => {
                assignments.iter_mut().for_each(|(_, e)| expr(e, f));
                if let Some(e) = filter {
                    expr(e, f);
                }
            }
            Statement::Delete { filter, .. } => {
                if let Some(e) = filter {
                    expr(e, f);
                }
            }
            Statement::Select(q) => query(q, f),
            Statement::Explain { statement: inner, .. } => statement(inner, f),
            Statement::Set { value, .. } => expr(value, f),
            Statement::CreateTable { .. } | Statement::CreateIndex { .. } | Statement::Show { .. } | Statement::Begin { .. } | Statement::Commit | Statement::Rollback => {}
        }
    }
    statements.iter_mut().for_each(|s| statement(s, f));
}
