//! Multi-document and navigation features: workspace symbols (fuzzy ranking), workspace
//! folders and watched files, the per-version analysis cache, documentHighlight,
//! selectionRange, call hierarchy, typeDefinition, references and semantic-token
//! range/delta. Driven in-process.

mod common;
use common::client::*;
use common::*;
use pebble_tools::lsp::SyncKind;
use pebble_tools::lsp::fuzzy::{self, Score};
use pebble_tools::lsp::workspace::path_to_uri;
use serde_json::{Value, json};
use std::path::PathBuf;

const URI: &str = "file:///nav.peb";

const NAV: &str = "struct Point { x: int, y: int }
enum Shape { Circle(int), Rect { w: int, h: int } }
fn area(s: Shape) -> int {
    match s {
        Shape::Circle(r) => r * r * 3,
        Shape::Rect { w, h } => w * h,
    }
}
fn helper(n: int) -> int { n + 1 }
fn twice(n: int) -> int { helper(helper(n)) }
fn main() {
    let mut p = Point { x: 1, y: 2 };
    p.x = helper(p.y);
    let total = twice(p.x) + area(Shape::Circle(2));
    println(to_str(total));
}
";

fn at(src: &str, needle: &str, nth: usize) -> Value {
    let off = src
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("`{needle}` #{nth} missing"))
        .0;
    let (l, c) = lsp_position(src, off);
    json!({"line": l, "character": c})
}

fn td(uri: &str) -> Value {
    json!({"uri": uri})
}

fn pos_params(uri: &str, src: &str, needle: &str, nth: usize) -> Value {
    json!({"textDocument": td(uri), "position": at(src, needle, nth)})
}

fn line_of(v: &Value) -> u64 {
    v["range"]["start"]["line"].as_u64().unwrap()
}

// ---------------------------------------------------------------- fuzzy

fn is_subsequence_ci(q: &str, n: &str) -> bool {
    let mut it = n.chars().flat_map(|c| c.to_lowercase());
    q.chars()
        .flat_map(|c| c.to_lowercase())
        .all(|qc| it.any(|nc| nc == qc))
}

/// Brute-force oracle for the hump rule, independent of the implementation (tries every
/// assignment of query chars to name positions).
fn hump_oracle(q: &[char], n: &[char]) -> bool {
    fn word_start(n: &[char], i: usize) -> bool {
        if i == 0 {
            return true;
        }
        let (p, c) = (n[i - 1], n[i]);
        (p == '_' && c != '_')
            || (!p.is_alphanumeric() && c.is_alphanumeric())
            || (c.is_uppercase() && (p.is_lowercase() || p.is_ascii_digit()))
            || (c.is_ascii_digit() && !p.is_ascii_digit())
    }
    fn go(q: &[char], n: &[char], qi: usize, from: usize, prev: Option<usize>) -> bool {
        if qi == q.len() {
            return true;
        }
        (from..n.len()).any(|i| {
            n[i].to_lowercase().eq(q[qi].to_lowercase())
                && (word_start(n, i) || prev == Some(i.wrapping_sub(1)) && i > 0)
                && go(q, n, qi + 1, i + 1, Some(i))
        })
    }
    go(q, n, 0, 0, None)
}

fn random_name(rng: &mut Rng) -> String {
    let parts = [
        "get", "Http", "parse", "item", "x", "Z", "len", "_", "value", "Count", "9", "é",
    ];
    let mut s = String::new();
    for _ in 0..1 + rng.below(4) {
        s.push_str(rng.pick(&parts));
        if rng.chance(30) {
            s.push('_');
        }
    }
    s
}

fn random_query(rng: &mut Rng, name: &str) -> String {
    let cs: Vec<char> = name.chars().collect();
    match rng.below(4) {
        0 => cs.iter().filter(|_| rng.chance(40)).collect(), // a random subsequence of the name
        1 => cs[..rng.below(cs.len() + 1)].iter().collect(),
        2 => name.to_uppercase(),
        _ => random_name(rng),
    }
}

