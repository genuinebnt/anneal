//! Range formatting, format-on-type, best-effort formatting of files with syntax errors,
//! and line endings (`\n`, `\r\n`, lone `\r`) including UTF-16 positions in LSP edits.

mod common;
use common::client::*;
use common::*;
use pebble_tools::analysis::syntax::ast::ItemKind;
use pebble_tools::analysis::syntax::parse;
use pebble_tools::fmt::{self, Replacement};
use pebble_tools::lsp::SyncKind;
use serde_json::json;

const URI: &str = "file:///f.peb";

fn item_spans(text: &str) -> Vec<(usize, usize)> {
    parse(text)
        .ast
        .items
        .iter()
        .filter(|i| !matches!(i.kind, ItemKind::Error))
        .map(|i| (i.span.start as usize, i.span.end as usize))
        .collect()
}

fn error_spans(text: &str) -> Vec<(usize, usize)> {
    parse(text)
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .flat_map(|d| {
            d.labels
                .iter()
                .map(|l| (l.span.start as usize, l.span.end as usize))
        })
        .collect()
}

fn clean(text: &str) -> bool {
    !parse(text).diagnostics.iter().any(|d| d.is_error())
}

fn run_output(text: &str) -> Option<(String, bool)> {
    let an = pebble_tools::analysis::analyze(text);
    if an.has_errors() {
        return None;
    }
    let (out, err) = pebble_tools::analysis::run(&an);
    Some((out, err.is_some()))
}

/// A program with deliberately ugly layout so formatting has something to do.
fn ugly(seed: u64) -> String {
    Gen::program(seed)
}

// ---------------------------------------------------------------- range formatting

