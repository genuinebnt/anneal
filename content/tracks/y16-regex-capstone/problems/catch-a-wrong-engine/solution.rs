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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Read one character in the class, then go on.
    Char(Class, usize),
    /// Read any one character, then go on.
    Any(usize),
    /// Go to both, reading nothing.
    Split(usize, usize),
    /// Go on, reading nothing, only at the start of the text.
    Start(usize),
    /// Go on, reading nothing, only at the end of the text.
    End(usize),
    Match,
}

#[derive(Debug, Clone)]
pub struct Nfa {
    pub states: Vec<State>,
    pub start: usize,
}

/// The accepting state is always the first one.
pub const MATCH: usize = 0;

pub fn compile(ast: &Ast) -> Nfa {
    let mut states = vec![State::Match];
    let start = emit(&mut states, ast, MATCH);
    Nfa { states, start }
}

fn push(states: &mut Vec<State>, state: State) -> usize {
    states.push(state);
    states.len() - 1
}

/// Builds the states for `ast` so that they carry on at `next`, and returns the entry.
fn emit(states: &mut Vec<State>, ast: &Ast, next: usize) -> usize {
    match ast {
        Ast::Empty => next,
        Ast::Class(class) => push(states, State::Char(class.clone(), next)),
        Ast::Any => push(states, State::Any(next)),
        Ast::Start => push(states, State::Start(next)),
        Ast::End => push(states, State::End(next)),
        Ast::Cat(items) => items
            .iter()
            .rev()
            .fold(next, |after, item| emit(states, item, after)),
        Ast::Alt(branches) => {
            let entries: Vec<usize> = branches.iter().map(|b| emit(states, b, next)).collect();
            entries
                .into_iter()
                .rev()
                .reduce(|others, entry| push(states, State::Split(entry, others)))
                .unwrap_or(next)
        }
        Ast::Star(inner) => {
            let split = push(states, State::Match); // filled in below
            let body = emit(states, inner, split);
            states[split] = State::Split(body, next);
            split
        }
        Ast::Plus(inner) => {
            let split = push(states, State::Match); // filled in below
            let body = emit(states, inner, split);
            states[split] = State::Split(body, next);
            body // enter through the body: at least once
        }
        Ast::Opt(inner) => {
            let body = emit(states, inner, next);
            push(states, State::Split(body, next))
        }
    }
}

/// The seeds and every state reachable from them without reading, each once.
fn closure(nfa: &Nfa, mut work: Vec<usize>, at_start: bool, at_end: bool) -> Vec<usize> {
    let mut seen = vec![false; nfa.states.len()];
    let mut live = Vec::new();
    while let Some(id) = work.pop() {
        if std::mem::replace(&mut seen[id], true) {
            continue;
        }
        live.push(id);
        match nfa.states[id] {
            State::Split(a, b) => work.extend([a, b]),
            State::Start(next) if at_start => work.push(next),
            State::End(next) if at_end => work.push(next),
            _ => {}
        }
    }
    live
}

/// Reads `c` in every live state. `at_end` says whether the text ends after `c`.
fn advance(nfa: &Nfa, live: &[usize], c: char, at_end: bool) -> Vec<usize> {
    let seeds = live
        .iter()
        .filter_map(|&id| match &nfa.states[id] {
            State::Char(class, next) if class.matches(c) => Some(*next),
            State::Any(next) => Some(*next),
            _ => None,
        })
        .collect();
    closure(nfa, seeds, false, at_end)
}

/// Does the machine accept the **whole** text?
pub fn is_match(nfa: &Nfa, text: &str) -> bool {
    let mut chars = text.chars().peekable();
    let mut live = closure(nfa, vec![nfa.start], true, text.is_empty());
    while let Some(c) = chars.next() {
        if live.is_empty() {
            return false;
        }
        live = advance(nfa, &live, c, chars.peek().is_none());
    }
    live.contains(&MATCH)
}

