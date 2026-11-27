//! RECIPE 1206 (compilers): Comparing syntax trees: show what changed in code, not which lines moved.
//! LEVEL: advanced
//! Use in: code review, change detection.   Run: cargo run --release --example syntax_tree_diff
//! COMMON USES: review tools, ignoring formatting-only commits, finding moved functions, checking generated code.
#![allow(dead_code)]
//!
//! The tree here is shallow on purpose: tokens, brackets and rough "statements" for a narrow C-like notation. It is
//! enough to ignore layout and comments, and it knows no grammar. It REFUSES what it can't cut into tokens for certain,
//! among it every `/` that doesn't open a comment, so no division and no regular expressions. Line breaks are thrown
//! away, so it is wrong for languages where they matter (Python, and JavaScript without `;`), and comments are ignored.
//! "No change" never proves that behaviour is the same. The answer is not always the smallest possible change.
//! Real tools build trees with a parser for each language.
//! Read: https://difftastic.wilfred.me.uk/diffing.html

use std::hash::{DefaultHasher, Hash, Hasher};

const MAX_BYTES: usize = 200_000;
/// Atoms and brackets in one file.
const MAX_TOKENS: usize = 5_000;
const MAX_NESTING: usize = 64;
/// The biggest table one alignment may fill.
const MAX_CELLS: usize = 250_000;
/// A block must have this many atoms before "moved" says anything useful.
const MOVE_SIZE: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Shape {
    Atom,
    /// What stands between a pair of brackets; the text is the opening one.
    Brackets,
    /// A run of items up to `;`, `,` or the end of a `{…}` block.
    Statement,
    File,
}

#[derive(Debug, Clone)]
struct Node {
    shape: Shape,
    text: String,
    children: Vec<Node>,
    /// Where the node starts, from 1. Not part of what makes two nodes equal.
    line: usize,
    /// A summary of the whole subtree.
    hash: u64,
    /// Hashes of the atoms and brackets inside, sorted.
    atoms: Vec<u64>,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        // the hash says "no" quickly; only the full comparison may say "yes", because hashes can collide
        self.hash == other.hash && self.shape == other.shape && self.text == other.text && self.children == other.children
    }
}

fn hash_of(value: impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

impl Node {
    fn new(shape: Shape, text: impl Into<String>, children: Vec<Node>, line: usize) -> Node {
        let text = text.into();
        let hash = hash_of((shape, &text, children.iter().map(|c| c.hash).collect::<Vec<_>>()));
        let mut atoms: Vec<u64> = children.iter().flat_map(|c| c.atoms.iter().copied()).collect();
        if matches!(shape, Shape::Atom | Shape::Brackets) {
            atoms.push(hash_of(&text));
        }
        atoms.sort_unstable();
        Node { shape, text, children, line, hash, atoms }
    }

    fn size(&self) -> usize {
        self.atoms.len()
    }
}

#[derive(Debug, PartialEq)]
struct ReadError {
    line: usize,
    why: &'static str,
}

/// The characters that can join into longer operators. `/` is not here: it is only accepted where it opens a comment.
const OPERATOR_CHARS: &str = "+-*=<>!&|^%~?:.";

/// Operators of more than one character. A run of operator characters must be exactly one of these, or one character.
/// Anything else (`:=`, `**`, `?.`, `<-`, `=-`) is refused: splitting it by guess would make `a := b` equal `a : = b`.
const OPERATORS: [&str; 36] = [
    "===", "!==", ">>>=", ">>>", "<<=", ">>=", "...", "..=", "<=>", "**=", "??=", "&&=", "||=", "==", "!=", "<=", ">=",
    "&&", "||", "++", "--", "->", "=>", "::", "..", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>", "??",
];

/// How many bytes the string at the start of `rest` takes.
fn string_length(rest: &str) -> Result<usize, &'static str> {
    let mut escaped = false;
    for (i, c) in rest.char_indices().skip(1) {
        match c {
            '\n' | '\r' => return Err("a string runs over a line end"),
            '"' if !escaped => return Ok(i + 1),
            // in some languages these hold code, with quotes of its own
            '(' if escaped => return Err("code inside a string"),
            '{' if rest[..i].ends_with(['$', '#']) => return Err("code inside a string"),
            _ => {}
        }
        escaped = c == '\\' && !escaped;
    }
    Err("a string never ends")
}

