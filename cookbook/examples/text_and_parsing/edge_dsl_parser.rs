//! RECIPE 65 (parsing): a tiny language: tokenizer + recursive-descent parser + errors with spans.
//! LEVEL: advanced
//! Use in: the future workflow DSL; any text format you design.   Run: cargo run --release --example edge_dsl_parser
//! COMMON USES: workflow/config DSLs, graph description languages (DOT-like), query languages.
#![allow(dead_code)]
//!
//! Grammar (domain: a metro map, not tasks):   program := stmt*    stmt := name ("->" name)* (";" | newline | EOF)
//! Pipeline: text --lex--> Vec<Token{kind, span}> --parse--> edges. Keeping the SPAN on every token lets errors point
//! at the exact bytes (see diagnostics_spans). Recursive descent = one function per grammar rule, a cursor over tokens.
//! Read: https://craftinginterpreters.com/parsing-expressions.html and pratt_parser for expression precedence.
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Name(String),
    Arrow,
    Sep, // ';' or newline
}
#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    span: Range<usize>,
}
#[derive(Debug)]
struct ParseError {
    message: String,
    span: Range<usize>,
}

fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    let b = src.as_bytes();
    let (mut i, mut out) = (0, vec![]);
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' | b'\r' => i += 1,
            b';' | b'\n' => {
                out.push(Token {
                    kind: Kind::Sep,
                    span: i..i + 1,
                });
                i += 1;
            }
            b'-' if b.get(i + 1) == Some(&b'>') => {
                out.push(Token {
                    kind: Kind::Arrow,
                    span: i..i + 2,
                });
                i += 2;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                out.push(Token {
                    kind: Kind::Name(src[start..i].to_string()),
                    span: start..i,
                });
            }
            _ => {
                let ch = src[i..].chars().next().unwrap();
                return Err(ParseError {
                    message: format!("unexpected character `{ch}`"),
                    span: i..i + ch.len_utf8(),
                });
            }
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    eof: usize,
}
impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.pos)
    }
    fn name(&mut self, what: &str, after: Range<usize>) -> Result<String, ParseError> {
        match self.toks.get(self.pos) {
            Some(Token {
                kind: Kind::Name(n),
                ..
            }) => {
                self.pos += 1;
                Ok(n.clone())
            }
            Some(t) => Err(ParseError {
                message: format!("expected {what}, found {:?}", t.kind),
                span: t.span.clone(),
            }),
            None => Err(ParseError {
                message: format!("expected {what}, found end of input"),
                span: after,
            }),
        }
    }
    // stmt := name ("->" name)*
    fn stmt(&mut self, edges: &mut Vec<(String, String)>) -> Result<(), ParseError> {
        let mut prev = self.name("a name", self.eof..self.eof)?;
        while let Some(Token {
            kind: Kind::Arrow,
            span,
        }) = self.peek().cloned()
        {
            self.pos += 1;
            let next = self.name("a name after `->`", span.clone())?;
            edges.push((prev, next.clone()));
            prev = next;
        }
        match self.peek() {
            None => Ok(()),
            Some(Token {
                kind: Kind::Sep, ..
            }) => {
                self.pos += 1;
                Ok(())
            }
            Some(t) => Err(ParseError {
                message: "expected `->` or end of statement".into(),
                span: t.span.clone(),
            }),
        }
    }
    fn program(&mut self) -> Result<Vec<(String, String)>, ParseError> {
        let mut edges = vec![];
        while let Some(t) = self.peek() {
            if t.kind == Kind::Sep {
                self.pos += 1; // blank statement
                continue;
            }
            self.stmt(&mut edges)?;
        }
        Ok(edges)
    }
}

fn parse(src: &str) -> Result<Vec<(String, String)>, ParseError> {
    let toks = lex(src)?;
    Parser {
        toks,
        pos: 0,
        eof: src.len(),
    }
    .program()
}

fn main() {
    let ok = "central -> market -> harbor\nmarket -> airport; harbor -> airport\n\n";
    println!("{:?}", parse(ok).unwrap());
    for bad in ["a -> -> b", "a -> ", "a b", "a -> b $", "->"] {
        match parse(bad) {
            Ok(e) => println!("{bad:?}: ok {e:?}"),
            Err(e) => println!(
                "{bad:?}: {} at {:?} = {:?}",
                e.message,
                e.span,
                bad.get(e.span.clone()).unwrap_or("<eof>")
            ),
        }
    }
    assert_eq!(parse("a -> b -> c").unwrap().len(), 2);
    assert!(parse("a -> ").is_err());
}
