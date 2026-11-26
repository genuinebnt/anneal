//! Language-server behaviour and properties, driven in-process.

mod common;
use common::client::*;
use common::*;
use pebble_tools::analysis::source::LineIndex;
use pebble_tools::lsp::{Server, SyncKind, serve, transport};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const URI: &str = "file:///t.peb";

fn run_output(text: &str) -> Option<String> {
    let an = pebble_tools::analysis::analyze(text);
    if an.has_errors() {
        return None;
    }
    let mut out = Vec::new();
    let res = pebble_tools::analysis::interp::run(&an.hir, &mut out);
    Some(format!(
        "{}{}",
        String::from_utf8_lossy(&out),
        res.err().map(|e| e.to_string()).unwrap_or_default()
    ))
}

fn pos_of(text: &str, offset: usize) -> Value {
    let lc = LineIndex::new(text).line_col_utf16(text, offset);
    json!({"line": lc.line, "character": lc.col})
}

fn offset_of(text: &str, pos: &Value) -> usize {
    LineIndex::new(text)
        .offset_from_utf16(
            text,
            pos["line"].as_u64().unwrap() as usize,
            pos["character"].as_u64().unwrap() as usize,
        )
        .unwrap_or(text.len())
}

fn apply_text_edits(text: &str, edits: &Value) -> String {
    let mut es: Vec<(usize, usize, String)> = edits
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                offset_of(text, &e["range"]["start"]),
                offset_of(text, &e["range"]["end"]),
                e["newText"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    es.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_string();
    for (a, b, t) in es {
        out.replace_range(a..b, &t);
    }
    out
}

// ---------------------------------------------------------------- lifecycle

#[test]
fn lifecycle_codes_and_errors() {
    big_stack(|| {
        let mut c = Client::new(SyncKind::Incremental);
        let r = c.request("textDocument/hover", json!({}));
        assert_eq!(
            r["error"]["code"], -32002,
            "requests before initialize are rejected"
        );
        let r = c.request("initialize", json!({}));
        assert!(
            r["result"]["capabilities"]["hoverProvider"]
                .as_bool()
                .unwrap()
        );
        assert_eq!(c.request("initialize", json!({}))["error"]["code"], -32600);
        assert_eq!(
            c.request("nonsense/method", json!({}))["error"]["code"],
            -32601
        );
        let out = c.server.handle_bytes(b"{ not json");
        assert_eq!(out[0]["error"]["code"], -32700);
        assert_eq!(out[0]["id"], Value::Null);
        let out = c.server.handle(json!([1, 2]));
        assert_eq!(out[0]["error"]["code"], -32600);
        assert_eq!(c.request("shutdown", Value::Null)["result"], Value::Null);
        assert_eq!(
            c.request("textDocument/hover", json!({}))["error"]["code"],
            -32600
        );
        assert_eq!(c.server.exit_code(), None);
        c.notify("exit", Value::Null);
        assert_eq!(c.server.exit_code(), Some(0));
        let mut c = Client::started(SyncKind::Full);
        c.notify("exit", Value::Null);
        assert_eq!(c.server.exit_code(), Some(1), "exit without shutdown");
    });
}

#[test]
fn cancellation_in_the_same_batch() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, "fn main() {}\n");
        let hover = |id: i64| json!({"jsonrpc": "2.0", "id": id, "method": "textDocument/hover", "params": c.at(URI, 0, 4)});
        let out = c.server.process(vec![
            hover(50),
            json!({"jsonrpc": "2.0", "method": "$/cancelRequest", "params": {"id": 50}}),
            hover(51),
        ]);
        assert_eq!(out[0]["error"]["code"], -32800);
        assert!(out[1].get("result").is_some(), "other requests still run");
    });
}