#[test]
fn fuzzy_match_is_sound_complete_and_correctly_tiered() {
    let mut rng = Rng::new(7);
    let (mut matched, mut unmatched) = (0, 0);
    for _ in 0..4000 {
        let name = random_name(&mut rng);
        let q = random_query(&mut rng, &name);
        let sc = fuzzy::score(&q, &name);
        let sub = is_subsequence_ci(&q, &name);
        // soundness and completeness: it matches exactly when the query is a subsequence
        assert_eq!(sc.is_some(), sub, "query {q:?} vs {name:?}");
        let Some(sc) = sc else {
            unmatched += 1;
            continue;
        };
        matched += 1;
        let (qc, nc): (Vec<char>, Vec<char>) = (q.chars().collect(), name.chars().collect());
        let lower = |s: &str| s.to_lowercase();
        let expected = if q.is_empty() {
            0
        } else if q == name {
            6
        } else if qc.len() == nc.len() && lower(&q) == lower(&name) {
            5
        } else if name.starts_with(&q) {
            4
        } else if lower(&name).starts_with(&lower(&q)) {
            3
        } else if hump_oracle(&qc, &nc) {
            2
        } else {
            1
        };
        // The oracle uses to_lowercase of whole strings; skip the exotic length-changing cases.
        if lower(&q).chars().count() == qc.len() && lower(&name).chars().count() == nc.len() {
            assert_eq!(sc.tier, expected, "tier of {q:?} vs {name:?}");
        }
    }
    assert!(
        matched > 500 && unmatched > 500,
        "weak generator: {matched}/{unmatched}"
    );
}

#[test]
fn fuzzy_ranking_orders_exact_prefix_hump_then_subsequence() {
    let names = [
        "parse_item",
        "ParseItem",
        "parse",
        "xparse",
        "pa_rse_item_x",
        "PARSE",
        "p",
        "append_rest",
        "rparsex",
        "parser",
    ];
    let mut ranked: Vec<(Score, String)> = names
        .iter()
        .filter_map(|n| fuzzy::score("parse", n).map(|s| (s, n.to_string())))
        .collect();
    fuzzy::rank(&mut ranked);
    let order: Vec<&str> = ranked.iter().map(|r| r.1.as_str()).collect();
    assert_eq!(order[0], "parse", "exact match first: {order:?}");
    assert_eq!(order[1], "PARSE", "then equal ignoring case: {order:?}");
    // case-sensitive prefixes (shorter first), then case-insensitive prefixes
    assert_eq!(&order[2..4], ["parser", "parse_item"], "{order:?}");
    assert_eq!(order[4], "ParseItem");
    assert!(
        order.contains(&"xparse") && !order.contains(&"p"),
        "{order:?}"
    );
    // tiers never increase down the list; ranking is deterministic
    for w in ranked.windows(2) {
        assert!(w[0].0 >= w[1].0);
    }
    let mut again = ranked.clone();
    again.reverse();
    fuzzy::rank(&mut again);
    assert_eq!(ranked, again);
    // camelCase / underscore initials
    assert_eq!(fuzzy::score("gH", "getHttpResponse").unwrap().tier, 2);
    assert_eq!(fuzzy::score("ghr", "get_http_response").unwrap().tier, 2);
    assert!(fuzzy::score("xyz", "getHttpResponse").is_none());
}

#[test]
fn fuzzy_ranking_is_total_and_transitive_on_random_inputs() {
    let mut rng = Rng::new(99);
    let names: Vec<String> = (0..60).map(|_| random_name(&mut rng)).collect();
    for _ in 0..200 {
        let pick = rng.below(names.len());
        let q = random_query(&mut rng, &names[pick]);
        let mut v: Vec<(Score, String)> = names
            .iter()
            .filter_map(|n| fuzzy::score(&q, n).map(|s| (s, n.clone())))
            .collect();
        fuzzy::rank(&mut v);
        for w in v.windows(2) {
            assert!(
                w[0].0 > w[1].0 || (w[0].0 == w[1].0 && w[0].1 <= w[1].1),
                "{q:?}: {:?} before {:?}",
                w[0],
                w[1]
            );
        }
    }
}

