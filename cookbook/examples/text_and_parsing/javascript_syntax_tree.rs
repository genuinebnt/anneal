//! RECIPE 1204 (parsing): Reading JavaScript as a syntax tree: find the addresses a script fetches, without running it.
//! LEVEL: advanced
//! Use in: code audits, link collectors, lint rules.   Run: cargo run --release --example javascript_syntax_tree
//! COMMON USES: linters, bundlers, code search, finding the API calls in a script.
#![allow(dead_code)]
//!
//! This parser reads a SMALL SUBSET of JavaScript and refuses everything else: top-level `const` and expression
//! statements, each ending in `;`, with names, plain numbers, quoted strings, `.name`, calls, `+`, arrays and object
//! literals. No functions, blocks, other operators, regular expressions or template strings. Refusing is the point:
//! a reader that guesses reports things that are not there. It lists calls SPELLED `fetch(…)`, which is not the list
//! of requests a script makes: finding none proves nothing. For real JavaScript use a full parser such as
//! `oxc_parser`, `swc_ecma_parser` or tree-sitter.
//! Read: https://tc39.es/ecma262/#sec-ecmascript-language-lexical-grammar

use std::collections::HashMap;

const MAX_BYTES: usize = 1 << 20;
const MAX_TOKENS: usize = 100_000;
/// The tallest tree, and the deepest the parser calls itself.
const MAX_DEPTH: u32 = 200;
/// The longest string the analysis builds from constants.
const MAX_VALUE: usize = 4_096;

#[derive(Debug, Clone, PartialEq)]
enum Why {
    /// Valid JavaScript or not, the subset doesn't read it.
    Unsupported(&'static str),
    /// Certainly not JavaScript: something is cut off, or a name is declared twice.
    Syntax(&'static str),
    TooBig,
    TooDeep,
}

#[derive(Debug, Clone, PartialEq)]
struct Refused {
    /// Byte position in the source.
    at: usize,
    why: Why,
}

fn refuse<T>(at: usize, why: Why) -> Result<T, Refused> {
    Err(Refused { at, why })
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    /// The string's value, with its escapes already read.
    Str(String),
    Num(String),
    Punct(char),
}

fn ends_line(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}

fn name_part(c: char) -> bool {
    name_start(c) || c.is_ascii_digit()
}

fn hex(s: &str) -> Option<u32> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| u32::from_str_radix(s, 16).ok())?
}

/// Reads the escape that follows a backslash. Returns the character it stands for (`None` for a
/// line continuation) and how many bytes it took.
fn escape(rest: &str) -> Option<(Option<char>, usize)> {
    let c = rest.chars().next()?;
    let simple = |c| Some((Some(c), 1));
    match c {
        'n' => simple('\n'),
        't' => simple('\t'),
        'r' => simple('\r'),
        'b' => simple('\u{8}'),
        'f' => simple('\u{c}'),
        'v' => simple('\u{b}'),
        // `\0` is a zero byte; with a digit after it, it is an old octal form
        '0' if !rest[1..].starts_with(|d: char| d.is_ascii_digit()) => simple('\0'),
        '0'..='9' => None,
        'x' => Some((Some(char::from_u32(hex(rest.get(1..3)?)?)?), 3)),
        'u' if rest[1..].starts_with('{') => {
            let close = rest.find('}')?;
            Some((Some(char::from_u32(hex(&rest[2..close])?)?), close + 1))
        }
        'u' => {
            let first = hex(rest.get(1..5)?)?;
            if !(0xD800..0xDC00).contains(&first) {
                return Some((Some(char::from_u32(first)?), 5));
            }
            // a JavaScript string holds UTF-16: two escapes can make one character
            let second = hex(rest.get(5..11)?.strip_prefix("\\u")?)?;
            let low = second.checked_sub(0xDC00).filter(|&l| l < 0x400)?;
            Some((Some(char::from_u32(0x10000 + ((first - 0xD800) << 10) + low)?), 11))
        }
        '\r' if rest[1..].starts_with('\n') => Some((None, 2)),
        c if ends_line(c) => Some((None, c.len_utf8())),
        c => Some((Some(c), c.len_utf8())),
    }
}

