//! Golden LSP transcripts: `tests/lsp/*.jsonl`. Each line is `{"->": message}` (sent to the
//! server) or `{"<-": message}` (an expected output of the preceding `->`, in order).
//! Request ids are normalised to null on both sides ("modulo ids"). `BLESS=1` rewrites
//! the `<-` lines from the server's real output; review the diff before committing.

mod common;
use common::*;
use pebble_tools::lsp::{Server, SyncKind};
use serde_json::{Value, json};

fn normalise(mut v: Value) -> Value {
    if v.get("id").is_some() {
        v["id"] = Value::Null;
    }
    v
}

fn run_transcript(text: &str) -> String {
    let mut server = Server::new(SyncKind::Incremental);
    let mut out = String::new();
    for line in text
        .lines()
        .filter(|l| l.trim_start().starts_with("{\"->\""))
    {
        let v: Value = serde_json::from_str(line).unwrap();
        out.push_str(&format!("{}\n", json!({"->": v["->"]})));
        for m in server.handle(v["->"].clone()) {
            out.push_str(&format!("{}\n", json!({"<-": normalise(m)})));
        }
    }
    out
}

#[test]
fn transcripts_match() {
    big_stack(|| {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/lsp");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .collect();
        files.sort();
        assert!(files.len() >= 4, "expected at least 4 transcripts");
        let bless = std::env::var_os("BLESS").is_some();
        let mut failures = Vec::new();
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            let actual = run_transcript(&text);
            if bless {
                std::fs::write(&f, &actual).unwrap();
            } else if text != actual {
                // Compare line by line so the failure names the first differing exchange.
                let first = text
                    .lines()
                    .zip(actual.lines())
                    .position(|(a, b)| a != b)
                    .unwrap_or(0);
                failures.push(format!(
                    "{}: first difference at line {}\n expected: {}\n actual:   {}",
                    f.display(),
                    first + 1,
                    text.lines().nth(first).unwrap_or(""),
                    actual.lines().nth(first).unwrap_or("")
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}
