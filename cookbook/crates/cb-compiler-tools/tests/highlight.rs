//! Highlighter and editor-grammar tests.

mod common;
use common::*;
use pebble_tools::analysis::syntax::{TokenKind, lex, token::keyword};
use pebble_tools::highlight::{self, Class, HlToken};
use pebble_tools::textmate;
use serde_json::Value;

fn assert_tiles(src: &str, toks: &[HlToken]) {
    let mut at = 0usize;
    let mut text = String::new();
    for t in toks {
        assert_eq!(t.span.start as usize, at, "gap or overlap before {t:?}");
        assert!(t.span.end > t.span.start, "empty token {t:?}");
        text.push_str(t.span.slice(src));
        at = t.span.end as usize;
    }
    assert_eq!(at, src.len());
    assert_eq!(
        text, src,
        "concatenating token texts must reproduce the source"
    );
}

#[test]
fn lexical_and_semantic_output_round_trips_on_the_corpus_and_on_broken_input() {
    big_stack(|| {
        let corpus = corpus();
        for (_, src) in &corpus {
            assert_tiles(src, &highlight::lexical(src));
            let an = pebble_tools::analysis::analyze(src);
            assert_tiles(src, &highlight::semantic(src, &an));
        }
        // Unterminated strings, stray characters, emoji, unterminated block comments.
        let mut rng = Rng::new(5);
        for _ in 0..1500 {
            let mut s = rng.pick(&corpus).1.clone();
            for _ in 0..3 {
                let mut at = rng.below(s.len() + 1);
                while !s.is_char_boundary(at) {
                    at -= 1;
                }
                s.insert_str(
                    at,
                    rng.pick(&["\"", "/*", "@", "😀", "\\", "\"\\u{", "1x", "\\n"]),
                );
            }
            assert_tiles(&s, &highlight::lexical(&s));
        }
        assert_tiles("", &highlight::lexical(""));
    });
}

fn class_of(src: &str, toks: &[HlToken], word: &str, nth: usize) -> HlToken {
    let mut seen = 0;
    for t in toks {
        if t.span.slice(src) == word {
            if seen == nth {
                return t.clone();
            }
            seen += 1;
        }
    }
    panic!("no token `{word}` #{nth}");
}

#[test]
fn lexical_classes() {
    let src = "/// doc\nfn f(x: int) -> str { // c\n    let s = \"a\\n\\u{1F600}b\"; if x == 1_0 { return Shape::Circle(1); } @ }\n";
    let toks = highlight::lexical(src);
    let c = |w: &str, n: usize| class_of(src, &toks, w, n).class;
    assert_eq!(c("/// doc", 0), Class::DocComment);
    assert_eq!(c("fn", 0), Class::Keyword);
    assert_eq!(c("f", 0), Class::Function);
    assert_eq!(c("int", 0), Class::Type);
    assert_eq!(c("if", 0), Class::ControlKeyword);
    assert_eq!(c("return", 0), Class::ControlKeyword);
    assert_eq!(c("\\n", 0), Class::Escape);
    assert_eq!(c("\\u{1F600}", 0), Class::Escape);
    assert_eq!(c("1_0", 0), Class::Number);
    assert_eq!(c("==", 0), Class::Operator);
    assert_eq!(c("// c", 0), Class::Comment);
    assert_eq!(c("Circle", 0), Class::EnumVariant);
    assert_eq!(c("Shape", 0), Class::Type);
    assert_eq!(c("@", 0), Class::Error);
    let all: std::collections::HashSet<Class> = toks.iter().map(|t| t.class).collect();
    assert!(all.contains(&Class::String));
}

