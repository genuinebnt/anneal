//! RECIPE 01 (data): typed JSON parsing with defaults and strict rejection.
//! LEVEL: simple
//! Use in: stages 1 and 3.   Run: cargo run --example serde_defaults
//! COMMON USES: config/API payload parsing (serde is the default for JSON/TOML/YAML in nearly every Rust service); strict schemas reject typos early.
//!
//! Pattern: let the *types* do the validating. `u32` cannot be negative, a missing `Vec` can default to
//! empty, and `deny_unknown_fields` turns typos into errors instead of silently ignoring them.
//! Real code: https://github.com/serde-rs/serde (derive attributes: https://serde.rs/field-attrs.html)
#![allow(dead_code)]
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // fields are only read by Debug in this demo
struct Recipe {
    name: String,
    minutes: u32,
    #[serde(default)]
    ingredients: Vec<String>,
}

fn main() {
    let inputs = [
        r#"{"name":"toast","minutes":3,"ingredients":["bread"]}"#,
        r#"{"name":"water","minutes":1}"#, // default kicks in
        r#"{"name":"bad","minutes":-5}"#,  // u32 rejects negatives
        r#"{"name":"typo","minutes":1,"ingredient":["x"]}"#, // unknown field
        r#"{"name":7,"minutes":1}"#,       // wrong type
        r#"["not","an","object"]"#,        // wrong shape
    ];
    for input in inputs {
        match serde_json::from_str::<Recipe>(input) {
            Ok(r) => println!("ok:  {r:?}"),
            Err(e) => println!("err: {e}"),
        }
    }
}
