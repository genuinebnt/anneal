#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// The ids are not in non-decreasing order.
    NotSorted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The input ends before the structure it describes does.
    Truncated,
    /// A block declares more than 32 bits per delta.
    BadWidth(u8),
    /// A running sum above `u32::MAX`.
    Overflow,
    /// Bytes left over after the last block.
    TrailingBytes,
}

/// Ids per block (the last block may hold fewer).
pub const BLOCK: usize = 128;

/// Encodes a non-decreasing list of document ids.
///
/// Layout: the id count as a LEB128 varint, then one **block** per up-to-128 ids: a byte `w` (the bit width), then the
/// block's deltas packed `w` bits each, least-significant bit first, the whole block padded to a whole number of bytes.
/// A delta is `id - previous id`, where the previous id of the very first id is 0 and of a later block's first id is
/// the last id of the block before it. `w` is the number of bits of the largest delta in the block (0 if they are all 0).
pub fn encode(ids: &[u32]) -> Result<Vec<u8>, EncodeError> {
    todo!()
}

/// The inverse of `encode`.
pub fn decode(bytes: &[u8]) -> Result<Vec<u32>, DecodeError> {
    todo!()
}
