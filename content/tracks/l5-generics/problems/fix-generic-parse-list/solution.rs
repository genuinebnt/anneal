use std::error::Error;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub enum ParseListError<E> {
    /// Nothing but whitespace.
    Empty,
    /// The item at `index` (0-based) didn't parse.
    Bad { index: usize, source: E },
}

impl<E: fmt::Display> fmt::Display for ParseListError<E> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseListError::Empty => write!(f, "empty list"),
            ParseListError::Bad { index, source } => write!(f, "item {index}: {source}"),
        }
    }
}

// `source` hands out `&(dyn Error + 'static)`, so E must be 'static too.
impl<E: Error + 'static> Error for ParseListError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ParseListError::Empty => None,
            ParseListError::Bad { source, .. } => Some(source),
        }
    }
}

/// Parses comma-separated items, trimming each one.
pub fn parse_list<T: FromStr>(s: &str) -> Result<Vec<T>, ParseListError<T::Err>> {
    if s.trim().is_empty() {
        return Err(ParseListError::Empty);
    }
    s.split(',')
        .enumerate()
        .map(|(index, item)| {
            item.trim().parse().map_err(|source| ParseListError::Bad {
                index: index,
                source,
            })
        })
        .collect()
}
