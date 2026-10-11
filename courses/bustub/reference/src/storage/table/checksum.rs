//! CRC-32 (IEEE) and sealed payloads.

#[derive(Debug, PartialEq, Eq)]
pub enum SealError {
    TooShort,
    Mismatch,
}

/// The CRC-32 (IEEE) of `data`: a 256-entry table, then one lookup per byte.
pub fn crc32(data: &[u8]) -> u32 {
    // @begin 3b-c4
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
    //~ todo!("3b-c4: the table of 256 entries, then one table lookup per byte")
    // @end
}

/// `payload` followed by its CRC-32 in 4 little-endian bytes.
pub fn seal(payload: &[u8]) -> Vec<u8> {
    // @begin 3b-c4
    let mut out = payload.to_vec();
    out.extend_from_slice(&crc32(payload).to_le_bytes());
    out
    //~ todo!("3b-c4: the payload, then its CRC in 4 little-endian bytes")
    // @end
}

/// The payload of a sealed message if its stored CRC matches it. `Err(TooShort)` for fewer than 4 bytes, `Err(Mismatch)` for a payload that was
/// changed.
pub fn unseal(bytes: &[u8]) -> Result<&[u8], SealError> {
    // @begin 3b-c4
    if bytes.len() < 4 {
        return Err(SealError::TooShort);
    }
    let (payload, crc) = bytes.split_at(bytes.len() - 4);
    if u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]]) == crc32(payload) {
        Ok(payload)
    } else {
        Err(SealError::Mismatch)
    }
    //~ todo!("3b-c4: check the stored CRC against the payload")
    // @end
}
