//! A recursive-descent parser for the SQL that the shell understands.

use super::ast::*;
use super::lexer::{parse_error, tokenize, Token};
use crate::common::exception::Result;

/// Words that end an expression or a table name and so cannot be used as an alias without `AS`.
const RESERVED: &[&str] = &[
    "select", "from", "where", "group", "having", "order", "limit", "offset", "join", "inner", "left", "right", "full", "outer", "cross",
    "natural", "on", "using", "union", "intersect", "except", "as", "and", "or", "not", "with", "values", "by", "asc", "desc", "nulls",
    "over", "partition", "is", "in", "like", "between", "case", "when", "then", "else", "end", "distinct", "set", "into", "returning",
    "window", "rows", "range", "unbounded", "preceding", "following", "current", "row",
];

pub fn parse(sql: &str) -> Result<Vec<Statement>> {
    let mut parser = Parser { tokens: tokenize(sql)?, pos: 0, params: 0 };
    let mut statements = vec![];
    loop {
        while parser.eat_symbol(";") {}
        if parser.peek().is_none() {
            break;
        }
        statements.push(parser.statement()?);
        if parser.peek().is_some() && !parser.at_symbol(";") {
            return Err(parser.unexpected());
        }
    }
    Ok(statements)
}

/// Parses one expression and nothing else: `1 + 2 * 3`, `lower(name) = 'x' and not b`.
pub fn parse_expr(sql: &str) -> Result<Expr> {
    let mut parser = Parser { tokens: tokenize(sql)?, pos: 0, params: 0 };
    let e = parser.expr()?;
    if parser.peek().is_some() {
        return Err(parser.unexpected());
    }
    Ok(e)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// How many `?` placeholders were read so far.
    params: usize,
}

