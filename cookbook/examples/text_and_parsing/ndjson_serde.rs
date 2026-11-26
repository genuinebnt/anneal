//! RECIPE 66 (parsing): serde beyond structs: tagged enums, renames, streaming NDJSON, tolerant line-by-line parsing.
//! LEVEL: simple
//! Use in: event logs, SSE payloads, API request/response bodies, persisted event streams.   Run: cargo run --release --example ndjson_serde
//! COMMON USES: event logs, webhooks, queue messages: tagged enums, tolerant NDJSON ingestion.
#![allow(dead_code)]
//!
//! `#[serde(tag = "type")]` turns an enum into JSON objects with a discriminator field: perfect for event streams.
//! Newline-delimited JSON (one JSON value per line) can be consumed LAZILY: parse each line independently so one bad
//! line does not lose the rest, and memory stays flat for huge logs.
//! Docs: https://serde.rs/enum-representations.html and https://serde.rs/attributes.html
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Cursor};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    Started {
        id: String,
    },
    Progress {
        id: String,
        percent: u8,
    },
    Finished {
        id: String,
        duration_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
}

fn main() {
    let log = r#"{"type":"started","id":"a"}
{"type":"progress","id":"a","percent":50}
not json at all
{"type":"finished","id":"a","duration_ms":1200}
{"type":"mystery","id":"a"}
{"type":"finished","id":"b","duration_ms":5,"note":"cached"}
"#;

    let mut good = vec![];
    let mut bad = vec![];
    for (n, line) in Cursor::new(log).lines().enumerate() {
        let line = line.unwrap();
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Event>(&line) {
            Ok(ev) => good.push(ev),
            Err(e) => bad.push(format!("line {}: {e}", n + 1)), // report and CONTINUE
        }
    }
    println!("{} parsed, {} bad", good.len(), bad.len());
    for b in &bad {
        println!("  {b}");
    }
    println!("{:?}", good.last());

    // serialize back: `note: None` is skipped, enum tag is written
    let out = serde_json::to_string(&Event::Finished {
        id: "z".into(),
        duration_ms: 9,
        note: None,
    })
    .unwrap();
    println!("{out}");
    assert_eq!(out, r#"{"type":"finished","id":"z","duration_ms":9}"#);

    // untyped access when you do not own the schema
    let v: serde_json::Value = serde_json::from_str(r#"{"a":{"b":[10,20]}}"#).unwrap();
    println!(
        "{:?} {:?} {:?}",
        v["a"]["b"][1],
        v.pointer("/a/b/0"),
        v["missing"]["x"].is_null()
    );

    // streaming several JSON values from ONE string (no newlines needed); stops at the first syntax error
    let stream = serde_json::Deserializer::from_str(
        r#"{"type":"started","id":"s1"} {"type":"started","id":"s2"}"#,
    )
    .into_iter::<Event>();
    println!(
        "{:?}",
        stream
            .map(|r| r.map(|e| format!("{e:?}")))
            .collect::<Vec<_>>()
            .len()
    );
    assert_eq!(good.len(), 4);
}
