//! Errors as values: reading a format that can be wrong in several ways. A **record** is a key and a value; a stream of records is
//! `key_len: u8`, `value_len: u16` (little-endian), the key's bytes (UTF-8), the value's bytes, record after record.

use std::fmt;
use std::io;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub key: String,
    pub value: Vec<u8>,
}

/// What can be wrong with the bytes of a stream of records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// The stream ends in the middle of a record: `needed` more bytes were expected at byte offset `at`, only `have` were left.
    Truncated { at: usize, needed: usize, have: usize },
    /// The key of the record that starts at byte offset `at` is not UTF-8.
    BadKey { at: usize },
    /// A key is longer than 255 bytes (found when encoding).
    KeyTooLong { len: usize },
    /// A value is longer than 65535 bytes (found when encoding).
    ValueTooLong { len: usize },
}

/// What can go wrong when records are loaded from a file: the file, or its contents.
#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Format(FormatError),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("r-02: one sentence per error that names its numbers (a match with write!)")
    }
}

impl std::error::Error for FormatError {}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("r-02: say which of the two it is and show the cause")
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        todo!("r-02: the wrapped error, so that callers can walk the chain of causes")
    }
}

// TODO(r-02): conversions from io::Error and FormatError into LoadError, so that `?` converts them

/// The bytes of the records, one after the other. Errors for a key or a value that does not fit its length field.
pub fn encode_records(records: &[Record]) -> Result<Vec<u8>, FormatError> {
    todo!("r-02: each record is its lengths, then its key and value bytes; a length that does not fit its field is an error")
}

/// Reads every record of `bytes`; an error names the byte where the trouble starts. Never panics, whatever the bytes are.
pub fn parse_records(bytes: &[u8]) -> Result<Vec<Record>, FormatError> {
    todo!("r-02: records are read one after another; one that does not fit gives Truncated (with the offset, the bytes needed and the bytes left), a key that is not UTF-8 gives BadKey")
}

/// Reads the file and parses it: an I/O error and a format error are different errors of one type.
pub fn load_records(path: impl AsRef<Path>) -> Result<Vec<Record>, LoadError> {
    todo!("r-02: read the file and parse it; `?` converts both errors into LoadError")
}
