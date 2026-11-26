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
    let mut parser = Parser {
        chars: pattern.chars().collect(),
        pos: 0,
        open: 0,
    };
    let ast = parser.alt()?;
    // `alt` stops early only at a `)` that no `(` owns.
    if parser.pos > parser.chars.len() {
        return Err(ParseError::UnmatchedClose(parser.pos));
    }
    Ok(ast)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    open: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        self.pos += usize::from(c.is_some());
        c
    }

    fn eat(&mut self, want: char) -> bool {
        let hit = self.peek() == Some(want);
        self.pos += usize::from(hit);
        hit
    }

    /// alt := cat ('|' cat)*
    fn alt(&mut self) -> Result<Ast, ParseError> {
        let mut branches = vec![self.cat()?];
        while self.eat('|') {
            branches.push(self.cat()?);
        }
        Ok(match branches.len() {
            1 => branches.remove(0),
            _ => Ast::Alt(branches),
        })
    }

    /// cat := repeat*
    fn cat(&mut self) -> Result<Ast, ParseError> {
        let mut items = Vec::new();
        while !matches!(self.peek(), None | Some('|' | ')')) {
            items.push(self.repeat()?);
        }
        Ok(match items.len() {
            0 => Ast::Empty,
            1 => items.remove(0),
            _ => Ast::Cat(items),
        })
    }

    /// repeat := atom ('*' | '+' | '?')*
    fn repeat(&mut self) -> Result<Ast, ParseError> {
        let mut node = self.atom()?;
        let mut stacked = 0;
        while let Some(quantifier @ ('*' | '+' | '?')) = self.peek() {
            if stacked == MAX_STACKED {
                return Err(ParseError::TooDeep(self.pos));
            }
            stacked += 1;
            self.pos += 1;
            let inner = Box::new(node);
            node = match quantifier {
                '*' => Ast::Star(inner),
                '+' => Ast::Plus(inner),
                _ => Ast::Opt(inner),
            };
        }
        Ok(node)
    }

    fn atom(&mut self) -> Result<Ast, ParseError> {
        let start = self.pos;
        let Some(c) = self.bump() else {
            return Ok(Ast::Empty);
        };
        match c {
            '(' => {
                if self.open == MAX_OPEN_GROUPS {
                    return Err(ParseError::TooDeep(start));
                }
                self.open += 1;
                let inner = self.alt()?;
                if !self.eat(')') {
                    return Err(ParseError::UnclosedGroup(start));
                }
                self.open -= 1;
                Ok(inner)
            }
            '*' | '+' | '?' => Err(ParseError::NothingToRepeat(start)),
            '.' => Ok(Ast::Any),
            '^' => Ok(Ast::Start),
            '$' => Ok(Ast::End),
            other => Ok(Ast::Class(Class::literal(other))),
        }
    }
}