// ---------------------------------------------------------------- workspace folders

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pebble_ws_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn workspace_symbols_scan_folders_cache_and_watch_files() {
    big_stack(|| {
        let dir = temp_dir("sym");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        std::fs::write(
            dir.join("a.peb"),
            "fn parse_item() {}\nfn main() {}\nstruct ParseState { depth: int }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("sub/b.peb"),
            "enum Token { Plus, Minus(int) }\nconst LIMIT: int = 3;\nfn tokenize() {}\n",
        )
        .unwrap();
        std::fs::write(dir.join("target/skip.peb"), "fn skipped_in_target() {}\n").unwrap();
        std::fs::write(dir.join(".hidden/skip.peb"), "fn skipped_hidden() {}\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "fn not_pebble() {}\n").unwrap();
        let root = path_to_uri(&dir);

        let mut c = Client::new(SyncKind::Incremental);
        let init = c.request("initialize", json!({"rootUri": root, "capabilities": {"workspace": {"didChangeWatchedFiles": {"dynamicRegistration": true}}}}));
        assert_eq!(
            init["result"]["capabilities"]["workspaceSymbolProvider"],
            true
        );
        assert_eq!(
            init["result"]["capabilities"]["workspace"]["workspaceFolders"]["supported"],
            true
        );
        let out = c.notify("initialized", json!({}));
        let reg = out
            .iter()
            .find(|m| m["method"] == "client/registerCapability")
            .expect("dynamic registration of the file watcher");
        assert_eq!(
            reg["params"]["registrations"][0]["registerOptions"]["watchers"][0]["globPattern"],
            "**/*.peb"
        );
        assert_eq!(
            c.server.workspace.files().len(),
            2,
            "target/, hidden dirs and non-.peb files are skipped"
        );

        let all = names(&c.result("workspace/symbol", json!({"query": ""})));
        for want in [
            "parse_item",
            "main",
            "ParseState",
            "depth",
            "Token",
            "Plus",
            "Minus",
            "LIMIT",
            "tokenize",
        ] {
            assert!(all.contains(&want.to_string()), "{want} in {all:?}");
        }
        assert!(
            !all.iter()
                .any(|n| n.starts_with("skipped") || n == "not_pebble")
        );
        let analyses = c.server.workspace.disk_analyses;
        assert_eq!(analyses, 2);

        // fuzzy ranking: exact, prefix, then hump (initials), then subsequence
        let r = c.result("workspace/symbol", json!({"query": "parse"}));
        assert_eq!(names(&r)[..2], ["parse_item", "ParseState"], "{r}");
        let r = c.result("workspace/symbol", json!({"query": "pi"}));
        assert_eq!(names(&r)[0], "parse_item", "initials of snake_case");
        let r = c.result("workspace/symbol", json!({"query": "PS"}));
        assert_eq!(names(&r)[0], "ParseState", "initials of CamelCase");
        let r = c.result("workspace/symbol", json!({"query": "plus"}));
        assert_eq!(r[0]["name"], "Plus");
        assert_eq!(r[0]["containerName"], "Token");
        assert_eq!(r[0]["kind"], 22);
        assert!(
            r[0]["location"]["uri"]
                .as_str()
                .unwrap()
                .ends_with("/sub/b.peb")
        );
        assert_eq!(
            c.server.workspace.disk_analyses, analyses,
            "unchanged files are served from the cache"
        );

        // an open document replaces its on-disk version in results (unsaved edits are searchable)
        let a_uri = path_to_uri(&dir.join("a.peb"));
        c.open(&a_uri, "fn parse_item_renamed() {}\n");
        let r = names(&c.result("workspace/symbol", json!({"query": "parse"})));
        assert!(
            r.contains(&"parse_item_renamed".to_string())
                && !r.contains(&"parse_item".to_string())
                && !r.contains(&"ParseState".to_string()),
            "{r:?}"
        );
        c.notify(
            "textDocument/didClose",
            json!({"textDocument": {"uri": a_uri}}),
        );
        let r = names(&c.result("workspace/symbol", json!({"query": "parse"})));
        assert!(
            r.contains(&"parse_item".to_string()),
            "closing falls back to disk: {r:?}"
        );

        // didChangeWatchedFiles: created, changed, deleted
        std::fs::write(dir.join("c.peb"), "fn created_later() {}\n").unwrap();
        c.notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": path_to_uri(&dir.join("c.peb")), "type": 1}]}),
        );
        assert!(
            names(&c.result("workspace/symbol", json!({"query": "created"})))
                .contains(&"created_later".to_string())
        );
        std::fs::write(
            dir.join("c.peb"),
            "fn changed_name_here() {}\nfn extra() {}\n",
        )
        .unwrap();
        c.notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": path_to_uri(&dir.join("c.peb")), "type": 2}]}),
        );
        assert!(names(&c.result("workspace/symbol", json!({"query": "created"}))).is_empty());
        assert!(
            names(&c.result("workspace/symbol", json!({"query": "changed"})))
                .contains(&"changed_name_here".to_string())
        );
        std::fs::remove_file(dir.join("c.peb")).unwrap();
        c.notify(
            "workspace/didChangeWatchedFiles",
            json!({"changes": [{"uri": path_to_uri(&dir.join("c.peb")), "type": 3}]}),
        );
        assert!(names(&c.result("workspace/symbol", json!({"query": "changed"}))).is_empty());
        // events for files outside the workspace or not .peb are ignored
        c.notify("workspace/didChangeWatchedFiles", json!({"changes": [{"uri": "file:///elsewhere/x.peb", "type": 1}, {"uri": path_to_uri(&dir.join("n.txt")), "type": 1}]}));
        assert_eq!(c.server.workspace.files().len(), 2);

        // workspace folder changes
        let other = temp_dir("sym2");
        std::fs::write(other.join("z.peb"), "fn zeta_function() {}\n").unwrap();
        c.notify(
            "workspace/didChangeWorkspaceFolders",
            json!({"event": {"added": [{"uri": path_to_uri(&other), "name": "o"}], "removed": []}}),
        );
        assert!(
            names(&c.result("workspace/symbol", json!({"query": "zeta"})))
                .contains(&"zeta_function".to_string())
        );
        c.notify(
            "workspace/didChangeWorkspaceFolders",
            json!({"event": {"added": [], "removed": [{"uri": path_to_uri(&other), "name": "o"}]}}),
        );
        assert!(names(&c.result("workspace/symbol", json!({"query": "zeta"}))).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    });
}

