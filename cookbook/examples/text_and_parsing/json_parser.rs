//! RECIPE 92 (parsing): a complete recursive-descent JSON parser with error positions, escapes, and a depth limit.
//! LEVEL: advanced
//! Use in: learning how real parsers handle EVERY edge case; template for any recursive format.   Run: cargo run --release --example json_parser
//! COMMON USES: config/IO boundaries, understanding serde_json internals, hardening recursive parsers against hostile input.
#![allow(dead_code)]
//!
//! One function per grammar rule (`value`, `array`, `object`, `string`, `number`) over a byte cursor. Production concerns shown:
//!  * errors carry a BYTE POSITION (turn into line/col with diagnostics_spans)       * escape sequences incl. \uXXXX and surrogate pairs
//!  * a DEPTH LIMIT: recursion on untrusted input must be bounded or `[[[[...` overflows the stack (see stage 8)
//!  * reject trailing garbage, leading zeros, lone surrogates, control characters in strings
//! Reference implementations: https://github.com/serde-rs/json/blob/master/src/de.rs and https://www.json.org/
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>), // Vec keeps insertion order and allows duplicate-key detection policies
}

#[derive(Debug, PartialEq)]
struct ParseError {
    msg: String,
    pos: usize,
}

const MAX_DEPTH: usize = 128;

struct P<'a> {
    src: &'a str,
    pos: usize,
    depth: usize,
}

type R<T> = Result<T, ParseError>;

impl<'a> P<'a> {
    fn err<T>(&self, msg: impl Into<String>) -> R<T> {
        Err(ParseError {
            msg: msg.into(),
            pos: self.pos,
        })
    }
    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\t' | b'\r')) {
            self.pos += 1;
        }
    }
    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, b: u8) -> R<()> {
        if self.eat(b) {
            Ok(())
        } else {
            self.err(format!("expected `{}`", b as char))
        }
    }
    fn lit(&mut self, word: &str, v: Json) -> R<Json> {
        if self.src[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(v)
        } else {
            self.err(format!("expected `{word}`"))
        }
    }

    fn value(&mut self) -> R<Json> {
        self.ws();
        if self.depth >= MAX_DEPTH {
            return self.err("nesting too deep");
        }
        match self.peek() {
            None => self.err("unexpected end of input"),
            Some(b'n') => self.lit("null", Json::Null),
            Some(b't') => self.lit("true", Json::Bool(true)),
            Some(b'f') => self.lit("false", Json::Bool(false)),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b'[') => {
                self.depth += 1;
                let r = self.array();
                self.depth -= 1;
                r
            }
            Some(b'{') => {
                self.depth += 1;
                let r = self.object();
                self.depth -= 1;
                r
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(c) => self.err(format!("unexpected `{}`", c as char)),
        }
    }

    fn array(&mut self) -> R<Json> {
        self.expect(b'[')?;
        let mut items = vec![];
        self.ws();
        if self.eat(b']') {
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            self.expect(b']')?;
            return Ok(Json::Arr(items));
        }
    }

    fn object(&mut self) -> R<Json> {
        self.expect(b'{')?;
        let mut fields: Vec<(String, Json)> = vec![];
        self.ws();
        if self.eat(b'}') {
            return Ok(Json::Obj(fields));
        }
        loop {
            self.ws();
            if self.peek() != Some(b'"') {
                return self.err("object keys must be strings");
            }
            let key = self.string()?;
            self.ws();
            self.expect(b':')?;
            let v = self.value()?;
            fields.push((key, v));
            self.ws();
            if self.eat(b',') {
                continue;
            }
            self.expect(b'}')?;
            return Ok(Json::Obj(fields));
        }
    }

    fn number(&mut self) -> R<Json> {
        let start = self.pos;
        self.eat(b'-');
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return self.err("leading zeros are not allowed");
                }
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return self.err("expected digit"),
        }
        if self.eat(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return self.err("expected digit after `.`");
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return self.err("expected digit in exponent");
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        self.src[start..self.pos]
            .parse::<f64>()
            .map(Json::Num)
            .or_else(|_| self.err("invalid number"))
    }

    fn hex4(&mut self) -> R<u32> {
        let h = self.src.get(self.pos..self.pos + 4).ok_or(ParseError {
            msg: "truncated \\u escape".into(),
            pos: self.pos,
        })?;
        let v = u32::from_str_radix(h, 16).or_else(|_| self.err("invalid \\u escape"))?;
        self.pos += 4;
        Ok(v)
    }

    fn string(&mut self) -> R<String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let Some(c) = self.src[self.pos..].chars().next() else {
                return self.err("unterminated string");
            };
            match c {
                '"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                '\\' => {
                    self.pos += 1;
                    let esc = self.peek();
                    self.pos += 1;
                    match esc {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'b') => out.push('\u{8}'),
                        Some(b'f') => out.push('\u{c}'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'u') => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                // high surrogate: must be followed by \uDC00..DFFF
                                if !self.src[self.pos..].starts_with("\\u") {
                                    return self.err("lone high surrogate");
                                }
                                self.pos += 2;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return self.err("invalid low surrogate");
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return self.err("lone low surrogate");
                            } else {
                                hi
                            };
                            out.push(char::from_u32(cp).ok_or(ParseError {
                                msg: "invalid code point".into(),
                                pos: self.pos,
                            })?);
                        }
                        _ => return self.err("invalid escape"),
                    }
                }
                c if (c as u32) < 0x20 => return self.err("control character in string"),
                c => {
                    out.push(c);
                    self.pos += c.len_utf8(); // advance by the UTF-8 length, staying on a char boundary
                }
            }
        }
    }
}

