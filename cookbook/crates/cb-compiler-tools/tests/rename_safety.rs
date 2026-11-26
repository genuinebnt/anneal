//! Rename must never change what a program does. Random renames are drawn from the
//! names a program ALREADY uses (plus built-ins), because those are the ones that can
//! capture or be captured; every accepted rename is run in the interpreter before and
//! after. Hand-written cases pin the kinds of clash the server must refuse.

mod common;
use common::client::*;
use common::*;
use pebble_tools::analysis::sema::DefKind;
use pebble_tools::analysis::{lex, syntax::TokenKind};
use pebble_tools::lsp::SyncKind;
use serde_json::{Value, json};

const URI: &str = "file:///rn.peb";

/// Output plus runtime error, with `from` renamed back to `to` in whole identifiers of
/// the error only (stack traces print function names).
fn behaviour(src: &str, trace_names: Option<(&str, &str)>) -> Option<(String, Option<String>)> {
    let an = pebble_tools::analysis::analyze(src);
    if an.has_errors() {
        return None;
    }
    let (out, err) = pebble_tools::analysis::run(&an);
    let err = err.map(|e| match trace_names {
        Some((from, to)) => e
            .lines()
            .map(|l| {
                if l.trim() == format!("at {from}") {
                    l.replace(from, to)
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        None => e,
    });
    Some((out, err))
}

fn identifiers(src: &str) -> Vec<String> {
    let (tokens, _, _) = lex(src);
    let mut v: Vec<String> = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Ident)
        .map(|t| t.span.slice(src).to_string())
        .collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn random_renames_over_the_run_corpus_never_change_behaviour() {
    big_stack(|| {
        let (mut accepted, mut accepted_reusing_a_name, mut refused, mut programs) = (0, 0, 0, 0);
        for (n, path) in peb_files("tests/run").into_iter().enumerate() {
            let src = std::fs::read_to_string(&path).unwrap();
            let Some(before) = behaviour(&src, None) else {
                continue;
            };
            let an = pebble_tools::analysis::analyze(&src);
            let targets: Vec<_> = an
                .index
                .defs
                .iter()
                .filter(|d| !d.name.is_empty() && d.name != "main" && !d.span.is_empty())
                .collect();
            if targets.is_empty() {
                continue;
            }
            programs += 1;
            let mut pool = identifiers(&src);
            pool.extend(
                ["len", "println", "push", "to_str", "fresh_a", "fresh_b"].map(String::from),
            );
            let mut c = Client::started(SyncKind::Incremental);
            c.open(URI, &src);
            let mut rng = Rng::new(n as u64 + 99);
            for _ in 0..40 {
                let d = rng.pick(&targets);
                let new = rng.pick(&pool).clone();
                if new == d.name {
                    continue;
                }
                let (line, character) = lsp_position(&src, d.span.start as usize);
                let r = c.request("textDocument/rename", json!({"textDocument": {"uri": URI}, "position": {"line": line, "character": character}, "newName": new}));
                if r.get("error").is_some() {
                    assert_eq!(r["error"]["code"], -32602, "{r}");
                    refused += 1;
                    continue;
                }
                let renamed = apply_lsp_edits(&src, &r["result"]["changes"][URI]);
                // Stack traces print function names, so only a renamed function is mapped back.
                let after = behaviour(
                    &renamed,
                    (d.kind == DefKind::Fn).then_some((new.as_str(), d.name.as_str())),
                );
                assert_eq!(
                    after.as_ref(),
                    Some(&before),
                    "{}: renaming `{}` ({:?}) to `{new}` changed the program:\n{renamed}",
                    path.display(),
                    d.name,
                    d.kind
                );
                accepted += 1;
                accepted_reusing_a_name += usize::from(!new.starts_with("fresh_"));
            }
            assert_eq!(c.server.internal_errors, 0);
        }
        println!(
            "{programs} programs: {accepted} accepted ({accepted_reusing_a_name} reusing an existing name), {refused} refused"
        );
        assert!(
            programs > 30 && accepted > 300 && accepted_reusing_a_name > 100 && refused > 100,
            "weak coverage: {programs} programs, {accepted}/{accepted_reusing_a_name} accepted, {refused} refused"
        );
    });
}

/// Rename the `nth` occurrence of the identifier `old`.
fn try_rename(src: &str, old: &str, nth: usize, new: &str) -> Result<String, String> {
    assert!(
        behaviour(src, None).is_some(),
        "the case must be a valid program:\n{src}"
    );
    let (tokens, _, _) = lex(src);
    let at = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Ident && t.span.slice(src) == old)
        .nth(nth)
        .expect("occurrence")
        .span
        .start as usize;
    let mut c = Client::started(SyncKind::Incremental);
    c.open(URI, src);
    let (line, character) = lsp_position(src, at);
    let params = json!({"textDocument": {"uri": URI}, "position": {"line": line, "character": character}, "newName": new});
    let prepare = c.request("textDocument/prepareRename", params.clone());
    let rename = c.request("textDocument/rename", params);
    // `prepareRename` with a proposed name and `rename` give the same verdict.
    assert_eq!(
        prepare.get("error").map(|e| &e["message"]),
        rename.get("error").map(|e| &e["message"]),
        "{prepare} / {rename}"
    );
    match rename.get("error") {
        Some(e) => Err(e["message"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| Value::to_string(e))),
        None => Ok(apply_lsp_edits(src, &rename["result"]["changes"][URI])),
    }
}

#[test]
fn clashes_in_every_scope_are_refused() {
    big_stack(|| {
        let cases: &[(&str, &str, &str, usize, &str)] = &[
            (
                "local captures a later use of a function",
                "fn two() -> int { 2 }\nfn main() { let a = 1; println(to_str(a + two())); }",
                "a",
                0,
                "two",
            ),
            (
                "parameter onto parameter",
                "fn f(a: int, b: int) -> int { a - b }\nfn main() { println(to_str(f(5, 3))); }",
                "a",
                0,
                "b",
            ),
            (
                "inner local would capture the outer one's use",
                "fn main() { let x = 1; if true { let y = 2; println(to_str(x + y)); } }",
                "y",
                0,
                "x",
            ),
            (
                "outer local would be shadowed",
                "fn main() { let x = 1; if true { let y = 2; println(to_str(x + y)); } }",
                "x",
                0,
                "y",
            ),
            (
                "match binding onto a parameter used in the arm",
                "fn f(k: int, o: Option<int>) -> int { match o { Option::Some(v) => v + k, Option::None => k } }\nenum Option<T> { Some(T), None }\nfn main() { println(to_str(f(1, Option::Some(2)))); }",
                "v",
                0,
                "k",
            ),
            (
                "item onto item",
                "fn a() -> int { 1 }\nfn b() -> int { 2 }\nfn main() { println(to_str(a() + b())); }",
                "a",
                0,
                "b",
            ),
            (
                "function onto a built-in",
                "fn show(x: int) { println(to_str(x)); }\nfn main() { show(1); }",
                "show",
                0,
                "println",
            ),
            (
                "field onto field",
                "struct P { x: int, y: int }\nfn main() { let p = P { x: 1, y: 2 }; println(to_str(p.x - p.y)); }",
                "x",
                0,
                "y",
            ),
            (
                "variant onto variant",
                "enum E { A, B }\nfn main() { let e = E::A; match e { E::A => println(\"a\"), E::B => println(\"b\") } }",
                "A",
                0,
                "B",
            ),
        ];
        for (what, src, old, nth, new) in cases {
            let r = try_rename(src, old, *nth, new);
            assert!(
                r.is_err(),
                "{what}: renaming `{old}` to `{new}` must be refused, got:\n{}",
                r.unwrap_or_default()
            );
        }
    });
}

#[test]
fn harmless_reuse_of_a_name_in_a_disjoint_scope_is_allowed() {
    big_stack(|| {
        let src = "fn f(n: int) -> int { n + 1 }\nfn main() { let m = 3; println(to_str(f(m))); }";
        let out = try_rename(src, "m", 0, "n").expect("`n` is a parameter of another function");
        assert_eq!(behaviour(&out, None), behaviour(src, None));
        assert!(out.contains("let n = 3; println(to_str(f(n)));"), "{out}");
    });
}

#[test]
fn keywords_and_invalid_identifiers_are_rejected_by_prepare_rename_and_rename() {
    big_stack(|| {
        let src = "fn main() { let a = 1; println(to_str(a)); }";
        for bad in [
            "fn", "let", "match", "true", "_", "int", "", "1a", "a-b", "a b", "a::b",
        ] {
            assert!(
                try_rename(src, "a", 0, bad).is_err(),
                "{bad:?} must be rejected"
            );
        }
        // On a keyword or a built-in there is nothing to rename at all.
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, src);
        for needle in ["let", "println"] {
            let (line, character) = lsp_position(src, src.find(needle).unwrap());
            let r = c.request("textDocument/prepareRename", json!({"textDocument": {"uri": URI}, "position": {"line": line, "character": character}}));
            assert_eq!(r["error"]["code"], -32602, "{needle}: {r}");
        }
    });
}