#[test]
fn rapid_edits_coalesce_into_one_analysis_latest_version_wins() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, "fn main() {}\n");
        let before = c.server.analyses;
        let msgs: Vec<Value> = (2..=101)
            .map(|v| {
                json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {
                    "textDocument": {"uri": URI, "version": v},
                    "contentChanges": [{"range": {"start": {"line": 0, "character": 11}, "end": {"line": 0, "character": 11}}, "text": " "}]}})
            })
            .collect();
        let out = c.server.process(msgs);
        assert_eq!(
            c.server.analyses - before,
            1,
            "one analysis for the whole burst"
        );
        let published: Vec<&Value> = out
            .iter()
            .filter(|m| m["method"] == "textDocument/publishDiagnostics")
            .collect();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0]["params"]["version"], 101);
        assert_eq!(
            c.server.document_text(URI).unwrap().len(),
            "fn main() {}\n".len() + 100
        );
        // A stale version that overtakes is ignored.
        c.notify("textDocument/didChange", json!({"textDocument": {"uri": URI, "version": 3}, "contentChanges": [{"text": "garbage"}]}));
        assert_ne!(c.server.document_text(URI).unwrap(), "garbage");
    });
}

#[test]
fn serve_over_byte_buffers() {
    big_stack(|| {
        let msgs = [
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
            json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": URI, "version": 1, "text": "fn main() { let x: int = \"s\"; }"}}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}),
            json!({"jsonrpc": "2.0", "method": "exit"}),
        ];
        let mut input = Vec::new();
        input.extend(transport::frame(&msgs[0]));
        input.extend(b"Content-Length: 5\r\n\r\n{bad}"); // malformed JSON frame
        for m in &msgs[1..] {
            input.extend(transport::frame(m));
        }
        let mut server = Server::new(SyncKind::Incremental);
        let mut out = Vec::new();
        let code = serve(
            &mut server,
            std::io::Cursor::new(input),
            &mut out,
            Duration::ZERO,
        );
        assert_eq!(code, 0);
        let mut r = std::io::Cursor::new(out);
        let mut got = Vec::new();
        while let Some(b) = transport::read_message(&mut r).unwrap() {
            got.push(serde_json::from_slice::<Value>(&b).unwrap());
        }
        assert!(got.iter().any(|m| m["error"]["code"] == -32700));
        assert!(got.iter().any(|m| {
            m["method"] == "textDocument/publishDiagnostics"
                && m["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|d| !d.is_empty())
        }));
        assert!(got.iter().any(|m| m["id"] == 2 && m["result"].is_null()));
        // End of input without `exit` is an unclean shutdown.
        let mut server = Server::new(SyncKind::Full);
        let code = serve(
            &mut server,
            std::io::Cursor::new(transport::frame(&msgs[0])),
            &mut Vec::new(),
            Duration::ZERO,
        );
        assert_eq!(code, 1);
    });
}

// ---------------------------------------------------------------- document sync

fn random_edit(rng: &mut Rng, text: &str) -> (usize, usize, usize, usize, String) {
    let lines: Vec<&str> = text.split('\n').collect();
    let pick = |rng: &mut Rng| {
        let l = rng.below(lines.len());
        let cs: Vec<char> = lines[l].chars().collect();
        let ci = rng.below(cs.len() + 1);
        (l, cs[..ci].iter().map(|c| c.len_utf16()).sum::<usize>())
    };
    let (a, b) = (pick(rng), pick(rng));
    let (s, e) = if a <= b { (a, b) } else { (b, a) };
    // Keep ranges short so the document does not collapse to nothing.
    let e = if e.0 > s.0 + 2 { (s.0 + 2, 0) } else { e };
    let pool = [
        "",
        "",
        "x",
        "😀",
        "\n",
        "fn ",
        "}",
        " é ",
        "let a = 1;",
        "\r\n",
        "(",
        "\"",
        "/*",
    ];
    (s.0, s.1, e.0, e.1, rng.pick(&pool).to_string())
}