#[test]
fn range_formatting_changes_only_the_covered_items() {
    let (mut changed, mut checked) = (0, 0);
    for seed in 0..400u64 {
        let text = ugly(seed);
        if !clean(&text) {
            continue;
        }
        let mut rng = Rng::new(seed + 1000);
        for _ in 0..4 {
            let a = rng.below(text.len() + 1);
            let b = (a + if rng.chance(30) {
                0
            } else {
                rng.below(text.len() / 2 + 1)
            })
            .min(text.len());
            let (a, b) = (floor(&text, a), floor(&text, b));
            let reps = fmt::format_range(&text, a, b);
            let items = item_spans(&text);
            // every replacement lies inside an item that overlaps the range
            for r in &reps {
                let hit = items.iter().any(|&(s, e)| {
                    s <= r.start
                        && r.end <= e
                        && if a >= b {
                            s <= a && a <= e
                        } else {
                            s < b && a < e
                        }
                });
                assert!(
                    hit,
                    "seed {seed}: replacement {r:?} is outside the items covered by {a}..{b}"
                );
            }
            let out = fmt::apply_replacements(&text, &reps);
            if let (Some(lo), Some(hi)) = (
                reps.iter().map(|r| r.start).min(),
                reps.iter().map(|r| r.end).max(),
            ) {
                // BYTE-FOR-BYTE outside the covered node range
                assert_eq!(&out[..lo], &text[..lo], "seed {seed}: prefix changed");
                assert!(out.ends_with(&text[hi..]), "seed {seed}: suffix changed");
                changed += 1;
                // idempotent: formatting the same covered region again changes nothing
                let delta = out.len() as isize - text.len() as isize;
                let again = fmt::format_range(&out, lo, (hi as isize + delta) as usize);
                assert_eq!(
                    again,
                    vec![],
                    "seed {seed}: second pass must be a no-op\n{out}"
                );
            } else {
                assert_eq!(out, text);
            }
            // semantics preserved: same AST modulo layout, same comments
            assert_eq!(
                fmt::ast_fingerprint(&out),
                fmt::ast_fingerprint(&text),
                "seed {seed}"
            );
            assert_eq!(
                parse(&out).comments.len(),
                parse(&text).comments.len(),
                "seed {seed}: comments"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 1000 && changed > 300,
        "weak coverage: checked {checked}, changed {changed}"
    );
}

fn floor(t: &str, mut i: usize) -> usize {
    while !t.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[test]
fn range_formatting_preserves_behaviour_over_the_run_corpus() {
    big_stack(|| {
        let mut n = 0;
        for path in peb_files("tests/run") {
            let text = std::fs::read_to_string(&path).unwrap();
            let Some(before) = run_output(&text) else {
                continue;
            };
            // scramble the layout of one item so there is work to do, then range-format around it
            let mut rng = Rng::new(n as u64 + 5);
            let mut messy = text.clone();
            if let Some(&(s, e)) = item_spans(&text).get(rng.below(item_spans(&text).len().max(1)))
            {
                let body = &text[s..e];
                if !body.contains("//") && !body.contains("/*") {
                    let squashed: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
                    messy = format!("{}{}{}", &text[..s], squashed, &text[e..]);
                }
            }
            if !clean(&messy) {
                continue;
            }
            let a = floor(&messy, rng.below(messy.len() + 1));
            let b = floor(&messy, (a + rng.below(200)).min(messy.len()));
            let out = fmt::apply_replacements(&messy, &fmt::format_range(&messy, a, b));
            assert_eq!(
                run_output(&out),
                Some(before),
                "{}: range formatting changed behaviour",
                path.display()
            );
            n += 1;
        }
        assert!(n > 30, "{n} programs exercised");
    });
}

#[test]
fn range_formatting_example_formats_the_enclosing_item_only() {
    let src = "fn  first( ){let a=1;}\n\n// keep   this   spacing\nfn  second( ){let b=2;}\nfn   third( ){let c=3;}\n";
    let a = src.find("second").unwrap();
    let reps = fmt::format_range(src, a, a + 2);
    let out = fmt::apply_replacements(src, &reps);
    assert!(
        out.starts_with("fn  first( ){let a=1;}\n\n// keep   this   spacing\n"),
        "{out}"
    );
    assert!(out.ends_with("fn   third( ){let c=3;}\n"), "{out}");
    assert!(out.contains("fn second() {\n    let b = 2;\n}\n"), "{out}");
    // a range spanning two items formats both, and the text between them stays
    let reps = fmt::format_range(src, 0, src.find("second").unwrap() + 3);
    let out = fmt::apply_replacements(src, &reps);
    assert!(
        out.contains("fn first() {\n    let a = 1;\n}\n\n// keep   this   spacing\nfn second() {"),
        "{out}"
    );
    assert!(out.ends_with("fn   third( ){let c=3;}\n"));
    // a range between items (in a comment) selects nothing
    let c = src.find("this").unwrap();
    assert_eq!(fmt::format_range(src, c, c + 4), vec![]);
}

// ---------------------------------------------------------------- best effort

#[test]
fn best_effort_formats_clean_items_and_keeps_error_regions_verbatim() {
    let src = "fn  one( ){let a=1;}\nfn broken( { let = ; \nfn  two( ){let b=2;}\n";
    assert!(fmt::format(src).is_err(), "strict mode refuses");
    let be = fmt::format_best_effort(src);
    assert!(
        be.text.starts_with("fn one() {\n    let a = 1;\n}\n"),
        "{}",
        be.text
    );
    assert!(
        be.text.contains("fn broken( { let = ; "),
        "the error region is verbatim: {}",
        be.text
    );
    assert!(be.reformatted >= 1);
    // The CLI-visible rule also holds for a file with an unterminated string and stray tokens.
    let src =
        "fn  ok( ){ 1 }\n@@@ stray tokens ###\nconst K : int=\"unterminated;\nfn  ok2( ){ 2 }\n";
    let be = fmt::format_best_effort(src);
    assert!(be.text.contains("@@@ stray tokens ###"), "{}", be.text);
    assert!(
        be.text.contains("const K : int=\"unterminated;"),
        "{}",
        be.text
    );
}

#[test]
fn best_effort_invariants_on_mutated_programs() {
    let mut rng = Rng::new(77);
    let (mut broken, mut partly) = (0, 0);
    for seed in 0..1500u64 {
        let mut text = ugly(seed);
        // corrupt: delete or insert a few characters
        for _ in 0..1 + rng.below(3) {
            let at = floor(&text, rng.below(text.len() + 1));
            match rng.below(3) {
                0 if at < text.len() => {
                    let end = floor(&text, (at + 1 + rng.below(3)).min(text.len()));
                    text.replace_range(at..end, "");
                }
                1 => text.insert_str(
                    at,
                    *rng.pick(&["{", "}", "(", ")", "@", "\"", "let", ";;", "fn", "=>"]),
                ),
                _ => {}
            }
        }
        let errors = error_spans(&text);
        let is_broken = !errors.is_empty();
        let reps: Vec<Replacement> = fmt::format_items(&text, None);
        let items = item_spans(&text);
        for r in &reps {
            assert!(
                items.iter().any(|&(s, e)| s <= r.start && r.end <= e),
                "seed {seed}: replacement outside every item"
            );
            // error regions are never touched
            assert!(
                !errors.iter().any(|&(es, ee)| es <= r.end && r.start <= ee),
                "seed {seed}: a replacement touches an error region: {r:?} vs {errors:?}\n{text}"
            );
        }
        let be = fmt::format_best_effort(&text);
        assert_eq!(be.text, fmt::apply_replacements(&text, &reps));
        // idempotent
        assert_eq!(
            fmt::format_best_effort(&be.text).text,
            be.text,
            "seed {seed}: best effort must be idempotent"
        );
        // every error region's bytes survive verbatim
        for &(es, ee) in &errors {
            let region = &text[floor(&text, es)..floor(&text, ee)];
            assert!(
                be.text.contains(region),
                "seed {seed}: error region {region:?} was altered"
            );
        }
        if is_broken {
            broken += 1;
            if be.reformatted > 0 {
                partly += 1;
            }
        } else {
            // clean files: formatting every item never changes the program
            assert_eq!(
                fmt::ast_fingerprint(&be.text),
                fmt::ast_fingerprint(&text),
                "seed {seed}"
            );
        }
    }
    assert!(
        broken > 400 && partly > 100,
        "weak coverage: broken {broken}, partly formatted {partly}"
    );
}

// ---------------------------------------------------------------- LSP integration

#[test]
fn lsp_formats_broken_files_best_effort_and_strict_mode_stays_strict() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        let src = "fn  one( ){let a=1;}\nfn broken( { let = ; \nfn  two( ){let b=2;}\n";
        c.open(URI, src);
        let r = c.request(
            "textDocument/formatting",
            json!({"textDocument": {"uri": URI}, "options": {}}),
        );
        assert!(
            r.get("error").is_none(),
            "the server no longer refuses broken files: {r}"
        );
        let out = apply_lsp_edits(src, &r["result"]);
        assert_eq!(out, fmt::format_best_effort(src).text);
        assert_ne!(out, src);
        // a clean file still goes through the strict path (identical to `fmt::format`)
        let ok = "fn  main( ){let a=1;}\n";
        c.open("file:///ok.peb", ok);
        let r = c.result(
            "textDocument/formatting",
            json!({"textDocument": {"uri": "file:///ok.peb"}, "options": {}}),
        );
        assert_eq!(apply_lsp_edits(ok, &r), fmt::format(ok).unwrap());
        // range formatting works on broken files too (error-free items only)
        let rr = c.result("textDocument/rangeFormatting", json!({"textDocument": {"uri": URI}, "options": {}, "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 3}}}));
        let out = apply_lsp_edits(src, &rr);
        assert!(
            out.starts_with("fn one() {\n    let a = 1;\n}\n")
                && out.ends_with(&src[src.find("fn broken").unwrap()..]),
            "{out}"
        );
    });
}

#[test]
fn on_type_formatting_for_closing_brace_and_semicolon() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        let src = "fn  a( ){let x=1;println(to_str(x));}\nfn  b( ){let y=2;}\n";
        c.open(URI, src);
        let close = src.find("}\n").unwrap() + 1; // right after the first `}`
        let (l, ch) = lsp_position(src, close);
        let r = c.result("textDocument/onTypeFormatting", json!({"textDocument": {"uri": URI}, "position": {"line": l, "character": ch}, "ch": "}", "options": {}}));
        let out = apply_lsp_edits(src, &r);
        assert!(
            out.starts_with("fn a() {\n    let x = 1;\n    println(to_str(x));\n}\n"),
            "{out}"
        );
        assert!(
            out.ends_with("fn  b( ){let y=2;}\n"),
            "only the item containing the typed character is formatted: {out}"
        );
        // `;` typed in the second item
        let semi = src.rfind(';').unwrap() + 1;
        let (l, ch) = lsp_position(src, semi);
        let r = c.result("textDocument/onTypeFormatting", json!({"textDocument": {"uri": URI}, "position": {"line": l, "character": ch}, "ch": ";", "options": {}}));
        let out = apply_lsp_edits(src, &r);
        assert!(
            out.ends_with("fn b() {\n    let y = 2;\n}\n") && out.starts_with("fn  a( )"),
            "{out}"
        );
        // other characters do nothing; an unfinished function (does not parse) is left alone
        let r = c.result("textDocument/onTypeFormatting", json!({"textDocument": {"uri": URI}, "position": {"line": 0, "character": 5}, "ch": "x", "options": {}}));
        assert_eq!(r, json!([]));
        let open_src = "fn  a( ){let x=1;\n";
        c.open("file:///open.peb", open_src);
        let r = c.result("textDocument/onTypeFormatting", json!({"textDocument": {"uri": "file:///open.peb"}, "position": {"line": 0, "character": 17}, "ch": ";", "options": {}}));
        assert_eq!(r, json!([]), "never reformat code that does not parse");
        let init = c.server.document_text(URI).unwrap().to_string();
        assert_eq!(init, src, "request did not mutate the document");
    });
}

