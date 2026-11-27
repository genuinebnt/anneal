//! RECIPE 65 (parsing): a tiny language: a lexer, a recursive-descent parser and errors with spans.
//! LEVEL: advanced
//! Use in: a small config or description format of your own.   Run: cargo run --release --example edge_dsl_parser
//! COMMON USES: configuration languages, graph description languages (DOT is one), query and filter strings.
//!
//! The language describes a metro map. The lexer turns text into tokens, and each token keeps its span (byte range). The
//! parser has one function for each rule of the grammar and looks one token ahead. It stops at the first error.
//! No rule contains itself, so there is no recursion to limit. The size of the source is limited.
//! NOT COVERED: escapes in quoted text, non-ASCII names, error recovery, nested blocks, checks of meaning such as two
//! lines with one name.
//! Read: https://craftinginterpreters.com/parsing-expressions.html
//!
//!   map   := line*
//!   line  := "line" WORD TEXT? "{" route* "}"
//!   route := WORD ("->" WORD)* ";"

use std::ops::Range;

const MAX_SOURCE: usize = 64 * 1024;

type Span = Range<usize>;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind<'a> {
    /// A name or a keyword. The lexer does not tell them apart.
    Word(&'a str),
    /// Quoted text, without its quotes.
    Text(&'a str),
    Arrow,
    OpenBrace,
    CloseBrace,
    Semi,
    /// The last token of every list, with an empty span at the end of the source.
    End,
}

#[derive(Debug, Clone)]
struct Token<'a> {
    kind: Kind<'a>,
    span: Span,
}

#[derive(Debug)]
struct ParseError {
    message: String,
    span: Span,
}

fn error<T>(message: impl Into<String>, span: Span) -> Result<T, ParseError> {
    Err(ParseError {
        message: message.into(),
        span,
    })
}

fn lex(source: &str) -> Result<Vec<Token<'_>>, ParseError> {
    if source.len() > MAX_SOURCE {
        return error(
            format!("the source is longer than {MAX_SOURCE} bytes"),
            0..0,
        );
    }
    let mut tokens = Vec::new();
    let mut at = 0;
    while let Some(first) = source[at..].chars().next() {
        let rest = &source[at..];
        // how many bytes starting at `at` satisfy `keep`
        let run = |keep: fn(char) -> bool| rest.find(|c| !keep(c)).unwrap_or(rest.len());
        let (kind, length) = match first {
            ' ' | '\t' | '\r' | '\n' => (None, 1),
            '#' => (None, run(|c| c != '\n')),
            '{' => (Some(Kind::OpenBrace), 1),
            '}' => (Some(Kind::CloseBrace), 1),
            ';' => (Some(Kind::Semi), 1),
            '-' if rest.starts_with("->") => (Some(Kind::Arrow), 2),
            '"' => match rest[1..].find(['"', '\n']) {
                Some(n) if rest[1 + n..].starts_with('"') => {
                    (Some(Kind::Text(&rest[1..1 + n])), n + 2)
                }
                _ => return error("this quoted text is not closed on its line", at..at + 1),
            },
            c if c.is_ascii_alphabetic() || c == '_' => {
                let length = run(|c| c.is_ascii_alphanumeric() || c == '_');
                (Some(Kind::Word(&rest[..length])), length)
            }
            other => {
                let message = format!("unexpected character `{}`", other.escape_debug());
                return error(message, at..at + other.len_utf8());
            }
        };
        if let Some(kind) = kind {
            tokens.push(Token {
                kind,
                span: at..at + length,
            });
        }
        at += length;
    }
    tokens.push(Token {
        kind: Kind::End,
        span: at..at,
    });
    Ok(tokens)
}

/// What was found, for a message. A long word is cut short.
fn describe(kind: Kind) -> String {
    match kind {
        Kind::Word(word) if word.len() > 30 => format!("`{}...`", &word[..30]), // words are ASCII
        Kind::Word(word) => format!("`{word}`"),
        Kind::Text(_) => "quoted text".into(),
        Kind::Arrow => "`->`".into(),
        Kind::OpenBrace => "`{`".into(),
        Kind::CloseBrace => "`}`".into(),
        Kind::Semi => "`;`".into(),
        Kind::End => "the end of the source".into(),
    }
}

