//! `anyhow`: one error type for everything, with context added on the way up.

use anyhow::{Context, Result, ensure};
use std::path::Path;

pub fn parse_port(text: &str) -> Result<u16> {
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix("port="))
        .context("no `port=` line")?;
    let port: u16 = value
        .parse()
        .with_context(|| format!("invalid port {value:?}"))?;
    ensure!(port != 0, "port must not be 0");
    Ok(port)
}

pub fn load_port(path: &Path) -> Result<u16> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse_port(&text)
}
