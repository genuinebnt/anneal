from author import T, write_track


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


WITHIN = """
fn within<T: Send + 'static>(ms: u64, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_millis(ms)).ok()
}
"""

P = []

# ---------------------------------------------------------------------------------------------------------------
# 1. Regex
RX_TYPES = """
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A `(` without its `)`, or a `)` without its `(`.
    UnbalancedParen,
    /// A `*`, `+` or `?` with nothing before it (at the start, after `(` or `|`, or directly after another quantifier).
    NothingToRepeat,
    /// A `\\` at the very end of the pattern.
    DanglingEscape,
}
"""

RX_STARTER = RX_TYPES + """
/// A compiled regular expression over `char`s: literals, `.`, `*`, `+`, `?`, `|`, `(` `)` and `\\` to escape a metacharacter.
pub struct Regex {
    // your compiled program
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Regex, ParseError> {
        todo!()
    }

    /// Does the pattern match the **whole** text?
    pub fn is_match(&self, text: &str) -> bool {
        todo!()
    }

    /// The leftmost match, and the longest one starting there, as `(start, end)` in `char` indices (`end` exclusive).
    pub fn find(&self, text: &str) -> Option<(usize, usize)> {
        todo!()
    }
}
"""

RX = RX_TYPES + """
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
            '\\\\' => match self.peek() {
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
"""


def M(pat, text):
    return f'Regex::new("{pat}").unwrap().is_match("{text}")'


def Fd(pat, text):
    return f'Regex::new("{pat}").unwrap().find("{text}")'


def E(pat):
    return f'Regex::new("{pat}").err()'


RX_HELPERS = WITHIN + """
        fn m(pat: &str, text: &str) -> bool {
            Regex::new(pat).unwrap().is_match(text)
        }
"""

