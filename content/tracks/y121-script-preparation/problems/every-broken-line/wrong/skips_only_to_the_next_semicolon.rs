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

struct Reader {
    tokens: Vec<(Token, u32)>,
    at: usize,
    mistakes: Vec<Mistake>,
}

impl Reader {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at).map(|(t, _)| t)
    }

    fn eat(&mut self, token: Token) -> bool {
        let hit = self.peek() == Some(&token);
        self.at += usize::from(hit);
        hit
    }

    /// The line of the current token, or of the last one at the end.
    fn line(&self) -> u32 {
        let here = self.tokens.get(self.at).or(self.tokens.last());
        here.map_or(1, |(_, line)| *line)
    }

    fn here(&self, problem: Problem) -> Mistake {
        Mistake {
            line: self.line(),
            problem,
        }
    }

    /// Items until a `}` or the end.
    fn items(&mut self, depth: usize) -> Vec<Item> {
        let mut out = Vec::new();
        while !matches!(self.peek(), None | Some(Token::CloseBrace)) {
            match self.item(depth) {
                Ok(item) => out.push(item),
                Err(mistake) => {
                    self.mistakes.push(mistake);
                    self.skip();
                }
            }
        }
        out
    }

    fn item(&mut self, depth: usize) -> Result<Item, Mistake> {
        let Some(Token::Word(name)) = self.peek().cloned() else {
            return Err(self.here(Problem::ExpectedName));
        };
        self.at += 1;
        if self.peek() == Some(&Token::OpenBrace) {
            if depth == MAX_DEPTH {
                return Err(self.here(Problem::TooDeep));
            }
            let opened = self.line();
            self.at += 1;
            let items = self.items(depth + 1);
            if !self.eat(Token::CloseBrace) {
                self.mistakes.push(Mistake {
                    line: opened,
                    problem: Problem::Unclosed,
                });
            }
            return Ok(Item::Section(name, items));
        }
        if !self.eat(Token::Equals) {
            return Err(self.here(Problem::ExpectedEqualsOrBrace));
        }
        let value = self.value(0)?;
        if !self.eat(Token::Semi) {
            return Err(self.here(Problem::ExpectedSemi));
        }
        Ok(Item::Setting(name, value))
    }

    fn value(&mut self, depth: usize) -> Result<Value, Mistake> {
        let value = match self.peek() {
            Some(Token::Word(w)) => Value::Word(w.clone()),
            Some(Token::Number(n)) => Value::Number(*n),
            Some(Token::OpenBracket) => return self.list(depth),
            _ => return Err(self.here(Problem::ExpectedValue)),
        };
        self.at += 1;
        Ok(value)
    }

    fn list(&mut self, depth: usize) -> Result<Value, Mistake> {
        if depth == MAX_DEPTH {
            return Err(self.here(Problem::TooDeep));
        }
        self.at += 1;
        let mut list = Vec::new();
        if self.eat(Token::CloseBracket) {
            return Ok(Value::List(list));
        }
        loop {
            list.push(self.value(depth + 1)?);
            if self.eat(Token::CloseBracket) {
                return Ok(Value::List(list));
            }
            if !self.eat(Token::Comma) {
                return Err(self.here(Problem::ExpectedCommaOrBracket));
            }
        }
    }

    /// Passes over the rest of a broken item, counting braces rather than recursing into them.
    fn skip(&mut self) {
        let mut open = 0usize;
        while let Some(token) = self.peek() {
            match token {
                Token::CloseBrace if open == 0 => return, // it closes the section we are in
                Token::CloseBrace => {
                    open -= 1;
                    if open == 0 {
                        self.at += 1;
                        return;
                    }
                }
                Token::Semi if open == 0 => {
                    self.at += 1;
                    return;
                }
                _ => {}
            }
            self.at += 1;
        }
    }
}

/// Reads the whole file: every item that is whole, and every mistake.
pub fn read(text: &str) -> (Vec<Item>, Vec<Mistake>) {
    let mut reader = Reader {
        tokens: tokens(text),
        at: 0,
        mistakes: Vec::new(),
    };
    let mut items = reader.items(0);
    while reader.peek().is_some() {
        // Only a `}` stops `items` early, and at the top there is nothing for it to close.
        reader.mistakes.push(reader.here(Problem::StrayClose));
        reader.at += 1;
        items.extend(reader.items(0));
    }
    reader.mistakes.sort_by_key(|m| m.line);
    (items, reader.mistakes)
}
