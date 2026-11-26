#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Token<'a> {
    Ident(&'a str),
    Number(&'a str),
    Punct(char),
}

pub struct Tokenizer<'a> {
    rest: &'a str,
}

impl<'a> Tokenizer<'a> {
    pub fn new(src: &'a str) -> Self {
        Tokenizer { rest: src }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        let s = self.rest.trim_start();
        let c = s.chars().next()?;
        let len = if is_ident_start(c) {
            s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(s.len())
        } else if c.is_ascii_digit() {
            s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len())
        } else {
            c.len_utf8()
        };
        let (text, rest) = s.split_at(len);
        self.rest = rest;
        Some(if is_ident_start(c) {
            Token::Ident(text)
        } else if c.is_ascii_digit() {
            Token::Number(text)
        } else {
            Token::Punct(c)
        })
    }
}

pub fn longest_ident<'a, I: IntoIterator<Item = Token<'a>>>(tokens: I) -> Option<&'a str> {
    tokens
        .into_iter()
        .filter_map(|t| match t {
            Token::Ident(s) => Some(s),
            _ => None,
        })
        .reduce(|best, s| if s.len() > best.len() { s } else { best })
}