P.append(dict(
    slug="regex-nfa-matcher", title="A regex engine that cannot be made to blow up", level="hard", stage="build-it",
    tags=["regular expressions", "Thompson NFA", "state sets", "parsing", "linear time", "ReDoS", "leftmost-longest"],
    teaches=["A regex is a tiny language: parse it (alternation binds loosest, then concatenation, then the postfix `* + ?`) into a tree, then compile the tree to a small program of `Char` / `Split` / `Jmp` / `Match` instructions.",
             "Running the program as a **set of states** (Thompson's construction) advances all alternatives at once, so matching takes O(text x program) with no backtracking: `(a*)*b` against 5 000 `a`s finishes instantly, where a backtracking engine takes longer than the age of the universe.",
             "Empty loops (`(a*)*`) make the epsilon closure cyclic: a per-step 'already visited' set is what keeps the closure finite."],
    statement="""
        Build a regular-expression engine in the style of RE2 and Rust's `regex` crate (no backtracking). It works on `char`s.

        Syntax: any character matches itself, `.` matches any one character, `*` zero or more, `+` one or more, `?` zero or one
        (all postfix, applying to the single atom before them), `|` alternation, `( )` grouping, and `\\` makes the next character
        literal (`\\.`, `\\*`, `\\\\`). Precedence from loosest: `|`, then concatenation, then the postfix operators. The empty pattern,
        an empty group `()` and an empty alternative (`a|`) are all legal and match the empty string.

        `Regex::new` returns a `ParseError`: `UnbalancedParen` for a `(` with no `)` or a `)` with no `(`; `NothingToRepeat`
        for a quantifier with nothing before it (at the start, after `(` or `|`) or directly after another quantifier (`a**`);
        `DanglingEscape` for a trailing `\\`.

        `is_match` is true when the pattern matches the **entire** text. `find` returns the **leftmost** match and, among matches
        starting there, the **longest**, as `(start, end)` in `char` indices (`end` exclusive). It may be an empty match.
        Matching must take time proportional to `text length x pattern length`, however nasty the pattern.
    """,
    starter=RX_STARTER,
    solution=RX,
    visible=[
        T("literals_must_match_whole_text", "full match, not a prefix", "(" + M("abc", "abc") + ", " + M("abc", "abcd") + ", " + M("abc", "ab") + ")", "(true, false, false)"),
        T("dot_matches_one_char", "a.c", "(" + M("a.c", "abc") + ", " + M("a.c", "ac") + ", " + M("a.c", "abbc") + ")", "(true, false, false)"),
        T("star_plus_quest", "the three quantifiers", "(" + M("a*", "") + ", " + M("a+", "") + ", " + M("a+", "aaa") + ", " + M("colou?r", "color") + ", " + M("colou?r", "colouur") + ")", "(true, false, true, true, false)"),
        T("alternation_and_groups", "cat|dog and (ab)+", "(" + M("cat|dog", "dog") + ", " + M("cat|dog", "cow") + ", " + M("(ab)+", "abab") + ", " + M("(ab)+", "aba") + ")", "(true, false, true, false)"),
        T("find_is_leftmost_longest", "b+ in aabbbc", Fd("b+", "aabbbc"), "Some((2, 5))"),
        T("empty_pattern", "matches the empty string only", "(" + M("", "") + ", " + M("", "a") + ")", "(true, false)"),
        T("parse_errors", "the three kinds", "(" + E("(a") + ", " + E("*a") + ", " + E(r"ab\\") + ")", "(Some(ParseError::UnbalancedParen), Some(ParseError::NothingToRepeat), Some(ParseError::DanglingEscape))"),
    ],
    hidden=[
        T("alternation_binds_loosest", "ab|cd is (ab)|(cd)", "(" + M("ab|cd", "ab") + ", " + M("ab|cd", "cd") + ", " + M("ab|cd", "abcd") + ", " + M("ab|cd", "ad") + ")", "(true, true, false, false)"),
        T("postfix_binds_tightest", "ab* is a(b*)", "(" + M("ab*", "abbb") + ", " + M("ab*", "abab") + ", " + M("(ab)*", "abab") + ")", "(true, false, true)"),
        T("nested_groups_and_alternatives", "(a|b)*abb", "(" + M("(a|b)*abb", "aababb") + ", " + M("(a|b)*abb", "abab") + ", " + M("(ab|a)(bc|c)?", "abc") + ", " + M("(ab|a)(bc|c)?", "ac") + ")", "(true, false, true, true)"),
        T("the_longer_alternative_is_found", "needs real nondeterminism", "(" + M("(foo|foobar)baz", "foobarbaz") + ", " + M("(a|ab)(c|bcd)", "abcd") + ")", "(true, true)"),
        T("empty_pieces", "a||b, (), (a*)*, (a*)+, (|a)+", "(" + M("a||b", "") + ", " + M("()", "") + ", " + M("(a*)*", "aaa") + ", " + M("(a*)+", "") + ", " + M("(|a)+", "aa") + ", " + M("a()b", "ab") + ")", "(true, true, true, true, true, true)"),
        T("escapes", r"a\.c, \*, \\", "(" + M(r"a\\.c", "a.c") + ", " + M(r"a\\.c", "abc") + ", " + M(r"\\*", "*") + ", " + M(r"\\\\", r"\\") + ", " + M(r"\\(a\\)", "(a)") + ")", "(true, false, true, true, true)"),
        T("every_parse_error", "paren and quantifier mistakes", "(" + E("a**") + ", " + E("a|*b") + ", " + E("(*)") + ", " + E(")") + ", " + E("a)b") + ", " + E("((a)") + ", " + E("a+?") + ", " + E("") + ")", "(Some(ParseError::NothingToRepeat), Some(ParseError::NothingToRepeat), Some(ParseError::NothingToRepeat), Some(ParseError::UnbalancedParen), Some(ParseError::UnbalancedParen), Some(ParseError::UnbalancedParen), Some(ParseError::NothingToRepeat), None)"),
        T("find_leftmost_then_longest", "positions", "(" + Fd("a*", "baaa") + ", " + Fd("a+", "baaa") + ", " + Fd("cat|category", "my category") + ", " + Fd("x", "abc") + ", " + Fd("a*", "") + ")", "(Some((0, 0)), Some((1, 4)), Some((3, 11)), None, Some((0, 0)))"),
        T("unicode_is_by_char", "indices are char indices", "(" + M("caf.", "caf\\u{e9}") + ", " + Fd("\\u{e9}+", "a\\u{e9}\\u{e9}b") + ")", "(true, Some((1, 3)))"),
        RX_HELPERS + """
        #[test]
        fn no_catastrophic_backtracking() {
            let ok = within(5000, || {
                let text = "a".repeat(3000);
                let nested = Regex::new("(a*)*b").unwrap();
                let alt = Regex::new("(a|aa)+$").unwrap();
                let many = Regex::new("(a?){30}a{30}").unwrap();
                (nested.is_match(&text), alt.is_match(&text), many.is_match(&text), Regex::new("(x+x+)+y").unwrap().is_match(&"x".repeat(3000)))
            });
            check!("finishes, and the answers are right", ok, Some((false, false, false, false)));
        }

        #[test]
        fn long_inputs_and_programs() {
            let ok = within(5000, || {
                let pat = "a".repeat(500) + &"b?".repeat(500);
                let re = Regex::new(&pat).unwrap();
                (re.is_match(&"a".repeat(500)), re.is_match(&"a".repeat(499)), re.is_match(&("a".repeat(500) + &"b".repeat(500))), re.find(&"a".repeat(600)))
            });
            check!("a 1000-instruction program", ok, Some((true, false, true, Some((0, 500)))));
        }

        #[test]
        fn matches_a_hand_written_model_on_small_inputs() {
            // every string over {a, b} up to length 6 against patterns whose language is easy to state
            let mut all: Vec<String> = vec![String::new()];
            for len in 1..=6 {
                for n in 0..(1u32 << len) {
                    all.push((0..len).map(|i| if n >> i & 1 == 0 { 'a' } else { 'b' }).collect());
                }
            }
            for s in &all {
                check!(format!("(a|b)*abb on {s:?}"), m("(a|b)*abb", s), s.ends_with("abb"));
                check!(format!("a*b* on {s:?}"), m("a*b*", s), { let t = s.trim_start_matches('a'); t.chars().all(|c| c == 'b') });
                check!(format!("(ab)*|b+ on {s:?}"), m("(ab)*|b+", s), { (s.len() % 2 == 0 && s.as_bytes().chunks(2).all(|c| c == b"ab")) || (!s.is_empty() && s.chars().all(|c| c == 'b')) });
                check!(format!("(a|b)(a|b)? on {s:?}"), m("(a|b)(a|b)?", s), s.len() == 1 || s.len() == 2);
            }
        }
        """,
    ],
    wrong=dict(
        star_needs_one=sub(RX, "        Node::Star(inner) => {\n            let split = prog.len();\n            prog.push(Inst::Split(0, 0));\n            emit(inner, prog);\n            prog.push(Inst::Jmp(split));\n            prog[split] = Inst::Split(split + 1, prog.len());\n        }", "        Node::Star(inner) => {\n            let start = prog.len();\n            emit(inner, prog);\n            prog.push(Inst::Split(start, prog.len() + 1));\n        }"),
        plus_matches_zero=sub(RX, "        Node::Plus(inner) => {\n            let start = prog.len();\n            emit(inner, prog);\n            prog.push(Inst::Split(start, prog.len() + 1));\n        }", "        Node::Plus(inner) => {\n            let split = prog.len();\n            prog.push(Inst::Split(0, 0));\n            emit(inner, prog);\n            prog.push(Inst::Jmp(split));\n            prog[split] = Inst::Split(split + 1, prog.len());\n        }"),
        quest_loops=sub(RX, "        Node::Quest(inner) => {\n            let split = prog.len();\n            prog.push(Inst::Split(0, 0));\n            emit(inner, prog);\n            prog[split] = Inst::Split(split + 1, prog.len());\n        }", "        Node::Quest(inner) => {\n            let split = prog.len();\n            prog.push(Inst::Split(0, 0));\n            emit(inner, prog);\n            prog.push(Inst::Jmp(split));\n            prog[split] = Inst::Split(split + 1, prog.len());\n        }"),
        prefix_match_is_enough=sub(RX, "self.longest_from(&chars, 0) == Some(chars.len())", "self.longest_from(&chars, 0).is_some()"),
        find_returns_the_shortest=sub(RX, "            if cur.iter().any(|&pc| matches!(self.prog[pc], Inst::Match)) {\n                best = Some(i);\n            }", "            if cur.iter().any(|&pc| matches!(self.prog[pc], Inst::Match)) {\n                return Some(i);\n            }"),
        escape_is_a_plain_char=sub(RX, "            '\\\\' => match self.peek() {", "            '\\\\' if false => match self.peek() {"),
        stray_close_paren_ignored=sub(RX, "        if p.pos < p.s.len() {\n            return Err(ParseError::UnbalancedParen); // a `)` with no `(`\n        }\n", ""),
        dangling_escape_is_literal=sub(RX, "                None => Err(ParseError::DanglingEscape),", "                None => Ok(Node::Char('\\\\')),"),
        unbalanced_open_paren_accepted=sub(RX, "                if self.peek() != Some(')') {\n                    return Err(ParseError::UnbalancedParen);\n                }\n                self.pos += 1;\n                Ok(inner)", "                if self.peek() == Some(')') {\n                    self.pos += 1;\n                }\n                Ok(inner)"),
        seen_set_is_never_reset=sub(sub(RX, "            let mut next = Vec::new();\n            let mut seen = vec![false; self.prog.len()];\n", "            let mut next = Vec::new();\n"), "        let mut best = None;\n        let mut i = start;\n", "        let mut best = None;\n        let mut i = start;\n        let mut seen = vec![false; self.prog.len()];\n"),    ),
    hints=[("approach", "Three stages. (1) A recursive-descent parser with one function per precedence level (`alt` calls `cat` calls `repeat` calls `atom`) builds a tree. (2) Compile the tree into instructions: `Split(x, y)` continues at both `x` and `y`, `Jmp(t)` continues at `t`, `Match` accepts. `a*` is `L: Split(L+1, out); a; Jmp L`. (3) Simulate: keep the *set* of program counters that are alive, and for each character build the next set."),
           ("rust", "Write `add_thread(list, seen, pc)` that follows `Jmp` and `Split` (with a `seen: Vec<bool>` reset per step) and pushes only `Char`/`Any`/`Match` states: that is the epsilon closure, and `seen` is what stops `(a*)*` from looping forever. `find` = for each start index, run the simulation and remember the **last** position at which `Match` was in the set; take the first start that has any."),
           ("edge", "`a**` and `a+?` are errors in this problem (a quantifier right after a quantifier). `()` and `a|` give `Node::Empty`. A `)` left over after the top-level parse means an unmatched close; a missing `)` inside `atom` means an unmatched open. Count characters, not bytes: collect `chars()` first.")],
    notes=("This is Russ Cox's 'Regular Expression Matching Can Be Simple And Fast': Thompson's 1968 construction, the basis of RE2, Go's `regexp` and Rust's `regex` crate, versus the backtracking engines (Perl, Python `re`, JavaScript) whose exponential cases cause real outages (the Stack Overflow and Cloudflare incidents were both a single regex). Leftmost-longest is POSIX; Perl-style engines pick the leftmost-*first* alternative instead, which needs priority-ordered threads.", "O(text x pattern) per start position (find is O(n^2 m) worst case)", "O(pattern)"),
    follow_up="1) Make `find` linear in the text by prefixing the program with `.*?` (an unanchored search that keeps one set, tracking where each thread started) so only one pass is needed. 2) Add capture groups (Pike's VM: each thread carries its capture slots) and `captures()`. 3) Add character classes `[a-z]`, `[^0-9]` and the anchors `^`, `$`: where do they live in the program? 4) Add counted repetition `a{2,4}` by expanding the tree, and note why the program size can explode for nested counts. 5) Convert the NFA to a DFA lazily (cache the state sets you reach) and measure the speedup on a long text.",
    related=["Y6", "S2", "F4"],
))