#[test]
fn incremental_and_full_sync_agree_over_500_random_edits() {
    big_stack(|| {
        let start = std::fs::read_to_string(compiler_dir().join("examples/showcase.peb")).unwrap();
        let mut inc = Client::started(SyncKind::Incremental);
        let mut full = Client::started(SyncKind::Full);
        inc.open(URI, &start);
        full.open(URI, &start);
        let mut model = start;
        let mut rng = Rng::new(99);
        for v in 2..502 {
            let (sl, sc, el, ec, new) = random_edit(&mut rng, &model);
            model = apply_edit_utf16(&model, sl, sc, el, ec, &new);
            let range = json!({"start": {"line": sl, "character": sc}, "end": {"line": el, "character": ec}});
            inc.notify("textDocument/didChange", json!({"textDocument": {"uri": URI, "version": v}, "contentChanges": [{"range": range, "text": new}]}));
            full.notify("textDocument/didChange", json!({"textDocument": {"uri": URI, "version": v}, "contentChanges": [{"text": model}]}));
            assert_eq!(
                inc.server.document_text(URI).unwrap(),
                model,
                "incremental text diverged at edit {v}"
            );
            assert_eq!(full.server.document_text(URI).unwrap(), model);
            assert_eq!(
                inc.last_diagnostics[URI], full.last_diagnostics[URI],
                "diagnostics differ at edit {v}"
            );
        }
        assert_eq!(inc.server.document_version(URI), Some(501));
    });
}

#[test]
fn diagnostics_use_utf16_columns_and_related_information() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        let src = "fn main() {\n    let s = \"😀\"; let n: int = \"oops\";\n    println(s);\n}\n";
        let d = c.open(URI, src);
        let err = d
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["severity"] == 1)
            .expect("an error");
        // 4 spaces + `let s = ` + `"😀"` (4 UTF-16 units) + `; ` + `let n: int = ` -> column 31; in bytes it would be 33.
        assert_eq!(err["range"]["start"]["character"], 31, "{err}");
        assert_eq!(err["code"], "E0301");
        assert!(
            err["relatedInformation"]
                .as_array()
                .is_some_and(|r| !r.is_empty()),
            "secondary labels become related information"
        );
        let unused = d
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == "W0501")
            .unwrap();
        assert_eq!(unused["tags"], json!([1]));
    });
}

#[test]
fn handles_broken_documents() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        let src = "struct P { x: int }\nfn main() {\n    let p = P { x: 1 };\n    p.\n    let y = (1 +;\n}\nfn broken( {\n";
        let d = c.open(URI, src);
        assert!(!d.as_array().unwrap().is_empty());
        let r = c.result("textDocument/completion", c.at(URI, 3, 6));
        let labels: Vec<&str> = r["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["label"].as_str().unwrap())
            .collect();
        assert!(
            labels.contains(&"x"),
            "field completion after `p.` despite syntax errors: {labels:?}"
        );
        for m in [
            "textDocument/hover",
            "textDocument/definition",
            "textDocument/documentSymbol",
            "textDocument/foldingRange",
            "textDocument/semanticTokens/full",
            "textDocument/inlayHint",
        ] {
            let mut p = c.at(URI, 2, 8);
            p["range"] =
                json!({"start": {"line": 0, "character": 0}, "end": {"line": 9, "character": 0}});
            assert!(c.request(m, p).get("error").is_none(), "{m}");
        }
        // Formatting a file that does not parse is best effort now (see tests/fmt_range.rs): an array, never an error.
        assert!(
            c.result(
                "textDocument/formatting",
                json!({"textDocument": {"uri": URI}, "options": {}})
            )
            .is_array()
        );
    });
}

// ---------------------------------------------------------------- features

#[test]
fn completion_after_double_colon_dot_and_in_scope() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        let src = "enum Shape { Circle(int), Square(int) }\nstruct P { x: int, name: str }\nfn main() {\n    let s = Shape::Circle(1);\n    let p = P { x: 1, name: \"a\" };\n    let q = Shape::;\n    p.\n    let xs = [1];\n    xs.\n}\n";
        c.open(URI, src);
        let labels = |c: &mut Client, l: usize, ch: usize| -> Vec<String> {
            let r = c.result("textDocument/completion", c.at(URI, l, ch));
            r["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["label"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(labels(&mut c, 5, 19), ["Circle", "Square"]);
        let m = labels(&mut c, 6, 6);
        assert!(
            m.contains(&"name".to_string()) && m.contains(&"x".to_string()),
            "{m:?}"
        );
        let m = labels(&mut c, 8, 7);
        assert!(
            m.contains(&"push".to_string()) && m.contains(&"len".to_string()),
            "{m:?}"
        );
        let g = labels(&mut c, 7, 4);
        assert!(
            g.contains(&"p".to_string())
                && g.contains(&"fn".to_string())
                && g.contains(&"println".to_string()),
            "{g:?}"
        );
        let r = c.result(
            "completionItem/resolve",
            json!({"label": "println", "data": {"uri": URI, "label": "println"}}),
        );
        assert!(
            r["documentation"]["value"]
                .as_str()
                .unwrap()
                .contains("newline")
        );
    });
}

