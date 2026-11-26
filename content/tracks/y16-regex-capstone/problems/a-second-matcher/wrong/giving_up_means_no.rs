use std::cell::Cell;

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
    if parser.pos < parser.chars.len() {
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
            '[' => self.class(start),
            '\\' => Ok(Ast::Class(match self.escape(start)? {
                Item::One(c) => Class::literal(c),
                Item::Shorthand(ranges) => Class {
                    negated: false,
                    ranges: ranges.to_vec(),
                },
            })),
            other => Ok(Ast::Class(Class::literal(other))),
        }
    }

    /// What follows a backslash; `at` is where the backslash is.
    fn escape(&mut self, at: usize) -> Result<Item, ParseError> {
        let c = self.bump().ok_or(ParseError::TrailingEscape(at))?;
        Ok(match c {
            'd' => Item::Shorthand(&[('0', '9')]),
            'w' => Item::Shorthand(&[('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')]),
            's' => Item::Shorthand(&[(' ', ' '), ('\t', '\r')]),
            'n' => Item::One('\n'),
            't' => Item::One('\t'),
            'r' => Item::One('\r'),
            c if c.is_ascii_alphanumeric() => return Err(ParseError::BadEscape(at)),
            c => Item::One(c),
        })
    }

    /// The rest of a bracket set whose `[` is at `start`.
    fn class(&mut self, start: usize) -> Result<Ast, ParseError> {
        let negated = self.eat('^');
        let mut ranges = Vec::new();
        loop {
            let at = self.pos;
            if self.eat(']') {
                return Ok(Ast::Class(Class { negated, ranges }));
            }
            let low = self.item(start)?;
            let is_range = self.peek() == Some('-')
                && !matches!(self.chars.get(self.pos + 1), None | Some(']'));
            if !is_range {
                match low {
                    Item::One(c) => ranges.push((c, c)),
                    Item::Shorthand(more) => ranges.extend_from_slice(more),
                }
                continue;
            }
            self.pos += 1;
            match (low, self.item(start)?) {
                (Item::One(lo), Item::One(hi)) if lo <= hi => ranges.push((lo, hi)),
                _ => return Err(ParseError::BadRange(at)),
            }
        }
    }

    /// One character or escape inside brackets.
    fn item(&mut self, start: usize) -> Result<Item, ParseError> {
        let at = self.pos;
        match self.bump() {
            None => Err(ParseError::UnclosedClass(start)),
            Some('\\') => self.escape(at),
            Some(c) => Ok(Item::One(c)),
        }
    }
}

enum Item {
    One(char),
    Shorthand(&'static [(char, char)]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepLimit;

type Answer = Result<bool, StepLimit>;
/// "The pattern so far has reached this position: can everything after it succeed?"
type Rest<'a> = &'a dyn Fn(usize) -> Answer;

struct Backtracker {
    text: Vec<char>,
    steps_left: Cell<u64>,
}

impl Backtracker {
    fn new(text: &str, limit: u64) -> Backtracker {
        Backtracker {
            text: text.chars().collect(),
            steps_left: Cell::new(limit),
        }
    }

    fn run(&self, ast: &Ast, at: usize, rest: Rest) -> Answer {
        let left = self.steps_left.get().checked_sub(1).ok_or(StepLimit)?;
        self.steps_left.set(left);
        let here = self.text.get(at).copied();
        match ast {
            Ast::Empty => rest(at),
            Ast::Class(class) => match here {
                Some(c) if class.matches(c) => rest(at + 1),
                _ => Ok(false),
            },
            Ast::Any => match here {
                Some(_) => rest(at + 1),
                None => Ok(false),
            },
            Ast::Start if at == 0 => rest(at),
            Ast::End if at == self.text.len() => rest(at),
            Ast::Start | Ast::End => Ok(false),
            Ast::Cat(items) => self.sequence(items, at, rest),
            Ast::Alt(branches) => {
                for branch in branches {
                    if self.run(branch, at, rest)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Ast::Star(inner) => self.repeat(inner, at, rest),
            Ast::Plus(inner) => self.run(inner, at, &|next| self.repeat(inner, next, rest)),
            Ast::Opt(inner) => Ok(self.run(inner, at, rest)? || rest(at)?),
        }
    }

    fn sequence(&self, items: &[Ast], at: usize, rest: Rest) -> Answer {
        match items.split_first() {
            None => rest(at),
            Some((first, tail)) => self.run(first, at, &|next| self.sequence(tail, next, rest)),
        }
    }

    /// One more round first, then none. A round that read nothing is refused, or `(a*)*` never ends.
    fn repeat(&self, inner: &Ast, at: usize, rest: Rest) -> Answer {
        let again = |next: usize| {
            if next == at {
                Ok(false)
            } else {
                self.repeat(inner, next, rest)
            }
        };
        Ok(self.run(inner, at, &again)? || rest(at)?)
    }
}

/// Does the pattern match the whole text? Tries one way at a time.
pub fn backtrack_is_match(ast: &Ast, text: &str, limit: u64) -> Result<bool, StepLimit> {
    let search = Backtracker::new(text, limit);
    let end = search.text.len();
    Ok(search.run(ast, 0, &|at| Ok(at == end)).unwrap_or(false))
}

/// The leftmost match and the longest from there, trying one way at a time.
pub fn backtrack_find(
    ast: &Ast,
    text: &str,
    limit: u64,
) -> Result<Option<(usize, usize)>, StepLimit> {
    let search = Backtracker::new(text, limit);
    for start in 0..=search.text.len() {
        let longest = Cell::new(None);
        // Always answering "no" makes the search visit every way there is.
        search.run(ast, start, &|end| {
            longest.set(longest.get().max(Some(end)));
            Ok(false)
        })?;
        if let Some(end) = longest.get() {
            return Ok(Some((start, end)));
        }
    }
    Ok(None)
}
