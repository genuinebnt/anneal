//! RECIPE 19 (parsing): a zero-copy lexer: tokens borrow from the input via a lifetime.
//! LEVEL: intermediate
//! Use in: the future DSL / bytecode VM.   Run: cargo run --example zero_copy_lexer
//! COMMON USES: DSL/config lexers, query parsers, log parsers: tokens that borrow from the input allocate nothing.
//!
//! Pattern: `Token<'a>` holds `&'a str` slices of the source, so lexing allocates nothing per token. The
//! lexer is an `Iterator`, so you get map/filter/collect for free.
//! Study real parsers: https://github.com/oxc-project/oxc/tree/main/crates/oxc_parser
#![allow(dead_code)]
#[derive(Debug, PartialEq)]
enum Token<'a> {
    Ident(&'a str),
    Num(&'a str),
    Arrow,
    LParen,
    RParen,
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer { src, pos: 0 }
    }
    fn take_while(&mut self, f: impl Fn(u8) -> bool) -> &'a str {
        let start = self.pos;
        let bytes = self.src.as_bytes();
        while self.pos < bytes.len() && f(bytes[self.pos]) {
            self.pos += 1;
        }
        &self.src[start..self.pos]
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;
    fn next(&mut self) -> Option<Token<'a>> {
        let bytes = self.src.as_bytes();
        loop {
            let b = *bytes.get(self.pos)?;
            match b {
                b' ' | b'\t' | b'\n' => self.pos += 1,
                b'(' => {
                    self.pos += 1;
                    return Some(Token::LParen);
                }
                b')' => {
                    self.pos += 1;
                    return Some(Token::RParen);
                }
                b'-' if bytes.get(self.pos + 1) == Some(&b'>') => {
                    self.pos += 2;
                    return Some(Token::Arrow);
                }
                b if b.is_ascii_alphabetic() || b == b'_' => {
                    return Some(Token::Ident(
                        self.take_while(|c| c.is_ascii_alphanumeric() || c == b'_'),
                    ));
                }
                b if b.is_ascii_digit() => {
                    return Some(Token::Num(self.take_while(|c| c.is_ascii_digit())));
                }
                _ => self.pos += 1, // unknown byte: skip (a real lexer would report an error with a span)
            }
        }
    }
}

fn main() {
    let src = "upload -> validate (transcode_hd 1080)";
    let tokens: Vec<Token> = Lexer::new(src).collect();
    println!("{tokens:#?}");
    assert_eq!(tokens[0], Token::Ident("upload"));
    assert_eq!(tokens[1], Token::Arrow);
    assert_eq!(tokens.last(), Some(&Token::RParen));
}