#[derive(Debug, Clone)]
struct Name<'a> {
    text: &'a str,
    span: Span,
}

#[derive(Debug)]
struct MetroLine<'a> {
    name: Name<'a>,
    label: Option<&'a str>,
    routes: Vec<Vec<Name<'a>>>,
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    at: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Token<'a> {
        &self.tokens[self.at]
    }

    /// Takes the next token if it is of this kind. Nothing ever takes `End`, so `peek` stays inside the list.
    fn eat(&mut self, kind: Kind) -> bool {
        let found = self.peek().kind == kind;
        if found {
            self.at += 1;
        }
        found
    }

    /// For punctuation that closes or opens something. If it is missing, the error points just after the token before,
    /// which is where it has to be typed.
    fn expect(&mut self, kind: Kind) -> Result<(), ParseError> {
        if self.eat(kind) {
            return Ok(());
        }
        let after = self
            .at
            .checked_sub(1)
            .map_or(0, |i| self.tokens[i].span.end);
        let found = describe(self.peek().kind);
        error(
            format!("expected {}, found {found}", describe(kind)),
            after..after,
        )
    }

    /// For something that has to be a name. The error points at what is there instead.
    fn name(&mut self, what: &str) -> Result<Name<'a>, ParseError> {
        let token = self.peek().clone();
        match token.kind {
            Kind::Word(text) => {
                self.at += 1;
                Ok(Name {
                    text,
                    span: token.span,
                })
            }
            other => error(
                format!("expected {what}, found {}", describe(other)),
                token.span,
            ),
        }
    }

    // map := line*
    fn map(&mut self) -> Result<Vec<MetroLine<'a>>, ParseError> {
        let mut lines = Vec::new();
        while self.peek().kind != Kind::End {
            lines.push(self.line()?);
        }
        Ok(lines)
    }

    // line := "line" WORD TEXT? "{" route* "}"
    fn line(&mut self) -> Result<MetroLine<'a>, ParseError> {
        // `line` is a keyword only here, so a stop may be called `line`
        if !self.eat(Kind::Word("line")) {
            let token = self.peek();
            return error(
                format!("expected `line`, found {}", describe(token.kind)),
                token.span.clone(),
            );
        }
        let name = self.name("a name for the line")?;
        let label = match self.peek().kind {
            Kind::Text(label) => {
                self.at += 1;
                Some(label)
            }
            _ => None,
        };
        let open = self.peek().span.clone();
        self.expect(Kind::OpenBrace)?;
        let mut routes = Vec::new();
        while !self.eat(Kind::CloseBrace) {
            if self.peek().kind == Kind::End {
                return error("this `{` is never closed", open);
            }
            routes.push(self.route()?);
        }
        Ok(MetroLine {
            name,
            label,
            routes,
        })
    }

    // route := WORD ("->" WORD)* ";"
    fn route(&mut self) -> Result<Vec<Name<'a>>, ParseError> {
        let mut stops = vec![self.name("a stop")?];
        while self.eat(Kind::Arrow) {
            stops.push(self.name("a stop after `->`")?);
        }
        self.expect(Kind::Semi)?;
        Ok(stops)
    }
}

fn parse(source: &str) -> Result<Vec<MetroLine<'_>>, ParseError> {
    Parser {
        tokens: lex(source)?,
        at: 0,
    }
    .map()
}

/// The tree without its spans, for comparing.
type Plain = Vec<(String, Option<String>, Vec<Vec<String>>)>;

fn plain(map: &[MetroLine]) -> Plain {
    let words = |route: &Vec<Name>| route.iter().map(|s| s.text.to_owned()).collect();
    map.iter()
        .map(|l| {
            (
                l.name.text.to_owned(),
                l.label.map(str::to_owned),
                l.routes.iter().map(words).collect(),
            )
        })
        .collect()
}

