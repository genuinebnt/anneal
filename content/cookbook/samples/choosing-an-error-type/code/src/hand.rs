//! A hand-written enum: only the standard library.

use std::error::Error;
use std::fmt;
use std::io;
use std::num::ParseIntError;
use std::path::{Path, PathBuf};

#[derive(Debug)]
#[non_exhaustive]
pub enum PortError {
    Read { path: PathBuf, source: io::Error },
    Missing,
    NotANumber { text: String, source: ParseIntError },
    Zero,
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Say what this layer knows. The cause is left to `source()`.
        match self {
            Self::Read { path, .. } => write!(f, "cannot read {}", path.display()),
            Self::Missing => f.write_str("no `port=` line"),
            Self::NotANumber { text, .. } => write!(f, "invalid port {text:?}"),
            Self::Zero => f.write_str("port must not be 0"),
        }
    }
}

impl Error for PortError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::NotANumber { source, .. } => Some(source),
            Self::Missing | Self::Zero => None,
        }
    }
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
