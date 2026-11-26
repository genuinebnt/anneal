//! CAPSTONE 02 (database): a tiny in-memory SQL engine: lex -> Pratt parse -> AST -> bind/plan -> volcano executor.
//! LEVEL: capstone
//! Use in: embedded query layers, admin consoles, test doubles for a database, learning how SQL engines are shaped.   Run: cargo run --release --example tiny_sql_engine
#![allow(dead_code)]
//!
//! WHAT: CREATE TABLE / INSERT / SELECT (WHERE, GROUP BY with COUNT/SUM/MIN/MAX, ORDER BY, LIMIT) over typed INT/TEXT/BOOL columns.
//! WHY: every database is this pipeline at a different scale; seeing it in ~450 lines makes DataFusion or SQLite readable.
//! HOW (each stage is a plain function you can test alone):
//!   lex      bytes -> tokens carrying byte spans         parse   tokens -> AST (recursive descent + Pratt for expressions)
//!   bind     AST expr -> `Bound` expr (column names become indices, static type check, so type errors surface BEFORE any row is read)
//!   plan     Select -> a tree of operators:  Scan -> Filter -> Sort -> Limit -> Project   (or ... Filter -> Aggregate -> Sort -> Limit)
//!   execute  volcano model: every operator implements `next() -> Result<Option<Row>>` and pulls from its child, so LIMIT stops the scan early.
//! NULL SEMANTICS (three-valued logic): any comparison or arithmetic with NULL is NULL; `NULL AND false` = false, `NULL AND true` = NULL,
//!   `NULL OR true` = true, `NULL OR false` = NULL, `NOT NULL` = NULL; WHERE keeps a row only when the predicate is exactly TRUE.
//!   NULLs sort first in ASC order (last in DESC); aggregates ignore NULLs; SUM/MIN/MAX of nothing is NULL, COUNT of nothing is 0.
//! ERRORS are values: parse errors carry a span and render compiler-style with a caret line; type, catalog and runtime
//!   (division by zero, overflow) errors are `SqlError` variants. Nothing in the engine panics on user input.
//! VERIFICATION: hand-checked results on a small table, then seeded random tables/predicates compared with an independent
//!   brute-force Rust oracle (sortedness, LIMIT = min(n, rows), WHERE = closure filter, GROUP BY = BTreeMap fold).
//! COMMON USES: query layers over in-memory data (DataFusion, DuckDB-style engines), SQLite-like embedded stores, log/CSV query tools,
//!   admin-console filters, rule engines, test doubles for databases; patterns: Pratt parsing, volcano iterators, bind-then-execute, differential testing.
//! Read: https://github.com/apache/datafusion and https://15445.courses.cs.cmu.edu/ and https://craftinginterpreters.com/
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use thiserror::Error;

// ---------- values, types, errors ----------
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
}

/// Variant order matters: the derived `Ord` makes NULL < BOOL < INT < TEXT, which gives "NULLs first" for free.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Text(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ty {
    Int,
    Text,
    Bool,
}

