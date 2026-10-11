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
        // @begin r-02
        match self {
            FormatError::Truncated { at, needed, have } => write!(f, "truncated record at byte {at}: needed {needed} more bytes, only {have} left"),
            FormatError::BadKey { at } => write!(f, "the key of the record at byte {at} is not valid UTF-8"),
            FormatError::KeyTooLong { len } => write!(f, "a key of {len} bytes is longer than 255"),
            FormatError::ValueTooLong { len } => write!(f, "a value of {len} bytes is longer than 65535"),
        }
        //~ todo!("r-02: one sentence per error that names its numbers (a match with write!)")
        // @end
    }
}

impl std::error::Error for FormatError {}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // @begin r-02
        match self {
            LoadError::Io(e) => write!(f, "could not read the file: {e}"),
            LoadError::Format(e) => write!(f, "the file is not a stream of records: {e}"),
        }
        //~ todo!("r-02: say which of the two it is and show the cause")
        // @end
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // @begin r-02
        match self {
            LoadError::Io(e) => Some(e),
            LoadError::Format(e) => Some(e),
        }
        //~ todo!("r-02: the wrapped error, so that callers can walk the chain of causes")
        // @end
    }
}

// @begin r-02
impl From<io::Error> for LoadError {
    fn from(e: io::Error) -> LoadError {
        LoadError::Io(e)
    }
}

impl From<FormatError> for LoadError {
    fn from(e: FormatError) -> LoadError {
        LoadError::Format(e)
    }
}
//~ // TODO(r-02): conversions from io::Error and FormatError into LoadError, so that `?` converts them
// @end

/// The bytes of the records, one after the other. Errors for a key or a value that does not fit its length field.
pub fn encode_records(records: &[Record]) -> Result<Vec<u8>, FormatError> {
    // @begin r-02
    let mut out = Vec::new();
    for r in records {
        let key = r.key.as_bytes();
        let key_len = u8::try_from(key.len()).map_err(|_| FormatError::KeyTooLong { len: key.len() })?;
        let value_len = u16::try_from(r.value.len()).map_err(|_| FormatError::ValueTooLong { len: r.value.len() })?;
        out.push(key_len);
        out.extend_from_slice(&value_len.to_le_bytes());
        out.extend_from_slice(key);
        out.extend_from_slice(&r.value);
    }
    Ok(out)
    //~ todo!("r-02: each record is its lengths, then its key and value bytes; a length that does not fit its field is an error")
    // @end
}

/// Reads every record of `bytes`; an error names the byte where the trouble starts. Never panics, whatever the bytes are.
pub fn parse_records(bytes: &[u8]) -> Result<Vec<Record>, FormatError> {
    // @begin r-02
    let mut records = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &bytes[at..];
        if rest.len() < 3 {
            return Err(FormatError::Truncated { at, needed: 3, have: rest.len() });
        }
        let key_len = rest[0] as usize;
        let value_len = u16::from_le_bytes([rest[1], rest[2]]) as usize;
        let total = 3 + key_len + value_len;
        if rest.len() < total {
            return Err(FormatError::Truncated { at, needed: total, have: rest.len() });
        }
        let key = std::str::from_utf8(&rest[3..3 + key_len]).map_err(|_| FormatError::BadKey { at })?.to_owned();
        records.push(Record { key, value: rest[3 + key_len..total].to_vec() });
        at += total;
    }
    Ok(records)
    //~ todo!("r-02: records are read one after another; one that does not fit gives Truncated (with the offset, the bytes needed and the bytes left), a key that is not UTF-8 gives BadKey")
    // @end
}

/// Reads the file and parses it: an I/O error and a format error are different errors of one type.
pub fn load_records(path: impl AsRef<Path>) -> Result<Vec<Record>, LoadError> {
    // @begin r-02
    let bytes = std::fs::read(path)?;
    Ok(parse_records(&bytes)?)
    //~ todo!("r-02: read the file and parse it; `?` converts both errors into LoadError")
    // @end
}
