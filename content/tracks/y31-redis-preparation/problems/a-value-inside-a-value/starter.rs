#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<Value>),
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// The most bytes a byte string may hold.
    pub max_bytes: usize,
    /// The most items one list may hold.
    pub max_items: usize,
    /// The most lists that may be open at once.
    pub max_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Malformed,
    TooLong,
    TooMany,
    TooDeep,
}

/// The value at the front of `buf` and the bytes it takes, or `Ok(None)` while more bytes could complete it.
pub fn decode(buf: &[u8], limits: &Limits) -> Result<Option<(Value, usize)>, Error> {
    todo!()
}
