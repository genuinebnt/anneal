pub trait Pattern {
    fn start(&self) -> u32;
    fn step(&self, state: u32, c: char) -> Option<u32>;
    fn accepts(&self, state: u32) -> bool;
}

pub struct Rule<'a> {
    pub pattern: &'a dyn Pattern,
    pub skip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub rule: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexError {
    NoToken(usize),
}

pub fn lex(rules: &[Rule], text: &str) -> Result<Vec<Token>, LexError> {
    todo!()
}