/// Reads a string whose opening quote is at `at`: its value and the position after it.
fn string(src: &str, at: usize, quote: char) -> Result<(String, usize), Refused> {
    let mut value = String::new();
    let mut i = at + 1;
    while let Some(c) = src[i..].chars().next() {
        i += c.len_utf8();
        match c {
            c if c == quote => return Ok((value, i)),
            '\n' | '\r' => break,
            '\\' => {
                let Some((made, used)) = escape(&src[i..]) else { return refuse(i - 1, Why::Unsupported("this escape")) };
                value.extend(made);
                i += used;
            }
            c => value.push(c),
        }
    }
    refuse(at, Why::Syntax("this string never ends"))
}

fn lex(src: &str) -> Result<Vec<(Token, usize)>, Refused> {
    if src.len() > MAX_BYTES {
        return refuse(0, Why::TooBig);
    }
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(c) = src[i..].chars().next() {
        let at = i;
        let rest = &src[i..];
        let token = if ends_line(c) || matches!(c, ' ' | '\t' | '\u{b}' | '\u{c}' | '\u{a0}' | '\u{feff}') {
            i += c.len_utf8();
            continue;
        } else if rest.starts_with("//") {
            // JavaScript has four line ends; stop at any of them
            i += rest.find(ends_line).unwrap_or(rest.len());
            continue;
        } else if let Some(body) = rest.strip_prefix("/*") {
            let Some(end) = body.find("*/") else { return refuse(at, Why::Syntax("this comment never ends")) };
            i += end + 4;
            continue;
        } else if c == '"' || c == '\'' {
            let (value, end) = string(src, i, c)?;
            i = end;
            Token::Str(value)
        } else if name_start(c) {
            let end = rest.find(|c| !name_part(c)).unwrap_or(rest.len());
            i += end;
            Token::Name(rest[..end].to_owned())
        } else if c.is_ascii_digit() {
            let digits = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
            let mut end = digits(rest);
            if rest[end..].starts_with('.') {
                let more = digits(&rest[end + 1..]);
                if more == 0 {
                    return refuse(at, Why::Unsupported("this number form"));
                }
                end += more + 1;
            }
            if (c == '0' && rest[1..].starts_with(|c: char| c.is_ascii_digit())) || rest[end..].starts_with(name_part) {
                return refuse(at, Why::Unsupported("this number form"));
            }
            i += end;
            Token::Num(rest[..end].to_owned())
        } else if "(){}[],.:;=+".contains(c) {
            i += 1;
            Token::Punct(c)
        } else {
            let why = match c {
                // telling a regular expression from a division needs the grammar around it
                '/' => "division or a regular expression",
                '`' => "a template string",
                _ => "this character",
            };
            return refuse(at, Why::Unsupported(why));
        };
        if out.len() == MAX_TOKENS {
            return refuse(at, Why::TooBig);
        }
        out.push((token, at));
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Str(String),
    Num(String),
    Name(String),
    Member(Box<Node>, String),
    Call(Box<Node>, Vec<Node>),
    Add(Box<Node>, Box<Node>),
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
}

#[derive(Debug, Clone)]
struct Node {
    expr: Expr,
    /// Where the expression starts.
    at: usize,
    /// 1 for a leaf, otherwise 1 more than the tallest child.
    height: u32,
}

/// Two trees are equal when their shapes are, wherever they stood in the text.
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.expr == other.expr
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Statement {
    Const(String, Node),
    Expression(Node),
}

const RESERVED: &str = "arguments async await break case catch class const continue debugger default delete do else enum \
    eval export extends false finally for function if implements import in instanceof interface let new null of package \
    private protected public return static super switch this throw true try typeof var void while with yield";

fn reserved(name: &str) -> bool {
    RESERVED.split_whitespace().any(|w| w == name)
}

struct Parser {
    tokens: Vec<(Token, usize)>,
    next: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.next).map(|t| &t.0)
    }

    fn at(&self) -> usize {
        self.tokens.get(self.next).map_or(self.end, |t| t.1)
    }

    fn bump(&mut self) -> Result<(Token, usize), Refused> {
        let Some(t) = self.tokens.get(self.next).cloned() else {
            return refuse(self.end, Why::Syntax("the script ends too early"));
        };
        self.next += 1;
        Ok(t)
    }

    fn eat(&mut self, c: char) -> bool {
        let found = self.peek() == Some(&Token::Punct(c));
        self.next += usize::from(found);
        found
    }

    fn expect(&mut self, c: char, message: &'static str) -> Result<(), Refused> {
        // JavaScript may well allow what stands here instead (`a[0]`, a second `const` name, a missing `;`)
        if self.eat(c) { Ok(()) } else { refuse(self.at(), Why::Unsupported(message)) }
    }

    /// Every node is made here, so no tree can grow taller than the limit.
    fn node(expr: Expr, at: usize) -> Result<Node, Refused> {
        let tallest = match &expr {
            Expr::Str(_) | Expr::Num(_) | Expr::Name(_) => 0,
            Expr::Member(a, _) => a.height,
            Expr::Add(a, b) => a.height.max(b.height),
            Expr::Call(f, args) => args.iter().map(|n| n.height).fold(f.height, u32::max),
            Expr::Array(items) => items.iter().map(|n| n.height).max().unwrap_or(0),
            Expr::Object(items) => items.iter().map(|(_, n)| n.height).max().unwrap_or(0),
        };
        if tallest >= MAX_DEPTH {
            return refuse(at, Why::TooDeep);
        }
        Ok(Node { expr, at, height: tallest + 1 })
    }

    /// `a, b, c` up to `close`, with an optional comma at the end.
    fn list<T>(&mut self, close: char, mut item: impl FnMut(&mut Self) -> Result<T, Refused>) -> Result<Vec<T>, Refused> {
        let mut out = Vec::new();
        while !self.eat(close) {
            out.push(item(self)?);
            if !self.eat(',') {
                self.expect(close, "expected `,` or the closing bracket")?;
                break;
            }
        }
        Ok(out)
    }

    /// `depth` counts how many times the parser has called itself.
    fn expression(&mut self, depth: u32) -> Result<Node, Refused> {
        if depth >= MAX_DEPTH {
            return refuse(self.at(), Why::TooDeep);
        }
        let mut sum = self.operand(depth)?;
        while self.eat('+') {
            let right = self.operand(depth)?;
            let at = sum.at;
            sum = Self::node(Expr::Add(Box::new(sum), Box::new(right)), at)?;
        }
        Ok(sum)
    }

    /// One value, followed by any number of `.name` and `(arguments)`.
    fn operand(&mut self, depth: u32) -> Result<Node, Refused> {
        let (token, at) = self.bump()?;
        let mut value = match token {
            Token::Str(s) => Self::node(Expr::Str(s), at)?,
            Token::Num(n) => Self::node(Expr::Num(n), at)?,
            Token::Name(n) if !reserved(&n) => Self::node(Expr::Name(n), at)?,
            Token::Name(_) => return refuse(at, Why::Unsupported("this keyword")),
            Token::Punct('(') => {
                let inner = self.expression(depth + 1)?;
                self.expect(')', "expected `)`")?;
                inner
            }
            Token::Punct('[') => {
                let items = self.list(']', |p| p.expression(depth + 1))?;
                Self::node(Expr::Array(items), at)?
            }
            Token::Punct('{') => {
                let fields = self.list('}', |p| {
                    let (key, at) = p.bump()?;
                    let (Token::Name(key) | Token::Str(key)) = key else {
                        return refuse(at, Why::Unsupported("this object key"));
                    };
                    // `__proto__:` sets the object's prototype instead of making a field
                    if key == "__proto__" {
                        return refuse(at, Why::Unsupported("`__proto__`"));
                    }
                    // `{a}` and `{a() {}}` are valid too
                    p.expect(':', "expected `:` after the key")?;
                    Ok((key, p.expression(depth + 1)?))
                })?;
                Self::node(Expr::Object(fields), at)?
            }
            // `[1,,2]` and a lone `;` are valid JavaScript
            Token::Punct(_) => return refuse(at, Why::Unsupported("this where a value should be")),
        };
        loop {
            let at = value.at;
            if self.eat('.') {
                // a keyword is allowed here: `list.delete(x)`
                let (Token::Name(name), _) = self.bump()? else {
                    return refuse(at, Why::Unsupported("this after `.`"));
                };
                value = Self::node(Expr::Member(Box::new(value), name), at)?;
            } else if self.eat('(') {
                let args = self.list(')', |p| p.expression(depth + 1))?;
                value = Self::node(Expr::Call(Box::new(value), args), at)?;
            } else {
                return Ok(value);
            }
        }
    }

    fn statement(&mut self) -> Result<Statement, Refused> {
        let statement = if self.peek() == Some(&Token::Name("const".into())) {
            self.next += 1;
            let (token, at) = self.bump()?;
            let Token::Name(name) = token else { return refuse(at, Why::Unsupported("this kind of `const`")) };
            if reserved(&name) {
                return refuse(at, Why::Unsupported("this name"));
            }
            self.expect('=', "expected `=`")?;
            Statement::Const(name, self.expression(0)?)
        } else {
            // at the start of a statement `{` opens a block, not an object
            if self.peek() == Some(&Token::Punct('{')) {
                return refuse(self.at(), Why::Unsupported("a block"));
            }
            Statement::Expression(self.expression(0)?)
        };
        if self.peek() == Some(&Token::Punct('=')) {
            return refuse(self.at(), Why::Unsupported("assignment"));
        }
        // JavaScript can supply some missing `;` by its own rules. This reader doesn't.
        self.expect(';', "expected `;`")?;
        Ok(statement)
    }
}

