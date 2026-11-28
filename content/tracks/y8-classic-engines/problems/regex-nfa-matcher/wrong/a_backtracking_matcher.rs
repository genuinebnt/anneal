#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A `(` without its `)`, or a `)` without its `(`.
    UnbalancedParen,
    /// A `*`, `+` or `?` with nothing before it (at the start, after `(` or `|`, or directly after another quantifier).
    NothingToRepeat,
    /// A `\` at the very end of the pattern.
    DanglingEscape,
}

enum Node {
    Empty,
    Char(char),
    Any,
    Cat(Vec<Node>),
    Alt(Vec<Node>),
    Star(Box<Node>),
    Plus(Box<Node>),
    Quest(Box<Node>),
}

struct Parser {
    s: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.s.get(self.pos).copied()
    }

    fn alt(&mut self) -> Result<Node, ParseError> {
        let mut branches = vec![self.cat()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            branches.push(self.cat()?);
        }
        Ok(if branches.len() == 1 { branches.pop().unwrap() } else { Node::Alt(branches) })
    }

    fn cat(&mut self) -> Result<Node, ParseError> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            items.push(self.repeat()?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap(),
            _ => Node::Cat(items),
        })
    }

    fn repeat(&mut self) -> Result<Node, ParseError> {
        let atom = self.atom()?;
        let node = match self.peek() {
            Some('*') => Node::Star(Box::new(atom)),
            Some('+') => Node::Plus(Box::new(atom)),
            Some('?') => Node::Quest(Box::new(atom)),
            _ => return Ok(atom),
        };
        self.pos += 1;
        Ok(node) // a second quantifier is caught by `atom`, which refuses to start with one
    }

    fn atom(&mut self) -> Result<Node, ParseError> {
        let c = self.peek().unwrap();
        self.pos += 1;
        match c {
            '(' => {
                let inner = self.alt()?;
                if self.peek() != Some(')') {
                    return Err(ParseError::UnbalancedParen);
                }
                self.pos += 1;
                Ok(inner)
            }
            '.' => Ok(Node::Any),
            '*' | '+' | '?' => Err(ParseError::NothingToRepeat),
            '\\' => match self.peek() {
                Some(e) => {
                    self.pos += 1;
                    Ok(Node::Char(e))
                }
                None => Err(ParseError::DanglingEscape),
            },
            other => Ok(Node::Char(other)),
        }
    }
}

// WRONG: a backtracking matcher. It tries one way at a time and goes back on failure,
// so a pattern with many ways to split the text takes exponential time.
fn go(node: &Node, s: &[char], pos: usize, k: &mut dyn FnMut(usize) -> bool) -> bool {
    match node {
        Node::Empty => k(pos),
        Node::Char(c) => pos < s.len() && s[pos] == *c && k(pos + 1),
        Node::Any => pos < s.len() && k(pos + 1),
        Node::Cat(items) => cat(items, s, pos, k),
        Node::Alt(bs) => bs.iter().any(|b| go(b, s, pos, k)),
        Node::Star(n) => star(n, s, pos, k),
        Node::Plus(n) => go(n, s, pos, &mut |p| star(n, s, p, k)),
        Node::Quest(n) => go(n, s, pos, k) || k(pos),
    }
}

fn cat(items: &[Node], s: &[char], pos: usize, k: &mut dyn FnMut(usize) -> bool) -> bool {
    match items.split_first() {
        None => k(pos),
        Some((first, rest)) => go(first, s, pos, &mut |p| cat(rest, s, p, k)),
    }
}

fn star(n: &Node, s: &[char], pos: usize, k: &mut dyn FnMut(usize) -> bool) -> bool {
    // another round only if it moves forward, so empty loops end
    go(n, s, pos, &mut |p| p > pos && star(n, s, p, k)) || k(pos)
}

pub struct Regex {
    node: Node,
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, ParseError> {
        let mut p = Parser {
            s: pattern.chars().collect(),
            pos: 0,
        };
        let node = p.alt()?;
        if p.pos < p.s.len() {
            return Err(ParseError::UnbalancedParen);
        }
        Ok(Regex { node })
    }

    pub fn is_match(&self, text: &str) -> bool {
        let s: Vec<char> = text.chars().collect();
        go(&self.node, &s, 0, &mut |p| p == s.len())
    }

    pub fn find(&self, text: &str) -> Option<(usize, usize)> {
        let s: Vec<char> = text.chars().collect();
        for start in 0..=s.len() {
            let mut best = None;
            go(&self.node, &s, start, &mut |p| {
                best = best.max(Some(p));
                false
            });
            if let Some(end) = best {
                return Some((start, end));
            }
        }
        None
    }
}