# ---------------------------------------------------------------------------------------------------------------
# 2. Diff
DIFF_TYPES = """
/// One step of an edit script turning `a` into `b`. Indexes are 0-based positions in `a` and `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `a[i] == b[j]`: the line is in both.
    Keep(usize, usize),
    /// `a[i]` is not in `b`.
    Delete(usize),
    /// `b[j]` is not in `a`.
    Insert(usize),
}
"""

DIFF_STARTER = DIFF_TYPES + """
/// A shortest edit script from `a` to `b`: the fewest `Delete`s plus `Insert`s, with the `Keep`s in between.
/// The ops walk both sequences from the start, in order. Where deleting and inserting are equally good, delete first.
pub fn diff(a: &[&str], b: &[&str]) -> Vec<Op> {
    todo!()
}
"""

DIFF = DIFF_TYPES + """
pub fn diff(a: &[&str], b: &[&str]) -> Vec<Op> {
    let (n, m) = (a.len(), b.len());
    // lcs[i][j] = length of the longest common subsequence of a[i..] and b[j..]
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push(Op::Keep(i, j)); // a match is always safe to take
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            ops.push(Op::Delete(i));
            i += 1;
        } else {
            ops.push(Op::Insert(j));
            j += 1;
        }
    }
    while i < n {
        ops.push(Op::Delete(i));
        i += 1;
    }
    while j < m {
        ops.push(Op::Insert(j));
        j += 1;
    }
    ops
}
"""

