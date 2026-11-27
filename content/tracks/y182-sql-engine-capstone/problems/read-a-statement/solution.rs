/// The tallest expression tree accepted.
pub const MAX_HEIGHT: usize = 100;
/// The most brackets an expression may have open at once.
pub const MAX_OPEN: usize = 100;

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

/// An operator with two operands. `!=` is `Ne`.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    BadCharacter,
    UnclosedText,
    NumberTooLarge,
    Unexpected,
    TooDeep,
}

/// What is wrong, and the bytes `start..end` of the source it is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntaxError {
    pub problem: Problem,
    pub start: usize,
    pub end: usize,
}

const RESERVED: [&str; 21] = [
    "select", "from", "where", "order", "by", "limit", "group", "and", "or", "not", "insert",
    "into", "values", "create", "table", "asc", "desc", "is", "null", "true", "false",
];
const SYMBOLS: [&str; 15] = [
    "<=", ">=", "<>", "!=", "(", ")", ",", "*", "+", "-", "/", "=", "<", ">", ";",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A word, in small letters. Keywords are words too.
    Word(String),
    Number(i64),
    /// The text between the quotes, with each doubled quote made single.
    Text(String),
    /// One of the symbols, as written, except that `!=` is given as `<>`.
    Symbol(&'static str),
}

/// A token, and the bytes `start..end` of the source it covers.
pub type Lexed = (Token, usize, usize);

fn error<T>(problem: Problem, start: usize, end: usize) -> Result<T, SyntaxError> {
    Err(SyntaxError {
        problem,
        start,
        end,
    })
}

/// Every token of the text, or the first bad one.
pub fn lex(sql: &str) -> Result<Vec<Lexed>, SyntaxError> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let first = bytes[at];
        let run = |from: usize, keep: fn(u8) -> bool| {
            from + bytes[from..].iter().take_while(|&&b| keep(b)).count()
        };
        let token = if first.is_ascii_whitespace() {
            at += 1;
            continue;
        } else if first.is_ascii_digit() {
            at = run(at, |b| b.is_ascii_digit());
            match sql[start..at].parse() {
                Ok(n) => Token::Number(n),
                Err(_) => return error(Problem::NumberTooLarge, start, at),
            }
        } else if first.is_ascii_alphabetic() || first == b'_' {
            at = run(at, |b| b.is_ascii_alphanumeric() || b == b'_');
            Token::Word(sql[start..at].to_ascii_lowercase())
        } else if first == b'\'' {
            let mut text = String::new();
            at += 1;
            loop {
                // copy up to the next quote in one piece; the quote is ASCII, so both cuts are between characters
                let Some(length) = sql[at..].find('\'') else {
                    return error(Problem::UnclosedText, start, sql.len());
                };
                text.push_str(&sql[at..at + length]);
                at += length + 1;
                if bytes.get(at) != Some(&b'\'') {
                    break;
                }
                text.push('\'');
                at += 1;
            }
            Token::Text(text)
        } else if let Some(symbol) = SYMBOLS
            .iter()
            .find(|s| bytes[at..].starts_with(s.as_bytes()))
        {
            at += symbol.len();
            Token::Symbol(if *symbol == "!=" { "<>" } else { symbol })
        } else {
            let width = sql[at..].chars().next().map_or(1, char::len_utf8);
            return error(Problem::BadCharacter, start, start + width);
        };
        out.push((token, start, at));
    }
    Ok(out)
}

const NOT: u8 = 3;
const COMPARISON: u8 = 4;
const MINUS: u8 = 7;

fn infix(token: &Token) -> Option<(Op, u8)> {
    Some(match token {
        Token::Word(w) if w == "or" => (Op::Or, 1),
        Token::Word(w) if w == "and" => (Op::And, 2),
        Token::Symbol(s) => match *s {
            "=" => (Op::Eq, COMPARISON),
            "<>" => (Op::Ne, COMPARISON),
            "<" => (Op::Lt, COMPARISON),
            "<=" => (Op::Le, COMPARISON),
            ">" => (Op::Gt, COMPARISON),
            ">=" => (Op::Ge, COMPARISON),
            "+" => (Op::Add, 5),
            "-" => (Op::Sub, 5),
            "*" => (Op::Mul, 6),
            "/" => (Op::Div, 6),
            _ => return None,
        },
        _ => return None,
    })
}

/// A subtree and its height.
type Tree = (Expr, usize);

