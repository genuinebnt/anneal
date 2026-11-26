use std::marker::PhantomData;

pub trait Tokenize {
    /// Splits the next token off `input`, returning (token, remainder).
    fn next_token(input: &str) -> Option<(&str, &str)>;
}

pub struct Whitespace;

pub struct Comma;

impl Tokenize for Whitespace {
    fn next_token(input: &str) -> Option<(&str, &str)> {
        let input = input.trim_start();
        if input.is_empty() {
            return None;
        }
        let end = input.find(char::is_whitespace).unwrap_or(input.len());
        Some(input.split_at(end))
    }
}

impl Tokenize for Comma {
    fn next_token(input: &str) -> Option<(&str, &str)> {
        let input = input.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
        if input.is_empty() {
            return None;
        }
        let end = input.find(',').unwrap_or(input.len());
        let (token, rest) = input.split_at(end);
        Some((token.trim_end(), rest))
    }
}

pub struct Parser<'a, T> {
    remaining: &'a str,
    current: Option<&'a str>,
    // T only picks the tokenizer; PhantomData records it without storing one.
    _tokenizer: PhantomData<T>,
}

impl<'a, T: Tokenize> Parser<'a, T> {
    pub fn new(source: &'a str) -> Self {
        Parser {
            remaining: source,
            current: None,
            _tokenizer: PhantomData,
        }
    }

    pub fn advance(&mut self) -> Option<&'a str> {
        self.current = match T::next_token(self.remaining) {
            Some((token, rest)) => {
                self.remaining = rest;
                Some(token)
            }
            None => None,
        };
        self.current
    }

    pub fn current(&self) -> Option<&'a str> {
        self.current
    }
}