fn parse(src: &str) -> Result<Vec<Statement>, Refused> {
    let mut parser = Parser { tokens: lex(src)?, next: 0, end: src.len() };
    let mut out = Vec::new();
    let mut names = std::collections::HashSet::new();
    while parser.peek().is_some() {
        let at = parser.at();
        let statement = parser.statement()?;
        if let Statement::Const(name, _) = &statement
            && !names.insert(name.clone())
        {
            return refuse(at, Why::Syntax("this name is declared twice"));
        }
        out.push(statement);
    }
    Ok(out)
}

/// The text an expression stands for, when constants alone decide it.
fn constant(node: &Node, known: &HashMap<&str, Option<String>>) -> Option<String> {
    match &node.expr {
        Expr::Str(s) => (s.len() <= MAX_VALUE).then(|| s.clone()),
        Expr::Name(n) => known.get(n.as_str())?.clone(),
        Expr::Add(a, b) => {
            let (mut a, b) = (constant(a, known)?, constant(b, known)?);
            // without this, `const b = a + a; const c = b + b; …` doubles on every line
            if a.len() + b.len() > MAX_VALUE {
                return None;
            }
            a.push_str(&b);
            Some(a)
        }
        _ => None,
    }
}

#[derive(Debug, PartialEq)]
struct Fetch {
    at: usize,
    /// `None`: the script decides the address while it runs.
    address: Option<String>,
}