pub struct Regex {
    nfa: Nfa,
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, ParseError> {
        Ok(Regex {
            nfa: compile(&parse(pattern)?),
        })
    }

    /// Does the pattern match the **whole** text?
    pub fn is_match(&self, text: &str) -> bool {
        is_match(&self.nfa, text)
    }

    /// The leftmost match and, from there, the longest one: `(start, end)` in characters.
    pub fn find(&self, text: &str) -> Option<(usize, usize)> {
        let chars: Vec<char> = text.chars().collect();
        (0..=chars.len()).find_map(|start| Some((start, self.longest_from(&chars, start)?)))
    }

    /// The last position at which a match that began at `start` was accepted.
    fn longest_from(&self, chars: &[char], start: usize) -> Option<usize> {
        let n = chars.len();
        let mut live = closure(&self.nfa, vec![self.nfa.start], start == 0, start == n);
        let mut last = None;
        for at in start..=n {
            if live.contains(&MATCH) {
                last = Some(at);
            }
            if at == n || live.is_empty() {
                break;
            }
            live = advance(&self.nfa, &live, chars[at], at + 1 == n);
        }
        last
    }
}

/// The trusted engine of the earlier steps.
pub fn reference_find(pattern: &str, text: &str) -> Result<Option<(usize, usize)>, ParseError> {
    Ok(Regex::new(pattern)?.find(text))
}

pub const MAX_CALLS: usize = 20_000;
pub const MAX_PATTERN: usize = 60;
pub const MAX_TEXT: usize = 12;

/// xorshift: small, and the same sequence every run.
struct Random(u64);

impl Random {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }

    fn pick<'a>(&mut self, list: &[&'a str]) -> &'a str {
        list[self.below(list.len() as u64) as usize]
    }
}

const ATOMS: &[&str] = &[
    "a", "a", "a", "b", "b", "c", "é", "_", " ", ".", "\\n", "[ab]", "[^a]", "[a-b]", "\\w", "\\d",
    "\\s", "^", "$", "",
];
const LETTERS: &[&str] = &["a", "a", "a", "b", "b", "c", "é", "_", "1", " ", "\n"];

/// Pattern text built the way the grammar is built. Groups keep every piece safe to combine.
fn random_pattern(random: &mut Random, depth: u32) -> String {
    if depth == 0 || random.below(4) == 0 {
        let atom = random.pick(ATOMS);
        if !atom.is_empty() && random.below(3) == 0 {
            return format!("{atom}{}", random.pick(&["*", "+", "?"]));
        }
        return atom.to_string();
    }
    let a = random_pattern(random, depth - 1);
    let b = random_pattern(random, depth - 1);
    match random.below(8) {
        0 | 1 => format!("{a}{b}"),
        2 => format!("({a}|{b})"),
        3 => format!("({a}|{b}|{})", random_pattern(random, depth - 1)),
        4 => format!("({a})*"),
        5 => format!("({a})+"),
        6 => format!("({a})?"),
        _ => format!("{a}|{b}"),
    }
}

fn random_text(random: &mut Random) -> String {
    let length = random.below(9);
    (0..length).map(|_| random.pick(LETTERS)).collect()
}

/// A pattern and a text on which `candidate` gives a wrong answer, if one can be found.
pub fn find_difference(
    candidate: &dyn Fn(&str, &str) -> Option<(usize, usize)>,
) -> Option<(String, String)> {
    let mut random = Random(0x9E37_79B9_7F4A_7C15);
    let mut calls = 0;
    while calls < MAX_CALLS {
        let pattern = random_pattern(&mut random, 3);
        if pattern.chars().count() > MAX_PATTERN {
            continue;
        }
        let Ok(regex) = Regex::new(&pattern) else {
            continue;
        };
        for _ in 0..8 {
            if calls == MAX_CALLS {
                break;
            }
            let text = random_text(&mut random);
            calls += 1;
            if candidate(&pattern, &text) != regex.find(&text) {
                return Some((pattern, text));
            }
        }
    }
    None
}