impl Value {
    fn ty(&self) -> Option<Ty> {
        match self {
            Value::Null => None,
            Value::Bool(_) => Some(Ty::Bool),
            Value::Int(_) => Some(Ty::Int),
            Value::Text(_) => Some(Ty::Text),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Null => write!(f, "NULL"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Text(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Debug, Error, PartialEq)]
enum SqlError {
    #[error("syntax error: {msg}")]
    Parse { msg: String, span: Span },
    #[error("type error: {0}")]
    Type(String),
    #[error("catalog error: {0}")]
    Catalog(String),
    #[error("runtime error: {0}")]
    Runtime(String),
}
type Res<T> = Result<T, SqlError>;
type Row = Vec<Value>;

fn type_err<T>(msg: impl Into<String>) -> Res<T> {
    Err(SqlError::Type(msg.into()))
}
fn overflow() -> SqlError {
    SqlError::Runtime("integer overflow".into())
}

/// Compiler-style rendering: message, location, the source line and a caret underline.
fn render_error(src: &str, err: &SqlError) -> String {
    let SqlError::Parse { msg, span } = err else {
        return format!("error: {err}");
    };
    let start = span.start.min(src.len());
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[start..].find('\n').map_or(src.len(), |i| start + i);
    let line_no = src[..start].matches('\n').count() + 1;
    let col = src[line_start..start].chars().count();
    let width = src[start..span.end.min(line_end).max(start)]
        .chars()
        .count()
        .max(1);
    let gutter = " ".repeat(line_no.to_string().len());
    format!(
        "error: {msg}\n{gutter}--> line {line_no}, col {}\n{gutter} |\n{line_no} | {}\n{gutter} | {}{}",
        col + 1,
        &src[line_start..line_end],
        " ".repeat(col),
        "^".repeat(width)
    )
}

// ---------- lexer ----------
#[derive(Debug, Clone, PartialEq)]
enum Tk {
    Ident(String), // lower-cased: SQL identifiers and keywords are case-insensitive
    Int(i64),
    Str(String),
    Sym(&'static str),
}
#[derive(Debug, Clone)]
struct Token {
    tk: Tk,
    span: Span,
}

fn parse_err<T>(msg: impl Into<String>, start: usize, end: usize) -> Res<T> {
    Err(SqlError::Parse {
        msg: msg.into(),
        span: Span { start, end },
    })
}

fn lex(src: &str) -> Res<Vec<Token>> {
    const SYMS: [&str; 15] = [
        "<=", ">=", "<>", "!=", "(", ")", ",", "*", "+", "-", "/", "=", "<", ">", ";",
    ];
    let b = src.as_bytes();
    let (mut i, mut out) = (0, vec![]);
    while i < b.len() {
        let start = i;
        let tk = if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        } else if b[i].is_ascii_digit() {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            match src[start..i].parse() {
                Ok(n) => Tk::Int(n),
                Err(_) => return parse_err("integer literal out of range", start, i),
            }
        } else if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            Tk::Ident(src[start..i].to_ascii_lowercase())
        } else if b[i] == b'\'' {
            i += 1;
            let mut s = String::new();
            loop {
                let Some(ch) = src[i..].chars().next() else {
                    return parse_err("unterminated string literal", start, src.len());
                };
                i += ch.len_utf8();
                if ch != '\'' {
                    s.push(ch);
                } else if b.get(i) == Some(&b'\'') {
                    s.push('\''); // '' is an escaped quote
                    i += 1;
                } else {
                    break;
                }
            }
            Tk::Str(s)
        } else if let Some(sym) = SYMS.iter().find(|s| src[i..].starts_with(**s)) {
            i += sym.len();
            Tk::Sym(if *sym == "!=" { "<>" } else { sym })
        } else {
            let ch = src[i..].chars().next().map_or(1, char::len_utf8);
            return parse_err("unexpected character", start, start + ch);
        };
        out.push(Token {
            tk,
            span: Span { start, end: i },
        });
    }
    Ok(out)
}

// ---------- AST ----------
#[derive(Debug, Clone, Copy, PartialEq)]
enum BinOp {
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
#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Lit(Value),
    Col(String),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    IsNull(Box<Expr>, bool), // bool: negated (IS NOT NULL)
    Bin(BinOp, Box<Expr>, Box<Expr>),
}
#[derive(Debug, Clone, Copy, PartialEq)]
enum AggFn {
    Count,
    Sum,
    Min,
    Max,
}
impl AggFn {
    fn from_name(s: &str) -> Option<AggFn> {
        Some(match s {
            "count" => AggFn::Count,
            "sum" => AggFn::Sum,
            "min" => AggFn::Min,
            "max" => AggFn::Max,
            _ => return None,
        })
    }
}
#[derive(Debug, Clone)]
enum Item {
    Star,
    Expr(Expr, String),
    Agg {
        f: AggFn,
        col: Option<String>,
        name: String,
    }, // col None = COUNT(*)
}
#[derive(Debug, Clone)]
struct Select {
    items: Vec<Item>,
    table: String,
    filter: Option<Expr>,
    group_by: Option<String>,
    order_by: Option<(String, bool)>, // (column, descending)
    limit: Option<usize>,
}
#[derive(Debug, Clone)]
enum Stmt {
    Create {
        name: String,
        cols: Vec<(String, Ty)>,
    },
    Insert {
        table: String,
        rows: Vec<Vec<Expr>>,
    },
    Select(Select),
}

// ---------- parser ----------
const RESERVED: [&str; 21] = [
    "select", "from", "where", "order", "by", "limit", "group", "and", "or", "not", "insert",
    "into", "values", "create", "table", "asc", "desc", "is", "null", "true", "false",
];

struct Parser<'a> {
    src: &'a str,
    toks: Vec<Token>,
    pos: usize,
}

impl Parser<'_> {
    fn peek_tk(&self, n: usize) -> Option<&Tk> {
        self.toks.get(self.pos + n).map(|t| &t.tk)
    }
    /// Errors point at the offending token, or at end-of-input when the statement stops too early.
    fn fail<T>(&self, msg: impl Into<String>) -> Res<T> {
        match self.toks.get(self.pos) {
            Some(t) => parse_err(msg, t.span.start, t.span.end),
            None => parse_err(msg, self.src.len(), self.src.len()),
        }
    }
    fn at_kw(&self, kw: &str) -> bool {
        matches!(self.peek_tk(0), Some(Tk::Ident(s)) if s == kw)
    }
    fn eat_kw(&mut self, kw: &str) -> bool {
        let hit = self.at_kw(kw);
        self.pos += usize::from(hit);
        hit
    }
    fn expect_kw(&mut self, kw: &str) -> Res<()> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            self.fail(format!("expected {}", kw.to_uppercase()))
        }
    }
    fn eat_sym(&mut self, s: &str) -> bool {
        let hit = matches!(self.peek_tk(0), Some(Tk::Sym(x)) if *x == s);
        self.pos += usize::from(hit);
        hit
    }
    fn expect_sym(&mut self, s: &str) -> Res<()> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            self.fail(format!("expected `{s}`"))
        }
    }
    fn ident(&mut self, what: &str) -> Res<String> {
        if let Some(Tk::Ident(s)) = self.peek_tk(0) {
            if !RESERVED.contains(&s.as_str()) {
                let s = s.clone();
                self.pos += 1;
                return Ok(s);
            }
        }
        self.fail(format!("expected {what}"))
    }
    fn ty(&mut self) -> Res<Ty> {
        let ty = match self.peek_tk(0) {
            Some(Tk::Ident(s)) if s == "int" => Ty::Int,
            Some(Tk::Ident(s)) if s == "text" => Ty::Text,
            Some(Tk::Ident(s)) if s == "bool" => Ty::Bool,
            _ => return self.fail("expected a column type (INT, TEXT or BOOL)"),
        };
        self.pos += 1;
        Ok(ty)
    }

    fn create(&mut self) -> Res<Stmt> {
        self.expect_kw("table")?;
        let name = self.ident("a table name")?;
        self.expect_sym("(")?;
        let mut cols = vec![];
        loop {
            let col = self.ident("a column name")?;
            cols.push((col, self.ty()?));
            if !self.eat_sym(",") {
                break;
            }
        }
        self.expect_sym(")")?;
        Ok(Stmt::Create { name, cols })
    }

    fn insert(&mut self) -> Res<Stmt> {
        self.expect_kw("into")?;
        let table = self.ident("a table name")?;
        self.expect_kw("values")?;
        let mut rows = vec![];
        loop {
            self.expect_sym("(")?;
            let mut row = vec![self.expr(0)?];
            while self.eat_sym(",") {
                row.push(self.expr(0)?);
            }
            self.expect_sym(")")?;
            rows.push(row);
            if !self.eat_sym(",") {
                break;
            }
        }
        Ok(Stmt::Insert { table, rows })
    }

    fn item(&mut self) -> Res<Item> {
        if self.eat_sym("*") {
            return Ok(Item::Star);
        }
        if let (Some(Tk::Ident(f)), Some(Tk::Sym("("))) = (self.peek_tk(0), self.peek_tk(1)) {
            if let Some(f) = AggFn::from_name(f) {
                self.pos += 2;
                let col = if self.eat_sym("*") {
                    None
                } else {
                    Some(self.ident("a column name or *")?)
                };
                self.expect_sym(")")?;
                let name = format!("{f:?}({})", col.as_deref().unwrap_or("*")).to_lowercase();
                return Ok(Item::Agg { f, col, name });
            }
        }
        let first = self.pos;
        let e = self.expr(0)?;
        let name = match &e {
            Expr::Col(c) => c.clone(),
            _ => {
                self.src[self.toks[first].span.start..self.toks[self.pos - 1].span.end].to_string()
            }
        };
        Ok(Item::Expr(e, name))
    }

    fn select(&mut self) -> Res<Stmt> {
        let mut items = vec![self.item()?];
        while self.eat_sym(",") {
            items.push(self.item()?);
        }
        self.expect_kw("from")?;
        let table = self.ident("a table name")?;
        let filter = if self.eat_kw("where") {
            Some(self.expr(0)?)
        } else {
            None
        };
        let group_by = if self.eat_kw("group") {
            self.expect_kw("by")?;
            Some(self.ident("a column name")?)
        } else {
            None
        };
        let order_by = if self.eat_kw("order") {
            self.expect_kw("by")?;
            let col = self.ident("a column name")?;
            let desc = self.eat_kw("desc");
            self.eat_kw("asc");
            Some((col, desc))
        } else {
            None
        };
        let limit = if self.eat_kw("limit") {
            let Some(Tk::Int(n)) = self.peek_tk(0) else {
                return self.fail("expected a row count");
            };
            let n = *n as usize; // the lexer only produces non-negative integers
            self.pos += 1;
            Some(n)
        } else {
            None
        };
        Ok(Stmt::Select(Select {
            items,
            table,
            filter,
            group_by,
            order_by,
            limit,
        }))
    }

    /// Binding powers (higher binds tighter): OR 1, AND 2, NOT 3, comparison/IS 4, + - 5, * / 6, unary minus 7.
    fn expr(&mut self, min_bp: u8) -> Res<Expr> {
        let mut lhs = self.prefix()?;
        loop {
            if self.at_kw("is") {
                if 4 < min_bp {
                    break;
                }
                self.pos += 1;
                let negated = self.eat_kw("not");
                self.expect_kw("null")?;
                lhs = Expr::IsNull(Box::new(lhs), negated);
                continue;
            }
            let Some((op, bp)) = self.peek_tk(0).and_then(infix) else {
                break;
            };
            if bp < min_bp {
                break;
            }
            self.pos += 1;
            let rhs = self.expr(bp + 1)?; // +1 makes every operator left-associative
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn prefix(&mut self) -> Res<Expr> {
        let Some(tk) = self.peek_tk(0).cloned() else {
            return self.fail("expected an expression");
        };
        let e = match tk {
            Tk::Int(n) => Expr::Lit(Value::Int(n)),
            Tk::Str(s) => Expr::Lit(Value::Text(s)),
            Tk::Sym("(") => {
                self.pos += 1;
                let e = self.expr(0)?;
                return self.expect_sym(")").map(|_| e);
            }
            Tk::Sym("-") => {
                self.pos += 1;
                return Ok(Expr::Neg(Box::new(self.expr(7)?)));
            }
            Tk::Ident(w) => match w.as_str() {
                "not" => {
                    self.pos += 1;
                    return Ok(Expr::Not(Box::new(self.expr(3)?)));
                }
                "true" => Expr::Lit(Value::Bool(true)),
                "false" => Expr::Lit(Value::Bool(false)),
                "null" => Expr::Lit(Value::Null),
                _ => return self.ident("an expression").map(Expr::Col),
            },
            _ => return self.fail("expected an expression"),
        };
        self.pos += 1;
        Ok(e)
    }
}

fn infix(tk: &Tk) -> Option<(BinOp, u8)> {
    Some(match tk {
        Tk::Ident(s) if s == "or" => (BinOp::Or, 1),
        Tk::Ident(s) if s == "and" => (BinOp::And, 2),
        Tk::Sym("=") => (BinOp::Eq, 4),
        Tk::Sym("<>") => (BinOp::Ne, 4),
        Tk::Sym("<") => (BinOp::Lt, 4),
        Tk::Sym("<=") => (BinOp::Le, 4),
        Tk::Sym(">") => (BinOp::Gt, 4),
        Tk::Sym(">=") => (BinOp::Ge, 4),
        Tk::Sym("+") => (BinOp::Add, 5),
        Tk::Sym("-") => (BinOp::Sub, 5),
        Tk::Sym("*") => (BinOp::Mul, 6),
        Tk::Sym("/") => (BinOp::Div, 6),
        _ => return None,
    })
}

fn parse(src: &str) -> Res<Stmt> {
    let mut p = Parser {
        src,
        toks: lex(src)?,
        pos: 0,
    };
    let stmt = if p.eat_kw("create") {
        p.create()?
    } else if p.eat_kw("insert") {
        p.insert()?
    } else if p.eat_kw("select") {
        p.select()?
    } else {
        return p.fail("expected CREATE, INSERT or SELECT");
    };
    p.eat_sym(";");
    if p.pos < p.toks.len() {
        return p.fail("unexpected trailing input");
    }
    Ok(stmt)
}

// ---------- binding (name resolution + static typing) and evaluation ----------
type Schema = [(String, Ty)];

#[derive(Debug, Clone)]
enum Bound {
    Lit(Value),
    Col(usize),
    Not(Box<Bound>),
    Neg(Box<Bound>),
    IsNull(Box<Bound>, bool),
    Bin(BinOp, Box<Bound>, Box<Bound>),
}

fn col_index(schema: &Schema, name: &str) -> Res<usize> {
    schema
        .iter()
        .position(|(n, _)| n == name)
        .ok_or_else(|| SqlError::Catalog(format!("unknown column `{name}`")))
}

/// `None` as a type means "the NULL literal": it is compatible with everything.
fn expect_ty(got: Option<Ty>, want: Ty, ctx: &str) -> Res<()> {
    match got {
        None => Ok(()),
        Some(t) if t == want => Ok(()),
        Some(t) => type_err(format!("{ctx} must be {want:?}, found {t:?}")),
    }
}

fn bind(e: &Expr, schema: &Schema) -> Res<(Bound, Option<Ty>)> {
    Ok(match e {
        Expr::Lit(v) => (Bound::Lit(v.clone()), v.ty()),
        Expr::Col(c) => {
            let i = col_index(schema, c)?;
            (Bound::Col(i), Some(schema[i].1))
        }
        Expr::Not(x) => {
            let (b, t) = bind(x, schema)?;
            expect_ty(t, Ty::Bool, "operand of NOT")?;
            (Bound::Not(b.into()), Some(Ty::Bool))
        }
        Expr::Neg(x) => {
            let (b, t) = bind(x, schema)?;
            expect_ty(t, Ty::Int, "operand of unary -")?;
            (Bound::Neg(b.into()), Some(Ty::Int))
        }
        Expr::IsNull(x, neg) => (
            Bound::IsNull(bind(x, schema)?.0.into(), *neg),
            Some(Ty::Bool),
        ),
        Expr::Bin(op, l, r) => {
            let ((lb, lt), (rb, rt)) = (bind(l, schema)?, bind(r, schema)?);
            let out = match op {
                BinOp::And | BinOp::Or => {
                    expect_ty(lt, Ty::Bool, "left operand of AND/OR")?;
                    expect_ty(rt, Ty::Bool, "right operand of AND/OR")?;
                    Ty::Bool
                }
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    expect_ty(lt, Ty::Int, "left operand of arithmetic")?;
                    expect_ty(rt, Ty::Int, "right operand of arithmetic")?;
                    Ty::Int
                }
                _ => {
                    if let (Some(a), Some(b)) = (lt, rt) {
                        if a != b {
                            return type_err(format!("cannot compare {a:?} with {b:?}"));
                        }
                    }
                    Ty::Bool
                }
            };
            (Bound::Bin(*op, lb.into(), rb.into()), Some(out))
        }
    })
}

