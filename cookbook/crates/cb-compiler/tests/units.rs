//! Unit-level tests of public APIs: line index, lexer, exhaustiveness, index queries, suggestions.

mod common;
use common::*;
use pebble::diag::{closest_match, edit_distance};
use pebble::sema::{CheckOptions, Ty};
use pebble::source::{LineIndex, SourceFile, Span};
use pebble::syntax::{CommentKind, TokenKind};

fn codes(src: &str) -> Vec<&'static str> {
    let src = src.to_string();
    big_stack(move || {
        pebble::analyze(&src, &CheckOptions::default())
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect()
    })
}

const PRELUDE: &str = "enum Option<T> { None, Some(T) }\nenum Color { Red, Green, Blue }\n";

fn with_main(body: &str) -> String {
    format!("{PRELUDE}fn main() {{\n{body}\n}}\n")
}

// ---------------------------------------------------------------- line index

#[test]
fn line_index_utf8_chars_utf16() {
    // 'é' = 2 bytes/1 char/1 unit, '😀' = 4 bytes/1 char/2 units, '日' = 3 bytes/1 char/1 unit.
    let text = "ab\nxé😀日z\r\nlast";
    let li = LineIndex::new(text);
    assert_eq!(li.line_count(), 3);
    let z = text.find('z').unwrap();
    let lc8 = li.line_col_utf8(text, z);
    assert_eq!((lc8.line, lc8.col), (1, 1 + 2 + 4 + 3));
    let lcc = li.line_col_chars(text, z);
    assert_eq!((lcc.line, lcc.col), (1, 4));
    let lc16 = li.line_col_utf16(text, z);
    assert_eq!((lc16.line, lc16.col), (1, 1 + 1 + 2 + 1));
    assert_eq!(li.offset_from_utf16(text, 1, 5), Some(z));
    // Inside the surrogate pair rounds down to the emoji start.
    assert_eq!(
        li.offset_from_utf16(text, 1, 3),
        Some(text.find('😀').unwrap())
    );
    // Past the end of the line clamps to the line end (before `\r\n`).
    assert_eq!(li.offset_from_utf16(text, 1, 99), Some(z + 1));
    assert_eq!(li.offset_from_utf16(text, 7, 0), None);
    assert_eq!(li.line_text(text, 1), "xé😀日z");
    // Mid-char offsets never panic.
    let _ = li.line_col_utf16(text, text.find('é').unwrap() + 1);
}

#[test]
fn round_trip_every_char_boundary() {
    let text = "héllo\n日本語 😀\n\nend";
    let li = LineIndex::new(text);
    for (i, _) in text.char_indices() {
        let lc = li.line_col_utf16(text, i);
        assert_eq!(
            li.offset_from_utf16(text, lc.line, lc.col),
            Some(i),
            "offset {i}"
        );
    }
}

#[test]
fn span_helpers() {
    let s = Span::new(2, 5);
    assert!(s.contains(2) && !s.contains(5) && s.contains_span(Span::new(3, 4)));
    assert_eq!(s.to(Span::new(7, 9)), Span::new(2, 9));
    assert_eq!(Span::new(0, 100).slice("short"), "");
    assert_eq!(SourceFile::new("f", "abc").slice(Span::new(1, 2)), "b");
}

// ---------------------------------------------------------------- lexer

#[test]
fn lexer_never_fails_and_keeps_comments() {
    let src = "// line\n/// doc\n/* a /* nested */ b */ fn x \"s\\n\" 1_000 #";
    let (toks, comments, diags) = pebble::syntax::lex(src);
    assert_eq!(comments.len(), 3);
    assert_eq!(comments[0].kind, CommentKind::Line);
    assert_eq!(comments[1].kind, CommentKind::DocLine);
    assert_eq!(comments[2].kind, CommentKind::Block);
    assert_eq!(comments[2].text, "/* a /* nested */ b */");
    assert!(toks.iter().any(|t| t.kind == TokenKind::Int(1000)));
    assert!(toks.iter().any(|t| t.kind == TokenKind::Str("s\n".into())));
    assert_eq!(toks.last().map(|t| &t.kind), Some(&TokenKind::Eof));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E0001");
}

#[test]
fn lexer_error_codes() {
    for (src, code) in [
        ("\"abc", "E0002"),
        ("/* x", "E0003"),
        ("\"\\q\"", "E0004"),
        ("99999999999999999999", "E0005"),
        ("1a", "E0006"),
        ("\\", "E0001"),
    ] {
        let (_, _, d) = pebble::syntax::lex(src);
        assert_eq!(d.first().map(|d| d.code), Some(code), "{src}");
    }
}

