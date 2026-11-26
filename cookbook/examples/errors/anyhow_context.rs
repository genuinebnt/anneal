//! RECIPE 60 (errors): `anyhow`: context chains for application-level errors, and downcasting back to the cause.
//! LEVEL: simple
//! Use in: `main`/CLI/glue code where you only need to REPORT an error well (libraries use thiserror, error_enum).   Run: cargo run --release --example anyhow_context
//! COMMON USES: application glue code and CLIs: add context at each layer, report the whole chain.
#![allow(dead_code)]
//!
//! `anyhow::Result<T>` = `Result<T, anyhow::Error>`: any error type converts with `?`, and `.context("what I was doing")`
//! stacks a human explanation on top. Print with `{:#}` for the whole chain on one line, `{:?}` for a multi-line report.
//! `bail!` returns an error early; `ensure!` is assert-as-error; `downcast_ref::<T>()` recovers the original type.
//! Real use: ripgrep's main https://github.com/BurntSushi/ripgrep/blob/master/crates/core/main.rs
use anyhow::{Context, Result, bail, ensure};

fn read_file(path: &str) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading workflow file `{path}`"))
}

fn parse(text: &str) -> Result<serde_json::Value> {
    serde_json::from_str(text).context("parsing workflow JSON")
}

fn check(v: &serde_json::Value) -> Result<usize> {
    let Some(tasks) = v["tasks"].as_array() else {
        bail!("`tasks` must be an array");
    };
    ensure!(!tasks.is_empty(), "workflow has no tasks");
    Ok(tasks.len())
}

fn load(path: &str) -> Result<usize> {
    let text = read_file(path)?;
    let v = parse(&text).with_context(|| format!("while loading `{path}`"))?;
    check(&v)
}

fn main() {
    let dir = std::env::temp_dir();
    let good = dir.join("cookbook_good.json");
    let bad_json = dir.join("cookbook_bad.json");
    let empty = dir.join("cookbook_empty.json");
    std::fs::write(&good, r#"{"tasks":[1,2]}"#).unwrap();
    std::fs::write(&bad_json, "{ nope").unwrap();
    std::fs::write(&empty, r#"{"tasks":[]}"#).unwrap();

    println!("{:?}", load(good.to_str().unwrap()));
    for p in [
        bad_json.to_str().unwrap(),
        empty.to_str().unwrap(),
        "/no/such/file.json",
    ] {
        let e = load(p).unwrap_err();
        println!("--- {{:#}} one line:   {e:#}");
        println!("    chain:");
        for (i, cause) in e.chain().enumerate() {
            println!("      {i}: {cause}");
        }
        if let Some(io) = e.downcast_ref::<std::io::Error>() {
            println!("    root cause is io::Error kind={:?}", io.kind());
        }
    }
}
