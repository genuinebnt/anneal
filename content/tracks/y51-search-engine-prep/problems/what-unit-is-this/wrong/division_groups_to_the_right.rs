use std::collections::BTreeMap;

/// The most brackets that may be open at once.
pub const MAX_OPEN: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitError {
    BadChar(usize),
    Unexpected(usize),
    UnexpectedEnd,
    TooDeep(usize),
    TooBig,
}

type Powers = BTreeMap<String, i32>;

#[derive(Debug, PartialEq)]
enum Token {
    Name(String),
    Number(i32),
    Slash,
    Dot,
    Caret,
    Open,
    Close,
}

fn tokens(text: &str) -> Result<Vec<(Token, usize)>, UnitError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let start = at;
        let c = chars[at];
        at += 1;
        let token = match c {
            ' ' => continue,
            '/' => Token::Slash,
            '.' => Token::Dot,
            '^' => Token::Caret,
            '(' => Token::Open,
            ')' => Token::Close,
            c if c.is_ascii_alphabetic() => {
                while chars.get(at).is_some_and(char::is_ascii_alphabetic) {
                    at += 1;
                }
                Token::Name(chars[start..at].iter().collect())
            }
            c if c.is_ascii_digit()
                || (c == '-' && chars.get(at).is_some_and(char::is_ascii_digit)) =>
            {
                while chars.get(at).is_some_and(char::is_ascii_digit) {
                    at += 1;
                }
                let digits: String = chars[start..at].iter().collect();
                Token::Number(digits.parse().map_err(|_| UnitError::TooBig)?)
            }
            _ => return Err(UnitError::BadChar(start)),
        };
        out.push((token, start));
    }
    Ok(out)
}

/// Adds `sign` times the powers of `more` into `into`.
fn combine(into: &mut Powers, more: Powers, sign: i32) -> Result<(), UnitError> {
    for (name, power) in more {
        let old = into.get(&name).copied().unwrap_or(0);
        let new = if sign > 0 {
            old.checked_add(power)
        } else {
            old.checked_sub(power)
        };
        match new.ok_or(UnitError::TooBig)? {
            0 => into.remove(&name),
            new => into.insert(name, new),
        };
    }
    Ok(())
}

struct Parser {
    tokens: Vec<(Token, usize)>,
    at: usize,
    open: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|(t, _)| t)
    }

    /// The error for the token at hand, which cannot be used.
    fn stuck(&self) -> UnitError {
        match self.tokens.get(self.at) {
            Some(&(_, pos)) => UnitError::Unexpected(pos),
            None => UnitError::UnexpectedEnd,
        }
    }

    fn ratio(&mut self) -> Result<Powers, UnitError> {
        let mut value = self.product()?;
        if self.peek() == Some(&Token::Slash) {
            self.at += 1;
            let divisor = self.ratio()?;
            combine(&mut value, divisor, -1)?;
        }
        Ok(value)
    }

    fn product(&mut self) -> Result<Powers, UnitError> {
        let mut value = self.factor()?;
        loop {
            match self.peek() {
                Some(Token::Dot) => self.at += 1,
                Some(Token::Name(_) | Token::Number(1) | Token::Open) => {}
                _ => return Ok(value),
            }
            let next = self.factor()?;
            combine(&mut value, next, 1)?;
        }
    }

    fn factor(&mut self) -> Result<Powers, UnitError> {
        let mut value = match self.peek() {
            Some(Token::Name(name)) => {
                let value = Powers::from([(name.clone(), 1)]);
                self.at += 1;
                value
            }
            Some(Token::Number(1)) => {
                self.at += 1;
                Powers::new()
            }
            Some(Token::Open) => {
                if self.open == MAX_OPEN {
                    return Err(UnitError::TooDeep(self.tokens[self.at].1));
                }
                self.open += 1;
                self.at += 1;
                let inside = self.ratio()?;
                if self.peek() != Some(&Token::Close) {
                    return Err(self.stuck());
                }
                self.at += 1;
                self.open -= 1;
                inside
            }
            _ => return Err(self.stuck()),
        };
        if self.peek() == Some(&Token::Caret) {
            self.at += 1;
            let Some(&Token::Number(times)) = self.peek() else {
                return Err(self.stuck());
            };
            self.at += 1;
            for power in value.values_mut() {
                *power = power.checked_mul(times).ok_or(UnitError::TooBig)?;
            }
            value.retain(|_, power| *power != 0);
        }
        Ok(value)
    }
}

/// Each unit name with its power; names with power 0 are left out.
pub fn parse(text: &str) -> Result<BTreeMap<String, i32>, UnitError> {
    let mut parser = Parser {
        tokens: tokens(text)?,
        at: 0,
        open: 0,
    };
    let value = parser.ratio()?;
    if parser.peek().is_some() {
        return Err(parser.stuck());
    }
    Ok(value)
}