#[test]
fn parser_returns_error_nodes_and_comments() {
    let r = pebble::syntax::parse("// hi\nfn main() { let x = ; foo(; }\nstruct");
    assert_eq!(r.comments.len(), 1);
    assert!(r.diagnostics.len() >= 3);
    assert!(r.ast.items.len() >= 2);
    assert!(r.diagnostics.iter().all(|d| d.is_error()));
}

// ---------------------------------------------------------------- exhaustiveness

#[test]
fn exhaustive_enum_and_missing_variant() {
    assert!(!codes(&with_main("let c = Color::Red; match c { Color::Red => {}, Color::Green => {}, Color::Blue => {} }")).contains(&"E0401"));
    assert!(
        codes(&with_main(
            "let c = Color::Red; match c { Color::Red => {}, Color::Green => {} }"
        ))
        .contains(&"E0401")
    );
}

#[test]
fn exhaustive_nested_and_tuple() {
    let ok = "let p = (Option::Some(true), 1); match p { (Option::Some(true), _) => {}, (Option::Some(false), _) => {}, (Option::None, _) => {} }";
    assert!(!codes(&with_main(ok)).contains(&"E0401"));
    let bad = "let p = (Option::Some(true), 1); match p { (Option::Some(true), _) => {}, (Option::None, _) => {} }";
    assert!(codes(&with_main(bad)).contains(&"E0401"));
    let bad2 = "let p = (true, false); match p { (true, true) => {}, (false, _) => {} }";
    assert!(codes(&with_main(bad2)).contains(&"E0401"));
}

#[test]
fn exhaustive_guards_do_not_count() {
    assert!(codes(&with_main("let n = 1; match n { x if x > 0 => {}, }")).contains(&"E0401"));
    assert!(
        !codes(&with_main(
            "let n = 1; match n { x if x > 0 => {}, _ => {} }"
        ))
        .contains(&"E0401")
    );
    assert!(
        codes(&with_main(
            "let b = true; match b { true if 1 > 0 => {}, false => {} }"
        ))
        .contains(&"E0401")
    );
}

#[test]
fn exhaustive_or_patterns_and_useless_arms() {
    assert!(
        !codes(&with_main(
            "let c = Color::Red; match c { Color::Red | Color::Green => {}, Color::Blue => {} }"
        ))
        .contains(&"E0401")
    );
    // The second alternative is already covered by the first arm.
    let c = codes(&with_main(
        "let c = Color::Red; match c { Color::Red => {}, Color::Red | Color::Green | Color::Blue => {} }",
    ));
    assert!(c.contains(&"W0504") && !c.contains(&"E0401"));
    // A wildcard after full coverage is unreachable.
    assert!(
        codes(&with_main(
            "let c = Color::Red; match c { Color::Red | Color::Green | Color::Blue => {}, _ => {} }"
        ))
        .contains(&"W0504")
    );
    // Integers are open: only a wildcard closes them.
    assert!(codes(&with_main("match 3 { 0 => {}, 1 => {} }")).contains(&"E0401"));
    assert!(codes(&with_main("match \"a\" { \"a\" => {}, \"b\" => {} }")).contains(&"E0401"));
}

#[test]
fn irrefutable_and_refutable_lets() {
    assert!(
        !codes(&with_main(
            "let (a, b) = (1, true); let _x = a; let _y = b;"
        ))
        .contains(&"E0402")
    );
    assert!(
        codes(&with_main(
            "let Option::Some(x) = Option::Some(1); let _z = x;"
        ))
        .contains(&"E0402")
    );
}

#[test]
fn witnesses_are_reported() {
    let src = with_main(
        "let c = Option::Some(Color::Red); match c { Option::Some(Color::Red) => {}, Option::None => {} }",
    );
    let src2 = src.clone();
    let msg = big_stack(move || {
        pebble::analyze(&src2, &CheckOptions::default())
            .diagnostics
            .iter()
            .find(|d| d.code == "E0401")
            .map(|d| d.message.clone())
            .unwrap_or_default()
    });
    assert!(
        msg.contains("Option::Some(Color::Green)") && msg.contains("Option::Some(Color::Blue)"),
        "{msg}"
    );
}

// ---------------------------------------------------------------- typing

#[test]
fn inference_and_generics() {
    let ok = "fn id<T>(x: T) -> T { x }\nfn main() { let a = id(1); let b = id(\"s\"); let mut xs = []; push(xs, a); println(b + to_str(xs[0])); }";
    assert!(codes(ok).is_empty(), "{:?}", codes(ok));
    assert!(codes("fn main() { let xs = []; }").contains(&"E0310"));
    assert!(codes("fn f<T>(a: T, b: T) {}\nfn main() { f(1, true); }").contains(&"E0301"));
    assert!(codes("fn f<T>(a: T) -> int { a + 1 }\nfn main() {}").contains(&"E0304"));
}