#[test]
fn completion_at_random_offsets_only_returns_visible_names() {
    big_stack(|| {
        let mut checked = 0;
        for (path, src) in corpus()
            .into_iter()
            .filter(|(p, _)| p.to_string_lossy().contains("examples"))
            .take(8)
        {
            let mut c = Client::started(SyncKind::Incremental);
            c.open(URI, &src);
            let an = pebble_tools::analysis::analyze(&src);
            let mut allowed: std::collections::HashSet<String> =
                an.index.defs.iter().map(|d| d.name.clone()).collect();
            allowed.extend(
                [
                    "print", "println", "assert", "panic", "to_str", "len", "push", "pop", "chars",
                ]
                .map(String::from),
            );
            allowed.extend(
                [
                    "fn", "struct", "enum", "const", "let", "mut", "if", "else", "match", "while",
                    "loop", "break", "return", "continue",
                ]
                .map(String::from),
            );
            let mut rng = Rng::new(src.len() as u64);
            for _ in 0..100 {
                let mut off = rng.below(src.len() + 1);
                while !src.is_char_boundary(off) {
                    off -= 1;
                }
                let r = c.result(
                    "textDocument/completion",
                    json!({"textDocument": {"uri": URI}, "position": pos_of(&src, off)}),
                );
                for it in r["items"].as_array().unwrap() {
                    let label = it["label"].as_str().unwrap();
                    let base = label.rsplit("::").next().unwrap();
                    assert!(
                        allowed.contains(base) || base.chars().all(|c| c.is_ascii_digit()),
                        "{}: unknown completion `{label}` at {off}",
                        path.display()
                    );
                    if it["kind"] == 6 {
                        assert!(
                            an.index.defs.iter().any(|d| d.name == label
                                && matches!(
                                    d.kind,
                                    pebble_tools::analysis::sema::DefKind::Local
                                        | pebble_tools::analysis::sema::DefKind::Param
                                )
                                && (d.span.start as usize) <= off),
                            "{}: `{label}` is not a local declared before offset {off}",
                            path.display()
                        );
                    }
                    checked += 1;
                }
            }
        }
        assert!(checked > 1000);
    });
}

#[test]
fn hover_types_match_dump_hir() {
    big_stack(|| {
        let mut compared = 0;
        for path in peb_files("tests/run").into_iter().take(30) {
            let src = std::fs::read_to_string(&path).unwrap();
            let an = pebble_tools::analysis::analyze(&src);
            if an.has_errors() {
                continue;
            }
            // Pull (start, end, primitive type) triples out of the HIR's Debug dump.
            let dump: String = an.hir.fns.iter().map(|f| format!("{f:#?}")).collect();
            let lines: Vec<&str> = dump.lines().map(str::trim).collect();
            let mut nodes: Vec<(usize, usize, &str)> = Vec::new();
            for i in 0..lines.len().saturating_sub(6) {
                if lines[i] == "span: Span {"
                    && let (Some(a), Some(b)) = (
                        lines[i + 1].strip_prefix("start: "),
                        lines[i + 2].strip_prefix("end: "),
                    )
                    && let Some(ty) = lines[i + 4].strip_prefix("ty: ")
                {
                    let ty = match ty.trim_end_matches(',') {
                        "Int" => "int",
                        "Bool" => "bool",
                        "Str" => "str",
                        _ => continue,
                    };
                    nodes.push((
                        a.trim_end_matches(',').parse().unwrap(),
                        b.trim_end_matches(',').parse().unwrap(),
                        ty,
                    ));
                }
            }
            let mut c = Client::started(SyncKind::Incremental);
            c.open(URI, &src);
            for (start, end, ty) in nodes {
                let Some(n) = an.index.node_at(start) else {
                    continue;
                };
                if n.span.start as usize != start || n.span.end as usize != end {
                    continue; // another node starts here (e.g. a parenthesis the HIR drops)
                }
                let r = c.result(
                    "textDocument/hover",
                    json!({"textDocument": {"uri": URI}, "position": pos_of(&src, start)}),
                );
                let text = r["contents"]["value"].as_str().unwrap_or("");
                let body = text
                    .trim_start_matches("```pebble\n")
                    .split("\n```")
                    .next()
                    .unwrap_or("");
                assert!(
                    body == ty || body.ends_with(&format!(": {ty}")) || body.starts_with("fn "),
                    "{}: hover at {start} says `{body}`, HIR says `{ty}`",
                    path.display()
                );
                compared += 1;
            }
        }
        assert!(compared > 200, "only {compared} hovers compared");
    });
}