/// How many bytes the number at the start of `rest` takes. Only `12` and `1.5` are known.
fn number_length(rest: &str) -> Result<usize, &'static str> {
    let digits = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let mut end = digits(rest);
    if rest[end..].starts_with('.') {
        let more = digits(&rest[end + 1..]);
        if more > 0 {
            end += more + 1;
        }
    }
    // `1e+5`, `0x1F`, `1.`, `1..2`: where such a thing ends depends on the language
    if rest[end..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '.') {
        return Err("this form of number");
    }
    Ok(end)
}

/// A comment can end somewhere else than it seems to.
fn check_comment(comment: &str) -> Result<(), &'static str> {
    // Java reads `\u000A` as a line end, even inside a comment
    if comment.contains("\\u") {
        return Err("a `\\u` inside a comment");
    }
    // in C, a `\` at a line end joins the next line on
    if comment.split(['\n', '\r']).any(|l| l.trim_end().ends_with('\\')) {
        return Err("a comment line that ends with a backslash");
    }
    Ok(())
}

/// The tokens: (text, line). Spaces and comments go. Whatever is not listed here is refused, never guessed at.
fn tokens(src: &str) -> Result<Vec<(String, usize)>, ReadError> {
    if src.len() > MAX_BYTES {
        return Err(ReadError { line: 1, why: "the file is too large" });
    }
    // old C reads these as `\`, `[` and `]`, before it looks for tokens or comments
    for trigraph in ["??/", "??(", "??)"] {
        if let Some(at) = src.find(trigraph) {
            return Err(ReadError { line: 1 + src[..at].matches('\n').count(), why: "a C trigraph" });
        }
    }
    let mut out: Vec<(String, usize)> = Vec::new();
    let (mut i, mut line) = (0, 1);
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    // where the last token ended
    let mut last_end = usize::MAX;
    while let Some(c) = src[i..].chars().next() {
        let rest = &src[i..];
        let fail = |why| ReadError { line, why };
        let length = if matches!(c, ' ' | '\t' | '\n' | '\r') {
            line += usize::from(c == '\n');
            i += 1;
            continue;
        } else if rest.starts_with("//") {
            // some languages end a line at a lone carriage return too: refuse rather than hide what follows it
            let comment = &rest[..rest.find('\n').unwrap_or(rest.len())];
            let comment = comment.strip_suffix('\r').unwrap_or(comment);
            if comment.contains(['\r', '\u{85}', '\u{2028}', '\u{2029}']) {
                return Err(fail("an unusual line break inside a comment"));
            }
            check_comment(comment).map_err(fail)?;
            i += comment.len();
            continue;
        } else if let Some(body) = rest.strip_prefix("/*") {
            let Some(end) = body.find("*/") else { return Err(fail("a comment never ends")) };
            // Rust nests comments and C does not
            if body[..end].contains("/*") {
                return Err(fail("a comment inside a comment"));
            }
            check_comment(&body[..end]).map_err(fail)?;
            line += rest[..end + 4].matches('\n').count();
            i += end + 4;
            continue;
        } else if c == '/' {
            // `/a  b/` and `y = /\//` can't be read without the language's grammar
            return Err(fail("a `/`: it may divide or start a regular expression"));
        } else if c == '"' {
            // `r"a\"`, `L"x"` and `"""` follow other rules
            let touches = |t: &(String, usize)| t.0.ends_with(|c| word(c) || c == '"');
            if last_end == i && out.last().is_some_and(touches) {
                return Err(fail("a string with something stuck to it"));
            }
            let length = string_length(rest).map_err(fail)?;
            if rest[length..].starts_with(|c| word(c) || c == '"') {
                return Err(fail("a string with something stuck to it"));
            }
            length
        } else if c == '\'' || c == '`' {
            return Err(fail("this kind of quote"));
        } else if c.is_ascii_digit() {
            number_length(rest).map_err(fail)?
        } else if word(c) {
            rest.find(|c| !word(c)).unwrap_or(rest.len())
        } else if "()[]{};,".contains(c) {
            1
        } else if OPERATOR_CHARS.contains(c) {
            let run = rest.find(|c| !OPERATOR_CHARS.contains(c)).unwrap_or(rest.len());
            // `.5` and `..15`: the last `.` may belong to the number
            if rest[..run].ends_with('.') && rest[run..].starts_with(|c: char| c.is_ascii_digit()) {
                return Err(fail("a `.` just before a digit"));
            }
            if run > 1 && !OPERATORS.contains(&&rest[..run]) {
                return Err(fail("operator characters that this reader can't split"));
            }
            run
        } else {
            // `#`, `@`, `$`, `\`, other white space, letters outside ASCII, ...
            return Err(fail("a character outside this notation"));
        };
        if out.len() == MAX_TOKENS {
            return Err(fail("too many tokens"));
        }
        out.push((rest[..length].to_owned(), line));
        i += length;
        last_end = i;
    }
    Ok(out)
}