#[test]
fn capabilities_advertise_the_new_formatting_entry_points() {
    big_stack(|| {
        let mut c = Client::new(SyncKind::Incremental);
        let r = c.request("initialize", json!({}));
        let caps = &r["result"]["capabilities"];
        assert_eq!(
            caps["documentOnTypeFormattingProvider"],
            json!({"firstTriggerCharacter": "}", "moreTriggerCharacter": [";"]})
        );
        assert_eq!(caps["documentRangeFormattingProvider"], true);
    });
}

// ---------------------------------------------------------------- line endings

fn with_eol(lf: &str, eol: &str) -> String {
    lf.replace('\n', eol)
}

const SRC_LF: &str = "fn main(){\nlet s=\"😀😀\"; // 😀 emoji\nlet n:int =  1;\nprintln(s);\nprintln(to_str(n));\n}\n";

#[test]
fn formatter_preserves_the_dominant_line_ending() {
    let lf = fmt::format(SRC_LF).unwrap();
    assert!(!lf.contains('\r'));
    for eol in ["\r\n", "\r"] {
        let src = with_eol(SRC_LF, eol);
        let out = fmt::format(&src).unwrap();
        assert_eq!(
            out,
            with_eol(&lf, eol),
            "{eol:?}: same layout as LF, terminators preserved"
        );
        assert_eq!(fmt::format(&out).unwrap(), out, "{eol:?}: idempotent");
        assert_eq!(
            fmt::ast_fingerprint(&out),
            fmt::ast_fingerprint(SRC_LF),
            "{eol:?}: same program"
        );
        if eol == "\r" {
            assert!(!out.contains('\n'));
        } else {
            assert_eq!(
                out.matches('\n').count(),
                out.matches("\r\n").count(),
                "no bare LF in CRLF output"
            );
        }
    }
    // mixed endings: the majority wins, nothing is lost
    let mixed = "fn a() {\r\n}\r\nfn b() {\n}\r\nfn c() {\r\n}\r\n";
    let out = fmt::format(mixed).unwrap();
    assert!(
        out.matches("\r\n").count() == out.matches('\n').count() && out.contains("fn b()"),
        "{out:?}"
    );
    // block comments spanning lines keep the file's terminator
    let c = "/* a\r\n b */\r\nfn main() {}\r\n";
    assert_eq!(fmt::format(c).unwrap(), c);
}