struct Parser {
    tokens: Vec<Lexed>,
    next: usize,
    length: usize,
    open: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.next).map(|t| &t.0)
    }

    fn fail_as<T>(&self, problem: Problem) -> Result<T, SyntaxError> {
        let (start, end) = self
            .tokens
            .get(self.next)
            .map_or((self.length, self.length), |t| (t.1, t.2));
        error(problem, start, end)
    }

    fn fail<T>(&self) -> Result<T, SyntaxError> {
        self.fail_as(Problem::Unexpected)
    }

    fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Word(w)) if w == word)
    }

    fn word(&mut self, word: &str) -> bool {
        let hit = self.at_word(word);
        self.next += usize::from(hit);
        hit
    }

    fn symbol(&mut self, symbol: &str) -> bool {
        let hit = matches!(self.peek(), Some(Token::Symbol(s)) if *s == symbol);
        self.next += usize::from(hit);
        hit
    }

    fn need_word(&mut self, word: &str) -> Result<(), SyntaxError> {
        if self.word(word) {
            Ok(())
        } else {
            self.fail()
        }
    }

    fn need_symbol(&mut self, symbol: &str) -> Result<(), SyntaxError> {
        if self.symbol(symbol) {
            Ok(())
        } else {
            self.fail()
        }
    }

    fn name(&mut self) -> Result<String, SyntaxError> {
        match self.peek() {
            Some(Token::Word(w)) if !RESERVED.contains(&w.as_str()) => {
                let name = w.clone();
                self.next += 1;
                Ok(name)
            }
            _ => self.fail(),
        }
    }

    /// One or more of `one`, separated by commas.
    fn list<T>(
        &mut self,
        one: fn(&mut Self) -> Result<T, SyntaxError>,
    ) -> Result<Vec<T>, SyntaxError> {
        let mut all = vec![one(self)?];
        while self.symbol(",") {
            all.push(one(self)?);
        }
        Ok(all)
    }

    /// The height of a node over a subtree of height `below`, with `above` operators over the node itself.
    fn grow(&self, below: usize, above: usize) -> Result<usize, SyntaxError> {
        if below + 1 + above > MAX_HEIGHT {
            return self.fail_as(Problem::TooDeep);
        }
        Ok(below + 1)
    }

    /// An expression made of operators at least as strong as `weakest`. `above` counts the operators already over it,
    /// so a run of prefix operators is refused before the recursion gets deep.
    fn expr(&mut self, weakest: u8, above: usize) -> Result<Tree, SyntaxError> {
        let (mut tree, mut height) = self.operand(weakest, above)?;
        loop {
            if COMPARISON >= weakest && self.at_word("is") {
                self.next += 1;
                let negated = self.word("not");
                self.need_word("null")?;
                height = self.grow(height, above)?;
                tree = Expr::IsNull {
                    of: Box::new(tree),
                    negated,
                };
                continue;
            }
            let Some((op, strength)) = self.peek().and_then(infix).filter(|i| i.1 >= weakest)
            else {
                return Ok((tree, height));
            };
            self.next += 1;
            // one stronger on the right: equal operators group from the left
            let (right, right_height) = self.expr(strength + 1, above + 1)?;
            // a long chain grows the tree here, in a loop
            height = self.grow(height.max(right_height), above)?;
            tree = Expr::Binary(op, Box::new(tree), Box::new(right));
        }
    }

    fn operand(&mut self, weakest: u8, above: usize) -> Result<Tree, SyntaxError> {
        if above >= MAX_HEIGHT {
            return self.fail_as(Problem::TooDeep);
        }
        if weakest <= NOT && self.word("not") {
            let (of, height) = self.expr(NOT, above + 1)?;
            return Ok((Expr::Not(Box::new(of)), self.grow(height, above)?));
        }
        if self.symbol("-") {
            let (of, height) = self.expr(MINUS, above + 1)?;
            return Ok((Expr::Neg(Box::new(of)), self.grow(height, above)?));
        }
        if matches!(self.peek(), Some(Token::Symbol("("))) {
            if self.open == MAX_OPEN {
                return self.fail_as(Problem::TooDeep);
            }
            self.next += 1;
            self.open += 1;
            let inside = self.expr(0, above)?;
            self.need_symbol(")")?;
            self.open -= 1;
            return Ok(inside);
        }
        let leaf = match self.peek() {
            Some(Token::Number(n)) => Expr::Lit(Value::Int(*n)),
            Some(Token::Text(t)) => Expr::Lit(Value::Text(t.clone())),
            Some(Token::Word(w)) => match w.as_str() {
                "true" => Expr::Lit(Value::Bool(true)),
                "false" => Expr::Lit(Value::Bool(false)),
                "null" => Expr::Lit(Value::Null),
                _ => return Ok((Expr::Column(self.name()?), 1)),
            },
            _ => return self.fail(),
        };
        self.next += 1;
        Ok((leaf, 1))
    }

    fn expression(&mut self) -> Result<Expr, SyntaxError> {
        Ok(self.expr(0, 0)?.0)
    }

    fn column(&mut self) -> Result<(String, Type), SyntaxError> {
        let name = self.name()?;
        let ty = if self.word("int") {
            Type::Int
        } else if self.word("text") {
            Type::Text
        } else if self.word("bool") {
            Type::Bool
        } else {
            return self.fail();
        };
        Ok((name, ty))
    }

    fn tuple(&mut self) -> Result<Vec<Expr>, SyntaxError> {
        self.need_symbol("(")?;
        let values = self.list(Self::expression)?;
        self.need_symbol(")")?;
        Ok(values)
    }

    fn create(&mut self) -> Result<Statement, SyntaxError> {
        self.need_word("table")?;
        let table = self.name()?;
        self.need_symbol("(")?;
        let columns = self.list(Self::column)?;
        self.need_symbol(")")?;
        Ok(Statement::CreateTable { table, columns })
    }

    fn insert(&mut self) -> Result<Statement, SyntaxError> {
        self.need_word("into")?;
        let table = self.name()?;
        self.need_word("values")?;
        let rows = self.list(Self::tuple)?;
        Ok(Statement::Insert { table, rows })
    }

    /// `count(` and its friends, only at the start of an item.
    fn aggregate(&self) -> Option<Aggregate> {
        let bracket = self.tokens.get(self.next + 1)?;
        let of = match self.peek()? {
            Token::Word(w) if w == "count" => Aggregate::Count,
            Token::Word(w) if w == "sum" => Aggregate::Sum,
            Token::Word(w) if w == "min" => Aggregate::Min,
            Token::Word(w) if w == "max" => Aggregate::Max,
            _ => return None,
        };
        (bracket.0 == Token::Symbol("(")).then_some(of)
    }

    fn item(&mut self, sql: &str) -> Result<Item, SyntaxError> {
        if self.symbol("*") {
            return Ok(Item::Star);
        }
        if let Some(of) = self.aggregate() {
            self.next += 2;
            let column = if self.symbol("*") {
                None
            } else {
                Some(self.name()?)
            };
            self.need_symbol(")")?;
            return Ok(Item::Aggregate { of, column });
        }
        let first = self.next;
        let expr = self.expression()?;
        let name = match &expr {
            Expr::Column(column) => column.clone(),
            _ => sql[self.tokens[first].1..self.tokens[self.next - 1].2].to_string(),
        };
        Ok(Item::Expr { expr, name })
    }

    fn select(&mut self, sql: &str) -> Result<Statement, SyntaxError> {
        let mut items = vec![self.item(sql)?];
        while self.symbol(",") {
            items.push(self.item(sql)?);
        }
        self.need_word("from")?;
        let table = self.name()?;
        let mut filter = None;
        if self.word("where") {
            filter = Some(self.expression()?);
        }
        let mut group_by = None;
        if self.word("group") {
            self.need_word("by")?;
            group_by = Some(self.name()?);
        }
        let mut order_by = None;
        if self.word("order") {
            self.need_word("by")?;
            let column = self.name()?;
            let descending = self.word("desc");
            if !descending {
                self.word("asc");
            }
            order_by = Some(OrderBy { column, descending });
        }
        let mut limit = None;
        if self.word("limit") {
            let Some(&Token::Number(n)) = self.peek() else {
                return self.fail();
            };
            self.next += 1;
            limit = Some(n.unsigned_abs());
        }
        Ok(Statement::Select(Select {
            items,
            table,
            filter,
            group_by,
            order_by,
            limit,
        }))
    }
}

/// The tree of one statement, or the first mistake in it.
pub fn parse(sql: &str) -> Result<Statement, SyntaxError> {
    let mut p = Parser {
        tokens: lex(sql)?,
        next: 0,
        length: sql.len(),
        open: 0,
    };
    let statement = if p.word("create") {
        p.create()?
    } else if p.word("insert") {
        p.insert()?
    } else if p.word("select") {
        p.select(sql)?
    } else {
        return p.fail();
    };
    p.symbol(";");
    if p.peek().is_some() {
        return p.fail();
    }
    Ok(statement)
}