/// Cuts the items of one bracket pair (or of the file) into statements.
fn statements(items: Vec<Node>) -> Vec<Node> {
    let mut out = Vec::new();
    let mut open: Vec<Node> = Vec::new();
    let mut items = items.into_iter().peekable();
    while let Some(item) = items.next() {
        let semicolon = item.shape == Shape::Atom && (item.text == ";" || item.text == ",");
        // a `{…}` ends a statement unless what follows plainly continues it
        let block = item.shape == Shape::Brackets
            && item.text == "{"
            && !items.peek().is_some_and(|n| ["else", ";", ",", ".", "?"].contains(&n.text.as_str()));
        open.push(item);
        if semicolon || block || items.peek().is_none() {
            let line = open[0].line;
            out.push(Node::new(Shape::Statement, "", std::mem::take(&mut open), line));
        }
    }
    out
}

fn read(src: &str) -> Result<Node, ReadError> {
    // an explicit stack of open brackets: (opening bracket, line, items so far)
    let mut stack: Vec<(String, usize, Vec<Node>)> = vec![(String::new(), 1, Vec::new())];
    for (token, line) in tokens(src)? {
        let fail = |why| Err(ReadError { line, why });
        match token.as_str() {
            "(" | "[" | "{" => {
                // the tree is walked, and dropped, by functions that call themselves
                if stack.len() > MAX_NESTING {
                    return fail("brackets nest too deeply");
                }
                stack.push((token, line, Vec::new()));
            }
            ")" | "]" | "}" => {
                let (open, start, items) = stack.pop().expect("the file level is never popped");
                if !matches!((open.as_str(), token.as_str()), ("(", ")") | ("[", "]") | ("{", "}")) {
                    return fail("this bracket closes nothing");
                }
                let node = Node::new(Shape::Brackets, open, statements(items), start);
                stack.last_mut().expect("below an open bracket is another level").2.push(node);
            }
            _ => {
                let node = Node::new(Shape::Atom, token, Vec::new(), line);
                stack.last_mut().expect("there is always a level").2.push(node);
            }
        }
    }
    match <[_; 1]>::try_from(stack) {
        Ok([(_, _, items)]) => Ok(Node::new(Shape::File, "", statements(items), 1)),
        Err(stack) => Err(ReadError { line: stack.last().map_or(1, |s| s.1), why: "this bracket is never closed" }),
    }
}

// ---- the comparison ----

