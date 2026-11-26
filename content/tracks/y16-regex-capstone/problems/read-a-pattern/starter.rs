/// One set of characters: a single letter is a set of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub negated: bool,
    /// Inclusive ranges, in any order.
    pub ranges: Vec<(char, char)>,
}

impl Class {
    pub fn literal(c: char) -> Class {
        Class {
            negated: false,
            ranges: vec![(c, c)],
        }
    }

    pub fn matches(&self, c: char) -> bool {
        self.ranges.iter().any(|&(lo, hi)| lo <= c && c <= hi) != self.negated
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ast {
    Empty,
    Class(Class),
    Any,
    Start,
    End,
    /// Two or more items, one after another.
    Cat(Vec<Ast>),
    /// Two or more branches.
    Alt(Vec<Ast>),
    Star(Box<Ast>),
    Plus(Box<Ast>),
    Opt(Box<Ast>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    UnmatchedClose(usize),
    UnclosedGroup(usize),
    NothingToRepeat(usize),
    TooDeep(usize),
    UnclosedClass(usize),
    BadRange(usize),
    BadEscape(usize),
    TrailingEscape(usize),
}

pub const MAX_OPEN_GROUPS: usize = 100;
pub const MAX_STACKED: usize = 3;

pub fn parse(pattern: &str) -> Result<Ast, ParseError> {
    todo!()
}