fn tri(v: &Value) -> Option<bool> {
    if let Value::Bool(b) = v {
        Some(*b)
    } else {
        None
    }
}
fn from_tri(b: Option<bool>) -> Value {
    b.map_or(Value::Null, Value::Bool)
}

fn apply(op: BinOp, l: Value, r: Value) -> Res<Value> {
    // AND/OR must see NULL operands, so they are handled before the generic "NULL in, NULL out" rule.
    match op {
        BinOp::And => {
            return Ok(from_tri(match (tri(&l), tri(&r)) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            }));
        }
        BinOp::Or => {
            return Ok(from_tri(match (tri(&l), tri(&r)) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            }));
        }
        _ => {}
    }
    if l == Value::Null || r == Value::Null {
        return Ok(Value::Null);
    }
    if let (Value::Int(a), Value::Int(b)) = (&l, &r) {
        let (a, b) = (*a, *b);
        let arith = match op {
            BinOp::Add => Some(a.checked_add(b)),
            BinOp::Sub => Some(a.checked_sub(b)),
            BinOp::Mul => Some(a.checked_mul(b)),
            BinOp::Div if b == 0 => return Err(SqlError::Runtime("division by zero".into())),
            BinOp::Div => Some(a.checked_div(b)),
            _ => None,
        };
        if let Some(v) = arith {
            return v.map(Value::Int).ok_or_else(overflow);
        }
    }
    if l.ty() != r.ty() {
        return type_err("operands have different types"); // unreachable after binding; kept as a value, not a panic
    }
    let o = l.cmp(&r);
    Ok(Value::Bool(match op {
        BinOp::Eq => o.is_eq(),
        BinOp::Ne => o.is_ne(),
        BinOp::Lt => o.is_lt(),
        BinOp::Le => o.is_le(),
        BinOp::Gt => o.is_gt(),
        BinOp::Ge => o.is_ge(),
        _ => return type_err("arithmetic on non-integers"),
    }))
}