#[derive(Debug)]
enum Delta<'a> {
    /// Equal in the old and the new tree (their lines may differ).
    Same(&'a Node, &'a Node),
    Removed(&'a Node),
    Added(&'a Node),
    /// The same kind of node, with changes inside.
    Inside(&'a Node, &'a Node, Vec<Delta<'a>>),
}

/// Atoms two nodes have in common, each counted as often as both have it. The lists are sorted.
fn common(a: &[u64], b: &[u64]) -> usize {
    let (mut i, mut j, mut n) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
        }
    }
    n
}

/// How good it is to put `a` and `b` side by side. 0 means: don't.
fn score(a: &Node, b: &Node) -> usize {
    if a == b {
        // better than any partial match of the same node
        return 2 * a.size() + 1;
    }
    // two different atoms have different text, so they never pair
    if a.shape != b.shape || a.text != b.text {
        return 0;
    }
    // look inside only when at least half of the atoms are shared
    let shared = common(&a.atoms, &b.atoms);
    if 4 * shared >= a.size() + b.size() { shared } else { 0 }
}

/// The pairs, in order and without crossing, with the highest total score.
/// This is the table of a longest common subsequence, with scores in place of 1.
fn best_pairs(n: usize, m: usize, score: impl Fn(usize, usize) -> usize) -> Vec<(usize, usize)> {
    let mut scores = vec![0; n * m];
    // best[i][j]: the best total for the first i old and the first j new items
    let width = m + 1;
    let mut best = vec![0usize; (n + 1) * width];
    for i in 1..=n {
        for j in 1..=m {
            let s = score(i - 1, j - 1);
            scores[(i - 1) * m + j - 1] = s;
            let mut top = best[(i - 1) * width + j].max(best[i * width + j - 1]);
            if s > 0 {
                top = top.max(best[(i - 1) * width + j - 1] + s);
            }
            best[i * width + j] = top;
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        let s = scores[(i - 1) * m + j - 1];
        if s > 0 && best[i * width + j] == best[(i - 1) * width + j - 1] + s {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if best[i * width + j] == best[(i - 1) * width + j] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();
    pairs
}

fn align<'a>(old: &'a [Node], new: &'a [Node]) -> Result<Vec<Delta<'a>>, ReadError> {
    // refuse: with no pairs, every statement would look removed and added, and then "moved"
    if old.len().saturating_mul(new.len()) > MAX_CELLS {
        return Err(ReadError { line: old[0].line, why: "too many statements side by side to match" });
    }
    let pairs = best_pairs(old.len(), new.len(), |i, j| score(&old[i], &new[j]));
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (a, b) in pairs {
        out.extend(old[i..a].iter().map(Delta::Removed));
        out.extend(new[j..b].iter().map(Delta::Added));
        out.push(diff(&old[a], &new[b])?);
        (i, j) = (a + 1, b + 1);
    }
    out.extend(old[i..].iter().map(Delta::Removed));
    out.extend(new[j..].iter().map(Delta::Added));
    Ok(out)
}

/// Both nodes have the same shape and text: two files, or a pair `align` chose.
fn diff<'a>(old: &'a Node, new: &'a Node) -> Result<Delta<'a>, ReadError> {
    Ok(if old == new { Delta::Same(old, new) } else { Delta::Inside(old, new, align(&old.children, &new.children)?) })
}

fn text_of(node: &Node, out: &mut Vec<String>) {
    if matches!(node.shape, Shape::Atom | Shape::Brackets) {
        out.push(node.text.clone());
    }
    node.children.iter().for_each(|c| text_of(c, out));
    if node.shape == Shape::Brackets {
        let close = match node.text.as_str() {
            "(" => ")",
            "[" => "]",
            _ => "}",
        };
        out.push(close.to_owned());
    }
}

fn words(node: &Node) -> Vec<String> {
    let mut out = Vec::new();
    text_of(node, &mut out);
    out
}

#[derive(Debug, PartialEq)]
enum Change {
    Removed { line: usize, text: String },
    Added { line: usize, text: String },
    /// A whole block that left one place and appears unchanged in another.
    Moved { from: usize, to: usize, text: String },
}

fn collect<'a>(delta: &Delta<'a>, removed: &mut Vec<&'a Node>, added: &mut Vec<&'a Node>) {
    match delta {
        Delta::Same(..) => {}
        Delta::Removed(n) => removed.push(n),
        Delta::Added(n) => added.push(n),
        Delta::Inside(_, _, children) => children.iter().for_each(|c| collect(c, removed, added)),
    }
}

fn changes(old: &Node, new: &Node) -> Result<Vec<Change>, ReadError> {
    let delta = diff(old, new)?;
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    collect(&delta, &mut removed, &mut added);
    let show = |n: &Node| words(n).join(" ");
    let mut out = Vec::new();
    for node in removed {
        let twin = (node.size() >= MOVE_SIZE).then(|| added.iter().position(|a| *a == node)).flatten();
        match twin {
            Some(i) => {
                let to = added.remove(i);
                out.push(Change::Moved { from: node.line, to: to.line, text: show(node) });
            }
            None => out.push(Change::Removed { line: node.line, text: show(node) }),
        }
    }
    out.extend(added.into_iter().map(|n| Change::Added { line: n.line, text: show(n) }));
    Ok(out)
}

fn try_compare(old: &str, new: &str) -> Result<Vec<Change>, ReadError> {
    changes(&read(old)?, &read(new)?)
}

fn compare(old: &str, new: &str) -> Vec<Change> {
    try_compare(old, new).unwrap_or_else(|e| panic!("{old:?} {new:?}: {e:?}"))
}

/// The one answer that may lead someone to skip a review.
fn says_no_change(old: &str, new: &str) -> bool {
    try_compare(old, new).is_ok_and(|c| c.is_empty())
}

fn fixed_cases() {
    let removed = |line, text: &str| Change::Removed { line, text: text.into() };
    let added = |line, text: &str| Change::Added { line, text: text.into() };
    // layout and comments are not changes
    assert_eq!(compare("fn f(a: u8) -> u8 { a + 1 }", "// new\nfn f(\n    a: u8\n) -> u8 {\n    a + 1 /* same */\n}\n"), vec![]);
    // one value deep inside
    assert_eq!(compare("fn f() {\n  g(1, 2);\n  h();\n}", "fn f() { g(1, 30); h(); }"), vec![removed(2, "2"), added(1, "30")]);
    // things that look alike but are not
    for (a, b) in [("a == b;", "a = = b;"), ("a - -b;", "a --b;"), ("x = \"a b\";", "x = \"a  b\";"), ("a <=> b;", "a <= > b;"), ("f(a);", "f[a];"), ("x = 1.5;", "x = 1 . 5;")] {
        assert!(!compare(a, b).is_empty(), "{a} and {b}");
    }
    // a backslash escapes one character, which may be another backslash
    assert_eq!(words(&read(r#""a\\" x "b\"c""#).unwrap()), [r#""a\\""#, "x", r#""b\"c""#]);
    // a whole function in another place
    let one = "fn one() { alpha(1); }\n";
    let two = "fn two() { beta(2); gamma(); }\n";
    let moved = compare(&format!("{one}{two}"), &format!("{two}{one}"));
    assert_eq!(moved, vec![Change::Moved { from: 1, to: 2, text: "fn one ( ) { alpha ( 1 ) ; }".into() }]);
    // moved AND edited: no pair of equal blocks, and pairs may not cross, so one side shows as gone and new
    let both = compare(&format!("{one}{two}"), &format!("{two}fn one() {{ alpha(9); }}"));
    assert!(matches!(both[..], [Change::Removed { line: 1, .. }, Change::Added { line: 2, .. }]), "{both:?}");
    // `else` stays with its `if`
    assert_eq!(read("if a { b(); } else { c(); } d();").unwrap().children.len(), 2);
    // wrong for JavaScript: a line break after `return` changes what the function gives back
    assert_eq!(compare("return value;", "return\nvalue;"), vec![]);

    let why = |src: &str| read(src).unwrap_err().why;
    assert_eq!(why("f(a]"), "this bracket closes nothing");
    assert_eq!(why(")"), "this bracket closes nothing");
    assert_eq!(read("a\n{ b\n(").unwrap_err(), ReadError { line: 3, why: "this bracket is never closed" });
    for src in [
        "\"open", "\"ends in \\", "\"two\nlines\"", "\"two\rlines\"", "/* open", "/*/", "'a'", "`x`", "r\"a\\\";",
        "br#\"x\"#", "\"a\"sv", "\"\"\"a\"\"\"", "\"${\" \"}\"", "\"#{x}\"", "\"\\(x)\"", "/* /* */ */", "// a\rb();",
        "// a \\\nb();", "/* a *\\\n/ b(); /* */", "// \\u000A b();", "// why??/\nb();",
        "..15", "..1.5", "a ..15;", "??//**/", "a ??/ b", "\"??/\"", "a??(0??);",
        // a `/` that is not a comment, in every place
        "a / b;", "a /= b;", "y = /\\//; send(secret);", "ok = /a  b/.test(s);", "f(/[/*]/);", "</p>", "<br/>",
        // numbers, operator runs and characters whose cut this reader would have to guess
        "1e+5", "1e5", "0x1F", "1.", "1..2", "1.2.3", "10u32", ".5", "a := 1;", "a ** b;", "a?.b;", "ch <- v;", "a =-1;",
        "!!a;", "a<!--b", "#define F(x) x", "$a", "@x", "a \\ b", "caf\u{e9}", "a\u{a0}b", "a\u{c}b",
    ] {
        assert!(read(src).is_err(), "{src}");
    }
    assert!(read("// ends in CRLF\r\nf(r, \"x\", 1.5, a.b, x?y:z);").is_ok());
    // two nodes whose hashes collide are still different: forge a collision, since a real one is hard to find
    let (a, mut b) = (read("a;").unwrap(), read("b;").unwrap());
    b.hash = a.hash;
    assert!(a != b);
}

fn limit_cases() {
    let nested = |n: usize| read(&format!("{}{}", "(".repeat(n), ")".repeat(n)));
    assert!(nested(MAX_NESTING).is_ok());
    assert_eq!(nested(MAX_NESTING + 1).unwrap_err().why, "brackets nest too deeply");
    assert!(nested(2_000).is_err());
    assert_eq!(read(&"a ".repeat(MAX_TOKENS + 1)).unwrap_err().why, "too many tokens");
    assert_eq!(read(&" ".repeat(MAX_BYTES + 1)).unwrap_err().why, "the file is too large");
    // one new statement before 499 others fits the table; before 501 it doesn't, and nothing is claimed
    let calls = |n: usize| (0..n).map(|i| format!("call(a{i}, b);")).collect::<String>();
    let one_more = |n: usize| try_compare(&calls(n), &format!("extra(1, 2);{}", calls(n)));
    assert!(matches!(one_more(499).as_deref(), Ok([Change::Added { .. }])));
    assert_eq!(one_more(501).unwrap_err().why, "too many statements side by side to match");
    // equal files need no table
    assert!(says_no_change(&calls(700), &calls(700)));
}

/// The old side of a delta must spell the old tree, and the new side the new one.
fn rebuilds(delta: &Delta, old: &Node, new: &Node) {
    fn side(delta: &Delta, old: bool, out: &mut Vec<String>) {
        match delta {
            Delta::Same(a, b) => text_of(if old { a } else { b }, out),
            Delta::Removed(n) if old => text_of(n, out),
            Delta::Added(n) if !old => text_of(n, out),
            Delta::Removed(_) | Delta::Added(_) => {}
            Delta::Inside(a, b, children) => {
                let node = if old { a } else { b };
                if node.shape == Shape::Brackets {
                    out.push(node.text.clone());
                }
                children.iter().for_each(|c| side(c, old, out));
                if node.shape == Shape::Brackets {
                    let close = words(node).pop().expect("brackets have a closer");
                    out.push(close);
                }
            }
        }
    }
    for (is_old, node) in [(true, old), (false, new)] {
        let mut got = Vec::new();
        side(delta, is_old, &mut got);
        assert_eq!(got, words(node));
    }
}

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// Tries every set of pairs. Only for a few items.
fn slow_best(table: &[Vec<usize>], i: usize, j: usize) -> usize {
    if i == table.len() || j == table[0].len() {
        return 0;
    }
    let skip = slow_best(table, i + 1, j).max(slow_best(table, i, j + 1));
    match table[i][j] {
        0 => skip,
        s => skip.max(s + slow_best(table, i + 1, j + 1)),
    }
}

fn alignment_matches_the_slow_way() {
    let mut rng = Rng(0x4242_4242_1111_2222);
    for _ in 0..3_000 {
        let (n, m) = (1 + rng.below(6), 1 + rng.below(6));
        let table: Vec<Vec<usize>> = (0..n).map(|_| (0..m).map(|_| rng.below(4)).collect()).collect();
        let pairs = best_pairs(n, m, |i, j| table[i][j]);
        assert!(pairs.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
        assert!(pairs.iter().all(|&(i, j)| table[i][j] > 0));
        assert_eq!(pairs.iter().map(|&(i, j)| table[i][j]).sum::<usize>(), slow_best(&table, 0, 0), "{table:?}");
    }
}

fn random_source(rng: &mut Rng, depth: usize, out: &mut Vec<&'static str>) {
    for _ in 0..rng.below(6) {
        let kind = rng.below(if depth > 3 { 8 } else { 11 });
        let (open, close) = [("(", ")"), ("[", "]"), ("{", "}")][rng.below(3)];
        if kind >= 8 {
            out.push(open);
            random_source(rng, depth + 1, out);
            out.push(close);
        } else {
            out.push(["a", "b", "if", "else", ";", ",", "==", "\"s ) // {\""][kind]);
        }
    }
}

fn layout(rng: &mut Rng, tokens: &[&str]) -> String {
    tokens.iter().map(|t| format!("{t}{}", [" ", "\n", "\t ", " /* ) */ ", " // \" {\n"][rng.below(5)])).collect()
}

/// A plain second opinion: two files are "unchanged" exactly when their token lists are equal.
fn random_files() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..8_000 {
        let mut tokens = Vec::new();
        random_source(&mut rng, 0, &mut tokens);
        let mut other = tokens.clone();
        match rng.below(4) {
            0 => {}
            1 if !other.is_empty() => {
                let i = rng.below(other.len());
                if !"()[]{}".contains(other[i]) {
                    other[i] = ["z", ";", "a"][rng.below(3)];
                }
            }
            2 => {
                let i = rng.below(other.len() + 1);
                other.insert(i, ["z", ";", "b"][rng.below(3)]);
            }
            _ => {
                other.clear();
                random_source(&mut rng, 0, &mut other);
            }
        }
        let a = read(&layout(&mut rng, &tokens)).unwrap();
        let b = read(&layout(&mut rng, &other)).unwrap();
        assert_eq!(words(&a), tokens);
        assert_eq!(changes(&a, &b).unwrap().is_empty(), tokens == other, "{tokens:?} {other:?}");
        rebuilds(&diff(&a, &b).unwrap(), &a, &b);
    }
}

fn hostile() {
    let parts = ["(", ")", "[", "]", "{", "}", "\"", "'", "\\", "/", "*", "\n", "\r", " ", "a", "r", "#", ";", "é", "🔥", "=", ".", ">"];
    let mut rng = Rng(0x0f0f_1234_5678_0001);
    for _ in 0..50_000 {
        let src: String = (0..rng.below(30)).map(|_| parts[rng.below(parts.len())]).collect();
        if let Ok(tree) = read(&src) {
            rebuilds(&diff(&tree, &tree).unwrap(), &tree, &tree);
        }
    }
}

/// Any two punctuation characters may be one token in some language. So "XY" and "X Y" must never be "no change":
/// the reader either refuses one of them or reports a difference. Every pair is tried. Brackets, `;` and `,` are left
/// out, as this notation says they are always tokens of their own, and `_` is part of a word.
fn every_pair_of_punctuation() {
    let marks: Vec<char> = (33..127u8).map(char::from).filter(|c| c.is_ascii_punctuation() && !"()[]{};,_".contains(*c)).collect();
    let around = ["", " ", "a", "a ", "1", "1 ", "\"s\"", "\"s\" "];
    let back = ["", " ", "b", " b", "2", " 2", "\"t\"", " \"t\""];
    for &x in &marks {
        for &y in &marks {
            for left in around {
                for right in back {
                    let joined = format!("{left}{x}{y}{right}");
                    let split = format!("{left}{x} {y}{right}");
                    assert!(!says_no_change(&joined, &split), "{joined:?} {split:?}");
                }
            }
        }
    }
}

/// Single tokens of various languages, many of them ones this reader doesn't know. Each stands for one meaning.
const UNITS: [&str; 51] = [
    "a", "ab", "if", "12", "1.5", "(", ")", "{", "}", ";", ",", "+", "-", "*", "=", "<", ">", "!", "?", ":", ".", "&",
    "==", "->", "::", "..", "&&", "\"a  b\"", "\"x\\\"y\"", "/", "/=", "/a  b/", "/\\//", "/[/*]/", "1e+5",
    "0x1F", "1.", ".5", "**", ":=", "?.", "<-", "|>", "r\"a\"", "\"\"\"d  d\"\"\"", "'c  c'", "`t  t`", "#if", "$ab", "@ab",
    "</",
];

/// Writes random lists of units with space between them, then breaks one unit in two (a space put inside it, which
/// changes what it means). The two files must never be reported as unchanged.
fn broken_tokens() {
    let mut rng = Rng(0x5afe_c0de_0000_0011);
    let mut compared = 0;
    for _ in 0..200_000 {
        let units: Vec<&str> = (0..1 + rng.below(5)).map(|_| UNITS[rng.below(UNITS.len())]).collect();
        let at = rng.below(units.len());
        if units[at].len() < 2 {
            continue;
        }
        let cut = 1 + rng.below(units[at].len() - 1);
        let mut broken: Vec<String> = units.iter().map(|u| u.to_string()).collect();
        broken[at].insert(cut, ' ');
        let gaps: Vec<&str> = units.iter().map(|_| [" ", "\n", "  ", " /* c */ "][rng.below(4)]).collect();
        let write = |list: &[String]| list.iter().zip(&gaps).map(|(u, g)| format!("{u}{g}")).collect::<String>();
        let (old, new) = (write(&units.iter().map(|u| u.to_string()).collect::<Vec<_>>()), write(&broken));
        compared += usize::from(try_compare(&old, &new).is_ok());
        assert!(!says_no_change(&old, &new), "{old:?} {new:?}");
    }
    // the check means something only if many pairs get as far as a comparison
    assert!(compared > 1_000, "{compared}");
}

const OLD: &str = r#"fn price(item: &Item, tax: u32) -> u32 {
    let base = item.cents * item.count;
    base + percent(base, tax)
}

fn show(total: u32) {
    log(total);
}
"#;

const NEW: &str = r#"fn show(total: u32) {
    log(total);
}

fn price(
    item: &Item,
    tax: u32
) -> u32 {
    // quantity was renamed
    let base = item.cents * item.quantity;
    base + percent(base, tax)
}
"#;

fn main() {
    fixed_cases();
    limit_cases();
    alignment_matches_the_slow_way();
    random_files();
    every_pair_of_punctuation();
    broken_tokens();
    hostile();

    for change in compare(OLD, NEW) {
        println!("{change:?}");
    }
    println!("all checks passed");
}