#[test]
fn semantic_classes_modifiers_and_fading() {
    big_stack(|| {
        let src = "struct P { x: int }\nenum E { A, B }\nconst K: int = 1;\n/// Deprecated: use new_f\nfn old_f() {}\nfn unused_fn() {}\nfn main() {\n    let mut m = P { x: K };\n    let dead = 1;\n    let e = E::A;\n    old_f();\n    m.x = 2;\n    println(to_str(m.x));\n    match e { E::A => {}, E::B => {} }\n}\nfn takes(arg: int) -> int { arg }\n";
        let an = pebble_tools::analysis::analyze(src);
        let toks = highlight::semantic(src, &an);
        assert_tiles(src, &toks);
        let at = |w: &str, n: usize| class_of(src, &toks, w, n);
        assert_eq!(at("P", 0).class, Class::Type);
        assert_eq!(at("x", 0).class, Class::Property);
        assert_eq!(at("x", 2).class, Class::Property, "field use");
        assert_eq!(at("K", 0).class, Class::Constant);
        assert_eq!(at("main", 0).class, Class::Function);
        assert_eq!(at("A", 0).class, Class::EnumVariant);
        let m = at("m", 0);
        assert_eq!(
            (m.class, m.mods.mutable, m.mods.declaration),
            (Class::Variable, true, true)
        );
        assert!(
            at("m", 1).mods.mutable,
            "uses of a `mut` variable are marked too"
        );
        assert!(at("dead", 0).mods.unused, "unused variable is faded");
        assert!(!at("e", 0).mods.unused);
        assert!(at("unused_fn", 0).mods.unused);
        let p = at("arg", 0);
        assert_eq!((p.class, p.mods.declaration), (Class::Parameter, true));
        assert_eq!(at("arg", 1).class, Class::Parameter);
        assert!(at("old_f", 1).mods.deprecated, "deprecated via doc comment");
        assert!(at("println", 0).mods.builtin);
    });
}

#[test]
fn ansi_output_strips_back_to_the_source() {
    for (_, src) in corpus().iter().take(20) {
        let ansi = highlight::render_ansi(src, &highlight::lexical(src));
        let mut plain = String::new();
        let mut chars = ansi.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for e in chars.by_ref() {
                    if e == 'm' {
                        break;
                    }
                }
            } else {
                plain.push(c);
            }
        }
        assert_eq!(&plain, src);
        assert!(ansi.contains("\x1b[38;5;"), "uses 256-colour codes");
    }
}

/// A minimal well-formedness check for the HTML we emit: only balanced `<span class="..">`,
/// no raw `<`/`>` in text, only the entities we produce.
fn assert_valid_html(html: &str) {
    assert!(html.starts_with("<!DOCTYPE html>"));
    let body = &html[html.find("<code>").unwrap() + 6..html.find("</code>").unwrap()];
    let mut depth = 0;
    let mut rest = body;
    while let Some(i) = rest.find(['<', '>', '&']) {
        let c = rest.as_bytes()[i];
        match c {
            b'<' => {
                if rest[i..].starts_with("<span class=\"") {
                    let close = rest[i..].find("\">").expect("tag end");
                    assert!(!rest[i + 13..i + close].contains(['<', '>', '"']));
                    depth += 1;
                    rest = &rest[i + close + 2..];
                } else {
                    assert!(
                        rest[i..].starts_with("</span>"),
                        "unexpected tag at {:?}",
                        &rest[i..(i + 20).min(rest.len())]
                    );
                    depth -= 1;
                    assert!(depth >= 0);
                    rest = &rest[i + 7..];
                }
            }
            b'>' => panic!("raw `>` in text"),
            _ => {
                let end = rest[i..].find(';').expect("entity must end with ;");
                assert!(
                    ["&amp", "&lt", "&gt", "&quot", "&#39"].contains(&&rest[i..i + end]),
                    "bad entity {}",
                    &rest[i..i + end]
                );
                rest = &rest[i + end + 1..];
            }
        }
    }
    assert_eq!(depth, 0, "unbalanced spans");
}

