//! Splits SQL text into tokens. Unquoted words are lower-cased (PostgreSQL folds identifiers and keywords to lower case); `--` and
//! `/* */` comments are skipped.

use crate::common::exception::{Exception, ExceptionType, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    /// An unquoted word: a keyword or an identifier, lower-cased.
    Word(String),
    /// A `"quoted identifier"`, exactly as written.
    Quoted(String),
    /// A number as written (`42`, `1.5`, `1e3`).
    Number(String),
    /// A `'string'` with `''` unescaped.
    Str(String),
    /// An operator or punctuation: `+ - * / % = < > <= >= <> != || , ( ) ; . ::`.
    Symbol(&'static str),
}

pub fn parse_error(msg: impl std::fmt::Display) -> Exception {
    Exception::new(ExceptionType::Invalid, format!("Query failed to parse: {msg}"))
}

/// Splits `sql` into tokens. Whitespace and comments are skipped, words are lower-cased, `'strings'` use `''` for a quote, and a two-character
/// symbol wins over its first character. Text that is no token (an unterminated string or quoted identifier, an unknown character) is an error.
pub fn tokenize(sql: &str) -> Result<Vec<Token>> {
    // @begin 3d-04
    let chars: Vec<char> = sql.chars().collect();
    let mut tokens = vec![];
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '-' && chars.get(i + 1) == Some(&'-') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$') {
                i += 1;
            }
            tokens.push(Token::Word(chars[start..i].iter().collect::<String>().to_lowercase()));
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len() && chars[i] == '.' {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let mut j = i + 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    i = j;
                }
            }
            tokens.push(Token::Number(chars[start..i].iter().collect()));
        } else if c == '\'' {
            i += 1;
            let mut s = String::new();
            loop {
                match chars.get(i) {
                    None => return Err(parse_error("unterminated quoted string")),
                    Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                        s.push('\'');
                        i += 2;
                    }
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(&ch) => {
                        s.push(ch);
                        i += 1;
                    }
                }
            }
            tokens.push(Token::Str(s));
        } else if c == '"' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            if i >= chars.len() {
                return Err(parse_error("unterminated quoted identifier"));
            }
            tokens.push(Token::Quoted(chars[start..i].iter().collect()));
            i += 1;
        } else {
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            let sym = match two.as_str() {
                "<=" => Some("<="),
                ">=" => Some(">="),
                "<>" => Some("<>"),
                "!=" => Some("!="),
                "||" => Some("||"),
                "::" => Some("::"),
                "==" => Some("=="),
                _ => None,
            };
            if let Some(s) = sym {
                tokens.push(Token::Symbol(s));
                i += 2;
                continue;
            }
            let s = match c {
                '+' => "+",
                '-' => "-",
                '*' => "*",
                '/' => "/",
                '%' => "%",
                '=' => "=",
                '<' => "<",
                '>' => ">",
                ',' => ",",
                '(' => "(",
                ')' => ")",
                ';' => ";",
                '.' => ".",
                '[' => "[",
                ']' => "]",
                // @begin 3i-07
                '?' => "?",
                // @end
                other => return Err(parse_error(format!("syntax error at or near \"{other}\""))),
            };
            tokens.push(Token::Symbol(s));
            i += 1;
        }
    }
    Ok(tokens)
    //~ todo!("3d-04: walk the characters: skip whitespace and comments; words (lower-cased), quoted names, numbers, 'strings' with '' for a quote, two-character symbols before one-character ones; anything else is a parse error")
    // @end
}