#[test]
fn rename_updates_every_reference_and_preserves_behaviour() {
    big_stack(|| {
        let (mut renamed, mut skipped) = (0, 0);
        for path in peb_files("tests/run") {
            let src = std::fs::read_to_string(&path).unwrap();
            let Some(before) = run_output(&src) else {
                continue;
            };
            let an = pebble_tools::analysis::analyze(&src);
            let mut c = Client::started(SyncKind::Incremental);
            c.open(URI, &src);
            let targets: Vec<_> = an
                .index
                .defs
                .iter()
                .filter(|d| !d.name.is_empty() && d.name != "main" && !d.span.is_empty())
                .collect();
            let step = (targets.len() / 8).max(1);
            for d in targets.iter().step_by(step) {
                let at = pos_of(&src, d.span.start as usize);
                let r = c.request(
                    "textDocument/rename",
                    json!({"textDocument": {"uri": URI}, "position": at, "newName": "renamed_zz"}),
                );
                if r.get("error").is_some() {
                    skipped += 1;
                    continue;
                }
                let edits = &r["result"]["changes"][URI];
                let new_src = apply_text_edits(&src, edits);
                let an2 = pebble_tools::analysis::analyze(&new_src);
                assert!(
                    !an2.has_errors(),
                    "{}: renaming `{}` ({:?}) broke the program:\n{}\n{}",
                    path.display(),
                    d.name,
                    d.kind,
                    edits,
                    pebble_tools::analysis::diag::render_all(
                        &an2.diagnostics,
                        &pebble_tools::analysis::source::SourceFile::new("x", new_src.clone()),
                        &Default::default()
                    )
                );
                // Stack traces print function names, so map the new name back before comparing.
                let after = run_output(&new_src).map(|o| o.replace("renamed_zz", &d.name));
                assert_eq!(
                    after.as_deref(),
                    Some(before.as_str()),
                    "{}: renaming `{}` changed behaviour",
                    path.display(),
                    d.name
                );
                renamed += 1;
            }
        }
        assert!(
            renamed > 150,
            "only {renamed} renames exercised ({skipped} skipped)"
        );
    });
}

#[test]
fn formatting_edit_applies_to_the_formatter_output() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        let src = "fn main(){let x=1;   println(to_str(x));}\n";
        c.open(URI, src);
        let edits = c.result(
            "textDocument/formatting",
            json!({"textDocument": {"uri": URI}, "options": {}}),
        );
        assert_eq!(
            apply_text_edits(src, &edits),
            pebble_tools::fmt::format(src).unwrap()
        );
        let r = c.result("textDocument/rangeFormatting", json!({"textDocument": {"uri": URI}, "options": {}, "range": {"start": {"line": 5, "character": 0}, "end": {"line": 6, "character": 0}}}));
        assert_eq!(
            r,
            json!([]),
            "range outside the changed region yields no edit"
        );
    });
}