fn calls(node: &Node, known: &HashMap<&str, Option<String>>, out: &mut Vec<Fetch>) {
    match &node.expr {
        Expr::Str(_) | Expr::Num(_) | Expr::Name(_) => {}
        Expr::Member(a, _) => calls(a, known, out),
        Expr::Add(a, b) => {
            calls(a, known, out);
            calls(b, known, out);
        }
        Expr::Call(function, args) => {
            if function.expr == Expr::Name("fetch".into()) {
                let address = args.first().and_then(|a| constant(a, known));
                out.push(Fetch { at: node.at, address });
            }
            calls(function, known, out);
            args.iter().for_each(|a| calls(a, known, out));
        }
        Expr::Array(items) => items.iter().for_each(|a| calls(a, known, out)),
        Expr::Object(items) => items.iter().for_each(|(_, a)| calls(a, known, out)),
    }
}

#[derive(Debug, PartialEq)]
struct Report {
    /// The calls SPELLED `fetch(…)`. Not every request the script makes: an empty list proves nothing, because
    /// `const get = fetch; get("/x");` and `window.fetch("/x")` are requests too.
    spelled: Vec<Fetch>,
    /// The script declares its own `fetch`. A call listed above may or may not reach the browser's.
    own_fetch: bool,
}

fn fetches(program: &[Statement]) -> Report {
    let own_fetch = program.iter().any(|s| matches!(s, Statement::Const(n, _) if n == "fetch"));
    let mut known = HashMap::new();
    let mut spelled = Vec::new();
    for statement in program {
        let node = match statement {
            Statement::Const(_, node) | Statement::Expression(node) => node,
        };
        calls(node, &known, &mut spelled);
        // a name is known only after its own line
        if let Statement::Const(name, node) = statement {
            let value = constant(node, &known);
            known.insert(name.as_str(), value);
        }
    }
    Report { spelled, own_fetch }
}