fn parse(src: &str) -> R<Json> {
    let mut p = P {
        src,
        pos: 0,
        depth: 0,
    };
    let v = p.value()?;
    p.ws();
    if p.pos != src.len() {
        return p.err("trailing characters");
    }
    Ok(v)
}

fn write_json(v: &Json, out: &mut String) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => write!(out, "{b}").unwrap(),
        Json::Num(n) => write!(out, "{n}").unwrap(),
        Json::Str(s) => write_str(s, out),
        Json::Arr(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json(x, out);
            }
            out.push(']');
        }
        Json::Obj(o) => {
            out.push('{');
            for (i, (k, x)) in o.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_str(k, out);
                out.push(':');
                write_json(x, out);
            }
            out.push('}');
        }
    }
}
fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn main() {
    let doc = r#" {"name":"flow","tags":["a","b\n"],"n":-12.5e1,"ok":true,"nested":{"x":null,"emoji":"\ud83d\ude80 and é"},"empty":[]} "#;
    let v = parse(doc).unwrap();
    println!("{v:#?}");
    let mut text = String::new();
    write_json(&v, &mut text);
    println!("{text}");
    assert_eq!(parse(&text).unwrap(), v, "round trip");

    for bad in [
        "",
        "[1,",
        "[1 2]",
        "{\"a\" 1}",
        "{1:2}",
        "01",
        "1.",
        "\"abc",
        "\"\\x\"",
        "[1] x",
        "\"\\ud800\"",
        "tru",
        "\"tab\there\"",
    ] {
        println!("{bad:>14?} -> {:?}", parse(bad).unwrap_err());
    }
    let deep = "[".repeat(10_000) + &"]".repeat(10_000);
    println!(
        "10k nesting -> {:?} (no stack overflow)",
        parse(&deep).unwrap_err()
    );
    assert_eq!(
        parse("[[[]]]").unwrap(),
        Json::Arr(vec![Json::Arr(vec![Json::Arr(vec![])])])
    );
    assert_eq!(parse("\"\\ud83d\\ude80\"").unwrap(), Json::Str("🚀".into()));
}