fn eval(b: &Bound, row: &[Value]) -> Res<Value> {
    Ok(match b {
        Bound::Lit(v) => v.clone(),
        Bound::Col(i) => row
            .get(*i)
            .cloned()
            .ok_or_else(|| SqlError::Runtime("column index out of range".into()))?,
        Bound::Not(x) => from_tri(tri(&eval(x, row)?).map(|v| !v)),
        Bound::Neg(x) => match eval(x, row)? {
            Value::Int(i) => Value::Int(i.checked_neg().ok_or_else(overflow)?),
            _ => Value::Null,
        },
        Bound::IsNull(x, neg) => Value::Bool((eval(x, row)? == Value::Null) != *neg),
        Bound::Bin(op, l, r) => apply(*op, eval(l, row)?, eval(r, row)?)?,
    })
}

// ---------- volcano operators ----------
trait Operator {
    fn next(&mut self) -> Res<Option<Row>>;
}

struct Scan {
    rows: std::vec::IntoIter<Row>,
}
impl Operator for Scan {
    fn next(&mut self) -> Res<Option<Row>> {
        Ok(self.rows.next())
    }
}

struct Filter {
    child: Box<dyn Operator>,
    pred: Bound,
}
impl Operator for Filter {
    fn next(&mut self) -> Res<Option<Row>> {
        while let Some(row) = self.child.next()? {
            if eval(&self.pred, &row)? == Value::Bool(true) {
                return Ok(Some(row)); // NULL and false both reject the row
            }
        }
        Ok(None)
    }
}

/// A blocking operator: it must see every input row before it can emit the first one.
struct Sort {
    child: Box<dyn Operator>,
    key: usize,
    desc: bool,
    sorted: Option<std::vec::IntoIter<Row>>,
}
impl Operator for Sort {
    fn next(&mut self) -> Res<Option<Row>> {
        if self.sorted.is_none() {
            let mut all = vec![];
            while let Some(r) = self.child.next()? {
                all.push(r);
            }
            // sort_by is stable, and DESC reverses only the comparison, so ties keep their input order either way.
            all.sort_by(|a, b| {
                if self.desc {
                    b[self.key].cmp(&a[self.key])
                } else {
                    a[self.key].cmp(&b[self.key])
                }
            });
            self.sorted = Some(all.into_iter());
        }
        Ok(self.sorted.as_mut().and_then(Iterator::next))
    }
}

struct Limit {
    child: Box<dyn Operator>,
    left: usize,
}
impl Operator for Limit {
    fn next(&mut self) -> Res<Option<Row>> {
        if self.left == 0 {
            return Ok(None); // do not pull from the child: this is what makes LIMIT cheap on a lazy pipeline
        }
        self.left -= 1;
        self.child.next()
    }
}

struct Project {
    child: Box<dyn Operator>,
    exprs: Vec<Bound>,
}
impl Operator for Project {
    fn next(&mut self) -> Res<Option<Row>> {
        let Some(row) = self.child.next()? else {
            return Ok(None);
        };
        self.exprs
            .iter()
            .map(|e| eval(e, &row))
            .collect::<Res<Row>>()
            .map(Some)
    }
}

#[derive(Clone, Default)]
struct Acc {
    n: i64,
    sum: Option<i64>,
    ext: Option<Value>,
}
impl Acc {
    fn add(&mut self, f: AggFn, v: Option<&Value>) -> Res<()> {
        let Some(v) = v else {
            self.n += 1; // COUNT(*) counts rows, NULLs included
            return Ok(());
        };
        if *v == Value::Null {
            return Ok(()); // every other aggregate skips NULL inputs
        }
        self.n += 1;
        match (f, v) {
            (AggFn::Sum, Value::Int(i)) => {
                self.sum = Some(self.sum.unwrap_or(0).checked_add(*i).ok_or_else(overflow)?)
            }
            (AggFn::Min, x) if self.ext.as_ref().is_none_or(|e| x < e) => {
                self.ext = Some(x.clone())
            }
            (AggFn::Max, x) if self.ext.as_ref().is_none_or(|e| x > e) => {
                self.ext = Some(x.clone())
            }
            _ => {}
        }
        Ok(())
    }
    fn finish(&self, f: AggFn) -> Value {
        match f {
            AggFn::Count => Value::Int(self.n),
            AggFn::Sum => self.sum.map_or(Value::Null, Value::Int),
            AggFn::Min | AggFn::Max => self.ext.clone().unwrap_or(Value::Null),
        }
    }
}

