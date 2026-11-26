//! Parser robustness: random token streams and mutated real programs must never
//! panic or hang, must keep the span-nesting invariant, and the checker must
//! survive whatever tree comes out.

mod common;
use common::*;
use pebble::sema::CheckOptions;
use std::time::{Duration, Instant};

const VOCAB: &[&str] = &[
    "fn",
    "struct",
    "enum",
    "const",
    "let",
    "mut",
    "if",
    "else",
    "match",
    "while",
    "loop",
    "break",
    "continue",
    "return",
    "as",
    "true",
    "false",
    "(",
    ")",
    "{",
    "}",
    "[",
    "]",
    ",",
    ";",
    ":",
    "::",
    ".",
    "..",
    "->",
    "=>",
    "=",
    "==",
    "!=",
    "<",
    "<=",
    ">",
    ">=",
    "+",
    "-",
    "*",
    "/",
    "%",
    "+=",
    "&&",
    "||",
    "!",
    "|",
    "_",
    "x",
    "y",
    "main",
    "Point",
    "Option",
    "Some",
    "0",
    "1",
    "42",
    "\"s\"",
    "\"unterminated",
    "#",
    "/*",
    "*/",
    "//",
    "99999999999999999999",
    "12ab",
    "é",
    "\n",
];

fn parse_checked(src: &str) {
    let t = Instant::now();
    let r = pebble::syntax::parse(src);
    assert!(!r.fuel_exhausted, "parser fuel exhausted on {src:?}");
    if let Err(e) = check_ast_invariants(&r.ast, src.len()) {
        panic!("invariant violated: {e}\nsource: {src:?}");
    }
    assert!(
        t.elapsed() < Duration::from_secs(2),
        "parse too slow on {src:?}"
    );
}

#[test]
fn random_token_streams() {
    big_stack(|| {
        let mut rng = Rng::new(1);
        for _ in 0..50_000 {
            let n = rng.below(60);
            let src: Vec<&str> = (0..n).map(|_| *rng.pick(VOCAB)).collect();
            parse_checked(&src.join(" "));
        }
    });
}

#[test]
fn random_bytes_and_deep_nesting() {
    big_stack(|| {
        let mut rng = Rng::new(2);
        for _ in 0..3_000 {
            let n = rng.below(80);
            let s: String = (0..n)
                .map(|_| char::from_u32(rng.below(0x3000) as u32 + 1).unwrap_or('?'))
                .collect();
            parse_checked(&s);
        }
        for depth in [10, 99, 100, 101, 500, 5_000] {
            for (open, close) in [
                ("(", ")"),
                ("[", "]"),
                ("{", "}"),
                ("if x {", "}"),
                ("-", ""),
                ("f(", ")"),
            ] {
                let src = format!(
                    "fn main() {{ let a = {}1{}; }}",
                    open.repeat(depth),
                    close.repeat(depth)
                );
                parse_checked(&src);
            }
        }
        let long = format!("fn main() {{ let a = {}; }}", vec!["1"; 5_000].join(" + "));
        parse_checked(&long);
        pebble::analyze(&long, &CheckOptions::default());
    });
}

fn corpus() -> Vec<String> {
    let mut v = Vec::new();
    for dir in ["tests/run", "examples", "tests/ui"] {
        for f in files_with_ext(&manifest_dir().join(dir), "peb") {
            let s = std::fs::read_to_string(f).unwrap();
            if s.len() < 4_000 {
                v.push(s);
            }
        }
    }
    v
}

fn mutate(rng: &mut Rng, src: &str) -> String {
    let mut chars: Vec<char> = src.chars().collect();
    for _ in 0..1 + rng.below(4) {
        if chars.is_empty() {
            break;
        }
        let i = rng.below(chars.len());
        match rng.below(6) {
            0 => {
                chars.remove(i);
            }
            1 => {
                let j = (i + rng.below(30)).min(chars.len());
                chars.drain(i..j);
            }
            2 => {
                let tok: Vec<char> = rng.pick(VOCAB).chars().collect();
                for (k, c) in tok.into_iter().enumerate() {
                    chars.insert(i + k, c);
                }
            }
            3 => chars.truncate(i),
            4 => {
                let j = rng.below(chars.len());
                chars.swap(i, j);
            }
            _ => {
                let j = (i + rng.below(40)).min(chars.len());
                let dup: Vec<char> = chars[i..j].to_vec();
                for (k, c) in dup.into_iter().enumerate() {
                    chars.insert(i + k, c);
                }
            }
        }
    }
    chars.into_iter().collect()
}

#[test]
fn mutated_programs_never_panic_parser_or_checker() {
    let corpus = corpus();
    assert!(corpus.len() > 100);
    big_stack(move || {
        let mut rng = Rng::new(3);
        for i in 0..6_000 {
            let base = &corpus[i % corpus.len()];
            let src = mutate(&mut rng, base);
            parse_checked(&src);
            let t = Instant::now();
            let an = pebble::analyze(&src, &CheckOptions { warn_shadow: true });
            assert!(
                t.elapsed() < Duration::from_secs(3),
                "analysis too slow on {src:?}"
            );
            // Rendering must also be total.
            let file = pebble::source::SourceFile::new("m.peb", src);
            let _ = pebble::diag::render_all(&an.diagnostics, &file, &Default::default());
            let _ = pebble::diag::to_json_array(&an.diagnostics, &file);
        }
    });
}