#[test]
fn quick_fix_for_did_you_mean() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        let src = "struct P { count: int }\nfn main() { let p = P { count: 1 }; println(to_str(p.cuont)); }\n";
        c.open(URI, src);
        let at = src.find("cuont").unwrap();
        let actions = c.result("textDocument/codeAction", json!({"textDocument": {"uri": URI}, "range": {"start": pos_of(src, at), "end": pos_of(src, at)}, "context": {"diagnostics": []}}));
        let a = &actions.as_array().unwrap()[0];
        assert_eq!(a["kind"], "quickfix");
        assert_eq!(
            apply_text_edits(src, &a["edit"]["changes"][URI]),
            src.replace("cuont", "count")
        );
    });
}

// ---------------------------------------------------------------- robustness

fn random_json(rng: &mut Rng, depth: usize) -> Value {
    match rng.below(if depth > 3 { 6 } else { 9 }) {
        0 => Value::Null,
        1 => json!(rng.below(200) as i64 - 20),
        2 => json!(rng.next()),
        3 => json!(["", "x", "😀é", "file:///t.peb", "fn", "\u{0}", "::"][rng.below(7)]),
        4 => json!(rng.chance(50)),
        5 => json!(-1.5e300),
        6 => Value::Array(
            (0..rng.below(3))
                .map(|_| random_json(rng, depth + 1))
                .collect(),
        ),
        7 => json!({"line": rng.below(30), "character": rng.below(80)}),
        _ => {
            let keys = [
                "textDocument",
                "uri",
                "position",
                "range",
                "start",
                "end",
                "line",
                "character",
                "text",
                "version",
                "contentChanges",
                "newName",
                "context",
                "query",
                "id",
                "label",
                "data",
            ];
            Value::Object(
                (0..rng.below(5))
                    .map(|_| {
                        (
                            keys[rng.below(keys.len())].to_string(),
                            random_json(rng, depth + 1),
                        )
                    })
                    .collect(),
            )
        }
    }
}

#[test]
fn fuzz_random_messages_and_edits_never_panic() {
    big_stack(|| {
        let methods = [
            "initialize",
            "shutdown",
            "textDocument/didOpen",
            "textDocument/didChange",
            "textDocument/didClose",
            "textDocument/hover",
            "textDocument/definition",
            "textDocument/references",
            "textDocument/rename",
            "textDocument/prepareRename",
            "textDocument/documentSymbol",
            "textDocument/completion",
            "completionItem/resolve",
            "textDocument/signatureHelp",
            "textDocument/semanticTokens/full",
            "textDocument/formatting",
            "textDocument/rangeFormatting",
            "textDocument/codeAction",
            "textDocument/inlayHint",
            "textDocument/foldingRange",
            "workspace/didChangeConfiguration",
            "$/cancelRequest",
            "bogus",
        ];
        let corpus = corpus();
        let mut rng = Rng::new(2024);
        let mut c = Client::started(SyncKind::Incremental);
        let mut text = corpus[0].1.clone();
        c.open(URI, &text);
        for i in 0..5000 {
            let method = *rng.pick(&methods);
            if method == "shutdown" || method == "initialize" {
                continue; // keep the session alive; lifecycle has its own tests
            }
            let msg = match rng.below(4) {
                // well-formed params with random garbage values
                0 => {
                    json!({"jsonrpc": "2.0", "id": i, "method": method, "params": random_json(&mut rng, 0)})
                }
                // plausible params at random positions (often past the end)
                1 => {
                    json!({"jsonrpc": "2.0", "id": i, "method": method, "params": {"textDocument": {"uri": URI}, "position": {"line": rng.below(60), "character": rng.below(120)},
                    "range": {"start": {"line": rng.below(40), "character": rng.below(80)}, "end": {"line": rng.below(60), "character": rng.below(80)}}, "newName": *rng.pick(&["zz", "", "fn", "a b", "😀"]), "options": {}, "context": {"diagnostics": []}}})
                }
                // random edits applied to the document
                2 => {
                    let (sl, sc, el, ec, new) = random_edit(&mut rng, &text);
                    text = apply_edit_utf16(&text, sl, sc, el, ec, &new);
                    json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {"textDocument": {"uri": URI, "version": i + 10}, "contentChanges": [{"range": {"start": {"line": sl, "character": sc}, "end": {"line": el, "character": ec}}, "text": new}]}})
                }
                // arbitrary shapes
                _ => random_json(&mut rng, 0),
            };
            let out = c.server.handle(msg.clone());
            if let Some(id) = msg.get("id").filter(|_| msg.get("method").is_some()) {
                let resp = out.iter().find(|m| m.get("id") == Some(id));
                assert!(
                    resp.is_some_and(|r| r.get("result").is_some() != r.get("error").is_some()),
                    "bad response for {msg}: {out:?}"
                );
            }
            assert_eq!(c.server.internal_errors, 0, "handler panicked on {msg}");
            if c.server.exit_code().is_some() {
                break;
            }
        }
    });
}