DIFF_HELPERS = WITHIN + """
        fn apply(a: &[&str], b: &[&str], ops: &[Op]) -> Option<Vec<String>> {
            // replays the script: must consume a and b exactly, in order
            let (mut i, mut j) = (0, 0);
            let mut out = Vec::new();
            for op in ops {
                match *op {
                    Op::Keep(x, y) => {
                        if x != i || y != j || i >= a.len() || j >= b.len() || a[i] != b[j] {
                            return None;
                        }
                        out.push(a[i].to_string());
                        i += 1;
                        j += 1;
                    }
                    Op::Delete(x) => {
                        if x != i || i >= a.len() {
                            return None;
                        }
                        i += 1;
                    }
                    Op::Insert(y) => {
                        if y != j || j >= b.len() {
                            return None;
                        }
                        out.push(b[j].to_string());
                        j += 1;
                    }
                }
            }
            if i == a.len() && j == b.len() { Some(out) } else { None }
        }

        fn lcs_len(a: &[&str], b: &[&str]) -> usize {
            let mut prev = vec![0usize; b.len() + 1];
            for x in a {
                let mut cur = vec![0usize; b.len() + 1];
                for (j, y) in b.iter().enumerate() {
                    cur[j + 1] = if x == y { prev[j] + 1 } else { cur[j].max(prev[j + 1]) };
                }
                prev = cur;
            }
            prev[b.len()]
        }

        fn edits(ops: &[Op]) -> usize {
            ops.iter().filter(|o| !matches!(o, Op::Keep(..))).count()
        }

        fn check_diff(a: &[&str], b: &[&str]) {
            let ops = diff(a, b);
            let replay = apply(a, b, &ops);
            assert!(replay.is_some(), "the script does not turn a into b: a={a:?} b={b:?} ops={ops:?}");
            assert_eq!(replay.unwrap(), b.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "replay differs from b");
            assert_eq!(edits(&ops), a.len() + b.len() - 2 * lcs_len(a, b), "not a shortest script for a={a:?} b={b:?}: {ops:?}");
        }
"""

