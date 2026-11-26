//! CAPSTONE 03 (automata): a regex engine: recursive-descent parser -> Thompson NFA -> linear-time simulation, checked against a backtracker.
//! LEVEL: capstone
//! Use in: log/route/input matching without ReDoS, lexers, search tools, learning how grep/RE2/rust-regex work.   Run: cargo run --release --example regex_nfa_engine
#![allow(dead_code)]
//!
//! WHAT: a small regex dialect: literals, `.`, `*`, `+`, `?`, `|`, `( )`, classes `[a-z_]` / `[^...]`, anchors `^` `$`, escapes (`\d \w \s \n \t \.`).
//! WHY: the classic backtracking matcher (Perl, PCRE, Python `re`, JS) is exponential on patterns like `(a*)*b`; that is the ReDoS bug class.
//!      A Thompson NFA keeps ALL possible positions at once, so matching costs O(text * states) and can never blow up.
//! HOW (each stage is a plain function):
//!   parse    text -> `Ast`  recursive descent, one function per precedence level (alt < concat < repeat < atom); errors carry a char position
//!   compile  `Ast` -> `Nfa` states in a `Vec`, ids are indices; built back-to-front so every fragment is told its `next` state (no patch lists)
//!   run      simulate the SET of live states; epsilon-closure follows `Split`, and `^`/`$` only when the position allows it
//! `find` is LEFTMOST-LONGEST (POSIX style): the earliest start that can match, then the longest end from it. Perl/rust-regex use
//! leftmost-FIRST (alternation priority); that needs thread priorities and is deliberately left out. Positions are char indexes.
//! VERIFY: an obviously-correct backtracking matcher over the AST (with a step budget) is the oracle; thousands of random regexes and
//! strings must agree, state count must stay linear in regex length, and `(a*)*b` on a run of `a`s must exhaust the oracle but not the NFA.
//! COMMON USES: grep/ripgrep-style search, input validation, URL routers, log scrapers, WAF rules, syntax highlighters and lexers (`[a-z_]\w*`),
//!   untrusted-pattern matching where ReDoS matters (RE2, rust `regex`, Go `regexp`), tokenizers feeding the Pratt parser of pratt_parser.
//! Read: https://swtch.com/~rsc/regexp/regexp1.html and https://github.com/rust-lang/regex
use std::cell::Cell;
use thiserror::Error;

// ---------------------------------------------------------------- AST + parser

/// A set of char ranges; a literal is a one-element class, which keeps the NFA to a single "consume" state kind.
#[derive(Debug, Clone, PartialEq)]
struct Class {
    negated: bool,
    ranges: Vec<(char, char)>,
}

