/// Longest size line, without its CRLF.
pub const MAX_LINE: usize = 64;
/// Longest trailer section, with the empty line that ends it.
pub const MAX_TRAILER: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkError {
    Malformed,
    TooLarge,
    TrailerTooLong,
}

pub fn decode(buf: &[u8], max_body: usize) -> Result<Option<(Vec<u8>, usize)>, ChunkError> {
    todo!()
}