P.append(dict(
    slug="shortest-edit-script", title="diff: the shortest edit script between two files", level="medium", stage="build-it",
    tags=["diff", "LCS", "dynamic programming", "edit script", "git diff", "Myers", "backtracking a table"],
    teaches=["`diff` is the longest-common-subsequence problem in disguise: lines in the LCS are kept, everything else is a delete or an insert, so a shorter script means a longer LCS.",
             "Fill the table from the *end* (`lcs[i][j]` for the suffixes) and the script can be read off front to back with no reversing.",
             "Checking an answer is part of the job: replay the script, and compare its size against an independent computation of the optimum."],
    statement="""
        Implement `diff` for sequences of lines: return a **shortest edit script** turning `a` into `b`, as a list of `Op`s
        that walks both sequences from the start. `Keep(i, j)` is a line present in both (`a[i] == b[j]`), `Delete(i)` drops `a[i]`,
        `Insert(j)` adds `b[j]`. The number of `Delete`s plus `Insert`s must be the smallest possible, which is
        `a.len() + b.len() - 2 * LCS(a, b)`.

        When deleting and inserting are equally good at some point, **delete first** (so a replaced line shows as a `Delete`
        followed by an `Insert`, as in `git diff`). Inputs can be thousands of lines: an `O(a.len() * b.len())` table is fine.
    """,
    starter=DIFF_STARTER,
    solution=DIFF,
    visible=[
        T("identical_files", "all keeps", "diff(&[\"a\", \"b\"], &[\"a\", \"b\"])", "vec![Op::Keep(0, 0), Op::Keep(1, 1)]"),
        T("delete_the_middle_line", "a b c -> a c", "diff(&[\"a\", \"b\", \"c\"], &[\"a\", \"c\"])", "vec![Op::Keep(0, 0), Op::Delete(1), Op::Keep(2, 1)]"),
        T("insert_a_line", "a c -> a b c", "diff(&[\"a\", \"c\"], &[\"a\", \"b\", \"c\"])", "vec![Op::Keep(0, 0), Op::Insert(1), Op::Keep(1, 2)]"),
        T("replacement_is_delete_then_insert", "x -> y", "diff(&[\"x\"], &[\"y\"])", "vec![Op::Delete(0), Op::Insert(0)]"),
        T("empty_sides", "from nothing and to nothing", "(diff(&[], &[\"a\", \"b\"]), diff(&[\"a\"], &[]), diff(&[], &[]))", "(vec![Op::Insert(0), Op::Insert(1)], vec![Op::Delete(0)], vec![])"),
    ],
    hidden=[
        T("a_moved_line_costs_two_edits", "a b c -> c a b", "{ let ops = diff(&[\"a\", \"b\", \"c\"], &[\"c\", \"a\", \"b\"]); ops.iter().filter(|o| !matches!(o, Op::Keep(..))).count() }", "2"),
        T("completely_different", "no common lines", "diff(&[\"a\", \"b\"], &[\"c\", \"d\"]).len()", "4"),
        T("duplicate_lines", "repeated lines are matched in order", "{ let ops = diff(&[\"x\", \"x\", \"y\"], &[\"x\", \"y\", \"x\"]); ops.iter().filter(|o| !matches!(o, Op::Keep(..))).count() }", "2"),
        T("trailing_deletes_and_inserts", "tail handling", "(diff(&[\"a\", \"b\", \"c\"], &[\"a\"]), diff(&[\"a\"], &[\"a\", \"b\", \"c\"]))", "(vec![Op::Keep(0, 0), Op::Delete(1), Op::Delete(2)], vec![Op::Keep(0, 0), Op::Insert(1), Op::Insert(2)])"),
        DIFF_HELPERS + """
        #[test]
        fn greedy_is_not_enough() {
            // deleting on every mismatch costs 4 here; the shortest script costs 2
            check_diff(&["a", "b", "c"], &["c", "a", "b"]);
            check_diff(&["a", "b", "c", "d"], &["b", "c", "d", "a"]);
            check_diff(&["x", "a", "b", "c"], &["a", "b", "c", "y"]);
        }

        #[test]
        fn scripts_replay_and_are_shortest_on_random_inputs() {
            let mut rng = anneal_prelude::Rng::new(77);
            let words = ["a", "b", "c", "d", "e"];
            for _ in 0..500 {
                let la = rng.below(12);
                let lb = rng.below(12);
                let a: Vec<&str> = (0..la).map(|_| words[rng.below(5)]).collect();
                let b: Vec<&str> = (0..lb).map(|_| words[rng.below(5)]).collect();
                check_diff(&a, &b);
            }
        }

        #[test]
        fn small_alphabet_stress() {
            let mut rng = anneal_prelude::Rng::new(5);
            for _ in 0..300 {
                let la = rng.below(25);
                let lb = rng.below(25);
                let a: Vec<&str> = (0..la).map(|_| if rng.bool() { "x" } else { "y" }).collect();
                let b: Vec<&str> = (0..lb).map(|_| if rng.bool() { "x" } else { "y" }).collect();
                check_diff(&a, &b);
            }
        }

        #[test]
        fn big_files_with_a_few_edits() {
            let r = within(10_000, || {
                let lines: Vec<String> = (0..2500).map(|i| format!("line {i}")).collect();
                let a: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                let mut b = a.clone();
                b.remove(1200);
                b.insert(10, "added near the top");
                b[2000] = "changed";
                let ops = diff(&a, &b);
                (apply(&a, &b, &ops).is_some(), edits(&ops))
            });
            check!("replays to b, with four edits: one insert, one delete, one replacement (2)", r, Some((true, 4)));
        }

        #[test]
        fn ties_prefer_delete_first() {
            let ops = diff(&["a", "b"], &["c", "d"]);
            check!("the whole replaced block deletes before it inserts", ops, vec![Op::Delete(0), Op::Delete(1), Op::Insert(0), Op::Insert(1)]);
            let ops = diff(&["a", "x", "b"], &["a", "y", "b"]);
            check!("a replaced middle line", ops, vec![Op::Keep(0, 0), Op::Delete(1), Op::Insert(1), Op::Keep(2, 2)]);
        }
        """,
    ],
    wrong=dict(
        greedy_delete_on_mismatch=sub(DIFF, "        } else if lcs[i + 1][j] >= lcs[i][j + 1] {", "        } else if true {"),
        ties_insert_first=sub(DIFF, "} else if lcs[i + 1][j] >= lcs[i][j + 1] {", "} else if lcs[i + 1][j] > lcs[i][j + 1] {"),
        forgets_trailing_deletes=sub(DIFF, "    while i < n {\n        ops.push(Op::Delete(i));\n        i += 1;\n    }\n", ""),
        forgets_trailing_inserts=sub(DIFF, "    while j < m {\n        ops.push(Op::Insert(j));\n        j += 1;\n    }\n", ""),
        keeps_only_on_the_diagonal_of_the_best_cell=sub(DIFF, "        if a[i] == b[j] {\n            ops.push(Op::Keep(i, j)); // a match is always safe to take\n            i += 1;\n            j += 1;\n        } else if", "        if a[i] == b[j] && lcs[i][j] == lcs[i + 1][j + 1] + 1 && false {\n            ops.push(Op::Keep(i, j));\n            i += 1;\n            j += 1;\n        } else if"),
        lcs_table_takes_min=sub(DIFF, "lcs[i + 1][j].max(lcs[i][j + 1])", "lcs[i + 1][j].min(lcs[i][j + 1])"),
    ),
    hints=[("approach", "Build `lcs[i][j]` for the suffixes `a[i..]` and `b[j..]`: if the first lines are equal, `1 + lcs[i+1][j+1]`, otherwise the better of skipping one line from either side. Then walk from `(0, 0)`: equal lines are kept; on a mismatch, go to the neighbour with the larger LCS (delete if `lcs[i+1][j] >= lcs[i][j+1]`, else insert). Flush what remains of either side."),
           ("rust", "`vec![vec![0u32; m + 1]; n + 1]` with a zero border row and column means no special cases at the ends. Iterate `(0..n).rev()` and `(0..m).rev()` to fill it. The `>=` in the walk is the 'delete first' rule."),
           ("edge", "Taking a match whenever `a[i] == b[j]` is safe: some optimal solution always matches the two heads. The test suite checks the *size* of your script against an independent LCS, not one particular script, so any shortest script that follows the delete-first tie rule passes.")],
    notes=("`git diff`, `diff -u` and every code-review UI do this. The DP here is O(nm) time and space; Myers' 1986 algorithm is O(ND) where D is the number of edits, which is why diffing two near-identical 100 000-line files takes milliseconds: git's default is Myers with heuristics, and `--patience` / `--histogram` pick *readable* scripts rather than minimal ones.", "O(a.len() x b.len())", "O(a.len() x b.len())"),
    follow_up="1) Strip the common prefix and suffix first and run the DP only on the middle: how much faster is the 'few edits in a big file' case? 2) Implement Myers' O(ND) greedy algorithm (the `V` array of furthest-reaching points per diagonal) and test it against this one. 3) Group the ops into hunks with three lines of context and print a unified diff (`@@ -a,b +c,d @@`). 4) Add a three-way merge: given a common base and two edited versions, merge the non-overlapping edits and report conflicts. 5) Patience diff: anchor on lines that occur exactly once in both files. Why are its scripts nicer to read for code?",
    related=["P2", "S2", "Y7"],
))

STAGES = [("build-it", "Build it", "hard")]

if __name__ == "__main__":
    P.reverse()  # the Medium problem first
    n = write_track("y8-classic-engines", "Y8", "Classic engines: regex and diff", "Y", "sde3", 8,
                    "Two programs everyone uses and few have written: a linear-time regular-expression engine and a shortest-edit-script diff.",
                    STAGES, P)
    print("Y8", n)
