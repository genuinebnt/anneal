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

enum Inst {
    Char(char),
    Any,
    Split(usize, usize),
    Jmp(usize),
    Match,
}

fn emit(node: &Node, prog: &mut Vec<Inst>) {
    match node {
        Node::Empty => {}
        Node::Char(c) => prog.push(Inst::Char(*c)),
        Node::Any => prog.push(Inst::Any),
        Node::Cat(items) => {
            for n in items {
                emit(n, prog);
            }
        }
        Node::Alt(branches) => {
            let mut jumps = Vec::new();
            for (i, b) in branches.iter().enumerate() {
                if i + 1 < branches.len() {
                    let split = prog.len();
                    prog.push(Inst::Split(0, 0));
                    emit(b, prog);
                    jumps.push(prog.len());
                    prog.push(Inst::Jmp(0));
                    prog[split] = Inst::Split(split + 1, prog.len());
                } else {
                    emit(b, prog);
                }
            }
            for j in jumps {
                prog[j] = Inst::Jmp(prog.len());
            }
        }
        Node::Star(inner) => {
            let split = prog.len();
            prog.push(Inst::Split(0, 0));
            emit(inner, prog);
            prog.push(Inst::Jmp(split));
            prog[split] = Inst::Split(split + 1, prog.len());
        }
        Node::Plus(inner) => {
            let start = prog.len();
            emit(inner, prog);
            prog.push(Inst::Split(start, prog.len() + 1));
        }
        Node::Quest(inner) => {
            let split = prog.len();
            prog.push(Inst::Split(0, 0));
            emit(inner, prog);
            prog[split] = Inst::Split(split + 1, prog.len());
        }
    }
}

pub struct Regex {
    prog: Vec<Inst>,
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, ParseError> {
        let mut p = Parser { s: pattern.chars().collect(), pos: 0 };
        let node = p.alt()?;
        if p.pos < p.s.len() {
            return Err(ParseError::UnbalancedParen); // a `)` with no `(`
        }
        let mut prog = Vec::new();
        emit(&node, &mut prog);
        prog.push(Inst::Match);
        Ok(Regex { prog })
    }

    /// Adds `pc` and everything reachable from it by empty moves to `list`. `seen` stops loops like `(a*)*`.
    fn add(&self, list: &mut Vec<usize>, seen: &mut [bool], pc: usize) {
        let mut stack = vec![pc];
        while let Some(pc) = stack.pop() {
            if seen[pc] {
                continue;
            }
            seen[pc] = true;
            match self.prog[pc] {
                Inst::Jmp(t) => stack.push(t),
                Inst::Split(a, b) => {
                    stack.push(b);
                    stack.push(a);
                }
                _ => list.push(pc),
            }
        }
    }

    /// The end of the longest match that starts exactly at `start`. Time O(text * program): every state is visited once per step.
    fn longest_from(&self, chars: &[char], start: usize) -> Option<usize> {
        let mut cur = Vec::new();
        self.add(&mut cur, &mut vec![false; self.prog.len()], 0);
        let mut best = None;
        let mut i = start;
        loop {
            if cur.iter().any(|&pc| matches!(self.prog[pc], Inst::Match)) {
                best = Some(i);
            }
            if i >= chars.len() || cur.is_empty() {
                break;
            }
            let c = chars[i];
            let mut next = Vec::new();
            let mut seen = vec![false; self.prog.len()];
            for &pc in &cur {
                match self.prog[pc] {
                    Inst::Char(x) if x == c => self.add(&mut next, &mut seen, pc + 1),
                    Inst::Any => self.add(&mut next, &mut seen, pc + 1),
                    _ => {}
                }
            }
            cur = next;
            i += 1;
        }
        best
    }

    pub fn is_match(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        self.longest_from(&chars, 0) == Some(chars.len())
    }

    pub fn find(&self, text: &str) -> Option<(usize, usize)> {
        let chars: Vec<char> = text.chars().collect();
        (0..=chars.len()).find_map(|s| self.longest_from(&chars, s).map(|e| (s, e)))
    }
}
