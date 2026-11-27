/// Where one word is: a page, and the places of the word on it, rising.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Posting {
    pub page: u32,
    pub positions: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    Truncated,
    Overflow,
    NotAscending,
    NoPositions,
    TrailingBytes,
}

/// The words of a text, in order, lowercased.
pub fn tokenize(text: &str) -> Vec<String> {
    todo!()
}

/// Packs a list whose pages rise, each with at least one position, positions rising.
pub fn encode(list: &[Posting]) -> Vec<u8> {
    todo!()
}

/// Reads bytes that anyone may have written.
pub fn decode(bytes: &[u8]) -> Result<Vec<Posting>, CodecError> {
    todo!()
}
