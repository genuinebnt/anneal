use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug, PartialEq)]
pub enum ParseListError {
    /// Nothing but whitespace.
    Empty,
    /// The item at `index` (0-based) didn't parse.
    Bad { index: usize, source: ParseIntError },
}

impl fmt::Display for ParseListError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseListError::Empty => write!(f, "empty list"),
            ParseListError::Bad { index, source } => write!(f, "item {index}: {source}"),
        }
    }
}

impl Error for ParseListError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ParseListError::Empty => None,
            ParseListError::Bad { source, .. } => Some(source),
        }
    }
}

/// Parses comma-separated items, trimming each one.
pub fn parse_list(s: &str) -> Result<Vec<i32>, ParseListError> {
    if s.trim().is_empty() {
        return Err(ParseListError::Empty);
    }
    s.split(',')
        .enumerate()
        .map(|(index, item)| {
            item.trim()
                .parse()
                .map_err(|source| ParseListError::Bad { index, source })
        })
        .collect()
}