#[test]
fn lsp_formatting_edits_have_correct_utf16_positions_for_every_line_ending() {
    big_stack(|| {
        for eol in ["\n", "\r\n", "\r"] {
            let src = with_eol(SRC_LF, eol);
            let mut c = Client::started(SyncKind::Incremental);
            c.open(URI, &src);
            let r = c.result(
                "textDocument/formatting",
                json!({"textDocument": {"uri": URI}, "options": {}}),
            );
            let out = apply_lsp_edits(&src, &r);
            assert_eq!(
                out,
                fmt::format(&src).unwrap(),
                "eol {eol:?}: edits applied by an independent client equal the formatter output"
            );
            for e in r.as_array().unwrap() {
                // The edit's range must be a valid position pair in the client's own line model.
                let s = &e["range"]["start"];
                let off = lsp_offset(
                    &src,
                    s["line"].as_u64().unwrap() as usize,
                    s["character"].as_u64().unwrap() as usize,
                );
                assert_eq!(
                    lsp_position(&src, off),
                    (
                        s["line"].as_u64().unwrap() as usize,
                        s["character"].as_u64().unwrap() as usize
                    ),
                    "eol {eol:?}: start is a canonical position"
                );
            }
            // range formatting on the same document
            let rr = c.result("textDocument/rangeFormatting", json!({"textDocument": {"uri": URI}, "options": {}, "range": {"start": {"line": 2, "character": 0}, "end": {"line": 2, "character": 3}}}));
            assert_eq!(
                apply_lsp_edits(&src, &rr),
                fmt::format(&src).unwrap(),
                "single item: range formatting is the whole thing"
            );
        }
    });
}

