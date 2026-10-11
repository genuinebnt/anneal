//! CRC-32 (IEEE) and sealed payloads.

#[derive(Debug, PartialEq, Eq)]
pub enum SealError {
    TooShort,
    Mismatch,
}

/// The CRC-32 (IEEE) of `data`: a 256-entry table, then one lookup per byte.
pub fn crc32(data: &[u8]) -> u32 {
    todo!("3b-c4: the table of 256 entries, then one table lookup per byte")
}

/// `payload` followed by its CRC-32 in 4 little-endian bytes.
pub fn seal(payload: &[u8]) -> Vec<u8> {
    todo!("3b-c4: the payload, then its CRC in 4 little-endian bytes")
}

/// The payload of a sealed message if its stored CRC matches it. `Err(TooShort)` for fewer than 4 bytes, `Err(Mismatch)` for a payload that was
/// changed.
pub fn unseal(bytes: &[u8]) -> Result<&[u8], SealError> {
    todo!("3b-c4: check the stored CRC against the payload")
}
