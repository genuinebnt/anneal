use std::fmt;
use std::io;
use std::num::ParseIntError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub sku: String,
    pub quantity: u32,
}

#[derive(Debug)]
pub enum RowError {
    MissingField(&'static str),
    BadQuantity { text: String, source: ParseIntError },
    ZeroQuantity,
}

#[derive(Debug)]
pub enum ImportError {
    Read { path: String, source: io::Error },
    BadRow { line: usize, source: RowError },
}

// These four impls only make the starter compile. Replace them.
impl fmt::Display for RowError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}
impl std::error::Error for RowError {}
impl fmt::Display for ImportError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}
impl std::error::Error for ImportError {}

pub fn parse_row(line: &str) -> Result<Item, RowError> {
    todo!()
}

pub fn import(
    path: &str,
    read: impl FnOnce(&str) -> io::Result<String>,
) -> Result<Vec<Item>, ImportError> {
    todo!()
}

pub fn chain(err: &(dyn std::error::Error + 'static)) -> Vec<String> {
    todo!()
}

pub fn is_retryable(err: &ImportError) -> bool {
    todo!()
}