#[test]
fn diagnostics_and_hover_use_the_clients_line_model_for_every_line_ending() {
    big_stack(|| {
        let lf = "fn main() {\n    let s = \"😀\"; let n: int = \"oops\";\n    println(s);\n}\n";
        for eol in ["\n", "\r\n", "\r"] {
            let src = with_eol(lf, eol);
            let mut c = Client::started(SyncKind::Incremental);
            let d = c.open(URI, &src);
            let err = d
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["severity"] == 1)
                .expect("error diagnostic");
            assert_eq!(
                err["range"]["start"],
                json!({"line": 1, "character": 31}),
                "eol {eol:?}: {err}"
            );
            // hover on `println` on line 2
            let off = src.find("println").unwrap();
            let (l, ch) = lsp_position(&src, off);
            assert_eq!((l, ch), (2, 4), "oracle sanity for {eol:?}");
            let h = c.result(
                "textDocument/hover",
                json!({"textDocument": {"uri": URI}, "position": {"line": l, "character": ch}}),
            );
            assert!(
                h["contents"]["value"].as_str().unwrap().contains("println"),
                "eol {eol:?}: {h}"
            );
            assert_eq!(h["range"]["start"], json!({"line": 2, "character": 4}));
            // semantic tokens: the line deltas count every terminator once
            let t = c.result(
                "textDocument/semanticTokens/full",
                json!({"textDocument": {"uri": URI}}),
            );
            let lines: u64 = t["data"]
                .as_array()
                .unwrap()
                .chunks(5)
                .map(|x| x[0].as_u64().unwrap())
                .sum();
            assert_eq!(
                lines, 2,
                "eol {eol:?}: the last token (`println`) is on line 2"
            );
        }
    });
}

#[test]
fn incremental_sync_applies_edits_in_the_clients_line_model_for_every_line_ending() {
    big_stack(|| {
        for eol in ["\r\n", "\r", "\n"] {
            let mut text = with_eol(SRC_LF, eol);
            let mut c = Client::started(SyncKind::Incremental);
            c.open(URI, &text);
            let mut rng = Rng::new(31);
            let snippets = ["x", "😀", "\n", "\r\n", "\r", "", "fn "];
            for v in 0..200 {
                let a = floor(&text, rng.below(text.len() + 1));
                let b = floor(&text, (a + rng.below(6)).min(text.len()));
                // never split a CRLF pair: positions inside it are not addressable
                if text[..a].ends_with('\r') && text[a..].starts_with('\n')
                    || text[..b].ends_with('\r') && text[b..].starts_with('\n')
                {
                    continue;
                }
                let (sl, sc) = lsp_position(&text, a);
                let (el, ec) = lsp_position(&text, b);
                let new = *rng.pick(&snippets);
                c.notify("textDocument/didChange", json!({"textDocument": {"uri": URI, "version": v + 2}, "contentChanges": [{"range": {"start": {"line": sl, "character": sc}, "end": {"line": el, "character": ec}}, "text": new}]}));
                text.replace_range(a..b, new);
                assert_eq!(
                    c.server.document_text(URI).unwrap(),
                    text,
                    "eol {eol:?} step {v}: server text diverged from the client's"
                );
            }
        }
    });
}