enum Out {
    Group,
    Agg(usize),
}
struct Aggregate {
    child: Box<dyn Operator>,
    group: Option<usize>,
    aggs: Vec<(AggFn, Option<usize>)>, // (function, input column; None = COUNT(*))
    layout: Vec<Out>,
    out: Option<std::vec::IntoIter<Row>>,
}
impl Operator for Aggregate {
    fn next(&mut self) -> Res<Option<Row>> {
        if self.out.is_none() {
            // BTreeMap gives deterministic group order (NULL group first) without extra sorting code.
            let mut groups: BTreeMap<Value, Vec<Acc>> = BTreeMap::new();
            while let Some(row) = self.child.next()? {
                let key = self.group.map_or(Value::Null, |g| row[g].clone());
                let accs = groups
                    .entry(key)
                    .or_insert_with(|| vec![Acc::default(); self.aggs.len()]);
                for (acc, (f, col)) in accs.iter_mut().zip(&self.aggs) {
                    acc.add(*f, col.map(|c| &row[c]))?;
                }
            }
            if self.group.is_none() && groups.is_empty() {
                groups.insert(Value::Null, vec![Acc::default(); self.aggs.len()]); // SELECT COUNT(*) on no rows still yields one row
            }
            let rows: Vec<Row> = groups
                .iter()
                .map(|(key, accs)| {
                    self.layout
                        .iter()
                        .map(|o| match o {
                            Out::Group => key.clone(),
                            Out::Agg(i) => accs[*i].finish(self.aggs[*i].0),
                        })
                        .collect()
                })
                .collect();
            self.out = Some(rows.into_iter());
        }
        Ok(self.out.as_mut().and_then(Iterator::next))
    }
}

// ---------- planner ----------
struct Table {
    cols: Vec<(String, Ty)>,
    rows: Vec<Row>,
}

fn plan(sel: &Select, tables: &HashMap<String, Table>) -> Res<(Box<dyn Operator>, Vec<String>)> {
    let t = tables
        .get(&sel.table)
        .ok_or_else(|| SqlError::Catalog(format!("no such table `{}`", sel.table)))?;
    let mut node: Box<dyn Operator> = Box::new(Scan {
        rows: t.rows.clone().into_iter(),
    });
    if let Some(f) = &sel.filter {
        let (pred, ty) = bind(f, &t.cols)?;
        expect_ty(ty, Ty::Bool, "WHERE clause")?;
        node = Box::new(Filter { child: node, pred });
    }
    let aggregated =
        sel.group_by.is_some() || sel.items.iter().any(|i| matches!(i, Item::Agg { .. }));
    if aggregated {
        plan_aggregate(sel, t, node)
    } else {
        plan_plain(sel, t, node)
    }
}

fn with_sort_limit(
    sel: &Select,
    key: Option<usize>,
    mut node: Box<dyn Operator>,
) -> Box<dyn Operator> {
    if let (Some(key), Some((_, desc))) = (key, &sel.order_by) {
        node = Box::new(Sort {
            child: node,
            key,
            desc: *desc,
            sorted: None,
        });
    }
    match sel.limit {
        Some(n) => Box::new(Limit {
            child: node,
            left: n,
        }),
        None => node,
    }
}

fn plan_plain(
    sel: &Select,
    t: &Table,
    node: Box<dyn Operator>,
) -> Res<(Box<dyn Operator>, Vec<String>)> {
    let key = sel
        .order_by
        .as_ref()
        .map(|(c, _)| col_index(&t.cols, c))
        .transpose()?;
    let node = with_sort_limit(sel, key, node); // sort BEFORE project so ORDER BY can use columns that are not selected
    let (mut exprs, mut names) = (vec![], vec![]);
    for item in &sel.items {
        match item {
            Item::Star => {
                for (i, (n, _)) in t.cols.iter().enumerate() {
                    exprs.push(Bound::Col(i));
                    names.push(n.clone());
                }
            }
            Item::Expr(e, name) => {
                exprs.push(bind(e, &t.cols)?.0);
                names.push(name.clone());
            }
            Item::Agg { .. } => return type_err("aggregate in a non-aggregate query"),
        }
    }
    Ok((Box::new(Project { child: node, exprs }), names))
}

fn plan_aggregate(
    sel: &Select,
    t: &Table,
    node: Box<dyn Operator>,
) -> Res<(Box<dyn Operator>, Vec<String>)> {
    let group = sel
        .group_by
        .as_ref()
        .map(|g| col_index(&t.cols, g))
        .transpose()?;
    let (mut aggs, mut layout, mut names) = (vec![], vec![], vec![]);
    for item in &sel.items {
        match item {
            Item::Star => return type_err("* cannot be combined with aggregates"),
            Item::Expr(Expr::Col(c), name) if sel.group_by.as_deref() == Some(c) => {
                layout.push(Out::Group);
                names.push(name.clone());
            }
            Item::Expr(..) => {
                return type_err("a non-aggregate select item must be the GROUP BY column");
            }
            Item::Agg { f, col, name } => {
                let idx = col.as_ref().map(|c| col_index(&t.cols, c)).transpose()?;
                match idx {
                    None if *f != AggFn::Count => return type_err("only COUNT accepts *"),
                    Some(i) if *f == AggFn::Sum && t.cols[i].1 != Ty::Int => {
                        return type_err("SUM needs an INT column");
                    }
                    _ => {}
                }
                layout.push(Out::Agg(aggs.len()));
                aggs.push((*f, idx));
                names.push(name.clone());
            }
        }
    }
    let node: Box<dyn Operator> = Box::new(Aggregate {
        child: node,
        group,
        aggs,
        layout,
        out: None,
    });
    // after aggregation the schema is the output names, so ORDER BY resolves against those
    let key = match &sel.order_by {
        Some((c, _)) => Some(
            names
                .iter()
                .position(|n| n == c)
                .ok_or_else(|| SqlError::Catalog(format!("unknown output column `{c}`")))?,
        ),
        None => None,
    };
    Ok((with_sort_limit(sel, key, node), names))
}

// ---------- database ----------
enum Output {
    Created,
    Inserted(usize),
    Rows { cols: Vec<String>, rows: Vec<Row> },
}

#[derive(Default)]
struct Database {
    tables: HashMap<String, Table>,
}

