#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Word(String),
    Number(u64),
    Equals,
    Semi,
    Comma,
    OpenBrace,
    CloseBrace,
    OpenBracket,
    CloseBracket,
    /// Any other character.
    Other(char),
}

/// Cuts the text into tokens, each with its line (from 1).
pub fn tokens(text: &str) -> Vec<(Token, u32)> {
    let mut out = Vec::new();
    let mut line = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let token = match c {
            '\n' => {
                line += 1;
                continue;
            }
            c if c.is_whitespace() => continue,
            '=' => Token::Equals,
            ';' => Token::Semi,
            ',' => Token::Comma,
            '{' => Token::OpenBrace,
            '}' => Token::CloseBrace,
            '[' => Token::OpenBracket,
            ']' => Token::CloseBracket,
            c if c.is_ascii_alphanumeric() => {
                let mut word = c.to_string();
                while let Some(&next) = chars.peek() {
                    if !next.is_ascii_alphanumeric() {
                        break;
                    }
                    word.push(next);
                    chars.next();
                }
                match word.parse() {
                    Ok(n) => Token::Number(n),
                    Err(_) => Token::Word(word),
                }
            }
            other => Token::Other(other),
        };
        out.push((token, line));
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Word(String),
    Number(u64),
    List(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Setting(String, Value),
    Section(String, Vec<Item>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    ExpectedName,
    ExpectedEqualsOrBrace,
    ExpectedValue,
    ExpectedCommaOrBracket,
    ExpectedSemi,
    StrayClose,
    Unclosed,
    TooDeep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mistake {
    pub line: u32,
    pub problem: Problem,
}

/// Sections inside sections, and lists inside lists.
pub const MAX_DEPTH: usize = 32;

/// Reads the whole file: every item that is whole, and every mistake.
pub fn read(text: &str) -> (Vec<Item>, Vec<Mistake>) {
    todo!()
}
