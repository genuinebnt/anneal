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

// ----- Step 1, given: tokens. -----

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

const SYMBOLS: [&str; 15] = [
    "<=", ">=", "<>", "!=", "(", ")", ",", "*", "+", "-", "/", "=", "<", ">", ";",
];

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

// ----- Step 2, yours. -----

/// The tree of one statement, or the first mistake in it.
pub fn parse(sql: &str) -> Result<Statement, SyntaxError> {
    let _ = sql;
    todo!()
}