impl Database {
    fn execute(&mut self, sql: &str) -> Res<Output> {
        match parse(sql)? {
            Stmt::Create { name, cols } => {
                if self.tables.contains_key(&name) {
                    return Err(SqlError::Catalog(format!("table `{name}` already exists")));
                }
                self.tables.insert(name, Table { cols, rows: vec![] });
                Ok(Output::Created)
            }
            Stmt::Insert { table, rows } => {
                let t = self
                    .tables
                    .get_mut(&table)
                    .ok_or_else(|| SqlError::Catalog(format!("no such table `{table}`")))?;
                let mut staged = Vec::with_capacity(rows.len());
                for exprs in &rows {
                    if exprs.len() != t.cols.len() {
                        return Err(SqlError::Catalog(format!(
                            "expected {} values, got {}",
                            t.cols.len(),
                            exprs.len()
                        )));
                    }
                    let mut row = vec![];
                    for (e, (name, ty)) in exprs.iter().zip(&t.cols) {
                        let v = eval(&bind(e, &[])?.0, &[])?;
                        if v.ty().is_some_and(|vt| vt != *ty) {
                            return type_err(format!("column `{name}` is {ty:?}, got {v:?}"));
                        }
                        row.push(v);
                    }
                    staged.push(row);
                }
                let n = staged.len();
                t.rows.extend(staged); // all-or-nothing: nothing is appended unless every row validated
                Ok(Output::Inserted(n))
            }
            Stmt::Select(sel) => {
                let (mut op, cols) = plan(&sel, &self.tables)?;
                let mut rows = vec![];
                while let Some(r) = op.next()? {
                    rows.push(r);
                }
                Ok(Output::Rows { cols, rows })
            }
        }
    }

    fn query(&mut self, sql: &str) -> Res<Vec<Row>> {
        match self.execute(sql)? {
            Output::Rows { rows, .. } => Ok(rows),
            _ => type_err("statement does not return rows"),
        }
    }
}