#[test]
fn workspace_scan_is_capped() {
    let dir = temp_dir("cap");
    for i in 0..2100 {
        std::fs::write(dir.join(format!("f{i:04}.peb")), "fn f() {}\n").unwrap();
    }
    let mut ws = pebble_tools::lsp::workspace::Workspace::default();
    ws.set_folders(vec![dir.clone()]);
    assert_eq!(ws.files().len(), pebble_tools::lsp::workspace::MAX_FILES);
    assert!(ws.truncated);
    // depth cap: a file 20 levels down is not found
    let deep = temp_dir("deep");
    let mut p = deep.clone();
    for i in 0..20 {
        p = p.join(format!("d{i}"));
    }
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(p.join("deep.peb"), "fn f() {}\n").unwrap();
    std::fs::write(deep.join("top.peb"), "fn f() {}\n").unwrap();
    let mut ws = pebble_tools::lsp::workspace::Workspace::default();
    ws.set_folders(vec![deep.clone()]);
    assert_eq!(ws.files().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&deep);
}

#[test]
fn workspace_symbols_without_folders_search_open_documents() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        c.open("file:///one.peb", "fn alpha_one() {}\n");
        c.open(
            "file:///two.peb",
            "fn alpha_two() {}\nstruct Alpha { f: int }\n",
        );
        let r = c.result("workspace/symbol", json!({"query": "alpha"}));
        assert_eq!(
            names(&r),
            ["Alpha", "alpha_one", "alpha_two"].map(String::from),
            "case-sensitive prefix beats case-insensitive; ties by name: {r}"
        );
        let r = c.result("workspace/symbol", json!({"query": "zzz"}));
        assert_eq!(r, json!([]));
    });
}

// ---------------------------------------------------------------- analysis cache

#[test]
fn analysis_cache_is_keyed_by_version() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, NAV);
        assert_eq!(
            c.server.analyses, 1,
            "didOpen analyses once for the diagnostics"
        );
        for _ in 0..5 {
            c.result("textDocument/hover", pos_params(URI, NAV, "helper", 0));
            c.result(
                "textDocument/documentSymbol",
                json!({"textDocument": td(URI)}),
            );
            c.result(
                "textDocument/semanticTokens/full",
                json!({"textDocument": td(URI)}),
            );
        }
        assert_eq!(
            c.server.analyses, 1,
            "requests at one version share one analysis"
        );
        let change = |v: i64, text: &str| json!({"textDocument": {"uri": URI, "version": v}, "contentChanges": [{"text": text}]});
        c.notify("textDocument/didChange", change(2, "fn main() {}\n"));
        assert_eq!(c.server.analyses, 2);
        c.result(
            "textDocument/hover",
            pos_params(URI, "fn main() {}\n", "main", 0),
        );
        assert_eq!(c.server.analyses, 2);
        // A client that reuses a version number for different text must still get fresh results.
        c.notify("textDocument/didChange", change(2, "fn other() {}\n"));
        let syms = c.result(
            "textDocument/documentSymbol",
            json!({"textDocument": td(URI)}),
        );
        assert_eq!(syms[0]["name"], "other");
        assert_eq!(c.server.analyses, 3);
        assert_eq!(c.server.document_version(URI), Some(2));
    });
}

// ---------------------------------------------------------------- highlight, selection, hierarchy, types