const OTHERS: &str = "other requests may exist: only calls spelled `fetch(…)` are listed";

/// The report in words. The last line is always there, also when nothing was found.
fn describe(src: &str, report: &Report) -> Vec<String> {
    let doubt = if report.own_fetch { " (the script has its own `fetch`: this may or may not be a request)" } else { "" };
    let mut lines: Vec<String> = report
        .spelled
        .iter()
        .map(|call| {
            let line = src[..call.at].matches('\n').count() + 1;
            format!("line {line}: {}{doubt}", call.address.as_deref().unwrap_or("(address unknown)"))
        })
        .collect();
    lines.push(OTHERS.to_owned());
    lines
}

fn report(src: &str) -> Report {
    fetches(&parse(src).unwrap_or_else(|e| panic!("{src:?}: {e:?}")))
}

fn addresses(src: &str) -> Vec<Option<String>> {
    report(src).spelled.into_iter().map(|f| f.address).collect()
}

fn why(src: &str) -> Why {
    parse(src).expect_err(src).why
}

fn fixed_cases() {
    let some = |s: &str| Some(s.to_owned());
    assert_eq!(addresses(r#"// fetch("/no");
        /* fetch("/no"); */ log('fetch("/no")'); a.fetch("/method"); prefetch("/no");"#), vec![]);
    assert_eq!(addresses(r#"fetch("\x2fa\u002Fb\u{2f}c\/d");"#), vec![some("/a/b/c/d")]);
    assert_eq!(addresses("fetch('\\uD83D\\uDD25');"), vec![some("🔥")]);
    // U+2028 ends a line comment, so the call after it is real code
    assert_eq!(addresses("// note\u{2028}fetch('/real');"), vec![some("/real")]);
    assert_eq!(addresses("// note\rfetch('/real');"), vec![some("/real")]);
    assert_eq!(
        addresses(r#"const base = "https://api.test"; fetch(base + "/v1", {retry: [fetch("/in")]}).then(done);"#),
        vec![some("https://api.test/v1"), some("/in")]
    );
    assert_eq!(addresses(r#"fetch(path); const path = "/late"; fetch(host() + "/x"); fetch(1 + "/n"); fetch();"#), vec![None; 4]);
    // its own `fetch` may still be the browser's, as here: the call is listed, with the doubt
    let own = r#"const fetch = window.fetch; fetch("/real");"#;
    assert_eq!(report(own), Report { spelled: vec![Fetch { at: 28, address: some("/real") }], own_fetch: true });
    assert!(describe(own, &report(own))[0].ends_with("may or may not be a request)"));
    assert!(!report(r#"fetch("/a");"#).own_fetch);
    // real requests this reader misses: it must not turn an empty list into "no requests"
    for missed in [r#"const get = fetch; get("/missed");"#, r#"window.fetch("/missed");"#] {
        assert_eq!(describe(missed, &report(missed)), [OTHERS], "{missed}");
    }

    for src in [
        "f(a / 2);", "f(/fetch('x')/);", "f(`t`);", "x = 1;", "if (a) f();", "if(a);", "return(1);", "new Thing();",
        "{ a: 1 };", "fetch?.('/x');", "a => 1;", "f(0x1f);", "f(1e5);", "f(012);", "f(1.);", "'\\012';",
        "const {a} = b;", "const let = 1;", "f({ __proto__: 1 });", "\\u0066etch('/x');", "café();", "#!x",
        // valid JavaScript outside the subset: never called a syntax error
        "[1,,2];", "[,];", "const x = {a};", ";", "const a = 1, b = 2;", "f()\ng();", "a[0];", "f(a[0]);",
        "f({ a() {} });", "f(.5);", "f(...x);", "f(a = 1);", "f(1)\n",
    ] {
        assert!(matches!(why(src), Why::Unsupported(_)), "{src}: {:?}", why(src));
    }
    for src in ["f('a\nb');", "f('open", "/* open", "const a = 1; const a = 2;", "const", "a.", "f(1 +"] {
        assert!(matches!(why(src), Why::Syntax(_)), "{src}: {:?}", why(src));
    }
    // not JavaScript either, but the reader doesn't claim to know that
    for src in ["f(,);", "f(a b);", "a.1;", "f(=);"] {
        assert!(matches!(why(src), Why::Unsupported(_)), "{src}: {:?}", why(src));
    }
    assert_eq!(parse("f(1)\n").unwrap_err(), Refused { at: 5, why: Why::Unsupported("expected `;`") });
}

fn deep_cases() {
    let big = 30_000;
    // each of these is a short script; without a limit each makes the program run out of stack
    for src in [
        format!("{}1{};", "(".repeat(big), ")".repeat(big)),
        format!("{}{};", "[".repeat(big), "]".repeat(big)),
        format!("{}1{};", "f(".repeat(big), ")".repeat(big)),
        // no brackets at all: the parser loops, but the tree still grows one level each time
        format!("a{};", ".b".repeat(big)),
        format!("a{};", "()".repeat(big)),
        format!("1{};", "+1".repeat(big - 1)),
    ] {
        assert_eq!(why(&src), Why::TooDeep, "{}", &src[..12]);
    }
    let fits = |n: usize| parse(&format!("a{};", ".b".repeat(n))).is_ok();
    assert!(fits(MAX_DEPTH as usize - 1) && !fits(MAX_DEPTH as usize));
    assert_eq!(why(&"x".repeat(MAX_BYTES + 1)), Why::TooBig);
    assert_eq!(why(&";".repeat(MAX_TOKENS + 1)), Why::TooBig);

    let mut doubling = "const v0 = 'xxxxxxxx';".to_owned();
    // 20 lines would make 4 MB, and 40 lines 4 TB
    for i in 1..20 {
        doubling += &format!("const v{i} = v{} + v{};", i - 1, i - 1);
    }
    assert_eq!(addresses(&(doubling + "fetch(v19);")), vec![None]);
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

/// A random tree, built directly: the expected answer exists before there is any text.
fn tree(rng: &mut Rng, depth: u32) -> Node {
    let kind = rng.below(if depth > 3 { 3 } else { 8 });
    let child = |rng: &mut Rng| tree(rng, depth + 1);
    let children = |rng: &mut Rng| (0..rng.below(3)).map(|_| tree(rng, depth + 1)).collect::<Vec<_>>();
    let words = ["fetch", "a", "b_2", "$", "delete"];
    let expr = match kind {
        0 => Expr::Str((0..rng.below(5)).map(|_| ['a', '/', '"', '\'', '\\', '\n', '\r', 'é', '🔥', '\u{2028}', '\0', '1'][rng.below(12)]).collect()),
        1 => Expr::Num(["0", "7", "42", "3.5"][rng.below(4)].into()),
        2 => Expr::Name(words[rng.below(4)].into()),
        3 => Expr::Member(Box::new(child(rng)), words[rng.below(5)].into()),
        4 => {
            let function = child(rng);
            Expr::Call(Box::new(function), children(rng))
        }
        5 => {
            let left = child(rng);
            Expr::Add(Box::new(left), Box::new(child(rng)))
        }
        6 => Expr::Array(children(rng)),
        _ => Expr::Object(children(rng).into_iter().map(|n| (words[rng.below(5)].to_owned(), n)).collect()),
    };
    Node { expr, at: 0, height: 0 }
}

/// Space or a comment, as a person might leave between two tokens.
fn gap(rng: &mut Rng) -> &'static str {
    ["", "", " ", "\n", "\t", "/* ) ; ' */", "// fetch('no')\n", "//\u{2029}", "\u{a0}"][rng.below(9)]
}

fn quoted(rng: &mut Rng, value: &str) -> String {
    let quote = ['"', '\''][rng.below(2)];
    let mut out = String::from(quote);
    for c in value.chars() {
        match rng.below(4) {
            0 => out += &format!("\\u{{{:x}}}", c as u32),
            1 => out += &c.encode_utf16(&mut [0; 2]).iter().map(|u| format!("\\u{u:04X}")).collect::<String>(),
            _ if c == quote || c == '\\' => out.extend(['\\', c]),
            _ if c == '\n' => out += "\\n",
            _ if c == '\r' => out += "\\r",
            _ if c == '\0' => out += "\\x00",
            _ => out.push(c),
        }
    }
    out.push(quote);
    out
}

fn print(rng: &mut Rng, node: &Node, out: &mut String) {
    let wrap = rng.below(8) == 0;
    if wrap {
        out.push('(');
    }
    out.push_str(gap(rng));
    let comma_list = |rng: &mut Rng, items: &[Node], out: &mut String| {
        for item in items {
            print(rng, item, out);
            out.push(',');
        }
    };
    match &node.expr {
        Expr::Str(s) => *out += &quoted(rng, s),
        Expr::Num(n) | Expr::Name(n) => *out += n,
        Expr::Member(a, name) => {
            receiver(rng, a, out);
            *out += &format!("{}.{}{name}", gap(rng), gap(rng));
        }
        Expr::Call(function, args) => {
            receiver(rng, function, out);
            out.push('(');
            comma_list(rng, args, out);
            out.push(')');
        }
        Expr::Add(a, b) => {
            print(rng, a, out);
            out.push('+');
            // `+` groups to the left, so a sum on the right needs brackets
            if matches!(b.expr, Expr::Add(..)) {
                out.push('(');
                print(rng, b, out);
                out.push(')');
            } else {
                print(rng, b, out);
            }
        }
        Expr::Array(items) => {
            out.push('[');
            comma_list(rng, items, out);
            out.push(']');
        }
        Expr::Object(fields) => {
            out.push('{');
            for (key, value) in fields {
                *out += &format!("{}{key}:", gap(rng));
                print(rng, value, out);
                out.push(',');
            }
            out.push('}');
        }
    }
    out.push_str(gap(rng));
    if wrap {
        out.push(')');
    }
}

/// What stands before `.` or `(`: a sum needs brackets, and so does a number before `.`.
fn receiver(rng: &mut Rng, node: &Node, out: &mut String) {
    let brackets = matches!(node.expr, Expr::Add(..) | Expr::Num(_));
    if brackets {
        out.push('(');
    }
    print(rng, node, out);
    if brackets {
        out.push(')');
    }
}

/// Print random trees as untidy code, read the code, and compare with the tree.
fn round_trips() {
    let mut rng = Rng(0x1d87_2c61_55aa_9f03);
    for _ in 0..5_000 {
        let want = tree(&mut rng, 0);
        let mut src = "const x = ".to_owned();
        print(&mut rng, &want, &mut src);
        src.push(';');
        assert_eq!(parse(&src), Ok(vec![Statement::Const("x".into(), want)]), "{src:?}");
    }
}

/// Random and cut-off input must give a tree or a refusal, never a panic.
fn hostile() {
    let parts = [
        "(", ")", "[", "]", "{", "}", ".", ",", ";", ":", "=", "+", "/", "*", "\"", "'", "\\", "`", "\n", " ", "a",
        "const", "1", "0", "x", "u", "{2f}", "D83D", "é", "🔥", "\u{2028}", "fetch",
    ];
    let mut rng = Rng(0x7777_1234_abcd_0001);
    for _ in 0..60_000 {
        let src: String = (0..rng.below(25)).map(|_| parts[rng.below(parts.len())]).collect();
        if let Err(e) = parse(&src) {
            assert!(src.is_char_boundary(e.at), "{src:?}");
        }
    }
    let script = "const u = '\\uD83D\\uDD25\\x41é'; /* é */ fetch(u + \"/p\", {a: [1.5]}); // 🔥";
    for (cut, _) in script.char_indices() {
        let _ = parse(&script[..cut]);
    }
    // every escape cut short, also next to characters of several bytes
    for escape in ["\\", "\\x", "\\x4", "\\xé", "\\u", "\\u00é1", "\\u{", "\\u{110000}", "\\u{ffffffffff}", "\\uD800", "\\uD800\\u", "\\uD800\\u0041", "\\uD800é", "\\uDC00"] {
        assert!(parse(&format!("'{escape}';")).is_err(), "{escape}");
        assert!(parse(&format!("'{escape}")).is_err(), "{escape}");
    }
}

fn main() {
    fixed_cases();
    deep_cases();
    round_trips();
    hostile();

    let script = r#"// fetch("/in-a-comment");
const api = "https://api.example.com";
log('then call fetch("/in-a-string")');
fetch(api + "/v1/users", {method: "GET"});
fetch("\x2fhidden");
fetch(api + location.hash);"#;
    println!("text search finds {} of `fetch(`", script.matches("fetch(").count());
    for line in describe(script, &report(script)) {
        println!("{line}");
    }
    println!("all checks passed");
}
