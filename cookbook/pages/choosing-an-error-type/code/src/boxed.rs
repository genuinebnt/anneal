//! `Box<dyn Error + Send + Sync>`: type erasure with only the standard library.
//! There is no built-in way to add context, so this file has to write one.

use std::error::Error;
use std::fmt;
use std::path::Path;

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

/// A message plus the error that caused it.
#[derive(Debug)]
pub struct WithContext {
    message: String,
    source: BoxError,
}

impl WithContext {
    pub fn new(message: String, source: impl Into<BoxError>) -> Self {
        Self {
            message,
            source: source.into(),
        }
    }
}

impl fmt::Display for WithContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for WithContext {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

pub fn parse_port(text: &str) -> Result<u16, BoxError> {
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix("port="))
        .ok_or("no `port=` line")?;
    let port: u16 = value
        .parse()
        .map_err(|e| WithContext::new(format!("invalid port {value:?}"), e))?;
    if port == 0 {
        return Err("port must not be 0".into());
    }
    Ok(port)
}

pub fn load_port(path: &Path) -> Result<u16, BoxError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| WithContext::new(format!("cannot read {}", path.display()), e))?;
    parse_port(&text)
}