#[test]
fn never_type_rules() {
    assert!(codes("fn f(c: bool) -> int { let x: int = if c { 1 } else { return 0 }; x }\nfn main() { println(to_str(f(true))); }").is_empty());
    assert!(codes("fn f() -> int { loop { } }\nfn main() { println(to_str(f())); }").is_empty());
    assert!(
        codes("fn f() -> int { loop { break 1; } }\nfn main() { println(to_str(f())); }")
            .is_empty()
    );
}

#[test]
fn errors_do_not_cascade() {
    let c = codes("fn main() { let x = nope; let y = x + 1; let z: str = y; println(z); }");
    assert_eq!(c, vec!["E0201"]);
}

// ---------------------------------------------------------------- suggestions

#[test]
fn edit_distance_and_closest() {
    assert_eq!(edit_distance("kitten", "sitting"), 3);
    assert_eq!(edit_distance("ab", "ba"), 1);
    assert_eq!(closest_match("lenght", ["length", "width"]), Some("length"));
    assert_eq!(closest_match("zzz", ["length", "width"]), None);
    assert_eq!(closest_match("Foo", ["foo"]), Some("foo"));
}

// ---------------------------------------------------------------- semantic index

#[test]
fn semantic_index_queries() {
    big_stack(|| {
        let src = "struct P { x: int }\nfn add(a: int, b: int) -> int { a + b }\nfn main() {\n    let p = P { x: 1 };\n    let r = add(p.x, 2);\n    println(to_str(r));\n}\n";
        let an = pebble::analyze(src, &CheckOptions::default());
        assert!(!an.has_errors());
        let ix = &an.index;
        // Type of `p.x` is int.
        let off = src.find("p.x").unwrap() + 2;
        assert_eq!(ix.type_at(off), Some(&Ty::Int));
        // Definition of `add` at the call site.
        let call = src.find("add(p").unwrap();
        let def = ix.definition_at(call).unwrap();
        assert_eq!(def.slice(src), "add");
        // References of `r`: its use in to_str.
        let rdef = ix.def_at(src.find("(r)").unwrap() + 1).unwrap();
        assert_eq!(ix.references(rdef, false).len(), 1);
        assert_eq!(ix.references(rdef, true).len(), 2);
        // Field definition.
        assert_eq!(ix.definition_at(off).unwrap().slice(src), "x");
        // Outline.
        let names: Vec<String> = ix.symbols().iter().map(|s| s.name.clone()).collect();
        for want in ["P", "x", "add", "a", "b", "main", "p", "r"] {
            assert!(
                names.iter().any(|n| n == want),
                "missing symbol {want} in {names:?}"
            );
        }
        // Completion: `r` is not visible before its declaration, `p` is.
        let before_r = src.find("let r").unwrap();
        let labels = |o: usize| {
            ix.completions_at(o)
                .into_iter()
                .map(|c| c.label)
                .collect::<Vec<_>>()
        };
        assert!(
            labels(before_r).contains(&"p".to_string())
                && !labels(before_r).contains(&"r".to_string())
        );
        assert!(labels(src.find("println").unwrap()).contains(&"r".to_string()));
        assert!(
            labels(before_r).contains(&"add".to_string())
                && labels(before_r).contains(&"len".to_string())
        );
        assert!(
            ix.fields_of(&an.hir.def(an.hir.structs[0].def).ty.clone().unwrap())
                .iter()
                .any(|c| c.label == "x")
        );
        assert!(ix.node_at(off).is_some());
    });
}

// ---------------------------------------------------------------- interpreter limits

#[test]
fn interpreter_limits_and_oracle_semantics() {
    big_stack(|| {
        let run = |src: &str| {
            let an = pebble::analyze(src, &CheckOptions::default());
            assert!(!an.has_errors(), "{:?}", an.diagnostics);
            let mut out = Vec::new();
            let r = pebble::interp::run_with(
                &an.hir,
                &mut out,
                &pebble::interp::RunOptions {
                    max_depth: 50,
                    fuel: 10_000,
                },
            );
            (String::from_utf8(out).unwrap(), r)
        };
        let (_, r) = run("fn f(n: int) -> int { f(n + 1) }\nfn main() { f(0); }");
        assert_eq!(
            r.unwrap_err().kind,
            pebble::interp::RuntimeErrorKind::DepthLimit
        );
        let (_, r) = run("fn main() { loop { } }");
        assert_eq!(
            r.unwrap_err().kind,
            pebble::interp::RuntimeErrorKind::FuelExhausted
        );
        let (out, r) = run(
            "fn main() { println(\"x\"); let a = 9223372036854775807; println(to_str(a + 1)); }",
        );
        assert_eq!(out, "x\n");
        let e = r.unwrap_err();
        assert_eq!(e.kind, pebble::interp::RuntimeErrorKind::Overflow);
        assert_eq!(e.trace.len(), 1);
    });
}
