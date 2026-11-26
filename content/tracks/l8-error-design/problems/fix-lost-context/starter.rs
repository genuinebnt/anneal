use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug)]
pub struct MissingKey(pub String);

impl fmt::Display for MissingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "missing key `{}`", self.0)
    }
}

impl std::error::Error for MissingKey {}

#[derive(Debug, PartialEq)]
pub struct Limits {
    pub workers: u32,
    pub queue: u32,
}

pub fn raw<'a>(map: &'a HashMap<String, String>, key: &str) -> Result<&'a str> {
    map.get(key)
        .map(String::as_str)
        .ok_or_else(|| anyhow!("missing key `{key}`"))
}

pub fn number(map: &HashMap<String, String>, key: &str) -> Result<u32> {
    let text = raw(map, key)?;
    text.trim()
        .parse()
        .map_err(|e| anyhow!("reading `{key}`: {e}"))
}

pub fn load(map: &HashMap<String, String>, service: &str) -> Result<Limits> {
    let workers = number(map, &format!("{service}.workers"))
        .map_err(|e| anyhow!("loading {service}: {e}"))?;
    if workers == 0 {
        return Err(anyhow!("loading {service}: workers must be at least 1"));
    }
    let queue =
        number(map, &format!("{service}.queue")).map_err(|e| anyhow!("loading {service}: {e}"))?;
    Ok(Limits { workers, queue })
}

pub fn exit_code(err: &anyhow::Error) -> i32 {
    let text = err.to_string();
    if text.contains("missing") {
        2
    } else if text.contains("reading") {
        3
    } else {
        1
    }
}

pub fn report(err: &anyhow::Error) -> String {
    err.chain()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}