#[test]
fn document_highlight_marks_reads_and_writes() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, NAV);
        let kinds = |r: &Value| {
            r.as_array()
                .unwrap()
                .iter()
                .map(|h| h["kind"].as_u64().unwrap())
                .collect::<Vec<_>>()
        };
        // `p`: declaration (write), `p.x = ..` (write through the place), `p.y`, `p.x` (reads)
        let r = c.result(
            "textDocument/documentHighlight",
            pos_params(URI, NAV, "p.y", 0),
        );
        assert_eq!(kinds(&r), [3, 3, 2, 2], "{r}");
        // the field `x`: declaration, struct-literal name, assignment target `p.x =`, read `p.x`
        let r = c.result(
            "textDocument/documentHighlight",
            pos_params(URI, NAV, "x: int", 0),
        );
        assert_eq!(r.as_array().unwrap().len(), 4, "{r}");
        assert_eq!(kinds(&r)[0], 3);
        assert_eq!(
            kinds(&r).iter().filter(|k| **k == 3).count(),
            2,
            "declaration and the assignment target: {r}"
        );
        // `helper`: declaration + 3 uses
        let r = c.result(
            "textDocument/documentHighlight",
            pos_params(URI, NAV, "helper(n", 0),
        );
        assert_eq!(kinds(&r), [3, 2, 2, 2]);
        // nothing under the cursor: null
        let r = c.result(
            "textDocument/documentHighlight",
            pos_params(URI, NAV, "-> int {", 0),
        );
        assert!(r.is_null());
    });
}

#[test]
fn references_respect_include_declaration() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, NAV);
        let ask = |c: &mut Client, include: bool, needle: &str, nth: usize| {
            let mut p = pos_params(URI, NAV, needle, nth);
            p["context"] = json!({"includeDeclaration": include});
            c.result("textDocument/references", p)
        };
        let with = ask(&mut c, true, "helper(p.y", 0);
        let without = ask(&mut c, false, "helper(p.y", 0);
        assert_eq!(with.as_array().unwrap().len(), 4);
        assert_eq!(without.as_array().unwrap().len(), 3);
        assert!(
            with.as_array().unwrap().iter().any(|l| line_of(l) == 8),
            "declaration is on line 9: {with}"
        );
        assert!(!without.as_array().unwrap().iter().any(|l| line_of(l) == 8));
        // from the declaration site the answer is the same set
        assert_eq!(ask(&mut c, true, "helper(n: int", 0), with);
        // variants and fields
        assert_eq!(
            ask(&mut c, true, "Circle", 1).as_array().unwrap().len(),
            3,
            "variant: declaration, pattern, construction"
        );
        assert_eq!(ask(&mut c, false, "Circle", 1).as_array().unwrap().len(), 2);
        assert_eq!(ask(&mut c, true, "y: int", 0).as_array().unwrap().len(), 3);
        // default (no context) includes the declaration
        let r = c.result(
            "textDocument/references",
            pos_params(URI, NAV, "helper(p.y", 0),
        );
        assert_eq!(r.as_array().unwrap().len(), 4);
    });
}

fn contains(outer: &Value, inner: &Value) -> bool {
    let key = |p: &Value| {
        (
            p["line"].as_u64().unwrap(),
            p["character"].as_u64().unwrap(),
        )
    };
    key(&outer["start"]) <= key(&inner["start"]) && key(&inner["end"]) <= key(&outer["end"])
}

#[test]
fn selection_ranges_nest_strictly_outward_over_the_corpus() {
    big_stack(|| {
        let mut rng = Rng::new(3);
        let mut chains = 0;
        for (path, text) in corpus().into_iter().take(40) {
            let mut c = Client::started(SyncKind::Incremental);
            c.open(URI, &text);
            let positions: Vec<Value> = (0..25)
                .map(|_| {
                    let mut off = rng.below(text.len() + 1);
                    while !text.is_char_boundary(off) {
                        off -= 1;
                    }
                    let (l, ch) = lsp_position(&text, off);
                    json!({"line": l, "character": ch})
                })
                .collect();
            let r = c.result(
                "textDocument/selectionRange",
                json!({"textDocument": td(URI), "positions": positions}),
            );
            let r = r.as_array().unwrap();
            assert_eq!(r.len(), positions.len());
            for (pos, sel) in positions.iter().zip(r) {
                let mut cur = sel;
                assert!(
                    contains(&cur["range"], &json!({"start": pos, "end": pos})),
                    "{}: innermost range must contain the cursor",
                    path.display()
                );
                let mut last_range = cur["range"].clone();
                let mut depth = 1;
                while let Some(parent) = cur.get("parent") {
                    assert!(
                        contains(&parent["range"], &last_range),
                        "{}: parent contains child",
                        path.display()
                    );
                    assert_ne!(
                        parent["range"],
                        last_range,
                        "{}: strictly growing",
                        path.display()
                    );
                    last_range = parent["range"].clone();
                    cur = parent;
                    depth += 1;
                }
                let whole = lsp_position(&text, text.len());
                assert_eq!(
                    last_range["start"],
                    json!({"line": 0, "character": 0}),
                    "outermost starts at 0"
                );
                assert_eq!(
                    (
                        last_range["end"]["line"].as_u64().unwrap() as usize,
                        last_range["end"]["character"].as_u64().unwrap() as usize
                    ),
                    whole,
                    "outermost covers the file"
                );
                chains += 1;
                assert!(depth >= 1);
            }
        }
        assert!(chains > 500);
        // a concrete chain: identifier, call, statement, block, function, file
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, NAV);
        let r = c.result(
            "textDocument/selectionRange",
            json!({"textDocument": td(URI), "positions": [at(NAV, "helper(p.y", 0)]}),
        );
        let mut depth = 0;
        let mut cur = &r[0];
        let first = cur["range"].clone();
        assert_eq!(
            first["end"]["character"].as_u64().unwrap()
                - first["start"]["character"].as_u64().unwrap(),
            6,
            "starts at the identifier `helper`: {first}"
        );
        while let Some(p) = cur.get("parent") {
            cur = p;
            depth += 1;
        }
        assert!(depth >= 5, "depth {depth}");
    });
}

