//! EXPECT-EXIT: 1   (this demo intentionally ends in an error; checker reads this marker)
//! RECIPE 02 (errors): one error enum, `?` everywhere, and a thin `main`.
//! LEVEL: simple
//! Use in: stages 2 and 4.   Run: cargo run --example error_enum [path]
//! COMMON USES: every CLI/service: one error enum per crate, `?` everywhere, only `main` maps errors to exit codes or HTTP statuses.
//!
//! Pattern: all logic lives in `run() -> Result<_, Error>`. Only `main` turns an error into an exit code.
//! `#[from]` lets `?` convert io/serde errors automatically. Variants can carry data for good messages.
//! Real code: ripgrep's main.rs -> https://github.com/BurntSushi/ripgrep/blob/master/crates/core/main.rs
//!            thiserror        -> https://github.com/dtolnay/thiserror
#![allow(dead_code)]
use std::process::ExitCode;

#[derive(Debug, thiserror::Error)]
enum CookError {
    #[error("usage: error_enum <path>")]
    Usage,
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid json: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("recipe `{0}` has no steps")]
    NoSteps(String),
}

fn run(args: &[String]) -> Result<usize, CookError> {
    let path = args.get(1).ok_or(CookError::Usage)?;
    let text = std::fs::read_to_string(path).map_err(|source| CookError::Read {
        path: path.clone(),
        source,
    })?;
    let value: serde_json::Value = serde_json::from_str(&text)?; // `?` uses #[from]
    let steps = value["steps"].as_array().map_or(0, |s| s.len());
    if steps == 0 {
        return Err(CookError::NoSteps(
            value["name"].as_str().unwrap_or("?").to_string(),
        ));
    }
    Ok(steps)
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        args.push("/definitely/missing.json".into()); // demo default so the example shows an error
    }
    match run(&args) {
        Ok(n) => {
            println!("{n} steps");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