#[test]
fn five_thousand_line_file_answers_within_budget() {
    big_stack(|| {
        let mut src = String::from("struct Acc { total: int, count: int }\n\n");
        for i in 0..230 {
            src.push_str(&format!(
                "/// Adds up to `n` using accumulator {i}.\nfn work_{i}(n: int) -> int {{\n    let mut acc = Acc {{ total: 0, count: 0 }};\n    let mut i = 0;\n    while i < n {{\n        if i % 2 == 0 {{\n            acc.total += i * {i};\n        }} else {{\n            acc.total -= 1;\n        }}\n        acc.count += 1;\n        i += 1;\n    }}\n    let label = \"run {i} 😀\";\n    match acc.count {{\n        0 => 0,\n        1 => acc.total,\n        _ => acc.total + len(label),\n    }}\n}}\n\n"
            ));
        }
        src.push_str("fn main() {\n    let mut s = 0;\n");
        for i in 0..230 {
            src.push_str(&format!("    s += work_{i}(3);\n"));
        }
        src.push_str("    println(to_str(s));\n}\n");
        let lines = src.lines().count();
        assert!(lines >= 5000, "{lines} lines");
        let mut c = Client::started(SyncKind::Incremental);
        let t = Instant::now();
        let d = c.open(URI, &src);
        let open_time = t.elapsed();
        assert!(
            d.as_array()
                .is_some_and(|d| d.iter().all(|x| x["severity"] != 1)),
            "{d}"
        );
        println!("5000-line file ({lines} lines): didOpen + analysis + diagnostics {open_time:?}");
        let mid = c.at(URI, lines / 2 + 3, 10);
        let mut budget = vec![
            ("hover", "textDocument/hover", mid.clone()),
            ("definition", "textDocument/definition", mid.clone()),
            ("completion", "textDocument/completion", mid.clone()),
            ("references", "textDocument/references", {
                let mut p = c.at(URI, 3, 4);
                p["context"] = json!({"includeDeclaration": true});
                p
            }),
            (
                "documentSymbol",
                "textDocument/documentSymbol",
                c.at(URI, 0, 0),
            ),
            (
                "semanticTokens",
                "textDocument/semanticTokens/full",
                c.at(URI, 0, 0),
            ),
            ("foldingRange", "textDocument/foldingRange", c.at(URI, 0, 0)),
            ("inlayHint", "textDocument/inlayHint", c.at(URI, 0, 0)),
            (
                "formatting",
                "textDocument/formatting",
                json!({"textDocument": {"uri": URI}, "options": {}}),
            ),
        ];
        for (name, method, params) in budget.drain(..) {
            let t = Instant::now();
            let r = c.request(method, params);
            let dt = t.elapsed();
            println!("  {name:<15} {dt:?}");
            assert!(r.get("error").is_none(), "{name}: {r}");
            assert!(dt < Duration::from_secs(3), "{name} took {dt:?}");
        }
        assert!(
            open_time < Duration::from_secs(10),
            "analysis took {open_time:?}"
        );
        // An edit re-analyses once and diagnostics stay fast.
        let t = Instant::now();
        c.notify("textDocument/didChange", json!({"textDocument": {"uri": URI, "version": 2}, "contentChanges": [{"range": {"start": {"line": 3, "character": 0}, "end": {"line": 3, "character": 0}}, "text": "    // edit\n"}]}));
        println!("  incremental edit + diagnostics {:?}", t.elapsed());
    });
}