#[test]
fn call_hierarchy_incoming_and_outgoing() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, NAV);
        let prep = c.result(
            "textDocument/prepareCallHierarchy",
            pos_params(URI, NAV, "helper(n: int", 0),
        );
        assert_eq!(prep.as_array().unwrap().len(), 1);
        assert_eq!(prep[0]["name"], "helper");
        assert_eq!(prep[0]["kind"], 12);
        assert!(
            c.result(
                "textDocument/prepareCallHierarchy",
                pos_params(URI, NAV, "Point {", 0)
            )
            .is_null(),
            "a struct is not callable"
        );
        let incoming = c.result("callHierarchy/incomingCalls", json!({"item": prep[0]}));
        let inc: Vec<(String, usize)> = incoming
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                (
                    i["from"]["name"].as_str().unwrap().to_string(),
                    i["fromRanges"].as_array().unwrap().len(),
                )
            })
            .collect();
        assert_eq!(
            inc,
            [("twice".to_string(), 2), ("main".to_string(), 1)],
            "{incoming}"
        );
        // outgoing from main: area, helper, twice (sorted by definition order), built-ins excluded
        let main = c.result(
            "textDocument/prepareCallHierarchy",
            pos_params(URI, NAV, "main()", 0),
        );
        let outgoing = c.result("callHierarchy/outgoingCalls", json!({"item": main[0]}));
        let out: Vec<(String, usize)> = outgoing
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                (
                    i["to"]["name"].as_str().unwrap().to_string(),
                    i["fromRanges"].as_array().unwrap().len(),
                )
            })
            .collect();
        assert_eq!(
            out,
            [
                ("area".to_string(), 1),
                ("helper".to_string(), 1),
                ("twice".to_string(), 1)
            ],
            "{outgoing}"
        );
        let twice = c.result(
            "textDocument/prepareCallHierarchy",
            pos_params(URI, NAV, "twice(n", 0),
        );
        let outgoing = c.result("callHierarchy/outgoingCalls", json!({"item": twice[0]}));
        assert_eq!(outgoing[0]["fromRanges"].as_array().unwrap().len(), 2);
        // main is called by nobody
        assert_eq!(
            c.result("callHierarchy/incomingCalls", json!({"item": main[0]})),
            json!([])
        );
        // the item resolves from a use site too
        let from_use = c.result(
            "textDocument/prepareCallHierarchy",
            pos_params(URI, NAV, "helper(p.y", 0),
        );
        assert_eq!(from_use[0]["selectionRange"], prep[0]["selectionRange"]);
        // recursion: a function that calls itself is its own caller
        let rec = "fn fact(n: int) -> int { if n < 2 { 1 } else { n * fact(n - 1) } }\nfn main() { println(to_str(fact(5))); }\n";
        c.open("file:///rec.peb", rec);
        let item = c.result(
            "textDocument/prepareCallHierarchy",
            pos_params("file:///rec.peb", rec, "fact(n", 0),
        );
        let incoming = c.result("callHierarchy/incomingCalls", json!({"item": item[0]}));
        let from: Vec<&str> = incoming
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["from"]["name"].as_str().unwrap())
            .collect();
        assert_eq!(from, ["fact", "main"]);
    });
}