#[test]
fn cli_fmt_preserves_crlf_and_check_mode_stays_strict() {
    let dir = std::env::temp_dir().join(format!("pebble_fmt_cli_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = env!("CARGO_BIN_EXE_pebble-tools");
    let f = dir.join("crlf.peb");
    std::fs::write(&f, "fn main(){\r\nlet x=1;\r\n}\r\n").unwrap();
    let st = std::process::Command::new(bin)
        .args(["fmt", f.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(st.success());
    assert_eq!(
        std::fs::read_to_string(&f).unwrap(),
        "fn main() {\r\n    let x = 1;\r\n}\r\n"
    );
    let st = std::process::Command::new(bin)
        .args(["fmt", "--check", f.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(st.success(), "a formatted CRLF file passes --check");
    // strict mode on a broken file: exit 1, file untouched; `--best-effort` formats what it can
    let g = dir.join("broken.peb");
    let broken = "fn  one( ){let a=1;}\nfn broken( { let = ; \n";
    std::fs::write(&g, broken).unwrap();
    let st = std::process::Command::new(bin)
        .args(["fmt", "--check", g.to_str().unwrap()])
        .status()
        .unwrap();
    assert_eq!(st.code(), Some(1));
    let st = std::process::Command::new(bin)
        .args(["fmt", g.to_str().unwrap()])
        .status()
        .unwrap();
    assert_eq!(st.code(), Some(1));
    assert_eq!(
        std::fs::read_to_string(&g).unwrap(),
        broken,
        "strict fmt never touches a file with errors"
    );
    let st = std::process::Command::new(bin)
        .args(["fmt", "--best-effort", g.to_str().unwrap()])
        .status()
        .unwrap();
    assert_eq!(st.code(), Some(0));
    let after = std::fs::read_to_string(&g).unwrap();
    assert!(
        after.starts_with("fn one() {") && after.contains("fn broken( { let = ; "),
        "{after}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Regression (found by seed 4 of the random range test): a `//` comment before the closing
/// `)` is printed after the item's `;`, so it must not swallow what follows on that line.
#[test]
fn a_line_comment_leaving_an_item_does_not_swallow_the_rest_of_the_line() {
    for eol in ["\n", "\r\n", "\r"] {
        for tail in ["/* multi\n line */\n", " fn g() { }\n", "\n", ""] {
            let src = format!("const K: int = (1 // why\n);{tail}").replace('\n', eol);
            // the compiler front end does not treat a lone `\r` as a line break; map it first
            let lf = |t: &str| t.replace("\r\n", "\n").replace('\r', "\n");
            assert!(clean(&lf(&src)), "{src:?}");
            let reps = fmt::format_range(&src, 0, 5);
            assert!(
                !reps.is_empty(),
                "{src:?}: the item must still be formatted"
            );
            let out = fmt::apply_replacements(&src, &reps);
            assert!(clean(&lf(&out)), "{src:?} -> {out:?}");
            assert_eq!(
                fmt::ast_fingerprint(&out),
                fmt::ast_fingerprint(&src),
                "{src:?} -> {out:?}"
            );
            assert_eq!(
                parse(&lf(&out)).comments.len(),
                parse(&lf(&src)).comments.len(),
                "{src:?} -> {out:?}"
            );
            assert!(
                out.ends_with(&tail.replace('\n', eol)),
                "text after the item is untouched: {out:?}"
            );
        }
    }
}