type P<T> = Result<T>;

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_at(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.pos + n)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn unexpected(&self) -> crate::common::exception::Exception {
        match self.peek() {
            Some(t) => parse_error(format!("syntax error at or near {t:?}")),
            None => parse_error("syntax error at end of input"),
        }
    }

    fn at_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(x)) if x == w)
    }

    fn at_word_n(&self, n: usize, w: &str) -> bool {
        matches!(self.peek_at(n), Some(Token::Word(x)) if x == w)
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if self.at_word(w) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, w: &str) -> P<()> {
        if self.eat_word(w) {
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    fn at_symbol(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Token::Symbol(x)) if *x == s)
    }

    fn eat_symbol(&mut self, s: &str) -> bool {
        if self.at_symbol(s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, s: &str) -> P<()> {
        if self.eat_symbol(s) {
            Ok(())
        } else {
            Err(self.unexpected())
        }
    }

    /// An identifier: a quoted one, or an unquoted word that is not reserved.
    fn ident(&mut self) -> P<String> {
        match self.peek().cloned() {
            Some(Token::Quoted(s)) => {
                self.pos += 1;
                Ok(s)
            }
            Some(Token::Word(w)) if !RESERVED.contains(&w.as_str()) => {
                self.pos += 1;
                Ok(w)
            }
            _ => Err(self.unexpected()),
        }
    }

    /// An alias after an expression or table: `AS name` or just `name` (when the word cannot start something else).
    fn optional_alias(&mut self) -> P<Option<String>> {
        if self.eat_word("as") {
            return Ok(Some(self.ident()?));
        }
        match self.peek() {
            Some(Token::Quoted(_)) => Ok(Some(self.ident()?)),
            Some(Token::Word(w)) if !RESERVED.contains(&w.as_str()) => Ok(Some(self.ident()?)),
            _ => Ok(None),
        }
    }

    // ---- statements ---------------------------------------------------------------------------------------------------------

    fn statement(&mut self) -> P<Statement> {
        let Some(Token::Word(w)) = self.peek().cloned() else {
            if self.at_symbol("(") {
                return Ok(Statement::Select(Box::new(self.query()?)));
            }
            return Err(self.unexpected());
        };
        match w.as_str() {
            "select" | "with" | "values" => Ok(Statement::Select(Box::new(self.query()?))),
            "create" => self.create(),
            "insert" => self.insert(),
            "update" => self.update(),
            "delete" => self.delete(),
            "explain" => self.explain(),
            "set" => {
                self.pos += 1;
                self.eat_word("session");
                self.eat_word("local");
                let name = self.ident()?;
                if !self.eat_symbol("=") {
                    self.expect_word("to")?;
                }
                // PostgreSQL reads a bare word after SET as a string (`set x = yes`), and a number or string as itself
                let value = match self.peek().cloned() {
                    Some(Token::Word(w)) => {
                        self.pos += 1;
                        Expr::Str(w)
                    }
                    _ => self.expr()?,
                };
                Ok(Statement::Set { name, value })
            }
            "show" => {
                self.pos += 1;
                Ok(Statement::Show { name: self.ident()? })
            }
            "begin" | "start" => {
                self.pos += 1;
                self.eat_word("transaction");
                Ok(Statement::Begin { isolation: None })
            }
            "commit" | "end" => {
                self.pos += 1;
                Ok(Statement::Commit)
            }
            "rollback" | "abort" => {
                self.pos += 1;
                Ok(Statement::Rollback)
            }
            _ => Err(self.unexpected()),
        }
    }

    fn explain(&mut self) -> P<Statement> {
        self.expect_word("explain")?;
        let mut options = vec![];
        if self.eat_word("analyze") {
            options.push("analyze".to_string());
        }
        if self.eat_symbol("(") {
            loop {
                match self.next() {
                    Some(Token::Word(w)) => options.push(w),
                    _ => return Err(self.unexpected()),
                }
                if !self.eat_symbol(",") {
                    break;
                }
            }
            self.expect_symbol(")")?;
        }
        Ok(Statement::Explain { options, statement: Box::new(self.statement()?) })
    }

    fn create(&mut self) -> P<Statement> {
        self.expect_word("create")?;
        if self.eat_word("unique") {
            return Err(parse_error("UNIQUE indexes are not supported"));
        }
        if self.eat_word("index") {
            let name = self.ident()?;
            self.expect_word("on")?;
            let table = self.ident()?;
            let using = if self.eat_word("using") { Some(self.ident()?) } else { None };
            self.expect_symbol("(")?;
            let mut columns = vec![self.ident()?];
            while self.eat_symbol(",") {
                columns.push(self.ident()?);
            }
            self.expect_symbol(")")?;
            return Ok(Statement::CreateIndex { name, table, columns, using });
        }
        self.expect_word("table")?;
        let name = self.ident()?;
        self.expect_symbol("(")?;
        let mut columns = vec![];
        let mut primary_key = vec![];
        loop {
            if self.at_word("primary") {
                self.pos += 1;
                self.expect_word("key")?;
                self.expect_symbol("(")?;
                let mut cols = vec![self.ident()?];
                while self.eat_symbol(",") {
                    cols.push(self.ident()?);
                }
                self.expect_symbol(")")?;
                primary_key = cols;
            } else {
                let col_name = self.ident()?;
                let type_name = match self.next() {
                    Some(Token::Word(w)) => w,
                    Some(Token::Quoted(s)) => s,
                    _ => return Err(self.unexpected()),
                };
                // `double precision`
                let type_name = if type_name == "double" && self.eat_word("precision") { "double".to_string() } else { type_name };
                let mut type_args = vec![];
                if self.eat_symbol("(") {
                    type_args.push(self.expr()?);
                    while self.eat_symbol(",") {
                        type_args.push(self.expr()?);
                    }
                    self.expect_symbol(")")?;
                }
                let mut is_pk = false;
                while let Some(Token::Word(w)) = self.peek().cloned() {
                    match w.as_str() {
                        "primary" => {
                            self.pos += 1;
                            self.expect_word("key")?;
                            is_pk = true;
                        }
                        "not" | "null" | "unique" | "default" => return Err(parse_error(format!("unsupported constraint: {w}"))),
                        _ => break,
                    }
                }
                columns.push(ColumnDef { name: col_name, type_name, type_args, primary_key: is_pk });
            }
            if !self.eat_symbol(",") {
                break;
            }
        }
        self.expect_symbol(")")?;
        Ok(Statement::CreateTable { name, columns, primary_key })
    }

    fn insert(&mut self) -> P<Statement> {
        self.expect_word("insert")?;
        self.expect_word("into")?;
        let table = self.ident()?;
        let mut columns = None;
        // `insert into t (select ...)` has a query in the parentheses, `insert into t (a, b) values ...` has a column list.
        let mut n = 1;
        while matches!(self.peek_at(n), Some(Token::Symbol("("))) {
            n += 1;
        }
        if self.at_symbol("(") && !(self.at_word_n(n, "select") || self.at_word_n(n, "values") || self.at_word_n(n, "with")) {
            self.pos += 1;
            let mut cols = vec![self.ident()?];
            while self.eat_symbol(",") {
                cols.push(self.ident()?);
            }
            self.expect_symbol(")")?;
            columns = Some(cols);
        }
        let query = self.query()?;
        Ok(Statement::Insert { table, columns, query: Box::new(query) })
    }

    fn update(&mut self) -> P<Statement> {
        self.expect_word("update")?;
        let table = self.ident()?;
        self.expect_word("set")?;
        let mut assignments = vec![];
        loop {
            let col = self.ident()?;
            self.expect_symbol("=")?;
            assignments.push((col, self.expr()?));
            if !self.eat_symbol(",") {
                break;
            }
        }
        let has_from = self.eat_word("from");
        if has_from {
            // parse and discard the FROM list; the binder refuses it
            self.from_list()?;
        }
        let filter = if self.eat_word("where") { Some(self.expr()?) } else { None };
        Ok(Statement::Update { table, assignments, filter, has_from })
    }

    fn delete(&mut self) -> P<Statement> {
        self.expect_word("delete")?;
        self.expect_word("from")?;
        let table = self.ident()?;
        let filter = if self.eat_word("where") { Some(self.expr()?) } else { None };
        Ok(Statement::Delete { table, filter })
    }

    // ---- queries ------------------------------------------------------------------------------------------------------------

    fn query(&mut self) -> P<Query> {
        let mut ctes = vec![];
        if self.eat_word("with") {
            let recursive = self.eat_word("recursive");
            loop {
                let name = self.ident()?;
                self.expect_word("as")?;
                self.expect_symbol("(")?;
                let query = self.query()?;
                self.expect_symbol(")")?;
                ctes.push(Cte { name, query, recursive });
                if !self.eat_symbol(",") {
                    break;
                }
            }
        }
        let mut query = if self.at_symbol("(") {
            self.pos += 1;
            let inner = self.query()?;
            self.expect_symbol(")")?;
            inner
        } else if self.eat_word("values") {
            let mut rows = vec![];
            loop {
                self.expect_symbol("(")?;
                let mut row = vec![self.expr()?];
                while self.eat_symbol(",") {
                    row.push(self.expr()?);
                }
                self.expect_symbol(")")?;
                rows.push(row);
                if !self.eat_symbol(",") {
                    break;
                }
            }
            Query { ctes: vec![], body: QueryBody::Values(rows), order_by: vec![], limit: None, offset: None }
        } else {
            let core = self.select_core()?;
            Query { ctes: vec![], body: QueryBody::Select(Box::new(core)), order_by: vec![], limit: None, offset: None }
        };
        if self.at_word("union") || self.at_word("intersect") || self.at_word("except") {
            return Err(parse_error("set operations (UNION, INTERSECT, EXCEPT) are not supported"));
        }
        if !ctes.is_empty() {
            query.ctes = ctes;
        }
        if self.eat_word("order") {
            self.expect_word("by")?;
            query.order_by = self.order_items()?;
        }
        loop {
            if self.eat_word("limit") {
                query.limit = Some(self.expr()?);
            } else if self.eat_word("offset") {
                query.offset = Some(self.expr()?);
            } else {
                break;
            }
        }
        Ok(query)
    }

    fn select_core(&mut self) -> P<SelectCore> {
        self.expect_word("select")?;
        let mut distinct = false;
        let mut distinct_on = false;
        if self.eat_word("distinct") {
            distinct = true;
            if self.eat_word("on") {
                distinct_on = true;
                self.expect_symbol("(")?;
                self.expr()?;
                while self.eat_symbol(",") {
                    self.expr()?;
                }
                self.expect_symbol(")")?;
            }
        } else {
            self.eat_word("all");
        }
        let mut items = vec![];
        loop {
            if self.eat_symbol("*") {
                items.push(SelectItem::Wildcard);
            } else {
                let expr = self.expr()?;
                let alias = self.optional_alias()?;
                items.push(SelectItem::Expr { expr, alias });
            }
            if !self.eat_symbol(",") {
                break;
            }
            // DuckDB's parser (which BusTub uses) allows a trailing comma: `select a, b, from t`
            if self.at_word("from") || self.at_symbol(")") || self.at_symbol(";") || self.peek().is_none() {
                break;
            }
        }
        let from = if self.eat_word("from") { self.from_list()? } else { vec![] };
        let filter = if self.eat_word("where") { Some(self.expr()?) } else { None };
        let mut group_by = vec![];
        if self.eat_word("group") {
            self.expect_word("by")?;
            group_by.push(self.expr()?);
            while self.eat_symbol(",") {
                group_by.push(self.expr()?);
            }
        }
        let having = if self.eat_word("having") { Some(self.expr()?) } else { None };
        Ok(SelectCore { distinct, distinct_on, items, from, filter, group_by, having })
    }

    /// One side of a set operation: a SELECT without its ORDER BY and LIMIT (those belong to the whole query), or a parenthesised query,
    /// which keeps its own.
    fn set_operand(&mut self) -> P<Query> {
        if self.eat_symbol("(") {
            let q = self.query()?;
            self.expect_symbol(")")?;
            return Ok(q);
        }
        let core = self.select_core()?;
        Ok(Query { ctes: vec![], body: QueryBody::Select(Box::new(core)), order_by: vec![], limit: None, offset: None })
    }

    /// The set operator at the cursor, if there is one, with its precedence: INTERSECT binds tighter than UNION and EXCEPT.
    fn peek_set_op(&self) -> Option<(SetOperator, u8)> {
        if self.at_word("union") {
            Some((SetOperator::Union, 1))
        } else if self.at_word("except") {
            Some((SetOperator::Except, 1))
        } else if self.at_word("intersect") {
            Some((SetOperator::Intersect, 2))
        } else {
            None
        }
    }

    fn set_node(op: SetOperator, all: bool, left: Query, right: Query) -> Query {
        Query { ctes: vec![], body: QueryBody::SetOp { op, all, left: Box::new(left), right: Box::new(right) }, order_by: vec![], limit: None, offset: None }
    }

    /// `first [UNION | INTERSECT | EXCEPT [ALL | DISTINCT] operand]...`: the chain of set operations that follows a query.
    fn set_ops(&mut self, first: Query) -> P<Query> {
        let mut left = first;
        while let Some((op, _)) = self.peek_set_op() {
            self.pos += 1;
            let all = self.eat_word("all");
            if !all {
                self.eat_word("distinct");
            }
            let right = self.set_operand()?;
            left = Self::set_node(op, all, left, right);
        }
        Ok(left)
    }

    // TODO(3j-06): a helper of yours

    fn order_items(&mut self) -> P<Vec<OrderItem>> {
        let mut items = vec![];
        loop {
            let expr = self.expr()?;
            let dir = if self.eat_word("asc") {
                SortDir::Asc
            } else if self.eat_word("desc") {
                SortDir::Desc
            } else {
                SortDir::Default
            };
            let nulls = if self.eat_word("nulls") {
                if self.eat_word("first") {
                    SortNulls::First
                } else {
                    self.expect_word("last")?;
                    SortNulls::Last
                }
            } else {
                SortNulls::Default
            };
            items.push(OrderItem { expr, dir, nulls });
            if !self.eat_symbol(",") {
                break;
            }
        }
        Ok(items)
    }

    fn from_list(&mut self) -> P<Vec<TableRef>> {
        let mut list = vec![self.table_ref()?];
        while self.eat_symbol(",") {
            list.push(self.table_ref()?);
        }
        Ok(list)
    }

    fn table_ref(&mut self) -> P<TableRef> {
        let mut left = self.table_primary()?;
        loop {
            let kind = if self.eat_word("join") {
                JoinKind::Inner
            } else if self.eat_word("inner") {
                self.expect_word("join")?;
                JoinKind::Inner
            } else if self.eat_word("left") {
                self.eat_word("outer");
                self.expect_word("join")?;
                JoinKind::Left
            } else if self.eat_word("right") {
                self.eat_word("outer");
                self.expect_word("join")?;
                JoinKind::Right
            } else if self.eat_word("full") {
                self.eat_word("outer");
                self.expect_word("join")?;
                JoinKind::Full
            } else if self.eat_word("cross") {
                self.expect_word("join")?;
                JoinKind::Cross
            } else if self.at_word("natural") {
                return Err(parse_error("NATURAL joins are not supported"));
            } else {
                break;
            };
            let right = self.table_primary()?;
            let on = if self.eat_word("on") {
                Some(self.expr()?)
            } else if self.at_word("using") {
                return Err(parse_error("JOIN ... USING is not supported"));
            } else {
                None
            };
            left = TableRef::Join { kind, left: Box::new(left), right: Box::new(right), on };
        }
        Ok(left)
    }

    fn table_primary(&mut self) -> P<TableRef> {
        if self.at_symbol("(") {
            // a derived table `(select ...) alias`, or a parenthesised join `(a join b on ...)`
            let mut n = 1;
            while matches!(self.peek_at(n), Some(Token::Symbol("("))) {
                n += 1;
            }
            let is_query = self.at_word_n(n, "select") || self.at_word_n(n, "values") || self.at_word_n(n, "with");
            self.pos += 1;
            if is_query {
                let query = self.query()?;
                self.expect_symbol(")")?;
                let alias = self.optional_alias()?;
                return Ok(TableRef::Subquery { query: Box::new(query), alias });
            }
            let inner = self.table_ref()?;
            self.expect_symbol(")")?;
            return Ok(inner);
        }
        let name = self.ident()?;
        if self.at_symbol(".") {
            return Err(parse_error("schema-qualified table names are not supported"));
        }
        let alias = self.optional_alias()?;
        Ok(TableRef::Named { name, alias })
    }

    // ---- expressions --------------------------------------------------------------------------------------------------------

    /// Parses an expression with SQL's precedence, loosest first: `OR`, `AND`, `NOT`, comparison and `IS [NOT] NULL`, `+ - ||`, `* / %`, unary minus.
    pub fn expr(&mut self) -> P<Expr> {
        todo!("3d-05: loosest first: or, and, not, comparison and IS [NOT] NULL, + - ||, * / %, unary minus; atoms are primary expressions")
    }

    fn primary(&mut self) -> P<Expr> {
        match self.next() {
            Some(Token::Number(n)) => {
                if n.chars().all(|c| c.is_ascii_digit()) {
                    match n.parse::<i64>() {
                        Ok(v) if v <= i32::MAX as i64 + 1 => Ok(Expr::Integer(v)),
                        _ => Ok(Expr::Float(n)),
                    }
                } else {
                    Ok(Expr::Float(n))
                }
            }
            Some(Token::Str(s)) => Ok(Expr::Str(s)),
            Some(Token::Symbol("(")) => {
                let e = self.expr()?;
                self.expect_symbol(")")?;
                Ok(e)
            }
            Some(Token::Symbol("*")) => Ok(Expr::Star),
            Some(Token::Quoted(name)) => self.column_or_call(name, true),
            Some(Token::Word(w)) => match w.as_str() {
                "null" => Ok(Expr::Null),
                "true" => Ok(Expr::Bool(true)),
                "false" => Ok(Expr::Bool(false)),
                _ if RESERVED.contains(&w.as_str()) => {
                    self.pos -= 1;
                    Err(self.unexpected())
                }
                _ => self.column_or_call(w, false),
            },
            _ => {
                self.pos -= 1;
                Err(self.unexpected())
            }
        }
    }


    fn column_or_call(&mut self, first: String, quoted: bool) -> P<Expr> {
        if !quoted && self.at_symbol("(") {
            self.pos += 1;
            let mut args = vec![];
            let mut distinct = false;
            if self.eat_symbol("*") {
                // count(*): no arguments
            } else if !self.at_symbol(")") {
                distinct = self.eat_word("distinct");
                args.push(self.expr()?);
                while self.eat_symbol(",") {
                    args.push(self.expr()?);
                }
            }
            self.expect_symbol(")")?;
            let over = if self.eat_word("over") { Some(Box::new(self.window_spec()?)) } else { None };
            return Ok(Expr::Function { name: first, args, distinct, over });
        }
        let mut parts = vec![first];
        while self.at_symbol(".") {
            self.pos += 1;
            if self.eat_symbol("*") {
                return Err(parse_error("table.* is not supported"));
            }
            parts.push(self.ident()?);
        }
        Ok(Expr::Column(parts))
    }

    fn window_spec(&mut self) -> P<WindowSpec> {
        self.expect_symbol("(")?;
        let mut partition_by = vec![];
        if self.eat_word("partition") {
            self.expect_word("by")?;
            partition_by.push(self.expr()?);
            while self.eat_symbol(",") {
                partition_by.push(self.expr()?);
            }
        }
        let mut order_by = vec![];
        if self.eat_word("order") {
            self.expect_word("by")?;
            order_by = self.order_items()?;
        }
        let mut frame = None;
        if self.at_word("rows") || self.at_word("range") {
            let range = self.at_word("range");
            self.pos += 1;
            if self.eat_word("between") {
                let start = self.frame_bound()?;
                self.expect_word("and")?;
                let end = self.frame_bound()?;
                frame = Some(WindowFrame { range, start, end });
            } else {
                frame = Some(WindowFrame { range, start: self.frame_bound()?, end: FrameBound::CurrentRow });
            }
        }
        self.expect_symbol(")")?;
        Ok(WindowSpec { partition_by, order_by, frame })
    }

    fn frame_bound(&mut self) -> P<FrameBound> {
        if self.eat_word("unbounded") {
            if self.eat_word("preceding") {
                return Ok(FrameBound::UnboundedPreceding);
            }
            self.expect_word("following")?;
            return Ok(FrameBound::UnboundedFollowing);
        }
        if self.eat_word("current") {
            self.expect_word("row")?;
            return Ok(FrameBound::CurrentRow);
        }
        let e = self.expr()?;
        if self.eat_word("preceding") {
            Ok(FrameBound::Preceding(Box::new(e)))
        } else {
            self.expect_word("following")?;
            Ok(FrameBound::Following(Box::new(e)))
        }
    }
}
