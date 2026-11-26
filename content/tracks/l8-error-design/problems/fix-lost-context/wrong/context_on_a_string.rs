use anyhow::{anyhow, Context, Result};
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
        .with_context(|| format!("missing key `{key}`"))
}

pub fn number(map: &HashMap<String, String>, key: &str) -> Result<u32> {
    let text = raw(map, key)?;
    text.trim()
        .parse()
        .with_context(|| format!("reading `{key}`"))
}

pub fn load(map: &HashMap<String, String>, service: &str) -> Result<Limits> {
    limits(map, service).with_context(|| format!("loading {service}"))
}

fn limits(map: &HashMap<String, String>, service: &str) -> Result<Limits> {
    let workers = number(map, &format!("{service}.workers"))?;
    if workers == 0 {
        return Err(anyhow!("workers must be at least 1"));
    }
    let queue = number(map, &format!("{service}.queue"))?;
    Ok(Limits { workers, queue })
}

pub fn exit_code(err: &anyhow::Error) -> i32 {
    if err
        .chain()
        .any(|c| c.to_string().starts_with("missing key"))
    {
        2
    } else if err.chain().any(|c| c.is::<std::num::ParseIntError>()) {
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
