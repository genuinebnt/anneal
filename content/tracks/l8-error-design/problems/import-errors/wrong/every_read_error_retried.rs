use std::io;
use std::num::ParseIntError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub sku: String,
    pub quantity: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum RowError {
    #[error("missing {0}")]
    MissingField(&'static str),
    #[error("quantity {text:?} is not a number")]
    BadQuantity { text: String, source: ParseIntError },
    #[error("quantity must be at least 1")]
    ZeroQuantity,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("cannot read {path}")]
    Read { path: String, source: io::Error },
    #[error("line {line} is invalid")]
    BadRow { line: usize, source: RowError },
}

pub fn parse_row(line: &str) -> Result<Item, RowError> {
    let (sku, quantity) = line.split_once(',').unwrap_or((line, ""));
    let (sku, text) = (sku.trim(), quantity.trim());
    if sku.is_empty() {
        return Err(RowError::MissingField("sku"));
    }
    if text.is_empty() {
        return Err(RowError::MissingField("quantity"));
    }
    let quantity: u32 = text.parse().map_err(|source| RowError::BadQuantity {
        text: text.to_string(),
        source,
    })?;
    if quantity == 0 {
        return Err(RowError::ZeroQuantity);
    }
    Ok(Item {
        sku: sku.to_string(),
        quantity,
    })
}

pub fn import(
    path: &str,
    read: impl FnOnce(&str) -> io::Result<String>,
) -> Result<Vec<Item>, ImportError> {
    let text = read(path).map_err(|source| ImportError::Read {
        path: path.to_string(),
        source,
    })?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            parse_row(line).map_err(|source| ImportError::BadRow {
                line: i + 1,
                source,
            })
        })
        .collect()
}

pub fn chain(err: &(dyn std::error::Error + 'static)) -> Vec<String> {
    std::iter::successors(Some(err), |e| e.source())
        .map(|e| e.to_string())
        .collect()
}

pub fn is_retryable(err: &ImportError) -> bool {
    matches!(err, ImportError::Read { .. })
}