#[test]
fn html_output_is_valid_and_escaped() {
    let src = "fn main() { let s = \"<script>alert('x') & </span>\"; if 1 < 2 && 3 > 2 {} }\n";
    let html = highlight::render_html(src, &highlight::lexical(src), "t <&> \"title\"");
    assert_valid_html(&html);
    assert!(!html.contains("<script>"), "user text must be escaped");
    assert!(html.contains("&lt;script&gt;") && html.contains("&amp;") && html.contains("&#39;"));
    assert!(
        html.contains("prefers-color-scheme:dark")
            && html.contains("<title>t &lt;&amp;&gt; &quot;title&quot;</title>")
    );
    for (_, s) in corpus().iter().take(15) {
        assert_valid_html(&highlight::render_html(s, &highlight::lexical(s), "x"));
    }
}

#[test]
fn json_output_parses() {
    let src = "fn main() { println(\"hi\"); }\n";
    let v: Value =
        serde_json::from_str(&highlight::render_json(src, &highlight::lexical(src))).unwrap();
    let texts: Vec<&str> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts.concat(), src.replace([' ', '\n'], ""));
}

// ---------------------------------------------------------------- generated editor files

#[test]
fn grammar_keywords_equal_the_lexers_keyword_table() {
    let kws = textmate::keywords();
    assert_eq!(kws.len(), 17);
    // Forward: every keyword the grammar lists is a keyword to the lexer, and lexes as one.
    for k in &kws {
        let kind = keyword(k).unwrap_or_else(|| panic!("`{k}` is not a lexer keyword"));
        assert!(kind.is_keyword());
        assert_eq!(lex(k).0[0].kind, kind);
    }
    // Backward, exhaustively for short words: nothing the lexer treats as a keyword is missing.
    let mut word = Vec::new();
    fn all_words(len: usize, word: &mut Vec<u8>, f: &mut impl FnMut(&str)) {
        if word.len() == len {
            f(std::str::from_utf8(word).unwrap());
            return;
        }
        for c in b'a'..=b'z' {
            word.push(c);
            all_words(len, word, f);
            word.pop();
        }
    }
    for len in 1..=4 {
        all_words(len, &mut word, &mut |w| {
            if keyword(w).is_some() {
                assert!(
                    kws.contains(&w),
                    "lexer keyword `{w}` missing from the grammar list"
                );
            }
        });
    }
    // And for every word seen in the corpus and every fixed-spelling token the lexer produces.
    for (_, src) in corpus() {
        for t in lex(&src).0 {
            if t.kind.is_keyword() {
                assert!(kws.contains(&t.span.slice(&src)));
            }
            if t.kind.symbol().is_some() {
                assert!(
                    textmate::all_kinds().contains(&t.kind),
                    "token kind {:?} not covered by textmate::all_kinds",
                    t.kind
                );
            }
        }
    }
    for k in textmate::all_kinds() {
        assert!(textmate::kind_guard(&k) && k.symbol().is_some());
    }
    assert!(!textmate::kind_guard(&TokenKind::Ident));
    // The grammar's regexes list exactly these words.
    let g = textmate::grammar();
    let mut from_grammar: Vec<String> = Vec::new();
    for rule in g["repository"]["keywords"]["patterns"].as_array().unwrap() {
        let m = rule["match"].as_str().unwrap();
        let inner = m.trim_start_matches("\\b(").trim_end_matches(")\\b");
        from_grammar.extend(inner.split('|').map(String::from));
    }
    from_grammar.sort();
    let mut want: Vec<String> = kws.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(from_grammar, want);
}

#[test]
fn editor_files_are_up_to_date_and_valid_json() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let bless = std::env::var_os("BLESS").is_some();
    for (rel, content) in textmate::editor_files() {
        let path = root.join(rel);
        if bless {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &content).unwrap();
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            on_disk, content,
            "{rel} is stale: run `pebble-tools gen-editors` (or BLESS=1)"
        );
        if rel.ends_with(".json") {
            serde_json::from_str::<Value>(&content).unwrap_or_else(|e| panic!("{rel}: {e}"));
        }
    }
    let pkg: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("editors/vscode/package.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        pkg["contributes"]["languages"][0]["configuration"],
        "./language-configuration.json"
    );
    assert!(
        std::fs::read_to_string(root.join("editors/vscode/extension.js"))
            .unwrap()
            .contains("args: ['lsp']")
    );
}
