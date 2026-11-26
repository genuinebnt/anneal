use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

/// A problem on one line of a config: no '=' (no source), or a value that isn't a number (the parse error
/// is the source).
#[derive(Debug)]
pub struct ConfigError {
    pub line: usize,
    pub source: Option<ParseIntError>,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.source {
            None => write!(f, "line {}: expected key=value", self.line),
            Some(_) => write!(f, "line {}: bad number", self.line),
        }
    }
}

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|e| e as &(dyn Error + 'static))
    }
}

/// Sums the values of `key=value` lines (1-based line numbers; blank lines skipped). Any problem is a
/// `ConfigError`.
pub fn sum_config(text: &str) -> Result<i64, Box<dyn Error + Send + Sync>> {
    let mut total = 0;
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let (_, value) = line.split_once('=').ok_or(ConfigError {
            line: i + 1,
            source: None,
        })?;
        total += value.trim().parse::<i64>().map_err(|e| ConfigError {
            line: i + 1,
            source: Some(e),
        })?;
    }
    Ok(total)
}

/// The line number, when `err` is a `ConfigError`.
pub fn bad_line(err: &(dyn Error + 'static)) -> Option<usize> {
    err.downcast_ref::<ConfigError>().map(|e| e.line)
}

/// The last error in `err`'s source chain (`err` itself if it has no source).
pub fn root_cause<'e>(err: &'e (dyn Error + 'static)) -> &'e (dyn Error + 'static) {
    let mut cur = err;
    while let Some(next) = cur.source() {
        cur = next;
    }
    cur
}

/// The messages of `err` and each of its sources, outermost first.
pub fn chain(err: &(dyn Error + 'static)) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = Some(err);
    while let Some(e) = cur {
        out.push(e.to_string());
        cur = e.source();
    }
    out
}