#[test]
fn type_definition_jumps_to_the_struct_or_enum() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, NAV);
        let ask = |c: &mut Client, needle: &str, nth: usize| {
            c.result(
                "textDocument/typeDefinition",
                pos_params(URI, NAV, needle, nth),
            )
        };
        let p = ask(&mut c, "p.y", 0);
        assert_eq!(
            line_of(&p),
            0,
            "variable of type Point -> struct Point: {p}"
        );
        let s = ask(&mut c, "s {", 0);
        assert_eq!(line_of(&s), 1, "parameter of type Shape -> enum Shape");
        assert_eq!(
            line_of(&ask(&mut c, "Circle(2)", 0)),
            1,
            "variant -> its enum"
        );
        assert!(
            ask(&mut c, "total", 0).is_null(),
            "int has no definition in the file"
        );
        assert!(ask(&mut c, "n + 1", 0).is_null());
        // a function returning a struct jumps to that struct
        let src = "struct Box2 { v: int }\nfn make() -> Box2 { Box2 { v: 1 } }\nfn main() { let b = make(); println(to_str(b.v)); }\n";
        c.open("file:///t2.peb", src);
        let r = c.result(
            "textDocument/typeDefinition",
            pos_params("file:///t2.peb", src, "b.v", 0),
        );
        assert_eq!(line_of(&r), 0);
        let r = c.result(
            "textDocument/typeDefinition",
            pos_params("file:///t2.peb", src, "make()", 1),
        );
        assert_eq!(line_of(&r), 0, "callee of a fn returning Box2");
    });
}

// ---------------------------------------------------------------- semantic tokens

fn decode(data: &Value) -> Vec<(u64, u64, u64, u64, u64)> {
    let d: Vec<u64> = data
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    assert_eq!(d.len() % 5, 0);
    let (mut line, mut col) = (0, 0);
    d.chunks(5)
        .map(|t| {
            line += t[0];
            col = if t[0] == 0 { col + t[1] } else { t[1] };
            (line, col, t[2], t[3], t[4])
        })
        .collect()
}

#[test]
fn semantic_token_range_is_the_filtered_full_result() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        let src = std::fs::read_to_string(compiler_dir().join("tests/run/expr_eval.peb")).unwrap();
        c.open(URI, &src);
        let full = decode(
            &c.result(
                "textDocument/semanticTokens/full",
                json!({"textDocument": td(URI)}),
            )["data"],
        );
        assert!(full.len() > 50);
        let lines = src.lines().count() as u64;
        let mut rng = Rng::new(11);
        for _ in 0..60 {
            let (a, b) = (
                rng.below(lines as usize + 2) as u64,
                rng.below(lines as usize + 2) as u64,
            );
            let (ca, cb) = (rng.below(12) as u64, rng.below(12) as u64);
            let (s, e) = if (a, ca) <= (b, cb) {
                ((a, ca), (b, cb))
            } else {
                ((b, cb), (a, ca))
            };
            let r = c.result("textDocument/semanticTokens/range", json!({"textDocument": td(URI), "range": {"start": {"line": s.0, "character": s.1}, "end": {"line": e.0, "character": e.1}}}));
            let got = decode(&r["data"]);
            let want: Vec<_> = full
                .iter()
                .copied()
                .filter(|&(l, ch, n, _, _)| (l, ch) < e && (l, ch + n) > s)
                .collect();
            assert_eq!(got, want, "range {s:?}..{e:?}");
        }
        // the whole document as a range equals the full result
        let r = c.result("textDocument/semanticTokens/range", json!({"textDocument": td(URI), "range": {"start": {"line": 0, "character": 0}, "end": {"line": lines + 5, "character": 0}}}));
        assert_eq!(decode(&r["data"]), full);
    });
}

fn apply_delta(old: &[u64], edits: &Value) -> Vec<u64> {
    let mut out = old.to_vec();
    let mut es: Vec<(usize, usize, Vec<u64>)> = edits
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["start"].as_u64().unwrap() as usize,
                e["deleteCount"].as_u64().unwrap() as usize,
                e["data"]
                    .as_array()
                    .map(|d| d.iter().map(|v| v.as_u64().unwrap()).collect())
                    .unwrap_or_default(),
            )
        })
        .collect();
    es.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (start, del, data) in es {
        out.splice(start..start + del, data);
    }
    out
}

fn ints(v: &Value) -> Vec<u64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap())
        .collect()
}