fn render_table(cols: &[String], rows: &[Row]) -> String {
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| r.iter().map(Value::to_string).collect())
        .collect();
    let widths: Vec<usize> = (0..cols.len())
        .map(|i| {
            cells
                .iter()
                .map(|r| r[i].chars().count())
                .chain([cols[i].chars().count()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cs: &[String]| {
        cs.iter()
            .zip(&widths)
            .map(|(c, w)| format!("{c:<w$}"))
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let rule = widths
        .iter()
        .map(|w| "-".repeat(*w))
        .collect::<Vec<_>>()
        .join("-+-");
    let mut out = format!("{}\n{rule}\n", line(cols));
    for r in &cells {
        out += &format!("{}\n", line(r));
    }
    out
}

// ---------- tests: seeded RNG, oracle, properties ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn cells(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|r| r.iter().map(Value::to_string).collect::<Vec<_>>().join("|"))
        .collect()
}

fn hand_checked() {
    let mut db = Database::default();
    db.execute("CREATE TABLE emp (id INT, name TEXT, dept TEXT, salary INT, active BOOL)")
        .unwrap();
    let n = db
        .execute(
            "INSERT INTO emp VALUES (1,'ann','eng',100,true),(2,'bob','eng',80,true),(3,'cy','ops',70,false),(4,'di','ops',NULL,true),(5,'ed','hr',60,NULL)",
        )
        .unwrap();
    assert!(matches!(n, Output::Inserted(5)));
    let mut check = |sql: &str, want: &[&str]| {
        let got = cells(&db.query(sql).unwrap_or_else(|e| panic!("{sql}: {e}")));
        assert_eq!(got, want, "{sql}");
    };
    check("SELECT name FROM emp WHERE salary > 70", &["ann", "bob"]); // di's NULL salary is NULL > 70 = NULL: rejected
    check(
        "SELECT name FROM emp WHERE NOT (salary > 70)",
        &["cy", "ed"],
    ); // NOT NULL is still NULL
    check(
        "SELECT name FROM emp WHERE salary > 70 OR active",
        &["ann", "bob", "di"],
    ); // NULL OR true = true
    check(
        "SELECT name FROM emp WHERE salary > 70 AND active",
        &["ann", "bob"],
    );
    check("SELECT name FROM emp WHERE active = NULL", &[]); // `= NULL` is never true; IS NULL is the right test
    check("SELECT name FROM emp WHERE salary IS NULL", &["di"]);
    check(
        "SELECT name FROM emp WHERE salary IS NOT NULL AND dept <> 'eng'",
        &["cy", "ed"],
    );
    check(
        "SELECT name, salary FROM emp ORDER BY salary DESC LIMIT 2",
        &["ann|100", "bob|80"],
    );
    check("SELECT name FROM emp ORDER BY salary LIMIT 1", &["di"]); // NULL sorts first ascending
    check(
        "SELECT salary * 2 + 1, -id FROM emp WHERE id = 1",
        &["201|-1"],
    );
    check(
        "SELECT id FROM emp WHERE 2 + 3 * 4 = 14 AND (1 + 2) * 3 = 9 AND 7 / 2 = 3 AND 10 - 4 - 3 = 3 LIMIT 1",
        &["1"],
    );
    check(
        "SELECT dept, COUNT(*), SUM(salary), MAX(salary) FROM emp GROUP BY dept ORDER BY dept",
        &["eng|2|180|100", "hr|1|60|60", "ops|2|70|70"],
    );
    check(
        "SELECT COUNT(*), COUNT(salary), MIN(name) FROM emp",
        &["5|4|ann"],
    );
    check(
        "SELECT COUNT(*), SUM(salary) FROM emp WHERE id > 99",
        &["0|NULL"],
    );
    check(
        "SELECT dept, COUNT(*) FROM emp GROUP BY dept ORDER BY dept DESC LIMIT 1",
        &["ops|2"],
    );
    check("select NAME from EMP where Id=3;", &["cy"]); // keywords and identifiers are case-insensitive
    check("SELECT name FROM emp WHERE name = 'it''s'", &[]);
}

fn error_cases() {
    let mut db = Database::default();
    db.execute("CREATE TABLE emp (id INT, name TEXT, salary INT)")
        .unwrap();
    db.execute("INSERT INTO emp VALUES (1,'ann',100)").unwrap();
    let kind = |db: &mut Database, sql: &str| {
        db.execute(sql)
            .err()
            .unwrap_or_else(|| panic!("expected an error for {sql}"))
    };

    let src = "SELECT FROM emp";
    let e = kind(&mut db, src);
    assert_eq!(
        e,
        SqlError::Parse {
            msg: "expected an expression".into(),
            span: Span { start: 7, end: 11 }
        }
    );
    let shown = render_error(src, &e);
    assert!(
        shown.contains("^^^^") && shown.contains("line 1, col 8") && shown.contains(src),
        "{shown}"
    );
    let e = kind(&mut db, "SELECT * FROM emp WHERE name = 'oops");
    assert!(
        matches!(
            e,
            SqlError::Parse {
                span: Span { start: 31, .. },
                ..
            }
        ),
        "{e:?}"
    );
    assert!(matches!(
        kind(&mut db, "SELECT * FROM emp LIMIT"),
        SqlError::Parse {
            span: Span { start: 23, end: 23 },
            ..
        }
    ));
    assert!(matches!(
        kind(&mut db, "SELECT * FROM emp WHERE id = 1 junk"),
        SqlError::Parse { .. }
    ));
    assert!(matches!(
        kind(&mut db, "SELECT * FROM emp WHERE id = #"),
        SqlError::Parse { .. }
    ));
    assert!(matches!(
        kind(&mut db, "SELECT * FROM emp WHERE salary > 'x'"),
        SqlError::Type(_)
    ));
    assert!(matches!(
        kind(&mut db, "SELECT * FROM emp WHERE salary"),
        SqlError::Type(_)
    ));
    assert!(matches!(
        kind(&mut db, "SELECT name + 1 FROM emp"),
        SqlError::Type(_)
    ));
    assert!(matches!(
        kind(&mut db, "SELECT SUM(name) FROM emp"),
        SqlError::Type(_)
    ));
    assert!(matches!(
        kind(&mut db, "SELECT name, COUNT(*) FROM emp"),
        SqlError::Type(_)
    ));
    assert!(matches!(
        kind(&mut db, "INSERT INTO emp VALUES ('x','y',1)"),
        SqlError::Type(_)
    ));
    assert!(matches!(
        kind(&mut db, "INSERT INTO emp VALUES (1,'y')"),
        SqlError::Catalog(_)
    ));
    assert!(matches!(
        kind(&mut db, "SELECT nope FROM emp"),
        SqlError::Catalog(_)
    ));
    assert!(matches!(
        kind(&mut db, "SELECT * FROM ghost"),
        SqlError::Catalog(_)
    ));
    assert!(matches!(
        kind(&mut db, "CREATE TABLE emp (a INT)"),
        SqlError::Catalog(_)
    ));
    assert_eq!(
        kind(&mut db, "SELECT 1 / 0 FROM emp"),
        SqlError::Runtime("division by zero".into())
    );
    assert!(matches!(
        kind(&mut db, "SELECT 9223372036854775807 + 1 FROM emp"),
        SqlError::Runtime(_)
    ));
    assert_eq!(
        db.query("SELECT id FROM emp").unwrap().len(),
        1,
        "failed INSERT must not append rows"
    );
    assert_eq!(
        db.query("SELECT 1 / 0 FROM emp LIMIT 0").unwrap().len(),
        0,
        "LIMIT 0 never pulls rows, so no error"
    );
}

const GRPS: [&str; 4] = ["a", "b", "c", "it's"];
#[derive(Clone)]
struct OracleRow {
    id: i64,
    score: Option<i64>,
    grp: Option<&'static str>,
    flag: Option<bool>,
}

/// Random predicate tree that can both print itself as SQL and evaluate itself with plain Rust `Option<bool>` logic.
enum Pred {
    Score(&'static str, i64),
    Arith(&'static str, i64), // score + id * 2 <op> k
    Grp(&'static str, &'static str),
    Flag(bool),
    IsNull(usize, bool),
    Not(Box<Pred>),
    And(Box<Pred>, Box<Pred>),
    Or(Box<Pred>, Box<Pred>),
}
const OPS: [&str; 6] = ["=", "<>", "<", "<=", ">", ">="];
fn cmp_op<T: Ord>(op: &str, a: &T, b: &T) -> bool {
    match op {
        "=" => a == b,
        "<>" => a != b,
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        _ => a >= b,
    }
}
impl Pred {
    fn random(r: &mut Rng, depth: u32) -> Pred {
        let op = OPS[r.below(6) as usize];
        match r.below(if depth == 0 { 6 } else { 9 }) {
            0 => Pred::Score(op, r.below(30) as i64 - 15),
            1 => Pred::Arith(op, r.below(60) as i64 - 10),
            2 => Pred::Grp(op, GRPS[r.below(4) as usize]),
            3 => Pred::Flag(r.below(2) == 0),
            4 => Pred::IsNull(1 + r.below(3) as usize, r.below(2) == 0),
            5 => Pred::Score(op, r.below(30) as i64 - 15),
            6 => Pred::Not(Box::new(Pred::random(r, depth - 1))),
            7 => Pred::And(
                Box::new(Pred::random(r, depth - 1)),
                Box::new(Pred::random(r, depth - 1)),
            ),
            _ => Pred::Or(
                Box::new(Pred::random(r, depth - 1)),
                Box::new(Pred::random(r, depth - 1)),
            ),
        }
    }
    fn sql(&self) -> String {
        match self {
            Pred::Score(op, k) => format!("score {op} {k}"),
            Pred::Arith(op, k) => format!("score + id * 2 {op} {k}"),
            Pred::Grp(op, g) => format!("grp {op} '{}'", g.replace('\'', "''")),
            Pred::Flag(b) => format!("flag = {b}"),
            Pred::IsNull(c, neg) => format!(
                "{} IS {}NULL",
                ["id", "score", "grp", "flag"][*c],
                if *neg { "NOT " } else { "" }
            ),
            Pred::Not(p) => format!("NOT ({})", p.sql()),
            Pred::And(a, b) => format!("({}) AND ({})", a.sql(), b.sql()),
            Pred::Or(a, b) => format!("({}) OR ({})", a.sql(), b.sql()),
        }
    }
    fn eval(&self, r: &OracleRow) -> Option<bool> {
        match self {
            Pred::Score(op, k) => r.score.map(|s| cmp_op(op, &s, k)),
            Pred::Arith(op, k) => r.score.map(|s| cmp_op(op, &(s + r.id * 2), k)),
            Pred::Grp(op, g) => r.grp.map(|x| cmp_op(op, &x, g)),
            Pred::Flag(b) => r.flag.map(|f| f == *b),
            Pred::IsNull(c, neg) => {
                Some([false, r.score.is_none(), r.grp.is_none(), r.flag.is_none()][*c] != *neg)
            }
            Pred::Not(p) => p.eval(r).map(|v| !v),
            Pred::And(a, b) => match (a.eval(r), b.eval(r)) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
            Pred::Or(a, b) => match (a.eval(r), b.eval(r)) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            },
        }
    }
}

fn random_table(r: &mut Rng, db: &mut Database) -> Vec<OracleRow> {
    db.execute("CREATE TABLE t (id INT, score INT, grp TEXT, flag BOOL)")
        .unwrap();
    let n = r.below(40) as usize;
    let rows: Vec<OracleRow> = (0..n as i64)
        .map(|id| OracleRow {
            id,
            score: (r.below(5) != 0).then(|| r.below(30) as i64 - 15),
            grp: (r.below(5) != 0).then(|| GRPS[r.below(4) as usize]),
            flag: (r.below(5) != 0).then(|| r.below(2) == 0),
        })
        .collect();
    if !rows.is_empty() {
        let lit = |o: Option<String>| o.unwrap_or_else(|| "NULL".into());
        let tuples: Vec<String> = rows
            .iter()
            .map(|x| {
                let grp = x.grp.map(|g| format!("'{}'", g.replace('\'', "''")));
                format!(
                    "({}, {}, {}, {})",
                    x.id,
                    lit(x.score.map(|s| s.to_string())),
                    lit(grp),
                    lit(x.flag.map(|f| f.to_string()))
                )
            })
            .collect();
        db.execute(&format!("INSERT INTO t VALUES {}", tuples.join(", ")))
            .unwrap();
    }
    rows
}

fn ids(rows: &[Row]) -> Vec<i64> {
    rows.iter()
        .map(|r| if let Value::Int(i) = r[0] { i } else { -1 })
        .collect()
}

fn property_tests(seed: u64, iterations: usize) {
    let mut r = Rng(seed);
    for _ in 0..iterations {
        let mut db = Database::default();
        let table = random_table(&mut r, &mut db);
        let all_ids: Vec<i64> = table.iter().map(|x| x.id).collect();
        assert_eq!(ids(&db.query("SELECT id FROM t").unwrap()), all_ids);

        // (c) + (d): WHERE equals a brute-force closure filter, and is a subset of the unfiltered rows.
        let p = Pred::random(&mut r, 3);
        let want: Vec<i64> = table
            .iter()
            .filter(|x| p.eval(x) == Some(true))
            .map(|x| x.id)
            .collect();
        let got = ids(&db
            .query(&format!("SELECT id FROM t WHERE {}", p.sql()))
            .unwrap());
        assert_eq!(got, want, "WHERE {}", p.sql());
        assert!(got.iter().all(|i| all_ids.contains(i)));

        // (a) ORDER BY output is sorted (NULLs first when ascending) and ties keep scan order.
        let desc = r.below(2) == 0;
        let sorted = db
            .query(&format!(
                "SELECT id, score FROM t ORDER BY score {}",
                if desc { "DESC" } else { "ASC" }
            ))
            .unwrap();
        let key = |row: &Row| {
            if let Value::Int(s) = row[1] {
                Some(s)
            } else {
                None
            }
        };
        for w in sorted.windows(2) {
            let (a, b) = (key(&w[0]), key(&w[1]));
            assert!(if desc { a >= b } else { a <= b }, "not sorted: {sorted:?}");
            if a == b {
                assert!(ids(&w[..1])[0] < ids(&w[1..])[0], "unstable tie order");
            }
        }
        assert_eq!(sorted.len(), table.len());

        // (b) LIMIT n returns min(n, rows) and is a prefix of the unlimited result.
        let n = r.below(50) as usize;
        let limited = ids(&db
            .query(&format!("SELECT id FROM t ORDER BY score LIMIT {n}"))
            .unwrap());
        assert_eq!(limited.len(), n.min(table.len()));
        assert_eq!(limited[..], ids(&sorted_asc(&mut db))[..limited.len()]);

        // (d) the full pipeline WHERE + ORDER BY + LIMIT against the oracle.
        let mut oracle: Vec<&OracleRow> =
            table.iter().filter(|x| p.eval(x) == Some(true)).collect();
        oracle.sort_by_key(|x| std::cmp::Reverse(x.score)); // stable; Reverse(None) sorts last, matching DESC
        let sql = format!(
            "SELECT id FROM t WHERE {} ORDER BY score DESC LIMIT {n}",
            p.sql()
        );
        let want: Vec<i64> = oracle.iter().take(n).map(|x| x.id).collect();
        assert_eq!(ids(&db.query(&sql).unwrap()), want, "{sql}");

        // GROUP BY against a BTreeMap fold: (count, sum, min, max); NULL group sorts first in both.
        let mut groups: BTreeMap<Option<&str>, (i64, Option<i64>, Option<i64>, Option<i64>)> =
            BTreeMap::new();
        for x in &table {
            let g = groups.entry(x.grp).or_insert((0, None, None, None));
            g.0 += 1;
            if let Some(s) = x.score {
                g.1 = Some(g.1.unwrap_or(0) + s);
                g.2 = Some(g.2.map_or(s, |m| m.min(s)));
                g.3 = Some(g.3.map_or(s, |m| m.max(s)));
            }
        }
        let opt = |o: Option<i64>| o.map_or("NULL".to_string(), |v| v.to_string());
        let want: Vec<String> = groups
            .iter()
            .map(|(g, a)| {
                format!(
                    "{}|{}|{}|{}|{}",
                    g.unwrap_or("NULL"),
                    a.0,
                    opt(a.1),
                    opt(a.2),
                    opt(a.3)
                )
            })
            .collect();
        let got = cells(&db.query("SELECT grp, COUNT(*), SUM(score), MIN(score), MAX(score) FROM t GROUP BY grp ORDER BY grp").unwrap());
        assert_eq!(got, want);
    }
}

fn sorted_asc(db: &mut Database) -> Vec<Row> {
    db.query("SELECT id FROM t ORDER BY score").unwrap()
}

fn main() {
    let mut db = Database::default();
    let script = [
        "CREATE TABLE emp (id INT, name TEXT, dept TEXT, salary INT, active BOOL)",
        "INSERT INTO emp VALUES (1,'ann','eng',100,true),(2,'bob','eng',80,true),(3,'cy','ops',70,false),(4,'di','ops',NULL,true),(5,'ed','hr',60,NULL)",
        "SELECT * FROM emp WHERE salary > 65 ORDER BY salary DESC",
        "SELECT dept, COUNT(*), SUM(salary) FROM emp GROUP BY dept ORDER BY dept",
        "SELECT name, salary * 12 FROM emp WHERE active OR salary IS NULL LIMIT 3",
    ];
    for sql in script {
        println!("sql> {sql}");
        match db.execute(sql) {
            Ok(Output::Rows { cols, rows }) => println!("{}", render_table(&cols, &rows)),
            Ok(Output::Inserted(n)) => println!("{n} rows inserted\n"),
            Ok(Output::Created) => println!("table created\n"),
            Err(e) => println!("{}\n", render_error(sql, &e)),
        }
    }
    let bad = "SELECT name FROM emp WHERE salary >> 5";
    println!(
        "sql> {bad}\n{}\n",
        render_error(bad, &db.execute(bad).err().expect("must fail"))
    );

    hand_checked();
    error_cases();
    property_tests(0x5EED_C0DE, 400);
    property_tests(0xDEAD_BEEF_1234_5678, 400);
    println!(
        "hand-checked queries, error cases and 800 seeded random tables (WHERE/ORDER BY/LIMIT/GROUP BY vs oracle): all passed"
    );
}