/// A second recognizer for the same grammar. No rule contains itself, so a table of states is enough.
/// Returns the index of the first token that cannot be there.
fn first_bad_token(tokens: &[Token]) -> Option<usize> {
    #[derive(Clone, Copy)]
    enum State {
        Top,
        WantName,
        WantLabelOrBrace,
        WantBrace,
        InBlock,
        AfterStop,
        WantStop,
    }
    use State::*;
    let mut state = Top;
    for (index, token) in tokens.iter().enumerate() {
        state = match (state, token.kind) {
            (Top, Kind::Word("line")) => WantName,
            (Top, Kind::End) => return None,
            (WantName, Kind::Word(_)) => WantLabelOrBrace,
            (WantLabelOrBrace, Kind::Text(_)) => WantBrace,
            (WantLabelOrBrace | WantBrace, Kind::OpenBrace) => InBlock,
            (InBlock, Kind::CloseBrace) => Top,
            (InBlock | WantStop, Kind::Word(_)) => AfterStop,
            (AfterStop, Kind::Arrow) => WantStop,
            (AfterStop, Kind::Semi) => InBlock,
            _ => return Some(index),
        };
    }
    None
}

/// A small seeded random generator (xorshift), so every run tests the same inputs.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
    fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from[self.below(from.len())]
    }
}

/// A random map, written with random spacing and comments, beside the tree it must parse to.
fn random_map(rng: &mut Rng) -> (String, Plain) {
    let names = ["central", "market", "harbor", "line", "_x9"];
    let mut words: Vec<String> = Vec::new();
    let mut expected = Plain::new();
    for _ in 0..rng.below(4) {
        let name = rng.pick(&names).to_owned();
        words.extend(["line".into(), name.clone()]);
        let label =
            (rng.below(2) == 0).then(|| rng.pick(&["", "Red line", "é 🦀 # { ;"]).to_owned());
        if let Some(label) = &label {
            words.push(format!("\"{label}\""));
        }
        words.push("{".into());
        let mut routes = Vec::new();
        for _ in 0..rng.below(4) {
            let stops: Vec<String> = (0..1 + rng.below(4))
                .map(|_| rng.pick(&names).to_owned())
                .collect();
            for (i, stop) in stops.iter().enumerate() {
                if i > 0 {
                    words.push("->".into());
                }
                words.push(stop.clone());
            }
            words.push(";".into());
            routes.push(stops);
        }
        words.push("}".into());
        expected.push((name, label, routes));
    }
    let in_a_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut text = String::new();
    for word in words {
        // two words need something between them; punctuation does not
        let must_separate = text.ends_with(in_a_word) && word.starts_with(in_a_word);
        if must_separate || rng.below(2) == 0 {
            text += rng.pick(&[" ", "\n", "\t", "\r\n", "  # note \" { é\n"]);
        }
        text += &word;
    }
    (text, expected)
}

/// Whatever the input, an error must point at a real part of it and stay short.
fn check_any(source: &str) {
    if let Err(e) = parse(source) {
        assert!(
            source.get(e.span.clone()).is_some(),
            "{source:?}: span {:?}",
            e.span
        );
        assert!(e.message.len() < 100, "{}", e.message);
    }
    let Ok(tokens) = lex(source) else {
        assert!(parse(source).is_err());
        return;
    };
    for token in &tokens {
        assert!(source.get(token.span.clone()).is_some());
    }
    // the two recognizers agree on what is accepted and on where it first goes wrong
    match (parse(source), first_bad_token(&tokens)) {
        (Ok(_), None) => {}
        (Err(e), Some(bad)) => {
            let at_token = e.span == tokens[bad].span;
            let after_previous =
                bad > 0 && e.span == (tokens[bad - 1].span.end..tokens[bad - 1].span.end);
            let unclosed = tokens[bad].kind == Kind::End && &source[e.span.clone()] == "{";
            assert!(
                at_token || after_previous || unclosed,
                "{source:?}: {e:?} but token {bad}"
            );
        }
        (result, bad) => panic!("{source:?}: parser {result:?}, table {bad:?}"),
    }
}