impl Class {
    fn literal(c: char) -> Self {
        Class {
            negated: false,
            ranges: vec![(c, c)],
        }
    }
    fn matches(&self, c: char) -> bool {
        self.ranges.iter().any(|&(lo, hi)| lo <= c && c <= hi) != self.negated
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Ast {
    Empty,
    Class(Class),
    Any,
    Bol,
    Eol,
    Cat(Vec<Ast>),
    Alt(Vec<Ast>),
    Star(Box<Ast>),
    Plus(Box<Ast>),
    Opt(Box<Ast>),
}

#[derive(Debug, Error, PartialEq)]
enum ParseError {
    #[error("unmatched ')' at {0}")]
    UnmatchedClose(usize),
    #[error("group opened at {0} is never closed")]
    UnclosedGroup(usize),
    #[error("class opened at {0} is never closed")]
    UnclosedClass(usize),
    #[error("nothing to repeat before the quantifier at {0}")]
    NothingToRepeat(usize),
    #[error("reversed range in class at {0}")]
    BadRange(usize),
    #[error("dangling backslash at {0}")]
    TrailingEscape(usize),
}

struct Parser {
    cs: Vec<char>,
    pos: usize,
}

fn parse(src: &str) -> Result<Ast, ParseError> {
    let mut p = Parser {
        cs: src.chars().collect(),
        pos: 0,
    };
    let ast = p.alt()?;
    // alt() only stops early at a ')' that no '(' owns.
    if p.pos < p.cs.len() {
        Err(ParseError::UnmatchedClose(p.pos))
    } else {
        Ok(ast)
    }
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.cs.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        self.pos += c.is_some() as usize;
        c
    }
    fn eat(&mut self, want: char) -> bool {
        let hit = self.peek() == Some(want);
        self.pos += hit as usize;
        hit
    }

    // alt := cat ('|' cat)*       lowest precedence, so it is the entry point
    fn alt(&mut self) -> Result<Ast, ParseError> {
        let mut branches = vec![self.cat()?];
        while self.eat('|') {
            branches.push(self.cat()?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().unwrap()
        } else {
            Ast::Alt(branches)
        })
    }

    // cat := repeat*              an empty concatenation is legal: `a|` and `()` match the empty string
    fn cat(&mut self) -> Result<Ast, ParseError> {
        let mut items = Vec::new();
        while !matches!(self.peek(), None | Some('|') | Some(')')) {
            items.push(self.repeat()?);
        }
        Ok(match items.len() {
            0 => Ast::Empty,
            1 => items.pop().unwrap(),
            _ => Ast::Cat(items),
        })
    }

    // repeat := atom ('*' | '+' | '?')*
    fn repeat(&mut self) -> Result<Ast, ParseError> {
        let mut node = self.atom()?;
        while let Some(q @ ('*' | '+' | '?')) = self.peek() {
            self.pos += 1;
            let inner = Box::new(node);
            node = match q {
                '*' => Ast::Star(inner),
                '+' => Ast::Plus(inner),
                _ => Ast::Opt(inner),
            };
        }
        Ok(node)
    }

    fn atom(&mut self) -> Result<Ast, ParseError> {
        let start = self.pos;
        match self.bump().expect("cat() checked for end of input") {
            '(' => {
                let inner = self.alt()?;
                if self.eat(')') {
                    Ok(inner)
                } else {
                    Err(ParseError::UnclosedGroup(start))
                }
            }
            '[' => self.class(start),
            '.' => Ok(Ast::Any),
            '^' => Ok(Ast::Bol),
            '$' => Ok(Ast::Eol),
            '*' | '+' | '?' => Err(ParseError::NothingToRepeat(start)),
            '\\' => {
                let c = self.bump().ok_or(ParseError::TrailingEscape(start))?;
                Ok(Ast::Class(escape(c)))
            }
            c => Ok(Ast::Class(Class::literal(c))),
        }
    }

    // class := '[' '^'? (char ('-' char)?)* ']'     a '-' before ']' is a literal dash
    fn class(&mut self, start: usize) -> Result<Ast, ParseError> {
        let negated = self.eat('^');
        let mut ranges = Vec::new();
        loop {
            let at = self.pos;
            let lo = match self.bump() {
                None => return Err(ParseError::UnclosedClass(start)),
                Some(']') => break,
                Some('\\') => self.bump().ok_or(ParseError::TrailingEscape(at))?,
                Some(c) => c,
            };
            let is_range =
                self.peek() == Some('-') && !matches!(self.cs.get(self.pos + 1), None | Some(']'));
            if is_range {
                self.pos += 1;
                let hi = self.bump().expect("range end checked above");
                if hi < lo {
                    return Err(ParseError::BadRange(at));
                }
                ranges.push((lo, hi));
            } else {
                ranges.push((lo, lo));
            }
        }
        Ok(Ast::Class(Class { negated, ranges }))
    }
}

fn escape(c: char) -> Class {
    let (negated, ranges) = match c {
        'd' => (false, vec![('0', '9')]),
        'w' => (false, vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')]),
        'n' => (false, vec![('\n', '\n')]),
        other => return Class::literal(other), // `\.` `\*` `\\` ...: the char itself
    };
    Class { negated, ranges }
}

// ---------------------------------------------------------------- Thompson NFA

#[derive(Debug, Clone)]
enum State {
    Char(Class, usize),
    Any(usize),
    Split(usize, usize), // epsilon edge to both
    Bol(usize),          // epsilon edge, only at text start
    Eol(usize),          // epsilon edge, only at text end
    Match,
}

const MATCH: usize = 0; // the accepting state is always id 0

#[derive(Debug)]
struct Nfa {
    states: Vec<State>,
    start: usize,
}

fn compile(ast: &Ast) -> Nfa {
    let mut nfa = Nfa {
        states: vec![State::Match],
        start: 0,
    };
    nfa.start = nfa.emit(ast, MATCH);
    nfa
}

impl Nfa {
    fn push(&mut self, s: State) -> usize {
        self.states.push(s);
        self.states.len() - 1
    }

    /// Emit states for `ast` that continue at `next`; return the entry state. Building back-to-front means a loop only needs
    /// one placeholder `Split` that is overwritten once the body's entry is known. Each AST node adds at most one state.
    fn emit(&mut self, ast: &Ast, next: usize) -> usize {
        match ast {
            Ast::Empty => next,
            Ast::Class(c) => self.push(State::Char(c.clone(), next)),
            Ast::Any => self.push(State::Any(next)),
            Ast::Bol => self.push(State::Bol(next)),
            Ast::Eol => self.push(State::Eol(next)),
            Ast::Cat(items) => items.iter().rev().fold(next, |n, a| self.emit(a, n)),
            Ast::Alt(branches) => {
                let mut entries: Vec<usize> = branches.iter().map(|b| self.emit(b, next)).collect();
                let mut acc = entries.pop().expect("Alt has >= 2 branches");
                while let Some(e) = entries.pop() {
                    acc = self.push(State::Split(e, acc));
                }
                acc
            }
            Ast::Star(inner) => {
                let split = self.push(State::Split(0, 0));
                let body = self.emit(inner, split); // body loops back to the split
                self.states[split] = State::Split(body, next);
                split
            }
            Ast::Plus(inner) => {
                let split = self.push(State::Split(0, 0));
                let body = self.emit(inner, split);
                self.states[split] = State::Split(body, next);
                body // enter through the body: at least one iteration
            }
            Ast::Opt(inner) => {
                let body = self.emit(inner, next);
                self.push(State::Split(body, next))
            }
        }
    }

    /// Epsilon-closure: everything reachable without consuming input. The visited bitmap makes loops like `(a*)*` terminate.
    /// Blocked `Bol`/`Eol` states stay in the result so a caller can re-close later when the position flags change.
    fn closure(&self, seeds: &[usize], at_start: bool, at_end: bool) -> Vec<usize> {
        let mut seen = vec![false; self.states.len()];
        let mut stack = seeds.to_vec();
        let mut out = Vec::new();
        while let Some(s) = stack.pop() {
            if std::mem::replace(&mut seen[s], true) {
                continue;
            }
            out.push(s);
            match &self.states[s] {
                State::Split(a, b) => stack.extend([*a, *b]),
                State::Bol(n) if at_start => stack.push(*n),
                State::Eol(n) if at_end => stack.push(*n),
                _ => {}
            }
        }
        out.sort_unstable(); // canonical order: equal sets compare equal (needed by the DFA cache)
        out
    }

    /// Consume `c` from every live state, then close. `at_end` tells whether the position AFTER `c` is the end of text.
    fn step(&self, set: &[usize], c: char, at_end: bool) -> Vec<usize> {
        let seeds: Vec<usize> = set
            .iter()
            .filter_map(|&s| match &self.states[s] {
                State::Char(cl, n) if cl.matches(c) => Some(*n),
                State::Any(n) => Some(*n),
                _ => None,
            })
            .collect();
        self.closure(&seeds, false, at_end)
    }

    fn is_match(&self, text: &[char]) -> bool {
        let n = text.len();
        let mut set = self.closure(&[self.start], true, n == 0);
        for (i, &c) in text.iter().enumerate() {
            set = self.step(&set, c, i + 1 == n);
            if set.is_empty() {
                return false; // dead: no thread survives, stop early
            }
        }
        set.contains(&MATCH)
    }

    /// Leftmost-longest: try starts left to right; from one start keep running until the set dies and remember the last
    /// position where `Match` was live. O(n^2 * states) worst case; a real engine adds a new start thread per step instead.
    fn find(&self, text: &[char]) -> Option<(usize, usize)> {
        let n = text.len();
        for start in 0..=n {
            let mut set = self.closure(&[self.start], start == 0, start == n);
            let mut last = None;
            for pos in start..=n {
                if set.contains(&MATCH) {
                    last = Some(pos);
                }
                if pos == n || set.is_empty() {
                    break;
                }
                set = self.step(&set, text[pos], pos + 1 == n);
            }
            if let Some(end) = last {
                return Some((start, end));
            }
        }
        None
    }
}

// ---------------------------------------------------------------- public facade

struct Regex {
    ast: Ast,
    nfa: Nfa,
}

impl Regex {
    fn new(pattern: &str) -> Result<Regex, ParseError> {
        let ast = parse(pattern)?;
        let nfa = compile(&ast);
        Ok(Regex { ast, nfa })
    }
    fn is_match(&self, s: &str) -> bool {
        self.nfa.is_match(&s.chars().collect::<Vec<_>>())
    }
    fn find(&self, s: &str) -> Option<(usize, usize)> {
        self.nfa.find(&s.chars().collect::<Vec<_>>())
    }
}

// ---------------------------------------------------------------- backtracking oracle

#[derive(Debug, PartialEq)]
struct StepLimit;

struct Budget {
    used: u64,
    limit: u64,
}

impl Budget {
    fn tick(&mut self) -> Result<(), StepLimit> {
        self.used += 1;
        if self.used > self.limit {
            Err(StepLimit)
        } else {
            Ok(())
        }
    }
}

type Res = Result<bool, StepLimit>;
/// Continuation: "given that the node matched up to position j, can the REST of the pattern succeed?"
type Cont<'a> = &'a dyn Fn(usize, &mut Budget) -> Res;

fn bt(ast: &Ast, t: &[char], i: usize, b: &mut Budget, k: Cont) -> Res {
    b.tick()?;
    let one = |ok: bool, b: &mut Budget| if ok { k(i + 1, b) } else { Ok(false) };
    match ast {
        Ast::Empty => k(i, b),
        Ast::Class(c) => one(i < t.len() && c.matches(t[i]), b),
        Ast::Any => one(i < t.len(), b),
        Ast::Bol => {
            if i == 0 {
                k(i, b)
            } else {
                Ok(false)
            }
        }
        Ast::Eol => {
            if i == t.len() {
                k(i, b)
            } else {
                Ok(false)
            }
        }
        Ast::Cat(items) => bt_cat(items, t, i, b, k),
        Ast::Alt(branches) => {
            for br in branches {
                if bt(br, t, i, b, k)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Ast::Star(inner) => bt_star(inner, t, i, b, k),
        Ast::Plus(inner) => bt(inner, t, i, b, &|j: usize, b: &mut Budget| {
            bt_star(inner, t, j, b, k)
        }),
        Ast::Opt(inner) => {
            if bt(inner, t, i, b, k)? {
                Ok(true)
            } else {
                k(i, b)
            }
        }
    }
}

fn bt_cat(items: &[Ast], t: &[char], i: usize, b: &mut Budget, k: Cont) -> Res {
    match items.split_first() {
        None => k(i, b),
        Some((first, rest)) => bt(first, t, i, b, &|j: usize, b: &mut Budget| {
            bt_cat(rest, t, j, b, k)
        }),
    }
}

/// Greedy loop. An iteration that consumed nothing is rejected, otherwise `(a*)*` would recurse forever; it adds no new
/// match positions, so the answer is unchanged. The exponential blow-up comes from the many ways to SPLIT the text.
fn bt_star(inner: &Ast, t: &[char], i: usize, b: &mut Budget, k: Cont) -> Res {
    let again = |j: usize, b: &mut Budget| {
        if j == i {
            Ok(false)
        } else {
            bt_star(inner, t, j, b, k)
        }
    };
    if bt(inner, t, i, b, &again)? {
        Ok(true)
    } else {
        k(i, b)
    }
}

fn oracle_is_match(ast: &Ast, text: &[char], limit: u64) -> Result<bool, StepLimit> {
    let mut b = Budget { used: 0, limit };
    bt(ast, text, 0, &mut b, &|j: usize, _: &mut Budget| {
        Ok(j == text.len())
    })
}

/// Exhaustive on purpose: the continuation always answers `false`, so every path is explored and the best end is recorded.
fn oracle_find(ast: &Ast, text: &[char], limit: u64) -> Result<Option<(usize, usize)>, StepLimit> {
    let mut b = Budget { used: 0, limit };
    for start in 0..=text.len() {
        let best = Cell::new(None::<usize>);
        bt(ast, text, start, &mut b, &|j: usize, _: &mut Budget| {
            best.set(best.get().max(Some(j)));
            Ok(false)
        })?;
        if let Some(end) = best.get() {
            return Ok(Some((start, end)));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------- deterministic randomness

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Random regex SOURCE text (not an AST) so the parser is exercised too. Alternations are always parenthesised, which keeps
/// every generated unit safe to concatenate or quantify.
fn gen_regex(rng: &mut Rng, depth: u32) -> String {
    if depth == 0 || rng.below(5) == 0 {
        let atom = match rng.below(12) {
            0..=4 => ["a", "b", "c"][rng.below(3) as usize].to_string(),
            5 | 6 => ".".into(),
            7 => "[ab]".into(),
            8 => "[^a]".into(),
            9 => "[a-b]".into(),
            10 => "^".into(),
            _ => "$".into(),
        };
        return if rng.below(3) == 0 {
            format!("{atom}{}", ["*", "+", "?"][rng.below(3) as usize])
        } else {
            atom
        };
    }
    let (x, y) = (gen_regex(rng, depth - 1), gen_regex(rng, depth - 1));
    match rng.below(6) {
        0 | 1 => format!("{x}{y}"),
        2 => format!("({x}|{y})"),
        3 => format!("({x})*"),
        4 => format!("({x})+"),
        _ => format!("({x})?"),
    }
}

fn gen_text(rng: &mut Rng) -> Vec<char> {
    (0..rng.below(8))
        .map(|_| ['a', 'b', 'c'][rng.below(3) as usize])
        .collect()
}

// ---------------------------------------------------------------- main

fn main() {
    // 1. Demo: matching and finding.
    let cases = [
        ("a(b|c)*d", "abcbcd", true),
        ("[a-z_][a-z0-9_]*", "snake_case9", true),
        ("^ab?$", "a", true),
        (r"\d+\.\d+", "3.14", true),
        (r"a\*", "a*", true),
        ("a|", "", true),
    ];
    for (pat, text, want) in cases {
        let re = Regex::new(pat).unwrap();
        assert_eq!(re.is_match(text), want, "{pat:?} on {text:?}");
    }
    let finds = [
        (r"\d+", "ab 123 45", Some((3, 6))), // leftmost start, longest end
        ("a|ab|abc", "xabcx", Some((1, 4))), // leftmost-LONGEST (a leftmost-first engine would say (1,2))
        ("c$", "abc", Some((2, 3))),
        ("x*", "abc", Some((0, 0))), // empty match at 0
    ];
    for (pat, text, want) in finds {
        let got = Regex::new(pat).unwrap().find(text);
        assert_eq!(got, want, "find {pat:?} in {text:?}");
    }

    // 2. Parse errors carry positions.
    let bad = [
        ("(ab", ParseError::UnclosedGroup(0)),
        ("ab)", ParseError::UnmatchedClose(2)),
        ("a|*", ParseError::NothingToRepeat(2)),
        ("[z-a]", ParseError::BadRange(1)),
    ];
    for (pat, want) in bad {
        let err = Regex::new(pat).err().expect("must fail");
        assert_eq!(err, want, "{pat:?}");
        println!("{pat:<8} -> error: {err}");
    }

    // 3. Differential test: NFA == backtracking oracle (is_match and find).
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut compared, mut skipped, mut matched) = (0u32, 0u32, 0u32);
    for _ in 0..4000 {
        let pat = gen_regex(&mut rng, 3);
        let re = Regex::new(&pat).unwrap_or_else(|e| panic!("generator made invalid {pat:?}: {e}"));
        // state count is linear: at most one state per source char, plus the Match state
        assert!(
            re.nfa.states.len() <= pat.chars().count() + 1,
            "{pat:?} has {} states",
            re.nfa.states.len()
        );
        for _ in 0..8 {
            let text = gen_text(&mut rng);
            let (want_m, want_f) = match (
                oracle_is_match(&re.ast, &text, 200_000),
                oracle_find(&re.ast, &text, 200_000),
            ) {
                (Ok(m), Ok(f)) => (m, f),
                _ => {
                    skipped += 1; // oracle ran out of budget: the weakness being demonstrated
                    continue;
                }
            };
            assert_eq!(
                re.nfa.is_match(&text),
                want_m,
                "is_match {pat:?} on {text:?}"
            );
            assert_eq!(re.nfa.find(&text), want_f, "find {pat:?} on {text:?}");
            compared += 1;
            matched += want_m as u32;
        }
    }
    assert!(
        compared > 20_000 && matched > 1000,
        "test must be non-trivial: {compared} compared, {matched} matched"
    );
    println!(
        "differential: {compared} (regex,text) pairs agree ({matched} matches, {skipped} skipped by oracle budget)"
    );

    // 4. Catastrophic backtracking vs. linear NFA.
    for pat in ["(a*)*b", "(a|aa)*c"] {
        let re = Regex::new(pat).unwrap();
        let ok = oracle_is_match(&re.ast, &['a', 'a', 'a', 'a', 'b'], 1_000_000);
        let boom = oracle_is_match(&re.ast, &['a'; 40], 1_000_000);
        assert!(ok.is_ok(), "short input is fine for the backtracker");
        assert_eq!(boom, Err(StepLimit), "{pat} on 40 a's must exceed 1M steps");
        assert!(!re.nfa.is_match(&vec!['a'; 100_000])); // 100 000 chars: instant
        println!(
            "{pat:<10} backtracker on 40 a's: {boom:?} (>1M steps); NFA on 100000 a's: false, {} states",
            re.nfa.states.len()
        );
    }
    let re = Regex::new("(a*)*b").unwrap();
    assert_eq!(re.find(&"a".repeat(1000)), None);
    assert_eq!(re.find(&format!("{}b", "a".repeat(1000))), Some((0, 1001)));

    // 5. NFA size grows linearly with the pattern: ~1 state per source char, never exponentially.
    for reps in [10usize, 1000] {
        let pat = "(a|b)*c?".repeat(reps);
        let (len, n) = (pat.len(), Regex::new(&pat).unwrap().nfa.states.len());
        assert!(n <= len + 1, "{n} states for {len} chars");
        println!("pattern of {len:>5} chars -> {n:>5} NFA states");
    }
    println!("ok");
}