#[test]
fn semantic_token_delta_applied_equals_full_recompute_over_500_random_edits() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        let mut text =
            std::fs::read_to_string(compiler_dir().join("tests/run/expr_eval.peb")).unwrap();
        c.open(URI, &text);
        let first = c.result(
            "textDocument/semanticTokens/full",
            json!({"textDocument": td(URI)}),
        );
        let (mut prev_id, mut prev) = (
            first["resultId"].as_str().unwrap().to_string(),
            ints(&first["data"]),
        );
        let mut rng = Rng::new(21);
        let snippets = [
            "x",
            " ",
            "\n",
            "fn ",
            "let a = 1;",
            "😀",
            "// c\n",
            "}",
            "{",
            "\"s\"",
            "",
            "",
            "struct S {}",
            "(",
            ")",
        ];
        let (mut deltas, mut fulls) = (0, 0);
        for step in 0..500 {
            // random edit of 0..8 chars
            let mut a = rng.below(text.len() + 1);
            while !text.is_char_boundary(a) {
                a -= 1;
            }
            let mut b = (a + rng.below(8)).min(text.len());
            while !text.is_char_boundary(b) {
                b -= 1;
            }
            let new = rng.pick(&snippets);
            let (sl, sc) = lsp_position(&text, a);
            let (el, ec) = lsp_position(&text, b);
            c.notify(
                "textDocument/didChange",
                json!({"textDocument": {"uri": URI, "version": step + 2}, "contentChanges": [{"range": {"start": {"line": sl, "character": sc}, "end": {"line": el, "character": ec}}, "text": new}]}),
            );
            text.replace_range(a..b, new);
            if text.len() > 20_000 {
                text.truncate(10_000);
                while !text.is_char_boundary(text.len()) {
                    text.pop();
                }
                c.notify("textDocument/didChange", json!({"textDocument": {"uri": URI, "version": step + 2}, "contentChanges": [{"text": text.clone()}]}));
            }
            let r = c.result(
                "textDocument/semanticTokens/full/delta",
                json!({"textDocument": td(URI), "previousResultId": prev_id}),
            );
            let fresh = c.result(
                "textDocument/semanticTokens/full",
                json!({"textDocument": td(URI)}),
            );
            let fresh_data = ints(&fresh["data"]);
            let got = if r.get("edits").is_some() {
                deltas += 1;
                assert_eq!(r["edits"].as_array().unwrap().len() <= 1, true);
                apply_delta(&prev, &r["edits"])
            } else {
                fulls += 1;
                ints(&r["data"])
            };
            assert_eq!(
                got, fresh_data,
                "step {step}: delta applied to the previous tokens must equal a full recompute"
            );
            // continue from the delta's result id (as a real client does)
            prev_id = r["resultId"].as_str().unwrap().to_string();
            prev = got;
        }
        assert!(deltas >= 450, "{deltas} deltas, {fulls} fulls");
        // unchanged text: an empty edit list
        let a = c.result(
            "textDocument/semanticTokens/full",
            json!({"textDocument": td(URI)}),
        );
        let r = c.result(
            "textDocument/semanticTokens/full/delta",
            json!({"textDocument": td(URI), "previousResultId": a["resultId"]}),
        );
        assert_eq!(r["edits"], json!([]));
    });
}

#[test]
fn semantic_token_result_cache_is_bounded() {
    big_stack(|| {
        let mut c = Client::started(SyncKind::Incremental);
        c.open(URI, NAV);
        let first = c.result(
            "textDocument/semanticTokens/full",
            json!({"textDocument": td(URI)}),
        );
        let old_id = first["resultId"].clone();
        for _ in 0..12 {
            c.result(
                "textDocument/semanticTokens/full",
                json!({"textDocument": td(URI)}),
            );
        }
        // the first result id was evicted: the server answers with full data, not a delta
        let r = c.result(
            "textDocument/semanticTokens/full/delta",
            json!({"textDocument": td(URI), "previousResultId": old_id}),
        );
        assert!(
            r.get("data").is_some() && r.get("edits").is_none(),
            "evicted id falls back to full tokens"
        );
        // an unknown id behaves the same
        let r = c.result(
            "textDocument/semanticTokens/full/delta",
            json!({"textDocument": td(URI), "previousResultId": "nonsense"}),
        );
        assert!(r.get("data").is_some());
        // closing the document drops its cache
        let last = c.result(
            "textDocument/semanticTokens/full",
            json!({"textDocument": td(URI)}),
        );
        c.notify("textDocument/didClose", json!({"textDocument": td(URI)}));
        c.open(URI, NAV);
        let r = c.result(
            "textDocument/semanticTokens/full/delta",
            json!({"textDocument": td(URI), "previousResultId": last["resultId"]}),
        );
        assert!(r.get("data").is_some());
    });
}