fn main() {
    let source = "# the city map\nline red \"Red line\" {\n    central -> market -> harbor;\n    market -> airport;\n}\n";
    let map = parse(source).unwrap();
    for line in &map {
        println!("{} {:?}", line.name.text, line.label.unwrap_or(""));
        for route in &line.routes {
            let stops: Vec<&str> = route.iter().map(|s| s.text).collect();
            println!("    {}", stops.join(" - "));
        }
    }
    assert_eq!(map.len(), 1);
    assert_eq!(map[0].routes[0].len(), 3);
    for name in map[0].routes.iter().flatten() {
        assert_eq!(&source[name.span.clone()], name.text);
    }

    // source, message, the byte range the error points at
    let cases = [
        ("line red { a -> b }", "expected `;`, found `}`", 17..17),
        (
            "line red {\n  a -> b\n  c;\n}",
            "expected `;`, found `c`",
            19..19,
        ),
        ("line red a; }", "expected `{`, found `a`", 8..8),
        (
            "line red { a -> ; }",
            "expected a stop after `->`, found `;`",
            16..17,
        ),
        (
            "line red { a -> }",
            "expected a stop after `->`, found `}`",
            16..17,
        ),
        ("line red { a;", "this `{` is never closed", 9..10),
        ("line { }", "expected a name for the line, found `{`", 5..6),
        ("stop red {}", "expected `line`, found `stop`", 0..4),
        (
            "line red \"Red {\n}",
            "this quoted text is not closed on its line",
            9..10,
        ),
        (
            "line red \"Red\n\" {}",
            "this quoted text is not closed on its line",
            9..10,
        ),
        ("line red { a - b; }", "unexpected character `-`", 13..14),
        ("line red { café; }", "unexpected character `é`", 14..16),
    ];
    for (source, message, span) in cases {
        let found = parse(source).expect_err(source);
        println!("{source:?}: {} at {:?}", found.message, found.span);
        assert_eq!(
            (found.message.as_str(), found.span),
            (message, span),
            "{source:?}"
        );
    }

    // random maps come back as they were generated, and so do all their beginnings without a panic
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for round in 0..3_000 {
        let (text, expected) = random_map(&mut rng);
        match parse(&text) {
            Ok(map) => assert_eq!(plain(&map), expected, "{text:?}"),
            Err(e) => panic!("{text:?}: {e:?}"),
        }
        check_any(&text);
        if round % 10 == 0 {
            for (cut, _) in text.char_indices() {
                check_any(&text[..cut]);
            }
        }
    }

    // random pieces, mostly nonsense
    let pieces = [
        "line", "a", "b", "{", "}", ";", "->", "-", ">", "\"", "\"x\"", "#", " ", "\n", "\r", "é",
        "🦀", "\0", "9",
    ];
    for _ in 0..100_000 {
        let text: String = (0..rng.below(16))
            .map(|_| format!("{}{}", rng.pick(&pieces), rng.pick(&["", " "])))
            .collect();
        check_any(&text);
    }

    // large and hostile input
    check_any(&"{".repeat(MAX_SOURCE));
    check_any(&"line a { ".repeat(MAX_SOURCE / 9));
    check_any(&format!("line a {{ b {} }}", "c".repeat(MAX_SOURCE - 20)));
    check_any(&format!("\"{}", "é".repeat(MAX_SOURCE / 3)));
    let full = format!("line a {{{}}}", " s;".repeat((MAX_SOURCE - 10) / 3));
    assert!(parse(&full).is_ok());
    assert!(parse(&" ".repeat(MAX_SOURCE)).is_ok());
    assert!(parse(&" ".repeat(MAX_SOURCE + 1)).is_err());
    println!("all checks passed");
}
