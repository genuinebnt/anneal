//! The same enum with `thiserror`: the macro writes `Display` and `Error`.

use std::io;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PortError {
    #[error("cannot read {}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("no `port=` line")]
    Missing,
    #[error("invalid port {text:?}")]
    NotANumber { text: String, source: ParseIntError },
    #[error("port must not be 0")]
    Zero,
}

pub fn parse_port(text: &str) -> Result<u16, PortError> {
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix("port="))
        .ok_or(PortError::Missing)?;
    let port: u16 = value.parse().map_err(|source| PortError::NotANumber {
        text: value.to_owned(),
        source,
    })?;
    if port == 0 {
        return Err(PortError::Zero);
    }
    Ok(port)
}

pub fn load_port(path: &Path) -> Result<u16, PortError> {
    let text = std::fs::read_to_string(path).map_err(|source| PortError::Read {
        path: path.to_owned(),
        source,
    })?;
    parse_port(&text)
}
